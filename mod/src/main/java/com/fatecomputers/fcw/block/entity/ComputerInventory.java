package com.fatecomputers.fcw.block.entity;

import com.fatecomputers.fcw.item.HardwareItem;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.ItemStackHandler;

/**
 * Hardware slots of a computer: motherboard, CPU, four memory slots and a graphics card.
 * The same class is used on the client side of the menu so that slot rules match.
 */
public class ComputerInventory extends ItemStackHandler {
    public static final int SLOT_MOTHERBOARD = 0;
    public static final int SLOT_CPU = 1;
    public static final int SLOT_RAM_FIRST = 2;
    public static final int RAM_SLOTS = 4;
    public static final int SLOT_GPU = SLOT_RAM_FIRST + RAM_SLOTS;
    public static final int SIZE = SLOT_GPU + 1;

    private final Runnable onChanged;

    public ComputerInventory(Runnable onChanged) {
        super(SIZE);
        this.onChanged = onChanged;
    }

    public static HardwareItem.Type typeOf(int slot) {
        if (slot == SLOT_MOTHERBOARD) {
            return HardwareItem.Type.MOTHERBOARD;
        }
        if (slot == SLOT_CPU) {
            return HardwareItem.Type.CPU;
        }
        if (slot == SLOT_GPU) {
            return HardwareItem.Type.GPU;
        }
        return HardwareItem.Type.RAM;
    }

    @Override
    public boolean isItemValid(int slot, ItemStack stack) {
        return HardwareItem.is(stack, typeOf(slot));
    }

    @Override
    public int getSlotLimit(int slot) {
        return 1;
    }

    @Override
    protected void onContentsChanged(int slot) {
        onChanged.run();
    }

    public boolean hasMotherboard() {
        return HardwareItem.is(getStackInSlot(SLOT_MOTHERBOARD), HardwareItem.Type.MOTHERBOARD);
    }

    public int cpuCores() {
        ItemStack cpu = getStackInSlot(SLOT_CPU);
        return HardwareItem.is(cpu, HardwareItem.Type.CPU) ? HardwareItem.valueOf(cpu) : 0;
    }

    public int memoryMb() {
        int total = 0;
        for (int i = 0; i < RAM_SLOTS; i++) {
            ItemStack s = getStackInSlot(SLOT_RAM_FIRST + i);
            if (HardwareItem.is(s, HardwareItem.Type.RAM)) {
                total += HardwareItem.valueOf(s);
            }
        }
        return total;
    }

    public boolean hasGpu() {
        return HardwareItem.is(getStackInSlot(SLOT_GPU), HardwareItem.Type.GPU);
    }
}
