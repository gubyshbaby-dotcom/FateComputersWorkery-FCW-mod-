package com.fatecomputers.fcw.production.item;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.registry.ModItems;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Rarity;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.List;

/** Materials and intermediate products of the production chain (docs/PRODUCTION.md). */
public final class ProductionItems {
    public static final DeferredRegister.Items ITEMS = DeferredRegister.createItems(FcwMod.MODID);

    /** How many exposures a photomask survives in the lithography scanner. */
    public static final int RETICLE_USES = 64;

    // Arc furnace
    public static final DeferredItem<ProductionItem> METALLURGICAL_SILICON = material("metallurgical_silicon", 64, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> STEEL_INGOT = material("steel_ingot", 64, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> POLYSILICON = material("polysilicon", 64, Rarity.COMMON);
    // Crystal puller, wafering
    public static final DeferredItem<ProductionItem> SILICON_BOULE = material("silicon_boule", 16, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> SILICON_WAFER = material("silicon_wafer", 64, Rarity.COMMON);
    // Boards
    public static final DeferredItem<ProductionItem> PCB_BLANK = material("pcb_blank", 64, Rarity.COMMON);
    // Photomasks: the lithography catalyst, worn by 1 per exposure
    public static final DeferredItem<ProductionItem> RETICLE_CONTROLLER = reticle("reticle_controller", Rarity.COMMON);
    public static final DeferredItem<ProductionItem> RETICLE_MEMORY = reticle("reticle_memory", Rarity.COMMON);
    public static final DeferredItem<ProductionItem> RETICLE_GPU = reticle("reticle_gpu", Rarity.UNCOMMON);
    public static final DeferredItem<ProductionItem> RETICLE_CPU = reticle("reticle_cpu", Rarity.UNCOMMON);
    // Patterned wafers
    public static final DeferredItem<ProductionItem> WAFER_CONTROLLER = material("wafer_controller", 16, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> WAFER_MEMORY = material("wafer_memory", 16, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> WAFER_GPU = material("wafer_gpu", 16, Rarity.UNCOMMON);
    public static final DeferredItem<ProductionItem> WAFER_CPU = material("wafer_cpu", 16, Rarity.UNCOMMON);
    // Packaged chips (processors leave the packaging line as cpu_* right away)
    public static final DeferredItem<ProductionItem> CHIP_CONTROLLER = material("chip_controller", 64, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> CHIP_MEMORY = material("chip_memory", 64, Rarity.COMMON);
    public static final DeferredItem<ProductionItem> CHIP_GPU = material("chip_gpu", 64, Rarity.UNCOMMON);

    public static final List<DeferredItem<ProductionItem>> RETICLES = List.of(RETICLE_CONTROLLER, RETICLE_MEMORY, RETICLE_GPU, RETICLE_CPU);

    private ProductionItems() {
    }

    private static DeferredItem<ProductionItem> material(String name, int stack, Rarity rarity) {
        return add(ITEMS.registerItem(name, ProductionItem::new, new Item.Properties().stacksTo(stack).rarity(rarity)));
    }

    private static DeferredItem<ProductionItem> reticle(String name, Rarity rarity) {
        return add(ITEMS.registerItem(name, ProductionItem::new, new Item.Properties().durability(RETICLE_USES).rarity(rarity)));
    }

    private static DeferredItem<ProductionItem> add(DeferredItem<ProductionItem> item) {
        ModItems.TAB_ORDER.add(item);
        return item;
    }

    public static void register(IEventBus modBus) {
        ITEMS.register(modBus);
    }
}
