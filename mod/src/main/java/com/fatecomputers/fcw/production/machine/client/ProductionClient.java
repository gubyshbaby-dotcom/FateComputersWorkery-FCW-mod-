package com.fatecomputers.fcw.production.machine.client;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.machine.MachineClientHooks;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import net.minecraft.client.resources.model.ModelResourceLocation;
import net.minecraft.resources.ResourceLocation;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.EntityRenderersEvent;
import net.neoforged.neoforge.client.event.ModelEvent;
import net.neoforged.neoforge.client.event.RegisterMenuScreensEvent;
import net.neoforged.neoforge.client.event.RegisterParticleProvidersEvent;

import java.util.HashSet;
import java.util.Set;

/** Client side of the production machines: terminal screen, renderer of moving parts, dyn/ models, particles. */
@Mod(value = FcwMod.MODID, dist = Dist.CLIENT)
public final class ProductionClient {
    public ProductionClient(IEventBus modBus, ModContainer container) {
        modBus.addListener(ProductionClient::onRegisterScreens);
        modBus.addListener(ProductionClient::onRegisterRenderers);
        modBus.addListener(ProductionClient::onRegisterModels);
        modBus.addListener(ProductionClient::onRegisterParticles);
        MachineClientHooks.setTicker(MachineParticles::tick);
    }

    private static void onRegisterScreens(RegisterMenuScreensEvent event) {
        event.register(ProductionMachines.MENU.get(), MachineScreen::new);
    }

    private static void onRegisterRenderers(EntityRenderersEvent.RegisterRenderers event) {
        event.registerBlockEntityRenderer(ProductionMachines.CONTROLLER.get(), MachineRenderer::new);
    }

    /** The models of dyn/ belong to no block state: they are baked on request and drawn by {@link MachineRenderer}. */
    private static void onRegisterModels(ModelEvent.RegisterAdditional event) {
        Set<ResourceLocation> models = new HashSet<>();
        for (MachineDef def : Machines.ALL.values()) {
            def.dynamic.values().forEach(d -> models.addAll(d.models().values()));
        }
        models.forEach(m -> event.register(ModelResourceLocation.standalone(m)));
    }

    private static void onRegisterParticles(RegisterParticleProvidersEvent event) {
        ProductionMachines.PARTICLE_TYPES.forEach((id, type) -> event.registerSpriteSet(type.get(), sprites -> {
            MachineParticles.SPRITES.put(id, sprites);
            // machines spawn their particles themselves with the emitter's data; /particle gets nothing
            return (options, level, x, y, z, vx, vy, vz) -> null;
        }));
    }
}
