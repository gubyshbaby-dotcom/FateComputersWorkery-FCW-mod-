package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.block.PeripheralKind;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import com.fatecomputers.fcw.menu.ComputerMenu;
import com.fatecomputers.fcw.network.ComputerActionPayload;
import com.fatecomputers.fcw.network.ComputerStatusPayload;
import com.fatecomputers.fcw.network.ScreenAccessPayload;
import com.fatecomputers.fcw.network.ScreenAccessRequestPayload;
import com.fatecomputers.fcw.proto.ControlProto.GetVm;
import com.fatecomputers.fcw.proto.ControlProto.HelloResult;
import com.fatecomputers.fcw.proto.ControlProto.ImageInfo;
import com.fatecomputers.fcw.proto.ControlProto.IssueToken;
import com.fatecomputers.fcw.proto.ControlProto.Request;
import com.fatecomputers.fcw.proto.ControlProto.ScreenInfo;
import com.fatecomputers.fcw.server.daemon.DaemonManager;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.network.PacketDistributor;
import net.neoforged.neoforge.network.handling.IPayloadContext;

import java.util.ArrayList;
import java.util.List;

/** Server side handling of the mod's packets (runs on the server thread). */
public final class ServerPacketHandler {
    private ServerPacketHandler() {
    }

    // ------------------------------------------------------------------ GUI actions

    public static void onComputerAction(ComputerActionPayload p, IPayloadContext ctx) {
        if (!(ctx.player() instanceof ServerPlayer player)) {
            return;
        }
        if (!(player.containerMenu instanceof ComputerMenu menu) || !menu.pos().equals(p.pos())
                || menu.computer() == null || !menu.stillValid(player)) {
            return;
        }
        ComputerBlockEntity computer = menu.computer();
        if (p.action() != ComputerActionPayload.REFRESH_IMAGES && !mayControl(player, computer)) {
            player.displayClientMessage(Component.translatable("message.fcw.owner_only"), true);
            return;
        }
        switch (p.action()) {
            case ComputerActionPayload.POWER_ON -> computer.powerOn(player);
            case ComputerActionPayload.POWER_OFF -> computer.powerOff(false);
            case ComputerActionPayload.FORCE_OFF -> computer.powerOff(true);
            case ComputerActionPayload.SELECT_IMAGE -> computer.selectImage(p.arg(), player);
            case ComputerActionPayload.REINSTALL -> computer.reinstall(p.arg(), player);
            case ComputerActionPayload.REFRESH_IMAGES -> {
                DaemonManager d = DaemonManager.get();
                if (d != null) {
                    d.refreshImages();
                }
            }
            default -> {
            }
        }
        FcwServerEvents.forceStatus(player);
    }

    public static boolean mayControl(ServerPlayer player, ComputerBlockEntity computer) {
        if (player.isSpectator()) {
            return false;
        }
        if (!FcwConfig.SERVER.ownerOnlyControl.get() || computer.owner() == null) {
            return true;
        }
        return computer.owner().equals(player.getUUID()) || player.hasPermissions(2);
    }

    public static ComputerStatusPayload buildStatus(ComputerBlockEntity c) {
        DaemonManager d = DaemonManager.get();
        int daemonStatus = d == null ? DaemonManager.Status.STOPPED.ordinal() : d.status().ordinal();
        Component daemonMessage = d == null ? Component.translatable("daemon.fcw.stopped") : d.reason();
        List<ComputerStatusPayload.ImageEntry> images = new ArrayList<>();
        Component imagesError = Component.empty();
        if (d != null) {
            for (ImageInfo i : d.images()) {
                images.add(new ComputerStatusPayload.ImageEntry(i.getId(), i.getName(), i.getVersion(), i.getDesktop(),
                        i.getMinMemoryMb(), i.getRecommendedMemoryMb(), i.getInstalled(), i.getImageVersion()));
            }
            imagesError = d.imagesError();
        }
        int monitors = (int) c.peripherals().values().stream().filter(k -> k == PeripheralKind.MONITOR).count();
        return new ComputerStatusPayload(c.getBlockPos(), daemonStatus, daemonMessage, d == null ? "" : d.accelerator(),
                c.vmState(), c.isBusy(), c.wantRunning(), c.lastError(), c.controller(), c.agentInfo(), c.uptimeMs(),
                c.imageId(), c.installedImage(), c.effectiveCpus(), c.effectiveMemoryMb(), c.inventory().hasGpu(),
                c.hasPeripheral(PeripheralKind.KEYBOARD), c.hasPeripheral(PeripheralKind.MOUSE), monitors,
                c.hasPeripheral(PeripheralKind.SPEAKER), images, imagesError, c.vmIdString(),
                c.effectiveHostname(), c.ipAddress(), d == null ? "" : d.dnsZone(),
                FcwConfig.SERVER.guestNetwork.get() && (d == null || !DaemonManager.isReady() || d.networkEnabled()),
                c.imageVersion(), c.imageUpdateAvailable(), qemuProblem(d));
    }

    private static Component qemuProblem(@javax.annotation.Nullable DaemonManager d) {
        Component c = d == null ? null : d.qemuProblem();
        return c == null ? Component.empty() : c;
    }

    // ------------------------------------------------------------------ screen access tokens

    public static void onScreenAccessRequest(ScreenAccessRequestPayload p, IPayloadContext ctx) {
        if (!(ctx.player() instanceof ServerPlayer player)) {
            return;
        }
        int kind = p.control() ? ScreenAccessPayload.KIND_CONTROL : ScreenAccessPayload.KIND_VIEW;
        BlockPos pos = p.pos();
        Level level = player.level();
        if (!level.isLoaded(pos) || !(level.getBlockEntity(pos) instanceof PeripheralBlockEntity device)) {
            fail(player, pos, kind, Component.translatable("message.fcw.access.no_device"));
            return;
        }
        PeripheralKind dk = device.kind();
        // Monitors: view and control; keyboards: control; speakers: view (a view token also grants sound).
        boolean allowed = dk == PeripheralKind.MONITOR
                || (p.control() ? dk == PeripheralKind.KEYBOARD : dk == PeripheralKind.SPEAKER);
        if (!allowed) {
            fail(player, pos, kind, Component.translatable("message.fcw.access.no_device"));
            return;
        }
        if (p.control()) {
            if (!player.canInteractWithBlock(pos, 1.0)) {
                fail(player, pos, kind, Component.translatable("message.fcw.access.too_far"));
                return;
            }
        } else {
            double max = FcwConfig.SERVER.viewDistance.get();
            if (player.distanceToSqr(pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5) > max * max) {
                fail(player, pos, kind, Component.translatable("message.fcw.access.too_far"));
                return;
            }
        }
        ComputerBlockEntity computer = device.computer();
        if (computer == null) {
            fail(player, pos, kind, Component.translatable(device.isLinked() ? "message.fcw.access.computer_missing" : "message.fcw.not_linked"));
            return;
        }
        DaemonManager d = DaemonManager.get();
        if (d == null || !DaemonManager.isReady()) {
            fail(player, pos, kind, Component.translatable("gui.fcw.error.daemon",
                    d == null ? Component.translatable("daemon.fcw.stopped") : d.reason()));
            return;
        }
        if (!computer.isMachineOn()) {
            fail(player, pos, kind, Component.translatable("message.fcw.access.computer_off"));
            return;
        }
        int perms = ScreenAccessPayload.PERM_VIEW;
        if (p.control() && mayControl(player, computer)) {
            if (computer.hasPeripheral(PeripheralKind.KEYBOARD)) {
                perms |= ScreenAccessPayload.PERM_KEYBOARD;
            }
            if (computer.hasPeripheral(PeripheralKind.MOUSE)) {
                perms |= ScreenAccessPayload.PERM_MOUSE;
            }
        }
        final int permissions = perms;
        String vmId = computer.vmIdString();
        boolean gpu = computer.inventory().hasGpu();
        d.request(Request.newBuilder().setGetVm(GetVm.newBuilder().setVmId(vmId)).build(), vr -> {
            List<ScreenAccessPayload.Screen> screens = new ArrayList<>();
            if (vr.getOk() && vr.hasVm()) {
                for (ScreenInfo s : vr.getVm().getScreensList()) {
                    screens.add(new ScreenAccessPayload.Screen(s.getScreenId(), s.getKindValue(), s.getWidth(), s.getHeight()));
                }
            }
            if (screens.isEmpty()) {
                // Not reported yet (machine still booting): what the hardware will provide.
                if (gpu) {
                    screens.add(new ScreenAccessPayload.Screen("gfx0", 1, FcwConfig.SERVER.displayWidth.get(), FcwConfig.SERVER.displayHeight.get()));
                }
                screens.add(new ScreenAccessPayload.Screen("tty0", 2, 640, 400));
            }
            d.request(Request.newBuilder().setIssueToken(IssueToken.newBuilder()
                    .setVmId(vmId)
                    .setPlayerUuid(player.getUUID().toString())
                    .setPlayerName(player.getGameProfile().getName())
                    .setPermissions(permissions)
                    .setTtlSeconds(FcwConfig.SERVER.tokenTtlSeconds.get())).build(), tr -> {
                if (player.hasDisconnected()) {
                    return;
                }
                if (!tr.getOk() || !tr.hasToken()) {
                    fail(player, pos, kind, Component.translatable("message.fcw.access.token_failed", tr.getError()));
                    return;
                }
                HelloResult hello = d.hello();
                int port = hello != null && hello.getQuicPort() != 0 ? hello.getQuicPort() : FcwConfig.SERVER.quicPort.get();
                String cert = hello != null ? hello.getCertSha256() : "";
                String host = FcwConfig.SERVER.publicHost.get().trim();
                PacketDistributor.sendToPlayer(player, new ScreenAccessPayload(pos, kind, vmId, tr.getToken().getToken(),
                        host, port, cert, screens, permissions, Component.empty()));
            });
        });
    }

    private static void fail(ServerPlayer player, BlockPos pos, int kind, Component why) {
        PacketDistributor.sendToPlayer(player, ScreenAccessPayload.failure(pos, kind, why));
    }
}
