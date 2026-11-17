package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.machine.MachineSpec.Display;
import com.fatecomputers.fcw.production.machine.MachineSpec.Mode;
import com.fatecomputers.fcw.production.machine.MachineSpec.Phase;
import com.fatecomputers.fcw.production.machine.MachineSpec.PortRole;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.MachineSpec.When;
import com.fatecomputers.fcw.production.recipe.FcwRecipes;

import javax.annotation.Nullable;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** The five machines of the production chain (docs/PRODUCTION.md). */
public final class Machines {
    public static final String EUV = "euv_scanner_mk2";

    /**
     * Arc furnace: charge (top) -> tap (west). One recipe = ignite, melt (repeats), tap into the ladle,
     * the ladle comes back.
     */
    public static final MachineDef ARC_FURNACE = MachineDef.load("arc_furnace", new MachineSpec("furnace",
            List.of(new Section("main", () -> FcwRecipes.ARC_FURNACE.get(), 4, false, 4, -1, null)),
            List.of(new PortRole("charge", 0, Role.INPUT), new PortRole("tap", 0, Role.OUTPUT)),
            List.of(new Phase("ignite", Mode.FIXED), new Phase("melt", Mode.LOOP), new Phase("tap", Mode.FIXED),
                    new Phase("ladle_return", Mode.FIXED)),
            List.of(), null, null));

    /** Crystal puller: charge (west) -> ingot (north). Heat-up, growth stretched over the recipe, unloading. */
    public static final MachineDef CRYSTAL_PULLER = MachineDef.load("crystal_puller", new MachineSpec("puller",
            List.of(new Section("main", () -> FcwRecipes.CRYSTAL_PULLER.get(), 4, false, 4, -1, null)),
            List.of(new PortRole("charge", 0, Role.INPUT), new PortRole("ingot", 0, Role.OUTPUT)),
            List.of(new Phase("heatup", Mode.FIXED), new Phase("grow", Mode.STRETCH), new Phase("unload", Mode.FIXED)),
            List.of(), null, null));

    /** Wafering cell: ingot (west) -> wafers (north). The boule is sliced, then the wafers are polished one by one. */
    public static final MachineDef WAFERING_CELL = MachineDef.load("wafering_cell", new MachineSpec("wafering",
            List.of(new Section("main", () -> FcwRecipes.WAFERING.get(), 4, false, 4, -1, null)),
            List.of(new PortRole("ingot", 0, Role.INPUT), new PortRole("wafers", 0, Role.OUTPUT)),
            List.of(new Phase("slice", Mode.FIXED), new Phase("polish", Mode.LOOP)),
            List.of(new Display("ingot_on_feed", 0, When.INPUT_PRESENT, "saw_feed")), null, null));

    /**
     * EUV scanner: wafers through the FOUP docks (both load and unload; the tin drawer feeds the same input
     * slots), the reticle (catalyst) through the reticle port. Wafer load, then exposure fields in a loop.
     */
    public static final MachineDef EUV_SCANNER = MachineDef.load(EUV, new MachineSpec("",
            List.of(new Section("main", () -> FcwRecipes.LITHOGRAPHY.get(), 4, true, 4, -1, null)),
            List.of(new PortRole("wafer_dock_1", 0, Role.BOTH), new PortRole("wafer_dock_2", 0, Role.BOTH),
                    new PortRole("tin", 0, Role.INPUT), new PortRole("reticle", 0, Role.CATALYST)),
            List.of(new Phase("wafer_load", Mode.FIXED), new Phase("expose_enter", Mode.FIXED), new Phase("expose", Mode.LOOP),
                    new Phase("expose_exit", Mode.FIXED)),
            List.of(new Display("reticle_on_stage", 0, When.CATALYST_PRESENT, "reticle_stage"),
                    new Display("foup_dock_1", 0, When.INPUT_PRESENT, null),
                    new Display("foup_dock_2", 0, When.OUTPUT_PRESENT, null)),
            "startup", "mask_load"));

    /**
     * Chip line: packaging (wafer -> chips) and board assembly (boards + chips -> out) share one clip, the
     * line's takt. They are independent: chips go from the packaging tray to the assembly inputs by hand or by pipes.
     */
    public static final MachineDef CHIP_LINE = MachineDef.load("chip_line", new MachineSpec("line",
            List.of(new Section("packaging", () -> FcwRecipes.CHIP_PACKAGING.get(), 4, false, 4, -1, Set.of("wafer", "die", "chip")),
                    new Section("assembly", () -> FcwRecipes.PCB_ASSEMBLY.get(), 6, false, 2, -1,
                            Set.of("board_blank", "board_etched", "board_populated"))),
            List.of(new PortRole("wafer", 0, Role.INPUT), new PortRole("chips", 0, Role.OUTPUT),
                    new PortRole("boards", 1, Role.INPUT), new PortRole("out", 1, Role.OUTPUT)),
            List.of(new Phase("takt", Mode.LOOP)),
            List.of(), null, null));

    public static final Map<String, MachineDef> ALL = new LinkedHashMap<>();

    static {
        for (MachineDef d : List.of(ARC_FURNACE, CRYSTAL_PULLER, WAFERING_CELL, EUV_SCANNER, CHIP_LINE)) {
            ALL.put(d.name, d);
        }
    }

    private Machines() {
    }

    @Nullable
    public static MachineDef byName(String name) {
        return ALL.get(name);
    }
}
