package com.fatecomputers.fcw.production.machine.shell;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.google.gson.JsonObject;
import org.jetbrains.annotations.Nullable;

/**
 * Командная оболочка машины: одна и та же для консоли пульта и для {@code fcw machine} с компьютера.
 * Контракт: docs/MACHINE_CONTROL.md. Владелец реализации — работа «машины в мире»; здесь заглушка,
 * чтобы мост компьютера компилировался раньше.
 */
public final class MachineShell {
    private MachineShell() {
    }

    /**
     * @param exit 0 — успех, 1 — отказ машины (нет питания, нет рецепта…), 2 — ошибка в команде
     * @param text вывод для человека, на языке {@code lang}
     * @param json вывод для скриптов (только при {@code json = true})
     */
    public record Result(int exit, String text, @Nullable JsonObject json) {
    }

    /**
     * Выполнить одну строку команды на машине (сервер, главный поток).
     *
     * @param line  строка без имени машины: {@code "status"}, {@code "recipe auto"}, {@code "log 5"}…
     * @param lang  код языка клиента ({@code ru_ru}, {@code en_us}); команды всегда английские
     * @param json  машиночитаемый вывод
     * @param actor кто выполняет — для журнала машины («оператор Steve», «euv-ctl.mc»)
     */
    public static Result exec(MachineControllerBlockEntity machine, String line, String lang, boolean json, String actor) {
        return new Result(2, "not implemented", null);
    }

    /** Имя машины для {@code fcw machine <имя>}: уникально в мире, по умолчанию «euv-1», «eaf-2»… */
    public static String name(MachineControllerBlockEntity machine) {
        return machine.def().name + "@" + machine.getBlockPos().toShortString().replace(", ", ",");
    }
}
