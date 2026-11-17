package com.fatecomputers.fcw.production.machine;

import net.minecraft.util.StringRepresentable;

/** Blockstate property "state" of every machine cell: breaker off, waiting, working. */
public enum MachineState implements StringRepresentable {
    OFF("off"),
    IDLE("idle"),
    RUN("run");

    private final String name;

    MachineState(String name) {
        this.name = name;
    }

    @Override
    public String getSerializedName() {
        return name;
    }

    public static MachineState byName(String name) {
        for (MachineState s : values()) {
            if (s.name.equals(name)) {
                return s;
            }
        }
        return OFF;
    }
}
