package com.fatecomputers.fcw.block;

import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import com.fatecomputers.fcw.network.ScreenAccessRequestPayload;
import com.fatecomputers.fcw.server.LinkManager;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.BlockHitResult;
import net.neoforged.neoforge.network.PacketDistributor;

import javax.annotation.Nullable;

/**
 * Monitor, keyboard, mouse or speaker. Linked to the nearest computer when placed;
 * sneak + right click (empty hand) on a computer and then on the device re-links it.
 * Right click on a monitor or keyboard opens the computer's screen.
 */
public class PeripheralBlock extends DeviceBlock implements EntityBlock {
    protected final PeripheralKind kind;
    private final MapCodec<PeripheralBlock> codec;

    public PeripheralBlock(Properties properties, DeviceModel model, PeripheralKind kind) {
        super(properties, model);
        this.kind = kind;
        this.codec = simpleCodec(p -> new PeripheralBlock(p, model, kind));
    }

    @Override
    protected MapCodec<? extends Block> codec() {
        return codec;
    }

    public PeripheralKind kind() {
        return kind;
    }

    @Nullable
    @Override
    public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
        return kind == PeripheralKind.MONITOR ? new MonitorBlockEntity(pos, state) : new PeripheralBlockEntity(pos, state);
    }

    @Override
    public void setPlacedBy(Level level, BlockPos pos, BlockState state, @Nullable LivingEntity placer, ItemStack stack) {
        super.setPlacedBy(level, pos, state, placer, stack);
        if (!level.isClientSide && level.getBlockEntity(pos) instanceof PeripheralBlockEntity be) {
            LinkManager.autoLink(be, placer instanceof ServerPlayer sp ? sp : null);
        }
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        if (!(level.getBlockEntity(pos) instanceof PeripheralBlockEntity be)) {
            return InteractionResult.PASS;
        }
        if (player.isSecondaryUseActive()) {
            if (!level.isClientSide && player instanceof ServerPlayer sp) {
                LinkManager.peripheralShiftClick(sp, be);
            }
            return InteractionResult.sidedSuccess(level.isClientSide);
        }
        if (kind == PeripheralKind.MONITOR || kind == PeripheralKind.KEYBOARD) {
            if (level.isClientSide) {
                if (be.isLinked()) {
                    // The server checks distance and rights and answers with a token (ScreenAccessPayload).
                    PacketDistributor.sendToServer(new ScreenAccessRequestPayload(pos, true));
                } else {
                    player.displayClientMessage(Component.translatable("message.fcw.not_linked"), true);
                }
            }
            return InteractionResult.sidedSuccess(level.isClientSide);
        }
        if (!level.isClientSide && player instanceof ServerPlayer sp) {
            LinkManager.describeLink(sp, be);
        }
        return InteractionResult.sidedSuccess(level.isClientSide);
    }

    @Override
    protected void onRemove(BlockState state, Level level, BlockPos pos, BlockState newState, boolean movedByPiston) {
        if (!state.is(newState.getBlock()) && !level.isClientSide && level.getBlockEntity(pos) instanceof PeripheralBlockEntity be) {
            be.onBroken();
        }
        super.onRemove(state, level, pos, newState, movedByPiston);
    }
}
