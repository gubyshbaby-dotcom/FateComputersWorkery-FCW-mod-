package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import com.fatecomputers.fcw.production.machine.block.MachineHatchBlock;
import com.fatecomputers.fcw.production.machine.block.MachinePartBlock;
import com.fatecomputers.fcw.production.machine.euv.EuvControllerBlock;
import com.fatecomputers.fcw.production.machine.euv.EuvKitBlock;
import com.fatecomputers.fcw.registry.ModItems;
import net.minecraft.core.Direction;
import net.minecraft.core.particles.ParticleType;
import net.minecraft.core.particles.SimpleParticleType;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.material.MapColor;
import net.minecraft.world.level.material.PushReaction;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Registration of the production machines: cell blocks {@code <machine>_part} / {@code <machine>_hatch},
 * the controller block entity, the terminal menu, the kits, the building blocks of the EUV scanner,
 * particle types and the capabilities of the ports. One call from {@link FcwMod}.
 */
public final class ProductionMachines {
    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks(FcwMod.MODID);
    private static final DeferredRegister.Items ITEMS = DeferredRegister.createItems(FcwMod.MODID);
    private static final DeferredRegister<BlockEntityType<?>> BLOCK_ENTITIES = DeferredRegister.create(Registries.BLOCK_ENTITY_TYPE, FcwMod.MODID);
    private static final DeferredRegister<ParticleType<?>> PARTICLES = DeferredRegister.create(Registries.PARTICLE_TYPE, FcwMod.MODID);

    private static final Map<String, DeferredBlock<MachinePartBlock>> PARTS = new LinkedHashMap<>();
    private static final Map<String, DeferredBlock<MachineHatchBlock>> HATCHES = new LinkedHashMap<>();
    private static final Map<String, DeferredItem<MachineKitItem>> KITS = new LinkedHashMap<>();
    /** Building blocks of the EUV scanner by name (euv_casing, ...), in the order of the parts kit. */
    public static final Map<String, DeferredBlock<? extends Block>> EUV_BLOCKS = new LinkedHashMap<>();
    /** Sprite sets of the machines: furnace_spark, puller_smoke, ... (assets/fcw/particles). */
    public static final Map<String, DeferredHolder<ParticleType<?>, SimpleParticleType>> PARTICLE_TYPES = new LinkedHashMap<>();
    private static final List<DeferredItem<? extends Item>> TAB_ITEMS = new ArrayList<>();

    static {
        for (MachineDef def : Machines.ALL.values()) {
            PARTS.put(def.name, BLOCKS.register(def.name + "_part", () -> MachinePartBlock.create(cellProperties(), def)));
            HATCHES.put(def.name, BLOCKS.register(def.name + "_hatch", () -> MachineHatchBlock.create(cellProperties(), def)));
            if (!def.name.equals(Machines.EUV)) {
                DeferredItem<MachineKitItem> kit = ITEMS.registerItem(def.name + "_kit", p -> new MachineKitItem(p, def), new Item.Properties().stacksTo(1));
                KITS.put(def.name, kit);
                TAB_ITEMS.add(kit);
            }
            if (!def.spec.particlePrefix().isEmpty()) {
                for (String sprite : List.of("spark", "ember", "smoke", "flash")) {
                    String id = def.spec.particlePrefix() + "_" + sprite;
                    PARTICLE_TYPES.put(id, PARTICLES.register(id, () -> new SimpleParticleType(false)));
                }
            }
        }
        euv("euv_casing", EuvKitBlock.Facing.NONE);
        euv("euv_casing_glass", EuvKitBlock.Facing.NONE);
        euv("euv_casing_vent", EuvKitBlock.Facing.NONE);
        euv("euv_frame", EuvKitBlock.Facing.NONE);
        euv("euv_vacuum_chamber", EuvKitBlock.Facing.NONE);
        euv("euv_mirror", EuvKitBlock.Facing.NONE);
        euv("euv_collector", EuvKitBlock.Facing.NONE);
        euv("euv_droplet_generator", EuvKitBlock.Facing.NONE);
        euv("euv_laser_amplifier", EuvKitBlock.Facing.NONE);
        euv("euv_chiller", EuvKitBlock.Facing.NONE);
        euv("euv_wafer_stage", EuvKitBlock.Facing.NONE);
        euv("euv_reticle_stage", EuvKitBlock.Facing.NONE);
        euv("euv_wafer_handler", EuvKitBlock.Facing.NONE);
        euv("euv_load_port", EuvKitBlock.Facing.HORIZONTAL);
        euv("euv_vacuum_pump", EuvKitBlock.Facing.NONE);
        euv("euv_power_unit", EuvKitBlock.Facing.HORIZONTAL);
        euv("euv_controller", null);
        euv("euv_port_energy", EuvKitBlock.Facing.ALL);
        euv("euv_port_coolant", EuvKitBlock.Facing.ALL);
        euv("euv_port_gas", EuvKitBlock.Facing.ALL);
        euv("euv_port_exhaust", EuvKitBlock.Facing.ALL);
        euv("euv_port_data", EuvKitBlock.Facing.ALL);
        euv("euv_port_item", EuvKitBlock.Facing.ALL);
    }

    @SuppressWarnings("DataFlowIssue")
    public static final DeferredHolder<BlockEntityType<?>, BlockEntityType<MachineControllerBlockEntity>> CONTROLLER =
            BLOCK_ENTITIES.register("machine_controller", () -> BlockEntityType.Builder.of(MachineControllerBlockEntity::new, cellBlocks()).build(null));

    private ProductionMachines() {
    }

    public static void register(IEventBus modBus) {
        BLOCKS.register(modBus);
        ITEMS.register(modBus);
        BLOCK_ENTITIES.register(modBus);
        modBus.addListener(com.fatecomputers.fcw.production.machine.console.MachineConsoleNet::register);
        NeoForge.EVENT_BUS.addListener(ProductionMachines::onRightClickBlock);
        PARTICLES.register(modBus);
        modBus.addListener(ProductionMachines::registerCapabilities);
        NeoForge.EVENT_BUS.addListener((RegisterCommandsEvent e) -> MachineCommands.register(e.getDispatcher()));
        if (net.neoforged.fml.ModList.get().isLoaded("ae2")) {
            com.fatecomputers.fcw.production.machine.compat.ae2.Ae2Compat.init(modBus);
        }
        ModItems.TAB_ORDER.addAll(TAB_ITEMS);
    }

    /**
     * Sneaking with something in the other hand, vanilla would skip the block and use the item; the cells
     * of a machine still get the click (sneak + empty main hand takes everything from a zone).
     */
    private static void onRightClickBlock(net.neoforged.neoforge.event.entity.player.PlayerInteractEvent.RightClickBlock event) {
        if (event.getEntity().isShiftKeyDown() && event.getEntity().getMainHandItem().isEmpty()
                && event.getLevel().getBlockState(event.getPos()).getBlock() instanceof MachineCellBlock) {
            event.setUseBlock(net.neoforged.neoforge.common.util.TriState.TRUE);
        }
    }

    private static BlockBehaviour.Properties cellProperties() {
        return BlockBehaviour.Properties.of()
                .mapColor(MapColor.METAL)
                .strength(3.0f, 1200.0f)
                .sound(SoundType.METAL)
                .noOcclusion()
                .noLootTable()
                .isRedstoneConductor((s, l, p) -> false)
                .isSuffocating((s, l, p) -> false)
                .isViewBlocking((s, l, p) -> false)
                .isValidSpawn((s, l, p, e) -> false)
                .lightLevel(s -> switch (s.getValue(MachineCellBlock.STATE)) {
                    case OFF -> 0;
                    case IDLE -> 5;
                    case RUN -> 9;
                })
                .pushReaction(PushReaction.BLOCK);
    }

    private static void euv(String name, @Nullable EuvKitBlock.Facing facing) {
        BlockBehaviour.Properties props = BlockBehaviour.Properties.of()
                .mapColor(MapColor.QUARTZ)
                .strength(2.5f, 6.0f)
                .sound(SoundType.METAL)
                .requiresCorrectToolForDrops();
        DeferredBlock<? extends Block> block = facing == null
                ? BLOCKS.register(name, () -> EuvControllerBlock.create(props.noOcclusion()))
                : BLOCKS.register(name, () -> EuvKitBlock.create(props, facing));
        EUV_BLOCKS.put(name, block);
        TAB_ITEMS.add(ITEMS.registerItem(name, p -> new BlockItem(block.get(), p), new Item.Properties()));
    }

    public static MachinePartBlock part(MachineDef def) {
        return PARTS.get(def.name).get();
    }

    public static MachineHatchBlock hatch(MachineDef def) {
        return HATCHES.get(def.name).get();
    }

    /** What a machine is made from: its kit, or the controller block for the EUV scanner. */
    public static ItemStack sourceItem(MachineDef def) {
        DeferredItem<MachineKitItem> kit = KITS.get(def.name);
        return kit != null ? new ItemStack(kit.get()) : new ItemStack(EUV_BLOCKS.get("euv_controller").get());
    }

    public static Block[] cellBlocks() {
        List<Block> list = new ArrayList<>();
        PARTS.values().forEach(b -> list.add(b.get()));
        HATCHES.values().forEach(b -> list.add(b.get()));
        return list.toArray(Block[]::new);
    }

    /**
     * Ports: a capability is offered only by the cell and through the face the manifest names
     * (or to callers that ask without a side).
     */
    private static void registerCapabilities(RegisterCapabilitiesEvent event) {
        Block[] cells = cellBlocks();
        event.registerBlock(Capabilities.ItemHandler.BLOCK, (level, pos, state, be, side) -> {
            MachineDef.Port port = portAt(state, side, "item");
            MachineControllerBlockEntity c = port == null ? null : ((MachineCellBlock) state.getBlock()).controller(level, pos, state);
            return c == null ? null : c.itemHandler(port.id());
        }, cells);
        event.registerBlock(Capabilities.EnergyStorage.BLOCK, (level, pos, state, be, side) -> {
            MachineDef.Port port = portAt(state, side, "energy");
            MachineControllerBlockEntity c = port == null ? null : ((MachineCellBlock) state.getBlock()).controller(level, pos, state);
            return c == null ? null : c.energy();
        }, cells);
        event.registerBlock(Capabilities.FluidHandler.BLOCK, (level, pos, state, be, side) -> {
            MachineDef.Port port = portAt(state, side, "fluid");
            MachineControllerBlockEntity c = port == null ? null : ((MachineCellBlock) state.getBlock()).controller(level, pos, state);
            return c == null ? null : c.tank(port.id());
        }, cells);
    }

    @Nullable
    private static MachineDef.Port portAt(net.minecraft.world.level.block.state.BlockState state, @Nullable Direction side, String kind) {
        MachineCellBlock block = (MachineCellBlock) state.getBlock();
        MachineDef.Cell cell = block.cell(state);
        if (cell == null) {
            return null;
        }
        Direction facing = state.getValue(MachineCellBlock.FACING);
        for (MachineDef.Port p : block.def().ports) {
            if (p.kind().equals(kind) && p.ax() == cell.x() && p.ay() == cell.y() && p.az() == cell.z()
                    && (side == null || side == MachineDef.rotate(facing, p.face()))) {
                return p;
            }
        }
        return null;
    }
}
