package com.fatecomputers.fcw.cinema;

import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;

import java.util.List;

/** Запасная лампа проектора. Саму замену делает {@link ProjectorBlock#useItemOn}. */
public class ProjectorLampItem extends Item {
    public ProjectorLampItem(Properties properties) {
        super(properties);
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        tooltip.add(Component.translatable("tooltip.fcw.projector_lamp").withStyle(ChatFormatting.GRAY));
    }
}
