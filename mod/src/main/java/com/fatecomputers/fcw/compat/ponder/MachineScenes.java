package com.fatecomputers.fcw.compat.ponder;

import com.fatecomputers.fcw.docs.DocStructures;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineState;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import com.fatecomputers.fcw.production.machine.euv.EuvPattern;
import net.createmod.catnip.math.Pointing;
import net.createmod.ponder.api.PonderPalette;
import net.createmod.ponder.api.scene.PonderStoryBoard;
import net.createmod.ponder.api.scene.SceneBuilder;
import net.createmod.ponder.api.scene.SceneBuildingUtil;
import net.createmod.ponder.api.scene.Selection;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The storyboards. One short sentence per step; everything a scene points at is computed from the
 * machine manifests (cells, ports), so a changed model needs new structure files and nothing here.
 *
 * <p>How a machine is operated (panel, breaker) is said in two shared sentences, {@code port_panel} and
 * {@code port_breaker}: when the controls change, those are the lines to rewrite.
 */
final class MachineScenes {
    /** English names for scene titles (the Russian ones are in the lang file). */
    private static final Map<String, String> NAMES = Map.of(
            "arc_furnace", "Arc Furnace",
            "crystal_puller", "Crystal Puller",
            "wafering_cell", "Wafering Cell",
            "chip_line", "Electronics Line",
            Machines.EUV, "EUV Scanner Mk2");

    /** Sentences used by several scenes: key -> English text. */
    static final Map<String, String> SHARED = new LinkedHashMap<>();

    static {
        SHARED.put("kit_place", "Right-click the ground: the whole machine is placed");
        SHARED.put("kit_front", "The operator panel faces the player who placed it");
        SHARED.put("kit_break", "Sneak and break any cell: the kit and the contents drop");
        SHARED.put("port_panel", "Operator panel: status, start and stop");
        SHARED.put("port_breaker", "Main breaker: the machine works only while it is on");
        SHARED.put("port_power", "Power input: FE");
        SHARED.put("port_fluids", "Fluid ports: accepted, not needed by the current recipes");
    }

    /** One step of a ports scene: what to outline and what to say (own text, or a shared key). */
    private record Note(PonderPalette color, String text, boolean shared, String... ports) {
        static Note in(String text, String... ports) {
            return new Note(PonderPalette.INPUT, text, false, ports);
        }

        static Note out(String text, String... ports) {
            return new Note(PonderPalette.OUTPUT, text, false, ports);
        }

        static Note other(String text, String... ports) {
            return new Note(PonderPalette.WHITE, text, false, ports);
        }

        static Note panel() {
            return new Note(PonderPalette.WHITE, "port_panel", true, "terminal");
        }

        static Note breaker() {
            return new Note(PonderPalette.WHITE, "port_breaker", true, "main_breaker");
        }

        static Note power() {
            return new Note(PonderPalette.RED, "port_power", true, "power");
        }

        static Note fluids(String... ports) {
            return new Note(PonderPalette.BLUE, "port_fluids", true, ports);
        }
    }

    private static final Map<String, List<Note>> NOTES = Map.of(
            "arc_furnace", List.of(
                    Note.in("Charge port on top: quartz, iron, coal, redstone, silicon", "charge"),
                    Note.out("Tap: silicon and steel come out here", "tap"),
                    Note.panel(), Note.breaker(), Note.power(),
                    Note.fluids("coolant_in", "coolant_out", "exhaust")),
            "crystal_puller", List.of(
                    Note.in("Charge port: polysilicon", "charge"),
                    Note.out("Ingot door: the finished boule", "ingot"),
                    Note.panel(), Note.breaker(), Note.power(),
                    Note.fluids("coolant_in", "coolant_out", "argon", "exhaust")),
            "wafering_cell", List.of(
                    Note.in("Ingot port: the silicon boule", "ingot"),
                    Note.out("Wafer port: polished wafers", "wafers"),
                    Note.panel(), Note.breaker(), Note.power(),
                    Note.fluids("slurry", "water")),
            "chip_line", List.of(
                    Note.in("Wafer port: patterned wafers and the metal for packaging", "wafer"),
                    Note.in("Board port: board blanks and the parts for assembly", "boards"),
                    Note.panel(), Note.breaker(),
                    Note.out("Board output: finished boards and memory modules", "out"),
                    Note.out("Chip tray: packaged chips and processors; the assembly also takes chips from it", "chips"),
                    Note.power(),
                    Note.fluids("etchant", "exhaust")),
            Machines.EUV, List.of(
                    Note.in("Two FOUP docks: wafers go in, patterned wafers come out", "wafer_dock_1", "wafer_dock_2"),
                    Note.in("Reticle port: the photomask stays inside and is not consumed", "reticle"),
                    Note.in("Tin drawer: one more item input, not needed by the current recipes", "tin"),
                    Note.panel(),
                    Note.breaker(), Note.power(),
                    Note.fluids("coolant_in", "hydrogen", "nitrogen", "exhaust"),
                    Note.other("Data port: reserved for control from a computer", "data")));

    private MachineScenes() {
    }

    // ───────────────────────── helpers

    /** Centres the view on a floor of sx by sz blocks and zooms out for the larger machines. */
    private static void stage(SceneBuilder scene, int sx, int sz, int height, float fit, float yOffset) {
        int size = Math.max(sx, sz);
        scene.configureBasePlate(Math.floorDiv(sx - size, 2), Math.floorDiv(sz - size, 2), size);
        scene.scaleSceneView(Math.min(1.0f, fit / Math.max(size, height * 1.5f)));
        scene.setSceneOffsetY(yOffset);
    }

    private static void stage(SceneBuilder scene, MachineDef def) {
        stage(scene, DocStructures.spanX(def) + 2 * DocStructures.MARGIN, DocStructures.spanZ(def) + 2 * DocStructures.MARGIN, def.sy, 6.0f, -def.sy / 3f);
    }

    private static AABB box(MachineDef def) {
        BlockPos a = DocStructures.cell(def, 0, 0, 0);
        BlockPos b = DocStructures.cell(def, def.sx - 1, def.sy - 1, def.sz - 1);
        return AABB.encapsulatingFullBlocks(a, b);
    }

    /** The camera shows the north and west sides first; half a turn shows the other two. */
    private static final class Camera {
        private boolean turned;

        void show(SceneBuilder scene, Direction face) {
            boolean back = face == Direction.SOUTH || face == Direction.EAST;
            if (face.getAxis() != Direction.Axis.Y && back != turned) {
                scene.rotateCameraY(180);
                scene.idle(25);
                turned = back;
            }
        }

        void reset(SceneBuilder scene) {
            show(scene, Direction.NORTH);
        }
    }

    // ───────────────────────── kit machines: placing and removing

    static PonderStoryBoard kit(MachineDef def) {
        return (scene, util) -> {
            scene.title(def.name + "_kit", NAMES.get(def.name) + ": placing and removing");
            stage(scene, def);
            ItemStack kit = ProductionMachines.sourceItem(def);
            Selection machine = util.select().layersFrom(1);
            AABB box = box(def);
            Vec3 top = new Vec3(box.getCenter().x, box.maxY, box.getCenter().z);

            scene.world().showSection(util.select().layer(0), Direction.UP);
            scene.idle(15);

            scene.overlay().chaseBoundingBoxOutline(PonderPalette.WHITE, "space", box, 80);
            // own text and not a shared one with parameters: those need a newer Ponder than the first 1.0 builds
            scene.overlay().showText(80).text("The kit needs " + def.sx + " x " + def.sy + " x " + def.sz + " free blocks: width, height, depth")
                    .pointAt(top).placeNearTarget();
            scene.idle(90);

            BlockPos ground = DocStructures.cell(def, def.sx / 2, 0, 0).below();
            scene.overlay().showControls(util.vector().topOf(ground), Pointing.DOWN, 40).rightClick().withItem(kit);
            scene.idle(20);
            scene.world().showSection(machine, Direction.DOWN);
            scene.idle(15);
            scene.overlay().showText(60).sharedText("kit_place").pointAt(top).placeNearTarget().attachKeyFrame();
            scene.idle(70);

            MachineDef.Port panel = def.terminal;
            BlockPos panelPos = DocStructures.cell(def, panel.ax(), panel.ay(), panel.az());
            scene.overlay().showOutlineWithText(util.select().position(panelPos), 60).colored(PonderPalette.WHITE)
                    .sharedText("kit_front").pointAt(util.vector().blockSurface(panelPos, DocStructures.face(panel.face())))
                    .placeNearTarget().attachKeyFrame();
            scene.idle(70);

            scene.overlay().showControls(util.vector().blockSurface(panelPos, DocStructures.face(panel.face())), Pointing.RIGHT, 40)
                    .leftClick().whileSneaking().withItem(new ItemStack(Items.IRON_PICKAXE));
            scene.idle(30);
            scene.world().hideSection(machine, Direction.UP);
            scene.idle(15);
            scene.world().createItemEntity(util.vector().topOf(ground).add(0, 1.5, 0), Vec3.ZERO, kit);
            scene.overlay().showText(70).sharedText("kit_break").pointAt(util.vector().topOf(ground).add(0, 0.5, 0)).placeNearTarget().attachKeyFrame();
            scene.idle(80);
            scene.markAsFinished();
        };
    }

    // ───────────────────────── ports

    static PonderStoryBoard ports(MachineDef def) {
        return (scene, util) -> {
            scene.title(def.name + "_ports", NAMES.get(def.name) + ": ports");
            stage(scene, def);
            scene.world().showSection(util.select().layer(0), Direction.UP);
            scene.idle(5);
            scene.world().showSection(util.select().layersFrom(1), Direction.DOWN);
            scene.idle(25);

            Camera camera = new Camera();
            for (Note note : NOTES.get(def.name)) {
                Selection selection = null;
                Vec3 target = null;
                for (String id : note.ports()) {
                    MachineDef.Port port = def.port(id);
                    BlockPos pos = DocStructures.cell(def, port.ax(), port.ay(), port.az());
                    Direction face = DocStructures.face(port.face());
                    if (selection == null) {
                        camera.show(scene, face);
                        selection = util.select().position(pos);
                        target = util.vector().blockSurface(pos, face);
                    } else {
                        selection = selection.add(util.select().position(pos));
                    }
                }
                var text = scene.overlay().showOutlineWithText(selection, 60).colored(note.color());
                (note.shared() ? text.sharedText(note.text()) : text.text(note.text()))
                        .pointAt(target).placeNearTarget().attachKeyFrame();
                scene.idle(70);
            }
            camera.reset(scene);
            scene.markAsFinished();
        };
    }

    // ───────────────────────── EUV scanner: building from the kit blocks

    private static final String[] LAYERS = {
            "Layer 1: wafer handler, wafer stage, vacuum chamber, laser amplifiers; ports in the back row",
            "Layer 2: controller in the front row, two FOUP load ports on the wafer end, chillers on the laser end",
            "Layer 3: mirrors above the wafer stage",
            "Layer 4: the collector inside the vacuum chamber",
            "Layer 5: reticle stage and droplet generator",
            "Layer 6: casing with vents"
    };

    static void euvBuild(SceneBuilder scene, SceneBuildingUtil util) {
        MachineDef def = Machines.EUV_SCANNER;
        EuvPattern pattern = EuvPattern.get();
        scene.title("euv_build", "Building the EUV Scanner Mk2");
        stage(scene, def);

        // one block of the top layer is wrong until the controller has pointed at it
        BlockPos wrong = DocStructures.cell(def, 5, 5, 2);
        BlockState right = pattern.stateAt(5, 5, 2, DocStructures.FACING);
        scene.world().setBlock(wrong, ProductionMachines.EUV_BLOCKS.get("euv_casing_glass").get().defaultBlockState(), false);

        scene.world().showSection(util.select().layer(0), Direction.UP);
        scene.idle(15);
        AABB box = box(def);
        scene.overlay().chaseBoundingBoxOutline(PonderPalette.WHITE, "space", box, 80);
        scene.overlay().showText(80).text("301 blocks of 23 kinds in a box of 13 x 6 x 5; the layout is in the guide")
                .pointAt(new Vec3(box.getCenter().x, box.maxY, box.getCenter().z)).placeNearTarget();
        scene.idle(90);

        for (int y = 0; y < def.sy; y++) {
            scene.world().showSection(util.select().layer(y + 1), Direction.DOWN);
            scene.idle(10);
            BlockPos at = DocStructures.cell(def, y < 4 ? 6 : 5, y, 0);
            scene.overlay().showText(60).text(LAYERS[y]).pointAt(util.vector().blockSurface(at, DocStructures.face(Direction.NORTH)))
                    .placeNearTarget().attachKeyFrame();
            scene.idle(70);
        }

        // blocks that must face outwards, front and wafer end first, then the back
        Camera camera = new Camera();
        Selection front = null, back = null;
        for (MachineDef.Cell cell : def.cells) {
            EuvPattern.Entry entry = pattern.entry(cell.x(), cell.y(), cell.z());
            if (entry.facing() == null) {
                continue;
            }
            Selection one = util.select().position(DocStructures.cell(def, cell.x(), cell.y(), cell.z()));
            if (entry.facing() == Direction.SOUTH || entry.facing() == Direction.EAST) {
                back = back == null ? one : back.add(one);
            } else {
                front = front == null ? one : front.add(one);
            }
        }
        scene.overlay().showOutlineWithText(front, 60).colored(PonderPalette.INPUT)
                .text("Load ports, item hatches and the controller face outwards")
                .pointAt(util.vector().blockSurface(DocStructures.cell(def, 0, 1, 1), DocStructures.face(Direction.WEST)))
                .placeNearTarget().attachKeyFrame();
        scene.idle(70);
        camera.show(scene, DocStructures.face(Direction.SOUTH));
        scene.overlay().showOutlineWithText(back, 60).colored(PonderPalette.INPUT)
                .text("Back row: data, gas, exhaust, energy and coolant ports, power cabinet")
                .pointAt(util.vector().blockSurface(DocStructures.cell(def, 8, 1, 4), DocStructures.face(Direction.SOUTH)))
                .placeNearTarget().attachKeyFrame();
        scene.idle(70);
        camera.reset(scene);

        MachineDef.Port panel = def.terminal;
        BlockPos controller = DocStructures.cell(def, panel.x(), panel.y(), panel.z());
        Vec3 controllerFace = util.vector().blockSurface(controller, DocStructures.face(panel.face()));
        scene.overlay().showControls(controllerFace, Pointing.RIGHT, 40).rightClick();
        scene.idle(20);
        scene.overlay().showOutlineWithText(util.select().position(wrong), 70).colored(PonderPalette.RED)
                .text("Right-click the controller: a wrong block is named in chat and marked with red dust")
                .pointAt(util.vector().topOf(wrong)).placeNearTarget().attachKeyFrame();
        scene.idle(80);
        scene.world().setBlock(wrong, right, true);
        scene.idle(20);

        scene.overlay().showControls(controllerFace, Pointing.RIGHT, 40).rightClick();
        scene.idle(20);
        for (MachineDef.Cell cell : def.cells) {
            scene.world().setBlock(DocStructures.cell(def, cell.x(), cell.y(), cell.z()), DocStructures.formed(def, cell, MachineState.OFF), false);
        }
        scene.effects().indicateSuccess(controller);
        scene.overlay().showText(70).text("When every block is in place the blank becomes the machine")
                .pointAt(controllerFace).placeNearTarget().attachKeyFrame();
        scene.idle(80);
        scene.overlay().showText(70).text("Breaking a cell while sneaking turns it back into the building blocks")
                .pointAt(new Vec3(box.getCenter().x, box.maxY, box.getCenter().z)).placeNearTarget();
        scene.idle(80);
        scene.markAsFinished();
    }

    // ───────────────────────── the chain

    private record Link(MachineDef def, String in, String out, String text) {
    }

    private static final List<Link> CHAIN = List.of(
            new Link(Machines.ARC_FURNACE, "charge", "tap", "Arc furnace: quartz to silicon, iron to steel, silicon to polysilicon"),
            new Link(Machines.CRYSTAL_PULLER, "charge", "ingot", "Crystal puller: 8 polysilicon to 1 boule"),
            new Link(Machines.WAFERING_CELL, "ingot", "wafers", "Wafering cell: 1 boule to 8 wafers"),
            new Link(Machines.EUV_SCANNER, "wafer_dock_1", "wafer_dock_2", "EUV scanner: wafer and reticle to a patterned wafer"),
            new Link(Machines.CHIP_LINE, "wafer", "out", "Electronics line: patterned wafers to chips, chips and board blanks to boards"));

    private static Vec3 chainPort(SceneBuildingUtil util, MachineDef def, String id) {
        MachineDef.Port port = def.port(id);
        return util.vector().blockSurface(DocStructures.chainCell(def, port.ax(), port.ay(), port.az()), DocStructures.face(port.face()));
    }

    static void chain(SceneBuilder scene, SceneBuildingUtil util) {
        scene.title("chain", "Production chain");
        stage(scene, DocStructures.CHAIN_SX, DocStructures.CHAIN_SZ, 7, 7.5f, 0f); // the floor is long and narrow
        scene.world().showSection(util.select().layer(0), Direction.UP);
        scene.idle(15);

        Link previous = null;
        for (Link link : CHAIN) {
            MachineDef def = link.def();
            BlockPos a = DocStructures.chainCell(def, 0, 0, 0);
            BlockPos b = DocStructures.chainCell(def, def.sx - 1, def.sy - 1, def.sz - 1);
            scene.world().showSection(util.select().fromTo(a, b), Direction.DOWN);
            scene.idle(15);
            if (previous != null) {
                scene.overlay().showBigLine(PonderPalette.OUTPUT, chainPort(util, previous.def(), previous.out()), chainPort(util, def, link.in()), 80);
            }
            AABB box = AABB.encapsulatingFullBlocks(a, b);
            scene.overlay().showText(70).text(link.text()).pointAt(new Vec3(box.getCenter().x, box.maxY, box.getCenter().z))
                    .placeNearTarget().attachKeyFrame();
            scene.idle(80);
            previous = link;
        }
        scene.overlay().showText(80).text("Item ports are item handlers: hoppers and pipes can link the machines").independent(20);
        scene.idle(90);
        scene.markAsFinished();
    }
}
