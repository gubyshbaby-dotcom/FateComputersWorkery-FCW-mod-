package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.block.ComputerBlock;
import com.fatecomputers.fcw.block.DeviceModel;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.HorizontalDirectionalBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Роутер: узел сети данных, который добавляет в неё все машины с пультом в радиусе
 * {@code production.routerRadius} блоков. Сам ничего не хранит и не тикает ({@link MachineLinks}).
 *
 * <p>{@code powered} — роутер в сети данных и работает: он касается кабеля данных или корпуса
 * компьютера (тогда горят индикаторы, модель {@code router_on}); {@code false} — не подключён.
 */
public class RouterBlock extends HorizontalDirectionalBlock {
    public static final MapCodec<RouterBlock> CODEC = simpleCodec(RouterBlock::new);
    public static final BooleanProperty POWERED = BlockStateProperties.POWERED;

    public RouterBlock(Properties properties) {
        super(properties);
        registerDefaultState(stateDefinition.any().setValue(FACING, Direction.NORTH).setValue(POWERED, false));
    }

    @Override
    protected MapCodec<? extends HorizontalDirectionalBlock> codec() {
        return CODEC;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING, POWERED);
    }

    /** Подключает роутер к сети: кабель данных или корпус компьютера вплотную. */
    private static boolean links(BlockState neighbor) {
        Block b = neighbor.getBlock();
        return b instanceof DataCableBlock || b instanceof ComputerBlock;
    }

    private static boolean linked(BlockGetter level, BlockPos pos) {
        for (Direction d : Direction.values()) {
            if (links(level.getBlockState(pos.relative(d)))) {
                return true;
            }
        }
        return false;
    }

    @Override
    public BlockState getStateForPlacement(BlockPlaceContext context) {
        return defaultBlockState()
                .setValue(FACING, context.getHorizontalDirection().getOpposite())
                .setValue(POWERED, linked(context.getLevel(), context.getClickedPos()));
    }

    @Override
    protected BlockState updateShape(BlockState state, Direction direction, BlockState neighborState, LevelAccessor level,
                                     BlockPos pos, BlockPos neighborPos) {
        return state.setValue(POWERED, links(neighborState) || linked(level, pos));
    }

    @Override
    protected VoxelShape getShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        return DeviceModel.ROUTER.shape(state.getValue(FACING));
    }

    @Override
    protected boolean isPathfindable(BlockState state, PathComputationType type) {
        return false;
    }
}
