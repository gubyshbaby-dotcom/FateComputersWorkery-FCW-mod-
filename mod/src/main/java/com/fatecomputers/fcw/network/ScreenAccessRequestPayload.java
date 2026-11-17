package com.fatecomputers.fcw.network;

import com.fatecomputers.fcw.FcwMod;
import io.netty.buffer.ByteBuf;
import net.minecraft.core.BlockPos;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.ResourceLocation;

/**
 * Client -> server: give me access to the screen of the machine behind this monitor/keyboard.
 * {@code control=false}: view only (live monitor picture); {@code control=true}: the player uses
 * the computer (keyboard/mouse rights depend on the linked devices).
 */
public record ScreenAccessRequestPayload(BlockPos pos, boolean control) implements CustomPacketPayload {
    public static final Type<ScreenAccessRequestPayload> TYPE =
            new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "screen_access_request"));

    public static final StreamCodec<ByteBuf, ScreenAccessRequestPayload> STREAM_CODEC = StreamCodec.composite(
            BlockPos.STREAM_CODEC, ScreenAccessRequestPayload::pos,
            ByteBufCodecs.BOOL, ScreenAccessRequestPayload::control,
            ScreenAccessRequestPayload::new);

    @Override
    public Type<? extends CustomPacketPayload> type() {
        return TYPE;
    }
}
