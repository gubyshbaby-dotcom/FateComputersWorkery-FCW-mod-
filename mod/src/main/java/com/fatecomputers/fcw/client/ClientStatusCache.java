package com.fatecomputers.fcw.client;

import com.fatecomputers.fcw.network.ComputerStatusPayload;
import net.minecraft.core.BlockPos;

import javax.annotation.Nullable;

/** Last computer status received for the GUI that is open. */
public final class ClientStatusCache {
    @Nullable
    private static ComputerStatusPayload last;

    private ClientStatusCache() {
    }

    public static void put(ComputerStatusPayload p) {
        last = p;
    }

    @Nullable
    public static ComputerStatusPayload get(BlockPos pos) {
        ComputerStatusPayload p = last;
        return p != null && p.pos().equals(pos) ? p : null;
    }

    public static void clear() {
        last = null;
    }
}
