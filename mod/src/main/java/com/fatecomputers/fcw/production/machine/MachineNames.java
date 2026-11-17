package com.fatecomputers.fcw.production.machine;

import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.saveddata.SavedData;

import java.util.HashMap;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * Names of the machines of a world ("euv-1", "eaf-2"): unique across dimensions, so that
 * {@code fcw machine <name>} means one machine. Kept with the overworld's saved data; a name belongs to
 * the controller at one position of one dimension.
 */
public final class MachineNames extends SavedData {
    public static final Pattern VALID = Pattern.compile("[a-z][a-z0-9-]{0,23}");
    private static final String ID = "fcw_machine_names";

    /** name -> "dimension|packed position" */
    private final Map<String, String> owners = new HashMap<>();

    public static MachineNames get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(
                new SavedData.Factory<>(MachineNames::new, MachineNames::load, null), ID);
    }

    private static String owner(ServerLevel level, BlockPos pos) {
        return level.dimension().location() + "|" + pos.asLong();
    }

    private static MachineNames load(CompoundTag tag, HolderLookup.Provider registries) {
        MachineNames names = new MachineNames();
        CompoundTag map = tag.getCompound("Names");
        for (String key : map.getAllKeys()) {
            names.owners.put(key, map.getString(key));
        }
        return names;
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        CompoundTag map = new CompoundTag();
        owners.forEach(map::putString);
        tag.put("Names", map);
        return tag;
    }

    /** A fresh default name for the machine at this place: the first free {@code <prefix>-<n>}. */
    public String assign(ServerLevel level, BlockPos pos, String prefix) {
        String me = owner(level, pos);
        for (int n = 1; ; n++) {
            String name = prefix + "-" + n;
            if (!owners.containsKey(name)) {
                owners.put(name, me);
                setDirty();
                return name;
            }
        }
    }

    /**
     * A loaded machine says which name it carries. If another machine holds that name (a copied structure),
     * this one gets a fresh default name.
     */
    public String claim(ServerLevel level, BlockPos pos, String name, String prefix) {
        String me = owner(level, pos);
        String holder = owners.get(name);
        if (holder == null) {
            owners.put(name, me);
            setDirty();
            return name;
        }
        return holder.equals(me) ? name : assign(level, pos, prefix);
    }

    /** Renames; false when the new name is taken by another machine. */
    public boolean rename(ServerLevel level, BlockPos pos, String oldName, String newName) {
        String me = owner(level, pos);
        String holder = owners.get(newName);
        if (holder != null && !holder.equals(me)) {
            return false;
        }
        if (me.equals(owners.get(oldName))) {
            owners.remove(oldName);
        }
        owners.put(newName, me);
        setDirty();
        return true;
    }

    /** The machine is gone: its name is free again. */
    public void release(ServerLevel level, BlockPos pos, String name) {
        if (owner(level, pos).equals(owners.get(name))) {
            owners.remove(name);
            setDirty();
        }
    }
}
