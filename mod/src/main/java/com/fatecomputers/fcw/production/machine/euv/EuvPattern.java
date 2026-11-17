package com.fatecomputers.fcw.production.machine.euv;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.Machines;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import javax.annotation.Nullable;
import java.util.HashMap;
import java.util.Map;

/**
 * The building plan of the EUV scanner ({@code euv_scanner_mk2_pattern.json} from the parts kit): which
 * kit block stands in which cell, and for ports and the controller which way it must face. The occupied
 * cells are exactly the cells of the finished machine.
 */
public final class EuvPattern {
    /** @param facing for facing = north of the structure; null where the direction does not matter */
    public record Entry(Block block, @Nullable Direction facing) {
    }

    /** The first thing wrong with a laid-out blank. */
    public record Mismatch(BlockPos pos, int x, int y, int z, Entry expected, BlockState found, boolean wrongFacing, Direction needFacing) {
    }

    private static EuvPattern instance;

    private final MachineDef def = Machines.EUV_SCANNER;
    private final Map<Long, Entry> entries = new HashMap<>();

    private EuvPattern() {
        JsonObject json = MachineDef.readJson("/assets/fcw/machines/euv_scanner_mk2_pattern.json");
        Map<Character, Block> key = new HashMap<>();
        for (Map.Entry<String, JsonElement> e : json.getAsJsonObject("key").entrySet()) {
            key.put(e.getKey().charAt(0), BuiltInRegistries.BLOCK.get(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, e.getValue().getAsString())));
        }
        Map<Long, Direction> oriented = new HashMap<>();
        for (JsonElement oe : json.getAsJsonArray("oriented")) {
            JsonObject o = oe.getAsJsonObject();
            JsonArray c = o.getAsJsonArray("cell");
            oriented.put(BlockPos.asLong(c.get(0).getAsInt(), c.get(1).getAsInt(), c.get(2).getAsInt()), Direction.byName(o.get("facing").getAsString()));
        }
        JsonArray layers = json.getAsJsonArray("layers");
        for (int y = 0; y < layers.size(); y++) {
            JsonArray rows = layers.get(y).getAsJsonArray();
            for (int z = 0; z < rows.size(); z++) {
                String row = rows.get(z).getAsString();
                for (int x = 0; x < row.length(); x++) {
                    char ch = row.charAt(x);
                    if (ch != '.') {
                        long k = BlockPos.asLong(x, y, z);
                        entries.put(k, new Entry(key.get(ch), oriented.get(k)));
                    }
                }
            }
        }
        for (MachineDef.Cell cell : def.cells) {
            if (!entries.containsKey(BlockPos.asLong(cell.x(), cell.y(), cell.z()))) {
                throw new IllegalStateException("EUV pattern has no block for cell " + cell.x() + "," + cell.y() + "," + cell.z());
            }
        }
    }

    public static synchronized EuvPattern get() {
        if (instance == null) {
            instance = new EuvPattern();
        }
        return instance;
    }

    public Entry entry(int x, int y, int z) {
        return entries.get(BlockPos.asLong(x, y, z));
    }

    /** The kit block of a cell as it stands in a structure with this facing. */
    public BlockState stateAt(int x, int y, int z, Direction structureFacing) {
        Entry e = entry(x, y, z);
        BlockState state = e.block().defaultBlockState();
        Direction dir = MachineDef.rotate(structureFacing, e.facing() == null ? Direction.NORTH : e.facing());
        if (state.hasProperty(BlockStateProperties.HORIZONTAL_FACING)) {
            state = state.setValue(BlockStateProperties.HORIZONTAL_FACING, dir);
        } else if (state.hasProperty(BlockStateProperties.FACING)) {
            state = state.setValue(BlockStateProperties.FACING, dir);
        }
        return state;
    }

    /** Checks a blank whose cell (0,0,0) is at origin; null when everything is in place. */
    @Nullable
    public Mismatch check(Level level, BlockPos origin, Direction structureFacing) {
        // bottom to top, front to back: the order a player builds in
        for (int y = 0; y < def.sy; y++) {
            for (int z = 0; z < def.sz; z++) {
                for (int x = 0; x < def.sx; x++) {
                    Entry e = entry(x, y, z);
                    if (e == null) {
                        continue;
                    }
                    BlockPos pos = MachineDef.worldPos(origin, structureFacing, x, y, z);
                    BlockState found = level.getBlockState(pos);
                    Direction need = MachineDef.rotate(structureFacing, e.facing() == null ? Direction.NORTH : e.facing());
                    if (!found.is(e.block())) {
                        return new Mismatch(pos, x, y, z, e, found, false, need);
                    }
                    if (e.facing() != null) {
                        Direction has = found.hasProperty(BlockStateProperties.HORIZONTAL_FACING)
                                ? found.getValue(BlockStateProperties.HORIZONTAL_FACING)
                                : found.hasProperty(BlockStateProperties.FACING) ? found.getValue(BlockStateProperties.FACING) : need;
                        if (has != need) {
                            return new Mismatch(pos, x, y, z, e, found, true, need);
                        }
                    }
                }
            }
        }
        return null;
    }
}
