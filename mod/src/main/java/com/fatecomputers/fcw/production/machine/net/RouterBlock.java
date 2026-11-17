package com.fatecomputers.fcw.production.machine.net;

import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.HorizontalDirectionalBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Роутер: узел сети данных, который добавляет в неё все машины с пультом в радиусе
 * {@code production.routerRadius} блоков. Работает, когда касается кабеля данных или компьютера;
 * сам ничего не хранит и не тикает ({@link MachineLinks}).
 */
public class RouterBlock extends HorizontalDirectionalBlock {
    public static final MapCodec<RouterBlock> CODEC = simpleCodec(RouterBlock::new);
    /** Корпус и две антенны у задней стенки (модель смотрит на север). */
    private static final VoxelShape BODY = Block.box(1, 0, 1, 15, 8, 15);
    private static final VoxelShape SHAPE_Z = Shapes.or(BODY, Block.box(2, 8, 7, 4, 16, 9), Block.box(12, 8, 7, 14, 16, 9));
    private static final VoxelShape SHAPE_X = Shapes.or(BODY, Block.box(7, 8, 2, 9, 16, 4), Block.box(7, 8, 12, 9, 16, 14));

    public RouterBlock(Properties properties) {
        super(properties);
        registerDefaultState(stateDefinition.any().setValue(FACING, Direction.NORTH));
    }

    @Override
    protected MapCodec<? extends HorizontalDirectionalBlock> codec() {
        return CODEC;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING);
    }

    @Override
    public BlockState getStateForPlacement(BlockPlaceContext context) {
        return defaultBlockState().setValue(FACING, context.getHorizontalDirection().getOpposite());
    }

    @Override
    protected VoxelShape getShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        return state.getValue(FACING).getAxis() == Direction.Axis.Z ? SHAPE_Z : SHAPE_X;
    }

    @Override
    protected boolean isPathfindable(BlockState state, PathComputationType type) {
        return false;
    }
}
