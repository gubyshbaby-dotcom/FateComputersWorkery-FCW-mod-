package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.FcwMod;
import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.ResourceLocation;

import java.io.InputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The mod's console font: Terminus 8x16 from the same PSF file the native client draws tty0 with
 * (native/crates/fcw-client/assets, copied into the jar by the build), with the same corrections as
 * font.rs there: lower-case Cyrillic "и" and "й" get their diagonal, and the block elements missing in
 * the font are drawn by hand. The glyphs live in one texture; the terminal console and the readouts on
 * the machines draw their cells from it.
 */
public final class ConsoleFont {
    public static final int CELL_W = 8, CELL_H = 16;
    public static final ResourceLocation TEXTURE = ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "dynamic/console_font");
    private static final int COLUMNS = 32;
    public static final int TEX_W = COLUMNS * CELL_W, TEX_H = 512;
    private static final String RESOURCE = "/assets/fcw/font/terminus-uni2-8x16.psf";

    private static ConsoleFont instance;

    private final List<byte[]> glyphs = new ArrayList<>();
    private final Map<Integer, Integer> map = new HashMap<>();
    private int fallback, space;

    private ConsoleFont() {
        try (InputStream in = ConsoleFont.class.getResourceAsStream(RESOURCE)) {
            if (in != null) {
                parsePsf1(in.readAllBytes());
            }
        } catch (Exception e) {
            FcwMod.LOGGER.warn("Cannot read the console font {}", RESOURCE, e);
        }
        if (glyphs.isEmpty()) {
            glyphs.add(new byte[CELL_H]); // no font: blank cells rather than a crash
        }
        NativeImage image = new NativeImage(TEX_W, TEX_H, true);
        for (int g = 0; g < glyphs.size() && g < COLUMNS * (TEX_H / CELL_H); g++) {
            int ox = (g % COLUMNS) * CELL_W, oy = (g / COLUMNS) * CELL_H;
            for (int y = 0; y < CELL_H; y++) {
                int row = glyphs.get(g)[y] & 0xFF;
                for (int x = 0; x < CELL_W; x++) {
                    image.setPixelRGBA(ox + x, oy + y, (row & (0x80 >> x)) != 0 ? 0xFFFFFFFF : 0x00FFFFFF);
                }
            }
        }
        Minecraft.getInstance().getTextureManager().register(TEXTURE, new DynamicTexture(image));
    }

    /** Call on the render thread: the first call uploads the texture. */
    public static ConsoleFont get() {
        if (instance == null) {
            instance = new ConsoleFont();
        }
        return instance;
    }

    private void parsePsf1(byte[] d) {
        if (d.length < 4 || (d[0] & 0xFF) != 0x36 || (d[1] & 0xFF) != 0x04 || (d[3] & 0xFF) != CELL_H) {
            return;
        }
        int mode = d[2] & 0xFF;
        int n = (mode & 1) != 0 ? 512 : 256;
        if (d.length < 4 + n * CELL_H) {
            return;
        }
        for (int g = 0; g < n; g++) {
            byte[] rows = new byte[CELL_H];
            System.arraycopy(d, 4 + g * CELL_H, rows, 0, CELL_H);
            glyphs.add(rows);
        }
        if ((mode & 2) != 0) {
            int g = 0;
            boolean inSequence = false;
            for (int i = 4 + n * CELL_H; i + 1 < d.length && g < n; i += 2) {
                int v = (d[i] & 0xFF) | ((d[i + 1] & 0xFF) << 8);
                if (v == 0xFFFF) {
                    g++;
                    inSequence = false;
                } else if (v == 0xFFFE) {
                    inSequence = true;
                } else if (!inSequence) {
                    map.putIfAbsent(v, g);
                }
            }
        } else {
            for (int i = 0; i < n; i++) {
                map.put(i, i);
            }
        }
        // as in font.rs: this Terminus draws "и" and "й" without the diagonal, like a latin "u"
        byte[] body = {0x42, 0x46, 0x4a, 0x52, 0x62, 0x42, 0x42};
        byte[] i = new byte[CELL_H];
        System.arraycopy(body, 0, i, 5, body.length);
        byte[] shortI = i.clone();
        shortI[2] = 0x24;
        shortI[3] = 0x18;
        replace(0x438, i);
        replace(0x439, shortI);
        // block elements the font lacks
        byte[] full = new byte[CELL_H];
        byte[] upper = new byte[CELL_H], lower = new byte[CELL_H];
        for (int r = 0; r < CELL_H; r++) {
            full[r] = (byte) 0xFF;
            upper[r] = (byte) (r < 8 ? 0xFF : 0);
            lower[r] = (byte) (r >= 8 ? 0xFF : 0);
        }
        synth(0x2588, full);
        synth(0x2580, upper);
        synth(0x2584, lower);
        fallback = map.getOrDefault(0xFFFD, map.getOrDefault((int) '?', 0));
        space = map.getOrDefault((int) ' ', 0);
    }

    private void replace(int codePoint, byte[] rows) {
        Integer g = map.get(codePoint);
        if (g != null) {
            glyphs.set(g, rows);
        }
    }

    private void synth(int codePoint, byte[] rows) {
        if (!map.containsKey(codePoint)) {
            map.put(codePoint, glyphs.size());
            glyphs.add(rows);
        }
    }

    public boolean has(int codePoint) {
        return map.containsKey(codePoint);
    }

    /** Index of the glyph in the texture. */
    public int glyph(int codePoint) {
        return codePoint == 0 || codePoint == ' ' ? space : map.getOrDefault(codePoint, fallback);
    }

    /** Texel position of a glyph in the texture. */
    public int texX(int glyph) {
        return (glyph % COLUMNS) * CELL_W;
    }

    public int texY(int glyph) {
        return (glyph / COLUMNS) * CELL_H;
    }
}
