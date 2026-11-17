package com.fatecomputers.fcw.server;

import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.saveddata.SavedData;

import java.util.LinkedHashSet;
import java.util.Set;

/**
 * Per-world bookkeeping of VM ids (overworld data storage, "fcw"):
 * the machines this world created (stopped when the server stops in remote mode, since a shared
 * daemon outlives the world), and machines whose block was broken while the daemon was unreachable
 * (deleted once it is back).
 */
public class FcwWorldData extends SavedData {
    private static final String NAME = "fcw";

    private final Set<String> machines = new LinkedHashSet<>();
    private final Set<String> pendingDeletes = new LinkedHashSet<>();

    public static FcwWorldData get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(
                new SavedData.Factory<>(FcwWorldData::new, FcwWorldData::load, null), NAME);
    }

    private static FcwWorldData load(CompoundTag tag, HolderLookup.Provider registries) {
        FcwWorldData d = new FcwWorldData();
        for (Tag t : tag.getList("Machines", Tag.TAG_STRING)) {
            d.machines.add(t.getAsString());
        }
        for (Tag t : tag.getList("PendingDeletes", Tag.TAG_STRING)) {
            d.pendingDeletes.add(t.getAsString());
        }
        return d;
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        ListTag m = new ListTag();
        machines.forEach(id -> m.add(StringTag.valueOf(id)));
        tag.put("Machines", m);
        ListTag p = new ListTag();
        pendingDeletes.forEach(id -> p.add(StringTag.valueOf(id)));
        tag.put("PendingDeletes", p);
        return tag;
    }

    public Set<String> machines() {
        return Set.copyOf(machines);
    }

    public void addMachine(String vmId) {
        if (machines.add(vmId)) {
            setDirty();
        }
    }

    public void removeMachine(String vmId) {
        if (machines.remove(vmId)) {
            setDirty();
        }
    }

    public Set<String> pendingDeletes() {
        return Set.copyOf(pendingDeletes);
    }

    public void addPendingDelete(String vmId) {
        if (pendingDeletes.add(vmId)) {
            setDirty();
        }
    }

    public void removePendingDelete(String vmId) {
        if (pendingDeletes.remove(vmId)) {
            setDirty();
        }
    }
}
