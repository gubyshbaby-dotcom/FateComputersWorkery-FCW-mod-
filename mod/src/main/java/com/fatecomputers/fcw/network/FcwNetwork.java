package com.fatecomputers.fcw.network;

import com.fatecomputers.fcw.SidedHooks;
import com.fatecomputers.fcw.server.ServerPacketHandler;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;
import net.neoforged.neoforge.network.registration.PayloadRegistrar;

public final class FcwNetwork {
    public static final String VERSION = "1";

    private FcwNetwork() {
    }

    public static void register(RegisterPayloadHandlersEvent event) {
        PayloadRegistrar r = event.registrar(VERSION);
        r.playToServer(ComputerActionPayload.TYPE, ComputerActionPayload.STREAM_CODEC, ServerPacketHandler::onComputerAction);
        r.playToServer(ScreenAccessRequestPayload.TYPE, ScreenAccessRequestPayload.STREAM_CODEC, ServerPacketHandler::onScreenAccessRequest);
        // Client handlers go through SidedHooks so that this class never loads client code on a dedicated server.
        r.playToClient(ScreenAccessPayload.TYPE, ScreenAccessPayload.STREAM_CODEC, (p, ctx) -> SidedHooks.client().onScreenAccess(p));
        r.playToClient(ComputerStatusPayload.TYPE, ComputerStatusPayload.STREAM_CODEC, (p, ctx) -> SidedHooks.client().onComputerStatus(p));
    }
}
