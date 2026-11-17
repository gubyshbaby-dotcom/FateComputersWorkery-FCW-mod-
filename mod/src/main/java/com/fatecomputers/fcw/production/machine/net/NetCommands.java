package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.production.machine.shell.MachineShell;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.context.CommandContext;
import net.minecraft.ChatFormatting;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.coordinates.BlockPosArgument;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;

import java.util.List;

/**
 * {@code /fcw net <позиция компьютера>} — какие машины видит компьютер и как (по кабелю или по
 * радио). Для операторов: быстро понять, почему {@code fcw machine list} в госте пуст.
 */
public final class NetCommands {
    private NetCommands() {
    }

    public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("fcw")
                .then(Commands.literal("net").requires(s -> s.hasPermission(2))
                        .then(Commands.argument("computer", BlockPosArgument.blockPos()).executes(NetCommands::show))));
    }

    private static int show(CommandContext<CommandSourceStack> ctx) {
        ServerLevel level = ctx.getSource().getLevel();
        BlockPos pos = BlockPosArgument.getBlockPos(ctx, "computer");
        if (!level.isLoaded(pos) || !(level.getBlockEntity(pos) instanceof ComputerBlockEntity pc)) {
            ctx.getSource().sendFailure(Component.translatable("command.fcw.net.no_computer", pos.toShortString()));
            return 0;
        }
        List<MachineLinks.Link> links = MachineLinks.links(level, pos);
        ctx.getSource().sendSuccess(() -> Component.translatable("command.fcw.net.header", pc.effectiveHostname(), links.size()), false);
        for (MachineLinks.Link l : links) {
            BlockPos at = l.machine().getBlockPos();
            ctx.getSource().sendSuccess(() -> Component.literal(" " + MachineShell.name(l.machine())).withStyle(ChatFormatting.AQUA)
                    .append(Component.literal(" ").withStyle(ChatFormatting.WHITE))
                    .append(Component.translatable("machine.fcw." + l.machine().def().name).withStyle(ChatFormatting.WHITE))
                    .append(Component.literal(" " + l.via().id() + " " + at.toShortString()).withStyle(ChatFormatting.GRAY)), false);
        }
        return links.size();
    }
}
