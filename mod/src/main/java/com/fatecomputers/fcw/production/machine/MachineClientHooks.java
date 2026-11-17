package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.machine.console.MachineConsoleNet;
import net.minecraft.network.chat.Component;

import java.util.function.Consumer;

/**
 * Client-only work of the machines (particles, the console screen, the hint line), set by the client
 * entry point so that common code never loads client classes.
 */
public final class MachineClientHooks {
    private static Consumer<MachineControllerBlockEntity> ticker = be -> {
    };
    private static Consumer<MachineConsoleNet.Output> console = p -> {
    };
    private static Consumer<Component> notice = c -> {
    };

    private MachineClientHooks() {
    }

    public static void setTicker(Consumer<MachineControllerBlockEntity> t) {
        ticker = t;
    }

    public static void setConsole(Consumer<MachineConsoleNet.Output> c) {
        console = c;
    }

    public static void setNotice(Consumer<Component> n) {
        notice = n;
    }

    static void tick(MachineControllerBlockEntity be) {
        ticker.accept(be);
    }

    public static void consoleOutput(MachineConsoleNet.Output output) {
        console.accept(output);
    }

    public static void notice(Component text) {
        notice.accept(text);
    }
}
