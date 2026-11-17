package com.fatecomputers.fcw.production.machine;

import net.minecraft.resources.ResourceLocation;

/** Recipes as the console names them: the last part of the id ("fcw:lithography/wafer_cpu" is "wafer_cpu"). */
public final class MachineRecipeNames {
    private MachineRecipeNames() {
    }

    public static String shortName(ResourceLocation id) {
        String path = id.getPath();
        return path.substring(path.lastIndexOf('/') + 1);
    }
}
