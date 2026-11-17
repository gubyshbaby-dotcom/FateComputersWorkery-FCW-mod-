package com.fatecomputers.fcw.production.machine.shell;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.SectionState;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity.Status;
import com.fatecomputers.fcw.production.machine.MachineLog;
import com.fatecomputers.fcw.production.machine.MachineNames;
import com.fatecomputers.fcw.production.machine.MachineRecipeNames;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.construction.ConstructionPlan;
import com.fatecomputers.fcw.production.machine.construction.ConstructionSite;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.fatecomputers.fcw.production.quality.MachineQuality;
import com.fatecomputers.fcw.production.quality.Quality;
import com.fatecomputers.fcw.production.quality.WaferMap;
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
    public static final List<String> COMMANDS = List.of("status", "start", "stop", "recipe", "log", "name", "bom", "stage", "cancel", "yield", "help", "exit");
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
            case "yield" -> shiftReport(machine, t, json);
            case "bom" -> bom(machine, t, lang, json);
            case "stage" -> stage(machine, t, lang);
            case "cancel" -> cancel(machine, t, actor);
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
        if (m.site() != null) {
            return m.site().mode() == ConstructionSite.Mode.BUILD ? "build" : "teardown";
        }
        if (m.isCommissioning()) {
            return "commissioning";
        }
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
        if (m.site() != null) {
            return siteStatus(m, t, json);
        }
        List<Line> lines = new ArrayList<>();
        if (m.isCommissioning()) {
            String headC = label(t, "label.state"), wordC = t.tr("state.commissioning");
            lines.add(new Line(headC + wordC + ": " + t.tr("site.seconds_left", m.commissioningTicks() / 20), headC.length(), headC.length() + wordC.length()));
            JsonObject oc = null;
            if (json) {
                oc = new JsonObject();
                oc.addProperty("name", name(m));
                oc.addProperty("type", m.def().name);
                oc.addProperty("state", "commissioning");
                oc.addProperty("seconds_left", m.commissioningTicks() / 20);
            }
            return new Rich(0, lines, oc);
        }
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
        qualityLines(m, t, lines);

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
            // the rest of a machine frame (docs/design/CONSOLE_APPS.md): programs read status --json and machine_frame alike
            o.addProperty("kind", m.def().name);
            o.addProperty("reason", code.equals("fault") ? fault.name().toLowerCase(Locale.ROOT) : "");
            o.addProperty("phase", String.valueOf(m.workPhase() == null ? "" : m.workPhase()));
            o.addProperty("log_seq", m.log().lastSeq());
            MachineQuality q = m.quality();
            o.addProperty("defects", Quality.enabled());
            o.add("apc", q.apcJson());
            if (q.isScanner()) {
                o.add("litho", q.lithoJson());
            } else {
                JsonObject env = new JsonObject();
                env.addProperty("water", q.hasWater());
                env.addProperty("power_pauses", q.pauses(0));
                o.add("env", env);
            }
            o.add("shift", q.today().json());
        }
        return new Rich(0, lines, o);
    }

    /** The lines of "status" about defects: the last wafer, the surroundings, APC. */
    private static void qualityLines(MachineControllerBlockEntity m, ShellLang t, List<Line> lines) {
        MachineQuality q = m.quality();
        if (!Quality.enabled()) {
            return;
        }
        String yes = t.tr("q.yes"), no = t.tr("q.no");
        if (q.isScanner()) {
            MachineQuality.Wafer last = q.lastWafer();
            MachineQuality.Shift day = q.today();
            if (last != null) {
                String causes = MachineQuality.causesText(last.map());
                lines.add(Line.plain(label(t, "label.yield") + t.tr("q.last", last.yieldClass().name(), last.good(), WaferMap.FIELDS)
                        + (causes.isEmpty() ? "" : " (" + causes + ")")
                        + (day.wafers > 0 ? "; " + t.tr("q.shift", day.wafers, Math.round(100.0 * day.goodFields / (day.wafers * WaferMap.FIELDS))) : "")));
            }
            if (!q.currentMap().isEmpty()) {
                lines.add(Line.plain(label(t, "label.map") + q.currentMap()));
            }
            lines.add(Line.plain(label(t, "label.env") + (q.iso() == 0 ? t.tr("q.open") : "ISO " + q.iso())
                    + " · " + t.tr("q.coolant") + ": " + (q.hasWater() ? yes : no)
                    + " · " + t.tr("q.vibration") + ": " + (q.vibration() ? yes : no)));
        } else if (m.def().name.equals("crystal_puller") || m.def().name.equals("wafering_cell")) {
            lines.add(Line.plain(label(t, "label.env") + t.tr("q.water") + ": " + (q.hasWater() ? yes : no)
                    + " · " + t.tr("q.pauses") + ": " + q.pauses(0)));
        }
        if (q.apcNeed() > 0) {
            List<MachineQuality.ApcReport> reports = q.apcReports();
            if (reports.isEmpty()) {
                lines.add(Line.plain(label(t, "label.apc") + t.tr("q.apc.none")));
            }
            for (MachineQuality.ApcReport r : reports) {
                lines.add(Line.plain(label(t, "label.apc") + r.host() + " " + cu(r.cu()) + "/" + cu(r.max()) + " CU"
                        + (r.service() ? "" : ", " + t.tr("q.apc.no_service"))));
            }
        }
    }

    private static String cu(double v) {
        return v == Math.rint(v) ? String.valueOf((long) v) : String.format(Locale.ROOT, "%.1f", v);
    }

    /** The shift report of one machine: wafers, yield, causes, downtime; for "fcw yield". */
    private static Rich shiftReport(MachineControllerBlockEntity m, ShellLang t, boolean json) {
        MachineQuality q = m.quality();
        List<Line> lines = new ArrayList<>();
        for (int k = 0; k < 2; k++) {
            MachineQuality.Shift d = k == 0 ? q.today() : q.yesterday();
            if (k == 1 && d.wafers == 0 && d.products == 0) {
                continue;
            }
            String head = t.tr(k == 0 ? "q.today" : "q.yesterday");
            if (q.isScanner()) {
                StringBuilder classes = new StringBuilder(), causes = new StringBuilder();
                for (Quality.YieldClass y : Quality.YieldClass.values()) {
                    if (d.classes[y.ordinal()] > 0) {
                        classes.append(' ').append(y.name()).append(' ').append(d.classes[y.ordinal()]);
                    }
                }
                for (int i = 0; i < d.causes.length; i++) {
                    if (d.causes[i] > 0) {
                        causes.append(' ').append(WaferMap.CAUSES.charAt(i)).append(' ').append(d.causes[i]);
                    }
                }
                lines.add(Line.plain(head + "  " + t.tr("q.wafers", d.wafers) + "  "
                        + t.tr("q.good", d.wafers == 0 ? 0 : Math.round(100.0 * d.goodFields / (d.wafers * WaferMap.FIELDS)))
                        + (classes.isEmpty() ? "" : " " + classes) + (causes.isEmpty() ? "" : "   " + t.tr("q.causes") + causes)));
            } else {
                lines.add(Line.plain(head + "  " + t.tr("q.products", d.products)));
            }
            int down = d.downtime[0] + d.downtime[1] + d.downtime[2];
            if (down > 0) {
                lines.add(Line.plain("  " + t.tr("q.downtime") + ": " + t.tr("reason.no_power") + " " + d.downtime[0] / 20 + " s, "
                        + t.tr("reason.output_full", "out") + " " + d.downtime[1] / 20 + " s, " + t.tr("reason.no_catalyst") + " " + d.downtime[2] / 20 + " s"));
            }
        }
        if (q.isScanner()) {
            List<MachineQuality.Wafer> all = q.history();
            for (MachineQuality.Wafer w : all.subList(Math.max(0, all.size() - 6), all.size())) {
                String causes = MachineQuality.causesText(w.map());
                lines.add(Line.plain("  " + CLOCK.format(Instant.ofEpochMilli(w.time())) + "  " + w.recipe() + "  " + w.yieldClass().name() + "  "
                        + w.good() + "/" + WaferMap.FIELDS + (causes.isEmpty() ? "" : "  " + causes)));
            }
        }
        JsonObject o = null;
        if (json) {
            o = new JsonObject();
            o.addProperty("name", name(m));
            o.addProperty("kind", m.def().name);
            o.add("today", q.today().json());
            o.add("yesterday", q.yesterday().json());
            if (q.isScanner()) {
                o.add("litho", q.lithoJson());
            }
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
        if (m.site() != null || m.isCommissioning()) {
            return new Rich(1, List.of(refused(t, t.tr("site.not_ready"))), null);
        }
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

    // ───────────────────────── construction site

    private static String stageTitle(ConstructionPlan.Stage s, String lang) {
        return s.title(lang).toLowerCase(Locale.ROOT);
    }

    /** One word for what a stage of a site is doing. */
    private static String stageState(MachineControllerBlockEntity m, ShellLang t, int index) {
        ConstructionSite site = m.site();
        if (site == null || index < site.stage()) {
            return t.tr("site.stage.done");
        }
        if (index > site.stage()) {
            return t.tr("site.stage.waits");
        }
        if (!site.started()) {
            return t.tr("site.stage.materials");
        }
        String word = t.tr(site.mode() == ConstructionSite.Mode.BUILD ? "site.stage.building" : "site.stage.teardown");
        int pct = (int) Math.round(site.stageProgress() * 100);
        return word + " " + pct + " %" + (site.waitingPower() ? ", " + t.tr("reason.no_power") : "");
    }

    private static Rich siteStatus(MachineControllerBlockEntity m, ShellLang t, boolean json) {
        ConstructionSite site = m.site();
        ConstructionPlan plan = site.plan();
        String lang = t.tr("lang.code");
        List<Line> lines = new ArrayList<>();
        boolean build = site.mode() == ConstructionSite.Mode.BUILD;
        String head = label(t, "label.state"), word = t.tr(build ? "state.build" : "state.teardown");
        int index = Math.max(0, Math.min(site.stage(), plan.stages.size() - 1));
        ConstructionPlan.Stage s = plan.stages.get(index);
        lines.add(new Line(head + word + ": " + t.tr("site.stage_of", index + 1, plan.stages.size(), stageTitle(s, lang)) + ", " + stageState(m, t, index),
                head.length(), head.length() + word.length()));
        JsonObject missing = new JsonObject();
        if (build && !site.started()) {
            StringBuilder sb = new StringBuilder();
            for (int[] lack : site.missing(index)) {
                String id = s.entries().get(lack[0]).itemId().getPath();
                sb.append(sb.isEmpty() ? "" : ", ").append(id).append(" x").append(lack[1]);
                missing.addProperty(s.entries().get(lack[0]).itemId().toString(), lack[1]);
            }
            lines.add(Line.plain(label(t, "label.missing") + sb));
        }
        int cost = com.fatecomputers.fcw.production.ProductionDifficulty.current().buildEnergyPerTick();
        lines.add(Line.plain(label(t, "label.power") + (cost > 0
                ? fe(m.energy().getEnergyStored()) + "/" + fe(MachineControllerBlockEntity.ENERGY_CAPACITY) + " FE  " + t.tr("site.needs", cost)
                : t.tr("site.no_power_needed"))));
        JsonObject o = null;
        if (json) {
            o = new JsonObject();
            o.addProperty("name", name(m));
            o.addProperty("type", m.def().name);
            o.addProperty("state", build ? "build" : "teardown");
            o.addProperty("stage", index + 1);
            o.addProperty("stages", plan.stages.size());
            o.addProperty("stage_id", s.id());
            o.addProperty("started", site.started());
            o.addProperty("progress", site.stageProgress());
            o.addProperty("waiting_power", site.waitingPower());
            o.add("missing", missing);
            o.addProperty("missing_total", site.missingTotal());
            o.addProperty("energy", m.energy().getEnergyStored());
            o.addProperty("capacity", MachineControllerBlockEntity.ENERGY_CAPACITY);
        }
        return new Rich(0, lines, o);
    }

    /** The whole bill of materials by stage: delivered / needed. For a finished machine, what it was built of. */
    private static Rich bom(MachineControllerBlockEntity m, ShellLang t, String lang, boolean json) {
        ConstructionSite site = m.site();
        ConstructionPlan plan = ConstructionPlan.of(m.def());
        List<Line> lines = new ArrayList<>();
        JsonArray stages = new JsonArray();
        for (int i = 0; i < plan.stages.size(); i++) {
            ConstructionPlan.Stage s = plan.stages.get(i);
            String title = stageTitle(s, lang);
            String head = t.tr("site.stage_n", i + 1) + "  " + title + " ".repeat(Math.max(1, 26 - title.length()));
            boolean open = site != null && site.mode() == ConstructionSite.Mode.BUILD && i >= site.stage() && !(i == site.stage() && site.started());
            int have = 0, need = 0;
            JsonArray items = new JsonArray();
            List<Line> rows = new ArrayList<>();
            for (int e = 0; e < s.entries().size(); e++) {
                ConstructionPlan.Entry entry = s.entries().get(e);
                int h = site == null ? entry.need() : site.have(i, e);
                have += h;
                need += entry.need();
                String id = entry.itemId().getPath();
                rows.add(Line.plain("          " + id + " ".repeat(Math.max(1, 24 - id.length())) + String.format(Locale.ROOT, "%3d/%d", h, entry.need())));
                JsonObject it = new JsonObject();
                it.addProperty("id", entry.itemId().toString());
                it.addProperty("have", h);
                it.addProperty("need", entry.need());
                items.add(it);
            }
            String state = stageState(m, t, i);
            // stages still open show their lines only when something of them has arrived; else a total
            boolean detail = open && (have > 0 || i == site.stage());
            lines.add(Line.plain(head + state + (open && !detail ? "    " + have + "/" + need : "")));
            if (detail || (site != null && i == site.stage() && site.started())) {
                lines.addAll(rows);
            }
            JsonObject so = new JsonObject();
            so.addProperty("id", s.id());
            so.addProperty("title", s.title(lang));
            so.addProperty("seconds", s.ticks() / 20);
            so.add("items", items);
            stages.add(so);
        }
        if (site != null && site.mode() == ConstructionSite.Mode.BUILD) {
            lines.add(Line.plain(t.tr("site.missing_total", site.missingTotal())));
        }
        JsonObject o = null;
        if (json) {
            o = new JsonObject();
            o.add("stages", stages);
            o.addProperty("missing_total", site == null ? 0 : site.missingTotal());
        }
        return new Rich(0, lines, o);
    }

    private static Rich stage(MachineControllerBlockEntity m, ShellLang t, String lang) {
        ConstructionSite site = m.site();
        if (site == null) {
            return new Rich(0, List.of(Line.plain(t.tr("site.built"))), null);
        }
        ConstructionPlan plan = site.plan();
        int index = Math.max(0, Math.min(site.stage(), plan.stages.size() - 1));
        ConstructionPlan.Stage s = plan.stages.get(index);
        List<Line> lines = new ArrayList<>();
        lines.add(Line.plain(t.tr("site.stage_of", index + 1, plan.stages.size(), stageTitle(s, lang)) + "  " + stageState(m, t, index)
                + "  " + (s.ticks() / 20) + " s"));
        for (int e = 0; e < s.entries().size(); e++) {
            String id = s.entries().get(e).itemId().getPath();
            lines.add(Line.plain("  " + id + " ".repeat(Math.max(1, 24 - id.length())) + site.have(index, e) + "/" + s.entries().get(e).need()));
        }
        return new Rich(0, lines, null);
    }

    private static Rich cancel(MachineControllerBlockEntity m, ShellLang t, String actor) {
        ConstructionSite site = m.site();
        if (site == null) {
            return new Rich(1, List.of(refused(t, t.tr("site.cancel_built"))), null);
        }
        if (site.mode() == ConstructionSite.Mode.TEARDOWN) {
            return new Rich(0, List.of(Line.plain(t.tr("site.already_teardown"))), null);
        }
        site.beginTeardown(actor);
        return new Rich(0, List.of(Line.plain(t.tr("site.cancelled"))), null);
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
        for (String c : List.of("status", "start", "stop", "recipe", "log", "name", "bom", "stage", "cancel", "yield", "help", "exit")) {
            lines.add(Line.plain(t.tr("help." + c)));
        }
        return new Rich(0, lines, null);
    }
}
