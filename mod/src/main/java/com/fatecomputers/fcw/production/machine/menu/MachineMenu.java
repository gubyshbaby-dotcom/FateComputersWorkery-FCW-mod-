package com.fatecomputers.fcw.production.machine.menu;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.Status;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import net.minecraft.core.BlockPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerData;
import net.minecraft.world.inventory.SimpleContainerData;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemStackHandler;
import net.neoforged.neoforge.items.SlotItemHandler;

import javax.annotation.Nullable;

/**
 * GUI of a machine's terminal: per section the input slots, the catalyst (the reticle of the scanner),
 * the output slots and the progress; energy, state and the start/stop button (the same as the breaker).
 */
public class MachineMenu extends AbstractContainerMenu {
    public static final int WIDTH = 176;
    public static final int SECTION_Y = 32;
    public static final int SECTION_HEIGHT = 50;
    public static final int INPUT_X = 8;
    public static final int CATALYST_X = 70;
    public static final int ARROW_X = 92;
    public static final int ARROW_W = 32;
    public static final int OUTPUT_X = 132;
    public static final int BUTTON_TOGGLE = 0;

    private final BlockPos pos;
    private final MachineDef def;
    @Nullable
    private final MachineControllerBlockEntity machine;
    private final ContainerData data;
    private final int machineSlots;
    private final net.minecraft.world.level.Level level;

    /** Client constructor (extra data = controller position and machine name). */
    public MachineMenu(int containerId, Inventory playerInventory, RegistryFriendlyByteBuf buf) {
        this(containerId, playerInventory, buf.readBlockPos(), Machines.byName(buf.readUtf()), null);
    }

    /** Server constructor. */
    public MachineMenu(int containerId, Inventory playerInventory, MachineControllerBlockEntity machine) {
        this(containerId, playerInventory, machine.getBlockPos(), machine.def(), machine);
    }

    private MachineMenu(int containerId, Inventory playerInventory, BlockPos pos, MachineDef def, @Nullable MachineControllerBlockEntity machine) {
        super(ProductionMachines.MENU.get(), containerId);
        this.pos = pos;
        this.def = def;
        this.machine = machine;
        this.level = playerInventory.player.level();
        int total = 0;
        for (Section s : def.spec.sections()) {
            total += s.slots();
        }
        IItemHandler items = machine != null ? machine.items() : new ItemStackHandler(total);
        this.data = machine != null ? machine.data() : new SimpleContainerData(3 + def.spec.sections().size() * 2);
        int index = 0;
        for (int i = 0; i < def.spec.sections().size(); i++) {
            Section s = def.spec.sections().get(i);
            int y = sectionY(i) + 12;
            int cols = inputColumns(s);
            for (int k = 0; k < s.inputs(); k++) {
                addSlot(new FilteredSlot(items, index++, INPUT_X + (k % cols) * 18, y + (k / cols) * 18));
            }
            if (s.catalyst()) {
                addSlot(new FilteredSlot(items, index++, CATALYST_X, y + 9));
            }
            for (int k = 0; k < s.outputs(); k++) {
                addSlot(new OutputSlot(items, index++, OUTPUT_X + (k % 2) * 18, y + (k / 2) * 18));
            }
        }
        machineSlots = index;
        int invY = playerInventoryY(def);
        for (int row = 0; row < 3; row++) {
            for (int col = 0; col < 9; col++) {
                addSlot(new Slot(playerInventory, col + row * 9 + 9, 8 + col * 18, invY + row * 18));
            }
        }
        for (int col = 0; col < 9; col++) {
            addSlot(new Slot(playerInventory, col, 8 + col * 18, invY + 58));
        }
        addDataSlots(data);
    }

    public static int inputColumns(Section s) {
        return s.inputs() > 4 ? 3 : 2;
    }

    public static int sectionY(int section) {
        return SECTION_Y + section * SECTION_HEIGHT;
    }

    public static int playerInventoryY(MachineDef def) {
        return sectionY(def.spec.sections().size()) + 12;
    }

    public static int height(MachineDef def) {
        return playerInventoryY(def) + 82;
    }

    public MachineDef def() {
        return def;
    }

    public BlockPos pos() {
        return pos;
    }

    public int energy() {
        return (data.get(0) & 0xFFFF) | (data.get(1) << 16);
    }

    public boolean enabled() {
        return data.get(2) != 0;
    }

    /** 0..1000 */
    public int progress(int section) {
        return data.get(3 + section * 2);
    }

    public Status status(int section) {
        int i = data.get(4 + section * 2);
        return i >= 0 && i < Status.values().length ? Status.values()[i] : Status.OFF;
    }

    @Override
    public boolean clickMenuButton(Player player, int id) {
        if (id == BUTTON_TOGGLE && machine != null) {
            machine.setEnabled(!machine.isEnabled());
            return true;
        }
        return false;
    }

    @Override
    public boolean stillValid(Player player) {
        if (machine != null) {
            return machine.stillValid(player);
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
        if (index < machineSlots) {
            if (!moveItemStackTo(stack, machineSlots, slots.size(), true)) {
                return ItemStack.EMPTY;
            }
        } else if (!moveItemStackTo(stack, 0, machineSlots, false)) { // output slots refuse by mayPlace
            return ItemStack.EMPTY;
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

    /** Input and catalyst slots take only what the recipes of their section use (so shift-click finds the right slot). */
    private class FilteredSlot extends SlotItemHandler {
        private final int index;

        FilteredSlot(IItemHandler handler, int index, int x, int y) {
            super(handler, index, x, y);
            this.index = index;
        }

        @Override
        public boolean mayPlace(ItemStack stack) {
            return MachineControllerBlockEntity.accepts(level, def, index, stack);
        }
    }

    private static class OutputSlot extends SlotItemHandler {
        OutputSlot(IItemHandler handler, int index, int x, int y) {
            super(handler, index, x, y);
        }

        @Override
        public boolean mayPlace(ItemStack stack) {
            return false;
        }
    }
}
