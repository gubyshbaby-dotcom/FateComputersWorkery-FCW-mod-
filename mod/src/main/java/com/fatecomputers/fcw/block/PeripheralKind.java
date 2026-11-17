package com.fatecomputers.fcw.block;

public enum PeripheralKind {
    MONITOR("monitor"),
    KEYBOARD("keyboard"),
    MOUSE("mouse"),
    SPEAKER("speaker"),
    /** Проектор: показывает экран, как монитор, но за компьютер с него не садятся. */
    PROJECTOR("projector");

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
