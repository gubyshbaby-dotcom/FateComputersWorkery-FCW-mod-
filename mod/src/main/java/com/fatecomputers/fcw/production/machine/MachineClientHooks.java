package com.fatecomputers.fcw.production.machine;

import java.util.function.Consumer;

/** Client-only work of the controller (particles), set by the client entry point so that common code never loads client classes. */
public final class MachineClientHooks {
    private static Consumer<MachineControllerBlockEntity> ticker = be -> {
    };

    private MachineClientHooks() {
    }

    public static void setTicker(Consumer<MachineControllerBlockEntity> t) {
        ticker = t;
    }

    static void tick(MachineControllerBlockEntity be) {
        ticker.accept(be);
    }
}
