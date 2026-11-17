package com.fatecomputers.fcw.production.machine.block;

import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineState;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.IntegerProperty;

import javax.annotation.Nullable;

/** {@code <machine>_part}: an ordinary cell; properties x, y, z, facing, state as in the model archives. */
public class MachinePartBlock extends MachineCellBlock {
    private IntegerProperty x, y, z;

    private MachinePartBlock(BlockBehaviour.Properties properties, MachineDef def) {
        super(properties, def);
        registerDefaultState(stateDefinition.any().setValue(FACING, Direction.NORTH).setValue(STATE, MachineState.OFF));
    }

    public static MachinePartBlock create(BlockBehaviour.Properties properties, MachineDef def) {
        PENDING.set(def);
        try {
            return new MachinePartBlock(properties, def);
        } finally {
            PENDING.remove();
        }
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        MachineDef d = PENDING.get();
        x = IntegerProperty.create("x", 0, d.sx - 1);
        y = IntegerProperty.create("y", 0, d.sy - 1);
        z = IntegerProperty.create("z", 0, d.sz - 1);
        builder.add(x, y, z, FACING, STATE);
    }

    @Nullable
    @Override
    public MachineDef.Cell cell(BlockState state) {
        MachineDef.Cell c = def.cell(state.getValue(x), state.getValue(y), state.getValue(z));
        return c == null || c.hatch() != null ? null : c;
    }

    public BlockState stateFor(MachineDef.Cell cell, Direction facing, MachineState state) {
        return defaultBlockState().setValue(x, cell.x()).setValue(y, cell.y()).setValue(z, cell.z())
                .setValue(FACING, facing).setValue(STATE, state);
    }
}
