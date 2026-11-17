package com.fatecomputers.fcw.compat;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.compat.guideme.FcwGuide;
import com.fatecomputers.fcw.compat.ponder.FcwPonderPlugin;
import com.fatecomputers.fcw.compat.ponder.PonderLangExport;
import com.fatecomputers.fcw.registry.ModCreativeTabs;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModList;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.event.BuildCreativeModeTabContentsEvent;

/**
 * In-game documentation: the guidebook (GuideME) and the multiblock scenes (Ponder). Both libraries are
 * optional. This is the only class that knows about them; the packages {@code compat.guideme} and
 * {@code compat.ponder} are touched only from the branches below, so without the library their classes
 * are never loaded.
 *
 * <p>A second entry point of the mod for the client (the first is {@link com.fatecomputers.fcw.client.FcwClient}),
 * so that the documentation needs no hooks in the rest of the code.
 */
@Mod(value = FcwMod.MODID, dist = Dist.CLIENT)
public final class DocsCompat {
    public static final String GUIDEME = "guideme";
    public static final String PONDER = "ponder";

    public DocsCompat(IEventBus modBus) {
        if (ModList.get().isLoaded(GUIDEME)) {
            Guide.init(modBus);
        }
        if (ModList.get().isLoaded(PONDER)) {
            Scenes.init();
        }
    }

    /** Kept apart from the entry point: resolving this class is what pulls GuideME in. */
    private static final class Guide {
        static void init(IEventBus modBus) {
            FcwGuide.init();
            modBus.addListener((BuildCreativeModeTabContentsEvent event) -> {
                if (event.getTab() == ModCreativeTabs.MAIN.get()) {
                    event.accept(FcwGuide.book());
                }
            });
        }
    }

    private static final class Scenes {
        static void init() {
            FcwPonderPlugin.register();
            String export = System.getProperty("fcw.docsExport");
            if (export != null) {
                PonderLangExport.register(export);
            }
        }
    }
}
