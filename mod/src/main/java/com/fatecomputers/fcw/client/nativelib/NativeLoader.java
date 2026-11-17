package com.fatecomputers.fcw.client.nativelib;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.util.NativeFiles;
import com.fatecomputers.fcw.util.Platform;
import net.minecraft.network.chat.Component;
import net.neoforged.fml.loading.FMLPaths;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Optional;

/**
 * Loads {@code libfcw_client} once: from the client config path, or extracted from the jar into
 * {@code <gameDir>/fcw/natives/<sha256>/}. Without it the mod still works, monitors just show the
 * static model picture.
 */
public final class NativeLoader {
    private static boolean attempted;
    private static boolean loaded;
    private static Component problem = Component.empty();

    private NativeLoader() {
    }

    public static synchronized boolean isLoaded() {
        if (!attempted) {
            attempted = true;
            load();
        }
        return loaded;
    }

    /** Why live screens are unavailable (empty when loaded). */
    public static Component problem() {
        isLoaded();
        return problem;
    }

    private static void load() {
        String name = Platform.clientLibraryFileName();
        try {
            Path lib;
            String configured = "";
            try {
                configured = FcwConfig.CLIENT.nativeLibraryPath.get().trim();
            } catch (IllegalStateException ignored) {
                // config not loaded yet
            }
            if (!configured.isEmpty()) {
                lib = Paths.get(configured).toAbsolutePath();
                if (!Files.isRegularFile(lib)) {
                    problem = Component.translatable("native.fcw.missing_file", lib.toString());
                    FcwMod.LOGGER.warn("FCW client library not found at {}", lib);
                    return;
                }
            } else {
                Optional<Path> extracted = NativeFiles.extract(name, FMLPaths.GAMEDIR.get().resolve("fcw").resolve("natives"), false);
                if (extracted.isEmpty()) {
                    problem = Component.translatable("native.fcw.not_bundled", Platform.id());
                    FcwMod.LOGGER.warn("No FCW client library for {} in the mod jar ({}); live screens are disabled",
                            Platform.id(), NativeFiles.resourcePath(name));
                    return;
                }
                lib = extracted.get();
            }
            System.load(lib.toString());
            loaded = true;
            problem = Component.empty();
            FcwMod.LOGGER.info("Loaded FCW client library {}", lib);
        } catch (Throwable t) {
            problem = Component.translatable("native.fcw.load_failed", t.toString());
            FcwMod.LOGGER.error("Could not load the FCW client library; live screens are disabled", t);
        }
    }
}
