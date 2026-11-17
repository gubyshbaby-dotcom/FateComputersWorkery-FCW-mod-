package com.fatecomputers.fcw.production.machine.euv;

import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineStructure;
import com.fatecomputers.fcw.production.machine.Machines;
import net.minecraft.ChatFormatting;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.phys.BlockHitResult;

import java.util.List;

/**
 * The controller of the EUV scanner blank. Its front faces the operator: that is the front of the whole
 * machine. Right click checks the blank against {@link EuvPattern}, names the first block that is wrong
 * and, when everything is in place, turns the blank into the machine.
 */
public class EuvControllerBlock extends EuvKitBlock {
    public static final BooleanProperty POWERED = BlockStateProperties.POWERED;

    private EuvControllerBlock(BlockBehaviour.Properties properties) {
        super(properties, Facing.HORIZONTAL);
        registerDefaultState(defaultBlockState().setValue(POWERED, false));
    }

    public static EuvControllerBlock create(BlockBehaviour.Properties properties) {
        pending(Facing.HORIZONTAL);
        try {
            return new EuvControllerBlock(properties);
        } finally {
            clearPending();
        }
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        super.createBlockStateDefinition(builder);
        builder.add(POWERED);
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        if (level.isClientSide) {
            return InteractionResult.SUCCESS;
        }
        MachineDef def = Machines.EUV_SCANNER;
        Direction facing = state.getValue(BlockStateProperties.HORIZONTAL_FACING);
        BlockPos origin = MachineDef.origin(pos, facing, def.terminal.x(), def.terminal.y(), def.terminal.z());
        EuvPattern.Mismatch m = EuvPattern.get().check(level, origin, facing);
        if (m != null) {
            Component cell = Component.literal(m.x() + ", " + m.y() + ", " + m.z());
            Component where = Component.literal(m.pos().getX() + " " + m.pos().getY() + " " + m.pos().getZ());
            Component msg = m.wrongFacing()
                    ? Component.translatable("message.fcw.euv.wrong_facing", cell, where, m.expected().block().getName(),
                            Component.translatable("direction.fcw." + m.needFacing().getName()))
                    : Component.translatable("message.fcw.euv.mismatch", cell, where, m.expected().block().getName(),
                            m.found().isAir() ? Component.translatable("message.fcw.euv.nothing") : m.found().getBlock().getName());
            player.displayClientMessage(msg.copy().withStyle(ChatFormatting.RED), false);
            if (player instanceof ServerPlayer sp) {
                MachineStructure.highlight(sp, List.of(m.pos()));
            }
            return InteractionResult.CONSUME;
        }
        MachineStructure.place(level, origin, facing, def);
        level.playSound(null, pos, SoundEvents.BEACON_ACTIVATE, SoundSource.BLOCKS, 1.0f, 1.0f);
        player.displayClientMessage(Component.translatable("message.fcw.euv.assembled").withStyle(ChatFormatting.GREEN), false);
        return InteractionResult.CONSUME;
    }
}
