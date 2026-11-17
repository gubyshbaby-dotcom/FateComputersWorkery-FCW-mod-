package com.fatecomputers.fcw.production.machine;

import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;

/**
 * The journal of a machine: the last {@value #CAPACITY} events, kept with the controller (so it survives
 * chunk reloads). An entry is a code and an argument, not text: the shell words it in the reader's
 * language. {@code seq} only grows, so scripts can ask for "everything after seq".
 */
public final class MachineLog {
    public static final int CAPACITY = 64;

    // event codes
    public static final String ON = "on";               // arg: who
    public static final String OFF = "off";             // arg: who
    public static final String STOP_AFTER = "stop_after"; // arg: who
    public static final String START = "start";         // arg: recipe
    public static final String DONE = "done";           // arg: recipe
    public static final String ABORT = "abort";         // arg: recipe
    public static final String FAULT = "fault";         // arg: reason (no_power, output_full, no_catalyst)
    public static final String WAIT = "wait";           // arg: reason (no_input)
    public static final String PIN = "pin";             // arg: recipe or "auto"
    public static final String RENAME = "rename";       // arg: new name

    /** @param time wall clock, epoch milliseconds */
    public record Entry(long seq, long time, String code, String arg) {
    }

    private final Deque<Entry> entries = new ArrayDeque<>();
    private long nextSeq = 1;

    public void add(String code, String arg) {
        entries.addLast(new Entry(nextSeq++, System.currentTimeMillis(), code, arg));
        while (entries.size() > CAPACITY) {
            entries.removeFirst();
        }
    }

    /** The last n entries, oldest first. */
    public List<Entry> last(int n) {
        List<Entry> all = new ArrayList<>(entries);
        return all.subList(Math.max(0, all.size() - Math.max(0, n)), all.size());
    }

    /** Entries with seq greater than the given one, oldest first. */
    public List<Entry> since(long seq) {
        List<Entry> out = new ArrayList<>();
        for (Entry e : entries) {
            if (e.seq > seq) {
                out.add(e);
            }
        }
        return out;
    }

    public CompoundTag save() {
        CompoundTag tag = new CompoundTag();
        tag.putLong("Next", nextSeq);
        ListTag list = new ListTag();
        for (Entry e : entries) {
            CompoundTag t = new CompoundTag();
            t.putLong("Seq", e.seq);
            t.putLong("Time", e.time);
            t.putString("Code", e.code);
            t.putString("Arg", e.arg);
            list.add(t);
        }
        tag.put("Entries", list);
        return tag;
    }

    public void load(CompoundTag tag) {
        entries.clear();
        nextSeq = Math.max(1, tag.getLong("Next"));
        ListTag list = tag.getList("Entries", Tag.TAG_COMPOUND);
        for (int i = 0; i < list.size(); i++) {
            CompoundTag t = list.getCompound(i);
            entries.addLast(new Entry(t.getLong("Seq"), t.getLong("Time"), t.getString("Code"), t.getString("Arg")));
        }
    }
}
