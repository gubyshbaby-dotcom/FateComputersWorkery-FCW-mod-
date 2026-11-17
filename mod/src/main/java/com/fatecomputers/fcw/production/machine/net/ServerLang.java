package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.FcwMod;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.regex.Pattern;

/**
 * Строки мода на языке, который назвал гость. Сервер отвечает {@code fcw machine} текстом, а языка
 * клиента у него нет (на выделенном сервере ассеты не загружаются), поэтому файлы
 * {@code assets/fcw/lang/<код>.json} читаются прямо из jar мода. Нет языка или ключа — английский.
 */
public final class ServerLang {
    public static final String FALLBACK = "en_us";
    private static final Pattern CODE = Pattern.compile("[a-z]{2,3}_[a-z]{2,3}");
    private static final Map<String, Map<String, String>> TABLES = new ConcurrentHashMap<>();

    private ServerLang() {
    }

    /** Код языка из запроса гостя, приведённый к виду {@code ru_ru}; мусор превращается в английский. */
    public static String normalize(String lang) {
        return lang != null && CODE.matcher(lang).matches() ? lang : FALLBACK;
    }

    public static String get(String lang, String key, Object... args) {
        String pattern = table(normalize(lang)).get(key);
        if (pattern == null) {
            pattern = table(FALLBACK).getOrDefault(key, key);
        }
        try {
            return args.length == 0 ? pattern : String.format(pattern, args);
        } catch (RuntimeException e) {
            return pattern;
        }
    }

    private static Map<String, String> table(String lang) {
        return TABLES.computeIfAbsent(lang, ServerLang::load);
    }

    private static Map<String, String> load(String lang) {
        Map<String, String> out = new HashMap<>();
        try (InputStream in = ServerLang.class.getResourceAsStream("/assets/" + FcwMod.MODID + "/lang/" + lang + ".json")) {
            if (in != null) {
                JsonObject o = JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8)).getAsJsonObject();
                for (Map.Entry<String, JsonElement> e : o.entrySet()) {
                    if (e.getValue().isJsonPrimitive()) {
                        out.put(e.getKey(), e.getValue().getAsString());
                    }
                }
            }
        } catch (IOException | RuntimeException e) {
            FcwMod.LOGGER.warn("Could not read the {} language file: {}", lang, e.toString());
        }
        return out;
    }
}
