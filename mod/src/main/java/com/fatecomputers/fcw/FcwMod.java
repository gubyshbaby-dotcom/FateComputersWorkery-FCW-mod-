package com.fatecomputers.fcw;

import com.fatecomputers.fcw.network.FcwNetwork;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import com.fatecomputers.fcw.registry.ModBlockEntities;
import com.fatecomputers.fcw.registry.ModBlocks;
import com.fatecomputers.fcw.registry.ModCreativeTabs;
import com.fatecomputers.fcw.registry.ModItems;
import com.fatecomputers.fcw.registry.ModMenus;
import com.fatecomputers.fcw.server.FcwServerEvents;
import com.mojang.logging.LogUtils;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.NeoForge;
import org.slf4j.Logger;

/**
 * Fate Computers Workery: computers with real Linux virtual machines inside.
 * Common entry point; the client part lives in {@link com.fatecomputers.fcw.client.FcwClient}.
 */
@Mod(FcwMod.MODID)
public final class FcwMod {
    public static final String MODID = "fcw";
    /** Mod version from neoforge.mods.toml (set when the mod is constructed). */
    private static String version = "0.0.0";
    public static final Logger LOGGER = LogUtils.getLogger();

    public FcwMod(IEventBus modBus, ModContainer container) {
        version = container.getModInfo().getVersion().toString();
        ModBlocks.BLOCKS.register(modBus);
        ModItems.ITEMS.register(modBus);
        ModBlockEntities.BLOCK_ENTITIES.register(modBus);
        ModMenus.MENUS.register(modBus);
        ModCreativeTabs.TABS.register(modBus);
        ProductionMachines.register(modBus);
        modBus.addListener(FcwNetwork::register);

        container.registerConfig(ModConfig.Type.SERVER, FcwConfig.SERVER_SPEC);
        container.registerConfig(ModConfig.Type.CLIENT, FcwConfig.CLIENT_SPEC);

        NeoForge.EVENT_BUS.register(FcwServerEvents.class);
    }

    public static String version() {
        return version;
    }
}
