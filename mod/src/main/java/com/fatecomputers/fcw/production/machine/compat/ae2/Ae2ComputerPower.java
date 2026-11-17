package com.fatecomputers.fcw.production.machine.compat.ae2;

import appeng.api.AECapabilities;
import appeng.api.networking.GridHelper;
import appeng.api.networking.IGridNode;
import appeng.api.networking.IGridNodeListener;
import appeng.api.networking.IInWorldGridNodeHost;
import appeng.api.networking.IManagedGridNode;
import appeng.api.util.AECableType;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.block.entity.ComputerPowerSource;
import com.fatecomputers.fcw.registry.ModBlocks;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;

import javax.annotation.Nullable;

/**
 * Applied Energistics 2 для компьютеров: при production.requirePower корпус становится узлом
 * ME-сети (ME-кабель с любой стороны) и пополняет свой буфер FE из энергии сети по курсу AE2 —
 * так же, как порт power у машин ({@link Ae2Compat}). Без AE2 класс не загружается.
 */
final class Ae2ComputerPower {
    private Ae2ComputerPower() {
    }

    static void init(IEventBus modBus) {
        ComputerBlockEntity.setNetworkPowerFactory(Link::new);
        modBus.addListener((RegisterCapabilitiesEvent event) -> event.registerBlock(AECapabilities.IN_WORLD_GRID_NODE_HOST,
                (level, pos, state, be, context) ->
                        be instanceof ComputerBlockEntity pc && pc.networkPower() instanceof Link link ? link : null,
                ModBlocks.computerBlocks()));
    }

    /** Узел сети одного компьютера: стоит в клетке корпуса и открыт со всех сторон. */
    private static final class Link implements ComputerPowerSource, IInWorldGridNodeHost {
        private static final IGridNodeListener<Link> LISTENER = (link, node) -> {
            // узел не сохраняется: он создаётся заново, когда компьютер загружается
        };

        private final IManagedGridNode node;
        private boolean created;

        Link(ComputerBlockEntity computer) {
            node = GridHelper.createManagedNode(this, LISTENER)
                    .setInWorldNode(true)
                    .setIdlePowerUsage(0)
                    .setTagName("fcw")
                    .setVisualRepresentation(new ItemStack(computer.getBlockState().getBlock()));
        }

        @Override
        public void tick(ComputerBlockEntity computer) {
            if (!created) {
                created = true;
                if (computer.getLevel() == null) {
                    return;
                }
                node.create(computer.getLevel(), computer.getBlockPos());
                computer.getLevel().invalidateCapabilities(computer.getBlockPos());
            }
            int want = computer.wantedEnergy();
            if (want > 0) {
                computer.acceptEnergy(Ae2Compat.pull(node, want));
            }
        }

        @Override
        public void remove() {
            node.destroy();
        }

        @Nullable
        @Override
        public IGridNode getGridNode(Direction dir) {
            return node.getNode();
        }

        @Override
        public AECableType getCableConnectionType(Direction dir) {
            return AECableType.SMART;
        }
    }
}
