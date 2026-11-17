package com.fatecomputers.fcw.cinema;

import net.minecraft.ChatFormatting;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.level.block.Block;

import java.util.List;
import java.util.Locale;

/**
 * Предмет блока с подсказками: первая строка серая, остальные тёмно-серые. У снятого проектора
 * дополнительно показывает наработку лампы, которую он унёс с собой.
 */
public class TooltipBlockItem extends BlockItem {
    private final String[] lines;

    public TooltipBlockItem(Block block, Properties properties, String... lines) {
        super(block, properties);
        this.lines = lines;
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        for (int i = 0; i < lines.length; i++) {
            tooltip.add(Component.translatable(lines[i]).withStyle(i == 0 ? ChatFormatting.GRAY : ChatFormatting.DARK_GRAY));
        }
        CustomData data = stack.get(DataComponents.CUSTOM_DATA);
        if (data != null && data.contains(ProjectorBlockEntity.LAMP_TICKS)) {
            double hours = data.copyTag().getInt(ProjectorBlockEntity.LAMP_TICKS) / (double) ProjectorBlockEntity.TICKS_PER_HOUR;
            tooltip.add(Component.translatable("tooltip.fcw.projector.lamp_used", String.format(Locale.ROOT, "%.1f", hours))
                    .withStyle(ChatFormatting.GOLD));
        }
    }
}
