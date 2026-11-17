package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import net.minecraft.world.item.crafting.RecipeType;

import javax.annotation.Nullable;
import java.util.List;
import java.util.Set;
import java.util.function.Supplier;

/**
 * What the code adds to a manifest: which recipes a machine runs, which item ports feed which slots,
 * where a player loads and unloads it by hand, which clips make up one recipe and which carried things
 * are shown outside of clips.
 *
 * @param namePrefix default machine names are {@code <prefix>-<number>}: euv-1, eaf-2 (docs/MACHINE_CONTROL.md)
 */
public record MachineSpec(String particlePrefix, String namePrefix, List<Section> sections, List<PortRole> portRoles,
                          List<Zone> zones, List<Phase> phases, List<Display> displays,
                          @Nullable String startupClip, @Nullable String catalystClip) {

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
        /** both of the above (automation ports only) */
        BOTH,
        /** the catalyst slot, both ways */
        CATALYST,
        /**
         * hand zones only: the output tray of a section that the next section also takes from. Results are
         * taken here; parts for the next section may be added here.
         */
        TRAY
    }

    /** An automation port (hoppers, pipes): which slots the item handler on that port reaches. */
    public record PortRole(String port, int section, Role role) {
    }

    /**
     * A place where a player loads or unloads the machine by hand (docs/design/MACHINE_UI.md): right click
     * with an item puts the stack in, with an empty hand takes one stack, sneaking takes everything.
     *
     * @param id          also the lang key part: interact.fcw.&lt;machine&gt;.&lt;id&gt; (and ….accepts)
     * @param port        manifest port whose cell (and reachable cell) belong to the zone; its position is
     *                    where the contents are shown; null for zones made of hatch cells only
     * @param hatch       hatch whose cells belong to the zone (a door that is opened by loading); null = none
     * @param handTakes   false: a plain empty-hand click keeps its old meaning (open/close the doors) and only
     *                    a sneaking click takes things back
     * @param showsItem   draw the first stack of the contents at the port (until the artist's props arrive)
     * @param extraCells  more cells of the zone, for ports buried behind other cells
     */
    public record Zone(String id, int section, Role role, @Nullable String port, @Nullable String hatch,
                       boolean handTakes, boolean showsItem, List<int[]> extraCells) {
        public static Zone of(String id, int section, Role role) {
            return new Zone(id, section, role, id, null, true, false, List.of());
        }

        public Zone hatch(String hatch) {
            return new Zone(id, section, role, port, hatch, handTakes, showsItem, extraCells);
        }

        public Zone shown() {
            return new Zone(id, section, role, port, hatch, handTakes, true, extraCells);
        }

        public Zone cells(int[]... cells) {
            return new Zone(id, section, role, port, hatch, handTakes, showsItem, List.of(cells));
        }
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
