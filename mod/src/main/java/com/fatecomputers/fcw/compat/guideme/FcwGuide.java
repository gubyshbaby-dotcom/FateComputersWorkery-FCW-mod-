package com.fatecomputers.fcw.compat.guideme;

import com.fatecomputers.fcw.FcwMod;
import guideme.Guide;
import guideme.GuideItemSettings;
import guideme.Guides;
import guideme.compiler.TagCompiler;
import guideme.compiler.tags.RecipeTypeMappingSupplier;
import guideme.scene.element.SceneElementTagCompiler;
import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.ItemStack;

import java.util.List;
import java.util.Optional;

/**
 * The guidebook: GuideME guide {@code fcw:guide}, pages in {@code assets/fcw/fcwguide} (English at the
 * root, Russian under {@code _ru_ru}). Loaded only when GuideME is present
 * ({@link com.fatecomputers.fcw.compat.DocsCompat}).
 *
 * <p>The book itself is GuideME's generic guide item carrying this guide's id; its recipe is
 * {@code data/fcw_docs/recipe/guide.json}.
 */
public final class FcwGuide {
    public static final ResourceLocation ID = ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "guide");

    private static Guide guide;

    private FcwGuide() {
    }

    /** Called once while the mod is constructed, on the client. */
    public static void init() {
        guide = Guide.builder(ID)
                .folder("fcwguide")
                .defaultLanguage("en_us")
                .itemSettings(new GuideItemSettings(
                        Optional.of(Component.translatable("item.fcw_docs.guide")),
                        List.of(Component.translatable("item.fcw_docs.guide.tooltip").withStyle(ChatFormatting.GRAY)),
                        Optional.empty()))
                .extension(RecipeTypeMappingSupplier.EXTENSION_POINT, MachineRecipeBoxes::collect)
                .extension(TagCompiler.EXTENSION_POINT, new MachineRecipeBoxes.AllOfType())
                .extension(TagCompiler.EXTENSION_POINT, new MachinePortTags.Table())
                .extension(SceneElementTagCompiler.EXTENSION_POINT, new MachinePortTags.Annotations())
                .build();
    }

    public static ItemStack book() {
        return Guides.createGuideItem(ID);
    }
}
