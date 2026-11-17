package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Clip;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Event;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import com.fatecomputers.fcw.production.machine.menu.MachineMenu;
import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import com.fatecomputers.fcw.production.recipe.MachineRecipes;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientGamePacketListener;
import net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.Containers;
import net.minecraft.world.MenuProvider;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerData;
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

/**
 * The controller of a machine, in its terminal cell: inventory, energy, tanks, the breaker, the recipe
 * progress of every section and what the cells show (state, hatches).
 *
 * <p>Work: with the breaker on, every section looks for a recipe ({@link MachineRecipes#find}) whose
 * results fit into its output slots, runs for the recipe's time (config production.speedMultiplier),
 * drawing its energy per tick when production.requirePower is on, then takes the inputs and puts out
 * the results. The clips are a picture of that progress ({@link Timeline}); the server follows them
 * only for their events (hatches that open, the state of the cells).
 *
 * <p>Outside control (a computer later on): {@link #setEnabled}, {@link #isEnabled}, {@link #status}.
 */
public class MachineControllerBlockEntity extends BlockEntity implements MenuProvider {
    public static final int ENERGY_CAPACITY = 400_000;
    public static final int ENERGY_MAX_RECEIVE = 20_000;
    public static final int TANK_CAPACITY = 16_000;

    /** What a section is doing, for the GUI and for status queries. */
    public enum Status {
        OFF, IDLE, WORKING, NO_POWER, OUTPUT_FULL
    }

    /** Recipe progress of one section. On the client only the synced fields are set. */
    public static final class SectionState {
        @Nullable
        ResourceLocation recipeId;
        @Nullable
        RecipeHolder<MachineRecipe> recipe; // server; resolved again after loading
        int progress, total;
        boolean advancing;
        Status status = Status.OFF;

        public boolean running() {
            return recipeId != null;
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

    private static final class Energy extends EnergyStorage {
        Energy() {
            super(ENERGY_CAPACITY, ENERGY_MAX_RECEIVE, 0);
        }

        void set(int value) {
            energy = Math.max(0, Math.min(capacity, value));
        }

        void use(int amount) {
            energy = Math.max(0, energy - amount);
        }
    }

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

    private boolean enabled;
    private MachineState shown = MachineState.OFF;
    @Nullable
    private String oneShot;
    private int oneShotAge;
    private int lastPhase = -1;
    private double lastClipTime = -1;
    private MachineState clipState = MachineState.RUN;
    private int presence;
    private boolean inventoryChanged = true;
    private boolean validated;

    // client: when the synced progress values were taken
    private long syncTime;

    private final ContainerData data = new ContainerData() {
        @Override
        public int get(int index) {
            if (index == 0) {
                return energy.getEnergyStored() & 0xFFFF;
            }
            if (index == 1) {
                return energy.getEnergyStored() >>> 16;
            }
            if (index == 2) {
                return enabled ? 1 : 0;
            }
            int s = (index - 3) / 2;
            SectionState st = sections[s];
            return (index - 3) % 2 == 0 ? (st.total <= 0 ? 0 : st.progress * 1000 / st.total) : st.status.ordinal();
        }

        @Override
        public void set(int index, int value) {
        }

        @Override
        public int getCount() {
            return 3 + sections.length * 2;
        }
    };

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

    public int inputStart(int section) {
        return inputStart[section];
    }

    public int catalystSlot(int section) {
        return catalystSlot[section];
    }

    public int outputStart(int section) {
        return outputStart[section];
    }

    public boolean isOutputSlot(int slot) {
        for (int i = 0; i < sections.length; i++) {
            if (slot >= outputStart[i] && slot < outputStart[i] + def.spec.sections().get(i).outputs()) {
                return true;
            }
        }
        return false;
    }

    public ContainerData data() {
        return data;
    }

    public SectionState section(int i) {
        return sections[i];
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

    /** The slots an item port reaches: inserts go to the input (or catalyst) slots, extracts take from the outputs. */
    private final class PortItems implements IItemHandler {
        private final int[] slots;
        private final boolean[] in, out;
        private final boolean catalystOnly;
        private final java.util.function.Supplier<net.minecraft.world.item.crafting.RecipeType<MachineRecipe>> recipeType;

        PortItems(MachineSpec.PortRole role) {
            Section spec = def.spec.sections().get(role.section());
            catalystOnly = role.role() == Role.CATALYST;
            recipeType = spec.recipeType();
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
            if (catalystOnly && level != null) {
                return MachineRecipes.isCatalyst(level, recipeType.get(), stack);
            }
            return in[slot];
        }
    }

    // ───────────────────────── control

    public boolean isEnabled() {
        return enabled;
    }

    /** The breaker: off stops everything and loses the progress of the current recipe. */
    public void setEnabled(boolean on) {
        if (level == null || level.isClientSide || enabled == on) {
            return;
        }
        enabled = on;
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
        updateStatus();
        refreshState();
        sync();
    }

    /** One line per section: what the machine is doing (for a future "status" command from a computer). */
    public List<Status> status() {
        List<Status> list = new ArrayList<>();
        for (SectionState s : sections) {
            list.add(s.status);
        }
        return list;
    }

    /** Right click on a hatch cell: the breaker switches the machine, the rest just opens and closes. */
    public void toggleHatch(String hatch) {
        if (hatch.equals(MachineDef.BREAKER)) {
            setEnabled(!enabled);
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
        if (level == null) {
            return;
        }
        long now = level.getGameTime();
        boolean changed = false;
        if (!validated) {
            validated = true;
            shown = shownState();
            MachineStructure.refresh(level, origin(), facing(), def, shown, hatchOpen);
        }
        if (enabled) {
            for (int i = 0; i < sections.length; i++) {
                changed |= tickSection(i, now);
            }
        }
        inventoryChanged = false;
        changed |= tickClips();
        changed |= updateStatus();
        int p = presence();
        if (p != presence) {
            // a mask put into an idle machine: play its loading
            int catalystBit = 4;
            if ((p & catalystBit) != 0 && (presence & catalystBit) == 0 && enabled && def.spec.catalystClip() != null
                    && oneShot == null && !anyRunning()) {
                startOneShot(def.spec.catalystClip());
            }
            presence = p;
            changed = true;
        }
        refreshState();
        if ((now + worldPosition.asLong()) % 100 == 0) {
            MachineStructure.refresh(level, origin(), facing(), def, shown, hatchOpen);
        }
        if (changed) {
            sync();
        }
    }

    private boolean anyRunning() {
        for (SectionState s : sections) {
            if (s.running()) {
                return true;
            }
        }
        return false;
    }

    private boolean tickSection(int i, long now) {
        SectionState s = sections[i];
        Section spec = def.spec.sections().get(i);
        List<ItemStack> inputs = inputs(i);
        ItemStack catalyst = catalyst(i);
        if (s.recipeId != null && s.recipe == null) {
            // after loading the world: the same recipe must still be there
            Optional<RecipeHolder<MachineRecipe>> found = MachineRecipes.find(level, spec.recipeType().get(), inputs, catalyst);
            if (found.isPresent() && found.get().id().equals(s.recipeId)) {
                s.recipe = found.get();
            } else {
                s.reset();
                return true;
            }
        }
        if (s.recipe == null) {
            if (!inventoryChanged && (now + i) % 20 != 0) {
                return false;
            }
            Optional<RecipeHolder<MachineRecipe>> found = MachineRecipes.find(level, spec.recipeType().get(), inputs, catalyst);
            if (found.isEmpty() || !fits(i, RecipeOutputs.max(found.get().value()))) {
                return false;
            }
            s.recipe = found.get();
            s.recipeId = found.get().id();
            s.progress = 0;
            double speed = Math.max(0.01, FcwConfig.SERVER.productionSpeedMultiplier.get());
            s.total = Math.max(1, (int) Math.round(found.get().value().ticks() / speed));
            s.advancing = true;
            if (i == driver()) {
                lastPhase = -1;
                lastClipTime = -1;
            }
            return true;
        }
        MachineRecipe recipe = s.recipe.value();
        boolean last = s.progress + 1 >= s.total;
        if (inventoryChanged || last || now % 20 == 0) {
            Optional<RecipeHolder<MachineRecipe>> found = MachineRecipes.find(level, spec.recipeType().get(), inputs, catalyst);
            if (found.isEmpty() || !found.get().id().equals(s.recipeId)) {
                s.reset(); // the ingredients were taken away
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
            s.reset();
            inventoryChanged = true;
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
        for (int i = 0; i < sections.length; i++) {
            SectionState s = sections[i];
            Status st;
            if (!enabled) {
                st = Status.OFF;
            } else if (!s.running()) {
                st = Status.IDLE;
            } else if (s.advancing) {
                st = Status.WORKING;
            } else {
                st = s.progress + 1 >= s.total ? Status.OUTPUT_FULL : Status.NO_POWER;
            }
            changed |= st != s.status;
            s.status = st;
        }
        return changed;
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

    /** Bits per section (3 each): inputs present, outputs present, catalyst present. */
    private int presence() {
        int bits = 0;
        for (int i = 0; i < sections.length; i++) {
            Section spec = def.spec.sections().get(i);
            boolean in = false, out = false;
            for (int k = 0; k < spec.inputs(); k++) {
                in |= !items.getStackInSlot(inputStart[i] + k).isEmpty();
            }
            for (int k = 0; k < spec.outputs(); k++) {
                out |= !items.getStackInSlot(outputStart[i] + k).isEmpty();
            }
            boolean cat = catalystSlot[i] >= 0 && !items.getStackInSlot(catalystSlot[i]).isEmpty();
            bits |= ((in ? 1 : 0) | (out ? 2 : 0) | (cat ? 4 : 0)) << (3 * i);
        }
        return bits;
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
        int bits = presence >> (3 * section);
        return switch (when) {
            case INPUT_PRESENT -> (bits & 1) != 0;
            case OUTPUT_PRESENT -> (bits & 2) != 0;
            case CATALYST_PRESENT -> (bits & 4) != 0;
        };
    }

    // ───────────────────────── persistence and sync

    private void sync() {
        setChanged();
        if (level != null && !level.isClientSide) {
            level.sendBlockUpdated(worldPosition, getBlockState(), getBlockState(), 2);
        }
    }

    private void writeShared(CompoundTag tag) {
        tag.putBoolean("Enabled", enabled);
        ListTag list = new ListTag();
        for (SectionState s : sections) {
            CompoundTag st = new CompoundTag();
            if (s.recipeId != null) {
                st.putString("Recipe", s.recipeId.toString());
                st.putInt("Progress", s.progress);
                st.putInt("Total", s.total);
                st.putBoolean("Advancing", s.advancing);
            }
            list.add(st);
        }
        tag.put("Sections", list);
        if (oneShot != null) {
            tag.putString("OneShot", oneShot);
            tag.putInt("OneShotAge", oneShotAge);
        }
    }

    private void readShared(CompoundTag tag) {
        enabled = tag.getBoolean("Enabled");
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
        }
        oneShot = tag.contains("OneShot") ? tag.getString("OneShot") : null;
        oneShotAge = tag.getInt("OneShotAge");
    }

    @Override
    protected void saveAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.saveAdditional(tag, registries);
        writeShared(tag);
        tag.put("Items", items.serializeNBT(registries));
        tag.putInt("Energy", energy.getEnergyStored());
        CompoundTag tanksTag = new CompoundTag();
        tanks.forEach((id, tank) -> tanksTag.put(id, tank.writeToNBT(registries, new CompoundTag())));
        tag.put("Tanks", tanksTag);
        CompoundTag hatches = new CompoundTag();
        hatchOpen.forEach(hatches::putBoolean);
        tag.put("Hatches", hatches);
    }

    @Override
    protected void loadAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.loadAdditional(tag, registries);
        readShared(tag);
        if (tag.contains("Presence")) {
            // the client's copy
            presence = tag.getInt("Presence");
            syncTime = tag.getLong("Time");
            return;
        }
        if (tag.contains("Items")) {
            CompoundTag it = tag.getCompound("Items");
            it.remove("Size"); // the slot layout comes from the code
            items.deserializeNBT(registries, it);
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
    }

    @Override
    public CompoundTag getUpdateTag(HolderLookup.Provider registries) {
        CompoundTag tag = new CompoundTag();
        writeShared(tag);
        tag.putInt("Presence", presence);
        tag.putLong("Time", level == null ? 0 : level.getGameTime());
        return tag;
    }

    @Nullable
    @Override
    public Packet<ClientGamePacketListener> getUpdatePacket() {
        return ClientboundBlockEntityDataPacket.create(this);
    }

    /** Everything the machine holds, when it is taken apart. */
    public void dropContents(Level level, BlockPos at) {
        for (int i = 0; i < items.getSlots(); i++) {
            ItemStack stack = items.getStackInSlot(i);
            if (!stack.isEmpty()) {
                Containers.dropItemStack(level, at.getX() + 0.5, at.getY() + 0.5, at.getZ() + 0.5, stack.copy());
                items.setStackInSlot(i, ItemStack.EMPTY);
            }
        }
    }

    // ───────────────────────── menu

    @Override
    public Component getDisplayName() {
        return Component.translatable("machine.fcw." + def.name);
    }

    @Override
    public AbstractContainerMenu createMenu(int id, Inventory inventory, Player player) {
        return new MachineMenu(id, inventory, this);
    }

    public boolean stillValid(Player player) {
        return level != null && level.getBlockEntity(worldPosition) == this
                && player.distanceToSqr(worldPosition.getX() + 0.5, worldPosition.getY() + 0.5, worldPosition.getZ() + 0.5) <= 64.0;
    }
}
