package com.fatecomputers.fcw.production.machine.shell;

import com.google.gson.Gson;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Texts of the machine shell by language. They are read from the mod jar ({@code data/fcw/shell/<lang>.json}),
 * not from client assets: the shell runs on the server, and a dedicated server has no lang files. A key
 * missing in a language falls back to en_us, then to the key itself.
 */
public final class ShellLang {
    private static final Map<String, ShellLang> CACHE = new ConcurrentHashMap<>();
    private static final ShellLang ENGLISH = load("en_us", null);

    private final Map<String, String> texts = new HashMap<>();
    private final ShellLang fallback;

    private ShellLang(ShellLang fallback) {
        this.fallback = fallback;
    }

    public static ShellLang of(String lang) {
        String code = lang == null ? "en_us" : lang.toLowerCase(Locale.ROOT);
        if (!code.matches("[a-z]{2,3}_[a-z]{2,3}") || code.equals("en_us")) {
            return ENGLISH;
        }
        return CACHE.computeIfAbsent(code, c -> load(c, ENGLISH));
    }

    private static ShellLang load(String code, ShellLang fallback) {
        ShellLang lang = new ShellLang(fallback);
        try (InputStream in = ShellLang.class.getResourceAsStream("/data/fcw/shell/" + code + ".json")) {
            if (in != null) {
                try (Reader r = new InputStreamReader(in, StandardCharsets.UTF_8)) {
                    for (Map.Entry<String, JsonElement> e : new Gson().fromJson(r, JsonObject.class).entrySet()) {
                        lang.texts.put(e.getKey(), e.getValue().getAsString());
                    }
                }
            }
        } catch (Exception e) {
            // a broken table only costs the translation
        }
        return lang;
    }

    public boolean has(String key) {
        return texts.containsKey(key) || (fallback != null && fallback.has(key));
    }

    public String tr(String key, Object... args) {
        String text = texts.get(key);
        if (text == null) {
            return fallback != null ? fallback.tr(key, args) : key;
        }
        try {
            return args.length == 0 ? text : String.format(Locale.ROOT, text, args);
        } catch (RuntimeException e) {
            return text;
        }
    }
}
