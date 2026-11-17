package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.machine.MachineSpec.Display;
import com.fatecomputers.fcw.production.machine.MachineSpec.Mode;
import com.fatecomputers.fcw.production.machine.MachineSpec.Phase;
import com.fatecomputers.fcw.production.machine.MachineSpec.PortRole;
import com.fatecomputers.fcw.production.machine.MachineSpec.Role;
import com.fatecomputers.fcw.production.machine.MachineSpec.Section;
import com.fatecomputers.fcw.production.machine.MachineSpec.When;
import com.fatecomputers.fcw.production.machine.MachineSpec.Zone;
import com.fatecomputers.fcw.production.recipe.FcwRecipes;

import javax.annotation.Nullable;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** The five machines of the production chain (docs/PRODUCTION.md, hand loading: docs/design/MACHINE_UI.md). */
public final class Machines {
    public static final String EUV = "euv_scanner_mk2";

    /**
     * Arc furnace: charge through the hood doors (or from the top for hoppers) -> tap tray at the ladle.
     * One recipe = ignite, melt (repeats), tap into the ladle, the ladle comes back.
     */
    public static final MachineDef ARC_FURNACE = MachineDef.load("arc_furnace", new MachineSpec("furnace", "eaf",
            List.of(new Section("main", () -> FcwRecipes.ARC_FURNACE.get(), 4, false, 4, -1, null)),
            List.of(new PortRole("charge", 0, Role.INPUT), new PortRole("tap", 0, Role.OUTPUT)),
            List.of(new Zone("hood", 0, Role.INPUT, null, "hood_doors", false, false, List.of()),
                    Zone.of("charge", 0, Role.INPUT),
                    Zone.of("tap", 0, Role.OUTPUT).shown()),
            List.of(new Phase("ignite", Mode.FIXED), new Phase("melt", Mode.LOOP), new Phase("tap", Mode.FIXED),
                    new Phase("ladle_return", Mode.FIXED)),
            List.of(), null, null));

    /** Crystal puller: charge hatch (west) -> ingot door (north). Heat-up, growth stretched over the recipe, unloading. */
    public static final MachineDef CRYSTAL_PULLER = MachineDef.load("crystal_puller", new MachineSpec("puller", "cz",
            List.of(new Section("main", () -> FcwRecipes.CRYSTAL_PULLER.get(), 4, false, 4, -1, null)),
            List.of(new PortRole("charge", 0, Role.INPUT), new PortRole("ingot", 0, Role.OUTPUT)),
            List.of(Zone.of("charge", 0, Role.INPUT).shown(),
                    Zone.of("ingot", 0, Role.OUTPUT).hatch("ingot_door").shown()),
            List.of(new Phase("heatup", Mode.FIXED), new Phase("grow", Mode.STRETCH), new Phase("unload", Mode.FIXED)),
            List.of(), null, null));

    /** Wafering cell: ingot feed (west) -> wafer cassette (north). The boule is sliced, then the wafers are polished. */
    public static final MachineDef WAFERING_CELL = MachineDef.load("wafering_cell", new MachineSpec("wafering", "saw",
            List.of(new Section("main", () -> FcwRecipes.WAFERING.get(), 4, false, 4, -1, null)),
            List.of(new PortRole("ingot", 0, Role.INPUT), new PortRole("wafers", 0, Role.OUTPUT)),
            List.of(Zone.of("ingot", 0, Role.INPUT),
                    Zone.of("wafers", 0, Role.OUTPUT).shown()),
            List.of(new Phase("slice", Mode.FIXED), new Phase("polish", Mode.LOOP)),
            List.of(new Display("ingot_on_feed", 0, When.INPUT_PRESENT, "saw_feed")), null, null));

    /**
     * EUV scanner: wafers in at dock 1, patterned wafers out at dock 2, the reticle (catalyst) in the reticle
     * port. The tin drawer is not a hand zone until a recipe uses tin. Automation may use both docks both ways. Wafer load,
     * then exposure fields in a loop.
     */
    public static final MachineDef EUV_SCANNER = MachineDef.load(EUV, new MachineSpec("", "euv",
            List.of(new Section("main", () -> FcwRecipes.LITHOGRAPHY.get(), 4, true, 4, -1, null)),
            List.of(new PortRole("wafer_dock_1", 0, Role.BOTH), new PortRole("wafer_dock_2", 0, Role.BOTH),
                    new PortRole("tin", 0, Role.INPUT), new PortRole("reticle", 0, Role.CATALYST)),
            List.of(Zone.of("wafer_dock_1", 0, Role.INPUT).hatch("foup_port_1"),
                    Zone.of("wafer_dock_2", 0, Role.OUTPUT).hatch("foup_port_2"),
                    // Лотка олова в руках нет: пока ни один рецепт не тратит олово (источник EUV придёт с
                    // сырьём уровня fab, docs/design/HARDCORE.md), иначе лоток показывал бы слоты дока 1.
                    Zone.of("reticle", 0, Role.CATALYST).hatch("reticle_port")),
            List.of(new Phase("wafer_load", Mode.FIXED), new Phase("expose_enter", Mode.FIXED), new Phase("expose", Mode.LOOP),
                    new Phase("expose_exit", Mode.FIXED)),
            List.of(new Display("reticle_on_stage", 0, When.CATALYST_PRESENT, "reticle_stage"),
                    new Display("foup_dock_1", 0, When.INPUT_PRESENT, null),
                    new Display("foup_dock_2", 0, When.OUTPUT_PRESENT, null)),
            "startup", "mask_load"));

    /**
     * Chip line: packaging (wafer -> chips) and board assembly (boards + chips -> out) share one clip, the
     * line's takt. The assembly also takes chips straight from the packaging tray; which board it makes is
     * what the loaded parts allow, or the recipe pinned from the console.
     */
    public static final MachineDef CHIP_LINE = MachineDef.load("chip_line", new MachineSpec("line", "line",
            List.of(new Section("packaging", () -> FcwRecipes.CHIP_PACKAGING.get(), 4, false, 4, -1, Set.of("wafer", "die", "chip")),
                    new Section("assembly", () -> FcwRecipes.PCB_ASSEMBLY.get(), 6, false, 2, 0,
                            Set.of("board_blank", "board_etched", "board_populated"))),
            List.of(new PortRole("wafer", 0, Role.INPUT), new PortRole("chips", 0, Role.OUTPUT),
                    new PortRole("boards", 1, Role.INPUT), new PortRole("out", 1, Role.OUTPUT)),
            List.of(Zone.of("wafer", 0, Role.INPUT),
                    Zone.of("chips", 0, Role.TRAY).shown(),
                    Zone.of("boards", 1, Role.INPUT).shown().cells(new int[] {2, 1, 0}),
                    Zone.of("out", 1, Role.OUTPUT).shown()),
            List.of(new Phase("takt", Mode.LOOP)),
            List.of(new Display("wafer_on_saw", 0, When.INPUT_PRESENT, "saw_table")), null, null));

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
