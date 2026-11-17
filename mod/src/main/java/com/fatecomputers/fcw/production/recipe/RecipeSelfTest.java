package com.fatecomputers.fcw.production.recipe;

import com.fatecomputers.fcw.FcwMod;
import net.minecraft.ChatFormatting;
import net.minecraft.commands.Commands;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.Level;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.Supplier;

/**
 * Sanity check of the machine recipes: how many of each type were loaded and whether
 * {@link MachineRecipes#find} / {@link MachineRecipes#consume} behave on a few known slot sets.
 * Runs once when the server has started (the counts and a summary line in the log, failures as
 * warnings, passed checks at debug level) and on demand with {@code /fcw recipes}, which prints
 * every check.
 */
public final class RecipeSelfTest {
    private RecipeSelfTest() {
    }

    @SubscribeEvent
    public static void onServerStarted(ServerStartedEvent event) {
        List<String> lines = run(event.getServer());
        for (String line : lines) {
            if (line.startsWith("FAIL")) {
                FcwMod.LOGGER.warn("[recipes] {}", line);
            } else if (line.startsWith("ok")) {
                FcwMod.LOGGER.debug("[recipes] {}", line);
            } else {
                FcwMod.LOGGER.info("[recipes] {}", line);
            }
        }
    }

    @SubscribeEvent
    public static void onRegisterCommands(RegisterCommandsEvent event) {
        // Merges into the existing /fcw command.
        event.getDispatcher().register(Commands.literal("fcw")
                .then(Commands.literal("recipes").requires(s -> s.hasPermission(2)).executes(ctx -> {
                    List<String> lines = run(ctx.getSource().getServer());
                    int failed = 0;
                    for (String line : lines) {
                        boolean fail = line.startsWith("FAIL");
                        failed += fail ? 1 : 0;
                        ctx.getSource().sendSuccess(() -> Component.literal(line)
                                .withStyle(fail ? ChatFormatting.RED : ChatFormatting.GRAY), false);
                    }
                    return failed == 0 ? 1 : 0;
                })));
    }

    private static Item item(String path) {
        return BuiltInRegistries.ITEM.get(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, path));
    }

    private static ItemStack fcw(String path, int count) {
        return new ItemStack(item(path), count);
    }

    private static List<ItemStack> slots(ItemStack... stacks) {
        return new ArrayList<>(List.of(stacks));
    }

    /** One line per recipe type and per check; lines of failed checks start with "FAIL". */
    public static List<String> run(MinecraftServer server) {
        Level level = server.overworld();
        List<String> out = new ArrayList<>();
        int total = 0;
        StringBuilder counts = new StringBuilder();
        for (Map.Entry<String, Supplier<RecipeType<MachineRecipe>>> e : FcwRecipes.types().entrySet()) {
            int n = level.getRecipeManager().getAllRecipesFor(e.getValue().get()).size();
            total += n;
            counts.append(counts.isEmpty() ? "" : ", ").append(e.getKey()).append('=').append(n);
            if (n == 0) {
                out.add("FAIL no recipes of type fcw:" + e.getKey());
            }
        }
        out.add("machine recipes loaded: " + total + " (" + counts + ")");

        int before = out.size();
        Checks c = new Checks(level, out);
        RecipeType<MachineRecipe> arc = FcwRecipes.ARC_FURNACE.get();
        RecipeType<MachineRecipe> litho = FcwRecipes.LITHOGRAPHY.get();
        RecipeType<MachineRecipe> pack = FcwRecipes.CHIP_PACKAGING.get();
        RecipeType<MachineRecipe> pcb = FcwRecipes.PCB_ASSEMBLY.get();

        c.find("arc: coal + quartz in any order", arc,
                slots(new ItemStack(Items.COAL, 2), new ItemStack(Items.QUARTZ, 4)), ItemStack.EMPTY, "metallurgical_silicon");
        c.find("arc: quartz split over two slots", arc,
                slots(new ItemStack(Items.QUARTZ, 3), ItemStack.EMPTY, new ItemStack(Items.QUARTZ, 1), new ItemStack(Items.COAL, 5)),
                ItemStack.EMPTY, "metallurgical_silicon");
        c.find("arc: too little quartz", arc,
                slots(new ItemStack(Items.QUARTZ, 3), new ItemStack(Items.COAL, 2)), ItemStack.EMPTY, null);
        c.find("arc: iron + charcoal (coals tag)", arc,
                slots(new ItemStack(Items.IRON_INGOT, 4), new ItemStack(Items.CHARCOAL, 2)), ItemStack.EMPTY, "steel_ingot");
        c.find("arc: silicon + redstone", arc,
                slots(fcw("metallurgical_silicon", 2), new ItemStack(Items.REDSTONE, 1)), ItemStack.EMPTY, "polysilicon");
        c.find("arc: empty slots", arc, slots(ItemStack.EMPTY, ItemStack.EMPTY), ItemStack.EMPTY, null);
        c.find("puller: polysilicon", FcwRecipes.CRYSTAL_PULLER.get(), slots(fcw("polysilicon", 8)), ItemStack.EMPTY, "silicon_boule");
        c.find("wafering: boule", FcwRecipes.WAFERING.get(), slots(fcw("silicon_boule", 1)), ItemStack.EMPTY, "silicon_wafer");
        c.find("litho: wafer + cpu reticle", litho, slots(fcw("silicon_wafer", 1)), fcw("reticle_cpu", 1), "wafer_cpu");
        c.find("litho: wafer + memory reticle", litho, slots(fcw("silicon_wafer", 1)), fcw("reticle_memory", 1), "wafer_memory");
        c.find("litho: no reticle", litho, slots(fcw("silicon_wafer", 1)), ItemStack.EMPTY, null);
        c.find("litho: wrong catalyst", litho, slots(fcw("silicon_wafer", 1)), new ItemStack(Items.STICK), null);
        c.find("packaging: simple cpu", pack,
                slots(fcw("wafer_cpu", 1), new ItemStack(Items.GOLD_INGOT, 2)), ItemStack.EMPTY, "cpu_pga");
        c.find("packaging: the most specific recipe wins", pack,
                slots(new ItemStack(Items.NETHERITE_SCRAP, 2), fcw("steel_ingot", 4), new ItemStack(Items.GOLD_INGOT, 8), fcw("wafer_cpu", 4)),
                ItemStack.EMPTY, "cpu_server");
        c.find("pcb: motherboard", pcb, slots(fcw("pcb_blank", 1), fcw("chip_controller", 4)), ItemStack.EMPTY, "motherboard");
        c.find("pcb: graphics card", pcb,
                slots(fcw("chip_memory", 2), fcw("chip_gpu", 1), fcw("chip_controller", 1), fcw("pcb_blank", 1)), ItemStack.EMPTY, "graphics_card");

        // consume: inputs shrink, the catalyst wears and is used up at the end of its durability
        List<ItemStack> in = slots(fcw("silicon_wafer", 3));
        ItemStack reticle = fcw("reticle_cpu", 1);
        Optional<RecipeHolder<MachineRecipe>> r = MachineRecipes.find(level, litho, in, reticle);
        if (r.isPresent()) {
            MachineRecipes.consume(r.get().value(), in, reticle);
            c.check("consume: wafer 3 -> 2, reticle damage 0 -> 1",
                    in.get(0).getCount() == 2 && reticle.getDamageValue() == 1 && reticle.getCount() == 1,
                    "wafers=" + in.get(0).getCount() + " damage=" + reticle.getDamageValue());
            reticle.setDamageValue(reticle.getMaxDamage() - 1);
            MachineRecipes.consume(r.get().value(), in, reticle);
            c.check("consume: last use breaks the reticle", in.get(0).getCount() == 1 && reticle.isEmpty(),
                    "wafers=" + in.get(0).getCount() + " reticle=" + reticle);
        } else {
            c.check("consume: lithography recipe found", false, "no recipe");
        }
        List<ItemStack> split = slots(new ItemStack(Items.QUARTZ, 3), new ItemStack(Items.COAL, 5), new ItemStack(Items.QUARTZ, 3));
        MachineRecipes.find(level, arc, split, ItemStack.EMPTY).ifPresent(h -> MachineRecipes.consume(h.value(), split, ItemStack.EMPTY));
        c.check("consume: inputs taken across slots",
                split.get(0).getCount() + split.get(2).getCount() == 2 && split.get(1).getCount() == 3,
                "quartz=" + split.get(0).getCount() + "+" + split.get(2).getCount() + " coal=" + split.get(1).getCount());

        int checks = out.size() - before;
        long failed = out.stream().skip(before).filter(l -> l.startsWith("FAIL")).count();
        out.add((failed == 0 ? "" : "FAIL ") + "recipe self-test: " + (checks - failed) + "/" + checks + " passed");
        return out;
    }

    private record Checks(Level level, List<String> out) {
        void check(String name, boolean ok, String got) {
            out.add((ok ? "ok   " : "FAIL ") + name + (ok ? "" : " (" + got + ")"));
        }

        /** expected = path of the first result, or null if nothing must be found. */
        void find(String name, RecipeType<MachineRecipe> type, List<ItemStack> inputs, ItemStack catalyst, String expected) {
            Optional<RecipeHolder<MachineRecipe>> r = MachineRecipes.find(level, type, inputs, catalyst);
            String got = r.map(h -> BuiltInRegistries.ITEM.getKey(h.value().results().get(0).stack().getItem()).getPath()).orElse(null);
            boolean ok = expected == null ? got == null : expected.equals(got);
            out.add((ok ? "ok   " : "FAIL ") + name + " -> " + (got == null ? "nothing" : got + " [" + r.get().id() + "]")
                    + (ok ? "" : ", expected " + (expected == null ? "nothing" : expected)));
        }
    }
}
