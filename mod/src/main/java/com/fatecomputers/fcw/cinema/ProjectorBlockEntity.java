package com.fatecomputers.fcw.cinema;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.SidedHooks;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponents;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;
import net.neoforged.neoforge.energy.EnergyStorage;
import net.neoforged.neoforge.energy.IEnergyStorage;

import javax.annotation.Nullable;
import java.util.Locale;
import java.util.function.Function;

/**
 * Состояние проектора: лампа (разогрев, остывание, наработка), сигнал от компьютера и питание.
 *
 * <p>Сервер хранит фазу и момент её начала в игровом времени; клиент сам считает по ним яркость
 * ({@link #brightness}), поэтому во время разогрева ничего не пересылается. Саму картинку на
 * стене рисует клиент: геометрию считает {@link Projection}, текстуру экрана даёт та же подписка,
 * что у мониторов.
 */
public class ProjectorBlockEntity extends PeripheralBlockEntity {
    /** Лампа выходит на полную яркость. */
    public static final int WARMUP_TICKS = 20 * 20;
    /** Вентилятор остужает лампу; пока он работает, включить проектор нельзя. */
    public static final int COOLDOWN_TICKS = 30 * 20;
    /** К концу ресурса лампа светит вполовину. */
    public static final float WORN_BRIGHTNESS = 0.5f;
    public static final int TICKS_PER_HOUR = 72_000;
    /** Ключ наработки лампы в данных блока и в custom_data предмета. */
    public static final String LAMP_TICKS = "LampTicks";

    public static final int ENERGY_CAPACITY = 2_400;
    public static final int ENERGY_MAX_RECEIVE = 240;
    /** Столько тиков проектор живёт на пустом буфере, прежде чем погаснуть. */
    private static final int POWER_GRACE_TICKS = 20;
    private static final int SYNC_INTERVAL = 1_200;

    public enum Phase { OFF, LIT, COOLING }

    private Phase phase = Phase.OFF;
    /** Игровое время начала фазы. */
    private long phaseStart;
    private int lampTicks;
    /** Компьютер включён: есть что показывать. Иначе проектор светит синим «нет сигнала». */
    private boolean signal;
    // настройки сервера, которые нужны клиенту для картинки
    private int lifeTicks = 40 * TICKS_PER_HOUR;
    private int range = 24;

    private final Energy energy = new Energy();
    private int powerLossTicks;
    @Nullable
    private static Function<ProjectorBlockEntity, ProjectorPowerSource> networkPowerFactory;
    @Nullable
    private ProjectorPowerSource networkPower;
    private boolean networkPowerAsked;

    // клиент: кэш геометрии, чтобы не пускать луч каждый кадр
    @Nullable
    private Projection projection;
    /** Игровое время последнего расчёта; -1 — пересчитать при первом же запросе. */
    private long projectionTime = -1;
    private float projectionAspect;

    /** Буфер FE: принимает с любой стороны, наружу не отдаёт. */
    private final class Energy extends EnergyStorage {
        Energy() {
            super(ENERGY_CAPACITY, ENERGY_MAX_RECEIVE, 0);
        }

        @Override
        public int receiveEnergy(int toReceive, boolean simulate) {
            if (!powerRequired()) {
                return 0; // питание не нужно: кабели других модов ничего не тратят
            }
            int got = super.receiveEnergy(toReceive, simulate);
            if (got > 0 && !simulate) {
                setChanged();
            }
            return got;
        }

        void set(int value) {
            energy = Math.max(0, Math.min(capacity, value));
        }
    }

    public ProjectorBlockEntity(BlockPos pos, BlockState state) {
        super(Cinema.PROJECTOR_BE.get(), pos, state);
    }

    // ------------------------------------------------------------------ состояние

    public Phase phase() {
        return phase;
    }

    public boolean isLit() {
        return phase == Phase.LIT;
    }

    public boolean hasSignal() {
        return signal && isLinked();
    }

    public int range() {
        return range;
    }

    public int lampTicks() {
        return lampTicks;
    }

    /** Ресурс лампы в тиках; 0 — лампа вечная. */
    public int lifeTicks() {
        return lifeTicks;
    }

    public boolean lampBurnt() {
        return lifeTicks > 0 && lampTicks >= lifeTicks;
    }

    /**
     * Яркость лампы 0..1 в момент {@code time} (игровое время с долей тика): разогрев за
     * {@link #WARMUP_TICKS}, спад с износом до {@link #WORN_BRIGHTNESS}, под конец ресурса мерцание.
     */
    public float brightness(double time) {
        if (phase != Phase.LIT) {
            return 0f;
        }
        float warm = Mth.clamp((float) ((time - phaseStart) / WARMUP_TICKS), 0f, 1f);
        float b = 0.12f + 0.88f * warm * warm * (3f - 2f * warm);
        if (lifeTicks > 0) {
            float wear = Mth.clamp((float) lampTicks / lifeTicks, 0f, 1f);
            b *= 1f - (1f - WORN_BRIGHTNESS) * wear;
            if (wear > 0.95f) {
                // последние 5% ресурса: редкие провалы яркости
                double t = time * 0.9;
                b *= 0.82f + 0.18f * (float) Math.abs(Math.sin(t) * Math.sin(t * 0.37 + 1.3));
            }
        }
        return b;
    }

    /** Компьютер включился или выключился. У проектора {@code powered} значит «лампа горит», а не это. */
    @Override
    public void setPoweredState(boolean on) {
        if (level == null || level.isClientSide || signal == on) {
            return;
        }
        signal = on;
        changedAndSync();
    }

    private void setPhase(Phase p) {
        if (level == null) {
            return;
        }
        phase = p;
        phaseStart = level.getGameTime();
        BlockState state = getBlockState();
        boolean lit = p == Phase.LIT;
        if (state.hasProperty(DeviceBlock.POWERED) && state.getValue(DeviceBlock.POWERED) != lit) {
            level.setBlock(worldPosition, state.setValue(DeviceBlock.POWERED, lit), Block.UPDATE_ALL);
        }
        changedAndSync();
    }

    private static String hours(int ticks) {
        return String.format(Locale.ROOT, "%.1f", ticks / (double) TICKS_PER_HOUR);
    }

    private int cooldownLeft() {
        return level == null ? 0 : (int) Math.max(0, phaseStart + COOLDOWN_TICKS - level.getGameTime());
    }

    // ------------------------------------------------------------------ действия игрока

    /** ПКМ пустой рукой: включить или выключить. */
    public void toggle(ServerPlayer player) {
        if (level == null) {
            return;
        }
        readConfig();
        switch (phase) {
            case LIT -> {
                setPhase(Phase.COOLING);
                click(0.5f);
                player.displayClientMessage(Component.translatable("message.fcw.projector.off", COOLDOWN_TICKS / 20), true);
            }
            case COOLING -> player.displayClientMessage(
                    Component.translatable("message.fcw.projector.cooling", (cooldownLeft() + 19) / 20), true);
            case OFF -> {
                if (lampBurnt()) {
                    player.displayClientMessage(Component.translatable("message.fcw.projector.lamp_dead"), true);
                } else if (powerRequired() && energy.getEnergyStored() < powerDraw() * POWER_GRACE_TICKS) {
                    player.displayClientMessage(Component.translatable("message.fcw.projector.no_power", powerDraw()), true);
                } else {
                    powerLossTicks = 0;
                    setPhase(Phase.LIT);
                    click(0.6f);
                    player.displayClientMessage(lifeTicks > 0
                            ? Component.translatable("message.fcw.projector.on", hours(lampTicks), hours(lifeTicks))
                            : Component.translatable("message.fcw.projector.on_eternal"), true);
                }
            }
        }
    }

    /** ПКМ лампой: заменить лампу. Только у выключенного и остывшего проектора. */
    public void replaceLamp(ServerPlayer player, ItemStack lamp) {
        if (phase != Phase.OFF) {
            player.displayClientMessage(Component.translatable("message.fcw.projector.lamp_hot"), true);
            return;
        }
        if (lampTicks == 0) {
            player.displayClientMessage(Component.translatable("message.fcw.projector.lamp_new"), true);
            return;
        }
        lampTicks = 0;
        lamp.consume(1, player);
        click(0.8f);
        changedAndSync();
        player.displayClientMessage(Component.translatable("message.fcw.projector.lamp_replaced"), true);
    }

    private void click(float pitch) {
        if (level != null) {
            level.playSound(null, worldPosition, SoundEvents.LEVER_CLICK, SoundSource.BLOCKS, 0.4f, pitch);
        }
    }

    private void tellNearby(Component message) {
        if (level == null) {
            return;
        }
        for (Player p : level.players()) {
            if (p.distanceToSqr(worldPosition.getX() + 0.5, worldPosition.getY() + 0.5, worldPosition.getZ() + 0.5) <= 24.0 * 24.0) {
                p.displayClientMessage(message, true);
            }
        }
    }

    private Component displayName() {
        return getBlockState().getBlock().getName();
    }

    // ------------------------------------------------------------------ тик сервера

    private void readConfig() {
        int life = FcwConfig.SERVER.lampLifeHours.get() * TICKS_PER_HOUR;
        int r = FcwConfig.SERVER.projectorRange.get();
        if (life != lifeTicks || r != range) {
            lifeTicks = life;
            range = r;
            changedAndSync();
        }
    }

    public void serverTick() {
        if (level == null || level.isClientSide || isRemoved()) {
            return;
        }
        long now = level.getGameTime();
        if (now % 100 == 0) {
            readConfig();
        }
        tickPower();
        switch (phase) {
            case COOLING -> {
                if (now - phaseStart >= COOLDOWN_TICKS) {
                    setPhase(Phase.OFF);
                }
            }
            case LIT -> {
                lampTicks++;
                if (lampBurnt()) {
                    setPhase(Phase.COOLING);
                    level.playSound(null, worldPosition, SoundEvents.GLASS_BREAK, SoundSource.BLOCKS, 0.5f, 1.6f);
                    tellNearby(Component.translatable("message.fcw.projector.lamp_burnt", displayName()));
                } else if (lampTicks % SYNC_INTERVAL == 0) {
                    changedAndSync(); // клиенту нужен износ: от него зависит яркость
                } else {
                    setChanged();
                }
            }
            default -> {
            }
        }
    }

    // ------------------------------------------------------------------ питание (production.requirePower)

    public static void registerCapabilities(RegisterCapabilitiesEvent event) {
        event.registerBlockEntity(Capabilities.EnergyStorage.BLOCK, Cinema.PROJECTOR_BE.get(), (be, side) -> be.energy);
    }

    /** Пакет совместимости с энергосетью другого мода ставит сюда свою фабрику. */
    public static void setNetworkPowerFactory(Function<ProjectorBlockEntity, ProjectorPowerSource> factory) {
        networkPowerFactory = factory;
    }

    @Nullable
    public ProjectorPowerSource networkPower() {
        return networkPower;
    }

    public static boolean powerRequired() {
        return FcwConfig.SERVER.productionRequirePower.get();
    }

    public static int powerDraw() {
        return FcwConfig.SERVER.projectorPower.get();
    }

    public IEnergyStorage energy() {
        return energy;
    }

    /** Сколько FE проектор готов принять в этот тик. */
    public int wantedEnergy() {
        return powerRequired() ? Math.min(ENERGY_MAX_RECEIVE, ENERGY_CAPACITY - energy.getEnergyStored()) : 0;
    }

    /** Энергия, принесённая {@link ProjectorPowerSource}. */
    public void acceptEnergy(int fe) {
        if (fe > 0) {
            energy.set(energy.getEnergyStored() + fe);
            setChanged();
        }
    }

    private void leavePowerNetwork() {
        if (networkPower != null) {
            networkPower.remove();
            networkPower = null;
        }
        networkPowerAsked = false;
    }

    private void tickPower() {
        if (!powerRequired()) {
            powerLossTicks = 0;
            return;
        }
        if (!networkPowerAsked) {
            networkPowerAsked = true;
            if (networkPowerFactory != null) {
                networkPower = networkPowerFactory.apply(this);
            }
        }
        if (networkPower != null) {
            networkPower.tick(this);
        }
        if (phase != Phase.LIT) {
            return;
        }
        int draw = powerDraw();
        if (energy.getEnergyStored() >= draw) {
            energy.set(energy.getEnergyStored() - draw);
            powerLossTicks = 0;
            return;
        }
        if (++powerLossTicks >= POWER_GRACE_TICKS) {
            powerLossTicks = 0;
            setPhase(Phase.COOLING);
            tellNearby(Component.translatable("message.fcw.projector.power_lost", displayName()));
        }
    }

    // ------------------------------------------------------------------ клиент: геометрия

    /**
     * Куда сейчас падает картинка с пропорциями {@code aspect}; null, если луч ни во что не упёрся.
     * Пересчитывается дважды в секунду и при смене пропорций.
     */
    @Nullable
    public Projection projection(float aspect) {
        if (level == null) {
            return null;
        }
        long now = level.getGameTime();
        if (projectionTime < 0 || now - projectionTime >= 10 || now < projectionTime
                || Math.abs(aspect - projectionAspect) > 0.001f) {
            projectionTime = now;
            projectionAspect = aspect;
            Projection old = projection;
            projection = Projection.compute(level, worldPosition, getBlockState(), aspect, range);
            if (projection != null && (old == null || old.onScreen() != projection.onScreen()
                    || Math.abs(old.halfW() - projection.halfW()) > 0.01 || Math.abs(old.distance() - projection.distance()) > 0.01)) {
                FcwMod.LOGGER.debug("Projector at {}: picture {}x{} on {} at {} blocks", worldPosition.toShortString(),
                        String.format(Locale.ROOT, "%.2f", projection.halfW() * 2), String.format(Locale.ROOT, "%.2f", projection.halfH() * 2),
                        projection.onScreen() ? "a screen" : "a wall", String.format(Locale.ROOT, "%.2f", projection.distance()));
            }
        }
        return projection;
    }

    /** Последняя посчитанная картинка (без пересчёта). */
    @Nullable
    public Projection lastProjection() {
        return projection;
    }

    // ------------------------------------------------------------------ загрузка и выгрузка

    @Override
    public void onLoad() {
        super.onLoad();
        if (level == null) {
            return;
        }
        if (level.isClientSide) {
            SidedHooks.client().projectorLoaded(this);
        } else {
            readConfig();
        }
    }

    @Override
    public void setRemoved() {
        super.setRemoved();
        if (level == null) {
            return;
        }
        if (level.isClientSide) {
            SidedHooks.client().projectorUnloaded(this);
        } else {
            leavePowerNetwork();
        }
    }

    @Override
    public void onChunkUnloaded() {
        super.onChunkUnloaded();
        if (level == null) {
            return;
        }
        if (level.isClientSide) {
            SidedHooks.client().projectorUnloaded(this);
        } else {
            leavePowerNetwork();
        }
    }

    // ------------------------------------------------------------------ сохранение и синхронизация

    private void writeState(CompoundTag tag) {
        tag.putString("Phase", phase.name());
        tag.putLong("PhaseStart", phaseStart);
        tag.putInt(LAMP_TICKS, lampTicks);
        tag.putBoolean("Signal", signal);
    }

    private void readState(CompoundTag tag) {
        try {
            phase = Phase.valueOf(tag.getString("Phase"));
        } catch (IllegalArgumentException e) {
            phase = Phase.OFF;
        }
        phaseStart = tag.getLong("PhaseStart");
        lampTicks = tag.getInt(LAMP_TICKS);
        signal = tag.getBoolean("Signal");
        if (tag.contains("Life")) {
            lifeTicks = tag.getInt("Life");
            range = tag.getInt("Range");
        }
        projectionTime = -1;
    }

    @Override
    protected void saveAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.saveAdditional(tag, registries);
        writeState(tag);
        if (energy.getEnergyStored() > 0) {
            tag.putInt("Energy", energy.getEnergyStored());
        }
    }

    @Override
    protected void loadAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.loadAdditional(tag, registries);
        readState(tag);
        energy.set(tag.getInt("Energy"));
    }

    @Override
    public CompoundTag getUpdateTag(HolderLookup.Provider registries) {
        CompoundTag tag = super.getUpdateTag(registries);
        writeState(tag);
        tag.putInt("Life", lifeTicks);
        tag.putInt("Range", range);
        return tag;
    }

    @Override
    public void handleUpdateTag(CompoundTag tag, HolderLookup.Provider registries) {
        super.handleUpdateTag(tag, registries);
        readState(tag);
    }

    // Наработка лампы уезжает в предмет при добыче (таблица добычи копирует custom_data) и
    // возвращается при установке: иначе лампу можно было бы обновлять, переставляя проектор.

    @Override
    protected void collectImplicitComponents(DataComponentMap.Builder components) {
        super.collectImplicitComponents(components);
        if (lampTicks > 0) {
            CompoundTag tag = new CompoundTag();
            tag.putInt(LAMP_TICKS, lampTicks);
            components.set(DataComponents.CUSTOM_DATA, CustomData.of(tag));
        }
    }

    @Override
    protected void applyImplicitComponents(DataComponentInput input) {
        super.applyImplicitComponents(input);
        CustomData data = input.get(DataComponents.CUSTOM_DATA);
        if (data != null && data.contains(LAMP_TICKS)) {
            lampTicks = data.copyTag().getInt(LAMP_TICKS);
        }
    }
}
