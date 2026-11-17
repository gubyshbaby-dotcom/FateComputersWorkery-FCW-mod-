package com.fatecomputers.fcw.menu;

import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.block.entity.ComputerInventory;
import com.fatecomputers.fcw.registry.ModMenus;
import net.minecraft.core.BlockPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.SlotItemHandler;

import javax.annotation.Nullable;

/**
 * Computer GUI: hardware slots plus the player inventory. Everything else (OS list, status)
 * comes through {@link com.fatecomputers.fcw.network.ComputerStatusPayload}.
 * Parts cannot be changed while the machine is on.
 */
public class ComputerMenu extends AbstractContainerMenu {
    public static final int IMAGE_WIDTH = 280;
    public static final int IMAGE_HEIGHT = 228;
    public static final int PLAYER_INV_X = 59;
    public static final int PLAYER_INV_Y = 146;

    // Hardware slot positions (left column of the GUI)
    public static final int[][] SLOT_POS = {
            {9, 20},    // motherboard
            {31, 20},   // cpu
            {9, 42}, {27, 42}, {45, 42}, {63, 42}, // memory
            {53, 20},   // gpu
    };

    private final BlockPos pos;
    private final Level level;
    @Nullable
    private final ComputerBlockEntity computer;
    private final int hardwareSlots;

    /** Client constructor (extra data = block position). */
    public ComputerMenu(int containerId, Inventory playerInventory, RegistryFriendlyByteBuf buf) {
        this(containerId, playerInventory, buf.readBlockPos(), new ComputerInventory(() -> {
        }), null);
    }

    /** Server constructor. */
    public ComputerMenu(int containerId, Inventory playerInventory, ComputerBlockEntity computer) {
        this(containerId, playerInventory, computer.getBlockPos(), computer.inventory(), computer);
    }

    private ComputerMenu(int containerId, Inventory playerInventory, BlockPos pos, IItemHandler items, @Nullable ComputerBlockEntity computer) {
        super(ModMenus.COMPUTER.get(), containerId);
        this.pos = pos;
        this.level = playerInventory.player.level();
        this.computer = computer;
        this.hardwareSlots = items.getSlots();
        for (int i = 0; i < items.getSlots(); i++) {
            addSlot(new HardwareSlot(items, i, SLOT_POS[i][0], SLOT_POS[i][1]));
        }
        for (int row = 0; row < 3; row++) {
            for (int col = 0; col < 9; col++) {
                addSlot(new Slot(playerInventory, col + row * 9 + 9, PLAYER_INV_X + col * 18, PLAYER_INV_Y + row * 18));
            }
        }
        for (int col = 0; col < 9; col++) {
            addSlot(new Slot(playerInventory, col, PLAYER_INV_X + col * 18, PLAYER_INV_Y + 58));
        }
    }

    public BlockPos pos() {
        return pos;
    }

    @Nullable
    public ComputerBlockEntity computer() {
        return computer;
    }

    /** Parts are locked while the machine is powered (block state is synced to both sides). */
    public boolean isLocked() {
        return DeviceBlock.isPowered(level.getBlockState(pos));
    }

    @Override
    public boolean stillValid(Player player) {
        if (computer != null) {
            return computer.stillValid(player);
        }
        return player.distanceToSqr(pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5) <= 64.0;
    }

    @Override
    public ItemStack quickMoveStack(Player player, int index) {
        Slot slot = slots.get(index);
        if (!slot.hasItem()) {
            return ItemStack.EMPTY;
        }
        ItemStack stack = slot.getItem();
        ItemStack original = stack.copy();
        int total = slots.size();
        if (index < hardwareSlots) {
            if (isLocked() || !moveItemStackTo(stack, hardwareSlots, total, true)) {
                return ItemStack.EMPTY;
            }
        } else {
            if (isLocked() || !moveItemStackTo(stack, 0, hardwareSlots, false)) {
                return ItemStack.EMPTY;
            }
        }
        if (stack.isEmpty()) {
            slot.setByPlayer(ItemStack.EMPTY);
        } else {
            slot.setChanged();
        }
        if (stack.getCount() == original.getCount()) {
            return ItemStack.EMPTY;
        }
        slot.onTake(player, stack);
        return original;
    }

    private class HardwareSlot extends SlotItemHandler {
        HardwareSlot(IItemHandler handler, int index, int x, int y) {
            super(handler, index, x, y);
        }

        @Override
        public boolean mayPlace(ItemStack stack) {
            return !isLocked() && super.mayPlace(stack);
        }

        @Override
        public boolean mayPickup(Player player) {
            return !isLocked() && super.mayPickup(player);
        }
    }
}
