package com.fatecomputers.fcw.production.recipe;

import com.fatecomputers.fcw.FcwMod;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.crafting.RecipeSerializer;
import net.minecraft.world.item.crafting.RecipeType;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Supplier;

/** Recipe types and serializers of the production machines (docs/PRODUCTION.md). */
public final class FcwRecipes {
    private static final DeferredRegister<RecipeType<?>> TYPES = DeferredRegister.create(Registries.RECIPE_TYPE, FcwMod.MODID);
    private static final DeferredRegister<RecipeSerializer<?>> SERIALIZERS = DeferredRegister.create(Registries.RECIPE_SERIALIZER, FcwMod.MODID);
    private static final Map<String, Supplier<RecipeType<MachineRecipe>>> BY_NAME = new LinkedHashMap<>();
    private static final Map<String, Supplier<MachineRecipe.Serializer>> SERIALIZER_BY_NAME = new LinkedHashMap<>();

    public static final Supplier<RecipeType<MachineRecipe>> ARC_FURNACE = type("arc_furnace");
    public static final Supplier<RecipeType<MachineRecipe>> CRYSTAL_PULLER = type("crystal_puller");
    public static final Supplier<RecipeType<MachineRecipe>> WAFERING = type("wafering");
    public static final Supplier<RecipeType<MachineRecipe>> LITHOGRAPHY = type("lithography");
    public static final Supplier<RecipeType<MachineRecipe>> CHIP_PACKAGING = type("chip_packaging");
    public static final Supplier<RecipeType<MachineRecipe>> PCB_ASSEMBLY = type("pcb_assembly");

    private FcwRecipes() {
    }

    private static Supplier<RecipeType<MachineRecipe>> type(String name) {
        ResourceLocation id = ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, name);
        Supplier<RecipeType<MachineRecipe>> type = TYPES.register(name, () -> RecipeType.<MachineRecipe>simple(id));
        BY_NAME.put(name, type);
        SERIALIZER_BY_NAME.put(name, SERIALIZERS.register(name, () -> new MachineRecipe.Serializer(type)));
        return type;
    }

    public static void register(IEventBus modBus) {
        TYPES.register(modBus);
        SERIALIZERS.register(modBus);
        NeoForge.EVENT_BUS.register(RecipeSelfTest.class);
    }

    /** Machine recipe types in chain order, by short name ("arc_furnace", ...). */
    public static Map<String, Supplier<RecipeType<MachineRecipe>>> types() {
        return Collections.unmodifiableMap(BY_NAME);
    }

    static RecipeSerializer<MachineRecipe> serializerOf(RecipeType<MachineRecipe> type) {
        for (Map.Entry<String, Supplier<RecipeType<MachineRecipe>>> e : BY_NAME.entrySet()) {
            if (e.getValue().get() == type) {
                return SERIALIZER_BY_NAME.get(e.getKey()).get();
            }
        }
        throw new IllegalStateException("Not a machine recipe type: " + type);
    }
}
