package com.fatecomputers.fcw.compat.ponder;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.docs.DocStructures;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import net.createmod.ponder.api.registration.PonderPlugin;
import net.createmod.ponder.api.registration.PonderSceneRegistrationHelper;
import net.createmod.ponder.api.registration.PonderTagRegistrationHelper;
import net.createmod.ponder.api.registration.SharedTextRegistrationHelper;
import net.createmod.ponder.foundation.PonderIndex;
import net.minecraft.resources.ResourceLocation;

import java.util.ArrayList;
import java.util.List;

/**
 * Ponder scenes of the production machines. This class and its package are loaded only when Ponder is
 * installed ({@link com.fatecomputers.fcw.compat.DocsCompat}); nothing else in the mod refers to them.
 *
 * <p>Scenes hang on the items a player holds: the blueprints (and the old kits), the wrench and every
 * building block of the EUV scanner.
 * The English text is written in the scenes, {@code assets/fcw_docs/lang/en_us.json} is generated from
 * it (docs/GUIDE.md), the Russian text lives in {@code ru_ru.json} next to it.
 */
public final class FcwPonderPlugin implements PonderPlugin {
    public static final ResourceLocation TAG = ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "production");

    public static void register() {
        PonderIndex.addPlugin(new FcwPonderPlugin());
    }

    private static ResourceLocation id(String path) {
        return ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, path);
    }

    private static ResourceLocation kit(MachineDef def) {
        return id(def.name + "_kit");
    }

    private static ResourceLocation blueprint(MachineDef def) {
        return id(def.name + "_blueprint");
    }

    private static List<ResourceLocation> euvBlocks() {
        List<ResourceLocation> list = new ArrayList<>();
        ProductionMachines.EUV_BLOCKS.keySet().forEach(name -> list.add(id(name)));
        return list;
    }

    @Override
    public String getModId() {
        return FcwMod.MODID;
    }

    @Override
    public void registerScenes(PonderSceneRegistrationHelper<ResourceLocation> helper) {
        for (MachineDef def : Machines.ALL.values()) {
            if (def == Machines.EUV_SCANNER) {
                continue;
            }
            helper.forComponents(blueprint(def), kit(def))
                    .addStoryBoard(def.name, MachineScenes.build(def), TAG)
                    .addStoryBoard(def.name, MachineScenes.ports(def), TAG)
                    .addStoryBoard(DocStructures.CHAIN, MachineScenes::chain, TAG);
        }
        helper.forComponents(blueprint(Machines.EUV_SCANNER))
                .addStoryBoard(Machines.EUV, MachineScenes.build(Machines.EUV_SCANNER), TAG)
                .addStoryBoard(Machines.EUV, MachineScenes.ports(Machines.EUV_SCANNER), TAG)
                .addStoryBoard(DocStructures.CHAIN, MachineScenes::chain, TAG);
        helper.forComponents(id("wrench"))
                .addStoryBoard(Machines.ARC_FURNACE.name, MachineScenes.build(Machines.ARC_FURNACE), TAG);
        helper.forComponents(euvBlocks())
                .addStoryBoard(DocStructures.EUV_BLANK, MachineScenes::euvBuild, TAG)
                .addStoryBoard(Machines.EUV, MachineScenes.ports(Machines.EUV_SCANNER), TAG)
                .addStoryBoard(DocStructures.CHAIN, MachineScenes::chain, TAG);
    }

    @Override
    public void registerTags(PonderTagRegistrationHelper<ResourceLocation> helper) {
        helper.registerTag(TAG)
                .addToIndex()
                .item(ProductionMachines.EUV_BLOCKS.get("euv_controller").get(), true, false)
                .title("Production machines")
                .description("Multiblock machines of the chip production chain")
                .register();
        var tag = helper.addToTag(TAG);
        for (MachineDef def : Machines.ALL.values()) {
            tag.add(blueprint(def));
        }
        tag.add(id("wrench"));
        tag.add(id("euv_controller"));
    }

    @Override
    public void registerSharedText(SharedTextRegistrationHelper helper) {
        MachineScenes.SHARED.forEach(helper::registerSharedText);
    }
}
