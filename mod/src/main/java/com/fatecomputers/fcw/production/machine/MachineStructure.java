package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import com.fatecomputers.fcw.production.machine.block.MachineHatchBlock;
import com.fatecomputers.fcw.production.machine.euv.EuvPattern;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.particles.DustParticleOptions;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Containers;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;

import org.joml.Vector3f;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** Puts machines into the world, keeps their cells in step with the controller and takes them apart. */
public final class MachineStructure {
    /** Cells are placed and changed without neighbour updates: 300 blocks would otherwise ripple through each other. */
    private static final int FLAGS = Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE;

    /** Origins of the machines being taken apart right now (their cells vanish one after another). */
    private static final Set<BlockPos> DISMANTLING = new HashSet<>();

    private MachineStructure() {
    }

    public static BlockState cellState(MachineDef def, MachineDef.Cell cell, Direction facing, MachineState state, boolean open) {
        return cell.hatch() != null
                ? ProductionMachines.hatch(def).stateFor(cell, facing, state, open)
                : ProductionMachines.part(def).stateFor(cell, facing, state);
    }

    /** Positions where the machine would not fit: something that cannot be replaced stands there, or the world ends. */
    public static List<BlockPos> blockers(Level level, BlockPos origin, Direction facing, MachineDef def) {
        List<BlockPos> list = new ArrayList<>();
        for (MachineDef.Cell cell : def.cells) {
            BlockPos pos = MachineDef.worldPos(origin, facing, cell);
            if (level.isOutsideBuildHeight(pos) || !level.getBlockState(pos).canBeReplaced()) {
                list.add(pos);
            }
        }
        return list;
    }

    /** Places the whole machine, breaker off. The caller has checked {@link #blockers}. */
    public static void place(Level level, BlockPos origin, Direction facing, MachineDef def) {
        DISMANTLING.add(origin); // replacing blocks (the EUV kit) must not trigger anything
        try {
            for (MachineDef.Cell cell : def.cells) {
                level.setBlock(MachineDef.worldPos(origin, facing, cell), cellState(def, cell, facing, MachineState.OFF, false), FLAGS);
            }
        } finally {
            DISMANTLING.remove(origin);
        }
    }

    /**
     * Lays out a construction site: scaffolding in every cell of the future machine and the terminal cell
     * as a pallet (state "site") with the controller that runs the build. The caller has checked {@link #blockers}.
     */
    public static MachineControllerBlockEntity placeSite(Level level, BlockPos origin, Direction facing, MachineDef def) {
        DISMANTLING.add(origin);
        try {
            for (MachineDef.Cell cell : def.cells) {
                boolean terminal = cell.x() == def.terminal.x() && cell.y() == def.terminal.y() && cell.z() == def.terminal.z();
                level.setBlock(MachineDef.worldPos(origin, facing, cell), terminal
                        ? cellState(def, cell, facing, MachineState.SITE, false)
                        : ProductionMachines.scaffoldState(def, cell, facing), FLAGS);
            }
        } finally {
            DISMANTLING.remove(origin);
        }
        return level.getBlockEntity(def.controllerPos(origin, facing)) instanceof MachineControllerBlockEntity be ? be : null;
    }

    /** Replaces one block of a machine or site without the "a cell is gone, take the machine apart" reaction. */
    public static void replaceQuietly(Level level, BlockPos origin, BlockPos pos, BlockState state) {
        boolean added = DISMANTLING.add(origin);
        try {
            level.setBlock(pos, state, FLAGS);
        } finally {
            if (added) {
                DISMANTLING.remove(origin);
            }
        }
    }

    public static void setState(Level level, BlockPos origin, Direction facing, MachineDef def, MachineState state) {
        for (MachineDef.Cell cell : def.cells) {
            BlockPos pos = MachineDef.worldPos(origin, facing, cell);
            if (!level.isLoaded(pos)) {
                continue;
            }
            BlockState now = level.getBlockState(pos);
            if (now.getBlock() instanceof MachineCellBlock b && b.def() == def && now.getValue(MachineCellBlock.STATE) != state
                    && now.getValue(MachineCellBlock.STATE) != MachineState.SITE) {
                level.setBlock(pos, now.setValue(MachineCellBlock.STATE, state), FLAGS);
            }
        }
    }

    public static void setHatch(Level level, BlockPos origin, Direction facing, MachineDef def, String hatch, boolean open) {
        MachineDef.Hatch h = def.hatches.get(hatch);
        if (h == null) {
            return;
        }
        for (MachineDef.Cell cell : h.cells()) {
            BlockPos pos = MachineDef.worldPos(origin, facing, cell);
            if (!level.isLoaded(pos)) {
                continue;
            }
            BlockState now = level.getBlockState(pos);
            if (now.getBlock() instanceof MachineHatchBlock b && b.def() == def && now.getValue(MachineHatchBlock.OPEN) != open) {
                level.setBlock(pos, now.setValue(MachineHatchBlock.OPEN, open), FLAGS);
            }
        }
    }

    /** Brings every loaded cell to what the controller says (cells in chunks that were not loaded during a change). */
    public static void refresh(Level level, BlockPos origin, Direction facing, MachineDef def, MachineState state, Map<String, Boolean> hatches) {
        setState(level, origin, facing, def, state);
        hatches.forEach((hatch, open) -> setHatch(level, origin, facing, def, hatch, open));
    }

    /**
     * A cell is gone (broken, blown up, replaced): the machine comes apart. Kit machines leave their kit,
     * the EUV scanner turns back into its building blocks; the contents drop where the cell was.
     */
    public static void onCellRemoved(Level level, BlockPos pos, BlockState state, MachineCellBlock block, boolean creative) {
        MachineDef def = block.def();
        Direction facing = state.getValue(MachineCellBlock.FACING);
        BlockPos origin = block.origin(pos, state);
        if (!DISMANTLING.add(origin)) {
            return;
        }
        try {
            if (level.getBlockEntity(def.controllerPos(origin, facing)) instanceof MachineControllerBlockEntity be && be.def() == def) {
                be.dropContents(level, pos);
            }
            boolean euv = def.name.equals(Machines.EUV);
            for (MachineDef.Cell cell : def.cells) {
                BlockPos at = MachineDef.worldPos(origin, facing, cell);
                if (at.equals(pos)) {
                    continue;
                }
                BlockState now = level.getBlockState(at);
                if (now.getBlock() instanceof MachineCellBlock b && b.def() == def) {
                    level.setBlock(at, euv ? EuvPattern.get().stateAt(cell.x(), cell.y(), cell.z(), facing) : Blocks.AIR.defaultBlockState(), FLAGS);
                }
            }
            if (!creative) {
                ItemStack drop;
                if (euv) {
                    MachineDef.Cell cell = block.cell(state);
                    drop = new ItemStack(EuvPattern.get().stateAt(cell.x(), cell.y(), cell.z(), facing).getBlock());
                } else {
                    drop = ProductionMachines.sourceItem(def);
                }
                Containers.dropItemStack(level, pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5, drop);
            }
        } finally {
            DISMANTLING.remove(origin);
        }
    }

    /** Shows a player which blocks are meant (in the way of a kit, wrong in a blank): red dust, only for them. */
    public static void highlight(ServerPlayer player, List<BlockPos> positions) {
        DustParticleOptions dust = new DustParticleOptions(new Vector3f(1.0f, 0.15f, 0.1f), 1.6f);
        int shown = 0;
        for (BlockPos pos : positions) {
            player.serverLevel().sendParticles(player, dust, true, pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5,
                    24, 0.32, 0.32, 0.32, 0.0);
            if (++shown >= 64) {
                break;
            }
        }
    }
}
