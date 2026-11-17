package com.fatecomputers.fcw;

import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import com.fatecomputers.fcw.network.ComputerStatusPayload;
import com.fatecomputers.fcw.network.ScreenAccessPayload;

/**
 * Calls from common code into the client. The client entry point installs a real
 * implementation; on a dedicated server everything stays a no-op, so common classes
 * never reference client-only classes directly.
 */
public final class SidedHooks {
    public interface Client {
        default void monitorLoaded(MonitorBlockEntity be) {
        }

        default void monitorUnloaded(MonitorBlockEntity be) {
        }

        default void speakerLoaded(PeripheralBlockEntity be) {
        }

        default void speakerUnloaded(PeripheralBlockEntity be) {
        }

        default void onScreenAccess(ScreenAccessPayload payload) {
        }

        default void onComputerStatus(ComputerStatusPayload payload) {
        }
    }

    private static volatile Client client = new Client() {
    };

    private SidedHooks() {
    }

    public static Client client() {
        return client;
    }

    public static void setClient(Client impl) {
        client = impl;
    }
}
