package com.fatecomputers.fcw.client;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.KeyMapping;
import net.neoforged.neoforge.client.event.RegisterKeyMappingsEvent;
import net.neoforged.neoforge.client.settings.KeyConflictContext;
import org.lwjgl.glfw.GLFW;

public final class FcwKeys {
    public static final String CATEGORY = "key.categories.fcw";

    /** Leaves the fullscreen computer view (input capture). */
    public static final KeyMapping RELEASE_INPUT = new KeyMapping("key.fcw.release_input", KeyConflictContext.GUI,
            InputConstants.Type.KEYSYM, GLFW.GLFW_KEY_RIGHT_CONTROL, CATEGORY);

    private FcwKeys() {
    }

    static void register(RegisterKeyMappingsEvent event) {
        event.register(RELEASE_INPUT);
    }
}
