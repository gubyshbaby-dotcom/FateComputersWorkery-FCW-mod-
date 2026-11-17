package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import com.fatecomputers.fcw.production.machine.construction.ConstructionPlan;
import com.fatecomputers.fcw.production.machine.construction.ConstructionScaffoldBlock;
import com.fatecomputers.fcw.production.machine.construction.ConstructionSite;
import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.neoforged.neoforge.client.event.RenderHighlightEvent;

import javax.annotation.Nullable;
import java.util.List;

/**
 * What a player sees when aiming at a machine (docs/design/MACHINE_UI.md): one line above the hotbar
 * that says what this place is, what lies there and what a right click will do; a white outline when the
 * item in the hand belongs there; and, for two seconds, the reason when an item was refused. No
 * particles, no chat. The line can be switched off in the client config (machines.hints).
 */
public final class MachineHud {
    private static final long NOTICE_MS = 2000;

    @Nullable
    private static Component notice;
    private static long noticeUntil;

    private MachineHud() {
    }

    /** The server refused an item: its reason replaces the hint for a moment. */
    public static void notice(Component text) {
        notice = text;
        noticeUntil = System.currentTimeMillis() + NOTICE_MS;
    }

    private record Target(MachineCellBlock block, BlockState state, BlockPos pos, MachineDef.Cell cell) {
    }

    @Nullable
    private static Target target(Minecraft mc) {
        if (mc.level == null || mc.player == null || !(mc.hitResult instanceof BlockHitResult hit) || hit.getType() != HitResult.Type.BLOCK) {
            return null;
        }
        BlockState state = mc.level.getBlockState(hit.getBlockPos());
        if (!(state.getBlock() instanceof MachineCellBlock block)) {
            return null;
        }
        MachineDef.Cell cell = block.cell(state);
        return cell == null ? null : new Target(block, state, hit.getBlockPos(), cell);
    }

    /** GUI layer above the hotbar. */
    public static void render(GuiGraphics g, DeltaTracker delta) {
        Minecraft mc = Minecraft.getInstance();
        if (mc.options.hideGui || mc.screen != null) {
            return;
        }
        Component line = null;
        if (notice != null && System.currentTimeMillis() < noticeUntil) {
            line = notice;
        } else if (FcwConfig.CLIENT.machineHints.get()) {
            MachineControllerBlockEntity site = siteAt(mc);
            if (site != null) {
                line = siteHint(mc, site);
            } else {
                Target t = target(mc);
                line = t == null ? null : hint(mc, t);
            }
        }
        if (line != null) {
            int w = mc.font.width(line);
            int x = (g.guiWidth() - w) / 2, y = g.guiHeight() - 72;
            g.fill(x - 3, y - 2, x + w + 3, y + 10, 0x80000000);
            g.drawString(mc.font, line, x, y, 0xFFFFFF, false);
        }
    }

    /** The site whose pallet, scaffolding or half-built cell the player aims at. */
    @Nullable
    private static MachineControllerBlockEntity siteAt(Minecraft mc) {
        if (mc.level == null || mc.player == null || !(mc.hitResult instanceof BlockHitResult hit) || hit.getType() != HitResult.Type.BLOCK) {
            return null;
        }
        BlockState state = mc.level.getBlockState(hit.getBlockPos());
        MachineControllerBlockEntity be = null;
        if (state.getBlock() instanceof ConstructionScaffoldBlock) {
            be = ConstructionScaffoldBlock.controller(mc.level, hit.getBlockPos(), state);
        } else if (state.getBlock() instanceof MachineCellBlock block) {
            be = block.controller(mc.level, hit.getBlockPos(), state);
        }
        return be != null && be.site() != null ? be : null;
    }

    /** "Arc furnace, stage 2/5 "Hearth and lining": lining 12/36, cooling panel 0/8". */
    private static Component siteHint(Minecraft mc, MachineControllerBlockEntity be) {
        ConstructionSite site = be.site();
        ConstructionPlan plan = site.plan();
        int index = Math.max(0, Math.min(site.stage(), plan.stages.size() - 1));
        ConstructionPlan.Stage stage = plan.stages.get(index);
        MutableComponent line = Component.translatable("interact.fcw.site.line", Component.translatable("machine.fcw." + be.def().name),
                index + 1, plan.stages.size(), stage.title(mc.getLanguageManager().getSelected()));
        if (site.mode() == ConstructionSite.Mode.TEARDOWN) {
            line.append(": ").append(Component.translatable("interact.fcw.site.teardown_progress", (int) Math.round(site.stageProgress() * 100)));
        } else if (site.started()) {
            line.append(": ").append(Component.translatable(site.waitingPower() ? "interact.fcw.site.no_power" : "interact.fcw.site.progress",
                    (int) Math.round(site.stageProgress() * 100)));
        } else {
            line.append(": ");
            int shown = 0;
            for (int e = 0; e < stage.entries().size() && shown < 3; e++) {
                int need = stage.entries().get(e).need(), have = site.have(index, e);
                if (have < need) {
                    line.append(shown++ > 0 ? ", " : "").append(stage.entries().get(e).item().getDescription()).append(" " + have + "/" + need);
                }
            }
        }
        if (site.wants(mc.player.getMainHandItem())) {
            line.append("  ·  ").append(Component.translatable("interact.fcw.put"));
        }
        return line;
    }

    @Nullable
    private static Component hint(Minecraft mc, Target t) {
        MachineDef def = t.block.def();
        MachineControllerBlockEntity be = t.block.controller(mc.level, t.pos, t.state);
        if (be == null) {
            return null;
        }
        Zone zone = def.zoneAt(t.cell);
        boolean terminal = t.block.isTerminal(t.state) || (t.cell.hatch() == null && zone == null && def.opensTerminal(t.cell));
        if (terminal) {
            return Component.translatable("interact.fcw.terminal");
        }
        ItemStack held = mc.player.getMainHandItem();
        if (MachineDef.BREAKER.equals(t.cell.hatch())) {
            return Component.translatable(be.isEnabled() ? "interact.fcw.breaker.on" : "interact.fcw.breaker.off");
        }
        if (zone == null) {
            return null;
        }
        MutableComponent line = Component.translatable("interact.fcw." + def.name + "." + zone.id()).append(": ");
        List<ItemStack> stacks = be.zoneContents(zone);
        if (stacks.isEmpty()) {
            line.append(Component.translatable("interact.fcw.empty"));
        } else {
            for (int i = 0; i < stacks.size(); i++) {
                ItemStack s = stacks.get(i);
                if (i > 0) {
                    line.append(", ");
                }
                line.append(s.getHoverName());
                if (zone.role() == Role.CATALYST && s.isDamageableItem()) {
                    line.append(", ").append(Component.translatable("interact.fcw.wear", s.getMaxDamage() - s.getDamageValue(), s.getMaxDamage()));
                } else if (stacks.size() == 1 && zone.role() == Role.INPUT) {
                    line.append(" " + s.getCount() + "/" + s.getMaxStackSize());
                } else {
                    line.append(" ×" + s.getCount());
                }
            }
        }
        Component action;
        Component accepts = Component.translatable("interact.fcw.accepts",
                Component.translatable("interact.fcw." + def.name + "." + zone.id() + ".accepts"));
        if (!held.isEmpty()) {
            action = MachineControllerBlockEntity.zoneAccepts(mc.level, def, zone, held)
                    ? Component.translatable("interact.fcw.put")
                    : zone.role() == Role.OUTPUT ? Component.translatable("interact.fcw.output_only") : accepts;
        } else if (be.handCanTake(zone)) {
            action = Component.translatable(!zone.handTakes() ? "interact.fcw.take_sneak"
                    : zone.role() == Role.OUTPUT || zone.role() == Role.TRAY ? "interact.fcw.take_all" : "interact.fcw.take");
        } else if (zone.role() == Role.CATALYST && !stacks.isEmpty()) {
            action = Component.translatable("interact.fcw.busy");
        } else {
            action = zone.role() == Role.OUTPUT ? null : accepts;
        }
        if (action != null) {
            line.append("  ·  ").append(action);
        }
        return line;
    }

    /** The outline of the aimed block: white instead of the dark vanilla one when the item in the hand belongs there. */
    public static void onHighlight(RenderHighlightEvent.Block event) {
        Minecraft mc = Minecraft.getInstance();
        MachineControllerBlockEntity site = siteAt(mc);
        if (site != null) {
            // a component the site still needs: white outline on whatever part of the site is aimed at
            if (mc.player != null && site.site().wants(mc.player.getMainHandItem())) {
                BlockPos p = event.getTarget().getBlockPos();
                VoxelShape s = mc.level.getBlockState(p).getShape(mc.level, p);
                Vec3 c = event.getCamera().getPosition();
                for (AABB box : s.toAabbs()) {
                    LevelRenderer.renderLineBox(event.getPoseStack(), event.getMultiBufferSource().getBuffer(RenderType.lines()),
                            box.move(p.getX() - c.x, p.getY() - c.y, p.getZ() - c.z).inflate(0.002), 1f, 1f, 1f, 0.9f);
                }
                event.setCanceled(true);
            }
            return;
        }
        Target t = target(mc);
        if (t == null || !t.pos.equals(event.getTarget().getBlockPos())) {
            return;
        }
        Zone zone = t.block.def().zoneAt(t.cell);
        ItemStack held = mc.player.getMainHandItem();
        if (zone == null || !MachineControllerBlockEntity.zoneAccepts(mc.level, t.block.def(), zone, held)) {
            return;
        }
        VoxelShape shape = t.state.getShape(mc.level, t.pos);
        if (shape.isEmpty()) {
            return;
        }
        Vec3 cam = event.getCamera().getPosition();
        PoseStack pose = event.getPoseStack();
        for (AABB box : shape.toAabbs()) {
            LevelRenderer.renderLineBox(pose, event.getMultiBufferSource().getBuffer(RenderType.lines()),
                    box.move(t.pos.getX() - cam.x, t.pos.getY() - cam.y, t.pos.getZ() - cam.z).inflate(0.002), 1f, 1f, 1f, 0.9f);
        }
        event.setCanceled(true);
    }
}
