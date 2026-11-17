package com.fatecomputers.fcw.production.item;

import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;

import java.util.List;

/**
 * An item of the production chain. Its tooltip says where it comes from and what it is for:
 * lang keys {@code tooltip.fcw.<id>.from} and {@code tooltip.fcw.<id>.to}. Items with durability
 * (photomasks) also show how many uses are left.
 */
public class ProductionItem extends Item {
    public ProductionItem(Properties properties) {
        super(properties);
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        String key = "tooltip." + getDescriptionId(stack).substring("item.".length());
        tooltip.add(Component.translatable(key + ".from").withStyle(ChatFormatting.GRAY));
        tooltip.add(Component.translatable(key + ".to").withStyle(ChatFormatting.DARK_AQUA));
        if (stack.isDamageableItem()) {
            tooltip.add(Component.translatable("tooltip.fcw.uses_left", stack.getMaxDamage() - stack.getDamageValue(), stack.getMaxDamage())
                    .withStyle(ChatFormatting.DARK_GRAY));
        }
    }

    /** Photomasks are precise optics: no enchanting, no anvil repair. */
    @Override
    public boolean isEnchantable(ItemStack stack) {
        return false;
    }
}
