package com.fatecomputers.fcw.item;

import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;

import java.util.List;

/**
 * A computer part that goes into a slot of the computer GUI.
 * For a CPU {@link #value()} is the number of vCPUs, for memory it is megabytes.
 */
public class HardwareItem extends Item {
    public enum Type {
        MOTHERBOARD, CPU, RAM, GPU
    }

    private final Type type;
    private final int value;

    public HardwareItem(Properties properties, Type type, int value) {
        super(properties);
        this.type = type;
        this.value = value;
    }

    public Type type() {
        return type;
    }

    public int value() {
        return value;
    }

    public static boolean is(ItemStack stack, Type type) {
        return stack.getItem() instanceof HardwareItem h && h.type == type;
    }

    public static int valueOf(ItemStack stack) {
        return stack.getItem() instanceof HardwareItem h ? h.value : 0;
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        switch (type) {
            case CPU -> tooltip.add(Component.translatable("tooltip.fcw.cpu", value).withStyle(ChatFormatting.GRAY));
            case RAM -> tooltip.add(Component.translatable("tooltip.fcw.ram", value).withStyle(ChatFormatting.GRAY));
            case GPU -> tooltip.add(Component.translatable("tooltip.fcw.gpu").withStyle(ChatFormatting.GRAY));
            case MOTHERBOARD -> tooltip.add(Component.translatable("tooltip.fcw.motherboard").withStyle(ChatFormatting.GRAY));
        }
    }
}
