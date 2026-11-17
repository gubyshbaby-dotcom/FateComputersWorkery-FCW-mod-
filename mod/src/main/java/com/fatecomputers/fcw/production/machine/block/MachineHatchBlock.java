package com.fatecomputers.fcw.production.machine.block;

import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineState;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.level.block.state.properties.Property;

import javax.annotation.Nullable;
import java.util.Collection;
import java.util.List;
import java.util.Optional;

/**
 * {@code <machine>_hatch}: a cell with a door, drawer or lever; properties hatch (which cell of which
 * hatch, e.g. foup_port_1_0), open, facing, state. Right click toggles the whole hatch; the lever
 * "main_breaker" is the machine's power switch.
 */
public class MachineHatchBlock extends MachineCellBlock {
    public static final BooleanProperty OPEN = BlockStateProperties.OPEN;

    /** A string property: the hatch ids differ from machine to machine. */
    public static final class HatchProperty extends Property<String> {
        private final List<String> values;

        HatchProperty(Collection<String> values) {
            super("hatch", String.class);
            this.values = List.copyOf(values);
        }

        @Override
        public Collection<String> getPossibleValues() {
            return values;
        }

        @Override
        public String getName(String value) {
            return value;
        }

        @Override
        public Optional<String> getValue(String value) {
            return values.contains(value) ? Optional.of(value) : Optional.empty();
        }

        @Override
        public boolean equals(Object other) {
            return this == other || other instanceof HatchProperty p && super.equals(other) && values.equals(p.values);
        }

        @Override
        public int generateHashCode() {
            return 31 * super.generateHashCode() + values.hashCode();
        }
    }

    private HatchProperty hatch;

    private MachineHatchBlock(BlockBehaviour.Properties properties, MachineDef def) {
        super(properties, def);
        registerDefaultState(stateDefinition.any().setValue(FACING, Direction.NORTH).setValue(STATE, MachineState.OFF).setValue(OPEN, false));
    }

    public static MachineHatchBlock create(BlockBehaviour.Properties properties, MachineDef def) {
        PENDING.set(def);
        try {
            return new MachineHatchBlock(properties, def);
        } finally {
            PENDING.remove();
        }
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        hatch = new HatchProperty(PENDING.get().hatchCells.keySet());
        builder.add(hatch, OPEN, FACING, STATE);
    }

    @Nullable
    @Override
    public MachineDef.Cell cell(BlockState state) {
        return def.hatchCells.get(state.getValue(hatch));
    }

    public BlockState stateFor(MachineDef.Cell cell, Direction facing, MachineState state, boolean open) {
        return defaultBlockState().setValue(hatch, cell.hatchStateId()).setValue(OPEN, open)
                .setValue(FACING, facing).setValue(STATE, state);
    }
}
