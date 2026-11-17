package com.fatecomputers.fcw.production.recipe;

import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.crafting.RecipeType;

import java.util.function.Supplier;

/**
 * STUB of the recipe API from docs/PRODUCTION.md so that the machines compile on their own branch.
 * The real class (recipe types and serializers, registered) comes from the materials/recipes branch
 * and replaces this file on merge.
 */
public final class FcwRecipes {
    public static Supplier<RecipeType<MachineRecipe>> ARC_FURNACE = stub("arc_furnace");
    public static Supplier<RecipeType<MachineRecipe>> CRYSTAL_PULLER = stub("crystal_puller");
    public static Supplier<RecipeType<MachineRecipe>> WAFERING = stub("wafering");
    public static Supplier<RecipeType<MachineRecipe>> LITHOGRAPHY = stub("lithography");
    public static Supplier<RecipeType<MachineRecipe>> CHIP_PACKAGING = stub("chip_packaging");
    public static Supplier<RecipeType<MachineRecipe>> PCB_ASSEMBLY = stub("pcb_assembly");

    private FcwRecipes() {
    }

    private static Supplier<RecipeType<MachineRecipe>> stub(String name) {
        RecipeType<MachineRecipe> type = RecipeType.simple(ResourceLocation.fromNamespaceAndPath("fcw", name));
        return () -> type;
    }
}
