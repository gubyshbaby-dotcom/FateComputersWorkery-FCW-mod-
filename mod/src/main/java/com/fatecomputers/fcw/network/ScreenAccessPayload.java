package com.fatecomputers.fcw.network;

import com.fatecomputers.fcw.FcwMod;
import net.minecraft.core.BlockPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.ResourceLocation;

import java.util.ArrayList;
import java.util.List;

/**
 * Server -> client: answer to {@link ScreenAccessRequestPayload}. On success it carries a one-time
 * token issued by the daemon and where to reach the daemon's QUIC server. An empty host means
 * "the address you used to join this Minecraft server". On failure {@code error} is non-empty.
 */
public record ScreenAccessPayload(
        BlockPos pos,
        int kind,              // KIND_VIEW or KIND_CONTROL
        String vmId,
        String token,
        String host,
        int quicPort,
        String certSha256,
        List<Screen> screens,
        int permissions,       // fcw.control.v1.Permission mask
        Component error
) implements CustomPacketPayload {
    public static final int KIND_VIEW = 0;
    public static final int KIND_CONTROL = 1;

    public static final int PERM_VIEW = 1;
    public static final int PERM_KEYBOARD = 2;
    public static final int PERM_MOUSE = 4;

    public static final Type<ScreenAccessPayload> TYPE =
            new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "screen_access"));

    /** kind: 1 graphical, 2 text (fcw.control.v1.ScreenKind). */
    public record Screen(String id, int kind, int width, int height) {
    }

    public static final StreamCodec<RegistryFriendlyByteBuf, ScreenAccessPayload> STREAM_CODEC =
            StreamCodec.ofMember(ScreenAccessPayload::write, ScreenAccessPayload::read);

    public static ScreenAccessPayload failure(BlockPos pos, int kind, Component error) {
        return new ScreenAccessPayload(pos, kind, "", "", "", 0, "", List.of(), 0, error);
    }

    public boolean ok() {
        return error.getString().isEmpty() && !token.isEmpty();
    }

    private void write(RegistryFriendlyByteBuf buf) {
        buf.writeBlockPos(pos);
        buf.writeVarInt(kind);
        buf.writeUtf(vmId);
        buf.writeUtf(token);
        buf.writeUtf(host);
        buf.writeVarInt(quicPort);
        buf.writeUtf(certSha256);
        buf.writeVarInt(screens.size());
        for (Screen s : screens) {
            buf.writeUtf(s.id);
            buf.writeVarInt(s.kind);
            buf.writeVarInt(s.width);
            buf.writeVarInt(s.height);
        }
        buf.writeVarInt(permissions);
        ComponentSerialization.STREAM_CODEC.encode(buf, error);
    }

    private static ScreenAccessPayload read(RegistryFriendlyByteBuf buf) {
        BlockPos pos = buf.readBlockPos();
        int kind = buf.readVarInt();
        String vmId = buf.readUtf();
        String token = buf.readUtf();
        String host = buf.readUtf();
        int port = buf.readVarInt();
        String cert = buf.readUtf();
        int n = buf.readVarInt();
        List<Screen> screens = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            screens.add(new Screen(buf.readUtf(), buf.readVarInt(), buf.readVarInt(), buf.readVarInt()));
        }
        int perms = buf.readVarInt();
        Component error = ComponentSerialization.STREAM_CODEC.decode(buf);
        return new ScreenAccessPayload(pos, kind, vmId, token, host, port, cert, screens, perms, error);
    }

    @Override
    public Type<? extends CustomPacketPayload> type() {
        return TYPE;
    }
}
