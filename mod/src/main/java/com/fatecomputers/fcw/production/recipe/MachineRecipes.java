package com.fatecomputers.fcw.production.recipe;

import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.crafting.SizedIngredient;

import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import java.util.Optional;

/** Lookup and execution of machine recipes against plain slot lists. */
public final class MachineRecipes {
    /**
     * More specific recipes win: more distinct inputs first, then more items in total, then by id.
     * So a slot set that satisfies both "server CPU" and "simple CPU" picks the server one.
     */
    private static final Comparator<RecipeHolder<MachineRecipe>> ORDER = Comparator
            .<RecipeHolder<MachineRecipe>>comparingInt(h -> -h.value().inputs().size())
            .thenComparingInt(h -> -h.value().inputs().stream().mapToInt(SizedIngredient::count).sum())
            .thenComparing(h -> h.id().toString());

    private MachineRecipes() {
    }

    /** All recipes of a type in lookup order. */
    public static List<RecipeHolder<MachineRecipe>> all(Level level, RecipeType<MachineRecipe> type) {
        return level.getRecipeManager().getAllRecipesFor(type).stream().sorted(ORDER).toList();
    }

    /** The first recipe of this type that can run from the contents of the input slots. */
    public static Optional<RecipeHolder<MachineRecipe>> find(Level level, RecipeType<MachineRecipe> type,
                                                             List<ItemStack> inputs, ItemStack catalyst) {
        if (inputs.stream().allMatch(ItemStack::isEmpty)) {
            return Optional.empty();
        }
        for (RecipeHolder<MachineRecipe> h : all(level, type)) {
            if (h.value().matches(inputs, catalyst)) {
                return Optional.of(h);
            }
        }
        return Optional.empty();
    }

    /** True if some recipe of this type uses the stack as an input (for slot filters and automation). */
    public static boolean isInput(Level level, RecipeType<MachineRecipe> type, ItemStack stack) {
        return level.getRecipeManager().getAllRecipesFor(type).stream()
                .anyMatch(h -> h.value().inputs().stream().anyMatch(i -> i.ingredient().test(stack)));
    }

    /** True if some recipe of this type uses the stack as its catalyst. */
    public static boolean isCatalyst(Level level, RecipeType<MachineRecipe> type, ItemStack stack) {
        return level.getRecipeManager().getAllRecipesFor(type).stream()
                .anyMatch(h -> h.value().catalyst().map(c -> c.test(stack)).orElse(false));
    }

    /**
     * Take the inputs out of the slots (the stacks are shrunk in place) and wear the catalyst:
     * a damageable catalyst loses 1 durability and is used up when none is left.
     * Does nothing if the inputs are not all there.
     */
    public static void consume(MachineRecipe r, List<ItemStack> inputs, ItemStack catalyst) {
        int[][] plan = plan(r, inputs);
        if (plan == null) {
            return;
        }
        for (int[] take : plan) {
            for (int slot = 0; slot < take.length; slot++) {
                if (take[slot] > 0) {
                    inputs.get(slot).shrink(take[slot]);
                }
            }
        }
        if (r.catalyst().isPresent() && !catalyst.isEmpty() && catalyst.isDamageableItem()) {
            int damage = catalyst.getDamageValue() + 1;
            if (damage >= catalyst.getMaxDamage()) {
                catalyst.shrink(1);
            } else {
                catalyst.setDamageValue(damage);
            }
        }
    }

    /**
     * How many items each ingredient takes from each slot: {@code plan[ingredient][slot]}, or null
     * if the slots do not cover the recipe. Ingredients with fewer matching slots are served first,
     * so overlapping ingredients (an item and a tag containing it) do not starve one another.
     */
    static int[][] plan(MachineRecipe r, List<ItemStack> slots) {
        int n = r.inputs().size();
        int[] left = new int[slots.size()];
        for (int s = 0; s < left.length; s++) {
            left[s] = slots.get(s).getCount();
        }
        boolean[][] fits = new boolean[n][left.length];
        Integer[] order = new Integer[n];
        int[] options = new int[n];
        for (int i = 0; i < n; i++) {
            order[i] = i;
            for (int s = 0; s < left.length; s++) {
                ItemStack stack = slots.get(s);
                fits[i][s] = !stack.isEmpty() && r.inputs().get(i).ingredient().test(stack);
                if (fits[i][s]) {
                    options[i]++;
                }
            }
        }
        Arrays.sort(order, Comparator.comparingInt(i -> options[i]));
        int[][] plan = new int[n][left.length];
        for (int i : order) {
            int need = r.inputs().get(i).count();
            for (int s = 0; s < left.length && need > 0; s++) {
                if (fits[i][s] && left[s] > 0) {
                    int take = Math.min(need, left[s]);
                    plan[i][s] = take;
                    left[s] -= take;
                    need -= take;
                }
            }
            if (need > 0) {
                return null;
            }
        }
        return plan;
    }
}
