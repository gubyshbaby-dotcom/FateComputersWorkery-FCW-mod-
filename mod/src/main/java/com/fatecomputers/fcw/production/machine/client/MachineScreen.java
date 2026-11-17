package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.Status;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.menu.MachineMenu;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.inventory.Slot;

import java.util.List;

/**
 * Terminal of a machine: energy, the start/stop button (the breaker), and per section the inputs, the
 * catalyst, the progress and the outputs with the section's state. Drawn with plain fills, like the
 * computer GUI.
 */
public class MachineScreen extends AbstractContainerScreen<MachineMenu> {
    private static final int BG = 0xFF2B2D31;
    private static final int PANEL = 0xFF1E1F22;
    private static final int BORDER_LIGHT = 0xFF4A4D55;
    private static final int BORDER_DARK = 0xFF111214;
    private static final int TEXT = 0xFFE6E6E6;
    private static final int TEXT_DIM = 0xFF9A9CA3;
    private static final int ACCENT = 0xFFB07CFF;
    private static final int ENERGY = 0xFFE0B040;
    private static final int CATALYST = 0xFF3A2F55;

    private static final int ENERGY_X = 8, ENERGY_Y = 19, ENERGY_W = 96, ENERGY_H = 8;

    private Button toggle;

    public MachineScreen(MachineMenu menu, Inventory inventory, Component title) {
        super(menu, inventory, title);
        this.imageWidth = MachineMenu.WIDTH;
        this.imageHeight = MachineMenu.height(menu.def());
    }

    @Override
    protected void init() {
        super.init();
        titleLabelX = 8;
        titleLabelY = 6;
        inventoryLabelX = 8;
        inventoryLabelY = MachineMenu.playerInventoryY(menu.def()) - 10;
        toggle = addRenderableWidget(Button.builder(Component.empty(), b -> {
            if (minecraft != null && minecraft.gameMode != null) {
                minecraft.gameMode.handleInventoryButtonClick(menu.containerId, MachineMenu.BUTTON_TOGGLE);
            }
        }).bounds(leftPos + 110, topPos + 15, 58, 14).build());
    }

    @Override
    protected void containerTick() {
        super.containerTick();
        toggle.setMessage(Component.translatable(menu.enabled() ? "screen.fcw.machine.stop" : "screen.fcw.machine.start"));
    }

    @Override
    public void render(GuiGraphics g, int mouseX, int mouseY, float partialTick) {
        super.render(g, mouseX, mouseY, partialTick);
        renderTooltip(g, mouseX, mouseY);
        int x = mouseX - leftPos, y = mouseY - topPos;
        if (x >= ENERGY_X && x < ENERGY_X + ENERGY_W && y >= ENERGY_Y && y < ENERGY_Y + ENERGY_H) {
            Component line = Component.translatable("screen.fcw.machine.energy", menu.energy(), MachineControllerBlockEntity.ENERGY_CAPACITY);
            Component mode = Component.translatable(FcwConfig.SERVER.productionRequirePower.get()
                    ? "screen.fcw.machine.power_required" : "screen.fcw.machine.power_builtin");
            g.renderComponentTooltip(font, List.of(line, mode), mouseX, mouseY);
        }
    }

    @Override
    protected void renderBg(GuiGraphics g, float partialTick, int mouseX, int mouseY) {
        int x0 = leftPos, y0 = topPos;
        panel(g, x0, y0, imageWidth, imageHeight, BG);
        // energy
        g.fill(x0 + ENERGY_X - 1, y0 + ENERGY_Y - 1, x0 + ENERGY_X + ENERGY_W + 1, y0 + ENERGY_Y + ENERGY_H + 1, BORDER_DARK);
        g.fill(x0 + ENERGY_X, y0 + ENERGY_Y, x0 + ENERGY_X + ENERGY_W, y0 + ENERGY_Y + ENERGY_H, PANEL);
        int filled = (int) ((long) menu.energy() * ENERGY_W / MachineControllerBlockEntity.ENERGY_CAPACITY);
        g.fill(x0 + ENERGY_X, y0 + ENERGY_Y, x0 + ENERGY_X + filled, y0 + ENERGY_Y + ENERGY_H, ENERGY);

        List<Section> sections = menu.def().spec.sections();
        int slot = 0;
        for (int i = 0; i < sections.size(); i++) {
            Section s = sections.get(i);
            int sy = y0 + MachineMenu.sectionY(i);
            g.fill(x0 + 6, sy - 1, x0 + imageWidth - 6, sy, BORDER_DARK);
            for (int k = 0; k < s.inputs(); k++) {
                slotBox(g, menu.slots.get(slot++), PANEL);
            }
            if (s.catalyst()) {
                slotBox(g, menu.slots.get(slot++), CATALYST);
            }
            for (int k = 0; k < s.outputs(); k++) {
                slotBox(g, menu.slots.get(slot++), PANEL);
            }
            // progress arrow
            int ax = x0 + MachineMenu.ARROW_X, ay = sy + 12 + 14;
            g.fill(ax, ay, ax + MachineMenu.ARROW_W, ay + 6, PANEL);
            int done = menu.progress(i) * MachineMenu.ARROW_W / 1000;
            g.fill(ax, ay, ax + done, ay + 6, ACCENT);
            for (int k = 0; k < 5; k++) { // arrow head
                g.fill(ax + MachineMenu.ARROW_W + k, ay - 2 + k, ax + MachineMenu.ARROW_W + k + 1, ay + 8 - k,
                        menu.progress(i) >= 1000 - 1 && menu.status(i) != Status.IDLE ? ACCENT : PANEL);
            }
        }
        // player inventory
        for (int i = slot; i < menu.slots.size(); i++) {
            slotBox(g, menu.slots.get(i), PANEL);
        }
    }

    @Override
    protected void renderLabels(GuiGraphics g, int mouseX, int mouseY) {
        g.drawString(font, title, titleLabelX, titleLabelY, TEXT, false);
        g.drawString(font, playerInventoryTitle, inventoryLabelX, inventoryLabelY, TEXT_DIM, false);
        List<Section> sections = menu.def().spec.sections();
        for (int i = 0; i < sections.size(); i++) {
            int sy = MachineMenu.sectionY(i) + 2;
            Status status = menu.status(i);
            Component state = Component.translatable("screen.fcw.machine.status." + status.name().toLowerCase(java.util.Locale.ROOT));
            int color = switch (status) {
                case WORKING -> ACCENT;
                case NO_POWER, OUTPUT_FULL -> 0xFFE06C5A;
                default -> TEXT_DIM;
            };
            if (sections.size() > 1) {
                g.drawString(font, Component.translatable("screen.fcw.machine.section." + sections.get(i).id()), 8, sy, TEXT, false);
            }
            g.drawString(font, state, imageWidth - 8 - font.width(state), sy, color, false);
            if (status == Status.WORKING || status == Status.NO_POWER || status == Status.OUTPUT_FULL) {
                String pct = (menu.progress(i) / 10) + "%";
                g.drawString(font, pct, MachineMenu.ARROW_X + (MachineMenu.ARROW_W - font.width(pct)) / 2, sy + 12, TEXT_DIM, false);
            }
        }
    }

    private void slotBox(GuiGraphics g, Slot slot, int fill) {
        int x = leftPos + slot.x, y = topPos + slot.y;
        g.fill(x - 1, y - 1, x + 17, y + 17, BORDER_DARK);
        g.fill(x, y, x + 16, y + 16, fill);
    }

    private static void panel(GuiGraphics g, int x, int y, int w, int h, int fill) {
        g.fill(x, y, x + w, y + h, BORDER_DARK);
        g.fill(x + 1, y + 1, x + w - 1, y + h - 1, BORDER_LIGHT);
        g.fill(x + 2, y + 2, x + w - 2, y + h - 2, fill);
    }
}
