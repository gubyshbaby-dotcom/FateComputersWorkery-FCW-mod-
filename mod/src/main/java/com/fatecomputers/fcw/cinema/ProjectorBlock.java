package com.fatecomputers.fcw.cinema;

import com.fatecomputers.fcw.block.DeviceModel;
import com.fatecomputers.fcw.block.PeripheralBlock;
import com.fatecomputers.fcw.block.PeripheralKind;
import com.fatecomputers.fcw.server.LinkManager;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.StringRepresentable;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.ItemInteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.EnumProperty;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

import javax.annotation.Nullable;

/**
 * Проектор: привязывается к компьютеру, как монитор, и бросает его экран на экран или стену
 * ({@link Projection}). Стоит на полу или висит под потолком ({@code mount}); {@code facing} —
 * куда смотрит объектив; {@code powered} — горит лампа (у модели светится линза).
 *
 * <p>ПКМ пустой рукой включает и выключает, ПКМ лампой меняет лампу, Shift+ПКМ перепривязывает.
 */
public class ProjectorBlock extends PeripheralBlock {
    public enum Mount implements StringRepresentable {
        FLOOR("floor"), CEILING("ceiling");

        private final String id;

        Mount(String id) {
            this.id = id;
        }

        @Override
        public String getSerializedName() {
            return id;
        }
    }

    public static final EnumProperty<Mount> MOUNT = EnumProperty.create("mount", Mount.class);
    private static final MapCodec<ProjectorBlock> CODEC = simpleCodec(ProjectorBlock::new);

    public ProjectorBlock(Properties properties) {
        super(properties, DeviceModel.PROJECTOR, PeripheralKind.PROJECTOR);
        registerDefaultState(defaultBlockState().setValue(MOUNT, Mount.FLOOR));
    }

    @Override
    protected MapCodec<? extends Block> codec() {
        return CODEC;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING, POWERED, MOUNT);
    }

    /** Объектив смотрит туда же, куда игрок: проектор ставят, стоя лицом к экрану. */
    @Override
    public BlockState getStateForPlacement(BlockPlaceContext context) {
        return defaultBlockState()
                .setValue(FACING, context.getHorizontalDirection())
                .setValue(MOUNT, context.getClickedFace() == Direction.DOWN ? Mount.CEILING : Mount.FLOOR);
    }

    @Override
    protected VoxelShape getShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        DeviceModel m = state.getValue(MOUNT) == Mount.CEILING ? DeviceModel.PROJECTOR_CEILING : DeviceModel.PROJECTOR;
        return m.shape(state.getValue(FACING));
    }

    @Nullable
    @Override
    public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
        return new ProjectorBlockEntity(pos, state);
    }

    @Nullable
    @Override
    public <T extends BlockEntity> BlockEntityTicker<T> getTicker(Level level, BlockState state, BlockEntityType<T> type) {
        if (level.isClientSide || type != Cinema.PROJECTOR_BE.get()) {
            return null;
        }
        return (l, p, s, be) -> ((ProjectorBlockEntity) be).serverTick();
    }

    @Override
    protected ItemInteractionResult useItemOn(ItemStack stack, BlockState state, Level level, BlockPos pos, Player player,
                                              InteractionHand hand, BlockHitResult hit) {
        if (!stack.is(Cinema.PROJECTOR_LAMP.get()) || !(level.getBlockEntity(pos) instanceof ProjectorBlockEntity be)) {
            return ItemInteractionResult.PASS_TO_DEFAULT_BLOCK_INTERACTION;
        }
        if (!level.isClientSide && player instanceof ServerPlayer sp) {
            be.replaceLamp(sp, stack);
        }
        return ItemInteractionResult.sidedSuccess(level.isClientSide);
    }

    /** Редстоун: фронт сигнала включает проектор, спад — выключает (как ПКМ, с остыванием). */
    @Override
    protected void neighborChanged(BlockState state, Level level, BlockPos pos, net.minecraft.world.level.block.Block block,
                                   BlockPos from, boolean moving) {
        super.neighborChanged(state, level, pos, block, from, moving);
        if (!level.isClientSide && level.getBlockEntity(pos) instanceof ProjectorBlockEntity be) {
            be.onRedstone(level.hasNeighborSignal(pos));
        }
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        if (!(level.getBlockEntity(pos) instanceof ProjectorBlockEntity be)) {
            return InteractionResult.PASS;
        }
        if (!level.isClientSide && player instanceof ServerPlayer sp) {
            if (player.isSecondaryUseActive()) {
                LinkManager.peripheralShiftClick(sp, be);
            } else {
                be.toggle(sp);
            }
        }
        return InteractionResult.sidedSuccess(level.isClientSide);
    }
}
