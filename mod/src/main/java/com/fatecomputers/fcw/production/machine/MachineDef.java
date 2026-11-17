package com.fatecomputers.fcw.production.machine;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.resources.ResourceLocation;

import javax.annotation.Nullable;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * One multiblock machine as described by its manifest ({@code assets/fcw/machines/<name>_manifest.json})
 * and clip file, plus what the code adds on top ({@link MachineSpec}). Read from the mod jar when the
 * blocks are registered, on both sides: block properties and shapes depend on it.
 *
 * <p>Structure space: x east, y up, z south, operator side = north (z = 0), for facing = north.
 */
public final class MachineDef {
    public record Cell(int x, int y, int z, @Nullable String hatch, @Nullable String hatchStateId, double[] bboxMin, double[] bboxMax) {
    }

    public record Hatch(String id, String kind, List<Cell> cells) {
        public boolean isLever() {
            return kind.equals("lever");
        }
    }

    /**
     * kind: item, fluid, energy, gui, switch, data. The face is given for facing = north. x, y, z is the
     * cell of the manifest; ax, ay, az is the cell where the port can actually be reached: a few ports sit
     * behind other cells of the same machine (the furnace's charge under the tower top), those are moved
     * straight out along their face to the surface.
     */
    public record Port(String id, String kind, Direction face, int x, int y, int z, int ax, int ay, int az, double[] pos) {
    }

    /** A model from dyn/: a moving part (always drawn) or a carried thing (drawn by clips and slot displays). */
    public record Dyn(String id, boolean moving, Map<MachineState, ResourceLocation> models, int[] anchor) {
    }

    public record Slot(String id, String model, double[] offset) {
    }

    public static final String BREAKER = "main_breaker";

    public final String name;
    public final int sx, sy, sz;
    public final List<Cell> cells = new ArrayList<>();
    private final Cell[] grid;
    public final Map<String, Hatch> hatches = new LinkedHashMap<>();
    /** Values of the hatch block's "hatch" property (e.g. foup_port_1_0) and their cells. */
    public final Map<String, Cell> hatchCells = new LinkedHashMap<>();
    public final List<Port> ports = new ArrayList<>();
    public final Port terminal;
    private final Map<Long, MachineSpec.Zone> zoneCells = new java.util.HashMap<>();
    public final Map<String, Dyn> dynamic = new LinkedHashMap<>();
    public final Map<String, Slot> slots = new LinkedHashMap<>();
    public final MachineAnimation anim;
    public final MachineSpec spec;

    private MachineDef(String name, JsonObject manifest, MachineAnimation anim, MachineSpec spec) {
        this.name = name;
        this.anim = anim;
        this.spec = spec;
        JsonObject size = manifest.getAsJsonObject("size");
        sx = size.get("x").getAsInt();
        sy = size.get("y").getAsInt();
        sz = size.get("z").getAsInt();
        grid = new Cell[sx * sy * sz];

        // hatch cells first: cell -> (hatch, blockstate id)
        Map<String, String[]> hatchOf = new LinkedHashMap<>();
        JsonObject hatchesJson = manifest.getAsJsonObject("hatches");
        for (Map.Entry<String, JsonElement> e : hatchesJson.entrySet()) {
            JsonObject h = e.getValue().getAsJsonObject();
            JsonArray hc = h.getAsJsonArray("cells");
            JsonArray ids = h.getAsJsonArray("blockstate_ids");
            for (int i = 0; i < hc.size(); i++) {
                JsonArray c = hc.get(i).getAsJsonArray();
                hatchOf.put(c.get(0).getAsInt() + "," + c.get(1).getAsInt() + "," + c.get(2).getAsInt(),
                        new String[] {e.getKey(), ids.get(i).getAsString()});
            }
        }
        for (Map.Entry<String, JsonElement> e : manifest.getAsJsonObject("cells").entrySet()) {
            String[] xyz = e.getKey().split(",");
            int x = Integer.parseInt(xyz[0]), y = Integer.parseInt(xyz[1]), z = Integer.parseInt(xyz[2]);
            JsonArray bbox = e.getValue().getAsJsonObject().getAsJsonArray("bbox");
            String[] hatch = hatchOf.get(e.getKey());
            Cell cell = new Cell(x, y, z, hatch == null ? null : hatch[0], hatch == null ? null : hatch[1],
                    MachineAnimation.doubles(bbox.get(0).getAsJsonArray()), MachineAnimation.doubles(bbox.get(1).getAsJsonArray()));
            cells.add(cell);
            grid[index(x, y, z)] = cell;
            if (hatch != null) {
                hatchCells.put(hatch[1], cell);
            }
        }
        for (Map.Entry<String, JsonElement> e : hatchesJson.entrySet()) {
            JsonObject h = e.getValue().getAsJsonObject();
            List<Cell> list = new ArrayList<>();
            for (JsonElement id : h.getAsJsonArray("blockstate_ids")) {
                list.add(hatchCells.get(id.getAsString()));
            }
            hatches.put(e.getKey(), new Hatch(e.getKey(), h.get("kind").getAsString(), List.copyOf(list)));
        }
        Port term = null;
        for (JsonElement pe : manifest.getAsJsonArray("ports")) {
            JsonObject p = pe.getAsJsonObject();
            JsonArray c = p.getAsJsonArray("cell");
            Direction face = Direction.byName(p.get("face").getAsString());
            int px = c.get(0).getAsInt(), py = c.get(1).getAsInt(), pz = c.get(2).getAsInt();
            int ax = px, ay = py, az = pz;
            while (cell(ax + face.getStepX(), ay + face.getStepY(), az + face.getStepZ()) != null) {
                ax += face.getStepX();
                ay += face.getStepY();
                az += face.getStepZ();
            }
            Port port = new Port(p.get("id").getAsString(), p.get("kind").getAsString(), face, px, py, pz, ax, ay, az,
                    MachineAnimation.doubles(p.getAsJsonArray("pos")));
            ports.add(port);
            if (port.kind.equals("gui")) {
                term = port;
            }
        }
        if (term == null || cell(term.x, term.y, term.z) == null) {
            throw new IllegalStateException("machine " + name + " has no terminal cell");
        }
        terminal = term;
        for (MachineSpec.Zone zone : spec.zones()) {
            Port port = zone.port() == null ? null : port(zone.port());
            if (port != null) {
                zoneCells.put(key(port.x, port.y, port.z), zone);
                if (port.ax != terminal.x || port.ay != terminal.y || port.az != terminal.z) {
                    zoneCells.put(key(port.ax, port.ay, port.az), zone);
                }
            }
            if (zone.hatch() != null) {
                for (Cell c : hatches.get(zone.hatch()).cells()) {
                    zoneCells.put(key(c.x(), c.y(), c.z()), zone);
                }
            }
            for (int[] c : zone.extraCells()) {
                zoneCells.put(key(c[0], c[1], c[2]), zone);
            }
        }
        for (Map.Entry<String, JsonElement> e : manifest.getAsJsonObject("dynamic").entrySet()) {
            JsonObject d = e.getValue().getAsJsonObject();
            Map<MachineState, ResourceLocation> models = new EnumMap<>(MachineState.class);
            for (MachineState s : MachineState.values()) {
                // the pallet state has no models of its own: the machine is not there yet
                String key = s == MachineState.SITE ? "off" : s.getSerializedName();
                models.put(s, ResourceLocation.parse(d.getAsJsonObject("models").get(key).getAsString()));
            }
            JsonArray an = d.getAsJsonArray("anchor");
            dynamic.put(e.getKey(), new Dyn(e.getKey(), d.get("kind").getAsString().equals("moving"), models,
                    new int[] {an.get(0).getAsInt(), an.get(1).getAsInt(), an.get(2).getAsInt()}));
        }
        for (JsonElement se : manifest.getAsJsonArray("slots")) {
            JsonObject s = se.getAsJsonObject();
            slots.put(s.get("id").getAsString(), new Slot(s.get("id").getAsString(), s.get("accepts").getAsString(),
                    MachineAnimation.doubles(s.getAsJsonArray("offset"))));
        }
    }

    static MachineDef load(String name, MachineSpec spec) {
        JsonObject manifest = readJson("/assets/fcw/machines/" + name + "_manifest.json");
        MachineAnimation anim = MachineAnimation.fromJson(readJson("/assets/fcw/machines/" + name + "_animations.json"));
        return new MachineDef(name, manifest, anim, spec);
    }

    public static JsonObject readJson(String path) {
        try (InputStream in = MachineDef.class.getResourceAsStream(path)) {
            if (in == null) {
                throw new IllegalStateException("missing resource " + path);
            }
            try (Reader r = new InputStreamReader(in, StandardCharsets.UTF_8)) {
                return new Gson().fromJson(r, JsonObject.class);
            }
        } catch (IOException e) {
            throw new IllegalStateException("cannot read " + path, e);
        }
    }

    private static long key(int x, int y, int z) {
        return BlockPos.asLong(x, y, z);
    }

    /** The hand-loading zone a cell belongs to, or null. */
    @Nullable
    public MachineSpec.Zone zoneAt(Cell c) {
        return zoneCells.get(key(c.x(), c.y(), c.z()));
    }

    private int index(int x, int y, int z) {
        return x + sx * (z + sz * y);
    }

    @Nullable
    public Cell cell(int x, int y, int z) {
        return x < 0 || y < 0 || z < 0 || x >= sx || y >= sy || z >= sz ? null : grid[index(x, y, z)];
    }

    /** Whether a right click on this cell opens the terminal: the terminal cell itself or a cell in front of it. */
    public boolean opensTerminal(Cell c) {
        Direction f = terminal.face();
        for (int x = terminal.x(), y = terminal.y(), z = terminal.z(); cell(x, y, z) != null;
             x += f.getStepX(), y += f.getStepY(), z += f.getStepZ()) {
            if (c.x() == x && c.y() == y && c.z() == z) {
                return true;
            }
        }
        return false;
    }

    @Nullable
    public Port port(String id) {
        for (Port p : ports) {
            if (p.id.equals(id)) {
                return p;
            }
        }
        return null;
    }

    // ───────────────────────── placement in the world

    /** Offset of a cell from the structure origin: north (x, z), east (-z, x), south (-x, -z), west (z, -x). */
    public static BlockPos offset(Direction facing, int x, int y, int z) {
        return switch (facing) {
            case EAST -> new BlockPos(-z, y, x);
            case SOUTH -> new BlockPos(-x, y, -z);
            case WEST -> new BlockPos(z, y, -x);
            default -> new BlockPos(x, y, z);
        };
    }

    public static BlockPos worldPos(BlockPos origin, Direction facing, int x, int y, int z) {
        return origin.offset(offset(facing, x, y, z));
    }

    public static BlockPos worldPos(BlockPos origin, Direction facing, Cell cell) {
        return worldPos(origin, facing, cell.x, cell.y, cell.z);
    }

    /** Origin (cell 0,0,0) of the structure whose given cell stands at pos. */
    public static BlockPos origin(BlockPos pos, Direction facing, int x, int y, int z) {
        return pos.subtract(offset(facing, x, y, z));
    }

    public BlockPos controllerPos(BlockPos origin, Direction facing) {
        return worldPos(origin, facing, terminal.x, terminal.y, terminal.z);
    }

    /** A direction given for facing = north, turned with the structure. */
    public static Direction rotate(Direction facing, Direction dir) {
        if (dir.getAxis() == Direction.Axis.Y) {
            return dir;
        }
        BlockPos o = offset(facing, dir.getStepX(), 0, dir.getStepZ());
        return Direction.fromDelta(o.getX(), 0, o.getZ());
    }

    /** Rotation about +y (degrees, as for Axis.YP) that turns structure space into the world for this facing. */
    public static float yRotation(Direction facing) {
        return switch (facing) {
            case EAST -> -90;
            case SOUTH -> 180;
            case WEST -> 90;
            default -> 0;
        };
    }
}
