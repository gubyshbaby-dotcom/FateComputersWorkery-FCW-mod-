package com.fatecomputers.fcw.production.item;

import com.fatecomputers.fcw.FcwMod;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.tags.TagKey;
import net.minecraft.world.item.Item;

/** Item tags of the production chain (data/fcw/tags/item, data/c/tags/item). */
public final class ProductionTags {
    // Common tags shared with other mods
    public static final TagKey<Item> STEEL_INGOTS = common("ingots/steel");
    public static final TagKey<Item> SILICON = common("silicon");

    // Chip tiers by lithography generation: compatibility packs ask for "a chip of tier N"
    public static final TagKey<Item> CHIPS_MICRON = fcw("chips/micron");
    public static final TagKey<Item> CHIPS_DUV = fcw("chips/duv");
    public static final TagKey<Item> CHIPS_IMMERSION = fcw("chips/immersion");
    public static final TagKey<Item> CHIPS_EUV = fcw("chips/euv");
    public static final TagKey<Item> CHIPS_ANY = fcw("chips/any");
    // Chips by purpose
    public static final TagKey<Item> CHIPS_CPU = fcw("chips/cpu");
    public static final TagKey<Item> CHIPS_MEMORY = fcw("chips/memory");
    public static final TagKey<Item> CHIPS_GPU = fcw("chips/gpu");
    public static final TagKey<Item> CHIPS_CONTROLLER = fcw("chips/controller");

    public static final TagKey<Item> RETICLES = fcw("reticles");
    public static final TagKey<Item> PATTERNED_WAFERS = fcw("patterned_wafers");

    private ProductionTags() {
    }

    private static TagKey<Item> fcw(String path) {
        return TagKey.create(Registries.ITEM, ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, path));
    }

    private static TagKey<Item> common(String path) {
        return TagKey.create(Registries.ITEM, ResourceLocation.fromNamespaceAndPath("c", path));
    }
}
