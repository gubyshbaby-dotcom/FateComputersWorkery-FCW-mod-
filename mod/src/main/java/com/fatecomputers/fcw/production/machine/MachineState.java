package com.fatecomputers.fcw.production.machine;

import net.minecraft.util.StringRepresentable;

/** Blockstate property "state" of every machine cell: breaker off, waiting, working; and the pallet of a site. */
public enum MachineState implements StringRepresentable {
    OFF("off"),
    IDLE("idle"),
    RUN("run"),
    /** only the terminal cell, while the machine is a construction site: it looks like a pallet */
    SITE("site");

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
