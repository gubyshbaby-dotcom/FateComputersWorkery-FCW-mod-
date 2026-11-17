package com.fatecomputers.fcw.production.recipe;

import net.minecraft.core.HolderLookup;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeSerializer;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.crafting.SizedIngredient;

import java.util.List;
import java.util.Optional;

/** STUB (see {@link FcwRecipes}): the record from docs/PRODUCTION.md without matching or a serializer. */
public record MachineRecipe(List<SizedIngredient> inputs, Optional<Ingredient> catalyst,
                            List<ChanceResult> results, int ticks, int energyPerTick,
                            RecipeType<MachineRecipe> type) implements Recipe<MachineRecipeInput> {
    /** Total FE for one run. */
    public long totalEnergy() {
        return (long) ticks * energyPerTick;
    }

    /** Outputs of one run with chances rolled. */
    public List<ItemStack> rollResults(RandomSource random) {
        return maxResults();
    }

    /** Every output at full count regardless of chance. */
    public List<ItemStack> maxResults() {
        return results.stream().map(r -> r.stack().copy()).toList();
    }

    @Override
    public boolean matches(MachineRecipeInput input, Level level) {
        return false;
    }

    @Override
    public ItemStack assemble(MachineRecipeInput input, HolderLookup.Provider registries) {
        return ItemStack.EMPTY;
    }

    @Override
    public boolean canCraftInDimensions(int width, int height) {
        return true;
    }

    @Override
    public ItemStack getResultItem(HolderLookup.Provider registries) {
        return results.isEmpty() ? ItemStack.EMPTY : results.get(0).stack();
    }

    @Override
    public RecipeSerializer<?> getSerializer() {
        throw new UnsupportedOperationException("stub");
    }

    @Override
    public RecipeType<?> getType() {
        return type;
    }
}
