package com.fatecomputers.fcw.production.recipe;

import com.mojang.serialization.Codec;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.NonNullList;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeSerializer;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.crafting.SizedIngredient;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.function.Supplier;

/**
 * A recipe of a production machine (docs/PRODUCTION.md). One class for all six recipe types;
 * {@link #type()} tells them apart (it comes from the serializer the JSON "type" selected,
 * it is not a JSON field).
 *
 * <pre>{@code
 * { "type": "fcw:arc_furnace",
 *   "inputs": [ {"ingredient": {"item": "minecraft:quartz"}, "count": 4} ],
 *   "catalyst": {"item": "fcw:reticle_cpu"},          // optional, not consumed
 *   "results": [ {"id": "fcw:metallurgical_silicon", "count": 2, "chance": 1.0} ],
 *   "ticks": 400, "energy_per_tick": 256 }
 * }</pre>
 */
public record MachineRecipe(List<SizedIngredient> inputs, Optional<Ingredient> catalyst,
                            List<ChanceResult> results, int ticks, int energyPerTick,
                            RecipeType<MachineRecipe> type) implements Recipe<MachineRecipeInput> {

    public MachineRecipe {
        inputs = List.copyOf(inputs);
        results = List.copyOf(results);
    }

    /** Total FE for one run. */
    public long totalEnergy() {
        return (long) ticks * energyPerTick;
    }

    /** Can this recipe run from these slots? Counts are respected, slot order does not matter. */
    public boolean matches(List<ItemStack> slots, ItemStack catalystStack) {
        if (catalyst.isPresent() && (catalystStack.isEmpty() || !catalyst.get().test(catalystStack))) {
            return false;
        }
        return MachineRecipes.plan(this, slots) != null;
    }

    /** Outputs of one run with chances rolled; outputs that did not play out are dropped. */
    public List<ItemStack> rollResults(RandomSource random) {
        List<ItemStack> out = new ArrayList<>(results.size());
        for (ChanceResult r : results) {
            ItemStack s = r.roll(random);
            if (!s.isEmpty()) {
                out.add(s);
            }
        }
        return out;
    }

    /** Every output at full count regardless of chance: what the output slots must be able to take. */
    public List<ItemStack> maxResults() {
        return results.stream().map(r -> r.stack().copy()).toList();
    }

    @Override
    public boolean matches(MachineRecipeInput input, Level level) {
        return matches(input.inputs(), input.catalyst());
    }

    /** The first output (machines should use {@link #rollResults}). */
    @Override
    public ItemStack assemble(MachineRecipeInput input, HolderLookup.Provider registries) {
        return getResultItem(registries).copy();
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
    public NonNullList<Ingredient> getIngredients() {
        NonNullList<Ingredient> list = NonNullList.create();
        inputs.forEach(i -> list.add(i.ingredient()));
        return list;
    }

    /** Machine recipes are not shown in the vanilla recipe book. */
    @Override
    public boolean isSpecial() {
        return true;
    }

    @Override
    public RecipeSerializer<?> getSerializer() {
        return FcwRecipes.serializerOf(type);
    }

    @Override
    public RecipeType<?> getType() {
        return type;
    }

    /** One serializer per recipe type: the JSON format is the same, the resulting {@link #type()} differs. */
    public static final class Serializer implements RecipeSerializer<MachineRecipe> {
        private final MapCodec<MachineRecipe> codec;
        private final StreamCodec<RegistryFriendlyByteBuf, MachineRecipe> streamCodec;

        public Serializer(Supplier<RecipeType<MachineRecipe>> type) {
            this.codec = RecordCodecBuilder.mapCodec(i -> i.group(
                    SizedIngredient.NESTED_CODEC.listOf(1, 16).fieldOf("inputs").forGetter(MachineRecipe::inputs),
                    Ingredient.CODEC_NONEMPTY.optionalFieldOf("catalyst").forGetter(MachineRecipe::catalyst),
                    ChanceResult.CODEC.listOf(1, 16).fieldOf("results").forGetter(MachineRecipe::results),
                    Codec.intRange(1, Integer.MAX_VALUE).fieldOf("ticks").forGetter(MachineRecipe::ticks),
                    Codec.intRange(0, Integer.MAX_VALUE).optionalFieldOf("energy_per_tick", 0).forGetter(MachineRecipe::energyPerTick)
            ).apply(i, (in, cat, res, ticks, energy) -> new MachineRecipe(in, cat, res, ticks, energy, type.get())));
            this.streamCodec = StreamCodec.composite(
                    SizedIngredient.STREAM_CODEC.apply(ByteBufCodecs.list()), MachineRecipe::inputs,
                    ByteBufCodecs.optional(Ingredient.CONTENTS_STREAM_CODEC), MachineRecipe::catalyst,
                    ChanceResult.STREAM_CODEC.apply(ByteBufCodecs.list()), MachineRecipe::results,
                    ByteBufCodecs.VAR_INT, MachineRecipe::ticks,
                    ByteBufCodecs.VAR_INT, MachineRecipe::energyPerTick,
                    (in, cat, res, ticks, energy) -> new MachineRecipe(in, cat, res, ticks, energy, type.get()));
        }

        @Override
        public MapCodec<MachineRecipe> codec() {
            return codec;
        }

        @Override
        public StreamCodec<RegistryFriendlyByteBuf, MachineRecipe> streamCodec() {
            return streamCodec;
        }
    }
}
