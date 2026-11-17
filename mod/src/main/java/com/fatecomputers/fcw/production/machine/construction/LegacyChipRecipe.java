package com.fatecomputers.fcw.production.machine.construction;

import com.fatecomputers.fcw.production.ProductionDifficulty;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.core.HolderLookup;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.CraftingBookCategory;
import net.minecraft.world.item.crafting.CraftingInput;
import net.minecraft.world.item.crafting.CustomRecipe;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.RecipeSerializer;
import net.minecraft.world.level.Level;

import java.util.Optional;

/**
 * Crafting recipe {@code fcw:legacy_chip}: bench chips stand in for a scanner chip, so that the first
 * scanner can be built before any scanner exists (docs/design/MACHINE_BUILD.md, "Первый литограф").
 * How many bench chips make one chip depends on production.difficulty: one on standard, three on
 * engineer, and on fab the recipe does not exist. The grid holds exactly that many of {@code chip},
 * plus one {@code extra} when the recipe has it, and nothing else.
 */
public class LegacyChipRecipe extends CustomRecipe {
    private final Ingredient chip;
    private final Optional<Ingredient> extra;
    private final ItemStack result;

    public LegacyChipRecipe(Ingredient chip, Optional<Ingredient> extra, ItemStack result) {
        super(CraftingBookCategory.MISC);
        this.chip = chip;
        this.extra = extra;
        this.result = result;
    }

    @Override
    public boolean matches(CraftingInput input, Level level) {
        int need;
        try {
            need = ProductionDifficulty.current().benchChipsPerChip();
        } catch (IllegalStateException e) {
            return false; // no server config yet
        }
        if (need <= 0) {
            return false;
        }
        int chips = 0, extras = 0;
        for (ItemStack stack : input.items()) {
            if (stack.isEmpty()) {
                continue;
            }
            if (chip.test(stack)) {
                chips++;
            } else if (extra.isPresent() && extra.get().test(stack)) {
                extras++;
            } else {
                return false;
            }
        }
        return chips == need && extras == (extra.isPresent() ? 1 : 0);
    }

    @Override
    public ItemStack assemble(CraftingInput input, HolderLookup.Provider registries) {
        return result.copy();
    }

    @Override
    public boolean canCraftInDimensions(int width, int height) {
        return width * height >= 2;
    }

    @Override
    public ItemStack getResultItem(HolderLookup.Provider registries) {
        return result;
    }

    @Override
    public RecipeSerializer<?> getSerializer() {
        return ProductionMachines.LEGACY_CHIP.get();
    }

    public static final class Serializer implements RecipeSerializer<LegacyChipRecipe> {
        private static final MapCodec<LegacyChipRecipe> CODEC = RecordCodecBuilder.mapCodec(i -> i.group(
                Ingredient.CODEC_NONEMPTY.fieldOf("chip").forGetter(r -> r.chip),
                Ingredient.CODEC_NONEMPTY.optionalFieldOf("extra").forGetter(r -> r.extra),
                ItemStack.STRICT_CODEC.fieldOf("result").forGetter(r -> r.result)
        ).apply(i, LegacyChipRecipe::new));
        private static final StreamCodec<RegistryFriendlyByteBuf, LegacyChipRecipe> STREAM_CODEC = StreamCodec.composite(
                Ingredient.CONTENTS_STREAM_CODEC, r -> r.chip,
                ByteBufCodecs.optional(Ingredient.CONTENTS_STREAM_CODEC), r -> r.extra,
                ItemStack.STREAM_CODEC, r -> r.result,
                LegacyChipRecipe::new);

        @Override
        public MapCodec<LegacyChipRecipe> codec() {
            return CODEC;
        }

        @Override
        public StreamCodec<RegistryFriendlyByteBuf, LegacyChipRecipe> streamCodec() {
            return STREAM_CODEC;
        }
    }
}
