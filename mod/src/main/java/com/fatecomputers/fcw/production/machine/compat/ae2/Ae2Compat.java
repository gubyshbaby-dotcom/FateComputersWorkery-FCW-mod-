package com.fatecomputers.fcw.production.machine.compat.ae2;

import appeng.api.AECapabilities;
import appeng.api.config.Actionable;
import appeng.api.config.PowerMultiplier;
import appeng.api.config.PowerUnit;
import appeng.api.networking.GridHelper;
import appeng.api.networking.IGridNode;
import appeng.api.networking.IGridNodeListener;
import appeng.api.networking.IInWorldGridNodeHost;
import appeng.api.networking.IManagedGridNode;
import appeng.api.util.AECableType;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachinePowerSource;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;

import javax.annotation.Nullable;
import java.util.EnumSet;

/**
 * Applied Energistics 2: a machine's power port is a node of the ME network. Connect an ME cable to
 * the port; with production.requirePower on, a working machine fills its FE buffer from the network's
 * energy at AE2's rate (1 AE = 2 FE by default).
 *
 * <p>This class is touched only when AE2 is installed ({@link ProductionMachines#register}); nothing
 * else in the mod refers to AE2 classes.
 */
public final class Ae2Compat {
    public static final String MODID = "ae2";

    private Ae2Compat() {
    }

    public static void init(IEventBus modBus) {
        MachineControllerBlockEntity.setNetworkPowerFactory(Link::new);
        Ae2ComputerPower.init(modBus);
        modBus.addListener((RegisterCapabilitiesEvent event) -> event.registerBlock(AECapabilities.IN_WORLD_GRID_NODE_HOST,
                (level, pos, state, be, context) -> {
                    MachineCellBlock block = (MachineCellBlock) state.getBlock();
                    MachineDef.Cell cell = block.cell(state);
                    MachineDef.Port port = block.def().port("power");
                    if (cell == null || port == null || port.ax() != cell.x() || port.ay() != cell.y() || port.az() != cell.z()) {
                        return null;
                    }
                    MachineControllerBlockEntity controller = block.controller(level, pos, state);
                    return controller != null && controller.networkPower() instanceof Link link ? link : null;
                }, ProductionMachines.cellBlocks()));
    }

    /** Takes up to {@code wantFe} from the node's ME network at AE2's rate; returns the FE actually taken. */
    static int pull(IManagedGridNode node, int wantFe) {
        int[] fe = {0};
        node.ifPresent(grid -> {
            double ae = PowerUnit.FE.convertTo(PowerUnit.AE, wantFe);
            double got = grid.getEnergyService().extractAEPower(ae, Actionable.MODULATE, PowerMultiplier.ONE);
            fe[0] = (int) Math.floor(PowerUnit.AE.convertTo(PowerUnit.FE, got) + 1e-6);
        });
        return fe[0];
    }

    /** The grid node of one machine, standing in its power port cell and open on the port's face. */
    private static final class Link implements MachinePowerSource, IInWorldGridNodeHost {
        private static final IGridNodeListener<Link> LISTENER = (link, node) -> {
            // nothing of the node is saved: it is made anew when the machine loads
        };

        private final IManagedGridNode node;
        @Nullable
        private Direction face;
        private boolean created;

        Link(MachineControllerBlockEntity machine) {
            node = GridHelper.createManagedNode(this, LISTENER)
                    .setInWorldNode(true)
                    .setIdlePowerUsage(0)
                    .setTagName("fcw")
                    .setVisualRepresentation(ProductionMachines.sourceItem(machine.def()));
        }

        @Override
        public void tick(MachineControllerBlockEntity machine) {
            if (!created) {
                created = true;
                MachineDef.Port port = machine.def().port("power");
                if (port == null || machine.getLevel() == null) {
                    return;
                }
                face = MachineDef.rotate(machine.facing(), port.face());
                BlockPos pos = MachineDef.worldPos(machine.origin(), machine.facing(), port.ax(), port.ay(), port.az());
                node.setExposedOnSides(EnumSet.of(face));
                node.create(machine.getLevel(), pos);
                machine.getLevel().invalidateCapabilities(pos);
            }
            int want = machine.wantedEnergy();
            if (want <= 0) {
                return;
            }
            machine.acceptEnergy(pull(node, want));
        }

        @Override
        public void remove() {
            node.destroy();
        }

        @Nullable
        @Override
        public IGridNode getGridNode(Direction dir) {
            return dir == face ? node.getNode() : null;
        }

        @Override
        public AECableType getCableConnectionType(Direction dir) {
            return AECableType.SMART;
        }
    }
}
