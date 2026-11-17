package com.fatecomputers.fcw.registry;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.menu.ComputerMenu;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.inventory.MenuType;
import net.neoforged.neoforge.common.extensions.IMenuTypeExtension;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;

public final class ModMenus {
    public static final DeferredRegister<MenuType<?>> MENUS = DeferredRegister.create(Registries.MENU, FcwMod.MODID);

    public static final DeferredHolder<MenuType<?>, MenuType<ComputerMenu>> COMPUTER =
            MENUS.register("computer", () -> IMenuTypeExtension.create(ComputerMenu::new));

    private ModMenus() {
    }
}
