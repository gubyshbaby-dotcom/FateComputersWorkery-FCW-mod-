package com.fatecomputers.fcw.client.gui;

import com.fatecomputers.fcw.block.entity.ComputerInventory;
import com.fatecomputers.fcw.client.ClientStatusCache;
import com.fatecomputers.fcw.menu.ComputerMenu;
import com.fatecomputers.fcw.network.ComputerActionPayload;
import com.fatecomputers.fcw.network.ComputerStatusPayload;
import com.fatecomputers.fcw.proto.ControlProto.VmState;
import com.fatecomputers.fcw.registry.ModBlocks;
import com.fatecomputers.fcw.registry.ModItems;
import net.minecraft.ChatFormatting;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.network.PacketDistributor;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.List;

/**
 * Computer GUI: hardware slots, power button, OS image list, reinstall with confirmation and the
 * machine status. Drawn with plain fills (no texture).
 */
public class ComputerScreen extends AbstractContainerScreen<ComputerMenu> {
    private static final int BG = 0xFF2B2D31;
    private static final int PANEL = 0xFF1E1F22;
    private static final int BORDER_LIGHT = 0xFF4A4D55;
    private static final int BORDER_DARK = 0xFF111214;
    private static final int TEXT = 0xFFE6E6E6;
    private static final int TEXT_DIM = 0xFF9A9CA3;
    private static final int ACCENT = 0xFF51A2DA;

    private static final int LIST_X = 92;
    private static final int LIST_Y = 28;
    private static final int LIST_W = 180;
    private static final int ROW_H = 11;
    private static final int ROWS = 5;
    private static final int STATUS_Y = 108;
    private static final int BUTTONS_Y = 88;

    private Button powerButton;
    private Button reinstallButton;
    private Button refreshButton;
    private Button confirmButton;
    private Button cancelButton;
    private int scroll;
    @Nullable
    private String confirmImage;

    public ComputerScreen(ComputerMenu menu, Inventory inventory, Component title) {
        super(menu, inventory, title);
        this.imageWidth = ComputerMenu.IMAGE_WIDTH;
        this.imageHeight = ComputerMenu.IMAGE_HEIGHT;
    }

    @Nullable
    private ComputerStatusPayload status() {
        return ClientStatusCache.get(menu.pos());
    }

    private void send(int action, String arg) {
        PacketDistributor.sendToServer(new ComputerActionPayload(menu.pos(), action, arg));
    }

    @Override
    protected void init() {
        super.init();
        titleLabelX = 8;
        titleLabelY = 6;
        inventoryLabelY = -1000; // no "Inventory" caption, the space is used by the status
        powerButton = addRenderableWidget(Button.builder(Component.translatable("gui.fcw.power_on"), b -> onPower())
                .bounds(leftPos + 8, topPos + 96, 78, 20).build());
        reinstallButton = addRenderableWidget(Button.builder(Component.translatable("gui.fcw.reinstall"), b -> onReinstall())
                .bounds(leftPos + LIST_X, topPos + BUTTONS_Y, 88, 16).build());
        refreshButton = addRenderableWidget(Button.builder(Component.translatable("gui.fcw.refresh"), b -> send(ComputerActionPayload.REFRESH_IMAGES, ""))
                .bounds(leftPos + LIST_X + 92, topPos + BUTTONS_Y, 88, 16).build());
        confirmButton = addRenderableWidget(Button.builder(Component.translatable("gui.fcw.reinstall.confirm"), b -> {
                    if (confirmImage != null) {
                        send(ComputerActionPayload.REINSTALL, confirmImage);
                    }
                    confirmImage = null;
                })
                .bounds(leftPos + LIST_X, topPos + BUTTONS_Y, 88, 16).build());
        cancelButton = addRenderableWidget(Button.builder(Component.translatable("gui.cancel"), b -> confirmImage = null)
                .bounds(leftPos + LIST_X + 92, topPos + BUTTONS_Y, 88, 16).build());
        updateWidgets();
    }

    private boolean isOn(ComputerStatusPayload s) {
        int st = s.vmState();
        return st == VmState.VM_STATE_STARTING_VALUE || st == VmState.VM_STATE_RUNNING_VALUE
                || st == VmState.VM_STATE_PAUSED_VALUE || st == VmState.VM_STATE_STOPPING_VALUE;
    }

    private void onPower() {
        ComputerStatusPayload s = status();
        if (s == null) {
            return;
        }
        if (s.vmState() == VmState.VM_STATE_STOPPING_VALUE) {
            send(ComputerActionPayload.FORCE_OFF, "");
        } else if (isOn(s)) {
            send(ComputerActionPayload.POWER_OFF, "");
        } else {
            send(ComputerActionPayload.POWER_ON, "");
        }
    }

    private void onReinstall() {
        ComputerStatusPayload s = status();
        if (s != null && !s.imageId().isEmpty()) {
            confirmImage = s.imageId();
        }
    }

    @Override
    protected void containerTick() {
        super.containerTick();
        updateWidgets();
    }

    private void updateWidgets() {
        ComputerStatusPayload s = status();
        boolean confirming = confirmImage != null;
        confirmButton.visible = confirming;
        cancelButton.visible = confirming;
        reinstallButton.visible = !confirming;
        refreshButton.visible = !confirming;
        if (s == null) {
            powerButton.active = false;
            reinstallButton.active = false;
            refreshButton.active = false;
            return;
        }
        boolean on = isOn(s);
        if (s.busy()) {
            powerButton.setMessage(Component.translatable("gui.fcw.busy"));
            powerButton.active = false;
        } else if (s.vmState() == VmState.VM_STATE_STOPPING_VALUE) {
            powerButton.setMessage(Component.translatable("gui.fcw.force_off"));
            powerButton.active = true;
        } else if (on) {
            powerButton.setMessage(Component.translatable("gui.fcw.power_off"));
            powerButton.active = true;
        } else {
            powerButton.setMessage(Component.translatable("gui.fcw.power_on"));
            powerButton.active = startProblem(s) == null;
        }
        ComputerStatusPayload.ImageEntry img = image(s, s.imageId());
        reinstallButton.active = !on && !s.busy() && img != null && img.installed() && s.daemonStatus() == 2;
        refreshButton.active = s.daemonStatus() == 2;
    }

    @Nullable
    private static ComputerStatusPayload.ImageEntry image(ComputerStatusPayload s, String id) {
        for (ComputerStatusPayload.ImageEntry e : s.images()) {
            if (e.id().equals(id)) {
                return e;
            }
        }
        return null;
    }

    private static String imageName(ComputerStatusPayload.ImageEntry e) {
        return e.name().isEmpty() ? e.id() : e.name();
    }

    /** Why the machine cannot start, computed from what the client knows (the server checks again). */
    @Nullable
    private Component startProblem(ComputerStatusPayload s) {
        if (!menu.getSlot(ComputerInventory.SLOT_MOTHERBOARD).hasItem()) {
            return Component.translatable("gui.fcw.error.no_motherboard");
        }
        if (!menu.getSlot(ComputerInventory.SLOT_CPU).hasItem()) {
            return Component.translatable("gui.fcw.error.no_cpu");
        }
        boolean ram = false;
        for (int i = 0; i < ComputerInventory.RAM_SLOTS; i++) {
            ram |= menu.getSlot(ComputerInventory.SLOT_RAM_FIRST + i).hasItem();
        }
        if (!ram) {
            return Component.translatable("gui.fcw.error.no_ram");
        }
        if (s.daemonStatus() != 2) {
            return Component.translatable("gui.fcw.error.daemon", s.daemonMessage());
        }
        if (s.imageId().isEmpty()) {
            return Component.translatable("gui.fcw.error.no_image");
        }
        ComputerStatusPayload.ImageEntry img = image(s, s.imageId());
        if (img == null) {
            return Component.translatable("gui.fcw.error.unknown_image", s.imageId());
        }
        if (!img.installed()) {
            return Component.translatable("gui.fcw.error.image_not_installed", imageName(img), img.id());
        }
        if (!s.installedImage().isEmpty() && !s.installedImage().equals(s.imageId())) {
            return Component.translatable("gui.fcw.error.needs_reinstall", s.installedImage(), s.imageId());
        }
        if (img.minMemoryMb() > 0 && s.memoryMb() < img.minMemoryMb()) {
            return Component.translatable("gui.fcw.error.not_enough_memory", imageName(img), img.minMemoryMb(), s.memoryMb());
        }
        return null;
    }

    // ------------------------------------------------------------------ drawing

    private static void bevel(GuiGraphics g, int x, int y, int w, int h, int fill, int light, int dark) {
        g.fill(x, y, x + w, y + h, dark);
        g.fill(x, y, x + w - 1, y + h - 1, light);
        g.fill(x + 1, y + 1, x + w - 1, y + h - 1, fill);
    }

    @Override
    protected void renderBg(GuiGraphics g, float partialTick, int mouseX, int mouseY) {
        int x = leftPos;
        int y = topPos;
        g.fill(x - 1, y - 1, x + imageWidth + 1, y + imageHeight + 1, 0xFF000000);
        bevel(g, x, y, imageWidth, imageHeight, BG, BORDER_LIGHT, BORDER_DARK);
        // OS list panel
        bevel(g, x + LIST_X - 1, y + LIST_Y - 1, LIST_W + 2, ROWS * ROW_H + 3, PANEL, BORDER_DARK, BORDER_LIGHT);
        // status panel
        g.fill(x + LIST_X - 1, y + STATUS_Y - 3, x + LIST_X + LIST_W + 1, y + STATUS_Y + 37, 0xFF25272B);
        // slots
        for (Slot slot : menu.slots) {
            bevel(g, x + slot.x - 1, y + slot.y - 1, 18, 18, 0xFF3A3C42, BORDER_DARK, BORDER_LIGHT);
        }
        // ghost icons in empty hardware slots
        for (int i = 0; i < ComputerInventory.SIZE; i++) {
            Slot slot = menu.getSlot(i);
            if (!slot.hasItem()) {
                g.renderFakeItem(ghost(i), x + slot.x, y + slot.y);
                overlay(g, x + slot.x, y + slot.y, 0xC83A3C42);
            }
        }
        if (menu.isLocked()) {
            for (int i = 0; i < ComputerInventory.SIZE; i++) {
                Slot slot = menu.getSlot(i);
                if (slot.hasItem()) {
                    overlay(g, x + slot.x, y + slot.y, 0x40FF4040);
                }
            }
        }
        renderDevices(g, x + 8, y + 122);
    }

    /** A translucent box above an item icon (items are drawn at z ~150). */
    private static void overlay(GuiGraphics g, int x, int y, int color) {
        g.pose().pushPose();
        g.pose().translate(0, 0, 200);
        g.fill(x, y, x + 16, y + 16, color);
        g.pose().popPose();
    }

    private static ItemStack ghost(int slot) {
        if (slot == ComputerInventory.SLOT_MOTHERBOARD) {
            return new ItemStack(ModItems.MOTHERBOARD.get());
        }
        if (slot == ComputerInventory.SLOT_CPU) {
            return new ItemStack(ModItems.CPU_LGA.get());
        }
        if (slot == ComputerInventory.SLOT_GPU) {
            return new ItemStack(ModItems.GRAPHICS_CARD.get());
        }
        return new ItemStack(ModItems.RAM_DDR.get());
    }

    private void renderDevices(GuiGraphics g, int x, int y) {
        ComputerStatusPayload s = status();
        ItemStack[] icons = {
                new ItemStack(ModBlocks.MONITOR.get()), new ItemStack(ModBlocks.KEYBOARD.get()),
                new ItemStack(ModBlocks.MOUSE.get()), new ItemStack(ModBlocks.SPEAKER_DESKTOP.get())};
        boolean[] present = s == null ? new boolean[4] : new boolean[]{s.monitors() > 0, s.keyboard(), s.mouse(), s.speakers()};
        for (int i = 0; i < 4; i++) {
            int ix = x + i * 19;
            g.renderFakeItem(icons[i], ix, y);
            g.pose().pushPose();
            g.pose().translate(0, 0, 200);
            if (!present[i]) {
                g.fill(ix, y, ix + 16, y + 16, 0xA02B2D31);
                g.drawString(font, "✘", ix + 10, y + 9, 0xFFE05050, false);
            } else {
                g.drawString(font, "✔", ix + 10, y + 9, 0xFF60D060, false);
            }
            g.pose().popPose();
        }
    }

    @Override
    protected void renderLabels(GuiGraphics g, int mouseX, int mouseY) {
        g.drawString(font, title, titleLabelX, titleLabelY, TEXT, false);
        g.drawString(font, Component.translatable("gui.fcw.os"), LIST_X, LIST_Y - 10, TEXT_DIM, false);
        ComputerStatusPayload s = status();
        if (s == null) {
            g.drawString(font, Component.translatable("gui.fcw.loading"), LIST_X + 4, LIST_Y + 3, TEXT_DIM, false);
            return;
        }
        // hardware summary
        g.drawString(font, Component.translatable("gui.fcw.hw.cpus", s.cpus()), 8, 64, TEXT_DIM, false);
        g.drawString(font, Component.translatable("gui.fcw.hw.memory", s.memoryMb()), 8, 74, TEXT_DIM, false);
        g.drawString(font, Component.translatable(s.gpu() ? "gui.fcw.hw.gpu" : "gui.fcw.hw.no_gpu"), 8, 84, TEXT_DIM, false);

        if (confirmImage != null) {
            renderConfirm(g, s);
        } else {
            renderImageList(g, s, mouseX - leftPos, mouseY - topPos);
        }
        renderStatus(g, s);
    }

    private void renderImageList(GuiGraphics g, ComputerStatusPayload s, int mx, int my) {
        List<ComputerStatusPayload.ImageEntry> images = s.images();
        if (images.isEmpty()) {
            Component msg = s.daemonStatus() != 2 ? Component.translatable("gui.fcw.images.no_daemon")
                    : !s.imagesError().getString().isEmpty() ? s.imagesError() : Component.translatable("gui.fcw.images.none");
            int yy = LIST_Y + 3;
            for (FormattedCharSequence line : font.split(msg, LIST_W - 8)) {
                if (yy > LIST_Y + ROWS * ROW_H - 8) {
                    break;
                }
                g.drawString(font, line, LIST_X + 4, yy, TEXT_DIM, false);
                yy += 10;
            }
            return;
        }
        scroll = Math.max(0, Math.min(scroll, Math.max(0, images.size() - ROWS)));
        for (int row = 0; row < ROWS && row + scroll < images.size(); row++) {
            ComputerStatusPayload.ImageEntry e = images.get(row + scroll);
            int ry = LIST_Y + 1 + row * ROW_H;
            boolean selected = e.id().equals(s.imageId());
            boolean hover = mx >= LIST_X && mx < LIST_X + LIST_W && my >= ry && my < ry + ROW_H;
            if (selected) {
                g.fill(LIST_X, ry, LIST_X + LIST_W, ry + ROW_H, 0xFF2F5F80);
            } else if (hover) {
                g.fill(LIST_X, ry, LIST_X + LIST_W, ry + ROW_H, 0xFF33363C);
            }
            String marker = e.id().equals(s.installedImage()) ? "● " : "  ";
            String label = marker + imageName(e) + (e.version().isEmpty() ? "" : " " + e.version());
            int color = e.installed() ? TEXT : TEXT_DIM;
            Component right = e.installed()
                    ? Component.translatable("gui.fcw.images.min_memory", e.minMemoryMb())
                    : Component.translatable("gui.fcw.images.not_downloaded");
            int rw = font.width(right);
            g.drawString(font, font.plainSubstrByWidth(label, LIST_W - rw - 10), LIST_X + 2, ry + 2, color, false);
            g.drawString(font, right, LIST_X + LIST_W - rw - 3, ry + 2, e.installed() ? TEXT_DIM : 0xFFC08040, false);
        }
        if (images.size() > ROWS) {
            int barH = ROWS * ROW_H;
            int thumb = Math.max(6, barH * ROWS / images.size());
            int ty = LIST_Y + 1 + (barH - thumb) * scroll / Math.max(1, images.size() - ROWS);
            g.fill(LIST_X + LIST_W - 2, ty, LIST_X + LIST_W, ty + thumb, BORDER_LIGHT);
        }
    }

    private static String uptime(long ms) {
        long s = ms / 1000;
        if (s >= 3600) {
            return String.format("%d:%02d:%02d", s / 3600, (s / 60) % 60, s % 60);
        }
        return String.format("%d:%02d", s / 60, s % 60);
    }

    private Component stateText(ComputerStatusPayload s) {
        if (s.busy()) {
            return Component.translatable("gui.fcw.state.preparing").withStyle(ChatFormatting.YELLOW);
        }
        return switch (s.vmState()) {
            case VmState.VM_STATE_STARTING_VALUE -> Component.translatable("gui.fcw.state.starting").withStyle(ChatFormatting.YELLOW);
            case VmState.VM_STATE_RUNNING_VALUE -> Component.translatable("gui.fcw.state.running").withStyle(ChatFormatting.GREEN);
            case VmState.VM_STATE_PAUSED_VALUE -> Component.translatable("gui.fcw.state.paused").withStyle(ChatFormatting.AQUA);
            case VmState.VM_STATE_STOPPING_VALUE -> Component.translatable("gui.fcw.state.stopping").withStyle(ChatFormatting.YELLOW);
            case VmState.VM_STATE_CRASHED_VALUE -> Component.translatable("gui.fcw.state.crashed").withStyle(ChatFormatting.RED);
            case VmState.VM_STATE_STOPPED_VALUE -> Component.translatable("gui.fcw.state.stopped").withStyle(ChatFormatting.GRAY);
            default -> Component.translatable("gui.fcw.state.unknown").withStyle(ChatFormatting.GRAY);
        };
    }

    private void renderStatus(GuiGraphics g, ComputerStatusPayload s) {
        List<FormattedCharSequence> lines = new ArrayList<>();
        MutableComponent state = Component.translatable("gui.fcw.state", stateText(s));
        if (s.uptimeMs() > 0) {
            state.append(Component.literal(" · " + uptime(s.uptimeMs())).withStyle(ChatFormatting.GRAY));
        }
        lines.add(state.getVisualOrderText());
        if (!s.agentInfo().isEmpty()) {
            lines.add(Component.translatable("gui.fcw.guest", s.agentInfo()).withStyle(ChatFormatting.GRAY).getVisualOrderText());
        }
        if (!s.controller().isEmpty()) {
            lines.add(Component.translatable("gui.fcw.controller", s.controller()).withStyle(ChatFormatting.GRAY).getVisualOrderText());
        }
        if (s.daemonStatus() != 2) {
            lines.addAll(font.split(Component.translatable("gui.fcw.error.daemon", s.daemonMessage()).withStyle(ChatFormatting.RED), LIST_W - 4));
        } else if (!s.error().getString().isEmpty()) {
            lines.addAll(font.split(s.error().copy().withStyle(ChatFormatting.RED), LIST_W - 4));
        } else if (!isOn(s) && !s.busy()) {
            Component problem = startProblem(s);
            if (problem != null) {
                lines.addAll(font.split(problem.copy().withStyle(ChatFormatting.YELLOW), LIST_W - 4));
            } else if (!s.gpu()) {
                lines.addAll(font.split(Component.translatable("gui.fcw.hint.text_only").withStyle(ChatFormatting.GRAY), LIST_W - 4));
            }
        } else if (isOn(s) && s.monitors() == 0) {
            lines.addAll(font.split(Component.translatable("gui.fcw.hint.no_monitor").withStyle(ChatFormatting.GRAY), LIST_W - 4));
        }
        int y = STATUS_Y;
        for (FormattedCharSequence line : lines) {
            if (y > STATUS_Y + 28) {
                break;
            }
            g.drawString(font, line, LIST_X + 1, y, TEXT, false);
            y += 10;
        }
    }

    private void renderConfirm(GuiGraphics g, ComputerStatusPayload s) {
        ComputerStatusPayload.ImageEntry img = image(s, confirmImage);
        String name = img == null ? confirmImage : imageName(img);
        int y = LIST_Y + 3;
        for (FormattedCharSequence line : font.split(Component.translatable("gui.fcw.reinstall.warning", name)
                .withStyle(ChatFormatting.GOLD), LIST_W - 8)) {
            if (y > LIST_Y + ROWS * ROW_H - 8) {
                break;
            }
            g.drawString(font, line, LIST_X + 4, y, TEXT, false);
            y += 10;
        }
    }

    @Override
    public void render(GuiGraphics g, int mouseX, int mouseY, float partialTick) {
        super.render(g, mouseX, mouseY, partialTick);
        renderTooltip(g, mouseX, mouseY);
        int mx = mouseX - leftPos;
        int my = mouseY - topPos;
        // tooltips for empty hardware slots
        if (menu.getCarried().isEmpty()) {
            for (int i = 0; i < ComputerInventory.SIZE; i++) {
                Slot slot = menu.getSlot(i);
                if (!slot.hasItem() && mx >= slot.x && mx < slot.x + 16 && my >= slot.y && my < slot.y + 16) {
                    List<Component> tip = new ArrayList<>();
                    tip.add(Component.translatable("gui.fcw.slot." + ComputerInventory.typeOf(i).name().toLowerCase()));
                    if (menu.isLocked()) {
                        tip.add(Component.translatable("gui.fcw.slot.locked").withStyle(ChatFormatting.RED));
                    }
                    g.renderComponentTooltip(font, tip, mouseX, mouseY);
                }
            }
        }
        // device icons
        if (my >= 122 && my < 138 && mx >= 8 && mx < 8 + 4 * 19) {
            int i = (mx - 8) / 19;
            String[] keys = {"monitor", "keyboard", "mouse", "speaker"};
            if (i >= 0 && i < 4) {
                g.renderComponentTooltip(font, List.of(
                        Component.translatable("device.fcw." + keys[i]),
                        Component.translatable("gui.fcw.device_hint." + keys[i]).withStyle(ChatFormatting.GRAY)), mouseX, mouseY);
            }
        }
        // why the power button is disabled
        if (powerButton.isHovered() && !powerButton.active) {
            ComputerStatusPayload s = status();
            Component problem = s == null ? null : startProblem(s);
            if (problem != null) {
                g.renderTooltip(font, font.split(problem, 200), mouseX, mouseY);
            }
        }
    }

    @Override
    public boolean mouseClicked(double mouseX, double mouseY, int button) {
        ComputerStatusPayload s = status();
        if (s != null && confirmImage == null && button == 0) {
            int mx = (int) mouseX - leftPos;
            int my = (int) mouseY - topPos;
            if (mx >= LIST_X && mx < LIST_X + LIST_W && my >= LIST_Y + 1 && my < LIST_Y + 1 + ROWS * ROW_H) {
                int idx = (my - LIST_Y - 1) / ROW_H + scroll;
                if (idx >= 0 && idx < s.images().size()) {
                    String id = s.images().get(idx).id();
                    if (!isOn(s) && !id.equals(s.imageId())) {
                        send(ComputerActionPayload.SELECT_IMAGE, id);
                    }
                    return true;
                }
            }
        }
        return super.mouseClicked(mouseX, mouseY, button);
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        int mx = (int) mouseX - leftPos;
        int my = (int) mouseY - topPos;
        if (mx >= LIST_X && mx < LIST_X + LIST_W && my >= LIST_Y && my < LIST_Y + ROWS * ROW_H + 2) {
            scroll -= (int) Math.signum(scrollY);
            return true;
        }
        return super.mouseScrolled(mouseX, mouseY, scrollX, scrollY);
    }

    @Override
    public void removed() {
        super.removed();
        ClientStatusCache.clear();
    }
}
