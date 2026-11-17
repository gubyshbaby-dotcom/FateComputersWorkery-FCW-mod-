package com.fatecomputers.fcw.client.render;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.client.view.ScreenViewManager;
import com.fatecomputers.fcw.client.view.ViewTexture;
import com.fatecomputers.fcw.registry.ModBlocks;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Axis;
import net.minecraft.client.renderer.LightTexture;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderer;
import net.minecraft.client.renderer.blockentity.BlockEntityRendererProvider;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.core.Direction;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.level.block.state.BlockState;

/**
 * Draws the live guest screen over the "screen" element of the monitor model, full bright, with
 * the aspect ratio kept (black bars). Without a live frame nothing is drawn and the model's own
 * screen texture stays visible.
 */
public class MonitorRenderer implements BlockEntityRenderer<MonitorBlockEntity> {
    /** Screen faces from the models, in model pixels: x0, y0, x1, y1, z of the north face. */
    private static final float[] LCD = {1.5f, 5.5f, 14.5f, 14.5f, 6.5f};
    private static final float[] CRT = {3f, 4.5f, 13f, 12f, 1f};
    private static final ResourceLocation WHITE = ResourceLocation.withDefaultNamespace("textures/misc/white.png");

    public MonitorRenderer(BlockEntityRendererProvider.Context context) {
    }

    @Override
    public void render(MonitorBlockEntity be, float partialTick, PoseStack pose, MultiBufferSource buffers, int packedLight, int packedOverlay) {
        if (!be.isScreenOn()) {
            return;
        }
        ViewTexture tex = ScreenViewManager.get().textureFor(be);
        if (tex == null) {
            return;
        }
        BlockState state = be.getBlockState();
        float[] r = state.is(ModBlocks.CRT_MONITOR.get()) ? CRT : LCD;
        Direction facing = state.hasProperty(DeviceBlock.FACING) ? state.getValue(DeviceBlock.FACING) : Direction.NORTH;

        pose.pushPose();
        pose.translate(0.5f, 0f, 0.5f);
        pose.mulPose(Axis.YP.rotationDegrees(180f - facing.toYRot()));
        pose.translate(-0.5f, 0f, -0.5f);
        PoseStack.Pose p = pose.last();

        float x0 = r[0] / 16f, y0 = r[1] / 16f, x1 = r[2] / 16f, y1 = r[3] / 16f, z = r[4] / 16f;
        // black bars over the whole screen face
        quad(buffers.getBuffer(RenderType.entitySolid(WHITE)), p, x0, y0, x1, y1, z - 0.0015f, 0, 0, 1, 1, 0);

        float sw = x1 - x0, sh = y1 - y0;
        float aspect = (float) tex.width() / tex.height();
        float iw = sw, ih = sh;
        if (aspect > sw / sh) {
            ih = sw / aspect;
        } else {
            iw = sh * aspect;
        }
        float ix0 = x0 + (sw - iw) / 2f, iy0 = y0 + (sh - ih) / 2f;
        quad(buffers.getBuffer(RenderType.entitySolid(tex.id())), p, ix0, iy0, ix0 + iw, iy0 + ih, z - 0.003f, 0, 0, 1, 1, 0xFF);
        pose.popPose();
    }

    /**
     * A quad facing north (-Z). Seen from the front the viewer's right is -X, so u=0 is at x1.
     * The normal points up so that entity lighting does not darken the screen.
     */
    private static void quad(VertexConsumer vc, PoseStack.Pose p, float x0, float y0, float x1, float y1, float z,
                             float u0, float v0, float u1, float v1, int gray) {
        vertex(vc, p, x1, y0, z, u0, v1, gray);
        vertex(vc, p, x0, y0, z, u1, v1, gray);
        vertex(vc, p, x0, y1, z, u1, v0, gray);
        vertex(vc, p, x1, y1, z, u0, v0, gray);
    }

    private static void vertex(VertexConsumer vc, PoseStack.Pose p, float x, float y, float z, float u, float v, int gray) {
        vc.addVertex(p, x, y, z)
                .setColor(gray, gray, gray, 255)
                .setUv(u, v)
                .setOverlay(OverlayTexture.NO_OVERLAY)
                .setLight(LightTexture.FULL_BRIGHT)
                .setNormal(p, 0f, 1f, 0f);
    }

    @Override
    public int getViewDistance() {
        return Math.max(16, FcwConfig.CLIENT.screenRenderDistance.get());
    }
}
