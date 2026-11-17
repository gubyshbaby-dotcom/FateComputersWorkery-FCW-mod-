package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.production.machine.MachineAnimation;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Clip;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Cursor;
import com.fatecomputers.fcw.production.machine.MachineAnimation.Prop;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineSpec;
import com.fatecomputers.fcw.production.machine.MachineState;
import com.fatecomputers.fcw.production.machine.Timeline;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Axis;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.block.model.BakedQuad;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderer;
import net.minecraft.client.renderer.blockentity.BlockEntityRendererProvider;
import net.minecraft.client.resources.model.BakedModel;
import net.minecraft.client.resources.model.ModelResourceLocation;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.client.model.data.ModelData;
import org.joml.Matrix4f;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.List;

/**
 * Draws what moves in a machine: the parts from dyn/ in the pose of the clips that play now, the things
 * the clips carry (props) and the things the inventory shows (slot displays). The cells themselves are
 * ordinary block models without these parts ("bare" models in the blockstates).
 */
public class MachineRenderer implements BlockEntityRenderer<MachineControllerBlockEntity> {
    private static final Direction[] SIDES = {null, Direction.DOWN, Direction.UP, Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST};
    private static final double[] IDENTITY = MachineAnimation.translate(0, 0, 0);

    private final RandomSource random = RandomSource.create();

    public MachineRenderer(BlockEntityRendererProvider.Context context) {
    }

    /** The clips playing at this moment: the work clip (if any) first, then the ambient one. */
    public static List<Cursor> cursors(MachineControllerBlockEntity be, float partialTick, @Nullable Timeline.At work) {
        List<Cursor> list = new ArrayList<>(2);
        if (work != null) {
            list.add(new Cursor(work.clip(), work.clipTime()));
        }
        Clip ambient = ambient(be);
        if (ambient != null && be.getLevel() != null) {
            list.add(new Cursor(ambient, MachineAnimation.clipTime(ambient, (be.getLevel().getGameTime() + partialTick) / 20.0)));
        }
        return list;
    }

    @Nullable
    public static Clip ambient(MachineControllerBlockEntity be) {
        MachineState state = be.shownState();
        if (state == MachineState.OFF) {
            return null;
        }
        return be.def().anim.clips.get(be.clientWorking() ? "ambient_run" : "ambient_idle");
    }

    @Override
    public void render(MachineControllerBlockEntity be, float partialTick, PoseStack pose, MultiBufferSource buffers, int packedLight, int packedOverlay) {
        Level level = be.getLevel();
        if (level == null) {
            return;
        }
        MachineDef def = be.def();
        Direction facing = be.facing();
        BlockPos origin = be.origin();
        MachineState state = be.shownState();
        Timeline.At work = be.clientWork(partialTick);
        List<Cursor> cursors = cursors(be, partialTick, work);
        VertexConsumer buffer = buffers.getBuffer(RenderType.cutout());

        pose.pushPose();
        BlockPos o = origin.subtract(be.getBlockPos());
        pose.translate(o.getX() + 0.5, o.getY(), o.getZ() + 0.5);
        pose.mulPose(Axis.YP.rotationDegrees(MachineDef.yRotation(facing)));
        pose.translate(-0.5, 0, -0.5);

        for (MachineDef.Dyn dyn : def.dynamic.values()) {
            if (dyn.moving()) {
                double[] m = def.anim.parts.containsKey(dyn.id()) ? def.anim.partMatrix(cursors, dyn.id()) : IDENTITY;
                draw(level, origin, facing, state, dyn, m, pose, buffer, packedOverlay);
            }
        }
        if (work != null) {
            for (Prop prop : work.clip().props) {
                MachineDef.Dyn dyn = def.dynamic.get(prop.model);
                if (dyn == null || !propShown(be, prop.model)) {
                    continue;
                }
                double[] m = def.anim.propMatrix(cursors, prop);
                if (m != null) {
                    draw(level, origin, facing, state, dyn, m, pose, buffer, packedOverlay);
                }
            }
        }
        for (MachineSpec.Display d : def.spec.displays()) {
            MachineDef.Slot slot = def.slots.get(d.slot());
            MachineDef.Dyn dyn = slot == null ? null : def.dynamic.get(slot.model());
            if (dyn == null || !be.present(d.section(), d.when()) || (work != null && movesModel(work.clip(), slot.model()))) {
                continue;
            }
            double[] m = MachineAnimation.translate(slot.offset()[0], slot.offset()[1], slot.offset()[2]);
            if (d.carrierPart() != null) {
                m = MachineAnimation.mul(def.anim.partMatrix(cursors, d.carrierPart()), m);
            }
            draw(level, origin, facing, state, dyn, m, pose, buffer, packedOverlay);
        }
        pose.popPose();
    }

    private static boolean movesModel(Clip clip, String model) {
        for (Prop p : clip.props) {
            if (p.model.equals(model)) {
                return true;
            }
        }
        return false;
    }

    /** Props of a clip shared by several sections (the chip line's takt) show only for the sections that work. */
    private static boolean propShown(MachineControllerBlockEntity be, String model) {
        List<MachineSpec.Section> sections = be.def().spec.sections();
        for (int i = 0; i < sections.size(); i++) {
            if (sections.get(i).propModels() != null && sections.get(i).propModels().contains(model)) {
                return be.sectionRunning(i);
            }
        }
        return true;
    }

    private void draw(Level level, BlockPos origin, Direction facing, MachineState state, MachineDef.Dyn dyn, double[] matrix,
                      PoseStack pose, VertexConsumer buffer, int overlay) {
        BakedModel model = Minecraft.getInstance().getModelManager().getModel(ModelResourceLocation.standalone(dyn.models().get(state)));
        int[] a = dyn.anchor();
        int light = LevelRenderer.getLightColor(level, MachineDef.worldPos(origin, facing, a[0], a[1], a[2]));
        pose.pushPose();
        pose.mulPose(new Matrix4f().set(MachineAnimation.toJoml(matrix)));
        pose.translate(a[0], a[1], a[2]);
        PoseStack.Pose last = pose.last();
        for (Direction side : SIDES) {
            random.setSeed(42L);
            for (BakedQuad quad : model.getQuads(null, side, random, ModelData.EMPTY, null)) {
                // the same face shading the cells get from the chunk mesher, for the structure's facing
                float shade = level.getShade(MachineDef.rotate(facing, quad.getDirection()), quad.isShade());
                buffer.putBulkData(last, quad, shade, shade, shade, 1.0f, light, overlay);
            }
        }
        pose.popPose();
    }

    @Override
    public AABB getRenderBoundingBox(MachineControllerBlockEntity be) {
        MachineDef def = be.def();
        BlockPos a = be.origin();
        BlockPos b = MachineDef.worldPos(a, be.facing(), def.sx - 1, def.sy - 1, def.sz - 1);
        return AABB.encapsulatingFullBlocks(a, b).inflate(2.0);
    }

    @Override
    public boolean shouldRenderOffScreen(MachineControllerBlockEntity be) {
        return true;
    }

    @Override
    public int getViewDistance() {
        return 128;
    }
}
