package com.fatecomputers.fcw.production.machine.compat.ae2;

import appeng.api.AECapabilities;
import appeng.api.networking.GridHelper;
import appeng.api.networking.IGridNode;
import appeng.api.networking.IGridNodeListener;
import appeng.api.networking.IInWorldGridNodeHost;
import appeng.api.networking.IManagedGridNode;
import appeng.api.util.AECableType;
import com.fatecomputers.fcw.cinema.Cinema;
import com.fatecomputers.fcw.cinema.ProjectorBlockEntity;
import com.fatecomputers.fcw.cinema.ProjectorPowerSource;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;

import javax.annotation.Nullable;

/**
 * Applied Energistics 2 для проекторов: при production.requirePower проектор становится узлом
 * ME-сети (ME-кабель с любой стороны) и пополняет свой буфер FE из энергии сети по курсу AE2 —
 * так же, как компьютеры ({@link Ae2ComputerPower}). Без AE2 класс не загружается.
 */
final class Ae2ProjectorPower {
    private Ae2ProjectorPower() {
    }

    static void init(IEventBus modBus) {
        ProjectorBlockEntity.setNetworkPowerFactory(Link::new);
        modBus.addListener((RegisterCapabilitiesEvent event) -> event.registerBlock(AECapabilities.IN_WORLD_GRID_NODE_HOST,
                (level, pos, state, be, context) ->
                        be instanceof ProjectorBlockEntity p && p.networkPower() instanceof Link link ? link : null,
                Cinema.PROJECTOR.get()));
    }

    /** Узел сети одного проектора: стоит в его клетке и открыт со всех сторон. */
    private static final class Link implements ProjectorPowerSource, IInWorldGridNodeHost {
        private static final IGridNodeListener<Link> LISTENER = (link, node) -> {
            // узел не сохраняется: он создаётся заново, когда проектор загружается
        };

        private final IManagedGridNode node;
        private boolean created;

        Link(ProjectorBlockEntity projector) {
            node = GridHelper.createManagedNode(this, LISTENER)
                    .setInWorldNode(true)
                    .setIdlePowerUsage(0)
                    .setTagName("fcw")
                    .setVisualRepresentation(new ItemStack(projector.getBlockState().getBlock()));
        }

        @Override
        public void tick(ProjectorBlockEntity projector) {
            if (!created) {
                created = true;
                if (projector.getLevel() == null) {
                    return;
                }
                node.create(projector.getLevel(), projector.getBlockPos());
                projector.getLevel().invalidateCapabilities(projector.getBlockPos());
            }
            int want = projector.wantedEnergy();
            if (want > 0) {
                projector.acceptEnergy(Ae2Compat.pull(node, want));
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
