package com.fatecomputers.fcw.block;

import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.server.LinkManager;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.phys.BlockHitResult;

import javax.annotation.Nullable;

/**
 * A computer case. powered = the virtual machine is running.
 * Right click opens the GUI; sneak + right click with an empty hand selects it for linking peripherals.
 */
public class ComputerBlock extends DeviceBlock implements EntityBlock {
    private final MapCodec<ComputerBlock> codec;

    public ComputerBlock(Properties properties, DeviceModel model) {
        super(properties, model);
        this.codec = simpleCodec(p -> new ComputerBlock(p, model));
    }

    @Override
    protected MapCodec<? extends Block> codec() {
        return codec;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING, POWERED);
    }

    @Nullable
    @Override
    public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
        return new ComputerBlockEntity(pos, state);
    }

    @Override
    public void setPlacedBy(Level level, BlockPos pos, BlockState state, @Nullable LivingEntity placer, ItemStack stack) {
        super.setPlacedBy(level, pos, state, placer, stack);
        if (!level.isClientSide && placer instanceof Player player && level.getBlockEntity(pos) instanceof ComputerBlockEntity be) {
            be.setOwner(player.getUUID());
        }
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        if (level.isClientSide) {
            return InteractionResult.SUCCESS;
        }
        if (!(level.getBlockEntity(pos) instanceof ComputerBlockEntity be) || !(player instanceof ServerPlayer sp)) {
            return InteractionResult.PASS;
        }
        if (player.isSecondaryUseActive()) {
            LinkManager.selectComputer(sp, be);
        } else {
            sp.openMenu(be, buf -> buf.writeBlockPos(pos));
        }
        return InteractionResult.CONSUME;
    }

    @Override
    protected void onRemove(BlockState state, Level level, BlockPos pos, BlockState newState, boolean movedByPiston) {
        if (!state.is(newState.getBlock()) && !level.isClientSide && level.getBlockEntity(pos) instanceof ComputerBlockEntity be) {
            be.onBroken();
        }
        super.onRemove(state, level, pos, newState, movedByPiston);
    }

    @Override
    protected void neighborChanged(BlockState state, Level level, BlockPos pos, Block neighborBlock, BlockPos neighborPos, boolean movedByPiston) {
        super.neighborChanged(state, level, pos, neighborBlock, neighborPos, movedByPiston);
        if (!level.isClientSide && level.getBlockEntity(pos) instanceof ComputerBlockEntity be) {
            be.updateRedstoneInputs(false);
        }
    }

    @Override
    protected boolean isSignalSource(BlockState state) {
        return true;
    }

    /** {@code direction} points from the querying neighbour to this block, so the output face is its opposite. */
    @Override
    protected int getSignal(BlockState state, BlockGetter level, BlockPos pos, Direction direction) {
        if (level.getBlockEntity(pos) instanceof ComputerBlockEntity be) {
            return be.getRedstoneOutput(direction.getOpposite());
        }
        return 0;
    }

    @Override
    protected int getDirectSignal(BlockState state, BlockGetter level, BlockPos pos, Direction direction) {
        return getSignal(state, level, pos, direction);
    }
}
