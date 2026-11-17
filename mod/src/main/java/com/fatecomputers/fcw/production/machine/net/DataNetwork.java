package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.ComputerBlock;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import com.fatecomputers.fcw.production.machine.euv.EuvControllerBlock;
import com.fatecomputers.fcw.production.machine.euv.EuvKitBlock;
import com.fatecomputers.fcw.registry.ModItems;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.MapColor;
import net.minecraft.world.level.material.PushReaction;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;

/**
 * Сеть данных: кабель {@code fcw:data_cable} и роутер {@code fcw:router}. По ней компьютер видит
 * производственные машины (docs/MACHINE_CONTROL.md). У сети нет блок-сущностей и своего состояния:
 * её каждый раз обходит {@link MachineLinks}.
 */
public final class DataNetwork {
    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks(FcwMod.MODID);
    private static final DeferredRegister.Items ITEMS = DeferredRegister.createItems(FcwMod.MODID);

    public static final DeferredBlock<DataCableBlock> DATA_CABLE = BLOCKS.registerBlock("data_cable", DataCableBlock::new,
            BlockBehaviour.Properties.of()
                    .mapColor(MapColor.COLOR_BLACK)
                    .strength(0.4f, 2.0f)
                    .sound(SoundType.WOOL)
                    .noOcclusion()
                    .isRedstoneConductor((s, l, p) -> false)
                    .isSuffocating((s, l, p) -> false)
                    .isViewBlocking((s, l, p) -> false)
                    .pushReaction(PushReaction.DESTROY));

    public static final DeferredBlock<RouterBlock> ROUTER = BLOCKS.registerBlock("router", RouterBlock::new,
            BlockBehaviour.Properties.of()
                    .mapColor(MapColor.QUARTZ)
                    .strength(1.0f, 6.0f)
                    .sound(SoundType.METAL)
                    .noOcclusion()
                    .isRedstoneConductor((s, l, p) -> false)
                    .isSuffocating((s, l, p) -> false)
                    .isViewBlocking((s, l, p) -> false)
                    .pushReaction(PushReaction.BLOCK));

    public static final DeferredItem<BlockItem> DATA_CABLE_ITEM =
            ITEMS.registerItem("data_cable", p -> new BlockItem(DATA_CABLE.get(), p), new Item.Properties());
    public static final DeferredItem<BlockItem> ROUTER_ITEM =
            ITEMS.registerItem("router", p -> new BlockItem(ROUTER.get(), p), new Item.Properties());

    private DataNetwork() {
    }

    public static void register(IEventBus modBus) {
        BLOCKS.register(modBus);
        ITEMS.register(modBus);
        ModItems.TAB_ORDER.add(DATA_CABLE_ITEM);
        ModItems.TAB_ORDER.add(ROUTER_ITEM);
        NeoForge.EVENT_BUS.addListener((RegisterCommandsEvent e) -> NetCommands.register(e.getDispatcher()));
    }

    /** Узел сети: по нему обход идёт дальше. */
    public static boolean isNode(BlockState state) {
        Block b = state.getBlock();
        return b instanceof DataCableBlock || b instanceof RouterBlock;
    }

    public static boolean isRouter(BlockState state) {
        return state.getBlock() instanceof RouterBlock;
    }

    /**
     * К чему тянется кабель: узлы сети, корпуса компьютеров, клетки машин и блоки заготовки литографа
     * (после сборки они становятся клетками машины, кабель к ним уже подведён).
     */
    public static boolean connectsTo(BlockState state) {
        Block b = state.getBlock();
        if (b instanceof MachineCellBlock cell) {
            return cell.cell(state) != null;
        }
        return isNode(state) || b instanceof ComputerBlock || b instanceof EuvKitBlock || b instanceof EuvControllerBlock;
    }
}
