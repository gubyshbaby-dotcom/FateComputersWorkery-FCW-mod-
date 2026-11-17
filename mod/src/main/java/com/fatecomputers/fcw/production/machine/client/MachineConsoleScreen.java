package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.production.machine.console.MachineConsoleNet;
import com.fatecomputers.fcw.production.machine.shell.MachineShell;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.neoforged.neoforge.network.PacketDistributor;
import org.lwjgl.glfw.GLFW;

import java.util.ArrayList;
import java.util.List;

/**
 * The terminal of a machine: a text console of 80x25 cells in the mod's font, nothing around it but the
 * dimmed world. A line typed here goes to the server, which runs it in the machine's shell and sends the
 * text back. Monochrome with one accent colour; history with the arrows, scrollback with PageUp/PageDown
 * and the wheel, command completion with Tab, Esc or "exit" closes.
 */
public class MachineConsoleScreen extends Screen {
    public static final int COLS = 80, ROWS = 25;
    private static final int BACKGROUND = 0xFF0B0C0F, TEXT = 0xC9CDD6, ACCENT = 0xA88BFF;
    private static final int SCROLLBACK = 500, INPUT_LIMIT = 200;

    private record Row(String text, int accentStart, int accentEnd) {
    }

    private final BlockPos pos;
    private String prompt;
    private final List<Row> rows = new ArrayList<>();
    private final List<String> history = new ArrayList<>();
    private final StringBuilder input = new StringBuilder();
    private int historyIndex = -1;
    private int scroll;

    public MachineConsoleScreen(BlockPos pos, String name) {
        super(Component.literal("FCW machine console"));
        this.pos = pos;
        this.prompt = name + "> ";
    }

    /** A packet from the server: open the console, or more output for the open one. */
    public static void handle(MachineConsoleNet.Output output) {
        Minecraft mc = Minecraft.getInstance();
        MachineConsoleScreen screen;
        if (output.open()) {
            screen = new MachineConsoleScreen(output.pos(), output.prompt());
            mc.setScreen(screen);
        } else if (mc.screen instanceof MachineConsoleScreen open && open.pos.equals(output.pos())) {
            screen = open;
            screen.prompt = output.prompt() + "> "; // "name" may have renamed the machine
        } else {
            return;
        }
        for (int i = 0; i < output.lines().size(); i++) {
            int accent = i < output.accents().size() ? output.accents().get(i) : -1;
            screen.print(output.lines().get(i), accent < 0 ? -1 : accent >> 8, accent < 0 ? -1 : accent & 0xFF);
        }
        if (!output.open() && !output.lines().isEmpty()) {
            screen.print("", -1, -1);
        }
    }

    private void print(String text, int accentStart, int accentEnd) {
        // long lines wrap at the screen width; the accent stays on the first part
        do {
            String part = text.length() > COLS ? text.substring(0, COLS) : text;
            rows.add(new Row(part, accentStart, Math.min(accentEnd, COLS)));
            text = text.substring(part.length());
            accentStart = accentEnd = -1;
        } while (!text.isEmpty());
        while (rows.size() > SCROLLBACK) {
            rows.remove(0);
        }
        scroll = 0;
    }

    private void submit() {
        String line = input.toString().trim();
        print(prompt + input, -1, -1);
        input.setLength(0);
        historyIndex = -1;
        if (line.isEmpty()) {
            return;
        }
        if (history.isEmpty() || !history.get(history.size() - 1).equals(line)) {
            history.add(line);
        }
        if (line.equals("exit")) {
            onClose();
            return;
        }
        PacketDistributor.sendToServer(new MachineConsoleNet.Command(pos, line));
    }

    private void complete() {
        String typed = input.toString();
        if (typed.isEmpty() || typed.contains(" ")) {
            return;
        }
        List<String> matches = MachineShell.COMMANDS.stream().filter(c -> c.startsWith(typed)).toList();
        if (matches.size() == 1) {
            input.setLength(0);
            input.append(matches.get(0)).append(' ');
        } else if (matches.size() > 1) {
            print(String.join("  ", matches), -1, -1);
        }
    }

    @Override
    public boolean keyPressed(int key, int scanCode, int modifiers) {
        switch (key) {
            case GLFW.GLFW_KEY_ENTER, GLFW.GLFW_KEY_KP_ENTER -> submit();
            case GLFW.GLFW_KEY_BACKSPACE -> {
                if (!input.isEmpty()) {
                    input.setLength(input.length() - 1);
                }
            }
            case GLFW.GLFW_KEY_UP -> {
                if (!history.isEmpty()) {
                    historyIndex = historyIndex < 0 ? history.size() - 1 : Math.max(0, historyIndex - 1);
                    input.setLength(0);
                    input.append(history.get(historyIndex));
                }
            }
            case GLFW.GLFW_KEY_DOWN -> {
                if (historyIndex >= 0) {
                    historyIndex++;
                    input.setLength(0);
                    if (historyIndex >= history.size()) {
                        historyIndex = -1;
                    } else {
                        input.append(history.get(historyIndex));
                    }
                }
            }
            case GLFW.GLFW_KEY_PAGE_UP -> scrollBy(ROWS - 2);
            case GLFW.GLFW_KEY_PAGE_DOWN -> scrollBy(-(ROWS - 2));
            case GLFW.GLFW_KEY_TAB -> complete();
            default -> {
                return super.keyPressed(key, scanCode, modifiers); // Esc closes
            }
        }
        return true;
    }

    @Override
    public boolean charTyped(char c, int modifiers) {
        if (c >= ' ' && c != 127 && input.length() < INPUT_LIMIT) {
            input.append(c);
            scroll = 0;
        }
        return true;
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        scrollBy((int) Math.signum(scrollY) * 3);
        return true;
    }

    private void scrollBy(int lines) {
        scroll = Math.max(0, Math.min(Math.max(0, rows.size() - (ROWS - 1)), scroll + lines));
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public void renderBackground(GuiGraphics g, int mouseX, int mouseY, float partialTick) {
        g.fill(0, 0, width, height, 0xB0000000);
    }

    @Override
    public void render(GuiGraphics g, int mouseX, int mouseY, float partialTick) {
        super.render(g, mouseX, mouseY, partialTick);
        Minecraft mc = Minecraft.getInstance();
        ConsoleFont font = ConsoleFont.get();
        // whole screen pixels per font pixel, as large as fits
        int pw = mc.getWindow().getWidth(), ph = mc.getWindow().getHeight();
        int k = Math.max(1, Math.min(pw / (COLS * ConsoleFont.CELL_W + 16), ph / (ROWS * ConsoleFont.CELL_H + 16)));
        float s = (float) (k / mc.getWindow().getGuiScale());
        int w = COLS * ConsoleFont.CELL_W, h = ROWS * ConsoleFont.CELL_H;
        g.pose().pushPose();
        g.pose().translate((width - w * s) / 2f, (height - h * s) / 2f, 0);
        g.pose().scale(s, s, 1);
        g.fill(-8, -8, w + 8, h + 8, BACKGROUND);

        int first = Math.max(0, rows.size() - (ROWS - 1) - scroll);
        int y = 0;
        for (int i = first; i < rows.size() && y < ROWS - 1; i++, y++) {
            Row row = rows.get(i);
            drawText(g, font, row.text, 0, y, TEXT, 0, row.text.length());
            if (row.accentStart >= 0) {
                drawText(g, font, row.text, 0, y, ACCENT, row.accentStart, Math.min(row.accentEnd, row.text.length()));
            }
        }
        if (scroll == 0) {
            String line = prompt + input;
            // a long input shows its tail
            int from = Math.max(0, line.length() - (COLS - 1));
            drawText(g, font, line.substring(from), 0, y, TEXT, 0, line.length() - from);
            if (System.currentTimeMillis() / 500 % 2 == 0) {
                drawCell(g, font, 0x2588, line.length() - from, y, TEXT);
            }
        }
        g.setColor(1, 1, 1, 1);
        g.pose().popPose();
    }

    private static void drawText(GuiGraphics g, ConsoleFont font, String text, int col, int row, int color, int from, int to) {
        for (int i = Math.max(0, from); i < to && i < text.length() && col + i < COLS; i++) {
            char c = text.charAt(i);
            if (c != ' ') {
                drawCell(g, font, c, col + i, row, color);
            }
        }
    }

    private static void drawCell(GuiGraphics g, ConsoleFont font, int codePoint, int col, int row, int color) {
        int glyph = font.glyph(codePoint);
        g.setColor((color >> 16 & 0xFF) / 255f, (color >> 8 & 0xFF) / 255f, (color & 0xFF) / 255f, 1);
        g.blit(ConsoleFont.TEXTURE, col * ConsoleFont.CELL_W, row * ConsoleFont.CELL_H, font.texX(glyph), font.texY(glyph),
                ConsoleFont.CELL_W, ConsoleFont.CELL_H, ConsoleFont.TEX_W, ConsoleFont.TEX_H);
    }
}
