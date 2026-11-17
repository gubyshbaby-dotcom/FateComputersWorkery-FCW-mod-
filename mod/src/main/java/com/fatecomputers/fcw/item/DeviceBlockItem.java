package com.fatecomputers.fcw.item;

import com.fatecomputers.fcw.block.PeripheralKind;
import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.level.block.Block;

import javax.annotation.Nullable;
import java.util.List;

/** Block item with a usage hint; computers also warn that breaking deletes the disk. */
public class DeviceBlockItem extends BlockItem {
    @Nullable
    private final PeripheralKind kind;

    /** @param kind null for a computer */
    public DeviceBlockItem(Block block, Properties properties, @Nullable PeripheralKind kind) {
        super(block, properties);
        this.kind = kind;
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        if (kind == null) {
            tooltip.add(Component.translatable("tooltip.fcw.computer").withStyle(ChatFormatting.GRAY));
            tooltip.add(Component.translatable("tooltip.fcw.computer.link").withStyle(ChatFormatting.DARK_GRAY));
            tooltip.add(Component.translatable("tooltip.fcw.computer.name").withStyle(ChatFormatting.DARK_GRAY));
            tooltip.add(Component.translatable("tooltip.fcw.computer.break_warning").withStyle(ChatFormatting.RED));
        } else {
            tooltip.add(Component.translatable("tooltip.fcw." + kind.id()).withStyle(ChatFormatting.GRAY));
            tooltip.add(Component.translatable("tooltip.fcw.peripheral.link").withStyle(ChatFormatting.DARK_GRAY));
        }
    }
}
