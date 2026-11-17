package com.fatecomputers.fcw.production.quality;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineLog;
import com.fatecomputers.fcw.production.machine.MachineRecipeNames;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.quality.Quality.Grade;
import com.fatecomputers.fcw.production.quality.Quality.YieldClass;
import com.fatecomputers.fcw.production.recipe.MachineRecipe;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.TagKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;

import javax.annotation.Nullable;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Defects and yield of one machine (docs/design/DEFECTS.md, the base model of the levels engineer and
 * fab). It belongs to the machine's controller, which tells it when a recipe starts, advances, stalls
 * on power and finishes.
 * <ul>
 * <li>Crystal puller and wafering cell: the product gets a class A/B/C from two conditions: water in the
 *     cooling (or rinse) port for the whole run, and no pause for lack of power.</li>
 * <li>Scanner: the 61 fields of a wafer are decided one by one as the exposure passes them, each with
 *     the causes E, P, W, C, T, M, V; the wafer leaves with a yield class S…D. The maps of the last 64
 *     wafers stay here for the console, the programs and the report.</li>
 * <li>Chip packaging: the output of a recipe is multiplied by the yield class of the wafer it took.</li>
 * <li>APC: computers that see the machine over the data network give it computing units; the share it
 *     gets softens some causes and speeds up other machines.</li>
 * </ul>
 */
public final class MachineQuality {
    /** Wafers whose maps the scanner remembers. */
    public static final int HISTORY = 64;
    private static final TagKey<Block> FILTERS = TagKey.create(Registries.BLOCK, ResourceLocation.fromNamespaceAndPath("fcw", "cleanroom_filters"));
    private static final int ROOM_LIMIT = 8192;
    /** A computer's APC report counts for three seconds. */
    private static final int APC_TTL = 60;

    /** One finished wafer. */
    public record Wafer(long time, String recipe, String map, int iso) {
        public int good() {
            return WaferMap.good(map);
        }

        public YieldClass yieldClass() {
            return YieldClass.ofGood(good());
        }
    }

    /** What a computer gives this machine: reported every second by the computer's bridge. */
    public record ApcReport(String host, double cu, double max, boolean service, long tick) {
    }

    /** Totals of one in-game day, for "fcw yield". */
    public static final class Shift {
        public long day;
        public int wafers, goodFields, products;
        public final int[] causes = new int[WaferMap.CAUSES.length()];
        public final int[] classes = new int[YieldClass.values().length];
        /** Ticks stood still: no power, output full, no mask. */
        public final int[] downtime = new int[3];

        CompoundTag save() {
            CompoundTag t = new CompoundTag();
            t.putLong("Day", day);
            t.putIntArray("N", new int[] {wafers, goodFields, products});
            t.putIntArray("Causes", causes);
            t.putIntArray("Classes", classes);
            t.putIntArray("Down", downtime);
            return t;
        }

        void load(CompoundTag t) {
            day = t.getLong("Day");
            int[] n = t.getIntArray("N");
            if (n.length == 3) {
                wafers = n[0];
                goodFields = n[1];
                products = n[2];
            }
            copy(t.getIntArray("Causes"), causes);
            copy(t.getIntArray("Classes"), classes);
            copy(t.getIntArray("Down"), downtime);
        }

        private static void copy(int[] from, int[] to) {
            System.arraycopy(from, 0, to, 0, Math.min(from.length, to.length));
        }

        public JsonObject json() {
            JsonObject o = new JsonObject();
            o.addProperty("day", day);
            o.addProperty("wafers", wafers);
            o.addProperty("good_fields", goodFields);
            o.addProperty("products", products);
            JsonObject c = new JsonObject();
            for (int i = 0; i < causes.length; i++) {
                c.addProperty(String.valueOf(WaferMap.CAUSES.charAt(i)), causes[i]);
            }
            o.add("causes", c);
            JsonObject k = new JsonObject();
            for (YieldClass y : YieldClass.values()) {
                k.addProperty(y.name(), classes[y.ordinal()]);
            }
            o.add("classes", k);
            JsonObject d = new JsonObject();
            d.addProperty("no_power", downtime[0]);
            d.addProperty("output_full", downtime[1]);
            d.addProperty("no_catalyst", downtime[2]);
            o.add("downtime_ticks", d);
            return o;
        }
    }

    private final MachineControllerBlockEntity machine;
    private final boolean scanner, puller, saw, line;
    // per section: pauses for lack of power and "the water ran out" during the current recipe
    private final int[] pauses;
    private final boolean[] ranDry;
    // the wafer being exposed
    private StringBuilder map = new StringBuilder();
    private int brownoutFields;
    private int brownouts;
    private Grade waferCrystal = Grade.A, waferPolish = Grade.A;
    private double maskWear;
    // captured before the inputs are consumed
    private Grade inputCrystal = Grade.A;
    private double inputMultiplier = 1.0;
    private final Deque<Wafer> history = new ArrayDeque<>();
    private int lot;
    // surroundings of the scanner
    private int iso; // 0 = open to the sky or too large, else 8, 7, 6, 5
    private boolean coolant;
    private boolean vibration;
    private final Map<Long, ApcReport> apc = new HashMap<>();
    private final Shift today = new Shift(), yesterday = new Shift();

    public MachineQuality(MachineControllerBlockEntity machine) {
        this.machine = machine;
        String name = machine.def().name;
        scanner = name.equals(Machines.EUV);
        puller = name.equals("crystal_puller");
        saw = name.equals("wafering_cell");
        line = name.equals("chip_line");
        pauses = new int[machine.sectionCount()];
        ranDry = new boolean[machine.sectionCount()];
    }

    public boolean isScanner() {
        return scanner;
    }

    // ───────────────────────── items

    private static boolean is(ItemStack stack, String path) {
        return !stack.isEmpty() && BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath().equals(path);
    }

    @Nullable
    private static ItemStack first(List<ItemStack> stacks, String path) {
        for (ItemStack s : stacks) {
            if (is(s, path)) {
                return s;
            }
        }
        return null;
    }

    // ───────────────────────── water

    /** The port whose water the machine needs while it works; null when it needs none. */
    @Nullable
    private String waterPort() {
        return saw ? "water" : scanner || puller ? "coolant_in" : null;
    }

    /** Takes 1 mB of water from the port for this tick of work; false when there is none. */
    private boolean drinkWater() {
        String port = waterPort();
        IFluidHandler tank = port == null ? null : machine.tank(port);
        if (tank == null || !tank.getFluidInTank(0).is(Fluids.WATER) || tank.getFluidInTank(0).isEmpty()) {
            return false;
        }
        tank.drain(1, IFluidHandler.FluidAction.EXECUTE);
        return true;
    }

    public boolean hasWater() {
        String port = waterPort();
        IFluidHandler tank = port == null ? null : machine.tank(port);
        return tank != null && tank.getFluidInTank(0).is(Fluids.WATER) && !tank.getFluidInTank(0).isEmpty();
    }

    // ───────────────────────── APC

    /** A computer reports what it gives this machine (once a second). */
    public void apcReport(long computer, String host, double cu, double max, boolean service, long tick) {
        ApcReport old = apc.put(computer, new ApcReport(host, cu, max, service, tick));
        if (old != null && old.service() != service) {
            machine.log().add(MachineLog.APC, host + " " + fmt(old.cu()) + " -> " + fmt(cu) + " CU");
        }
    }

    private static String fmt(double v) {
        return v == Math.rint(v) ? String.valueOf((long) v) : String.format(java.util.Locale.ROOT, "%.1f", v);
    }

    public List<ApcReport> apcReports() {
        long now = machine.getLevel() == null ? 0 : machine.getLevel().getGameTime();
        apc.values().removeIf(r -> now - r.tick() > APC_TTL || now < r.tick());
        return new ArrayList<>(apc.values());
    }

    /** CU the machine needs for the full effect; 0 when APC does nothing for it. */
    public double apcNeed() {
        if (scanner) {
            ResourceLocation id = machine.section(0).recipeId();
            String recipe = id == null ? "" : MachineRecipeNames.shortName(id);
            return recipe.equals("wafer_controller") ? 2 : recipe.equals("wafer_memory") ? 4 : 8;
        }
        return puller ? 2 : saw ? 1 : line ? 4 : 0;
    }

    /** Share of the needed CU the machine gets, 0..1. */
    public double apcCoverage() {
        double need = apcNeed();
        if (need <= 0 || !Quality.enabled()) {
            return 0;
        }
        double got = 0;
        for (ApcReport r : apcReports()) {
            got += r.cu();
        }
        return Math.min(1.0, got / need);
    }

    // ───────────────────────── hooks of the controller

    /**
     * A recipe starts. Returns the factor for its time: APC shortens the growth of a crystal and the
     * takt of the line by up to 20 %.
     */
    public double onStart(int section, MachineRecipe recipe, List<ItemStack> inputs, ItemStack catalyst) {
        pauses[section] = 0;
        ranDry[section] = false;
        if (!Quality.enabled()) {
            return 1.0;
        }
        if (scanner) {
            map = new StringBuilder();
            brownoutFields = 0;
            brownouts = 0;
            ItemStack wafer = first(inputs, "silicon_wafer");
            waferCrystal = wafer == null ? Grade.A : Quality.crystal(wafer);
            waferPolish = wafer == null ? Grade.A : Quality.polish(wafer);
            maskWear = catalyst.isDamageableItem() ? (double) catalyst.getDamageValue() / catalyst.getMaxDamage() : 0;
        }
        return puller || line ? 1.0 - 0.2 * apcCoverage() : 1.0;
    }

    /** The recipe stopped for lack of power (called once per stop). */
    public void onPowerPause(int section) {
        pauses[section]++;
        if (scanner && Quality.enabled()) {
            // the stage loses its reference: the field in work and the two after it are lost
            brownoutFields = 3;
            brownouts++;
            machine.log().add(MachineLog.BROWNOUT, "");
        }
    }

    /** One tick of work done; progress is counted in ticks of the recipe. */
    public void onProgress(int section, int progress, int total) {
        if (!Quality.enabled()) {
            return;
        }
        boolean water = waterPort() == null || drinkWater();
        if (!water) {
            ranDry[section] = true;
        }
        if (scanner) {
            coolant = water;
            int due = (int) Math.min(WaferMap.FIELDS, (long) WaferMap.FIELDS * progress / Math.max(1, total));
            while (map.length() < due) {
                map.append(decide(map.length(), machine.getLevel().random));
            }
        }
    }

    /** The fate of one field at the moment it is exposed. */
    private char decide(int field, RandomSource random) {
        if (brownoutFields > 0) {
            brownoutFields--;
            return 'E';
        }
        double c = apcCoverage();
        double soft = 1.0 - 0.6 * c;   // M, T, V with full APC: x0.4
        double softW = 1.0 - 0.3 * c;  // W with full APC: x0.7
        double p = switch (iso) {
            case 8 -> 0.09;
            case 7 -> 0.05;
            case 6 -> 0.025;
            case 5 -> 0.01;
            default -> 0.16;
        };
        if (random.nextDouble() < p) {
            return 'P';
        }
        double w = (waferPolish == Grade.C ? 0.08 : waferPolish == Grade.B ? 0.04 : 0.01) * (WaferMap.edge(field) ? 1.6 : 0.6) * softW;
        if (random.nextDouble() < w) {
            return 'W';
        }
        double cr = waferCrystal == Grade.C ? 0.06 : waferCrystal == Grade.B ? 0.03 : 0.005;
        if (field > 0 && map.charAt(field - 1) == 'C' && WaferMap.row(field - 1) == WaferMap.row(field)) {
            cr *= 3; // crystal defects run in lines
        }
        if (random.nextDouble() < cr) {
            return 'C';
        }
        // heat: nothing with water; without it 7 % on average, denser towards the end of the wafer
        double t = coolant ? 0 : 0.07 * (0.5 + (double) field / (WaferMap.FIELDS - 1)) * soft;
        if (random.nextDouble() < t) {
            return 'T';
        }
        double m = maskWear <= 0.5 ? 0 : (maskWear - 0.5) / 0.5 * 0.12 * soft;
        if (random.nextDouble() < m) {
            return 'M';
        }
        if (vibration && random.nextDouble() < 0.05 * soft) {
            return 'V';
        }
        return '.';
    }

    /** Just before the inputs are consumed: what quality goes into the product. */
    public void beforeFinish(int section, MachineRecipe recipe, List<ItemStack> inputs) {
        inputCrystal = Grade.A;
        inputMultiplier = 1.0;
        if (!Quality.enabled()) {
            return;
        }
        if (saw) {
            ItemStack boule = first(inputs, "silicon_boule");
            inputCrystal = boule == null ? Grade.A : Quality.crystal(boule);
        } else if (line && section == 0) {
            for (ItemStack s : inputs) {
                if (!s.isEmpty() && BuiltInRegistries.ITEM.getKey(s.getItem()).getPath().startsWith("wafer_")) {
                    inputMultiplier = Quality.yieldClass(s).multiplier;
                    break;
                }
            }
        }
    }

    /** The results of a finished recipe get their classes; the packaging output is scaled by the wafer's class. */
    public List<ItemStack> stamp(int section, MachineRecipe recipe, List<ItemStack> results) {
        ServerLevel level = (ServerLevel) machine.getLevel();
        roll(level);
        today.products++;
        if (!Quality.enabled()) {
            return results;
        }
        Grade grade = Grade.of(!ranDry[section], pauses[section] == 0);
        List<ItemStack> out = new ArrayList<>();
        for (ItemStack r : results) {
            ItemStack s = r.copy();
            if (puller && is(s, "silicon_boule")) {
                Quality.setCrystal(s, grade);
            } else if (saw && is(s, "silicon_wafer")) {
                Quality.setCrystal(s, inputCrystal);
                Quality.setPolish(s, grade);
                // APC: the ninth wafer from a boule
                if (level.random.nextDouble() < apcCoverage()) {
                    s.grow(1);
                }
            } else if (scanner) {
                while (map.length() < WaferMap.FIELDS) {
                    map.append(decide(map.length(), level.random));
                }
                Wafer wafer = new Wafer(System.currentTimeMillis(), BuiltInRegistries.ITEM.getKey(s.getItem()).getPath(), map.toString(), iso);
                history.addLast(wafer);
                while (history.size() > HISTORY) {
                    history.removeFirst();
                }
                lot++;
                Quality.setYieldClass(s, wafer.yieldClass());
                today.wafers++;
                today.goodFields += wafer.good();
                today.classes[wafer.yieldClass().ordinal()]++;
                int[] causes = WaferMap.causes(wafer.map());
                for (int i = 0; i < causes.length; i++) {
                    today.causes[i] += causes[i];
                }
                machine.log().add(MachineLog.WAFER, wafer.yieldClass().name() + " " + wafer.good() + "/" + WaferMap.FIELDS);
                map = new StringBuilder();
            } else if (line && section == 0 && inputMultiplier != 1.0) {
                double exact = s.getCount() * inputMultiplier;
                int n = (int) Math.floor(exact);
                if (level.random.nextDouble() < exact - n) {
                    n++;
                }
                if (n <= 0) {
                    continue;
                }
                s.setCount(n);
            }
            out.add(s);
        }
        return out;
    }

    public void onAbort(int section) {
        if (scanner) {
            map = new StringBuilder();
            brownoutFields = 0;
        }
    }

    // ───────────────────────── surroundings and statistics

    /** Every server tick of the controller. */
    public void tick(long now) {
        if (!(machine.getLevel() instanceof ServerLevel level)) {
            return;
        }
        MachineControllerBlockEntity.Status fault = machine.faultStatus();
        if (fault != null) {
            roll(level);
            today.downtime[fault == MachineControllerBlockEntity.Status.NO_POWER ? 0
                    : fault == MachineControllerBlockEntity.Status.OUTPUT_FULL ? 1 : 2]++;
        }
        if (!scanner || !Quality.enabled()) {
            return;
        }
        long phase = now + machine.getBlockPos().asLong();
        if (phase % 100 == 0) {
            int was = iso;
            iso = cleanliness(level);
            if (was != iso && now > 200) {
                machine.log().add(MachineLog.ROOM, isoName(was) + " -> " + isoName(iso));
            }
        }
        if (phase % 20 == 0) {
            vibration = vibrationNear(level);
        }
    }

    private void roll(ServerLevel level) {
        long day = level.getDayTime() / 24000L;
        if (day != today.day) {
            yesterday.load(today.save());
            today.load(new Shift().save());
            today.day = day;
        }
    }

    public static String isoName(int iso) {
        return iso == 0 ? "open" : "ISO " + iso;
    }

    private AABB bounds(MachineControllerBlockEntity m) {
        MachineDef def = m.def();
        BlockPos a = m.origin();
        BlockPos b = MachineDef.worldPos(a, m.facing(), def.sx - 1, def.sy - 1, def.sz - 1);
        return AABB.encapsulatingFullBlocks(a, b);
    }

    private static double gap(AABB a, AABB b) {
        double dx = Math.max(0, Math.max(a.minX - b.maxX, b.minX - a.maxX));
        double dy = Math.max(0, Math.max(a.minY - b.maxY, b.minY - a.maxY));
        double dz = Math.max(0, Math.max(a.minZ - b.maxZ, b.minZ - a.maxZ));
        return Math.sqrt(dx * dx + dy * dy + dz * dz);
    }

    /** A heavy machine is working nearby: arc furnace within 16 blocks, wafering cell or an unloading puller within 8. */
    private boolean vibrationNear(ServerLevel level) {
        AABB me = bounds(machine);
        int cx = machine.getBlockPos().getX() >> 4, cz = machine.getBlockPos().getZ() >> 4;
        for (int x = cx - 2; x <= cx + 2; x++) {
            for (int z = cz - 2; z <= cz + 2; z++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(x, z);
                if (chunk == null) {
                    continue;
                }
                for (BlockEntity be : chunk.getBlockEntities().values()) {
                    if (!(be instanceof MachineControllerBlockEntity other) || other == machine || !other.clientWorking()) {
                        continue;
                    }
                    String kind = other.def().name;
                    double radius = kind.equals("arc_furnace") ? 16 : kind.equals("wafering_cell") ? 8
                            : kind.equals("crystal_puller") && "unload".equals(other.workPhase()) ? 8 : 0;
                    if (radius > 0 && gap(me, bounds(other)) <= radius) {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    /**
     * Cleanliness of the room the scanner stands in: a flood fill of the air around its cells. Reaching (a glass roof is a roof)
     * the sky or more than 8192 cells means "open"; a closed room is ISO 8, and filter blocks (block tag
     * fcw:cleanroom_filters; the FFU of the artist's archive goes there) at its walls and ceiling raise
     * it: one per 64 cells of air ISO 7, per 32 ISO 6, per 16 ISO 5. Doors are not looked at yet.
     */
    private int cleanliness(ServerLevel level) {
        MachineDef def = machine.def();
        BlockPos origin = machine.origin();
        Direction facing = machine.facing();
        Set<Long> own = new HashSet<>();
        for (MachineDef.Cell c : def.cells) {
            own.add(MachineDef.worldPos(origin, facing, c).asLong());
        }
        Set<Long> seen = new HashSet<>();
        Set<Long> filters = new HashSet<>();
        ArrayDeque<BlockPos> queue = new ArrayDeque<>();
        for (MachineDef.Cell c : def.cells) {
            BlockPos at = MachineDef.worldPos(origin, facing, c);
            for (Direction d : Direction.values()) {
                BlockPos n = at.relative(d);
                if (!own.contains(n.asLong()) && level.isLoaded(n) && level.getBlockState(n).isAir() && seen.add(n.asLong())) {
                    queue.add(n);
                }
            }
        }
        while (!queue.isEmpty()) {
            BlockPos p = queue.poll();
            if (seen.size() > ROOM_LIMIT || p.getY() >= level.getHeight(net.minecraft.world.level.levelgen.Heightmap.Types.MOTION_BLOCKING, p.getX(), p.getZ())) {
                return 0;
            }
            for (Direction d : Direction.values()) {
                BlockPos n = p.relative(d);
                if (own.contains(n.asLong()) || !level.isLoaded(n)) {
                    continue;
                }
                BlockState state = level.getBlockState(n);
                if (state.isAir()) {
                    if (seen.add(n.asLong())) {
                        queue.add(n);
                    }
                } else if (state.is(FILTERS)) {
                    filters.add(n.asLong());
                }
            }
        }
        int air = Math.max(1, seen.size()), f = filters.size();
        return f > 0 && f * 16 >= air ? 5 : f > 0 && f * 32 >= air ? 6 : f > 0 && f * 64 >= air ? 7 : 8;
    }

    // ───────────────────────── what the console and the programs see

    public int iso() {
        return iso;
    }

    public boolean vibration() {
        return vibration;
    }

    public List<Wafer> history() {
        return new ArrayList<>(history);
    }

    @Nullable
    public Wafer lastWafer() {
        return history.peekLast();
    }

    /** Fields of the wafer in work decided so far ('.' good, a letter the cause). */
    public String currentMap() {
        return map.toString();
    }

    public Shift today() {
        if (machine.getLevel() instanceof ServerLevel level) {
            roll(level);
        }
        return today;
    }

    public Shift yesterday() {
        return yesterday;
    }

    public int pauses(int section) {
        return pauses[section];
    }

    public boolean ranDry(int section) {
        return ranDry[section];
    }

    /** "P 3, W 2" for a map; empty when every field is good. */
    public static String causesText(String map) {
        int[] n = WaferMap.causes(map);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < n.length; i++) {
            if (n[i] > 0) {
                sb.append(sb.isEmpty() ? "" : ", ").append(WaferMap.CAUSES.charAt(i)).append(' ').append(n[i]);
            }
        }
        return sb.toString();
    }

    private static JsonObject causesJson(String map) {
        int[] n = WaferMap.causes(map);
        JsonObject o = new JsonObject();
        for (int i = 0; i < n.length; i++) {
            if (n[i] > 0) {
                o.addProperty(String.valueOf(WaferMap.CAUSES.charAt(i)), n[i]);
            }
        }
        return o;
    }

    public JsonObject apcJson() {
        JsonObject o = new JsonObject();
        double got = 0, max = 0;
        JsonArray hosts = new JsonArray();
        for (ApcReport r : apcReports()) {
            got += r.cu();
            max += r.max();
            JsonObject h = new JsonObject();
            h.addProperty("host", r.host());
            h.addProperty("cu", r.cu());
            h.addProperty("max", r.max());
            h.addProperty("service", r.service());
            hosts.add(h);
        }
        o.addProperty("cu", got);
        o.addProperty("max", max);
        o.addProperty("need", apcNeed());
        o.addProperty("coverage", apcCoverage());
        o.add("hosts", hosts);
        return o;
    }

    /** The "litho" block of a machine frame (docs/design/CONSOLE_APPS.md). */
    public JsonObject lithoJson() {
        JsonObject o = new JsonObject();
        o.addProperty("lot", lot);
        o.addProperty("field", map.length());
        o.addProperty("map", map.toString());
        ItemStack mask = machine.items().getStackInSlot(Math.max(0, machine.catalystSlot(0)));
        if (!mask.isEmpty()) {
            JsonObject m = new JsonObject();
            m.addProperty("id", BuiltInRegistries.ITEM.getKey(mask.getItem()).toString());
            m.addProperty("used", mask.getDamageValue());
            m.addProperty("max", mask.getMaxDamage());
            o.add("mask", m);
        }
        JsonObject env = new JsonObject();
        env.addProperty("iso", iso);
        env.addProperty("sealed", iso != 0);
        env.addProperty("coolant", hasWater());
        env.addProperty("vibration", vibration);
        env.addProperty("brownouts", brownouts);
        o.add("env", env);
        JsonArray wafers = new JsonArray();
        List<Wafer> all = history();
        for (Wafer w : all.subList(Math.max(0, all.size() - 16), all.size())) {
            JsonObject j = new JsonObject();
            j.addProperty("recipe", w.recipe());
            j.addProperty("good", w.good());
            j.addProperty("class", w.yieldClass().name());
            j.addProperty("map", w.map());
            j.addProperty("time", w.time());
            j.add("causes", causesJson(w.map()));
            wafers.add(j);
        }
        o.add("lot_wafers", wafers);
        return o;
    }

    // ───────────────────────── persistence

    public CompoundTag save() {
        CompoundTag tag = new CompoundTag();
        tag.putIntArray("Pauses", pauses);
        byte[] dry = new byte[ranDry.length];
        for (int i = 0; i < dry.length; i++) {
            dry[i] = (byte) (ranDry[i] ? 1 : 0);
        }
        tag.putByteArray("Dry", dry);
        tag.put("Today", today.save());
        tag.put("Yesterday", yesterday.save());
        if (scanner) {
            tag.putString("Map", map.toString());
            tag.putInt("BrownoutFields", brownoutFields);
            tag.putInt("Brownouts", brownouts);
            tag.putString("WaferCrystal", waferCrystal.id());
            tag.putString("WaferPolish", waferPolish.id());
            tag.putDouble("MaskWear", maskWear);
            tag.putInt("Lot", lot);
            tag.putInt("Iso", iso);
            ListTag list = new ListTag();
            for (Wafer w : history) {
                CompoundTag t = new CompoundTag();
                t.putLong("T", w.time());
                t.putString("R", w.recipe());
                t.putString("M", w.map());
                t.putByte("I", (byte) w.iso());
                list.add(t);
            }
            tag.put("History", list);
        }
        return tag;
    }

    public void load(CompoundTag tag) {
        int[] p = tag.getIntArray("Pauses");
        System.arraycopy(p, 0, pauses, 0, Math.min(p.length, pauses.length));
        byte[] dry = tag.getByteArray("Dry");
        for (int i = 0; i < Math.min(dry.length, ranDry.length); i++) {
            ranDry[i] = dry[i] != 0;
        }
        today.load(tag.getCompound("Today"));
        yesterday.load(tag.getCompound("Yesterday"));
        if (scanner) {
            map = new StringBuilder(tag.getString("Map"));
            brownoutFields = tag.getInt("BrownoutFields");
            brownouts = tag.getInt("Brownouts");
            waferCrystal = Grade.parse(tag.getString("WaferCrystal"));
            waferPolish = Grade.parse(tag.getString("WaferPolish"));
            maskWear = tag.getDouble("MaskWear");
            lot = tag.getInt("Lot");
            iso = tag.getInt("Iso");
            history.clear();
            ListTag list = tag.getList("History", Tag.TAG_COMPOUND);
            for (int i = 0; i < list.size(); i++) {
                CompoundTag t = list.getCompound(i);
                history.addLast(new Wafer(t.getLong("T"), t.getString("R"), t.getString("M"), t.getByte("I")));
            }
        }
    }
}
