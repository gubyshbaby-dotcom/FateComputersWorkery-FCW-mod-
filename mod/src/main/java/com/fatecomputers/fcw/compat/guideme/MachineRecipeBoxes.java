package com.fatecomputers.fcw.compat.guideme;

import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import com.fatecomputers.fcw.production.recipe.ChanceResult;
import com.fatecomputers.fcw.production.recipe.FcwRecipes;
import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import guideme.compiler.PageCompiler;
import guideme.compiler.tags.BlockTagCompiler;
import guideme.compiler.tags.MdxAttrs;
import guideme.compiler.tags.RecipeTypeMappingSupplier;
import guideme.document.DefaultStyles;
import guideme.document.block.LytBlock;
import guideme.document.block.LytBlockContainer;
import guideme.document.block.LytParagraph;
import guideme.document.block.LytSlotGrid;
import guideme.document.block.recipes.LytStandardRecipeBox;
import guideme.libs.mdast.mdx.model.MdxJsxElementFields;
import net.minecraft.client.Minecraft;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeType;
import net.neoforged.neoforge.common.crafting.SizedIngredient;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;

/**
 * Machine recipes in the guidebook. A recipe is drawn with GuideME's standard recipe box: inputs with
 * their counts, the reticle (if any) in a column of its own, outputs, and one line with time and energy.
 *
 * <ul>
 * <li>{@code <Recipe id="fcw:arc_furnace/steel_ingot" />}, {@code <RecipeFor id="fcw:steel_ingot" />}: the
 * stock GuideME tags, taught the six machine recipe types;</li>
 * <li>{@code <MachineRecipes type="fcw:arc_furnace" />}: every recipe of one type, datapack recipes
 * included, so the pages need no hand-written tables.</li>
 * </ul>
 */
final class MachineRecipeBoxes {
    /** What the box of a recipe type is titled with and which item stands for the machine. */
    private record Station(MachineDef def, String sectionKey) {
    }

    private static final Map<Supplier<RecipeType<MachineRecipe>>, Station> STATIONS = new LinkedHashMap<>();

    static {
        STATIONS.put(FcwRecipes.ARC_FURNACE, new Station(Machines.ARC_FURNACE, null));
        STATIONS.put(FcwRecipes.CRYSTAL_PULLER, new Station(Machines.CRYSTAL_PULLER, null));
        STATIONS.put(FcwRecipes.WAFERING, new Station(Machines.WAFERING_CELL, null));
        STATIONS.put(FcwRecipes.LITHOGRAPHY, new Station(Machines.EUV_SCANNER, null));
        STATIONS.put(FcwRecipes.CHIP_PACKAGING, new Station(Machines.CHIP_LINE, "guide.fcw.section.packaging"));
        STATIONS.put(FcwRecipes.PCB_ASSEMBLY, new Station(Machines.CHIP_LINE, "guide.fcw.section.assembly"));
    }

    private MachineRecipeBoxes() {
    }

    static void collect(RecipeTypeMappingSupplier.RecipeTypeMappings mappings) {
        STATIONS.forEach((type, station) -> mappings.add(type.get(), holder -> box(holder, station)));
    }

    private static LytBlock box(RecipeHolder<MachineRecipe> holder, Station station) {
        MachineRecipe recipe = holder.value();
        boolean catalyst = recipe.catalyst().isPresent();

        int count = recipe.inputs().size();
        int columns = count > 4 ? 3 : Math.min(count, 2);
        int rows = (count + columns - 1) / columns;
        LytSlotGrid inputs = new LytSlotGrid(columns + (catalyst ? 1 : 0), rows);
        for (int i = 0; i < count; i++) {
            SizedIngredient in = recipe.inputs().get(i);
            inputs.setIngredient(i % columns, i / columns, Ingredient.of(in.getItems()));
        }
        if (catalyst) {
            inputs.setIngredient(columns, 0, recipe.catalyst().get());
        }

        List<ChanceResult> results = recipe.results();
        int outColumns = Math.min(results.size(), 2);
        LytSlotGrid outputs = new LytSlotGrid(outColumns, (results.size() + outColumns - 1) / outColumns);
        for (int i = 0; i < results.size(); i++) {
            outputs.setItem(i % outColumns, i / outColumns, results.get(i).stack().copy());
        }

        Component title = Component.translatable("machine.fcw." + station.def().name);
        if (station.sectionKey() != null) {
            title = Component.translatable("guide.fcw.recipe.section", title, Component.translatable(station.sectionKey()));
        }
        LytStandardRecipeBox.Builder box = LytStandardRecipeBox.builder()
                .title(title.getString())
                .icon(ProductionMachines.sourceItem(station.def()))
                .input(inputs)
                .output(outputs)
                .addBottom(note(Component.translatable("guide.fcw.recipe.stats",
                        seconds(recipe.ticks()), recipe.energyPerTick(), grouped(recipe.totalEnergy()))));
        if (catalyst) {
            box.addBottom(note(Component.translatable("guide.fcw.recipe.catalyst")));
        }
        for (ChanceResult result : results) {
            if (!result.guaranteed()) {
                box.addBottom(note(Component.translatable("guide.fcw.recipe.chance",
                        result.stack().getCount(), Math.round(result.chance() * 100))));
            }
        }
        return box.build(holder);
    }

    /** A line under the slots, in the colour of the box title (the body colour is for the page background). */
    private static LytParagraph note(Component text) {
        LytParagraph paragraph = LytParagraph.of(text.getString());
        paragraph.setStyle(DefaultStyles.CRAFTING_RECIPE_TYPE);
        return paragraph;
    }

    private static String seconds(int ticks) {
        return ticks % 20 == 0 ? Integer.toString(ticks / 20) : String.format(Locale.ROOT, "%.1f", ticks / 20.0);
    }

    /** 1228800 -> "1 228 800" (the same in both languages of the guide). */
    private static String grouped(long value) {
        return String.format(Locale.ROOT, "%,d", value).replace(',', ' ');
    }

    /** {@code <MachineRecipes type="fcw:arc_furnace" />}: all recipes of that type, sorted by id. */
    static final class AllOfType extends BlockTagCompiler {
        @Override
        public Set<String> getTagNames() {
            return Set.of("MachineRecipes");
        }

        @Override
        protected void compile(PageCompiler compiler, LytBlockContainer parent, MdxJsxElementFields el) {
            ResourceLocation typeId = MdxAttrs.getRequiredId(compiler, parent, el, "type");
            if (typeId == null) {
                return;
            }
            RecipeType<?> type = BuiltInRegistries.RECIPE_TYPE.get(typeId);
            Station station = null;
            for (Map.Entry<Supplier<RecipeType<MachineRecipe>>, Station> e : STATIONS.entrySet()) {
                if (e.getKey().get() == type) {
                    station = e.getValue();
                }
            }
            if (station == null) {
                parent.appendError(compiler, "Not a machine recipe type: " + typeId, el);
                return;
            }
            var level = Minecraft.getInstance().level;
            if (level == null) {
                parent.appendError(compiler, "Cannot show recipes while not in-game", el);
                return;
            }
            List<RecipeHolder<MachineRecipe>> recipes = new ArrayList<>();
            for (RecipeHolder<?> holder : level.getRecipeManager().getRecipes()) {
                if (holder.value() instanceof MachineRecipe r && r.type() == type) {
                    @SuppressWarnings("unchecked")
                    RecipeHolder<MachineRecipe> h = (RecipeHolder<MachineRecipe>) holder;
                    recipes.add(h);
                }
            }
            // cheapest first: the order a player meets them in
            recipes.sort(Comparator.<RecipeHolder<MachineRecipe>>comparingLong(h -> h.value().totalEnergy())
                    .thenComparing(h -> h.id().toString()));
            for (RecipeHolder<MachineRecipe> holder : recipes) {
                parent.append(box(holder, station));
            }
        }
    }
}
