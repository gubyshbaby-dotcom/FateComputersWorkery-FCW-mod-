package com.fatecomputers.fcw.production.machine.console;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.machine.MachineClientHooks;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.shell.MachineShell;
import io.netty.buffer.ByteBuf;
import net.minecraft.core.BlockPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerPlayer;
import net.neoforged.neoforge.network.PacketDistributor;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;
import net.neoforged.neoforge.network.handling.IPayloadContext;
import net.neoforged.neoforge.network.registration.PayloadRegistrar;

import java.util.ArrayList;
import java.util.List;

/**
 * Packets of the terminal console and of the hand-loading hints.
 * <ul>
 * <li>client -> server {@link Command}: one line typed at a terminal; the server runs it through
 *     {@link MachineShell} in the language of that client;</li>
 * <li>server -> client {@link Output}: lines for the console (also opens it), each with at most one
 *     accented column range;</li>
 * <li>server -> client {@link Notice}: why an item was not accepted, shown above the hotbar for two seconds.</li>
 * </ul>
 */
public final class MachineConsoleNet {
    private MachineConsoleNet() {
    }

    public record Command(BlockPos pos, String line) implements CustomPacketPayload {
        public static final Type<Command> TYPE = new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "machine_command"));
        public static final StreamCodec<ByteBuf, Command> STREAM_CODEC = StreamCodec.composite(
                BlockPos.STREAM_CODEC, Command::pos,
                ByteBufCodecs.stringUtf8(256), Command::line,
                Command::new);

        @Override
        public Type<? extends CustomPacketPayload> type() {
            return TYPE;
        }
    }

    /**
     * @param open    open the console of this machine (the lines are its banner)
     * @param prompt  machine name for the prompt
     * @param accents per line: start &lt;&lt; 8 | end, or -1
     */
    public record Output(BlockPos pos, boolean open, String prompt, List<String> lines, List<Integer> accents) implements CustomPacketPayload {
        public static final Type<Output> TYPE = new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "machine_output"));
        public static final StreamCodec<ByteBuf, Output> STREAM_CODEC = StreamCodec.composite(
                BlockPos.STREAM_CODEC, Output::pos,
                ByteBufCodecs.BOOL, Output::open,
                ByteBufCodecs.stringUtf8(64), Output::prompt,
                ByteBufCodecs.stringUtf8(1024).apply(ByteBufCodecs.list(256)), Output::lines,
                ByteBufCodecs.VAR_INT.apply(ByteBufCodecs.list(256)), Output::accents,
                Output::new);

        @Override
        public Type<? extends CustomPacketPayload> type() {
            return TYPE;
        }
    }

    public record Notice(Component text) implements CustomPacketPayload {
        public static final Type<Notice> TYPE = new Type<>(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "machine_notice"));
        public static final StreamCodec<RegistryFriendlyByteBuf, Notice> STREAM_CODEC = StreamCodec.composite(
                ComponentSerialization.STREAM_CODEC, Notice::text, Notice::new);

        @Override
        public Type<? extends CustomPacketPayload> type() {
            return TYPE;
        }
    }

    public static void register(RegisterPayloadHandlersEvent event) {
        PayloadRegistrar r = event.registrar("1");
        r.playToServer(Command.TYPE, Command.STREAM_CODEC, MachineConsoleNet::onCommand);
        // client handlers go through the hooks so that this class never loads client code on a dedicated server
        r.playToClient(Output.TYPE, Output.STREAM_CODEC, (p, ctx) -> MachineClientHooks.consoleOutput(p));
        r.playToClient(Notice.TYPE, Notice.STREAM_CODEC, (p, ctx) -> MachineClientHooks.notice(p.text()));
    }

    private static String lang(ServerPlayer player) {
        return player.clientInformation().language();
    }

    private static Output output(MachineControllerBlockEntity machine, boolean open, List<MachineShell.Line> lines) {
        List<String> text = new ArrayList<>();
        List<Integer> accents = new ArrayList<>();
        for (MachineShell.Line l : lines) {
            text.add(l.text());
            accents.add(l.accentStart() < 0 ? -1 : (l.accentStart() << 8) | l.accentEnd());
        }
        return new Output(machine.getBlockPos(), open, MachineShell.name(machine), text, accents);
    }

    /** Right click on the terminal: the console opens on the player's screen. */
    public static void open(ServerPlayer player, MachineControllerBlockEntity machine) {
        PacketDistributor.sendToPlayer(player, output(machine, true, MachineShell.banner(machine, lang(player))));
    }

    public static void notice(ServerPlayer player, Component text) {
        PacketDistributor.sendToPlayer(player, new Notice(text));
    }

    private static void onCommand(Command payload, IPayloadContext ctx) {
        if (!(ctx.player() instanceof ServerPlayer player) || !player.level().isLoaded(payload.pos())) {
            return;
        }
        if (!(player.level().getBlockEntity(payload.pos()) instanceof MachineControllerBlockEntity machine) || !machine.stillValid(player)) {
            return;
        }
        MachineShell.Rich result = MachineShell.execRich(machine, payload.line(), lang(player), false, player.getGameProfile().getName());
        PacketDistributor.sendToPlayer(player, output(machine, false, result.lines()));
    }
}
