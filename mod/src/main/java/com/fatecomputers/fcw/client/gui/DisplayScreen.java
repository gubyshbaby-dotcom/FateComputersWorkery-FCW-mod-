package com.fatecomputers.fcw.client.gui;

import com.fatecomputers.fcw.client.FcwKeys;
import com.fatecomputers.fcw.client.nativelib.FcwNative;
import com.fatecomputers.fcw.client.nativelib.NativeLoader;
import com.fatecomputers.fcw.client.view.ScreenViewManager;
import com.fatecomputers.fcw.client.view.ViewTexture;
import com.fatecomputers.fcw.network.ScreenAccessPayload;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import org.lwjgl.glfw.GLFW;

import javax.annotation.Nullable;

/**
 * Fullscreen view of a computer with input capture. Keys go to the guest as GLFW key + scancode
 * (the native library maps them to evdev), typed text separately, the mouse as absolute
 * coordinates of the guest screen. Right Ctrl (configurable) leaves.
 */
public class DisplayScreen extends Screen {
    private static final int BAR = 12;

    private final String vmId;
    private final BlockPos devicePos;
    private int permissions;
    private long lastRetake;
    private int buttons;
    private int lastGuestX = -1;
    private int lastGuestY = -1;
    private char pendingHighSurrogate;
    private boolean cursorHidden;
    // image rectangle in GUI coordinates
    private float imgX;
    private float imgY;
    private float imgW;
    private float imgH;

    public DisplayScreen(String vmId, int permissions, BlockPos devicePos) {
        super(Component.translatable("screen.fcw.display"));
        this.vmId = vmId;
        this.permissions = permissions;
        this.devicePos = devicePos.immutable();
    }

    public String vmId() {
        return vmId;
    }

    /** A new control token was accepted while the display is open. */
    public void grant(int perms) {
        permissions |= perms;
    }

    /** False while another player holds the input of this machine (InputControl). */
    private boolean inControl() {
        ScreenViewManager.Link l = link();
        return l == null || l.inputActive();
    }

    @Nullable
    private String otherController() {
        ScreenViewManager.Link l = link();
        if (l == null || l.inputActive()) {
            return null;
        }
        return l.controllerName().isEmpty() ? "?" : l.controllerName();
    }

    private boolean keyboard() {
        return (permissions & ScreenAccessPayload.PERM_KEYBOARD) != 0 && inControl();
    }

    private boolean mouse() {
        return (permissions & ScreenAccessPayload.PERM_MOUSE) != 0 && inControl();
    }

    private boolean mayControl() {
        return (permissions & (ScreenAccessPayload.PERM_KEYBOARD | ScreenAccessPayload.PERM_MOUSE)) != 0;
    }

    /** Ask the server for a fresh control token: the daemon gives the input to the last token holder. */
    private void retakeControl() {
        long now = System.currentTimeMillis();
        if (now - lastRetake > 1000) {
            lastRetake = now;
            ScreenViewManager.get().requestControl(devicePos);
        }
    }

    @Nullable
    private ScreenViewManager.Link link() {
        return NativeLoader.isLoaded() ? ScreenViewManager.get().link(vmId) : null;
    }

    private long session() {
        ScreenViewManager.Link l = link();
        return l == null || l.state() != ScreenViewManager.LinkState.ATTACHED ? 0 : l.sessionHandle();
    }

    @Override
    protected void init() {
        super.init();
        setCursorHidden(mouse() && NativeLoader.isLoaded());
    }

    private void setCursorHidden(boolean hidden) {
        if (cursorHidden == hidden || minecraft == null) {
            return;
        }
        cursorHidden = hidden;
        GLFW.glfwSetInputMode(minecraft.getWindow().getWindow(), GLFW.GLFW_CURSOR, hidden ? GLFW.GLFW_CURSOR_HIDDEN : GLFW.GLFW_CURSOR_NORMAL);
    }

    @Override
    public void tick() {
        ScreenViewManager.Link l = link();
        if (l != null) {
            permissions |= l.permissions() & ~ScreenAccessPayload.PERM_VIEW;
        }
    }

    @Override
    public void renderBackground(GuiGraphics g, int mouseX, int mouseY, float partialTick) {
        // no blur/dirt: the guest screen covers everything
    }

    @Override
    public void render(GuiGraphics g, int mouseX, int mouseY, float partialTick) {
        g.fill(0, 0, width, height, 0xFF000000);
        ScreenViewManager.Link l = link();
        ViewTexture tex = l == null ? null : l.texture();
        Component status = null;
        if (!NativeLoader.isLoaded()) {
            status = Component.translatable("screen.fcw.no_native", NativeLoader.problem());
        } else if (l == null) {
            status = Component.translatable("screen.fcw.connecting");
        } else if (l.state() == ScreenViewManager.LinkState.FAILED) {
            status = Component.translatable("screen.fcw.failed", l.error());
        } else if (tex == null || !tex.hasFrame()) {
            status = Component.translatable("screen.fcw.waiting");
        }

        int areaY = BAR;
        int areaH = height - BAR;
        if (tex != null && tex.hasFrame()) {
            float tw = tex.width();
            float th = tex.height();
            float scale = Math.min(width / tw, areaH / th);
            imgW = tw * scale;
            imgH = th * scale;
            imgX = (width - imgW) / 2f;
            imgY = areaY + (areaH - imgH) / 2f;
            g.blit(tex.id(), Math.round(imgX), Math.round(imgY), Math.round(imgW), Math.round(imgH),
                    0, 0, tex.width(), tex.height(), tex.width(), tex.height());
        } else {
            imgW = 0;
            imgH = 0;
        }

        // Top bar: how to leave, what is captured.
        g.fill(0, 0, width, BAR, 0xC0202020);
        Component keyName = FcwKeys.RELEASE_INPUT.getTranslatedKeyMessage();
        Component hint = Component.translatable("screen.fcw.release_hint", keyName);
        g.drawString(font, hint, 4, 2, 0xFFFFFF55, false);
        String other = otherController();
        Component caps = other != null
                ? Component.translatable("screen.fcw.cap.controlled_by", other)
                : Component.translatable(keyboard() ? "screen.fcw.cap.keyboard_on" : "screen.fcw.cap.keyboard_off")
                .append("  ").append(Component.translatable(mouse() ? "screen.fcw.cap.mouse_on" : "screen.fcw.cap.mouse_off"));
        g.drawString(font, caps, width - font.width(caps) - 4, 2, other != null ? 0xFFFF8855 : 0xFFBBBBBB, false);
        if (other != null && status == null) {
            Component msg = Component.translatable("screen.fcw.controlled_by", other);
            Component retake = Component.translatable(mayControl() ? "screen.fcw.click_to_retake" : "screen.fcw.view_only");
            int bw = Math.max(font.width(msg), font.width(retake)) + 16;
            int by = height - 40;
            g.fill((width - bw) / 2, by - 4, (width + bw) / 2, by + 22, 0xC0000000);
            g.drawCenteredString(font, msg, width / 2, by, 0xFFFF8855);
            g.drawCenteredString(font, retake, width / 2, by + 11, 0xFFDDDDDD);
        }

        if (status != null) {
            int w = font.width(status);
            g.drawString(font, status, (width - w) / 2, height / 2 - 4, 0xFFFFFFFF, false);
        }
        setCursorHidden(mouse() && imgW > 0 && insideImage(mouseX, mouseY));
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public boolean shouldCloseOnEsc() {
        return false; // Escape belongs to the guest
    }

    @Override
    public void removed() {
        super.removed();
        setCursorHidden(false);
        if (NativeLoader.isLoaded()) {
            long s = session();
            if (s != 0) {
                FcwNative.releaseAll(s, vmId);
            }
            ScreenViewManager.get().displayClosed(vmId);
        }
    }

    // ------------------------------------------------------------------ keyboard

    @Override
    public boolean keyPressed(int keyCode, int scanCode, int modifiers) {
        if (FcwKeys.RELEASE_INPUT.matches(keyCode, scanCode)) {
            onClose();
            return true;
        }
        long s = session();
        if (keyboard() && s != 0) {
            FcwNative.sendKey(s, vmId, keyCode, scanCode, true);
        }
        return true;
    }

    @Override
    public boolean keyReleased(int keyCode, int scanCode, int modifiers) {
        long s = session();
        if (keyboard() && s != 0) {
            FcwNative.sendKey(s, vmId, keyCode, scanCode, false);
        }
        return true;
    }

    @Override
    public boolean charTyped(char c, int modifiers) {
        long s = session();
        if (!keyboard() || s == 0) {
            return true;
        }
        String text;
        if (Character.isHighSurrogate(c)) {
            pendingHighSurrogate = c;
            return true;
        } else if (Character.isLowSurrogate(c) && pendingHighSurrogate != 0) {
            text = new String(new char[]{pendingHighSurrogate, c});
            pendingHighSurrogate = 0;
        } else {
            text = String.valueOf(c);
        }
        FcwNative.sendText(s, vmId, text);
        return true;
    }

    // ------------------------------------------------------------------ mouse

    private boolean insideImage(double x, double y) {
        return imgW > 0 && x >= imgX && y >= imgY && x < imgX + imgW && y < imgY + imgH;
    }

    private int guestX(double x) {
        ScreenViewManager.Link l = link();
        int gw = l == null ? 0 : l.texture().width();
        return (int) Math.max(0, Math.min(gw - 1, (x - imgX) / imgW * gw));
    }

    private int guestY(double y) {
        ScreenViewManager.Link l = link();
        int gh = l == null ? 0 : l.texture().height();
        return (int) Math.max(0, Math.min(gh - 1, (y - imgY) / imgH * gh));
    }

    @Override
    public void mouseMoved(double x, double y) {
        long s = session();
        if (!mouse() || s == 0 || !insideImage(x, y)) {
            return;
        }
        int gx = guestX(x);
        int gy = guestY(y);
        if (gx != lastGuestX || gy != lastGuestY) {
            lastGuestX = gx;
            lastGuestY = gy;
            FcwNative.sendPointer(s, vmId, gx, gy);
        }
    }

    private static int buttonBit(int glfwButton) {
        return switch (glfwButton) {
            case GLFW.GLFW_MOUSE_BUTTON_LEFT -> 1;
            case GLFW.GLFW_MOUSE_BUTTON_RIGHT -> 2;
            case GLFW.GLFW_MOUSE_BUTTON_MIDDLE -> 4;
            case GLFW.GLFW_MOUSE_BUTTON_4 -> 8;
            case GLFW.GLFW_MOUSE_BUTTON_5 -> 16;
            default -> 0;
        };
    }

    @Override
    public boolean mouseClicked(double x, double y, int button) {
        if (otherController() != null && mayControl()) {
            retakeControl();
            return true;
        }
        long s = session();
        if (mouse() && s != 0 && insideImage(x, y)) {
            buttons |= buttonBit(button);
            FcwNative.sendButtons(s, vmId, buttons, guestX(x), guestY(y));
        }
        return true;
    }

    @Override
    public boolean mouseReleased(double x, double y, int button) {
        long s = session();
        int bit = buttonBit(button);
        if (mouse() && s != 0 && (buttons & bit) != 0) {
            buttons &= ~bit;
            FcwNative.sendButtons(s, vmId, buttons, guestX(x), guestY(y));
        }
        return true;
    }

    @Override
    public boolean mouseDragged(double x, double y, int button, double dragX, double dragY) {
        mouseMoved(x, y);
        return true;
    }

    @Override
    public boolean mouseScrolled(double x, double y, double scrollX, double scrollY) {
        long s = session();
        if (mouse() && s != 0 && insideImage(x, y)) {
            int dx = (int) Math.signum(scrollX);
            int dy = (int) Math.signum(scrollY); // positive = away from the user (wheel up), as GLFW
            if (dx != 0 || dy != 0) {
                FcwNative.sendWheel(s, vmId, dx, dy, guestX(x), guestY(y));
            }
        }
        return true;
    }


}
