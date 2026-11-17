package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Clip;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Event;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import com.fatecomputers.fcw.production.recipe.MachineRecipes;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientGamePacketListener;
import net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Containers;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.energy.EnergyStorage;
import net.neoforged.neoforge.energy.IEnergyStorage;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.fluids.capability.templates.FluidTank;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemStackHandler;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.function.Function;

/**
 * The controller of a machine, in its terminal cell: inventory, energy, tanks, the breaker, the recipe
 * progress of every section, the journal and name of the machine, and what the cells show (state, hatches).
 *
 * <p>Work: with the breaker on, every section looks for a recipe ({@link MachineRecipes#find}, or the
 * recipe pinned from the console) whose results fit into its output slots, runs for the recipe's time
 * (config production.speedMultiplier), drawing its energy per tick when production.requirePower is on,
 * then takes the inputs and puts out the results. The clips are a picture of that progress
 * ({@link Timeline}); the server follows them only for their events (hatches that open, the state of the
 * cells).
 *
 * <p>There is no inventory screen: players load and unload the machine by hand at its zones
 * ({@link #handInsert}, {@link #handTake}; docs/design/MACHINE_UI.md), automation uses the ports, and the
 * terminal is a text console ({@link com.fatecomputers.fcw.production.machine.shell.MachineShell}).
 */
public class MachineControllerBlockEntity extends BlockEntity {
    public static final int ENERGY_CAPACITY = 400_000;
    public static final int ENERGY_MAX_RECEIVE = 20_000;
    public static final int TANK_CAPACITY = 16_000;

    /** What a section is doing: for the console, the readout on the terminal and the signal lamp. */
    public enum Status {
        OFF, IDLE, WORKING, NO_POWER, OUTPUT_FULL, NO_CATALYST;

        public boolean isFault() {
            return this == NO_POWER || this == OUTPUT_FULL || this == NO_CATALYST;
        }
    }

    /** Recipe progress of one section. On the client only the synced fields are set. */
    public static final class SectionState {
        @Nullable
        ResourceLocation recipeId;
        @Nullable
        RecipeHolder<MachineRecipe> recipe; // server; resolved again after loading
        @Nullable
        ResourceLocation pinned;
        int progress, total;
        boolean advancing;
        Status status = Status.OFF;

        public boolean running() {
            return recipeId != null;
        }

        @Nullable
        public ResourceLocation recipeId() {
            return recipeId;
        }

        /** The recipe being run (server side), for its energy and results. */
        @Nullable
        public MachineRecipe recipe() {
            return recipe == null ? null : recipe.value();
        }

        @Nullable
        public ResourceLocation pinned() {
            return pinned;
        }

        public int progress() {
            return progress;
        }

        public int total() {
            return total;
        }

        public Status status() {
            return status;
        }

        void reset() {
            recipeId = null;
            recipe = null;
            progress = 0;
            total = 0;
            advancing = false;
        }
    }

    private final class Energy extends EnergyStorage {
        Energy() {
            super(ENERGY_CAPACITY, ENERGY_MAX_RECEIVE, 0);
        }

        @Override
        public int receiveEnergy(int toReceive, boolean simulate) {
            int got = super.receiveEnergy(toReceive, simulate);
            if (!simulate && got > 0) {
                receivedInWindow += got;
                setChanged();
            }
            return got;
        }

        void set(int value) {
            energy = Math.max(0, Math.min(capacity, value));
        }

        void use(int amount) {
            energy = Math.max(0, energy - amount);
        }
    }

    @Nullable
    private static Function<MachineControllerBlockEntity, MachinePowerSource> networkPowerFactory;

    private final MachineDef def;
    private final ItemStackHandler items;
    private final int[] inputStart, catalystSlot, outputStart;
    private final Energy energy = new Energy();
    private final Map<String, FluidTank> tanks = new LinkedHashMap<>();
    private final Map<String, IItemHandler> portHandlers = new LinkedHashMap<>();
    private final SectionState[] sections;
    private final Map<String, Boolean> hatchOpen = new LinkedHashMap<>();
    /** Hatches opened by clip events: closed again when the clip's work ends. */
    private final Set<String> clipOpened = new HashSet<>();
    private final MachineLog log = new MachineLog();

    private boolean enabled;
    /** "stop": switch off when the product in work is finished. */
    private boolean stopAfter;
    private MachineState shown = MachineState.OFF;
    @Nullable
    private String oneShot;
    private int oneShotAge;
    private int lastPhase = -1;
    private double lastClipTime = -1;
    private MachineState clipState = MachineState.RUN;
    private boolean inventoryChanged = true;
    private boolean validated;
    @Nullable
    private MachinePowerSource networkPower;
    @Nullable
    private String name;
    private int receivedInWindow;
    private int income;
    private boolean powered;
    private boolean fault;
    private boolean worked;
    private boolean syncWanted;
    private int syncCooldown;

    // client: when the synced progress values were taken
    private long syncTime;

    public MachineControllerBlockEntity(BlockPos pos, BlockState state) {
        super(ProductionMachines.CONTROLLER.get(), pos, state);
        this.def = ((MachineCellBlock) state.getBlock()).def();
        List<Section> specs = def.spec.sections();
        sections = new SectionState[specs.size()];
        inputStart = new int[specs.size()];
        catalystSlot = new int[specs.size()];
        outputStart = new int[specs.size()];
        int n = 0;
        for (int i = 0; i < specs.size(); i++) {
            Section s = specs.get(i);
            sections[i] = new SectionState();
            inputStart[i] = n;
            n += s.inputs();
            catalystSlot[i] = s.catalyst() ? n++ : -1;
            outputStart[i] = n;
            n += s.outputs();
        }
        items = new ItemStackHandler(n) {
            @Override
            protected void onContentsChanged(int slot) {
                inventoryChanged = true;
                syncWanted = true;
                setChanged();
            }
        };
        for (MachineDef.Port p : def.ports) {
            if (p.kind().equals("fluid")) {
                tanks.put(p.id(), new FluidTank(TANK_CAPACITY) {
                    @Override
                    protected void onContentsChanged() {
                        setChanged();
                    }
                });
            }
        }
        for (MachineSpec.PortRole role : def.spec.portRoles()) {
            portHandlers.put(role.port(), new PortItems(role));
        }
        for (String h : def.hatches.keySet()) {
            hatchOpen.put(h, false);
        }
    }

    public MachineDef def() {
        return def;
    }

    public Direction facing() {
        return getBlockState().getValue(MachineCellBlock.FACING);
    }

    public BlockPos origin() {
        return MachineDef.origin(worldPosition, facing(), def.terminal.x(), def.terminal.y(), def.terminal.z());
    }

    public MachineState shownState() {
        return getBlockState().getValue(MachineCellBlock.STATE);
    }

    // ───────────────────────── inventory layout

    public ItemStackHandler items() {
        return items;
    }

    public int sectionCount() {
        return sections.length;
    }

    public SectionState section(int i) {
        return sections[i];
    }

    public MachineLog log() {
        return log;
    }

    /** What a slot is for: 0 = input, 1 = catalyst, 2 = output; with the section it belongs to. */
    public record SlotRole(int section, int kind) {
    }

    public static SlotRole slotRole(MachineDef def, int slot) {
        int n = 0;
        List<Section> specs = def.spec.sections();
        for (int i = 0; i < specs.size(); i++) {
            Section s = specs.get(i);
            if (slot < n + s.inputs()) {
                return new SlotRole(i, 0);
            }
            n += s.inputs();
            if (s.catalyst() && slot == n) {
                return new SlotRole(i, 1);
            }
            n += s.catalyst() ? 1 : 0;
            if (slot < n + s.outputs()) {
                return new SlotRole(i, 2);
            }
            n += s.outputs();
        }
        return new SlotRole(0, 2);
    }

    /**
     * Whether a stack may be put into a slot (by hand or through a port): input slots take what some
     * recipe of their section uses, the catalyst slot what some recipe uses as its catalyst, output
     * slots nothing. The same answer on both sides: recipes are synced to clients.
     */
    public static boolean accepts(Level level, MachineDef def, int slot, ItemStack stack) {
        SlotRole role = slotRole(def, slot);
        Section spec = def.spec.sections().get(role.section());
        return switch (role.kind()) {
            case 0 -> MachineRecipes.isInput(level, spec.recipeType().get(), stack);
            case 1 -> MachineRecipes.isCatalyst(level, spec.recipeType().get(), stack);
            default -> false;
        };
    }

    private int[] range(int start, int count) {
        int[] r = new int[count];
        for (int i = 0; i < count; i++) {
            r[i] = start + i;
        }
        return r;
    }

    public int[] inputSlots(int section) {
        return range(inputStart[section], def.spec.sections().get(section).inputs());
    }

    public int[] outputSlots(int section) {
        return range(outputStart[section], def.spec.sections().get(section).outputs());
    }

    public int catalystSlot(int section) {
        return catalystSlot[section];
    }

    /** Live stacks of the section's input slots (and of the output slots it may also take from). */
    private List<ItemStack> inputs(int i) {
        Section spec = def.spec.sections().get(i);
        List<ItemStack> list = new ArrayList<>();
        for (int k = 0; k < spec.inputs(); k++) {
            list.add(items.getStackInSlot(inputStart[i] + k));
        }
        int from = spec.alsoTakesFromSection();
        if (from >= 0) {
            for (int k = 0; k < def.spec.sections().get(from).outputs(); k++) {
                list.add(items.getStackInSlot(outputStart[from] + k));
            }
        }
        return list;
    }

    private ItemStack catalyst(int i) {
        return catalystSlot[i] < 0 ? ItemStack.EMPTY : items.getStackInSlot(catalystSlot[i]);
    }

    /** Whether all of these stacks fit into the section's output slots at once. */
    private boolean fits(int i, List<ItemStack> results) {
        int n = def.spec.sections().get(i).outputs();
        ItemStack[] sim = new ItemStack[n];
        for (int k = 0; k < n; k++) {
            sim[k] = items.getStackInSlot(outputStart[i] + k).copy();
        }
        for (ItemStack r : results) {
            int left = r.getCount();
            for (int k = 0; k < n && left > 0; k++) {
                if (!sim[k].isEmpty() && ItemStack.isSameItemSameComponents(sim[k], r)) {
                    int room = Math.min(sim[k].getMaxStackSize(), items.getSlotLimit(outputStart[i] + k)) - sim[k].getCount();
                    int put = Math.min(room, left);
                    if (put > 0) {
                        sim[k].grow(put);
                        left -= put;
                    }
                }
            }
            for (int k = 0; k < n && left > 0; k++) {
                if (sim[k].isEmpty()) {
                    int put = Math.min(r.getMaxStackSize(), left);
                    sim[k] = r.copyWithCount(put);
                    left -= put;
                }
            }
            if (left > 0) {
                return false;
            }
        }
        return true;
    }

    private void putResults(int i, List<ItemStack> results) {
        int n = def.spec.sections().get(i).outputs();
        for (ItemStack r : results) {
            ItemStack left = r;
            for (int k = 0; k < n && !left.isEmpty(); k++) {
                left = items.insertItem(outputStart[i] + k, left, false);
            }
            if (!left.isEmpty() && level != null) {
                BlockPos out = worldPosition.relative(facing());
                Containers.dropItemStack(level, out.getX() + 0.5, out.getY() + 0.5, out.getZ() + 0.5, left);
            }
        }
    }

    // ───────────────────────── hand loading (zones)

    /** Why a hand insert was refused; the lang key is interact.fcw.reject.&lt;name in lower case&gt;. */
    public enum Reject {
        NOT_ACCEPTED, FULL, BUSY, OUTPUT_ONLY
    }

    /** Slots a zone puts items into (empty for output zones). */
    private int[] zoneInsertSlots(Zone zone) {
        return switch (zone.role()) {
            case INPUT, BOTH -> inputSlots(zone.section());
            case CATALYST -> new int[] {catalystSlot[zone.section()]};
            case TRAY -> outputSlots(zone.section());
            case OUTPUT -> new int[0];
        };
    }

    /** Slots a zone gives items from. */
    public int[] zoneTakeSlots(Zone zone) {
        return switch (zone.role()) {
            case INPUT -> inputSlots(zone.section());
            case CATALYST -> new int[] {catalystSlot[zone.section()]};
            case OUTPUT, BOTH, TRAY -> outputSlots(zone.section());
        };
    }

    /** What lies in a zone, as the player sees it (both sides: the inventory is synced). */
    public List<ItemStack> zoneContents(Zone zone) {
        List<ItemStack> list = new ArrayList<>();
        for (int slot : zoneTakeSlots(zone)) {
            ItemStack s = items.getStackInSlot(slot);
            if (!s.isEmpty()) {
                list.add(s);
            }
        }
        return list;
    }

    /** Whether a stack belongs into this zone at all (both sides; the hint and the white outline use it). */
    public static boolean zoneAccepts(Level level, MachineDef def, Zone zone, ItemStack stack) {
        if (stack.isEmpty()) {
            return false;
        }
        List<Section> specs = def.spec.sections();
        return switch (zone.role()) {
            case INPUT, BOTH -> MachineRecipes.isInput(level, specs.get(zone.section()).recipeType().get(), stack);
            case CATALYST -> MachineRecipes.isCatalyst(level, specs.get(zone.section()).recipeType().get(), stack);
            case TRAY -> {
                boolean ok = false;
                for (Section s : specs) {
                    ok |= s.alsoTakesFromSection() == zone.section() && MachineRecipes.isInput(level, s.recipeType().get(), stack);
                }
                yield ok;
            }
            case OUTPUT -> false;
        };
    }

    /**
     * Right click with an item: as much of the stack as fits goes in (the stack in the hand shrinks).
     * Returns null on success, else why nothing went in.
     */
    @Nullable
    public Reject handInsert(Zone zone, ItemStack held) {
        if (level == null) {
            return Reject.NOT_ACCEPTED;
        }
        if (zone.role() == Role.OUTPUT) {
            return Reject.OUTPUT_ONLY;
        }
        if (!zoneAccepts(level, def, zone, held)) {
            return Reject.NOT_ACCEPTED;
        }
        int before = held.getCount();
        ItemStack left = held.copy();
        for (int slot : zoneInsertSlots(zone)) {
            left = items.insertItem(slot, left, false);
            if (left.isEmpty()) {
                break;
            }
        }
        if (left.getCount() == before) {
            return Reject.FULL;
        }
        held.setCount(left.getCount());
        if (zone.hatch() != null && !hatchOpen.getOrDefault(zone.hatch(), false)) {
            applyHatch(zone.hatch(), true);
        }
        return null;
    }

    /** Whether an empty hand would get something here (and is allowed to). */
    public boolean handCanTake(Zone zone) {
        return !zoneContents(zone).isEmpty() && !(zone.role() == Role.CATALYST && sections[zone.section()].running());
    }

    /**
     * Right click with an empty hand: one stack (or everything, when sneaking) leaves the zone.
     * A catalyst stays while its section works. The caller hands the stacks to the player.
     */
    public List<ItemStack> handTake(Zone zone, boolean all) {
        List<ItemStack> out = new ArrayList<>();
        if (!handCanTake(zone)) {
            return out;
        }
        for (int slot : zoneTakeSlots(zone)) {
            ItemStack s = items.extractItem(slot, Integer.MAX_VALUE, false);
            if (!s.isEmpty()) {
                out.add(s);
                if (!all) {
                    break;
                }
            }
        }
        if (!out.isEmpty() && zone.hatch() != null && !hatchOpen.getOrDefault(zone.hatch(), false)) {
            applyHatch(zone.hatch(), true);
        }
        return out;
    }

    // ───────────────────────── capabilities

    @Nullable
    public IItemHandler itemHandler(String port) {
        return portHandlers.get(port);
    }

    public IEnergyStorage energy() {
        return energy;
    }

    @Nullable
    public IFluidHandler tank(String port) {
        return tanks.get(port);
    }

    /** Set by a compat package (AE2): makes the network power source of a controller. */
    public static void setNetworkPowerFactory(Function<MachineControllerBlockEntity, MachinePowerSource> factory) {
        networkPowerFactory = factory;
    }

    @Nullable
    public MachinePowerSource networkPower() {
        return networkPower;
    }

    /**
     * FE the machine would take from an energy network right now: only when power is required
     * (production.requirePower), the breaker is on, a recipe is in progress and the buffer is not full.
     */
    public int wantedEnergy() {
        if (!enabled || !anyRunning() || !FcwConfig.SERVER.productionRequirePower.get()) {
            return 0;
        }
        return Math.min(ENERGY_MAX_RECEIVE, ENERGY_CAPACITY - energy.getEnergyStored());
    }

    /** Energy brought by a {@link MachinePowerSource}. */
    public void acceptEnergy(int fe) {
        if (fe > 0) {
            energy.set(energy.getEnergyStored() + fe);
            receivedInWindow += fe;
            setChanged();
        }
    }

    /** Average FE per tick that came in over the last second. */
    public int energyIncome() {
        return income;
    }

    @Override
    public void setRemoved() {
        super.setRemoved();
        leaveNetwork();
    }

    @Override
    public void onChunkUnloaded() {
        super.onChunkUnloaded();
        leaveNetwork();
    }

    private void leaveNetwork() {
        if (networkPower != null) {
            networkPower.remove();
            networkPower = null;
            validated = false; // joins again if the block entity comes back (chunk reload)
        }
    }

    /** The slots an item port reaches: inserts go to the input (or catalyst) slots, extracts take from the outputs. */
    private final class PortItems implements IItemHandler {
        private final int[] slots;
        private final boolean[] in, out;

        PortItems(MachineSpec.PortRole role) {
            Section spec = def.spec.sections().get(role.section());
            int s = role.section();
            List<Integer> list = new ArrayList<>();
            List<Boolean> ins = new ArrayList<>();
            if (role.role() == Role.CATALYST) {
                list.add(catalystSlot[s]);
                ins.add(true);
            } else {
                if (role.role() != Role.OUTPUT) {
                    for (int k = 0; k < spec.inputs(); k++) {
                        list.add(inputStart[s] + k);
                        ins.add(true);
                    }
                }
                if (role.role() != Role.INPUT) {
                    for (int k = 0; k < spec.outputs(); k++) {
                        list.add(outputStart[s] + k);
                        ins.add(false);
                    }
                }
            }
            slots = list.stream().mapToInt(Integer::intValue).toArray();
            in = new boolean[slots.length];
            out = new boolean[slots.length];
            for (int k = 0; k < slots.length; k++) {
                in[k] = ins.get(k);
                out[k] = !ins.get(k) || role.role() == Role.CATALYST;
            }
        }

        @Override
        public int getSlots() {
            return slots.length;
        }

        @Override
        public ItemStack getStackInSlot(int slot) {
            return items.getStackInSlot(slots[slot]);
        }

        @Override
        public ItemStack insertItem(int slot, ItemStack stack, boolean simulate) {
            return isItemValid(slot, stack) ? items.insertItem(slots[slot], stack, simulate) : stack;
        }

        @Override
        public ItemStack extractItem(int slot, int amount, boolean simulate) {
            return out[slot] ? items.extractItem(slots[slot], amount, simulate) : ItemStack.EMPTY;
        }

        @Override
        public int getSlotLimit(int slot) {
            return items.getSlotLimit(slots[slot]);
        }

        @Override
        public boolean isItemValid(int slot, ItemStack stack) {
            return in[slot] && level != null && accepts(level, def, slots[slot], stack);
        }
    }

    // ───────────────────────── name

    /** The machine's name ("euv-1"): given on first use, unique in the world, kept with the controller. */
    public String machineName() {
        if (name == null && level instanceof ServerLevel sl) {
            name = MachineNames.get(sl.getServer()).assign(sl, worldPosition, def.spec.namePrefix());
            sync();
        }
        return name == null ? def.spec.namePrefix() : name;
    }

    /** Renames the machine; false when another machine has that name or it is not a valid name. */
    public boolean rename(String newName, String actor) {
        if (!(level instanceof ServerLevel sl) || !MachineNames.VALID.matcher(newName).matches()) {
            return false;
        }
        String old = machineName();
        if (!MachineNames.get(sl.getServer()).rename(sl, worldPosition, old, newName)) {
            return false;
        }
        name = newName;
        log.add(MachineLog.RENAME, newName);
        sync();
        return true;
    }

    // ───────────────────────── control

    public boolean isEnabled() {
        return enabled;
    }

    public boolean isStopping() {
        return stopAfter;
    }

    public void setEnabled(boolean on) {
        setEnabled(on, "");
    }

    /**
     * The breaker: off stops everything at once and loses the progress of the current recipe (nothing has
     * been consumed yet). {@code actor} goes to the journal.
     */
    public void setEnabled(boolean on, String actor) {
        if (level == null || level.isClientSide || (enabled == on && !stopAfter)) {
            return;
        }
        stopAfter = false;
        if (enabled != on) {
            enabled = on;
            log.add(on ? MachineLog.ON : MachineLog.OFF, actor);
            applyHatch(MachineDef.BREAKER, on);
            if (on) {
                if (def.spec.startupClip() != null) {
                    startOneShot(def.spec.startupClip());
                }
            } else {
                for (SectionState s : sections) {
                    s.reset();
                }
                oneShot = null;
                closeClipHatches();
            }
        }
        updateStatus();
        refreshState();
        sync();
    }

    /** "stop": finish the product in work, then switch off (at once when nothing is in work). */
    public void stopAfterCurrent(String actor) {
        if (!enabled) {
            return;
        }
        if (!anyRunning()) {
            setEnabled(false, actor);
            return;
        }
        stopAfter = true;
        log.add(MachineLog.STOP_AFTER, actor);
        sync();
    }

    /** Pins a recipe for a section (the machine then runs only it), or null for automatic choice. */
    public void pinRecipe(int section, @Nullable ResourceLocation recipe) {
        sections[section].pinned = recipe;
        inventoryChanged = true;
        log.add(MachineLog.PIN, recipe == null ? "auto" : MachineRecipeNames.shortName(recipe));
        sync();
    }

    /** The first fault among the sections (for the lamp, the readout and "status"), or null. */
    @Nullable
    public Status faultStatus() {
        for (SectionState s : sections) {
            if (s.status.isFault()) {
                return s.status;
            }
        }
        return null;
    }

    /** Right click on a hatch cell: the breaker switches the machine, the rest just opens and closes. */
    public void toggleHatch(String hatch, String actor) {
        if (hatch.equals(MachineDef.BREAKER)) {
            setEnabled(!enabled || stopAfter, actor);
        } else {
            applyHatch(hatch, !hatchOpen.getOrDefault(hatch, false));
            clipOpened.remove(hatch);
        }
    }

    private void applyHatch(String hatch, boolean open) {
        if (!def.hatches.containsKey(hatch) || level == null) {
            return;
        }
        hatchOpen.put(hatch, open);
        MachineStructure.setHatch(level, origin(), facing(), def, hatch, open);
        setChanged();
    }

    private void closeClipHatches() {
        for (String h : clipOpened) {
            if (hatchOpen.getOrDefault(h, false)) {
                applyHatch(h, false);
            }
        }
        clipOpened.clear();
    }

    private void startOneShot(String clip) {
        oneShot = clip;
        oneShotAge = 0;
    }

    // ───────────────────────── tick

    public void serverTick() {
        if (!(level instanceof ServerLevel sl)) {
            return;
        }
        long now = level.getGameTime();
        boolean changed = false;
        if (!validated) {
            validated = true;
            shown = shownState();
            MachineStructure.refresh(level, origin(), facing(), def, shown, hatchOpen);
            if (name != null) {
                String kept = MachineNames.get(sl.getServer()).claim(sl, worldPosition, name, def.spec.namePrefix());
                changed |= !kept.equals(name);
                name = kept;
            }
            if (networkPowerFactory != null) {
                networkPower = networkPowerFactory.apply(this);
            }
            hadCatalyst = catalystSlot[0] >= 0 && !items.getStackInSlot(catalystSlot[0]).isEmpty();
        }
        if (networkPower != null) {
            networkPower.tick(this);
        }
        if (now % 20 == 0) {
            income = receivedInWindow / 20;
            receivedInWindow = 0;
        }
        if (enabled) {
            for (int i = 0; i < sections.length; i++) {
                changed |= tickSection(i, now);
            }
            if (stopAfter && !anyRunning()) {
                setEnabled(false, "");
            }
        }
        boolean hasCatalyst = catalystSlot[0] >= 0 && !items.getStackInSlot(catalystSlot[0]).isEmpty();
        if (hasCatalyst && !hadCatalyst && enabled && def.spec.catalystClip() != null && oneShot == null && !anyRunning()) {
            startOneShot(def.spec.catalystClip()); // a mask put into an idle machine: play its loading
        }
        hadCatalyst = hasCatalyst;
        inventoryChanged = false;
        changed |= tickClips();
        changed |= updateStatus();
        refreshState();
        if ((now + worldPosition.asLong()) % 100 == 0) {
            MachineStructure.refresh(level, origin(), facing(), def, shown, hatchOpen);
        }
        if (syncCooldown > 0) {
            syncCooldown--;
        }
        if (changed) {
            sync();
        } else if (syncWanted && syncCooldown == 0) {
            sync();
        }
    }

    private boolean hadCatalyst;

    private boolean anyRunning() {
        for (SectionState s : sections) {
            if (s.running()) {
                return true;
            }
        }
        return false;
    }

    /** The recipe a section would run now: the pinned one if it can be made, else the first that fits. */
    private Optional<RecipeHolder<MachineRecipe>> pick(int i, List<ItemStack> inputs, ItemStack catalyst) {
        Section spec = def.spec.sections().get(i);
        ResourceLocation pinned = sections[i].pinned;
        if (pinned == null) {
            return MachineRecipes.find(level, spec.recipeType().get(), inputs, catalyst);
        }
        for (RecipeHolder<MachineRecipe> h : MachineRecipes.all(level, spec.recipeType().get())) {
            if (h.id().equals(pinned)) {
                return h.value().matches(inputs, catalyst) ? Optional.of(h) : Optional.empty();
            }
        }
        return Optional.empty();
    }

    private boolean tickSection(int i, long now) {
        SectionState s = sections[i];
        List<ItemStack> inputs = inputs(i);
        ItemStack catalyst = catalyst(i);
        if (s.recipeId != null && s.recipe == null) {
            // after loading the world: the same recipe must still be there
            Optional<RecipeHolder<MachineRecipe>> found = pick(i, inputs, catalyst);
            if (found.isPresent() && found.get().id().equals(s.recipeId)) {
                s.recipe = found.get();
            } else {
                s.reset();
                return true;
            }
        }
        if (s.recipe == null) {
            if (stopAfter || (!inventoryChanged && (now + i) % 20 != 0)) {
                return false;
            }
            Optional<RecipeHolder<MachineRecipe>> found = pick(i, inputs, catalyst);
            if (found.isEmpty() || !fits(i, RecipeOutputs.max(found.get().value()))) {
                return false;
            }
            s.recipe = found.get();
            s.recipeId = found.get().id();
            s.progress = 0;
            double speed = Math.max(0.01, FcwConfig.SERVER.productionSpeedMultiplier.get());
            s.total = Math.max(1, (int) Math.round(found.get().value().ticks() / speed));
            s.advancing = true;
            worked = true;
            log.add(MachineLog.START, MachineRecipeNames.shortName(s.recipeId));
            if (i == driver()) {
                lastPhase = -1;
                lastClipTime = -1;
            }
            return true;
        }
        MachineRecipe recipe = s.recipe.value();
        boolean last = s.progress + 1 >= s.total;
        if (inventoryChanged || last || now % 20 == 0) {
            Optional<RecipeHolder<MachineRecipe>> found = pick(i, inputs, catalyst);
            if (found.isEmpty() || !found.get().id().equals(s.recipeId)) {
                log.add(MachineLog.ABORT, MachineRecipeNames.shortName(s.recipeId));
                s.reset(); // the ingredients were taken away, or a better recipe became possible
                if (!anyRunning()) {
                    closeClipHatches();
                }
                return true;
            }
        }
        boolean can = true;
        int cost = recipe.energyPerTick();
        if (FcwConfig.SERVER.productionRequirePower.get() && cost > 0) {
            // after running dry, wait for a second's worth of energy so that the machine does not stutter
            if (energy.getEnergyStored() < (s.advancing ? cost : Math.min(cost * 20, ENERGY_CAPACITY))) {
                can = false;
            } else {
                energy.use(cost);
                setChanged();
            }
        }
        if (can && last && !fits(i, RecipeOutputs.max(recipe))) {
            can = false;
        }
        boolean changed = can != s.advancing;
        s.advancing = can;
        if (!can) {
            return changed;
        }
        s.progress++;
        if (s.progress >= s.total) {
            List<ItemStack> results = RecipeOutputs.roll(recipe, level.random);
            MachineRecipes.consume(recipe, inputs, catalyst);
            putResults(i, results);
            log.add(MachineLog.DONE, MachineRecipeNames.shortName(s.recipeId));
            s.reset();
            inventoryChanged = true;
            syncWanted = true;
            setChanged();
            if (!anyRunning()) {
                closeClipHatches();
            }
            return true;
        }
        if (s.progress % 20 == 0) {
            setChanged();
        }
        return changed;
    }

    /** The section whose progress drives the clips: the first one that is running. */
    private int driver() {
        for (int i = 0; i < sections.length; i++) {
            if (sections[i].running()) {
                return i;
            }
        }
        return -1;
    }

    /** The clip playing for the work in progress ("expose", "melt"), or null: the phase shown by "status". */
    @Nullable
    public String workPhase() {
        int d = driver();
        if (d < 0) {
            return null;
        }
        Timeline.At at = Timeline.at(def, sections[d].progress, sections[d].total);
        return at == null ? null : at.clip().id;
    }

    /** Follows the clips for their events. Returns true when clients need to hear about it. */
    private boolean tickClips() {
        boolean changed = false;
        int d = driver();
        if (d >= 0) {
            SectionState s = sections[d];
            Timeline.At at = Timeline.at(def, s.progress, s.total);
            if (at != null) {
                if (at.phase() != lastPhase) {
                    lastPhase = at.phase();
                    lastClipTime = -1;
                }
                if (!at.clip().loop) {
                    fireEvents(at.clip(), lastClipTime, at.clipTime());
                    clipState = floor(MachineState.byName(MachineAnimation.stateAt(at.clip(), at.clipTime())));
                } else {
                    clipState = MachineState.RUN;
                }
                lastClipTime = at.clipTime();
            } else {
                clipState = MachineState.RUN;
            }
        } else {
            lastPhase = -1;
            lastClipTime = -1;
        }
        if (oneShot != null) {
            Clip clip = def.anim.clips.get(oneShot);
            double before = oneShotAge == 0 ? -1 : oneShotAge / 20.0;
            changed |= oneShotAge == 0;
            oneShotAge++;
            double t = oneShotAge / 20.0;
            if (clip == null || t >= clip.length) {
                if (clip != null && d < 0) {
                    fireEvents(clip, before, clip.length);
                }
                oneShot = null;
                if (d < 0) {
                    closeClipHatches();
                }
                changed = true;
            } else if (d < 0) {
                fireEvents(clip, before, t);
            }
        }
        return changed;
    }

    private void fireEvents(Clip clip, double after, double upTo) {
        for (Event e : clip.events) {
            if (e.t > after && e.t <= upTo && e.kind.equals("hatch") && !e.arg.equals(MachineDef.BREAKER)) {
                if (hatchOpen.getOrDefault(e.arg, false) != e.open) {
                    applyHatch(e.arg, e.open);
                    if (e.open) {
                        clipOpened.add(e.arg);
                    } else {
                        clipOpened.remove(e.arg);
                    }
                }
            }
        }
    }

    private static MachineState floor(MachineState s) {
        return s == MachineState.OFF ? MachineState.IDLE : s;
    }

    private boolean updateStatus() {
        boolean changed = false;
        boolean required = FcwConfig.SERVER.productionRequirePower.get();
        boolean hasPower = !required || energy.getEnergyStored() > 0 || income > 0;
        boolean anyFault = false, anyWork = false;
        for (int i = 0; i < sections.length; i++) {
            SectionState s = sections[i];
            Section spec = def.spec.sections().get(i);
            Status st;
            if (!enabled) {
                st = Status.OFF;
            } else if (s.running()) {
                if (s.advancing) {
                    st = Status.WORKING;
                } else {
                    st = s.progress + 1 >= s.total && s.recipe != null && !fits(i, RecipeOutputs.max(s.recipe.value()))
                            ? Status.OUTPUT_FULL : Status.NO_POWER;
                }
            } else if (!hasPower) {
                st = Status.NO_POWER;
            } else if (spec.catalyst() && items.getStackInSlot(catalystSlot[i]).isEmpty() && hasInputs(i)) {
                st = Status.NO_CATALYST;
            } else {
                st = Status.IDLE;
            }
            if (st != s.status) {
                changed = true;
                if (st.isFault()) {
                    log.add(MachineLog.FAULT, st.name().toLowerCase(java.util.Locale.ROOT));
                }
                s.status = st;
            }
            anyFault |= st.isFault();
            anyWork |= st == Status.WORKING;
        }
        if (anyFault && !fault && level != null) {
            // one short signal when a fault appears, then silence
            level.playSound(null, worldPosition, SoundEvents.NOTE_BLOCK_BIT.value(), SoundSource.BLOCKS, 0.7f, 0.6f);
        }
        fault = anyFault;
        if (worked && enabled && !anyWork && !anyFault && !anyRunning()) {
            worked = false;
            log.add(MachineLog.WAIT, "no_input");
        }
        boolean lit = enabled && hasPower;
        changed |= lit != powered;
        powered = lit;
        return changed;
    }

    private boolean hasInputs(int i) {
        for (int slot : inputSlots(i)) {
            if (!items.getStackInSlot(slot).isEmpty()) {
                return true;
            }
        }
        return false;
    }

    private void refreshState() {
        MachineState want;
        if (!enabled) {
            want = MachineState.OFF;
        } else {
            boolean working = false;
            for (SectionState s : sections) {
                working |= s.running() && s.advancing;
            }
            if (working) {
                want = clipState;
            } else if (oneShot != null && def.anim.clips.containsKey(oneShot)) {
                // a starting machine stays dark until its clip says otherwise; "run" belongs to real work only
                MachineState s = MachineState.byName(MachineAnimation.stateAt(def.anim.clips.get(oneShot), oneShotAge / 20.0));
                want = s == MachineState.RUN ? MachineState.IDLE : s;
            } else {
                want = MachineState.IDLE;
            }
        }
        if (want != shown && level != null) {
            shown = want;
            MachineStructure.setState(level, origin(), facing(), def, want);
        }
    }

    // ───────────────────────── client

    public void clientTick() {
        MachineClientHooks.tick(this);
    }

    /** Progress in ticks of a section at this moment, between the synced values. */
    public double clientProgress(int section, float partialTick) {
        SectionState s = sections[section];
        if (!s.running() || level == null) {
            return 0;
        }
        double p = s.progress + (s.advancing ? level.getGameTime() - syncTime + partialTick : 0);
        return Math.max(0, Math.min(p, s.total));
    }

    /** The work clip to show now: a recipe in progress, else a one-shot clip (start-up, mask loading), else null. */
    @Nullable
    public Timeline.At clientWork(float partialTick) {
        int d = driver();
        if (d >= 0) {
            return Timeline.at(def, clientProgress(d, partialTick), sections[d].total);
        }
        if (oneShot != null && level != null) {
            Clip clip = def.anim.clips.get(oneShot);
            double t = (oneShotAge + level.getGameTime() - syncTime + partialTick) / 20.0;
            if (clip != null && t < clip.length) {
                return new Timeline.At(-1, clip, Math.max(0, t), 1);
            }
        }
        return null;
    }

    public boolean clientWorking() {
        for (SectionState s : sections) {
            if (s.running() && s.advancing) {
                return true;
            }
        }
        return false;
    }

    public boolean sectionRunning(int i) {
        return sections[i].running();
    }

    public boolean present(int section, MachineSpec.When when) {
        return switch (when) {
            case INPUT_PRESENT -> hasInputs(section);
            case OUTPUT_PRESENT -> {
                boolean any = false;
                for (int slot : outputSlots(section)) {
                    any |= !items.getStackInSlot(slot).isEmpty();
                }
                yield any;
            }
            case CATALYST_PRESENT -> catalystSlot[section] >= 0 && !items.getStackInSlot(catalystSlot[section]).isEmpty();
        };
    }

    /** The inlet lamp: the breaker is on and there is power (or none is needed). */
    public boolean isPowered() {
        return powered;
    }

    // ───────────────────────── persistence and sync

    private void sync() {
        setChanged();
        syncWanted = false;
        syncCooldown = 10;
        if (level != null && !level.isClientSide) {
            level.sendBlockUpdated(worldPosition, getBlockState(), getBlockState(), 2);
        }
    }

    private void writeShared(CompoundTag tag, HolderLookup.Provider registries) {
        tag.putBoolean("Enabled", enabled);
        tag.putBoolean("StopAfter", stopAfter);
        ListTag list = new ListTag();
        for (SectionState s : sections) {
            CompoundTag st = new CompoundTag();
            if (s.recipeId != null) {
                st.putString("Recipe", s.recipeId.toString());
                st.putInt("Progress", s.progress);
                st.putInt("Total", s.total);
                st.putBoolean("Advancing", s.advancing);
            }
            if (s.pinned != null) {
                st.putString("Pinned", s.pinned.toString());
            }
            st.putInt("Status", s.status.ordinal());
            list.add(st);
        }
        tag.put("Sections", list);
        if (oneShot != null) {
            tag.putString("OneShot", oneShot);
            tag.putInt("OneShotAge", oneShotAge);
        }
        if (name != null) {
            tag.putString("Name", name);
        }
        tag.put("Items", items.serializeNBT(registries));
    }

    private void readShared(CompoundTag tag, HolderLookup.Provider registries) {
        enabled = tag.getBoolean("Enabled");
        stopAfter = tag.getBoolean("StopAfter");
        ListTag list = tag.getList("Sections", Tag.TAG_COMPOUND);
        for (int i = 0; i < sections.length; i++) {
            SectionState s = sections[i];
            s.reset();
            CompoundTag st = i < list.size() ? list.getCompound(i) : new CompoundTag();
            if (st.contains("Recipe")) {
                s.recipeId = ResourceLocation.tryParse(st.getString("Recipe"));
                s.progress = st.getInt("Progress");
                s.total = st.getInt("Total");
                s.advancing = st.getBoolean("Advancing");
            }
            s.pinned = st.contains("Pinned") ? ResourceLocation.tryParse(st.getString("Pinned")) : null;
            int status = st.getInt("Status");
            s.status = status >= 0 && status < Status.values().length ? Status.values()[status] : Status.OFF;
        }
        oneShot = tag.contains("OneShot") ? tag.getString("OneShot") : null;
        oneShotAge = tag.getInt("OneShotAge");
        name = tag.contains("Name") ? tag.getString("Name") : null;
        if (tag.contains("Items")) {
            CompoundTag it = tag.getCompound("Items").copy();
            it.remove("Size"); // the slot layout comes from the code
            for (int i = 0; i < items.getSlots(); i++) {
                items.setStackInSlot(i, ItemStack.EMPTY);
            }
            items.deserializeNBT(registries, it);
        }
    }

    @Override
    protected void saveAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.saveAdditional(tag, registries);
        writeShared(tag, registries);
        tag.putInt("Energy", energy.getEnergyStored());
        CompoundTag tanksTag = new CompoundTag();
        tanks.forEach((id, tank) -> tanksTag.put(id, tank.writeToNBT(registries, new CompoundTag())));
        tag.put("Tanks", tanksTag);
        CompoundTag hatches = new CompoundTag();
        hatchOpen.forEach(hatches::putBoolean);
        tag.put("Hatches", hatches);
        tag.put("Log", log.save());
    }

    @Override
    protected void loadAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.loadAdditional(tag, registries);
        readShared(tag, registries);
        if (tag.contains("ClientTime")) {
            // the client's copy
            syncTime = tag.getLong("ClientTime");
            powered = tag.getBoolean("Powered");
            return;
        }
        energy.set(tag.getInt("Energy"));
        CompoundTag tanksTag = tag.getCompound("Tanks");
        tanks.forEach((id, tank) -> {
            if (tanksTag.contains(id)) {
                tank.readFromNBT(registries, tanksTag.getCompound(id));
            }
        });
        CompoundTag hatches = tag.getCompound("Hatches");
        for (String h : hatchOpen.keySet()) {
            hatchOpen.put(h, hatches.getBoolean(h));
        }
        hatchOpen.put(MachineDef.BREAKER, enabled);
        log.load(tag.getCompound("Log"));
    }

    @Override
    public CompoundTag getUpdateTag(HolderLookup.Provider registries) {
        CompoundTag tag = new CompoundTag();
        writeShared(tag, registries);
        tag.putLong("ClientTime", level == null ? 0 : level.getGameTime());
        tag.putBoolean("Powered", powered);
        return tag;
    }

    @Nullable
    @Override
    public Packet<ClientGamePacketListener> getUpdatePacket() {
        return ClientboundBlockEntityDataPacket.create(this);
    }

    /** The machine is taken apart: everything it holds drops, its name is free again. */
    public void dropContents(Level level, BlockPos at) {
        for (int i = 0; i < items.getSlots(); i++) {
            ItemStack stack = items.getStackInSlot(i);
            if (!stack.isEmpty()) {
                Containers.dropItemStack(level, at.getX() + 0.5, at.getY() + 0.5, at.getZ() + 0.5, stack.copy());
                items.setStackInSlot(i, ItemStack.EMPTY);
            }
        }
        if (name != null && level instanceof ServerLevel sl) {
            MachineNames.get(sl.getServer()).release(sl, worldPosition, name);
        }
    }

    /** Whether a player stands close enough to use the terminal. */
    public boolean stillValid(Player player) {
        return level != null && level.getBlockEntity(worldPosition) == this
                && player.distanceToSqr(worldPosition.getX() + 0.5, worldPosition.getY() + 0.5, worldPosition.getZ() + 0.5) <= 100.0;
    }
}
