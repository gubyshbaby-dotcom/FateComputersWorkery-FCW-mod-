package com.fatecomputers.fcw.registry;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.ComputerBlock;
import com.fatecomputers.fcw.block.PeripheralBlock;
import com.fatecomputers.fcw.item.DeviceBlockItem;
import com.fatecomputers.fcw.item.HardwareItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Rarity;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.ArrayList;
import java.util.List;

public final class ModItems {
    public static final DeferredRegister.Items ITEMS = DeferredRegister.createItems(FcwMod.MODID);

    /** Creative tab order. */
    public static final List<DeferredItem<? extends Item>> TAB_ORDER = new ArrayList<>();

    static {
        for (DeferredBlock<ComputerBlock> b : ModBlocks.COMPUTERS) {
            TAB_ORDER.add(ITEMS.registerItem(b.getId().getPath(), p -> new DeviceBlockItem(b.get(), p, null), new Item.Properties()));
        }
        for (DeferredBlock<? extends PeripheralBlock> b : ModBlocks.PERIPHERALS) {
            TAB_ORDER.add(ITEMS.registerItem(b.getId().getPath(), p -> new DeviceBlockItem(b.get(), p, b.get().kind()), new Item.Properties()));
        }
    }

    // Parts. motherboard and graphics_card use their block models as item models; no blocks are registered.
    public static final DeferredItem<HardwareItem> MOTHERBOARD = part("motherboard", HardwareItem.Type.MOTHERBOARD, 0, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> GRAPHICS_CARD = part("graphics_card", HardwareItem.Type.GPU, 0, Rarity.UNCOMMON);

    // CPUs: value = vCPUs (capped by the host's cores).
    public static final DeferredItem<HardwareItem> CPU_DIP = part("cpu_dip", HardwareItem.Type.CPU, 1, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> CPU_QFP = part("cpu_qfp", HardwareItem.Type.CPU, 1, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> CPU_PGA = part("cpu_pga", HardwareItem.Type.CPU, 2, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> CPU_LGA = part("cpu_lga", HardwareItem.Type.CPU, 2, Rarity.UNCOMMON);
    public static final DeferredItem<HardwareItem> CPU_MULTICORE = part("cpu_multicore", HardwareItem.Type.CPU, 4, Rarity.RARE);
    public static final DeferredItem<HardwareItem> CPU_SERVER = part("cpu_server", HardwareItem.Type.CPU, 8, Rarity.EPIC);

    // Memory: value = MB, summed over the slots.
    public static final DeferredItem<HardwareItem> RAM_DIP = part("ram_dip", HardwareItem.Type.RAM, 256, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> RAM_SIMM = part("ram_simm", HardwareItem.Type.RAM, 512, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> RAM_DIMM = part("ram_dimm", HardwareItem.Type.RAM, 1024, Rarity.COMMON);
    public static final DeferredItem<HardwareItem> RAM_DDR = part("ram_ddr", HardwareItem.Type.RAM, 2048, Rarity.UNCOMMON);
    public static final DeferredItem<HardwareItem> RAM_DDR_RGB = part("ram_ddr_rgb", HardwareItem.Type.RAM, 4096, Rarity.RARE);
    public static final DeferredItem<HardwareItem> RAM_SERVER = part("ram_server", HardwareItem.Type.RAM, 8192, Rarity.EPIC);
    public static final DeferredItem<HardwareItem> RAM_SODIMM = part("ram_sodimm", HardwareItem.Type.RAM, 2048, Rarity.UNCOMMON);

    private ModItems() {
    }

    private static DeferredItem<HardwareItem> part(String name, HardwareItem.Type type, int value, Rarity rarity) {
        DeferredItem<HardwareItem> item = ITEMS.registerItem(name, p -> new HardwareItem(p, type, value), new Item.Properties().stacksTo(16).rarity(rarity));
        TAB_ORDER.add(item);
        return item;
    }
}
