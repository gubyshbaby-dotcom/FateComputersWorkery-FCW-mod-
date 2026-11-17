package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.proto.ControlProto.Event;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.world.level.Level;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * Server side registry of loaded computers by VM id: routes daemon events to block entities.
 * Only touched from the server thread.
 */
public final class ComputerTracker {
    private static final Map<UUID, ComputerBlockEntity> BY_VM = new HashMap<>();

    private ComputerTracker() {
    }

    public static void add(ComputerBlockEntity be) {
        BY_VM.put(be.vmId(), be);
    }

    public static void remove(ComputerBlockEntity be) {
        BY_VM.remove(be.vmId(), be);
    }

    @Nullable
    public static ComputerBlockEntity get(UUID vmId) {
        return BY_VM.get(vmId);
    }

    @Nullable
    public static ComputerBlockEntity get(String vmId) {
        try {
            return BY_VM.get(UUID.fromString(vmId));
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    public static Collection<ComputerBlockEntity> all() {
        return List.copyOf(BY_VM.values());
    }

    public static void clear() {
        BY_VM.clear();
    }

    /** Nearest loaded computer in the same level within {@code radius} blocks (Euclidean). */
    @Nullable
    public static ComputerBlockEntity nearest(Level level, BlockPos pos, int radius) {
        ComputerBlockEntity best = null;
        double bestDist = (double) radius * radius + 1e-6;
        for (ComputerBlockEntity be : BY_VM.values()) {
            if (be.getLevel() != level || be.isRemoved()) {
                continue;
            }
            double d = be.getBlockPos().distSqr(pos);
            if (d <= bestDist) {
                bestDist = d;
                best = be;
            }
        }
        return best;
    }

    /** Раз в тик сервера: питание компьютеров и таймеры обновления агента. */
    public static void tick() {
        if (BY_VM.isEmpty()) {
            return;
        }
        for (ComputerBlockEntity be : new ArrayList<>(BY_VM.values())) {
            be.serverTick();
        }
    }

    public static void onDaemonReady() {
        for (ComputerBlockEntity be : new ArrayList<>(BY_VM.values())) {
            be.onDaemonReady();
        }
    }

    public static void onDaemonLost(Component reason) {
        for (ComputerBlockEntity be : new ArrayList<>(BY_VM.values())) {
            be.onDaemonLost(reason);
        }
    }

    /** Remote daemon unreachable for a while: machines keep running on the host, state is unknown. */
    public static void onDaemonDisconnected() {
        for (ComputerBlockEntity be : new ArrayList<>(BY_VM.values())) {
            be.onDaemonDisconnected();
        }
    }

    public static void handleEvent(Event e) {
        switch (e.getKindCase()) {
            case VM_STATE -> {
                ComputerBlockEntity be = get(e.getVmState().getVmId());
                if (be != null) {
                    be.onVmStateEvent(e.getVmState().getStateValue(), e.getVmState().getReason());
                }
            }
            case REDSTONE_OUTPUT -> {
                ComputerBlockEntity be = get(e.getRedstoneOutput().getVmId());
                if (be != null) {
                    be.setRedstoneOutput(e.getRedstoneOutput().getSide(), e.getRedstoneOutput().getLevel());
                }
            }
            case AGENT_MESSAGE -> {
                ComputerBlockEntity be = get(e.getAgentMessage().getVmId());
                if (be != null) {
                    be.onAgentMessage(e.getAgentMessage().getJson());
                }
            }
            default -> {
            }
        }
    }
}
