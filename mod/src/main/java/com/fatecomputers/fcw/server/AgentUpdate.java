package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.util.NativeFiles;
import com.google.gson.JsonObject;

import javax.annotation.Nullable;
import java.io.IOException;
import java.io.InputStream;
import java.util.Base64;
import java.util.function.Consumer;

/**
 * Самообновление агента гостя (docs/CONTRACT.md §7). Мод носит свежий бинарник {@code fcw} в своём
 * jar ({@code /natives/guest-x86_64/fcw}); если агент в {@code hello} сообщил другой SHA-256 и умеет
 * обновляться, бинарник уходит ему кусками base64 — по одному, следующий только после подтверждения
 * предыдущего. Агент сам проверяет хэш, подменяет файл и перезапускается.
 *
 * <p>Один экземпляр на компьютер, только главный поток сервера. Сообщения гостю уходят через
 * переданный {@code send}, поэтому класс проверяется без сервера и демона.
 */
public final class AgentUpdate {
    /** Путь бинарника агента в jar мода. Гость всегда x86_64. */
    public static final String RESOURCE = "/natives/guest-x86_64/fcw";
    /** Кусок до кодирования: в base64 это 60 000 знаков, строка JSON остаётся меньше 64 КиБ. */
    public static final int CHUNK_BYTES = 45_000;
    /** Сколько тиков ждать ответа агента, прежде чем бросить передачу. */
    public static final int TIMEOUT_TICKS = 20 * 20;

    /** Бинарник агента из jar: содержимое и его SHA-256. */
    public record Binary(byte[] data, String sha256) {
        public int chunks() {
            return (data.length + CHUNK_BYTES - 1) / CHUNK_BYTES;
        }
    }

    @Nullable
    private static Binary bundled;
    private static boolean bundledLoaded;

    /** Агент из jar мода; null, если jar собран без него (обновления тогда нет). */
    @Nullable
    public static synchronized Binary bundled() {
        if (!bundledLoaded) {
            bundledLoaded = true;
            try (InputStream in = AgentUpdate.class.getResourceAsStream(RESOURCE)) {
                if (in != null) {
                    byte[] data = in.readAllBytes();
                    if (data.length > 0) {
                        bundled = new Binary(data, NativeFiles.sha256(data));
                        FcwMod.LOGGER.info("Guest agent bundled with the mod: {} bytes, sha256 {}", data.length, bundled.sha256());
                    }
                }
            } catch (IOException e) {
                FcwMod.LOGGER.warn("Could not read the bundled guest agent: {}", e.toString());
            }
        }
        return bundled;
    }

    @Nullable
    private final Binary binary;
    private final String label;
    private boolean active;
    private int next;       // сколько кусков агент уже подтвердил
    private int idleTicks;
    /** Хэш, который уже пробовали поставить за этот запуск ВМ: второй раз не шлём, чтобы не зациклиться. */
    @Nullable
    private String attempted;

    public AgentUpdate(@Nullable Binary binary, String label) {
        this.binary = binary;
        this.label = label;
    }

    public boolean active() {
        return active;
    }

    /** ВМ остановлена: следующая загрузка начинает с чистого листа. */
    public void reset() {
        active = false;
        attempted = null;
    }

    /** Агент поздоровался: если его бинарник не тот, что в моде, начать передачу. */
    public void onHello(JsonObject hello, Consumer<JsonObject> send) {
        if (binary == null || active) {
            return;
        }
        String theirs = hello.has("sha256") && hello.get("sha256").isJsonPrimitive() ? hello.get("sha256").getAsString() : "";
        boolean canUpdate = hello.has("update") && hello.get("update").isJsonPrimitive() && hello.get("update").getAsInt() >= 1;
        if (!canUpdate || theirs.isEmpty()) {
            return; // агент из старого образа: обновляться не умеет
        }
        if (theirs.equalsIgnoreCase(binary.sha256())) {
            if (attempted != null) {
                FcwMod.LOGGER.info("Guest agent of {} is now up to date ({})", label, theirs);
                attempted = null;
            }
            return;
        }
        if (binary.sha256().equals(attempted)) {
            FcwMod.LOGGER.warn("Guest agent of {} still reports {} after an update, not trying again", label, theirs);
            return;
        }
        attempted = binary.sha256();
        active = true;
        next = 0;
        idleTicks = 0;
        FcwMod.LOGGER.info("Updating the guest agent of {}: {} -> {} ({} bytes)", label, theirs, binary.sha256(), binary.data().length);
        JsonObject o = new JsonObject();
        o.addProperty("type", "update_begin");
        o.addProperty("version", FcwMod.version());
        o.addProperty("size", binary.data().length);
        o.addProperty("sha256", binary.sha256());
        o.addProperty("chunks", binary.chunks());
        send.accept(o);
    }

    /** {@code update_ack}: агент принял {@code seq} кусков, шлём следующий или завершаем. */
    public void onAck(JsonObject ack, Consumer<JsonObject> send) {
        if (!active || binary == null || !ack.has("seq") || ack.get("seq").getAsInt() != next) {
            return;
        }
        idleTicks = 0;
        JsonObject o = new JsonObject();
        if (next >= binary.chunks()) {
            o.addProperty("type", "update_end");
        } else {
            int from = next * CHUNK_BYTES;
            int len = Math.min(CHUNK_BYTES, binary.data().length - from);
            byte[] part = new byte[len];
            System.arraycopy(binary.data(), from, part, 0, len);
            o.addProperty("type", "update_chunk");
            o.addProperty("seq", next);
            o.addProperty("data", Base64.getEncoder().encodeToString(part));
            next++;
        }
        send.accept(o);
    }

    /** {@code update_result}: агент поставил бинарник (и сейчас перезапустится) или отказался. */
    public void onResult(JsonObject result) {
        if (!active) {
            return;
        }
        active = false;
        boolean ok = result.has("ok") && result.get("ok").isJsonPrimitive() && result.get("ok").getAsBoolean();
        if (ok) {
            FcwMod.LOGGER.info("Guest agent of {} accepted the update and restarts", label);
        } else {
            FcwMod.LOGGER.warn("Guest agent of {} refused the update: {}", label,
                    result.has("error") ? result.get("error").getAsString() : "unknown error");
        }
    }

    /** Раз в тик: молчащий агент не держит передачу вечно. */
    public void tick() {
        if (active && ++idleTicks > TIMEOUT_TICKS) {
            active = false;
            FcwMod.LOGGER.warn("Guest agent of {} stopped answering during an update", label);
        }
    }
}
