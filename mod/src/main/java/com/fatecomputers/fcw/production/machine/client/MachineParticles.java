package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.production.machine.MachineAnimation.Clip;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Emitter;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Fx;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.Timeline;
import com.mojang.blaze3d.platform.GlStateManager;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.Tesselator;
import com.mojang.blaze3d.vertex.VertexFormat;
import net.minecraft.client.Minecraft;
import net.minecraft.client.ParticleStatus;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.particle.ParticleRenderType;
import net.minecraft.client.particle.SpriteSet;
import net.minecraft.client.particle.TextureSheetParticle;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.util.RandomSource;
import net.minecraft.world.phys.Vec3;

import java.util.HashMap;
import java.util.Map;

/**
 * Particles of the machines. The clip files say where and when: every clip lists emitters with a time
 * window and a rate; an emitter has boxes to spawn in (structure space), a velocity range, gravity,
 * drag, life time and size. Spawned on the client every tick while the clip plays.
 */
public final class MachineParticles {
    /** Sprite sets by particle id (furnace_spark, ...), filled when the particle providers are registered. */
    static final Map<String, SpriteSet> SPRITES = new HashMap<>();
    private static final double MAX_DISTANCE = 64.0;

    /** Bright, unlit particles (sparks, embers, flashes): additive blending. */
    static final ParticleRenderType ADDITIVE = new ParticleRenderType() {
        @Override
        public BufferBuilder begin(Tesselator tesselator, TextureManager textures) {
            RenderSystem.depthMask(false);
            RenderSystem.setShaderTexture(0, TextureAtlas.LOCATION_PARTICLES);
            RenderSystem.enableBlend();
            RenderSystem.blendFunc(GlStateManager.SourceFactor.SRC_ALPHA, GlStateManager.DestFactor.ONE);
            return tesselator.begin(VertexFormat.Mode.QUADS, DefaultVertexFormat.PARTICLE);
        }

        @Override
        public String toString() {
            return "FCW_MACHINE_ADDITIVE";
        }
    };

    private MachineParticles() {
    }

    /** Client tick of a controller. */
    public static void tick(MachineControllerBlockEntity be) {
        Minecraft mc = Minecraft.getInstance();
        if (!(be.getLevel() instanceof ClientLevel level) || mc.isPaused() || mc.gameRenderer == null) {
            return;
        }
        Vec3 cam = mc.gameRenderer.getMainCamera().getPosition();
        if (cam.distanceToSqr(Vec3.atCenterOf(be.getBlockPos())) > MAX_DISTANCE * MAX_DISTANCE) {
            return;
        }
        double amount = switch (mc.options.particles().get()) {
            case ALL -> 1.0;
            case DECREASED -> 0.5;
            case MINIMAL -> 0.2;
        };
        Timeline.At work = be.clientWork(0);
        if (work != null) {
            emit(be, level, work.clip(), work.clipTime(), amount);
        }
        Clip ambient = MachineRenderer.ambient(be);
        if (ambient != null) {
            emit(be, level, ambient, (level.getGameTime() / 20.0) % ambient.length, amount);
        }
    }

    private static void emit(MachineControllerBlockEntity be, ClientLevel level, Clip clip, double time, double amount) {
        MachineDef def = be.def();
        RandomSource random = level.random;
        for (Fx fx : clip.fx) {
            if (time < fx.t0() || time >= fx.t1()) {
                continue;
            }
            Emitter e = def.anim.emitters.get(fx.emitter());
            SpriteSet sprites = e == null ? null : SPRITES.get(def.spec.particlePrefix() + "_" + e.sprite);
            if (sprites == null) {
                continue;
            }
            double perTick = e.rate * fx.rate() * amount / 20.0;
            for (double[][] box : e.boxes) {
                int n = (int) perTick;
                if (random.nextDouble() < perTick - n) {
                    n++;
                }
                for (int i = 0; i < n; i++) {
                    spawn(be, level, e, sprites, box, random);
                }
            }
        }
    }

    private static void spawn(MachineControllerBlockEntity be, ClientLevel level, Emitter e, SpriteSet sprites, double[][] box, RandomSource random) {
        Direction facing = be.facing();
        BlockPos origin = be.origin();
        double[] p = new double[3], v = new double[3];
        for (int k = 0; k < 3; k++) {
            p[k] = (box[0][k] + (box[1][k] - box[0][k]) * random.nextDouble()) / 16.0;
            v[k] = (e.vel[0][k] + (e.vel[1][k] - e.vel[0][k]) * random.nextDouble()) / 16.0 / 20.0;
        }
        double[] wp = turn(facing, p[0] - 0.5, p[2] - 0.5);
        double[] wv = turn(facing, v[0], v[2]);
        Minecraft.getInstance().particleEngine.add(new MachineParticle(level,
                origin.getX() + 0.5 + wp[0], origin.getY() + p[1], origin.getZ() + 0.5 + wp[1],
                wv[0], v[1], wv[1], e, sprites, random));
    }

    /** A horizontal vector of structure space in the world: north (x, z), east (-z, x), south (-x, -z), west (z, -x). */
    private static double[] turn(Direction facing, double x, double z) {
        return switch (facing) {
            case EAST -> new double[] {-z, x};
            case SOUTH -> new double[] {-x, -z};
            case WEST -> new double[] {z, -x};
            default -> new double[] {x, z};
        };
    }

    /**
     * One particle: every second v.y += gravity and v *= (1 - drag); the size goes from size[0] to size[1],
     * the sprite frame follows the age.
     */
    static final class MachineParticle extends TextureSheetParticle {
        private final SpriteSet sprites;
        private final boolean additive;
        private final float size0, size1, gravityPerTick, dragPerTick;

        MachineParticle(ClientLevel level, double x, double y, double z, double vx, double vy, double vz, Emitter e, SpriteSet sprites, RandomSource random) {
            super(level, x, y, z);
            this.sprites = sprites;
            this.additive = e.additive;
            this.xd = vx;
            this.yd = vy;
            this.zd = vz;
            this.lifetime = Math.max(2, (int) Math.round((e.life[0] + (e.life[1] - e.life[0]) * random.nextDouble()) * 20.0));
            this.size0 = (float) (e.size[0] / 32.0);
            this.size1 = (float) (e.size[1] / 32.0);
            this.gravityPerTick = (float) (e.gravity / 16.0 / 400.0);
            this.dragPerTick = (float) Math.pow(Math.max(0.0, 1.0 - e.drag), 1.0 / 20.0);
            this.hasPhysics = false;
            this.quadSize = size0;
            if (!additive) {
                this.alpha = 0.55f;
                setColor((e.tint >> 16 & 0xFF) / 255.0f, (e.tint >> 8 & 0xFF) / 255.0f, (e.tint & 0xFF) / 255.0f);
                this.roll = this.oRoll = random.nextFloat() * 6.2832f;
            }
            setSpriteFromAge(sprites);
        }

        @Override
        public void tick() {
            xo = x;
            yo = y;
            zo = z;
            if (age++ >= lifetime) {
                remove();
                return;
            }
            yd += gravityPerTick;
            xd *= dragPerTick;
            yd *= dragPerTick;
            zd *= dragPerTick;
            setPos(x + xd, y + yd, z + zd);
            float u = (float) age / lifetime;
            quadSize = size0 + (size1 - size0) * u;
            alpha = additive ? 1.0f - u * u : 0.55f * (1.0f - u);
            setSpriteFromAge(sprites);
        }

        @Override
        public ParticleRenderType getRenderType() {
            return additive ? ADDITIVE : ParticleRenderType.PARTICLE_SHEET_TRANSLUCENT;
        }

        @Override
        protected int getLightColor(float partialTick) {
            return additive ? 0xF000F0 : super.getLightColor(partialTick);
        }
    }
}
