package com.fatecomputers.fcw.client;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.SidedHooks;
import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.cinema.Cinema;
import com.fatecomputers.fcw.cinema.ProjectorBlockEntity;
import com.fatecomputers.fcw.client.render.ProjectorRenderer;
import net.minecraft.client.renderer.entity.NoopRenderer;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import com.fatecomputers.fcw.client.audio.SpeakerAudio;
import com.fatecomputers.fcw.client.gui.ComputerScreen;
import com.fatecomputers.fcw.client.nativelib.NativeLoader;
import com.fatecomputers.fcw.client.render.MonitorRenderer;
import com.fatecomputers.fcw.client.view.ScreenViewManager;
import com.fatecomputers.fcw.network.ComputerStatusPayload;
import com.fatecomputers.fcw.network.ScreenAccessPayload;
import com.fatecomputers.fcw.registry.ModBlockEntities;
import com.fatecomputers.fcw.registry.ModMenus;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLClientSetupEvent;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.EntityRenderersEvent;
import net.neoforged.neoforge.client.event.RegisterMenuScreensEvent;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.common.NeoForge;

/** Client entry point: GUI, monitor renderer, key mapping, live screens and audio. */
@Mod(value = FcwMod.MODID, dist = Dist.CLIENT)
public final class FcwClient {
    public FcwClient(IEventBus modBus, ModContainer container) {
        modBus.addListener(FcwClient::onClientSetup);
        modBus.addListener(FcwClient::onRegisterScreens);
        modBus.addListener(FcwClient::onRegisterRenderers);
        modBus.addListener(FcwKeys::register);

        NeoForge.EVENT_BUS.addListener(FcwClient::onClientTick);
        NeoForge.EVENT_BUS.addListener(FcwClient::onRenderFrame);
        NeoForge.EVENT_BUS.addListener(FcwClient::onLoggingOut);

        SidedHooks.setClient(new SidedHooks.Client() {
            @Override
            public void monitorLoaded(MonitorBlockEntity be) {
                ScreenViewManager.get().monitorLoaded(be);
            }

            @Override
            public void monitorUnloaded(MonitorBlockEntity be) {
                ScreenViewManager.get().monitorUnloaded(be);
            }

            @Override
            public void projectorLoaded(ProjectorBlockEntity be) {
                ScreenViewManager.get().projectorLoaded(be);
            }

            @Override
            public void projectorUnloaded(ProjectorBlockEntity be) {
                ScreenViewManager.get().projectorUnloaded(be);
            }

            @Override
            public void speakerLoaded(PeripheralBlockEntity be) {
                SpeakerAudio.get().speakerLoaded(be);
            }

            @Override
            public void speakerUnloaded(PeripheralBlockEntity be) {
                SpeakerAudio.get().speakerUnloaded(be);
            }

            @Override
            public void onScreenAccess(ScreenAccessPayload payload) {
                ScreenViewManager.get().onScreenAccess(payload);
            }

            @Override
            public void onComputerStatus(ComputerStatusPayload payload) {
                ClientStatusCache.put(payload);
            }
        });
    }

    private static void onClientSetup(FMLClientSetupEvent event) {
        event.enqueueWork(() -> {
            if (!NativeLoader.isLoaded()) {
                FcwMod.LOGGER.warn("Live computer screens are unavailable: {}", NativeLoader.problem().getString());
            }
        });
    }

    private static void onRegisterScreens(RegisterMenuScreensEvent event) {
        event.register(ModMenus.COMPUTER.get(), ComputerScreen::new);
    }

    private static void onRegisterRenderers(EntityRenderersEvent.RegisterRenderers event) {
        event.registerBlockEntityRenderer(ModBlockEntities.MONITOR.get(), MonitorRenderer::new);
        event.registerBlockEntityRenderer(Cinema.PROJECTOR_BE.get(), ProjectorRenderer::new);
        event.registerEntityRenderer(Cinema.SEAT.get(), NoopRenderer::new);
    }

    private static void onClientTick(ClientTickEvent.Post event) {
        ScreenViewManager.get().tick();
        SpeakerAudio.get().tick();
    }

    private static void onRenderFrame(RenderFrameEvent.Pre event) {
        ScreenViewManager.get().updateTextures();
    }

    private static void onLoggingOut(ClientPlayerNetworkEvent.LoggingOut event) {
        ScreenViewManager.get().closeAll();
        SpeakerAudio.get().reset();
        ClientStatusCache.clear();
    }
}
