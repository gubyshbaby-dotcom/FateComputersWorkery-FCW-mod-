package com.fatecomputers.fcw.production.recipe;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.ItemStack;

/**
 * One output of a machine recipe: {@code {"id": "fcw:steel_ingot", "count": 2, "chance": 0.5}}.
 * {@code count} defaults to 1, {@code chance} to 1 (always).
 */
public record ChanceResult(ItemStack stack, float chance) {
    public static final Codec<ChanceResult> CODEC = RecordCodecBuilder.create(i -> i.group(
            ItemStack.ITEM_NON_AIR_CODEC.fieldOf("id").forGetter(r -> r.stack.getItemHolder()),
            Codec.intRange(1, 99).optionalFieldOf("count", 1).forGetter(r -> r.stack.getCount()),
            Codec.floatRange(0f, 1f).optionalFieldOf("chance", 1f).forGetter(ChanceResult::chance)
    ).apply(i, (item, count, chance) -> new ChanceResult(new ItemStack(item, count), chance)));

    public static final StreamCodec<RegistryFriendlyByteBuf, ChanceResult> STREAM_CODEC = StreamCodec.composite(
            ItemStack.STREAM_CODEC, ChanceResult::stack,
            ByteBufCodecs.FLOAT, ChanceResult::chance,
            ChanceResult::new);

    public ChanceResult(ItemStack stack) {
        this(stack, 1f);
    }

    /** True if the output always appears. */
    public boolean guaranteed() {
        return chance >= 1f;
    }

    /** A copy of the output, or an empty stack if the chance did not play out. */
    public ItemStack roll(RandomSource random) {
        return guaranteed() || random.nextFloat() < chance ? stack.copy() : ItemStack.EMPTY;
    }
}
