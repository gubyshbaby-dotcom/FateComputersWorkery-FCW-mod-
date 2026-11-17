package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
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
            Target t = target(mc);
            line = t == null ? null : hint(mc, t);
        }
        if (line != null) {
            int w = mc.font.width(line);
            int x = (g.guiWidth() - w) / 2, y = g.guiHeight() - 72;
            g.fill(x - 3, y - 2, x + w + 3, y + 10, 0x80000000);
            g.drawString(mc.font, line, x, y, 0xFFFFFF, false);
        }
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
