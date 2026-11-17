package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.SectionState;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.Status;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineRecipeNames;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Axis;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.client.resources.language.I18n;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemDisplayContext;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import org.joml.Matrix4f;
import org.joml.Vector3f;

import java.util.List;
import java.util.Locale;

/**
 * The state of a machine as seen in the world (docs/design/MACHINE_UI.md, section 4), drawn by code until
 * the artist's models arrive: a two-line readout of 16 characters on the terminal in the mod's console
 * font, a signal lamp above it (blinking red on a fault), a lamp at the power inlet (white when there is
 * power), and the first stack of what lies in the hand-loading zones that have no prop of their own.
 * Called by {@link MachineRenderer} with the pose stack in structure space.
 */
final class MachineStatusRenderer {
    private static final int LIGHT = 0xF000F0;
    private static final int COLS = 16;
    /** Size of the readout in blocks: 16 cells of 8x16 font pixels, two lines. */
    private static final float WIDTH = 0.875f, CELL_W = WIDTH / COLS, CELL_H = CELL_W * 2, HEIGHT = CELL_H * 2;
    private static final int GLASS = 0x0B0C0F, TEXT = 0xC9CDD6, LAMP_OFF = 0x2A2B30, LAMP_RED = 0xFF3B30, LAMP_WHITE = 0xF4F6FA;

    private MachineStatusRenderer() {
    }

    static void render(MachineControllerBlockEntity be, PoseStack pose, MultiBufferSource buffers) {
        Level level = be.getLevel();
        MachineDef def = be.def();
        if (level == null) {
            return;
        }
        items(be, pose, buffers, level);

        ConsoleFont font = ConsoleFont.get();
        VertexConsumer vc = buffers.getBuffer(RenderType.text(ConsoleFont.TEXTURE));
        Matrix4f m = pose.last().pose();
        int block = font.glyph(0x2588);

        // readout on the terminal
        MachineDef.Port term = def.terminal;
        Vector3f n = normal(term.face());
        Vector3f r = normal(term.face().getAxis() == Direction.Axis.Y ? Direction.EAST : term.face().getCounterClockWise());
        Vector3f up = term.face().getAxis() == Direction.Axis.Y ? new Vector3f(0, 0, -1) : new Vector3f(0, 1, 0);
        Vector3f c = point(term.pos()).add(new Vector3f(n).mul(0.03f));
        // glass plate, a little behind the text
        Vector3f plate = new Vector3f(c).sub(new Vector3f(n).mul(0.01f)).add(new Vector3f(r).mul(-WIDTH / 2 - 0.03f)).add(new Vector3f(up).mul(HEIGHT / 2 + 0.03f));
        quad(vc, m, font, block, plate, r, up, WIDTH + 0.06f, HEIGHT + 0.06f, GLASS);
        String[] lines = readout(be);
        for (int l = 0; l < 2; l++) {
            String text = lines[l];
            for (int i = 0; i < text.length() && i < COLS; i++) {
                char ch = text.charAt(i);
                if (ch == ' ') {
                    continue;
                }
                Vector3f p = new Vector3f(c).add(new Vector3f(r).mul(-WIDTH / 2 + i * CELL_W)).add(new Vector3f(up).mul(HEIGHT / 2 - l * CELL_H));
                quad(vc, m, font, font.glyph(ch), p, r, up, CELL_W, CELL_H, TEXT);
            }
        }

        // signal lamp above the readout: dark, or blinking red at 1 Hz on a fault
        boolean fault = be.isEnabled() && be.faultStatus() != null;
        boolean blink = level.getGameTime() / 10 % 2 == 0;
        Vector3f lamp = new Vector3f(c).add(new Vector3f(up).mul(HEIGHT / 2 + 0.11f));
        cube(vc, m, font, block, lamp, n, r, up, 0.11f, fault && blink ? LAMP_RED : LAMP_OFF);

        // lamp at the power inlet
        MachineDef.Port power = def.port("power");
        if (power != null) {
            Vector3f pn = normal(power.face());
            Vector3f pr = normal(power.face().getAxis() == Direction.Axis.Y ? Direction.EAST : power.face().getCounterClockWise());
            Vector3f pu = power.face().getAxis() == Direction.Axis.Y ? new Vector3f(0, 0, -1) : new Vector3f(0, 1, 0);
            Vector3f at = point(power.pos()).add(new Vector3f(pn).mul(0.04f)).add(new Vector3f(pu).mul(0.3f));
            cube(vc, m, font, block, at, pn, pr, pu, 0.09f, be.isPowered() ? LAMP_WHITE : LAMP_OFF);
        }
    }

    /** Two lines of at most 16 characters: the state, and a detail. */
    static String[] readout(MachineControllerBlockEntity be) {
        String state, detail = "";
        Status fault = be.faultStatus();
        if (!be.isEnabled()) {
            state = I18n.get("readout.fcw.off");
        } else if (fault != null) {
            state = I18n.get("readout.fcw.fault");
            detail = I18n.get("readout.fcw.reason." + fault.name().toLowerCase(Locale.ROOT));
        } else {
            SectionState working = null;
            int index = 0;
            for (int i = 0; i < be.sectionCount(); i++) {
                if (be.section(i).status() == Status.WORKING && working == null) {
                    working = be.section(i);
                    index = i;
                }
            }
            if (working != null && working.recipeId() != null) {
                state = I18n.get("readout.fcw.run");
                String pct = (int) (be.clientProgress(index, 0) * 100 / Math.max(1, working.total())) + "%";
                String recipe = MachineRecipeNames.shortName(working.recipeId());
                int room = COLS - pct.length() - 1;
                recipe = recipe.length() > room ? recipe.substring(0, room) : recipe;
                detail = recipe + " ".repeat(COLS - recipe.length() - pct.length()) + pct;
            } else {
                state = I18n.get("readout.fcw.idle");
                boolean loaded = false;
                for (int i = 0; i < be.sectionCount(); i++) {
                    loaded |= be.present(i, com.fatecomputers.fcw.production.machine.MachineSpec.When.INPUT_PRESENT);
                }
                detail = loaded ? "" : I18n.get("readout.fcw.reason.no_input");
            }
        }
        return new String[] {state.toUpperCase(Locale.ROOT), detail.toUpperCase(Locale.ROOT)};
    }

    private static Vector3f normal(Direction d) {
        return new Vector3f(d.getStepX(), d.getStepY(), d.getStepZ());
    }

    private static Vector3f point(double[] pos) {
        return new Vector3f((float) (pos[0] / 16), (float) (pos[1] / 16), (float) (pos[2] / 16));
    }

    /** A glyph cell: top-left corner p, extending along right and down. */
    private static void quad(VertexConsumer vc, Matrix4f m, ConsoleFont font, int glyph, Vector3f p, Vector3f right, Vector3f up,
                             float w, float h, int color) {
        float u0 = (font.texX(glyph) + 0.01f) / ConsoleFont.TEX_W, v0 = (font.texY(glyph) + 0.01f) / ConsoleFont.TEX_H;
        float u1 = (font.texX(glyph) + ConsoleFont.CELL_W - 0.01f) / ConsoleFont.TEX_W;
        float v1 = (font.texY(glyph) + ConsoleFont.CELL_H - 0.01f) / ConsoleFont.TEX_H;
        Vector3f tr = new Vector3f(p).add(new Vector3f(right).mul(w));
        Vector3f bl = new Vector3f(p).sub(new Vector3f(up).mul(h));
        Vector3f br = new Vector3f(tr).sub(new Vector3f(up).mul(h));
        int r = color >> 16 & 0xFF, g = color >> 8 & 0xFF, b = color & 0xFF;
        // both windings: the text render type may or may not cull
        vertex(vc, m, bl, r, g, b, u0, v1);
        vertex(vc, m, br, r, g, b, u1, v1);
        vertex(vc, m, tr, r, g, b, u1, v0);
        vertex(vc, m, p, r, g, b, u0, v0);
        vertex(vc, m, p, r, g, b, u0, v0);
        vertex(vc, m, tr, r, g, b, u1, v0);
        vertex(vc, m, br, r, g, b, u1, v1);
        vertex(vc, m, bl, r, g, b, u0, v1);
    }

    private static void vertex(VertexConsumer vc, Matrix4f m, Vector3f p, int r, int g, int b, float u, float v) {
        vc.addVertex(m, p.x, p.y, p.z).setColor(r, g, b, 255).setUv(u, v).setLight(LIGHT);
    }

    /** A small box sitting on a surface: centre c, its back on the surface. */
    private static void cube(VertexConsumer vc, Matrix4f m, ConsoleFont font, int block, Vector3f c, Vector3f n, Vector3f right, Vector3f up,
                             float size, int color) {
        float h = size / 2;
        Vector3f front = new Vector3f(c).add(new Vector3f(n).mul(size));
        // front
        quad(vc, m, font, block, new Vector3f(front).add(new Vector3f(right).mul(-h)).add(new Vector3f(up).mul(h)), right, up, size, size, color);
        int side = shade(color, 0.75f);
        // left and right
        quad(vc, m, font, block, new Vector3f(c).add(new Vector3f(right).mul(-h)).add(new Vector3f(up).mul(h)), n, up, size, size, side);
        quad(vc, m, font, block, new Vector3f(c).add(new Vector3f(right).mul(h)).add(new Vector3f(up).mul(h)), n, up, size, size, side);
        // top and bottom
        quad(vc, m, font, block, new Vector3f(front).add(new Vector3f(right).mul(-h)).add(new Vector3f(up).mul(h)), right, n, size, size, shade(color, 0.9f));
        quad(vc, m, font, block, new Vector3f(front).add(new Vector3f(right).mul(-h)).add(new Vector3f(up).mul(-h)), right, n, size, size, shade(color, 0.6f));
    }

    private static int shade(int color, float f) {
        return ((int) ((color >> 16 & 0xFF) * f) << 16) | ((int) ((color >> 8 & 0xFF) * f) << 8) | (int) ((color & 0xFF) * f);
    }

    /** The first stack of a zone, lying at its port: for zones the models have no prop for yet. */
    private static void items(MachineControllerBlockEntity be, PoseStack pose, MultiBufferSource buffers, Level level) {
        MachineDef def = be.def();
        for (Zone zone : def.spec.zones()) {
            MachineDef.Port port = zone.showsItem() && zone.port() != null ? def.port(zone.port()) : null;
            if (port == null) {
                continue;
            }
            List<ItemStack> stacks = be.zoneContents(zone);
            if (stacks.isEmpty()) {
                continue;
            }
            Direction face = port.face();
            pose.pushPose();
            pose.translate(port.pos()[0] / 16 + face.getStepX() * 0.12, port.pos()[1] / 16 + face.getStepY() * 0.12 + 0.05,
                    port.pos()[2] / 16 + face.getStepZ() * 0.12);
            if (face.getAxis() == Direction.Axis.Y) {
                pose.mulPose(Axis.XP.rotationDegrees(90));
            } else {
                pose.mulPose(Axis.YP.rotationDegrees(180 - face.toYRot()));
            }
            pose.scale(0.4f, 0.4f, 0.4f);
            int light = LevelRenderer.getLightColor(level, MachineDef.worldPos(be.origin(), be.facing(), port.ax(), port.ay(), port.az())
                    .relative(MachineDef.rotate(be.facing(), face)));
            Minecraft.getInstance().getItemRenderer().renderStatic(stacks.get(0), ItemDisplayContext.FIXED, light, OverlayTexture.NO_OVERLAY,
                    pose, buffers, level, 0);
            pose.popPose();
        }
    }
}
