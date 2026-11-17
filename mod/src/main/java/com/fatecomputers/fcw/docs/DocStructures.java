package com.fatecomputers.fcw.docs;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineState;
import com.fatecomputers.fcw.production.machine.MachineStructure;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.euv.EuvPattern;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Vec3i;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * The structures the documentation shows (Ponder scenes and the 3D scenes of the guidebook) and how they
 * are laid out. The files are not drawn by hand: {@code ./gradlew runServer -Pfcw.docsExport} places the
 * real machines in a world, saves them the way a structure block would and stops (docs/GUIDE.md). Run it
 * again whenever the machines change.
 *
 * <p>Layout of a schematic: a floor at y = 0 with a margin of {@link #MARGIN} around the machine, the
 * machine on top of it. The guidebook files hold the machine alone.
 */
@EventBusSubscriber(modid = FcwMod.MODID)
public final class DocStructures {
    /** Which way the operator side looks in every schematic. */
    public static final Direction FACING = Direction.NORTH;
    public static final int MARGIN = 1;
    /** Free blocks between two machines in the chain schematic. */
    public static final int GAP = 2;

    /** The EUV blank laid out from the kit blocks; the scene turns it into the machine itself. */
    public static final String EUV_BLANK = "euv_blank";
    public static final String CHAIN = "chain";

    /** Where each machine stands in the chain schematic: x and z of its north-west bottom corner. */
    public static final Map<MachineDef, int[]> CHAIN_SPOTS = new LinkedHashMap<>();
    public static final int CHAIN_SX, CHAIN_SZ;

    static {
        // two rows, the flow goes along the first row and then along the second
        int x = MARGIN;
        int depth = 0;
        for (MachineDef def : new MachineDef[] {Machines.ARC_FURNACE, Machines.CRYSTAL_PULLER, Machines.WAFERING_CELL}) {
            CHAIN_SPOTS.put(def, new int[] {x, MARGIN});
            x += spanX(def) + GAP;
            depth = Math.max(depth, spanZ(def));
        }
        int width = x - GAP;
        int second = MARGIN + depth + GAP + 1;
        x = MARGIN;
        int depth2 = 0;
        for (MachineDef def : new MachineDef[] {Machines.EUV_SCANNER, Machines.CHIP_LINE}) {
            CHAIN_SPOTS.put(def, new int[] {x, second});
            x += spanX(def) + GAP;
            depth2 = Math.max(depth2, spanZ(def));
        }
        CHAIN_SX = Math.max(width, x - GAP) + MARGIN;
        CHAIN_SZ = second + depth2 + MARGIN;
    }

    private DocStructures() {
    }

    // ───────────────────────── layout, shared with the scenes

    /** Origin (cell 0,0,0) of a machine whose north-west bottom corner is at (x, 1, z). */
    private static BlockPos originAt(MachineDef def, int x, int z) {
        BlockPos far = MachineDef.offset(FACING, def.sx - 1, 0, def.sz - 1);
        return new BlockPos(x - Math.min(0, far.getX()), 1, z - Math.min(0, far.getZ()));
    }

    /** Footprint along x and z as the machine stands in a schematic. */
    public static int spanX(MachineDef def) {
        return Math.abs(MachineDef.offset(FACING, def.sx - 1, 0, def.sz - 1).getX()) + 1;
    }

    public static int spanZ(MachineDef def) {
        return Math.abs(MachineDef.offset(FACING, def.sx - 1, 0, def.sz - 1).getZ()) + 1;
    }

    /** A cell of the machine in its own schematic. */
    public static BlockPos cell(MachineDef def, int x, int y, int z) {
        return MachineDef.worldPos(originAt(def, MARGIN, MARGIN), FACING, x, y, z);
    }

    /** A cell of the machine in its guidebook structure (the machine alone, no floor). */
    public static BlockPos guideCell(MachineDef def, int x, int y, int z) {
        return cell(def, x, y, z).offset(-MARGIN, -1, -MARGIN);
    }

    /** A cell of a machine in the chain schematic. */
    public static BlockPos chainCell(MachineDef def, int x, int y, int z) {
        int[] spot = CHAIN_SPOTS.get(def);
        return MachineDef.worldPos(originAt(def, spot[0], spot[1]), FACING, x, y, z);
    }

    /** A direction of the manifest (given for facing = north) as it points in the schematics. */
    public static Direction face(Direction manifestFace) {
        return MachineDef.rotate(FACING, manifestFace);
    }

    /** The formed cell the way the machine itself would place it. */
    public static BlockState formed(MachineDef def, MachineDef.Cell cell, MachineState state) {
        return MachineStructure.cellState(def, cell, FACING, state, false);
    }

    // ───────────────────────── export

    @SubscribeEvent
    static void onServerStarted(ServerStartedEvent event) {
        String out = System.getProperty("fcw.docsExport");
        if (out == null || !event.getServer().isDedicatedServer()) {
            return;
        }
        MinecraftServer server = event.getServer();
        try {
            export(server.overworld(), Path.of(out, FcwMod.MODID));
            FcwMod.LOGGER.info("[docs] structures written to {}", out);
        } catch (IOException | RuntimeException e) {
            FcwMod.LOGGER.error("[docs] structure export failed", e);
        }
        server.halt(false);
    }

    private static void export(ServerLevel level, Path assets) throws IOException {
        Path ponder = assets.resolve("ponder");
        Path guide = assets.resolve("fcwguide/assets/structures");
        Files.createDirectories(ponder);
        Files.createDirectories(guide);
        int slot = 0;
        for (MachineDef def : Machines.ALL.values()) {
            BlockPos base = base(slot++);
            Vec3i size = new Vec3i(spanX(def) + 2 * MARGIN, def.sy + 1, spanZ(def) + 2 * MARGIN);
            floor(level, base, size);
            MachineStructure.place(level, base.offset(originAt(def, MARGIN, MARGIN)), FACING, def);
            save(level, base, size, ponder.resolve(def.name + ".nbt"));
            save(level, base.offset(MARGIN, 1, MARGIN), new Vec3i(spanX(def), def.sy, spanZ(def)), guide.resolve(def.name + ".nbt"));
        }

        MachineDef euv = Machines.EUV_SCANNER;
        BlockPos base = base(slot++);
        Vec3i size = new Vec3i(spanX(euv) + 2 * MARGIN, euv.sy + 1, spanZ(euv) + 2 * MARGIN);
        floor(level, base, size);
        BlockPos origin = base.offset(originAt(euv, MARGIN, MARGIN));
        for (MachineDef.Cell cell : euv.cells) {
            level.setBlock(MachineDef.worldPos(origin, FACING, cell), EuvPattern.get().stateAt(cell.x(), cell.y(), cell.z(), FACING), Block.UPDATE_CLIENTS);
        }
        save(level, base, size, ponder.resolve(EUV_BLANK + ".nbt"));
        save(level, base.offset(MARGIN, 1, MARGIN), new Vec3i(spanX(euv), euv.sy, spanZ(euv)), guide.resolve(EUV_BLANK + ".nbt"));
        for (int y = 0; y < euv.sy; y++) {
            // the layers of the blank one by one, for the building page of the guide
            save(level, base.offset(MARGIN, 1 + y, MARGIN), new Vec3i(spanX(euv), 1, spanZ(euv)), guide.resolve(EUV_BLANK + "_layer_" + (y + 1) + ".nbt"));
        }

        base = base(slot);
        int height = 1;
        for (MachineDef def : CHAIN_SPOTS.keySet()) {
            height = Math.max(height, def.sy + 1);
        }
        size = new Vec3i(CHAIN_SX, height, CHAIN_SZ);
        floor(level, base, size);
        for (Map.Entry<MachineDef, int[]> e : CHAIN_SPOTS.entrySet()) {
            MachineStructure.place(level, base.offset(originAt(e.getKey(), e.getValue()[0], e.getValue()[1])), FACING, e.getKey());
        }
        save(level, base, size, ponder.resolve(CHAIN + ".nbt"));
        save(level, base, size, guide.resolve(CHAIN + ".nbt"));
    }

    /** Every structure gets its own stretch of sky, far from spawn and from each other. */
    private static BlockPos base(int slot) {
        return new BlockPos(4096 + slot * 64, 200, 4096);
    }

    private static void floor(ServerLevel level, BlockPos base, Vec3i size) {
        for (int x = 0; x < size.getX(); x++) {
            for (int z = 0; z < size.getZ(); z++) {
                boolean dark = ((x + z) & 1) == 0;
                level.setBlock(base.offset(x, 0, z), (dark ? Blocks.LIGHT_GRAY_CONCRETE : Blocks.WHITE_CONCRETE).defaultBlockState(), Block.UPDATE_CLIENTS);
            }
        }
    }

    private static void save(ServerLevel level, BlockPos min, Vec3i size, Path file) throws IOException {
        StructureTemplate template = new StructureTemplate();
        template.fillFromWorld(level, min, size, false, Blocks.AIR);
        CompoundTag tag = template.save(new CompoundTag());
        NbtIo.writeCompressed(tag, file);
    }
}
