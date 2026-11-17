package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.shell.MachineShell;
import com.google.gson.JsonArray;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;

import javax.annotation.Nullable;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Мост между агентом гостя и машинами: сообщение {@code machine_req} с компьютера превращается в
 * вызов {@link MachineShell}, ответ уходит обратно как {@code machine_resp}
 * (docs/MACHINE_CONTROL.md). Один экземпляр на компьютер; вызывается только в главном потоке сервера.
 *
 * <p>Запросы одной ВМ ограничены по частоте ({@code production.machineRequestsPerSecond}), чтобы
 * скрипт в цикле не занял тик сервера. Лишние запросы получают отказ с кодом 2, а если гость не
 * унимается — остаются без ответа (CLI сам сообщит «агент не отвечает»).
 */
public final class MachineBridge {
    /** Строка агента не длиннее 64 КиБ; запас оставлен на обёртку JSON. */
    public static final int MAX_LINE_BYTES = 60_000;
    private static final int MAX_COMMAND_CHARS = 1024;
    /** Сколько тиков живёт найденный список машин: `wait` опрашивает раз в секунду. */
    private static final int LINKS_TTL_TICKS = 10;

    private final Bucket requests = new Bucket();
    private final Bucket refusals = new Bucket();
    @Nullable
    private List<MachineLinks.Link> links;
    private long linksAt;

    /** Ведро жетонов: {@code rate} в секунду, запас на две секунды. */
    static final class Bucket {
        private double tokens = Double.NaN;
        private long at;

        boolean take(double rate, long nowNanos) {
            double capacity = Math.max(1.0, rate * 2);
            if (Double.isNaN(tokens)) {
                tokens = capacity;
            } else {
                tokens = Math.min(capacity, tokens + (nowNanos - at) / 1e9 * rate);
            }
            at = nowNanos;
            if (tokens < 1.0) {
                return false;
            }
            tokens -= 1.0;
            return true;
        }
    }

    /**
     * @param actor имя компьютера для журнала машины («fedora-1.mc»)
     * @return ответ {@code machine_resp} или null, если отвечать не нужно
     */
    @Nullable
    public JsonObject handle(ServerLevel level, BlockPos computerPos, String actor, JsonObject req) {
        return handle(level, computerPos, actor, req, System.nanoTime());
    }

    @Nullable
    JsonObject handle(ServerLevel level, BlockPos computerPos, String actor, JsonObject req, long nowNanos) {
        if (!req.has("id") || !req.get("id").isJsonPrimitive()) {
            return null; // без id ответ не сопоставить
        }
        long id;
        try {
            id = req.get("id").getAsLong();
        } catch (RuntimeException e) {
            return null;
        }
        String lang = ServerLang.normalize(string(req, "lang"));
        double rate = FcwConfig.SERVER.machineRequestsPerSecond.get();
        if (!requests.take(rate, nowNanos)) {
            return refusals.take(rate, nowNanos) ? response(id, 2, ServerLang.get(lang, "fcw.machine.net.busy"), null) : null;
        }
        boolean json = req.has("json") && req.get("json").isJsonPrimitive() && req.get("json").getAsBoolean();
        String op = string(req, "op");
        try {
            return switch (op) {
                case "list" -> list(id, links(level, computerPos), lang, json);
                case "exec" -> exec(id, links(level, computerPos), string(req, "machine"), string(req, "line"), lang, json, actor);
                case "yield" -> shiftReport(id, links(level, computerPos), lang, actor);
                default -> response(id, 2, ServerLang.get(lang, "fcw.machine.net.bad_request"), null);
            };
        } catch (RuntimeException e) {
            FcwMod.LOGGER.error("Machine request from {} failed: {}", actor, req, e);
            return response(id, 2, ServerLang.get(lang, "fcw.machine.net.failed"), null);
        }
    }

    private List<MachineLinks.Link> links(ServerLevel level, BlockPos computerPos) {
        long now = level.getGameTime();
        if (links == null || now - linksAt >= LINKS_TTL_TICKS || now < linksAt) {
            links = MachineLinks.links(level, computerPos);
            linksAt = now;
        }
        links.removeIf(l -> l.machine().isRemoved());
        return links;
    }

    private static JsonObject list(long id, List<MachineLinks.Link> links, String lang, boolean json) {
        List<String[]> rows = new ArrayList<>();
        JsonArray machines = new JsonArray();
        for (MachineLinks.Link l : links) {
            MachineControllerBlockEntity m = l.machine();
            String name = MachineShell.name(m);
            State st = state(m);
            String word = ServerLang.get(lang, "fcw.machine.net.state." + st.id());
            if (st.reason() != null) {
                word += ": " + ServerLang.get(lang, "fcw.machine.net.fault." + st.reason());
            }
            rows.add(new String[] {name, ServerLang.get(lang, "machine.fcw." + m.def().name), word});
            JsonObject o = new JsonObject();
            o.addProperty("name", name);
            o.addProperty("kind", m.def().name);
            o.addProperty("state", st.id());
            o.addProperty("link", l.via().id());
            machines.add(o);
        }
        String text;
        if (rows.isEmpty()) {
            text = ServerLang.get(lang, "fcw.machine.net.empty");
        } else {
            int w0 = 0, w1 = 0;
            for (String[] r : rows) {
                w0 = Math.max(w0, r[0].length());
                w1 = Math.max(w1, r[1].length());
            }
            StringBuilder sb = new StringBuilder();
            for (String[] r : rows) {
                if (!sb.isEmpty()) {
                    sb.append('\n');
                }
                sb.append(pad(r[0], w0 + 2)).append(pad(r[1], w1 + 2)).append(r[2]);
            }
            text = sb.toString();
        }
        JsonObject out = null;
        if (json) {
            out = new JsonObject();
            out.add("machines", machines);
        }
        return response(id, 0, text, out);
    }

    private static JsonObject exec(long id, List<MachineLinks.Link> links, String machine, String line, String lang, boolean json,
                                   String actor) {
        if (machine.isEmpty() || line.isBlank() || line.length() > MAX_COMMAND_CHARS) {
            return response(id, 2, ServerLang.get(lang, "fcw.machine.net.bad_request"), null);
        }
        for (MachineLinks.Link l : links) {
            if (MachineShell.name(l.machine()).equals(machine)) {
                MachineShell.Result r = MachineShell.exec(l.machine(), line.strip(), lang, json, actor);
                return response(id, r.exit(), r.text() == null ? "" : r.text(), r.json());
            }
        }
        return response(id, 2, ServerLang.get(lang, "fcw.machine.net.none", machine), null);
    }

    /** Отчёт за смену по всем машинам сети компьютера: для {@code fcw yield}. */
    private static JsonObject shiftReport(long id, List<MachineLinks.Link> links, String lang, String actor) {
        JsonArray machines = new JsonArray();
        StringBuilder text = new StringBuilder();
        for (MachineLinks.Link l : links) {
            MachineShell.Result r = MachineShell.exec(l.machine(), "yield", lang, true, actor);
            if (r.json() != null) {
                machines.add(r.json());
            }
            if (!r.text().isEmpty()) {
                text.append(text.isEmpty() ? "" : "\n").append(MachineShell.name(l.machine())).append("\n").append(r.text());
            }
        }
        JsonObject out = new JsonObject();
        out.add("machines", machines);
        return response(id, 0, text.toString(), out);
    }

    // ───────────────────────── APC и подписка на снимки (docs/CONTRACT.md §7, docs/design/DEFECTS.md §6)

    /** Сколько тиков служба {@code fcw apc} считается живой после подтверждения: три пропуска по секунде. */
    private static final int APC_BEAT_TTL = 70;
    private static final int MAX_WATCHED = 8;

    private long apcBeatAt = Long.MIN_VALUE;
    private final List<String> apcMachines = new ArrayList<>();
    private final java.util.Map<String, Double> apcShares = new java.util.HashMap<>();
    private long watchId = -1;
    private final List<String> watched = new ArrayList<>();
    private int watchInterval = 5;
    private long frameSeq;

    /** CU процессора компьютера (DEFECTS.md, раздел 6); 0 — процессора нет. */
    public static double computeUnits(net.minecraft.world.item.ItemStack cpu) {
        if (cpu.isEmpty()) {
            return 0;
        }
        return switch (net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(cpu.getItem()).getPath()) {
            case "cpu_dip" -> 0.5;
            case "cpu_qfp" -> 1;
            case "cpu_pga" -> 2;
            case "cpu_lga" -> 4;
            case "cpu_multicore" -> 8;
            case "cpu_server" -> 16;
            default -> 0;
        };
    }

    /**
     * {@code {"type":"apc_beat","machines":["euv-1","cz-1"],"shares":{"euv-1":8}}}: служба {@code fcw apc}
     * в госте жива. {@code machines} пуст или отсутствует — все машины сети поровну; {@code shares} — CU
     * по машинам (не больше того, что есть у процессора).
     */
    public void onApcBeat(ServerLevel level, JsonObject msg) {
        apcBeatAt = level.getGameTime();
        apcMachines.clear();
        apcShares.clear();
        if (msg.has("machines") && msg.get("machines").isJsonArray()) {
            for (com.google.gson.JsonElement e : msg.getAsJsonArray("machines")) {
                if (apcMachines.size() < 64 && e.isJsonPrimitive()) {
                    apcMachines.add(e.getAsString());
                }
            }
        }
        if (msg.has("shares") && msg.get("shares").isJsonObject()) {
            for (java.util.Map.Entry<String, com.google.gson.JsonElement> e : msg.getAsJsonObject("shares").entrySet()) {
                if (apcShares.size() < 64 && e.getValue().isJsonPrimitive()) {
                    apcShares.put(e.getKey(), Math.max(0, e.getValue().getAsDouble()));
                }
            }
        }
    }

    /** {@code {"type":"machine_watch","id":21,"machines":["euv-1"],"hz":4}}: слать снимки этих машин, не чаще 4 раз в секунду. */
    public void onWatch(JsonObject msg) {
        watched.clear();
        watchId = msg.has("id") && msg.get("id").isJsonPrimitive() ? msg.get("id").getAsLong() : 0;
        if (msg.has("machines") && msg.get("machines").isJsonArray()) {
            for (com.google.gson.JsonElement e : msg.getAsJsonArray("machines")) {
                if (watched.size() < MAX_WATCHED && e.isJsonPrimitive()) {
                    watched.add(e.getAsString());
                }
            }
        }
        double hz = msg.has("hz") && msg.get("hz").isJsonPrimitive() ? msg.get("hz").getAsDouble() : 4;
        watchInterval = (int) Math.max(5, Math.min(100, Math.round(20 / Math.max(0.2, hz))));
    }

    public void onUnwatch() {
        watched.clear();
        watchId = -1;
    }

    /** ВМ выключилась или агент пропал: подписка и служба APC кончились вместе с ней. */
    public void onGuestGone() {
        onUnwatch();
        apcBeatAt = Long.MIN_VALUE;
    }

    /**
     * Каждый тик компьютера. Раз в секунду компьютер сообщает машинам своей сети, сколько CU им даёт
     * (100 % при живой службе, иначе 75 %; выключенная ВМ — тоже 75 %); по подписке шлёт снимки.
     *
     * @param vmOn ВМ запущена и агент на связи
     */
    public void tick(ServerLevel level, BlockPos computerPos, String actor, net.minecraft.world.item.ItemStack cpu, boolean vmOn,
                     java.util.function.Consumer<JsonObject> send) {
        long now = level.getGameTime();
        if ((now + computerPos.asLong()) % 20 == 0) {
            double max = computeUnits(cpu);
            List<MachineLinks.Link> all = max > 0 ? links(level, computerPos) : List.of();
            boolean service = vmOn && now - apcBeatAt <= APC_BEAT_TTL && now >= apcBeatAt;
            double total = max * (service ? 1.0 : 0.75);
            List<MachineControllerBlockEntity> targets = new ArrayList<>();
            for (MachineLinks.Link l : all) {
                if (l.machine().quality().apcNeed() > 0
                        && (!service || apcMachines.isEmpty() || apcMachines.contains(MachineShell.name(l.machine())))) {
                    targets.add(l.machine());
                }
            }
            for (MachineControllerBlockEntity m : targets) {
                double share = total / targets.size();
                if (service && !apcShares.isEmpty()) {
                    share = Math.min(total, apcShares.getOrDefault(MachineShell.name(m), 0.0));
                }
                m.quality().apcReport(computerPos.asLong(), actor, share, max / targets.size(), service, now);
            }
        }
        if (watchId >= 0 && !watched.isEmpty() && vmOn && now % watchInterval == 0) {
            JsonArray frames = new JsonArray();
            for (MachineLinks.Link l : links(level, computerPos)) {
                if (watched.contains(MachineShell.name(l.machine()))) {
                    JsonObject f = MachineShell.exec(l.machine(), "status", "en_us", true, actor).json();
                    if (f != null) {
                        f.addProperty("link", l.via().id());
                        frames.add(f);
                    }
                }
            }
            JsonObject o = new JsonObject();
            o.addProperty("type", "machine_frame");
            o.addProperty("id", watchId);
            o.addProperty("seq", ++frameSeq);
            // game clock for the header of the console programs: ticks of the day (0 = 06:00) and the day number
            o.addProperty("time", level.getDayTime() % 24000L);
            o.addProperty("day", level.getDayTime() / 24000L);
            o.add("machines", frames);
            if (o.toString().getBytes(StandardCharsets.UTF_8).length <= MAX_LINE_BYTES) {
                send.accept(o);
            }
        }
    }

    /** Состояние для {@code list}: off, idle, run или fault с причиной. */
    record State(String id, @Nullable String reason) {
    }

    static State state(MachineControllerBlockEntity m) {
        if (!m.isEnabled()) {
            return new State("off", null);
        }
        boolean working = false;
        for (int i = 0; i < m.sectionCount(); i++) {
            MachineControllerBlockEntity.Status s = m.section(i).status();
            switch (s) {
                case OFF, IDLE -> {
                }
                case WORKING -> working = true;
                // всё остальное — причина, по которой участок стоит
                default -> {
                    return new State("fault", s.name().toLowerCase(Locale.ROOT));
                }
            }
        }
        return new State(working ? "run" : "idle", null);
    }

    private static String pad(String s, int width) {
        return s.length() >= width ? s : s + " ".repeat(width - s.length());
    }

    private static String string(JsonObject o, String key) {
        return o.has(key) && o.get(key).isJsonPrimitive() ? o.get(key).getAsString() : "";
    }

    /** {@code machine_resp}; ответ, который не влезает в строку агента, урезается до текста. */
    static JsonObject response(long id, int exit, String text, @Nullable JsonObject json) {
        JsonObject o = new JsonObject();
        o.addProperty("type", "machine_resp");
        o.addProperty("id", id);
        o.addProperty("exit", exit);
        o.addProperty("text", text);
        o.add("json", json == null ? JsonNull.INSTANCE : json);
        if (o.toString().getBytes(StandardCharsets.UTF_8).length <= MAX_LINE_BYTES) {
            return o;
        }
        if (json != null) {
            // машиночитаемый ответ резать нельзя: лучше честная ошибка
            return response(id, 2, "answer too long", null);
        }
        String cut = text;
        while (cut.length() > 16) {
            cut = cut.substring(0, cut.length() * 3 / 4);
            o.addProperty("text", cut + "\n…");
            if (o.toString().getBytes(StandardCharsets.UTF_8).length <= MAX_LINE_BYTES) {
                break;
            }
        }
        return o;
    }
}
