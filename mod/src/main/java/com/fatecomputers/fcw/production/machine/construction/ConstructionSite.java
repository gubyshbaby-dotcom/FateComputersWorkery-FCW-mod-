package com.fatecomputers.fcw.production.machine.construction;

import com.fatecomputers.fcw.production.ProductionDifficulty;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineLog;
import com.fatecomputers.fcw.production.machine.MachineState;
import com.fatecomputers.fcw.production.machine.MachineStructure;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import com.fatecomputers.fcw.production.machine.construction.ConstructionPlan.Entry;
import com.fatecomputers.fcw.production.machine.construction.ConstructionPlan.Stage;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Containers;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.items.IItemHandler;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.List;

/**
 * A machine under construction or being taken apart (docs/design/MACHINE_BUILD.md). It lives inside the
 * machine's controller, which stands in the terminal cell from the first moment and looks like a pallet
 * until the machine is finished: so the site has the machine's name, journal, console and place in a
 * computer's network without anything of its own.
 *
 * <p>Building: components are delivered for all stages at once (never more than the bill asks for). A
 * stage starts by itself when the one before is done and its components are there; while it runs, its
 * cells replace the scaffolding one by one from the ground up, with sparks. Energy is drawn only when
 * the difficulty and production.requirePower ask for it. After the last stage the controller runs the
 * commissioning and the machine is ready.
 *
 * <p>Teardown: the stages in reverse, each in half its time; when a stage is down, its components come
 * back (a share by difficulty; control cabinets always), then the scaffolding and the pallet go and the
 * blueprint drops.
 */
public final class ConstructionSite {
    private static final int FLAGS = Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE;

    public enum Mode {
        BUILD, TEARDOWN
    }

    private final MachineControllerBlockEntity machine;
    private final MachineDef def;
    private final ConstructionPlan plan;
    private Mode mode = Mode.BUILD;
    /** Stages finished. While a stage is running or coming down it is this index. */
    private int stage;
    /** The stage with index {@link #stage} has taken its components and is raising (or lowering) cells. */
    private boolean started;
    private int stageTicks;
    private int builtCells;
    private final int[][] have;
    private boolean waitingPower;
    private int lastStage = -1, lastEntry = -1, lastCount;

    public ConstructionSite(MachineControllerBlockEntity machine) {
        this.machine = machine;
        this.def = machine.def();
        this.plan = ConstructionPlan.of(def);
        have = new int[plan.stages.size()][];
        for (int i = 0; i < have.length; i++) {
            have[i] = new int[plan.stages.get(i).entries().size()];
        }
    }

    public ConstructionPlan plan() {
        return plan;
    }

    public Mode mode() {
        return mode;
    }

    public int stage() {
        return stage;
    }

    public boolean started() {
        return started;
    }

    public int have(int stageIndex, int entry) {
        return have[stageIndex][entry];
    }

    /** Progress of the running stage, 0..1. */
    public double stageProgress() {
        if (!started || stage >= plan.stages.size()) {
            return 0;
        }
        int total = stageTime(plan.stages.get(stage));
        return Math.min(1.0, (double) stageTicks / total);
    }

    private int stageTime(Stage s) {
        return mode == Mode.TEARDOWN ? Math.max(20, s.ticks() / 2) : s.ticks();
    }

    public boolean waitingPower() {
        return waitingPower;
    }

    /** Waiting for something a player has to bring: materials for the next stage, or power. */
    public boolean waiting() {
        return mode == Mode.BUILD && stage < plan.stages.size() && (!started || waitingPower);
    }

    /** What the stage still lacks: entry index and count. */
    public List<int[]> missing(int stageIndex) {
        List<int[]> list = new ArrayList<>();
        Stage s = plan.stages.get(stageIndex);
        for (int e = 0; e < s.entries().size(); e++) {
            int lack = s.entries().get(e).need() - have[stageIndex][e];
            if (lack > 0) {
                list.add(new int[] {e, lack});
            }
        }
        return list;
    }

    public int missingTotal() {
        int n = 0;
        for (int i = Math.min(stage, plan.stages.size()); i < plan.stages.size(); i++) {
            if (i == stage && started) {
                continue;
            }
            for (int[] m : missing(i)) {
                n += m[1];
            }
        }
        return n;
    }

    /** All components are on the pallet at once: old machine kits were a whole machine in a box. */
    public void fillAll() {
        for (int i = 0; i < have.length; i++) {
            for (int e = 0; e < have[i].length; e++) {
                have[i][e] = plan.stages.get(i).entries().get(e).need();
            }
        }
    }

    /** A finished machine becomes a site that takes itself apart. */
    public void startTeardownOfFinished() {
        mode = Mode.TEARDOWN;
        stage = plan.stages.size() - 1;
        started = true;
        stageTicks = 0;
        builtCells = plan.cells.size();
        fillAll();
    }

    // ───────────────────────── delivery

    /** Whether the site still needs this item for a stage that has not started. */
    public boolean wants(ItemStack stack) {
        return !stack.isEmpty() && mode == Mode.BUILD && room(stack) > 0;
    }

    private int room(ItemStack stack) {
        int room = 0;
        for (int i = stage; i < plan.stages.size(); i++) {
            if (i == stage && started) {
                continue;
            }
            Stage s = plan.stages.get(i);
            for (int e = 0; e < s.entries().size(); e++) {
                if (stack.is(s.entries().get(e).item())) {
                    room += Math.max(0, s.entries().get(e).need() - have[i][e]);
                }
            }
        }
        return room;
    }

    /** Takes as much of the stack as the bill still asks for; returns how many were taken. */
    public int deliver(ItemStack stack, boolean simulate) {
        if (stack.isEmpty() || mode != Mode.BUILD) {
            return 0;
        }
        int left = stack.getCount();
        for (int i = stage; i < plan.stages.size() && left > 0; i++) {
            if (i == stage && started) {
                continue;
            }
            Stage s = plan.stages.get(i);
            for (int e = 0; e < s.entries().size() && left > 0; e++) {
                if (stack.is(s.entries().get(e).item())) {
                    int take = Math.min(left, s.entries().get(e).need() - have[i][e]);
                    if (take > 0) {
                        left -= take;
                        if (!simulate) {
                            have[i][e] += take;
                            lastStage = i;
                            lastEntry = e;
                            lastCount = take;
                        }
                    }
                }
            }
        }
        return stack.getCount() - left;
    }

    /** The stack delivered last, if its stage has not used it yet. */
    public ItemStack takeBack() {
        if (lastStage < 0 || lastStage < stage || (lastStage == stage && started) || mode != Mode.BUILD) {
            return ItemStack.EMPTY;
        }
        int n = Math.min(lastCount, have[lastStage][lastEntry]);
        if (n <= 0) {
            return ItemStack.EMPTY;
        }
        have[lastStage][lastEntry] -= n;
        ItemStack out = new ItemStack(plan.stages.get(lastStage).entries().get(lastEntry).item(), n);
        lastStage = -1;
        return out;
    }

    /** The pallet as a plain item acceptor for hoppers and pipes: takes what the bill needs, gives nothing. */
    public final IItemHandler handler = new IItemHandler() {
        @Override
        public int getSlots() {
            return 1;
        }

        @Override
        public ItemStack getStackInSlot(int slot) {
            return ItemStack.EMPTY;
        }

        @Override
        public ItemStack insertItem(int slot, ItemStack stack, boolean simulate) {
            int taken = deliver(stack, simulate);
            if (taken > 0 && !simulate) {
                machine.siteChanged();
            }
            return taken == 0 ? stack : stack.copyWithCount(stack.getCount() - taken);
        }

        @Override
        public ItemStack extractItem(int slot, int amount, boolean simulate) {
            return ItemStack.EMPTY;
        }

        @Override
        public int getSlotLimit(int slot) {
            return 64;
        }

        @Override
        public boolean isItemValid(int slot, ItemStack stack) {
            return wants(stack);
        }
    };

    // ───────────────────────── mode changes

    /** "cancel" or the wrench on a building site: start taking it apart. */
    public void beginTeardown(String actor) {
        if (mode == Mode.TEARDOWN) {
            return;
        }
        Level level = machine.getLevel();
        // what no stage has used yet comes back in full, at once
        for (int i = stage; i < plan.stages.size(); i++) {
            if (i == stage && started) {
                continue;
            }
            Stage s = plan.stages.get(i);
            for (int e = 0; e < s.entries().size(); e++) {
                drop(level, new ItemStack(s.entries().get(e).item()), have[i][e]);
                have[i][e] = 0;
            }
        }
        mode = Mode.TEARDOWN;
        if (started) {
            // the running stage comes down from where it got to
            stageTicks = (int) ((1.0 - stageProgress()) * stageTime(plan.stages.get(stage)));
        } else {
            stage--;
            started = stage >= 0;
            stageTicks = 0;
        }
        machine.log().add(MachineLog.SITE_TEARDOWN, actor);
        machine.siteChanged();
    }

    /** The wrench on a site that is coming apart: stop, and build again from the stage reached. */
    public void resumeBuild(String actor) {
        if (mode != Mode.TEARDOWN) {
            return;
        }
        mode = Mode.BUILD;
        if (stage < 0) {
            stage = 0;
            started = false;
            stageTicks = 0;
        } else {
            // the stage that was coming down is raised again; its components were never given back
            Stage s = plan.stages.get(stage);
            stageTicks = (int) ((double) (builtCells - s.firstCell()) / Math.max(1, s.cellCount()) * s.ticks());
            started = true;
        }
        machine.log().add(MachineLog.SITE_RESUME, actor);
        machine.siteChanged();
    }

    // ───────────────────────── tick

    /** One server tick. Returns true when the site is finished: built (BUILD) or gone (TEARDOWN). */
    public boolean tick() {
        if (!(machine.getLevel() instanceof ServerLevel level)) {
            return false;
        }
        return mode == Mode.BUILD ? tickBuild(level) : tickTeardown(level);
    }

    private boolean power() {
        int cost = ProductionDifficulty.current().buildEnergyPerTick();
        boolean ok = cost == 0 || machine.useEnergy(cost);
        if (ok == waitingPower) {
            waitingPower = !ok;
            machine.siteChanged();
        }
        return ok;
    }

    private boolean tickBuild(ServerLevel level) {
        if (stage >= plan.stages.size()) {
            return true;
        }
        Stage s = plan.stages.get(stage);
        if (!started) {
            if (!missing(stage).isEmpty()) {
                return false;
            }
            started = true;
            stageTicks = 0;
            machine.log().add(MachineLog.SITE_STAGE, String.valueOf(stage + 1));
            machine.siteChanged();
        }
        if (!power()) {
            return false;
        }
        stageTicks++;
        int total = s.ticks();
        int target = s.firstCell() + (int) Math.min(s.cellCount(), (long) s.cellCount() * stageTicks / total);
        while (builtCells < target) {
            raise(level, plan.cells.get(builtCells++));
        }
        if (stageTicks % 20 == 0) {
            machine.siteChanged();
        }
        if (stageTicks >= total) {
            while (builtCells < s.firstCell() + s.cellCount()) {
                raise(level, plan.cells.get(builtCells++));
            }
            stage++;
            started = false;
            stageTicks = 0;
            machine.log().add(MachineLog.SITE_STAGE_DONE, String.valueOf(stage));
            machine.siteChanged();
        }
        return stage >= plan.stages.size();
    }

    private boolean tickTeardown(ServerLevel level) {
        if (stage < 0) {
            return true;
        }
        Stage s = plan.stages.get(stage);
        if (!power()) {
            return false;
        }
        stageTicks++;
        int total = stageTime(s);
        int target = s.firstCell() + s.cellCount() - (int) Math.min(s.cellCount(), (long) s.cellCount() * stageTicks / total);
        while (builtCells > target) {
            lower(level, plan.cells.get(--builtCells));
        }
        if (stageTicks % 20 == 0) {
            machine.siteChanged();
        }
        if (stageTicks >= total) {
            while (builtCells > s.firstCell()) {
                lower(level, plan.cells.get(--builtCells));
            }
            refund(level, s, stage);
            stage--;
            stageTicks = 0;
            started = stage >= 0;
            machine.siteChanged();
        }
        return stage < 0;
    }

    private BlockPos pos(MachineDef.Cell cell) {
        return MachineDef.worldPos(machine.origin(), machine.facing(), cell);
    }

    /** A cell of the machine takes the place of its scaffolding. */
    private void raise(ServerLevel level, MachineDef.Cell cell) {
        BlockPos at = pos(cell);
        level.setBlock(at, MachineStructure.cellState(def, cell, machine.facing(), MachineState.OFF, false), FLAGS);
        // welding sparks and a short crackle (vanilla particles until the artist's weld_spark arrives)
        level.sendParticles(ParticleTypes.ELECTRIC_SPARK, at.getX() + 0.5, at.getY() + 0.6, at.getZ() + 0.5, 14, 0.35, 0.35, 0.35, 0.08);
        level.sendParticles(ParticleTypes.LAVA, at.getX() + 0.5, at.getY() + 0.6, at.getZ() + 0.5, 2, 0.3, 0.3, 0.3, 0.0);
        level.playSound(null, at, SoundEvents.COPPER_BULB_TURN_ON, SoundSource.BLOCKS, 0.5f, 1.7f);
    }

    private void lower(ServerLevel level, MachineDef.Cell cell) {
        BlockPos at = pos(cell);
        MachineStructure.replaceQuietly(level, machine.origin(), at, ProductionMachines.scaffoldState(def, cell, machine.facing()));
        level.sendParticles(ParticleTypes.ELECTRIC_SPARK, at.getX() + 0.5, at.getY() + 0.6, at.getZ() + 0.5, 8, 0.35, 0.35, 0.35, 0.05);
        level.playSound(null, at, SoundEvents.COPPER_BULB_TURN_OFF, SoundSource.BLOCKS, 0.5f, 1.3f);
    }

    /** A stage is down: its components come back, a share of each kind (the fraction is a chance). */
    private void refund(ServerLevel level, Stage s, int index) {
        double share = ProductionDifficulty.current().refund();
        for (int e = 0; e < s.entries().size(); e++) {
            Entry entry = s.entries().get(e);
            int had = have[index][e];
            have[index][e] = 0;
            boolean always = entry.itemId().getPath().startsWith("control_cabinet");
            double exact = always ? had : had * share;
            int n = (int) Math.floor(exact);
            if (level.random.nextDouble() < exact - n) {
                n++;
            }
            drop(level, new ItemStack(entry.item()), n);
        }
    }

    private void drop(@Nullable Level level, ItemStack kind, int count) {
        if (level == null || kind.isEmpty()) {
            return;
        }
        BlockPos at = machine.getBlockPos().relative(machine.facing());
        while (count > 0) {
            int n = Math.min(count, kind.getMaxStackSize());
            Containers.dropItemStack(level, at.getX() + 0.5, at.getY() + 0.5, at.getZ() + 0.5, kind.copyWithCount(n));
            count -= n;
        }
    }

    /** The teardown is over: the scaffolding goes. The controller removes the pallet and drops the blueprint. */
    public void clearScaffolding(Level level) {
        Direction facing = machine.facing();
        for (MachineDef.Cell cell : plan.cells) {
            BlockPos at = pos(cell);
            BlockState now = level.getBlockState(at);
            if (now.is(ProductionMachines.scaffoldBlock())) {
                level.setBlock(at, Blocks.AIR.defaultBlockState(), FLAGS);
            }
        }
    }

    // ───────────────────────── persistence

    public CompoundTag save() {
        CompoundTag tag = new CompoundTag();
        tag.putString("Mode", mode.name());
        tag.putInt("Stage", stage);
        tag.putBoolean("Started", started);
        tag.putInt("Ticks", stageTicks);
        tag.putInt("Built", builtCells);
        tag.putBoolean("NoPower", waitingPower);
        for (int i = 0; i < have.length; i++) {
            tag.putIntArray("Have" + i, have[i]);
        }
        tag.putInt("LastStage", lastStage);
        tag.putInt("LastEntry", lastEntry);
        tag.putInt("LastCount", lastCount);
        return tag;
    }

    public void load(CompoundTag tag) {
        mode = tag.getString("Mode").equals("TEARDOWN") ? Mode.TEARDOWN : Mode.BUILD;
        stage = tag.getInt("Stage");
        started = tag.getBoolean("Started");
        stageTicks = tag.getInt("Ticks");
        builtCells = Math.max(0, Math.min(plan.cells.size(), tag.getInt("Built")));
        waitingPower = tag.getBoolean("NoPower");
        for (int i = 0; i < have.length; i++) {
            int[] saved = tag.getIntArray("Have" + i);
            for (int e = 0; e < have[i].length; e++) {
                have[i][e] = e < saved.length ? saved[e] : 0;
            }
        }
        lastStage = tag.getInt("LastStage");
        lastEntry = tag.getInt("LastEntry");
        lastCount = tag.getInt("LastCount");
    }
}
