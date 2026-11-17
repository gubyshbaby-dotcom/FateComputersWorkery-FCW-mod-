package com.fatecomputers.fcw.production.machine.euv;

import net.minecraft.core.Direction;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DirectionProperty;

import javax.annotation.Nullable;

/**
 * A building block of the EUV scanner blank (parts kit). Plain cubes; some have a front that turns
 * to the player when placed: four ways (load port, power unit, controller) or six (ports).
 */
public class EuvKitBlock extends Block {
    public enum Facing {
        NONE, HORIZONTAL, ALL
    }

    private static final ThreadLocal<Facing> PENDING = new ThreadLocal<>();

    private final Facing facing;

    protected EuvKitBlock(BlockBehaviour.Properties properties, Facing facing) {
        super(properties);
        this.facing = facing;
        DirectionProperty p = property(facing);
        if (p != null) {
            registerDefaultState(defaultBlockState().setValue(p, Direction.NORTH));
        }
    }

    public static EuvKitBlock create(BlockBehaviour.Properties properties, Facing facing) {
        PENDING.set(facing);
        try {
            return new EuvKitBlock(properties, facing);
        } finally {
            PENDING.remove();
        }
    }

    protected static void pending(Facing facing) {
        PENDING.set(facing);
    }

    protected static void clearPending() {
        PENDING.remove();
    }

    @Nullable
    private static DirectionProperty property(Facing facing) {
        return switch (facing) {
            case HORIZONTAL -> BlockStateProperties.HORIZONTAL_FACING;
            case ALL -> BlockStateProperties.FACING;
            case NONE -> null;
        };
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        DirectionProperty p = property(PENDING.get());
        if (p != null) {
            builder.add(p);
        }
    }

    @Override
    public BlockState getStateForPlacement(BlockPlaceContext context) {
        return switch (facing) {
            case HORIZONTAL -> defaultBlockState().setValue(BlockStateProperties.HORIZONTAL_FACING, context.getHorizontalDirection().getOpposite());
            case ALL -> defaultBlockState().setValue(BlockStateProperties.FACING, context.getNearestLookingDirection().getOpposite());
            case NONE -> defaultBlockState();
        };
    }

    @Override
    protected BlockState rotate(BlockState state, Rotation rotation) {
        DirectionProperty p = property(facing);
        return p == null ? state : state.setValue(p, rotation.rotate(state.getValue(p)));
    }

    @Override
    protected BlockState mirror(BlockState state, Mirror mirror) {
        DirectionProperty p = property(facing);
        return p == null ? state : state.rotate(mirror.getRotation(state.getValue(p)));
    }
}
