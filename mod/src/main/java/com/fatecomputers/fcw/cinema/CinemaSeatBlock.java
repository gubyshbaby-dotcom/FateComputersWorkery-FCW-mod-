package com.fatecomputers.fcw.cinema;

import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.DeviceModel;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.commands.arguments.EntityAnchorArgument;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

import java.util.List;

/**
 * Кресло кинозала. {@code facing} — куда смотрит сидящий (при установке: куда смотрит игрок).
 * {@code left} и {@code right} (от сидящего) — есть ли в ряду сосед с тем же направлением: тогда
 * кресла делят подлокотник. ПКМ сажает игрока на невидимую сущность {@link SeatEntity}; встать — Shift.
 */
public class CinemaSeatBlock extends DeviceBlock {
    public static final BooleanProperty LEFT = BooleanProperty.create("left");
    public static final BooleanProperty RIGHT = BooleanProperty.create("right");
    /** Высота подушки над низом блока. */
    public static final double SEAT_HEIGHT = 6.0 / 16.0;
    private static final MapCodec<CinemaSeatBlock> CODEC = simpleCodec(CinemaSeatBlock::new);

    public CinemaSeatBlock(Properties properties) {
        super(properties, DeviceModel.CINEMA_SEAT);
        registerDefaultState(defaultBlockState().setValue(LEFT, false).setValue(RIGHT, false));
    }

    @Override
    protected MapCodec<? extends Block> codec() {
        return CODEC;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING, LEFT, RIGHT);
    }

    @Override
    public BlockState getStateForPlacement(BlockPlaceContext context) {
        return connect(defaultBlockState().setValue(FACING, context.getHorizontalDirection()), context.getLevel(), context.getClickedPos());
    }

    @Override
    protected BlockState updateShape(BlockState state, Direction direction, BlockState neighborState, LevelAccessor level,
                                     BlockPos pos, BlockPos neighborPos) {
        return connect(state, level, pos);
    }

    private static BlockState connect(BlockState state, BlockGetter level, BlockPos pos) {
        Direction facing = state.getValue(FACING);
        return state
                .setValue(LEFT, joins(level, pos.relative(facing.getCounterClockWise()), facing))
                .setValue(RIGHT, joins(level, pos.relative(facing.getClockWise()), facing));
    }

    private static boolean joins(BlockGetter level, BlockPos pos, Direction facing) {
        BlockState s = level.getBlockState(pos);
        return s.getBlock() instanceof CinemaSeatBlock && s.getValue(FACING) == facing;
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        if (player.isSecondaryUseActive() || player.isPassenger()) {
            return InteractionResult.PASS;
        }
        if (level.isClientSide) {
            return InteractionResult.SUCCESS;
        }
        List<SeatEntity> seats = level.getEntitiesOfClass(SeatEntity.class, new AABB(pos));
        for (SeatEntity s : seats) {
            if (s.isVehicle()) {
                player.displayClientMessage(Component.translatable("message.fcw.seat.taken"), true);
                return InteractionResult.CONSUME;
            }
        }
        SeatEntity seat = seats.isEmpty() ? null : seats.get(0);
        if (seat == null) {
            seat = new SeatEntity(Cinema.SEAT.get(), level);
            seat.setPos(pos.getX() + 0.5, pos.getY() + SEAT_HEIGHT, pos.getZ() + 0.5);
            seat.setYRot(state.getValue(FACING).toYRot());
            level.addFreshEntity(seat);
        }
        if (player.startRiding(seat) && player instanceof ServerPlayer sp) {
            // развернуть севшего лицом туда, куда смотрит кресло: в зале это экран
            Direction facing = state.getValue(FACING);
            sp.lookAt(EntityAnchorArgument.Anchor.EYES, new Vec3(
                    pos.getX() + 0.5 + facing.getStepX() * 8, sp.getEyeY(), pos.getZ() + 0.5 + facing.getStepZ() * 8));
        }
        return InteractionResult.CONSUME;
    }
}
