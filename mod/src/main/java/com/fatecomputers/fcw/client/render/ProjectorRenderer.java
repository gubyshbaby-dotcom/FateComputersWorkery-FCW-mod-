package com.fatecomputers.fcw.client.render;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.cinema.Projection;
import com.fatecomputers.fcw.cinema.ProjectorBlockEntity;
import com.fatecomputers.fcw.client.view.ScreenViewManager;
import com.fatecomputers.fcw.client.view.ViewTexture;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.LightTexture;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderer;
import net.minecraft.client.renderer.blockentity.BlockEntityRendererProvider;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/**
 * Рисует то, что светит проектор: картинку на экране или стене, тени стоящих в луче и сам луч.
 *
 * <ul>
 * <li>Картинка: та же текстура экрана ВМ, что у монитора ({@link ScreenViewManager}); без сигнала —
 * синий прямоугольник. Яркость зависит от лампы (разогрев, износ) и от света в точке картинки:
 * поверх кладётся светлая вуаль, тем плотнее, чем светлее в зале, так что днём изображение
 * вымывается. На голой стене картинка тусклее и слегка просвечивает.</li>
 * <li>Тени: силуэт каждого живого существа в луче проецируется из линзы на плоскость картинки
 * двумя прямоугольниками (туловище и голова). Блоки, которые перекрывают часть луча, тени не дают.</li>
 * <li>Луч: полупрозрачная пирамида от линзы к углам картинки с редкими пылинками, сложение цветов
 * без записи глубины. Виден только в темноте.</li>
 * </ul>
 */
public class ProjectorRenderer implements BlockEntityRenderer<ProjectorBlockEntity> {
    private static final ResourceLocation WHITE = ResourceLocation.withDefaultNamespace("textures/misc/white.png");
    /** Пропорции, пока экран ВМ неизвестен (нет сигнала). */
    private static final float DEFAULT_ASPECT = 4f / 3f;
    private static final float WALL_GAIN = 0.7f;
    private static final float WALL_ALPHA = 0.85f;
    private static final float BEAM_ALPHA = 0.085f;
    private static final int MAX_MOTES = 36;

    private final EntityRenderDispatcher entities;

    public ProjectorRenderer(BlockEntityRendererProvider.Context context) {
        this.entities = context.getEntityRenderer();
    }

    @Override
    public void render(ProjectorBlockEntity be, float partialTick, PoseStack pose, MultiBufferSource buffers, int packedLight, int packedOverlay) {
        Level level = be.getLevel();
        if (level == null || !be.isLit()) {
            return;
        }
        double time = level.getGameTime() + partialTick;
        float lamp = be.brightness(time);
        if (lamp <= 0f) {
            return;
        }
        ViewTexture tex = be.hasSignal() ? ScreenViewManager.get().textureFor(be.vmId()) : null;
        float aspect = tex != null && tex.height() > 0 ? (float) tex.width() / tex.height() : DEFAULT_ASPECT;
        Projection pr = be.projection(aspect);
        if (pr == null) {
            return;
        }
        float ambient = ambient(level, pr.center().add(pr.normal().scale(0.4)), partialTick);

        pose.pushPose();
        BlockPos o = be.getBlockPos();
        pose.translate(-o.getX(), -o.getY(), -o.getZ());
        PoseStack.Pose p = pose.last();

        // --- картинка
        float gain = lamp * (pr.onScreen() ? 1f : WALL_GAIN) * (1f - 0.45f * ambient);
        int alpha = pr.onScreen() ? 255 : (int) (255 * WALL_ALPHA);
        if (tex != null) {
            int g = (int) (255 * gain);
            RenderType type = pr.onScreen() ? RenderType.entitySolid(tex.id()) : RenderType.entityTranslucent(tex.id());
            quad(buffers.getBuffer(type), p, pr, -1, -1, 1, 1, 0.010, g, g, g, alpha, true);
        } else {
            // нет сигнала: синий экран, как у настоящих проекторов
            RenderType type = pr.onScreen() ? RenderType.entitySolid(WHITE) : RenderType.entityTranslucent(WHITE);
            quad(buffers.getBuffer(type), p, pr, -1, -1, 1, 1, 0.010, (int) (24 * gain), (int) (40 * gain), (int) (150 * gain), alpha, false);
        }

        // --- тени тех, кто стоит в луче
        VertexConsumer black = buffers.getBuffer(RenderType.entitySolid(WHITE));
        AABB beamBox = new AABB(pr.lens(), pr.lens());
        for (int i = 0; i < 4; i++) {
            Vec3 c = pr.corner((i & 1) == 0 ? -1 : 1, (i & 2) == 0 ? -1 : 1, 0);
            beamBox = beamBox.minmax(new AABB(c, c));
        }
        for (LivingEntity e : level.getEntitiesOfClass(LivingEntity.class, beamBox, e -> !e.isSpectator() && !e.isInvisible())) {
            AABB b = e.getBoundingBox();
            double split = b.minY + b.getYsize() * 0.72;
            double shrink = b.getXsize() * 0.22;
            shadow(black, p, pr, new AABB(b.minX, b.minY, b.minZ, b.maxX, split, b.maxZ));
            shadow(black, p, pr, new AABB(b.minX + shrink, split, b.minZ + shrink, b.maxX - shrink, b.maxY, b.maxZ - shrink));
        }

        // --- вуаль от света в зале: чёрное становится серым, картинка бледнеет
        if (ambient > 0.02f) {
            int a = (int) (255 * Math.min(0.7f, 0.56f * ambient));
            quad(buffers.getBuffer(RenderType.entityTranslucent(WHITE)), p, pr, -1, -1, 1, 1, 0.014, 236, 236, 228, a, false);
        }

        // --- луч и пыль в нём
        if (FcwConfig.CLIENT.projectorBeam.get()) {
            Vec3 mid = pr.lens().lerp(pr.center(), 0.5);
            float dark = 1f - Math.max(ambient(level, pr.lens(), partialTick), ambient(level, mid, partialTick));
            float a = BEAM_ALPHA * lamp * dark * dark;
            if (a > 0.004f) {
                VertexConsumer rays = buffers.getBuffer(RenderType.dragonRays());
                beam(rays, p, pr, a);
                motes(rays, p, pr, time, a, be.getBlockPos().asLong());
            }
        }
        pose.popPose();
    }

    /**
     * Свет в точке, 0..1: от блоков и от неба с поправкой на время суток (ночью небо не засвечивает).
     */
    private static float ambient(Level level, Vec3 at, float partialTick) {
        BlockPos pos = BlockPos.containing(at);
        float block = level.getBrightness(LightLayer.BLOCK, pos) / 15f;
        float sky = level.getBrightness(LightLayer.SKY, pos) / 15f;
        float day = level instanceof ClientLevel cl ? Mth.clamp((cl.getSkyDarken(partialTick) - 0.2f) / 0.8f, 0f, 1f) : 1f;
        return Mth.clamp(Math.max(block, sky * day), 0f, 1f);
    }

    /**
     * Прямоугольник в плоскости картинки, координаты в долях её половин (-1..1), лицом к зрителю.
     * Нормаль смотрит вверх, чтобы освещение сущностей не затемняло изображение (как у монитора).
     */
    private static void quad(VertexConsumer vc, PoseStack.Pose p, Projection pr, double u0, double v0, double u1, double v1,
                             double lift, int r, int g, int b, int a, boolean textured) {
        float tu0 = textured ? (float) (u0 + 1) / 2f : 0f, tu1 = textured ? (float) (u1 + 1) / 2f : 1f;
        float tv0 = textured ? (float) (1 - v0) / 2f : 1f, tv1 = textured ? (float) (1 - v1) / 2f : 0f;
        vertex(vc, p, pr.corner(u0, v0, lift), tu0, tv0, r, g, b, a);
        vertex(vc, p, pr.corner(u1, v0, lift), tu1, tv0, r, g, b, a);
        vertex(vc, p, pr.corner(u1, v1, lift), tu1, tv1, r, g, b, a);
        vertex(vc, p, pr.corner(u0, v1, lift), tu0, tv1, r, g, b, a);
    }

    private static void vertex(VertexConsumer vc, PoseStack.Pose p, Vec3 at, float u, float v, int r, int g, int b, int a) {
        vc.addVertex(p, (float) at.x, (float) at.y, (float) at.z)
                .setColor(r, g, b, a)
                .setUv(u, v)
                .setOverlay(OverlayTexture.NO_OVERLAY)
                .setLight(LightTexture.FULL_BRIGHT)
                .setNormal(p, 0f, 1f, 0f);
    }

    /** Тень коробки: её углы проецируются из линзы на плоскость картинки, берётся охватывающий прямоугольник. */
    private static void shadow(VertexConsumer vc, PoseStack.Pose p, Projection pr, AABB box) {
        Vec3 dir = pr.normal().scale(-1);
        double u0 = Double.MAX_VALUE, v0 = Double.MAX_VALUE, u1 = -Double.MAX_VALUE, v1 = -Double.MAX_VALUE;
        for (int i = 0; i < 8; i++) {
            Vec3 c = new Vec3((i & 1) == 0 ? box.minX : box.maxX, (i & 2) == 0 ? box.minY : box.maxY, (i & 4) == 0 ? box.minZ : box.maxZ);
            Vec3 rel = c.subtract(pr.lens());
            double depth = rel.dot(dir);
            if (depth < 0.15 || depth > pr.distance() - 0.05) {
                return; // существо за линзой, вплотную к ней или за плоскостью картинки
            }
            Vec3 hit = pr.lens().add(rel.scale(pr.distance() / depth)).subtract(pr.center());
            double u = hit.dot(pr.right()) / pr.halfW(), v = hit.y / pr.halfH();
            u0 = Math.min(u0, u);
            u1 = Math.max(u1, u);
            v0 = Math.min(v0, v);
            v1 = Math.max(v1, v);
        }
        u0 = Math.max(u0, -1);
        v0 = Math.max(v0, -1);
        u1 = Math.min(u1, 1);
        v1 = Math.min(v1, 1);
        if (u0 < u1 && v0 < v1) {
            quad(vc, p, pr, u0, v0, u1, v1, 0.012, 0, 0, 0, 255, false);
        }
    }

    /** Четыре грани пирамиды; каждая выдаётся в обе стороны, потому что у типа включено отсечение граней. */
    private static void beam(VertexConsumer vc, PoseStack.Pose p, Projection pr, float alpha) {
        Vec3[] c = {pr.corner(-1, -1, 0.02), pr.corner(1, -1, 0.02), pr.corner(1, 1, 0.02), pr.corner(-1, 1, 0.02)};
        int near = (int) (255 * alpha), far = (int) (255 * alpha * 0.22f);
        for (int i = 0; i < 4; i++) {
            Vec3 a = c[i], b = c[(i + 1) % 4];
            tri(vc, p, pr.lens(), near, a, far, b, far);
            tri(vc, p, pr.lens(), near, b, far, a, far);
        }
    }

    private static void tri(VertexConsumer vc, PoseStack.Pose p, Vec3 a, int aa, Vec3 b, int ba, Vec3 c, int ca) {
        vc.addVertex(p, (float) a.x, (float) a.y, (float) a.z).setColor(255, 250, 236, aa);
        vc.addVertex(p, (float) b.x, (float) b.y, (float) b.z).setColor(255, 250, 236, ba);
        vc.addVertex(p, (float) c.x, (float) c.y, (float) c.z).setColor(255, 250, 236, ca);
    }

    /**
     * Пылинки: у каждой свой цикл жизни; на каждый цикл она получает новое случайное место в
     * пирамиде, медленно плывёт и плавно загорается и гаснет. Частицы игры не используются:
     * пылинка существует только как пара треугольников, повёрнутых к камере.
     */
    private void motes(VertexConsumer vc, PoseStack.Pose p, Projection pr, double time, float beamAlpha, long seed) {
        int n = (int) Mth.clamp(pr.distance() * 1.5, 6, MAX_MOTES);
        Quaternionf cam = entities.cameraOrientation();
        Vector3f camRight = new Vector3f(1, 0, 0).rotate(cam);
        Vector3f camUp = new Vector3f(0, 1, 0).rotate(cam);
        Vec3 axis = pr.center().subtract(pr.lens());
        for (int i = 0; i < n; i++) {
            long h = mix(seed + i * 0x9E3779B97F4A7C15L);
            double life = 160 + (h & 127);
            double t = time / life + ((h >> 8) & 255) / 255.0;
            long cycle = (long) Math.floor(t);
            double phase = t - cycle;
            long r = mix(h + cycle);
            double depth = 0.15 + 0.8 * ((r & 1023) / 1023.0);
            double u = (((r >> 10) & 1023) / 511.5 - 1) * 0.9 + 0.06 * Math.sin(time * 0.02 + i);
            double v = (((r >> 20) & 1023) / 511.5 - 1) * 0.9 - 0.10 * phase;
            Vec3 at = pr.lens().add(axis.scale(depth))
                    .add(pr.right().scale(pr.halfW() * u * depth))
                    .add(0, pr.halfH() * v * depth, 0);
            float fade = (float) Math.sin(Math.PI * phase);
            int a = (int) (255 * Math.min(1f, beamAlpha * 4.5f) * fade * fade);
            if (a <= 2) {
                continue;
            }
            float s = 0.010f + 0.008f * ((r >> 30) & 3) / 3f;
            Vec3 rx = new Vec3(camRight.x * s, camRight.y * s, camRight.z * s);
            Vec3 ux = new Vec3(camUp.x * s, camUp.y * s, camUp.z * s);
            Vec3 a0 = at.subtract(rx).subtract(ux), a1 = at.add(rx).subtract(ux), a2 = at.add(rx).add(ux), a3 = at.subtract(rx).add(ux);
            tri(vc, p, a0, a, a1, a, a2, a);
            tri(vc, p, a0, a, a2, a, a3, a);
            tri(vc, p, a0, a, a2, a, a1, a);
            tri(vc, p, a0, a, a3, a, a2, a);
        }
    }

    /** Перемешивание битов (splitmix64): дешёвый детерминированный генератор для пылинок. */
    private static long mix(long z) {
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return (z ^ (z >>> 31)) & Long.MAX_VALUE;
    }

    @Override
    public boolean shouldRenderOffScreen(ProjectorBlockEntity be) {
        return true; // картинка далеко от самого блока
    }

    @Override
    public AABB getRenderBoundingBox(ProjectorBlockEntity be) {
        return AABB.INFINITE;
    }

    @Override
    public boolean shouldRender(ProjectorBlockEntity be, Vec3 camera) {
        double max = getViewDistance();
        Projection pr = be.lastProjection();
        return Vec3.atCenterOf(be.getBlockPos()).closerThan(camera, max) || (pr != null && pr.center().closerThan(camera, max));
    }

    @Override
    public int getViewDistance() {
        return Math.max(16, FcwConfig.CLIENT.screenRenderDistance.get());
    }
}
