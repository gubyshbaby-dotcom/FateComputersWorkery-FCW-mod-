package com.fatecomputers.fcw.production.recipe;

import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeInput;

import java.util.List;

/** Contents of a machine's input slots plus its catalyst slot (index {@code inputs.size()}). */
public record MachineRecipeInput(List<ItemStack> inputs, ItemStack catalyst) implements RecipeInput {
    @Override
    public ItemStack getItem(int index) {
        return index == inputs.size() ? catalyst : inputs.get(index);
    }

    @Override
    public int size() {
        return inputs.size() + 1;
    }

    @Override
    public boolean isEmpty() {
        return catalyst.isEmpty() && inputs.stream().allMatch(ItemStack::isEmpty);
    }
}
