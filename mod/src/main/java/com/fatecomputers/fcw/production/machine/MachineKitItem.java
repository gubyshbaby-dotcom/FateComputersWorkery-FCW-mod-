package com.fatecomputers.fcw.production.machine;

import net.minecraft.ChatFormatting;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;

import java.util.List;

/**
 * A machine in a box. Right click on the ground puts the whole machine in front of the player, its
 * operator side towards them and centred on the clicked spot, if every cell is free; otherwise it says
 * how many blocks are in the way and shows them. Taking the machine apart gives the kit back.
 */
public class MachineKitItem extends Item {
    private final MachineDef def;

    public MachineKitItem(Properties properties, MachineDef def) {
        super(properties);
        this.def = def;
    }

    public MachineDef def() {
        return def;
    }

    /** Where the machine goes for a click at this position by a player looking in this direction. */
    public static BlockPos originFor(MachineDef def, BlockPos clicked, Direction facing) {
        return MachineDef.origin(clicked, facing, def.sx / 2, 0, 0);
    }

    @Override
    public InteractionResult useOn(UseOnContext context) {
        Level level = context.getLevel();
        Player player = context.getPlayer();
        if (level.isClientSide || player == null) {
            return InteractionResult.SUCCESS;
        }
        BlockPos clicked = context.getClickedPos();
        if (!level.getBlockState(clicked).canBeReplaced()) {
            clicked = clicked.relative(context.getClickedFace());
        }
        Direction facing = context.getHorizontalDirection().getOpposite();
        BlockPos origin = originFor(def, clicked, facing);
        List<BlockPos> blockers = MachineStructure.blockers(level, origin, facing, def);
        if (!blockers.isEmpty()) {
            BlockPos first = blockers.get(0);
            player.displayClientMessage(Component.translatable("message.fcw.machine.blocked", blockers.size(),
                    first.getX() + " " + first.getY() + " " + first.getZ(), def.sx, def.sy, def.sz).withStyle(ChatFormatting.RED), false);
            if (player instanceof ServerPlayer sp) {
                MachineStructure.highlight(sp, blockers);
            }
            return InteractionResult.FAIL;
        }
        MachineStructure.place(level, origin, facing, def);
        level.playSound(null, clicked, SoundEvents.ANVIL_PLACE, SoundSource.BLOCKS, 0.8f, 0.8f);
        if (!player.getAbilities().instabuild) {
            context.getItemInHand().shrink(1);
        }
        return InteractionResult.CONSUME;
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        tooltip.add(Component.translatable("tooltip.fcw.machine_kit", def.sx, def.sy, def.sz).withStyle(ChatFormatting.GRAY));
        tooltip.add(Component.translatable("tooltip.fcw.machine_kit.break").withStyle(ChatFormatting.DARK_GRAY));
    }
}
