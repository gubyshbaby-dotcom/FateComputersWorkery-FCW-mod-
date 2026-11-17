package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.production.quality.Quality;
import net.minecraft.ChatFormatting;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.event.entity.player.ItemTooltipEvent;

/**
 * One grey line under the name of boules, wafers and patterned wafers: their class
 * (docs/design/DEFECTS.md, section 7). Shown only where the defect model is on.
 */
final class QualityTooltips {
    private QualityTooltips() {
    }

    static void onTooltip(ItemTooltipEvent event) {
        ItemStack stack = event.getItemStack();
        if (!BuiltInRegistries.ITEM.getKey(stack.getItem()).getNamespace().equals("fcw")) {
            return;
        }
        try {
            if (!Quality.enabled()) {
                return;
            }
        } catch (IllegalStateException e) {
            return; // no world, no server config
        }
        String path = BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath();
        Component line = null;
        if (path.equals("silicon_boule")) {
            line = Component.translatable("tooltip.fcw.quality.crystal", Quality.crystal(stack).name());
        } else if (path.equals("silicon_wafer")) {
            line = Component.translatable("tooltip.fcw.quality.crystal", Quality.crystal(stack).name()).append(" · ")
                    .append(Component.translatable("tooltip.fcw.quality.polish", Quality.polish(stack).name()));
        } else if (path.startsWith("wafer_")) {
            Quality.YieldClass c = Quality.yieldClass(stack);
            line = Component.translatable("tooltip.fcw.quality.yield", c.name(), c.share);
        }
        if (line != null) {
            event.getToolTip().add(Math.min(1, event.getToolTip().size()), line.copy().withStyle(ChatFormatting.GRAY));
        }
    }
}
