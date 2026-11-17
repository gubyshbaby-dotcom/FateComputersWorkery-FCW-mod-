package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.ProductionDifficulty;
import com.fatecomputers.fcw.production.machine.construction.ConstructionPlan;
import net.minecraft.ChatFormatting;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The blueprint of a machine ({@code <machine>_blueprint}) and, for old worlds, the machine kit
 * ({@code <machine>_kit}). Right click on the ground lays out a construction site in front of the player,
 * the operator side towards them and centred on the clicked spot, if every cell is free; otherwise it says
 * how many blocks are in the way and shows them.
 *
 * <p>A blueprint is cheap: the price of a machine is its bill of materials, delivered to the site. A kit
 * used to be the whole machine in a box, so nobody is robbed: its site starts with every component
 * already delivered and builds itself.
 */
public class MachineKitItem extends Item {
    private final MachineDef def;
    private final boolean legacyKit;

    public MachineKitItem(Properties properties, MachineDef def, boolean legacyKit) {
        super(properties);
        this.def = def;
        this.legacyKit = legacyKit;
    }

    public MachineDef def() {
        return def;
    }

    /** Where the machine goes for a click at this position by a player looking in this direction. */
    public static BlockPos originFor(MachineDef def, BlockPos clicked, Direction facing) {
        return MachineDef.origin(clicked, facing, def.sx / 2, 0, 0);
    }

    @Override
    public InteractionResult useOn(UseOnContext context) {
        Level level = context.getLevel();
        Player player = context.getPlayer();
        if (level.isClientSide || player == null) {
            return InteractionResult.SUCCESS;
        }
        BlockPos clicked = context.getClickedPos();
        if (!level.getBlockState(clicked).canBeReplaced()) {
            clicked = clicked.relative(context.getClickedFace());
        }
        Direction facing = context.getHorizontalDirection().getOpposite();
        BlockPos origin = originFor(def, clicked, facing);
        List<BlockPos> blockers = MachineStructure.blockers(level, origin, facing, def);
        if (!blockers.isEmpty()) {
            BlockPos first = blockers.get(0);
            player.displayClientMessage(Component.translatable("message.fcw.machine.blocked", blockers.size(),
                    first.getX() + " " + first.getY() + " " + first.getZ(), def.sx, def.sy, def.sz).withStyle(ChatFormatting.RED), false);
            if (player instanceof ServerPlayer sp) {
                MachineStructure.highlight(sp, blockers);
            }
            return InteractionResult.FAIL;
        }
        MachineControllerBlockEntity site = MachineStructure.placeSite(level, origin, facing, def);
        if (site == null) {
            return InteractionResult.FAIL;
        }
        site.beginConstruction(legacyKit, player.getGameProfile().getName());
        level.playSound(null, clicked, SoundEvents.SCAFFOLDING_PLACE, SoundSource.BLOCKS, 1.0f, 0.8f);
        if (!player.getAbilities().instabuild) {
            context.getItemInHand().shrink(1);
        }
        return InteractionResult.CONSUME;
    }

    @Override
    public void appendHoverText(ItemStack stack, TooltipContext context, List<Component> tooltip, TooltipFlag flag) {
        ConstructionPlan plan = ConstructionPlan.of(def);
        ProductionDifficulty difficulty;
        try {
            difficulty = ProductionDifficulty.current();
        } catch (IllegalStateException e) {
            return; // the server config is not there yet (main menu)
        }
        if (legacyKit) {
            tooltip.add(Component.translatable("tooltip.fcw.machine_kit.legacy").withStyle(ChatFormatting.GOLD));
        }
        tooltip.add(Component.translatable("tooltip.fcw.blueprint.size", def.sx, def.sy, def.sz, plan.stages.size(), plan.totalNeed())
                .withStyle(ChatFormatting.GRAY));
        int energy = difficulty.buildEnergyPerTick();
        int minutes = Math.max(1, plan.totalTicks() / 1200);
        tooltip.add((energy > 0 ? Component.translatable("tooltip.fcw.blueprint.time_power", minutes, energy)
                : Component.translatable("tooltip.fcw.blueprint.time", minutes)).withStyle(ChatFormatting.GRAY));
        if (!flag.hasShiftDown()) {
            tooltip.add(Component.translatable("tooltip.fcw.blueprint.shift").withStyle(ChatFormatting.DARK_GRAY));
            return;
        }
        // the whole bill, summed over the stages
        Map<Item, Integer> total = new LinkedHashMap<>();
        for (ConstructionPlan.Stage s : plan.stages) {
            for (ConstructionPlan.Entry e : s.entries()) {
                total.merge(e.item(), e.need(), Integer::sum);
            }
        }
        int shown = 0;
        for (Map.Entry<Item, Integer> e : total.entrySet()) {
            if (shown == 12) {
                tooltip.add(Component.translatable("tooltip.fcw.blueprint.more", total.size() - shown).withStyle(ChatFormatting.DARK_GRAY));
                break;
            }
            tooltip.add(Component.literal(" " + e.getValue() + " × ").append(e.getKey().getDescription()).withStyle(ChatFormatting.DARK_AQUA));
            shown++;
        }
    }
}
