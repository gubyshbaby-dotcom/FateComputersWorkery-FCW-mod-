package com.fatecomputers.fcw.production.recipe;

import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.Level;

import java.util.List;
import java.util.Optional;

/** STUB (see {@link FcwRecipes}): no recipes are ever found. */
public final class MachineRecipes {
    private MachineRecipes() {
    }

    /** First recipe of this type that can be made from the contents of the input slots. */
    public static Optional<RecipeHolder<MachineRecipe>> find(Level level, RecipeType<MachineRecipe> type,
                                                             List<ItemStack> inputs, ItemStack catalyst) {
        return Optional.empty();
    }

    /** Takes the inputs (and the catalyst's durability) out of the slots. */
    public static void consume(MachineRecipe r, List<ItemStack> inputs, ItemStack catalyst) {
    }

    /** All recipes of a type in lookup order. */
    public static List<RecipeHolder<MachineRecipe>> all(Level level, RecipeType<MachineRecipe> type) {
        return List.of();
    }

    /** True if some recipe of this type uses the stack as an input. */
    public static boolean isInput(Level level, RecipeType<MachineRecipe> type, ItemStack stack) {
        return false;
    }

    /** True if some recipe of this type uses the stack as its catalyst. */
    public static boolean isCatalyst(Level level, RecipeType<MachineRecipe> type, ItemStack stack) {
        return false;
    }
}
