package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import net.minecraft.world.item.crafting.RecipeType;

import javax.annotation.Nullable;
import java.util.List;
import java.util.Set;
import java.util.function.Supplier;

/**
 * What the code adds to a manifest: which recipes a machine runs, which item ports feed which slots,
 * which clips make up one recipe and which carried things are shown outside of clips.
 */
public record MachineSpec(String particlePrefix, List<Section> sections, List<PortRole> portRoles, List<Phase> phases,
                          List<Display> displays, @Nullable String startupClip, @Nullable String catalystClip) {

    /**
     * One recipe line of a machine (the chip line has two). A section may also take ingredients from the
     * output slots of an earlier one (chips for the board assembly).
     *
     * @param propModels models of the clip props that belong to this section (shown only while it works); null = all
     */
    public record Section(String id, Supplier<RecipeType<MachineRecipe>> recipeType, int inputs, boolean catalyst, int outputs,
                          int alsoTakesFromSection, @Nullable Set<String> propModels) {
        public int slots() {
            return inputs + (catalyst ? 1 : 0) + outputs;
        }
    }

    public enum Role {
        /** insert into the section's input slots */
        INPUT,
        /** extract from the section's output slots */
        OUTPUT,
        /** both of the above */
        BOTH,
        /** the catalyst slot, both ways */
        CATALYST
    }

    public record PortRole(String port, int section, Role role) {
    }

    public enum Mode {
        /** the clip plays once at its own speed (faster when the recipe is short) */
        FIXED,
        /** the clip repeats for the rest of the recipe time, a whole number of times */
        LOOP,
        /** the clip plays once, stretched over the rest of the recipe time */
        STRETCH
    }

    /** One clip of the work sequence; the whole sequence takes exactly the recipe's time. */
    public record Phase(String clip, Mode mode) {
    }

    public enum When {
        INPUT_PRESENT, OUTPUT_PRESENT, CATALYST_PRESENT
    }

    /**
     * A manifest slot (a carried thing at a fixed place) shown by the contents of the inventory rather than by
     * a clip. Hidden while the playing work clip moves a prop with the same model itself.
     */
    public record Display(String slot, int section, When when, @Nullable String carrierPart) {
    }
}
