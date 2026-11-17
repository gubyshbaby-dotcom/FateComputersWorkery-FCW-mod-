package com.fatecomputers.fcw.registry;

import com.fatecomputers.fcw.FcwMod;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;

public final class ModCreativeTabs {
    public static final DeferredRegister<CreativeModeTab> TABS = DeferredRegister.create(Registries.CREATIVE_MODE_TAB, FcwMod.MODID);

    public static final DeferredHolder<CreativeModeTab, CreativeModeTab> MAIN = TABS.register("main", () -> CreativeModeTab.builder()
            .title(Component.translatable("itemGroup.fcw"))
            .icon(() -> new ItemStack(ModBlocks.SYSTEM_UNIT.get()))
            .displayItems((params, output) -> ModItems.TAB_ORDER.forEach(item -> output.accept(item.get())))
            .build());

    private ModCreativeTabs() {
    }
}
