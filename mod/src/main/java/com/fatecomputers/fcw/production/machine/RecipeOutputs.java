package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.ItemStack;

import java.util.List;

/** The results of a {@link MachineRecipe} as the controller needs them. */
final class RecipeOutputs {
    private RecipeOutputs() {
    }

    /** Everything the recipe can give at most (to reserve room in the output slots). */
    static List<ItemStack> max(MachineRecipe recipe) {
        return recipe.maxResults();
    }

    /** The results of one run, chances rolled. */
    static List<ItemStack> roll(MachineRecipe recipe, RandomSource random) {
        return recipe.rollResults(random);
    }
}
