package com.fatecomputers.fcw.production.machine.shell;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.SectionState;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.Status;
import com.fatecomputers.fcw.production.machine.MachineLog;
import com.fatecomputers.fcw.production.machine.MachineNames;
import com.fatecomputers.fcw.production.machine.MachineRecipeNames;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.fatecomputers.fcw.production.recipe.ChanceResult;
import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import com.fatecomputers.fcw.production.recipe.MachineRecipes;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.crafting.SizedIngredient;
import org.jetbrains.annotations.Nullable;

import java.time.Instant;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;

/**
 * Командная оболочка машины: одна и та же для консоли пульта и для {@code fcw machine} с компьютера.
 * Контракт: docs/MACHINE_CONTROL.md, команды и вид вывода: docs/design/MACHINE_UI.md.
 *
 * <p>Команды всегда английские, пояснения на языке {@code lang} (таблицы {@link ShellLang} в данных
 * мода), имена предметов и рецептов латиницей. Вывод короткий, построчный, без псевдографики; в строке
 * не больше одного акцента (слово состояния, «отказ», «ошибка»).
 */
public final class MachineShell {
    /** Имена команд, для автодополнения на пульте. */
    public static final List<String> COMMANDS = List.of("status", "start", "stop", "recipe", "log", "name", "help", "exit");
    private static final DateTimeFormatter CLOCK = DateTimeFormatter.ofPattern("HH:mm:ss").withZone(ZoneId.systemDefault());

    private MachineShell() {
    }

    /**
     * @param exit 0 — успех, 1 — отказ машины (нет питания, нет рецепта…), 2 — ошибка в команде
     * @param text вывод для человека, на языке {@code lang}
     * @param json вывод для скриптов (только при {@code json = true})
     */
    public record Result(int exit, String text, @Nullable JsonObject json) {
    }

    /** Строка вывода; столбцы [accentStart, accentEnd) выделяются акцентным цветом (−1: нет акцента). */
    public record Line(String text, int accentStart, int accentEnd) {
        public static Line plain(String text) {
            return new Line(text, -1, -1);
        }
    }

    /** То же, что {@link Result}, но построчно и с акцентами: для консоли пульта. */
    public record Rich(int exit, List<Line> lines, @Nullable JsonObject json) {
    }

    /**
     * Выполнить одну строку команды на машине (сервер, главный поток).
     *
     * @param line  строка без имени машины: {@code "status"}, {@code "recipe auto"}, {@code "log 5"}…
     * @param lang  код языка клиента ({@code ru_ru}, {@code en_us}); команды всегда английские
     * @param json  машиночитаемый вывод
     * @param actor кто выполняет — для журнала машины («оператор Steve», «euv-ctl.mc»)
     */
    public static Result exec(MachineControllerBlockEntity machine, String line, String lang, boolean json, String actor) {
        Rich rich = execRich(machine, line, lang, json, actor);
        StringBuilder text = new StringBuilder();
        for (Line l : rich.lines()) {
            if (!text.isEmpty()) {
                text.append('\n');
            }
            text.append(l.text());
        }
        return new Result(rich.exit(), text.toString(), rich.json());
    }

    /** Имя машины для {@code fcw machine <имя>}: уникально в мире, по умолчанию «euv-1», «eaf-2»… */
    public static String name(MachineControllerBlockEntity machine) {
        return machine.machineName();
    }

    /** Первые строки консоли пульта. */
    public static List<Line> banner(MachineControllerBlockEntity machine, String lang) {
        ShellLang t = ShellLang.of(lang);
        return List.of(Line.plain(t.tr("banner", name(machine), t.tr("machine." + machine.def().name))),
                Line.plain(t.tr("banner.hint")), Line.plain(""));
    }

    public static Rich execRich(MachineControllerBlockEntity machine, String line, String lang, boolean json, String actor) {
        ShellLang t = ShellLang.of(lang);
        List<String> args = new ArrayList<>(Arrays.asList(line.trim().split("\\s+")));
        if (args.remove("--json")) {
            json = true;
        }
        String cmd = args.isEmpty() ? "" : args.remove(0).toLowerCase(Locale.ROOT);
        return switch (cmd) {
            case "" -> new Rich(0, List.of(), null);
            case "status" -> status(machine, t, json);
            case "start" -> start(machine, t, actor);
            case "stop" -> stop(machine, t, actor, args.contains("now"));
            case "recipe" -> recipe(machine, t, json, args);
            case "log" -> log(machine, t, json, args);
            case "name" -> name(machine, t, args, actor);
            case "help" -> help(t);
            default -> error(t, t.tr("unknown", cmd));
        };
    }

    // ───────────────────────── helpers

    private static Rich error(ShellLang t, String message) {
        String word = t.tr("error");
        return new Rich(2, List.of(new Line(word + ": " + message, 0, word.length())), null);
    }

    private static Line refused(ShellLang t, String message) {
        String word = t.tr("refused");
        return new Line(word + ": " + message, 0, word.length());
    }

    private static String label(ShellLang t, String key) {
        String s = t.tr(key);
        return s + " ".repeat(Math.max(1, 11 - s.length()));
    }

    private static String itemName(ItemStack stack) {
        return BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath();
    }

    private static String fe(int value) {
        return value >= 10_000 ? (value / 1000) + "k" : Integer.toString(value);
    }

    /** "silicon_wafer x6, tin_ingot x16" or the word for "empty". */
    private static String contents(ShellLang t, List<ItemStack> stacks) {
        if (stacks.isEmpty()) {
            return t.tr("empty");
        }
        StringBuilder sb = new StringBuilder();
        for (ItemStack s : stacks) {
            if (!sb.isEmpty()) {
                sb.append(", ");
            }
            sb.append(itemName(s)).append(" x").append(s.getCount());
        }
        return sb.toString();
    }

    private static JsonArray contentsJson(List<ItemStack> stacks) {
        JsonArray a = new JsonArray();
        for (ItemStack s : stacks) {
            JsonObject o = new JsonObject();
            o.addProperty("id", BuiltInRegistries.ITEM.getKey(s.getItem()).toString());
            o.addProperty("count", s.getCount());
            a.add(o);
        }
        return a;
    }

    /** off, idle, run or fault: the machine as a whole. */
    public static String stateCode(MachineControllerBlockEntity m) {
        if (!m.isEnabled()) {
            return "off";
        }
        if (m.faultStatus() != null) {
            return "fault";
        }
        for (int i = 0; i < m.sectionCount(); i++) {
            if (m.section(i).status() == Status.WORKING) {
                return "run";
            }
        }
        return "idle";
    }

    /** The cause of a fault in words: "no power", "output full (tap)". */
    private static String reason(MachineControllerBlockEntity m, ShellLang t, Status fault) {
        if (fault == Status.OUTPUT_FULL) {
            for (int i = 0; i < m.sectionCount(); i++) {
                if (m.section(i).status() == Status.OUTPUT_FULL) {
                    for (Zone z : m.def().spec.zones()) {
                        if (z.section() == i && (z.role() == Role.OUTPUT || z.role() == Role.TRAY)) {
                            return t.tr("reason.output_full", z.id());
                        }
                    }
                }
            }
            return t.tr("reason.output_full", "out");
        }
        return t.tr("reason." + fault.name().toLowerCase(Locale.ROOT));
    }

    private static String phase(MachineControllerBlockEntity m, ShellLang t) {
        String clip = m.workPhase();
        if (clip == null) {
            return "";
        }
        return t.has("phase." + clip) ? t.tr("phase." + clip) : clip;
    }

    // ───────────────────────── status

    private static Rich status(MachineControllerBlockEntity m, ShellLang t, boolean json) {
        List<Line> lines = new ArrayList<>();
        String code = stateCode(m);
        Status fault = m.faultStatus();
        String word, detail;
        switch (code) {
            case "off" -> {
                word = t.tr("state.off");
                detail = "";
            }
            case "fault" -> {
                word = t.tr("state.fault");
                detail = reason(m, t, fault);
            }
            case "run" -> {
                word = t.tr("state.run");
                detail = phase(m, t);
            }
            default -> {
                word = t.tr("state.idle");
                detail = hasAnyInput(m) ? "" : t.tr("reason.no_input");
            }
        }
        if (m.isStopping()) {
            detail = detail.isEmpty() ? t.tr("state.stopping") : detail + ", " + t.tr("state.stopping");
        }
        String head = label(t, "label.state");
        lines.add(new Line(head + word + (detail.isEmpty() ? "" : ": " + detail), head.length(), head.length() + word.length()));

        boolean many = m.sectionCount() > 1;
        List<Section> specs = m.def().spec.sections();
        for (int i = 0; i < m.sectionCount(); i++) {
            SectionState s = m.section(i);
            String prefix = many ? specs.get(i).id() + ": " : "";
            if (s.running()) {
                MachineRecipe r = s.recipe();
                lines.add(Line.plain(label(t, "label.recipe") + prefix + MachineRecipeNames.shortName(s.recipeId()) + "  "
                        + s.progress() + "/" + s.total() + " t" + (r == null ? "" : "  " + r.energyPerTick() + " FE/t")));
            } else if (s.pinned() != null) {
                lines.add(Line.plain(label(t, "label.recipe") + prefix + t.tr("recipe.pinned", MachineRecipeNames.shortName(s.pinned()))));
            }
            if (specs.get(i).catalyst()) {
                ItemStack mask = m.items().getStackInSlot(m.catalystSlot(i));
                String text = mask.isEmpty() ? t.tr("empty") : itemName(mask)
                        + (mask.isDamageableItem() ? "  " + t.tr("wear", mask.getMaxDamage() - mask.getDamageValue(), mask.getMaxDamage()) : "");
                lines.add(Line.plain(label(t, "label.mask") + text));
            }
        }
        JsonObject load = new JsonObject(), out = new JsonObject();
        StringBuilder loadText = new StringBuilder(), outText = new StringBuilder();
        for (Zone z : m.def().spec.zones()) {
            if (z.role() == Role.CATALYST || z.port() == null) {
                continue; // the mask has its own line; the hood is the same slots as "charge"
            }
            boolean isOut = z.role() == Role.OUTPUT || z.role() == Role.TRAY;
            List<ItemStack> stacks = m.zoneContents(z);
            if (!isOut && isDuplicateInput(m, z)) {
                continue;
            }
            StringBuilder sb = isOut ? outText : loadText;
            if (!sb.isEmpty()) {
                sb.append("   ");
            }
            sb.append(z.id()).append(": ").append(contents(t, stacks));
            (isOut ? out : load).add(z.id(), contentsJson(stacks));
        }
        lines.add(Line.plain(label(t, "label.load") + loadText));
        lines.add(Line.plain(label(t, "label.out") + outText));
        boolean required = FcwConfig.SERVER.productionRequirePower.get();
        int energy = m.energy().getEnergyStored();
        lines.add(Line.plain(label(t, "label.power") + (required
                ? fe(energy) + "/" + fe(MachineControllerBlockEntity.ENERGY_CAPACITY) + " FE  " + t.tr("income", m.energyIncome())
                : t.tr("builtin"))));

        JsonObject o = null;
        if (json) {
            o = new JsonObject();
            o.addProperty("name", name(m));
            o.addProperty("type", m.def().name);
            o.addProperty("state", code);
            o.addProperty("detail", code.equals("fault") ? fault.name().toLowerCase(Locale.ROOT)
                    : code.equals("run") ? String.valueOf(m.workPhase()) : "");
            o.addProperty("stopping", m.isStopping());
            int driver = -1;
            for (int i = 0; i < m.sectionCount() && driver < 0; i++) {
                driver = m.section(i).running() ? i : -1;
            }
            if (driver >= 0) {
                SectionState s = m.section(driver);
                o.addProperty("recipe", MachineRecipeNames.shortName(s.recipeId()));
                o.addProperty("ticks", s.progress());
                o.addProperty("total", s.total());
            } else {
                o.add("recipe", null);
                o.addProperty("ticks", 0);
                o.addProperty("total", 0);
            }
            o.addProperty("energy", energy);
            o.addProperty("capacity", MachineControllerBlockEntity.ENERGY_CAPACITY);
            o.addProperty("in", m.energyIncome());
            o.addProperty("power_required", required);
            o.add("load", load);
            o.add("out", out);
        }
        return new Rich(0, lines, o);
    }

    /** Several input zones of one section share its slots (tin drawer and dock 1): list them once. */
    private static boolean isDuplicateInput(MachineControllerBlockEntity m, Zone zone) {
        for (Zone z : m.def().spec.zones()) {
            if (z == zone) {
                return false;
            }
            if (z.port() != null && z.section() == zone.section() && z.role() == zone.role()) {
                return true;
            }
        }
        return false;
    }

    private static boolean hasAnyInput(MachineControllerBlockEntity m) {
        for (int i = 0; i < m.sectionCount(); i++) {
            for (int slot : m.inputSlots(i)) {
                if (!m.items().getStackInSlot(slot).isEmpty()) {
                    return true;
                }
            }
        }
        return false;
    }

    // ───────────────────────── start, stop

    private static Rich start(MachineControllerBlockEntity m, ShellLang t, String actor) {
        boolean was = m.isEnabled() && !m.isStopping();
        m.setEnabled(true, actor);
        Status fault = m.faultStatus();
        if (fault == Status.NO_POWER || fault == Status.NO_CATALYST) {
            return new Rich(1, List.of(refused(t, t.tr("start." + fault.name().toLowerCase(Locale.ROOT)))), null);
        }
        return new Rich(0, List.of(Line.plain(t.tr(was ? "start.already" : "start.ok"))), null);
    }

    private static Rich stop(MachineControllerBlockEntity m, ShellLang t, String actor, boolean now) {
        if (!m.isEnabled()) {
            return new Rich(0, List.of(Line.plain(t.tr("stop.already"))), null);
        }
        if (now) {
            m.setEnabled(false, actor);
            return new Rich(0, List.of(Line.plain(t.tr("stop.now"))), null);
        }
        m.stopAfterCurrent(actor);
        return new Rich(0, List.of(Line.plain(t.tr(m.isEnabled() ? "stop.after" : "stop.now"))), null);
    }

    // ───────────────────────── recipe

    private static String ingredient(SizedIngredient in) {
        ItemStack[] items = in.ingredient().getItems();
        String name = items.length == 0 ? "?" : itemName(items[0]);
        return in.count() > 1 ? in.count() + " " + name : name;
    }

    private static String describe(RecipeHolder<MachineRecipe> h) {
        MachineRecipe r = h.value();
        StringBuilder sb = new StringBuilder();
        for (SizedIngredient in : r.inputs()) {
            sb.append(sb.isEmpty() ? "" : " + ").append(ingredient(in));
        }
        sb.append(" -> ");
        boolean first = true;
        for (ChanceResult res : r.results()) {
            sb.append(first ? "" : " + ").append(res.stack().getCount() > 1 ? res.stack().getCount() + " " : "").append(itemName(res.stack()));
            first = false;
        }
        return sb + "   " + r.ticks() + " t  " + r.energyPerTick() + " FE/t";
    }

    private static Rich recipe(MachineControllerBlockEntity m, ShellLang t, boolean json, List<String> args) {
        Level level = m.getLevel();
        List<Section> specs = m.def().spec.sections();
        if (!args.isEmpty()) {
            String want = args.get(0).toLowerCase(Locale.ROOT);
            if (want.equals("auto")) {
                for (int i = 0; i < m.sectionCount(); i++) {
                    if (m.section(i).pinned() != null) {
                        m.pinRecipe(i, null);
                    }
                }
                return new Rich(0, List.of(Line.plain(t.tr("recipe.choice", "auto"))), null);
            }
            for (int i = 0; i < m.sectionCount(); i++) {
                for (RecipeHolder<MachineRecipe> h : MachineRecipes.all(level, specs.get(i).recipeType().get())) {
                    if (MachineRecipeNames.shortName(h.id()).equals(want)) {
                        m.pinRecipe(i, h.id());
                        return new Rich(0, List.of(Line.plain(t.tr("recipe.choice", want))), null);
                    }
                }
            }
            return error(t, t.tr("recipe.unknown", want));
        }
        List<Line> lines = new ArrayList<>();
        JsonArray list = new JsonArray();
        JsonObject pinned = new JsonObject();
        boolean anyAvailable = false;
        List<String[]> rows = new ArrayList<>();
        for (int i = 0; i < m.sectionCount(); i++) {
            List<ItemStack> inputs = new ArrayList<>();
            for (int slot : m.inputSlots(i)) {
                inputs.add(m.items().getStackInSlot(slot));
            }
            int from = specs.get(i).alsoTakesFromSection();
            if (from >= 0) {
                for (int slot : m.outputSlots(from)) {
                    inputs.add(m.items().getStackInSlot(slot));
                }
            }
            ItemStack catalyst = m.catalystSlot(i) < 0 ? ItemStack.EMPTY : m.items().getStackInSlot(m.catalystSlot(i));
            for (RecipeHolder<MachineRecipe> h : MachineRecipes.all(level, specs.get(i).recipeType().get())) {
                boolean available = h.value().matches(inputs, catalyst);
                anyAvailable |= available;
                rows.add(new String[] {MachineRecipeNames.shortName(h.id()), describe(h), available ? "1" : "0"});
                if (json) {
                    JsonObject o = new JsonObject();
                    o.addProperty("name", MachineRecipeNames.shortName(h.id()));
                    o.addProperty("id", h.id().toString());
                    o.addProperty("section", specs.get(i).id());
                    o.addProperty("available", available);
                    o.addProperty("ticks", h.value().ticks());
                    o.addProperty("energy_per_tick", h.value().energyPerTick());
                    list.add(o);
                }
            }
            ResourceLocation pin = m.section(i).pinned();
            pinned.addProperty(specs.get(i).id(), pin == null ? "auto" : MachineRecipeNames.shortName(pin));
        }
        // what can be made from the load; with nothing suitable loaded, everything the machine knows
        if (!anyAvailable) {
            lines.add(Line.plain(t.tr("recipe.none")));
        }
        int width = 0;
        for (String[] r : rows) {
            width = Math.max(width, r[0].length());
        }
        for (String[] r : rows) {
            if (!anyAvailable || r[2].equals("1")) {
                lines.add(Line.plain(r[0] + " ".repeat(width - r[0].length() + 3) + r[1]));
            }
        }
        StringBuilder choice = new StringBuilder();
        for (int i = 0; i < m.sectionCount(); i++) {
            ResourceLocation pin = m.section(i).pinned();
            if (!choice.isEmpty()) {
                choice.append(", ");
            }
            choice.append(m.sectionCount() > 1 ? specs.get(i).id() + " " : "").append(pin == null ? "auto" : MachineRecipeNames.shortName(pin));
        }
        lines.add(Line.plain(t.tr("recipe.choice", choice)));
        JsonObject o = null;
        if (json) {
            o = new JsonObject();
            o.add("recipes", list);
            o.add("pinned", pinned);
        }
        return new Rich(0, lines, o);
    }

    // ───────────────────────── log

    private static String logText(ShellLang t, MachineLog.Entry e) {
        String arg = e.arg();
        if (e.code().equals(MachineLog.FAULT) || e.code().equals(MachineLog.WAIT)) {
            arg = e.arg().equals("output_full") ? t.tr("reason.output_full", "out") : t.tr("reason." + e.arg());
        }
        String key = "log." + e.code();
        return arg.isEmpty() && t.has(key + ".plain") ? t.tr(key + ".plain") : t.tr(key, arg);
    }

    private static Rich log(MachineControllerBlockEntity m, ShellLang t, boolean json, List<String> args) {
        List<MachineLog.Entry> entries;
        try {
            int since = args.indexOf("--since");
            if (since >= 0) {
                entries = m.log().since(Long.parseLong(args.get(since + 1)));
            } else {
                entries = m.log().last(args.isEmpty() ? 10 : Integer.parseInt(args.get(0)));
            }
        } catch (RuntimeException e) {
            return error(t, t.tr("log.usage"));
        }
        List<Line> lines = new ArrayList<>();
        JsonArray list = new JsonArray();
        for (MachineLog.Entry e : entries) {
            String text = logText(t, e);
            lines.add(Line.plain(CLOCK.format(Instant.ofEpochMilli(e.time())) + "  " + text));
            if (json) {
                JsonObject o = new JsonObject();
                o.addProperty("seq", e.seq());
                o.addProperty("time", e.time());
                o.addProperty("code", e.code());
                o.addProperty("arg", e.arg());
                o.addProperty("text", text);
                list.add(o);
            }
        }
        if (lines.isEmpty() && !args.contains("--since")) {
            lines.add(Line.plain(t.tr("log.empty")));
        }
        JsonObject o = null;
        if (json) {
            o = new JsonObject();
            o.add("log", list);
        }
        return new Rich(0, lines, o);
    }

    // ───────────────────────── name, help

    private static Rich name(MachineControllerBlockEntity m, ShellLang t, List<String> args, String actor) {
        if (args.isEmpty()) {
            return new Rich(0, List.of(Line.plain(name(m))), null);
        }
        String want = args.get(0).toLowerCase(Locale.ROOT);
        if (!MachineNames.VALID.matcher(want).matches()) {
            return error(t, t.tr("name.bad"));
        }
        if (want.equals(name(m))) {
            return new Rich(0, List.of(Line.plain(want)), null);
        }
        if (!m.rename(want, actor)) {
            return new Rich(1, List.of(refused(t, t.tr("name.taken", want))), null);
        }
        return new Rich(0, List.of(Line.plain(t.tr("name.ok", want))), null);
    }

    private static Rich help(ShellLang t) {
        List<Line> lines = new ArrayList<>();
        for (String c : List.of("status", "start", "stop", "recipe", "log", "name", "help", "exit")) {
            lines.add(Line.plain(t.tr("help." + c)));
        }
        return new Rich(0, lines, null);
    }
}
