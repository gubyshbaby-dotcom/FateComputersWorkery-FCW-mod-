package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.machine.euv.EuvPattern;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.commands.SharedSuggestionProvider;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.block.Block;

import java.util.List;

/**
 * Operator commands for map makers and tests (they join the /fcw command of the mod):
 * <pre>
 * /fcw machine place &lt;name&gt;   puts a machine in front of the player, like its kit does
 * /fcw machine blank          lays out the blank of the EUV scanner from the building blocks
 * </pre>
 */
final class MachineCommands {
    private MachineCommands() {
    }

    static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("fcw")
                .then(Commands.literal("machine").requires(s -> s.hasPermission(2))
                        .then(Commands.literal("place")
                                .then(Commands.argument("name", StringArgumentType.word())
                                        .suggests((c, b) -> SharedSuggestionProvider.suggest(Machines.ALL.keySet(), b))
                                        .executes(MachineCommands::place)))
                        .then(Commands.literal("blank").executes(MachineCommands::blank))));
    }

    /** Two blocks in front of the player, operator side towards them. */
    private static BlockPos spot(ServerPlayer player) {
        return player.blockPosition().relative(player.getDirection(), 2);
    }

    private static int place(CommandContext<CommandSourceStack> ctx) throws CommandSyntaxException {
        ServerPlayer player = ctx.getSource().getPlayerOrException();
        MachineDef def = Machines.byName(StringArgumentType.getString(ctx, "name"));
        if (def == null) {
            ctx.getSource().sendFailure(Component.translatable("command.fcw.machine.unknown"));
            return 0;
        }
        ServerLevel level = player.serverLevel();
        Direction facing = player.getDirection().getOpposite();
        BlockPos origin = MachineKitItem.originFor(def, spot(player), facing);
        List<BlockPos> blockers = MachineStructure.blockers(level, origin, facing, def);
        if (!blockers.isEmpty()) {
            BlockPos first = blockers.get(0);
            ctx.getSource().sendFailure(Component.translatable("message.fcw.machine.blocked", blockers.size(),
                    first.getX() + " " + first.getY() + " " + first.getZ(), def.sx, def.sy, def.sz));
            MachineStructure.highlight(player, blockers);
            return 0;
        }
        MachineStructure.place(level, origin, facing, def);
        ctx.getSource().sendSuccess(() -> Component.translatable("command.fcw.machine.placed", Component.translatable("machine.fcw." + def.name)), true);
        return 1;
    }

    private static int blank(CommandContext<CommandSourceStack> ctx) throws CommandSyntaxException {
        ServerPlayer player = ctx.getSource().getPlayerOrException();
        MachineDef def = Machines.EUV_SCANNER;
        ServerLevel level = player.serverLevel();
        Direction facing = player.getDirection().getOpposite();
        BlockPos origin = MachineKitItem.originFor(def, spot(player), facing);
        List<BlockPos> blockers = MachineStructure.blockers(level, origin, facing, def);
        if (!blockers.isEmpty()) {
            BlockPos first = blockers.get(0);
            ctx.getSource().sendFailure(Component.translatable("message.fcw.machine.blocked", blockers.size(),
                    first.getX() + " " + first.getY() + " " + first.getZ(), def.sx, def.sy, def.sz));
            MachineStructure.highlight(player, blockers);
            return 0;
        }
        for (MachineDef.Cell cell : def.cells) {
            level.setBlock(MachineDef.worldPos(origin, facing, cell), EuvPattern.get().stateAt(cell.x(), cell.y(), cell.z(), facing), Block.UPDATE_CLIENTS);
        }
        ctx.getSource().sendSuccess(() -> Component.translatable("command.fcw.machine.blank"), true);
        return def.cells.size();
    }
}
