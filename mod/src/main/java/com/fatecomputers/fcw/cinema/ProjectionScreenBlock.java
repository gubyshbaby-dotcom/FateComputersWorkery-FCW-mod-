package com.fatecomputers.fcw.cinema;

import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.DeviceModel;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BooleanProperty;

/**
 * Проекционный экран: полотно толщиной 1/16 у стены. {@code facing} — куда смотрит белая сторона.
 * Соседние полотна в одной плоскости сливаются: {@code up}, {@code down}, {@code left},
 * {@code right} (от зрителя) говорят, есть ли сосед, и там, где его нет, модель рисует чёрную рамку.
 * Какой прямоугольник считать экраном, решает {@link Projection}.
 */
public class ProjectionScreenBlock extends DeviceBlock {
    public static final BooleanProperty UP = BooleanProperty.create("up");
    public static final BooleanProperty DOWN = BooleanProperty.create("down");
    public static final BooleanProperty LEFT = BooleanProperty.create("left");
    public static final BooleanProperty RIGHT = BooleanProperty.create("right");
    private static final MapCodec<ProjectionScreenBlock> CODEC = simpleCodec(ProjectionScreenBlock::new);

    public ProjectionScreenBlock(Properties properties) {
        super(properties, DeviceModel.PROJECTION_SCREEN);
        registerDefaultState(defaultBlockState().setValue(UP, false).setValue(DOWN, false).setValue(LEFT, false).setValue(RIGHT, false));
    }

    @Override
    protected MapCodec<? extends Block> codec() {
        return CODEC;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING, UP, DOWN, LEFT, RIGHT);
    }

    /** Щелчок по стене вешает полотно на неё; по полу или потолку — лицом к игроку. */
    @Override
    public BlockState getStateForPlacement(BlockPlaceContext context) {
        Direction face = context.getClickedFace();
        Direction facing = face.getAxis().isHorizontal() ? face : context.getHorizontalDirection().getOpposite();
        return connect(defaultBlockState().setValue(FACING, facing), context.getLevel(), context.getClickedPos());
    }

    @Override
    protected BlockState updateShape(BlockState state, Direction direction, BlockState neighborState, LevelAccessor level,
                                     BlockPos pos, BlockPos neighborPos) {
        return connect(state, level, pos);
    }

    private static BlockState connect(BlockState state, BlockGetter level, BlockPos pos) {
        Direction facing = state.getValue(FACING);
        return state
                .setValue(UP, joins(level, pos.above(), facing))
                .setValue(DOWN, joins(level, pos.below(), facing))
                .setValue(LEFT, joins(level, pos.relative(facing.getClockWise()), facing))
                .setValue(RIGHT, joins(level, pos.relative(facing.getCounterClockWise()), facing));
    }

    private static boolean joins(BlockGetter level, BlockPos pos, Direction facing) {
        BlockState s = level.getBlockState(pos);
        return s.getBlock() instanceof ProjectionScreenBlock && s.getValue(FACING) == facing;
    }

    @Override
    protected boolean propagatesSkylightDown(BlockState state, BlockGetter level, BlockPos pos) {
        return true;
    }
}
