package com.fatecomputers.fcw.block;

public enum PeripheralKind {
    MONITOR("monitor"),
    KEYBOARD("keyboard"),
    MOUSE("mouse"),
    SPEAKER("speaker");

    private final String id;

    PeripheralKind(String id) {
        this.id = id;
    }

    public String id() {
        return id;
    }

    public static PeripheralKind byId(String id) {
        for (PeripheralKind k : values()) {
            if (k.id.equals(id)) {
                return k;
            }
        }
        return null;
    }
}
