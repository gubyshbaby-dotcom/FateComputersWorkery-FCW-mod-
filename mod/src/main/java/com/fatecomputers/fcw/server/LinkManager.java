package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.Level;

import javax.annotation.Nullable;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * Linking peripherals to computers. On placement a device links to the nearest loaded computer
 * within {@code linkRadius}; sneak + right click (empty hand) on a computer and then on a device
 * re-links it.
 */
public final class LinkManager {
    private static final long SELECTION_TTL_MS = 60_000;

    private record Selection(ResourceKey<Level> dimension, BlockPos pos, UUID vmId, long time) {
    }

    private static final Map<UUID, Selection> SELECTIONS = new HashMap<>();

    private LinkManager() {
    }

    public static void clear() {
        SELECTIONS.clear();
    }

    private static String pos(BlockPos p) {
        return p.getX() + " " + p.getY() + " " + p.getZ();
    }

    public static void selectComputer(ServerPlayer player, ComputerBlockEntity computer) {
        SELECTIONS.put(player.getUUID(), new Selection(player.level().dimension(), computer.getBlockPos().immutable(),
                computer.vmId(), System.currentTimeMillis()));
        player.displayClientMessage(Component.translatable("message.fcw.computer_selected", pos(computer.getBlockPos())), true);
    }

    public static void peripheralShiftClick(ServerPlayer player, PeripheralBlockEntity device) {
        Selection sel = SELECTIONS.get(player.getUUID());
        if (sel == null || System.currentTimeMillis() - sel.time > SELECTION_TTL_MS || sel.dimension != player.level().dimension()) {
            SELECTIONS.remove(player.getUUID());
            describeLink(player, device);
            player.displayClientMessage(Component.translatable("message.fcw.relink_hint"), false);
            return;
        }
        Level level = player.level();
        if (!(level.isLoaded(sel.pos) && level.getBlockEntity(sel.pos) instanceof ComputerBlockEntity computer)
                || !computer.vmId().equals(sel.vmId)) {
            SELECTIONS.remove(player.getUUID());
            player.displayClientMessage(Component.translatable("message.fcw.selection_gone"), true);
            return;
        }
        int radius = radius(device);
        if (computer.getBlockPos().distSqr(device.getBlockPos()) > (double) radius * radius) {
            player.displayClientMessage(Component.translatable("message.fcw.too_far", radius), true);
            return;
        }
        device.linkTo(computer);
        player.displayClientMessage(Component.translatable("message.fcw.linked",
                Component.translatable("device.fcw." + device.kind().id()), pos(computer.getBlockPos())), true);
    }

    public static void autoLink(PeripheralBlockEntity device, @Nullable ServerPlayer placer) {
        Level level = device.getLevel();
        if (level == null) {
            return;
        }
        int radius = radius(device);
        ComputerBlockEntity computer = ComputerTracker.nearest(level, device.getBlockPos(), radius);
        if (computer == null) {
            if (placer != null) {
                placer.displayClientMessage(Component.translatable("message.fcw.no_computer_near", radius), true);
            }
            return;
        }
        device.linkTo(computer);
        if (placer != null) {
            placer.displayClientMessage(Component.translatable("message.fcw.linked",
                    Component.translatable("device.fcw." + device.kind().id()), pos(computer.getBlockPos())), true);
        }
    }

    /** Проекторы и колонки стоят по залу, остальная периферия — у стола. */
    private static int radius(PeripheralBlockEntity device) {
        return switch (device.kind()) {
            case PROJECTOR, SPEAKER -> FcwConfig.SERVER.hallLinkRadius.get();
            default -> FcwConfig.SERVER.linkRadius.get();
        };
    }

    public static void describeLink(ServerPlayer player, PeripheralBlockEntity device) {
        Component kind = Component.translatable("device.fcw." + device.kind().id());
        if (!device.isLinked() || device.computerPos() == null) {
            player.displayClientMessage(Component.translatable("message.fcw.status_unlinked", kind), true);
        } else {
            player.displayClientMessage(Component.translatable("message.fcw.status_linked", kind, pos(device.computerPos())), true);
        }
    }
}
