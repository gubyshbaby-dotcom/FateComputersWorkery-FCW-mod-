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
 * Server -> client: everything the computer GUI shows besides the slots
 * (daemon state, OS images, machine state, error, who controls the machine).
 */
public record ComputerStatusPayload(
        BlockPos pos,
        int daemonStatus,          // DaemonManager.Status ordinal
        Component daemonMessage,
        String accelerator,
        int vmState,               // fcw.control.v1.VmState
        boolean busy,
        boolean wantRunning,
        Component error,
        String controller,
        String agentInfo,
        long uptimeMs,
        String imageId,
        String installedImage,
        int cpus,
        int memoryMb,
        boolean gpu,
        boolean keyboard,
        boolean mouse,
        int monitors,
        boolean speakers,
        List<ImageEntry> images,
        Component imagesError,
        String vmId,
        String hostname,           // name in the guest network (without the DNS zone)
        String ipAddress,          // empty until the guest got an address by DHCP
        String dnsZone,            // "mc"; empty if the daemon has no network
        boolean network,           // this machine gets a network card
        String imageVersion,       // build of the image the machine's disk is based on
        boolean imageUpdateAvailable
) implements CustomPacketPayload {
    public static final Type<ComputerStatusPayload> TYPE =
            new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "computer_status"));

    public record ImageEntry(String id, String name, String version, String desktop, int minMemoryMb,
                             int recommendedMemoryMb, boolean installed, String imageVersion) {
        void write(RegistryFriendlyByteBuf buf) {
            buf.writeUtf(id);
            buf.writeUtf(name);
            buf.writeUtf(version);
            buf.writeUtf(desktop);
            buf.writeVarInt(minMemoryMb);
            buf.writeVarInt(recommendedMemoryMb);
            buf.writeBoolean(installed);
            buf.writeUtf(imageVersion);
        }

        static ImageEntry read(RegistryFriendlyByteBuf buf) {
            return new ImageEntry(buf.readUtf(), buf.readUtf(), buf.readUtf(), buf.readUtf(), buf.readVarInt(), buf.readVarInt(),
                    buf.readBoolean(), buf.readUtf());
        }
    }

    public static final StreamCodec<RegistryFriendlyByteBuf, ComputerStatusPayload> STREAM_CODEC =
            StreamCodec.ofMember(ComputerStatusPayload::write, ComputerStatusPayload::read);

    private void write(RegistryFriendlyByteBuf buf) {
        buf.writeBlockPos(pos);
        buf.writeVarInt(daemonStatus);
        ComponentSerialization.STREAM_CODEC.encode(buf, daemonMessage);
        buf.writeUtf(accelerator);
        buf.writeVarInt(vmState);
        buf.writeBoolean(busy);
        buf.writeBoolean(wantRunning);
        ComponentSerialization.STREAM_CODEC.encode(buf, error);
        buf.writeUtf(controller);
        buf.writeUtf(agentInfo);
        buf.writeVarLong(uptimeMs);
        buf.writeUtf(imageId);
        buf.writeUtf(installedImage);
        buf.writeVarInt(cpus);
        buf.writeVarInt(memoryMb);
        buf.writeBoolean(gpu);
        buf.writeBoolean(keyboard);
        buf.writeBoolean(mouse);
        buf.writeVarInt(monitors);
        buf.writeBoolean(speakers);
        buf.writeVarInt(images.size());
        for (ImageEntry e : images) {
            e.write(buf);
        }
        ComponentSerialization.STREAM_CODEC.encode(buf, imagesError);
        buf.writeUtf(vmId);
        buf.writeUtf(hostname);
        buf.writeUtf(ipAddress);
        buf.writeUtf(dnsZone);
        buf.writeBoolean(network);
        buf.writeUtf(imageVersion);
        buf.writeBoolean(imageUpdateAvailable);
    }

    private static ComputerStatusPayload read(RegistryFriendlyByteBuf buf) {
        BlockPos pos = buf.readBlockPos();
        int daemonStatus = buf.readVarInt();
        Component daemonMessage = ComponentSerialization.STREAM_CODEC.decode(buf);
        String accelerator = buf.readUtf();
        int vmState = buf.readVarInt();
        boolean busy = buf.readBoolean();
        boolean wantRunning = buf.readBoolean();
        Component error = ComponentSerialization.STREAM_CODEC.decode(buf);
        String controller = buf.readUtf();
        String agentInfo = buf.readUtf();
        long uptime = buf.readVarLong();
        String imageId = buf.readUtf();
        String installed = buf.readUtf();
        int cpus = buf.readVarInt();
        int mem = buf.readVarInt();
        boolean gpu = buf.readBoolean();
        boolean keyboard = buf.readBoolean();
        boolean mouse = buf.readBoolean();
        int monitors = buf.readVarInt();
        boolean speakers = buf.readBoolean();
        int n = buf.readVarInt();
        List<ImageEntry> images = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            images.add(ImageEntry.read(buf));
        }
        Component imagesError = ComponentSerialization.STREAM_CODEC.decode(buf);
        String vmId = buf.readUtf();
        String hostname = buf.readUtf();
        String ip = buf.readUtf();
        String zone = buf.readUtf();
        boolean network = buf.readBoolean();
        String imageVersion = buf.readUtf();
        boolean update = buf.readBoolean();
        return new ComputerStatusPayload(pos, daemonStatus, daemonMessage, accelerator, vmState, busy, wantRunning, error,
                controller, agentInfo, uptime, imageId, installed, cpus, mem, gpu, keyboard, mouse, monitors, speakers,
                images, imagesError, vmId, hostname, ip, zone, network, imageVersion, update);
    }

    @Override
    public Type<? extends CustomPacketPayload> type() {
        return TYPE;
    }
}
