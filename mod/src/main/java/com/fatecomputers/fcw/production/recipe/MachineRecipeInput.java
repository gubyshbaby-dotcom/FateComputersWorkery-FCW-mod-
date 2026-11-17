package com.fatecomputers.fcw.production.recipe;

import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeInput;

import java.util.List;

/** STUB (see {@link FcwRecipes}). */
public record MachineRecipeInput(List<ItemStack> inputs, ItemStack catalyst) implements RecipeInput {
    @Override
    public ItemStack getItem(int index) {
        return inputs.get(index);
    }

    @Override
    public int size() {
        return inputs.size();
    }
}
