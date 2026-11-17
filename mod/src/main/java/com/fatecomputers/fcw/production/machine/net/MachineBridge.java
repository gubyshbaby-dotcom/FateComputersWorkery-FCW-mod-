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

    /** Состояние для {@code list}: off, idle, run или fault с причиной. */
    record State(String id, @Nullable String reason) {
    }

    static State state(MachineControllerBlockEntity m) {
        if (!m.isEnabled()) {
            return new State("off", null);
        }
        boolean working = false;
        for (MachineControllerBlockEntity.Status s : m.status()) {
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
