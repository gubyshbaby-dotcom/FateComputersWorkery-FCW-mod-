package com.fatecomputers.fcw.network;

import com.fatecomputers.fcw.FcwMod;
import io.netty.buffer.ByteBuf;
import net.minecraft.core.BlockPos;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.ResourceLocation;

/** Client -> server: a button in the computer GUI. */
public record ComputerActionPayload(BlockPos pos, int action, String arg) implements CustomPacketPayload {
    public static final int POWER_ON = 0;
    public static final int POWER_OFF = 1;
    public static final int FORCE_OFF = 2;
    public static final int SELECT_IMAGE = 3;
    public static final int REINSTALL = 4;
    public static final int REFRESH_IMAGES = 5;

    public static final Type<ComputerActionPayload> TYPE =
            new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "computer_action"));

    public static final StreamCodec<ByteBuf, ComputerActionPayload> STREAM_CODEC = StreamCodec.composite(
            BlockPos.STREAM_CODEC, ComputerActionPayload::pos,
            ByteBufCodecs.VAR_INT, ComputerActionPayload::action,
            ByteBufCodecs.stringUtf8(256), ComputerActionPayload::arg,
            ComputerActionPayload::new);

    @Override
    public Type<? extends CustomPacketPayload> type() {
        return TYPE;
    }
}
