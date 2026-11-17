package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.menu.ComputerMenu;
import com.fatecomputers.fcw.network.ComputerStatusPayload;
import com.fatecomputers.fcw.proto.ControlProto.Request;
import com.fatecomputers.fcw.proto.ControlProto.RevokeTokens;
import com.fatecomputers.fcw.server.daemon.DaemonManager;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.event.server.ServerAboutToStartEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.network.PacketDistributor;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/** Game-bus events of the logical server: daemon lifecycle, ticking, GUI status, commands. */
public final class FcwServerEvents {
    private static volatile boolean stopping;
    private static final Map<UUID, Integer> LAST_STATUS = new HashMap<>();
    private static int tick;

    private FcwServerEvents() {
    }

    public static boolean isStopping() {
        return stopping;
    }

    @SubscribeEvent
    public static void onServerAboutToStart(ServerAboutToStartEvent event) {
        stopping = false;
        ComputerTracker.clear();
        LinkManager.clear();
        FcwCommands.reset();
        LAST_STATUS.clear();
        // Starts in the background, in parallel with world loading. Computers loaded before the
        // daemon is ready are synchronised when it becomes ready.
        DaemonManager.start(event.getServer());
    }

    @SubscribeEvent
    public static void onServerStopping(ServerStoppingEvent event) {
        stopping = true;
        DaemonManager.beginShutdown();
    }

    @SubscribeEvent
    public static void onServerStopped(ServerStoppedEvent event) {
        DaemonManager.finishShutdown();
        ComputerTracker.clear();
        LinkManager.clear();
        FcwCommands.reset();
        LAST_STATUS.clear();
    }

    @SubscribeEvent
    public static void onRegisterCommands(RegisterCommandsEvent event) {
        FcwCommands.register(event.getDispatcher());
    }

    @SubscribeEvent
    public static void onLogout(PlayerEvent.PlayerLoggedOutEvent event) {
        LAST_STATUS.remove(event.getEntity().getUUID());
        DaemonManager d = DaemonManager.get();
        if (d != null && DaemonManager.isReady()) {
            d.send(Request.newBuilder().setRevokeTokens(RevokeTokens.newBuilder()
                    .setPlayerUuid(event.getEntity().getUUID().toString())).build());
        }
    }

    @SubscribeEvent
    public static void onServerTick(ServerTickEvent.Post event) {
        DaemonManager d = DaemonManager.get();
        if (d != null) {
            d.tick();
        }
        tick++;
        if (tick % 5 == 0) {
            pushStatus(event.getServer(), tick % 40 == 0);
        }
    }

    /** Sends the computer GUI status to players that have it open, when it changed (and every 2 s). */
    private static void pushStatus(MinecraftServer server, boolean periodic) {
        boolean anyOpen = false;
        for (ServerPlayer player : server.getPlayerList().getPlayers()) {
            if (!(player.containerMenu instanceof ComputerMenu menu) || menu.computer() == null) {
                LAST_STATUS.remove(player.getUUID());
                continue;
            }
            anyOpen = true;
            ComputerBlockEntity c = menu.computer();
            if (periodic) {
                c.refreshInfo();
            }
            DaemonManager d = DaemonManager.get();
            int key = c.statusVersion() * 31 + (d == null ? 0 : d.status().ordinal() * 7 + d.images().hashCode())
                    + c.getBlockPos().hashCode() * 17 + menu.containerId;
            Integer last = LAST_STATUS.get(player.getUUID());
            if (periodic || last == null || last != key) {
                LAST_STATUS.put(player.getUUID(), key);
                ComputerStatusPayload payload = ServerPacketHandler.buildStatus(c);
                PacketDistributor.sendToPlayer(player, payload);
            }
        }
        if (anyOpen && periodic) {
            DaemonManager d = DaemonManager.get();
            if (d != null) {
                d.refreshImagesIfOlder(20_000);
            }
        }
    }

    /** Send the status right away (after a GUI action). */
    public static void forceStatus(ServerPlayer player) {
        LAST_STATUS.remove(player.getUUID());
    }
}
