package com.fatecomputers.fcw.compat.ponder;

import com.fatecomputers.fcw.FcwMod;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.createmod.ponder.foundation.PonderIndex;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.common.NeoForge;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.TreeMap;

/**
 * Development tool: {@code ./gradlew runClient -Pfcw.docsExport} rewrites the Ponder keys of
 * {@code assets/fcw_docs/lang/en_us.json} from the text written in the scenes, as soon as a world is
 * joined. Ponder reads scene text from lang files only, so the English file is generated instead of
 * being kept in step by hand. The other keys of the file are left alone.
 */
public final class PonderLangExport {
    private static final String PREFIX = FcwMod.MODID + ".ponder.";

    private PonderLangExport() {
    }

    public static void register(String assetsDir) {
        Path file = Path.of(assetsDir, "fcw_docs", "lang", "en_us.json");
        NeoForge.EVENT_BUS.addListener((ClientPlayerNetworkEvent.LoggingIn event) -> {
            try {
                write(file);
                FcwMod.LOGGER.info("[docs] Ponder texts written to {}", file);
            } catch (IOException | RuntimeException e) {
                FcwMod.LOGGER.error("[docs] Ponder text export failed", e);
            }
        });
    }

    private static void write(Path file) throws IOException {
        Map<String, String> ponder = new TreeMap<>();
        PonderIndex.getLangAccess().provideLang(FcwMod.MODID, ponder::put);
        PonderIndex.reload(); // provideLang registered every scene a second time

        JsonObject out = new JsonObject();
        if (Files.exists(file)) {
            JsonObject old = JsonParser.parseString(Files.readString(file, StandardCharsets.UTF_8)).getAsJsonObject();
            for (Map.Entry<String, JsonElement> e : old.entrySet()) {
                if (!e.getKey().startsWith(PREFIX)) {
                    out.add(e.getKey(), e.getValue());
                }
            }
        }
        ponder.forEach(out::addProperty);
        Files.createDirectories(file.getParent());
        Files.writeString(file, new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(out) + "\n", StandardCharsets.UTF_8);
    }
}
