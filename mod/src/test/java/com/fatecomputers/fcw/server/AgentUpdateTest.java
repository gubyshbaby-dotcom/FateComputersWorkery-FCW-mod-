package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.util.NativeFiles;
import com.google.gson.JsonObject;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.Base64;
import java.util.Deque;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Протокол самообновления агента со стороны мода: против поддельного агента, без сервера и демона. */
class AgentUpdateTest {
    private static AgentUpdate.Binary binary(int size) {
        byte[] data = new byte[size];
        new Random(7).nextBytes(data);
        return new AgentUpdate.Binary(data, NativeFiles.sha256(data));
    }

    private static JsonObject hello(String sha256, boolean canUpdate) {
        JsonObject o = new JsonObject();
        o.addProperty("type", "hello");
        o.addProperty("agent", "0.1.0");
        if (sha256 != null) {
            o.addProperty("sha256", sha256);
        }
        if (canUpdate) {
            o.addProperty("update", 1);
        }
        return o;
    }

    private static JsonObject ack(int seq) {
        JsonObject o = new JsonObject();
        o.addProperty("type", "update_ack");
        o.addProperty("seq", seq);
        return o;
    }

    private static JsonObject result(boolean ok) {
        JsonObject o = new JsonObject();
        o.addProperty("type", "update_result");
        o.addProperty("ok", ok);
        return o;
    }

    @Test
    void sendsTheBinaryInAcknowledgedChunks() {
        AgentUpdate.Binary bin = binary(AgentUpdate.CHUNK_BYTES * 2 + 1234);
        AgentUpdate update = new AgentUpdate(bin, "pc-test");
        Deque<JsonObject> sent = new ArrayDeque<>();

        update.onHello(hello("00".repeat(32), true), sent::add);
        assertTrue(update.active());
        JsonObject begin = sent.poll();
        assertEquals("update_begin", begin.get("type").getAsString());
        assertEquals(bin.data().length, begin.get("size").getAsInt());
        assertEquals(bin.sha256(), begin.get("sha256").getAsString());
        assertEquals(3, begin.get("chunks").getAsInt());
        assertTrue(sent.isEmpty(), "nothing goes out before the agent acknowledges");

        // поддельный агент: подтверждает каждое сообщение, собирает файл
        ByteArrayOutputStream file = new ByteArrayOutputStream();
        int accepted = 0;
        update.onAck(ack(0), sent::add);
        while (true) {
            assertEquals(1, sent.size(), "one message per acknowledgement");
            JsonObject m = sent.poll();
            assertTrue(m.toString().getBytes(StandardCharsets.UTF_8).length < 64 * 1024, "a line must stay below 64 KiB");
            if (m.get("type").getAsString().equals("update_end")) {
                break;
            }
            assertEquals("update_chunk", m.get("type").getAsString());
            assertEquals(accepted, m.get("seq").getAsInt());
            file.writeBytes(Base64.getDecoder().decode(m.get("data").getAsString()));
            accepted++;
            update.onAck(ack(accepted - 1), sent::add); // повтор старого подтверждения ничего не шлёт
            assertTrue(sent.isEmpty());
            update.onAck(ack(accepted), sent::add);
        }
        assertEquals(3, accepted);
        assertArrayEquals(bin.data(), file.toByteArray());

        update.onResult(result(true));
        assertFalse(update.active());
        // агент перезапустился с новым бинарником: больше ничего не шлём
        update.onHello(hello(bin.sha256(), true), sent::add);
        assertTrue(sent.isEmpty());
        assertFalse(update.active());
    }

    @Test
    void leavesOldAndCurrentAgentsAlone() {
        AgentUpdate.Binary bin = binary(1000);
        Deque<JsonObject> sent = new ArrayDeque<>();
        AgentUpdate update = new AgentUpdate(bin, "pc-test");
        update.onHello(hello(null, false), sent::add);               // агент старого образа
        update.onHello(hello("11".repeat(32), false), sent::add);    // хэш есть, но обновляться не умеет
        update.onHello(hello(bin.sha256(), true), sent::add);        // уже свежий
        update.onHello(hello(bin.sha256().toUpperCase(), true), sent::add);
        assertTrue(sent.isEmpty());
        assertFalse(update.active());

        // в jar нет агента: обновления нет вовсе
        AgentUpdate none = new AgentUpdate(null, "pc-test");
        none.onHello(hello("11".repeat(32), true), sent::add);
        none.onAck(ack(0), sent::add);
        assertTrue(sent.isEmpty());
    }

    @Test
    void doesNotLoopWhenTheAgentKeepsItsOldBinary() {
        AgentUpdate.Binary bin = binary(1000);
        Deque<JsonObject> sent = new ArrayDeque<>();
        AgentUpdate update = new AgentUpdate(bin, "pc-test");
        JsonObject old = hello("22".repeat(32), true);

        update.onHello(old, sent::add);
        assertEquals(1, sent.size());
        update.onHello(old, sent::add); // ping во время передачи: второй передачи не начинаем
        assertEquals(1, sent.size());
        update.onResult(result(false));
        assertFalse(update.active());

        sent.clear();
        update.onHello(old, sent::add); // тот же запуск ВМ: не повторяем
        assertTrue(sent.isEmpty());

        update.reset();                 // машину выключили и включили
        update.onHello(old, sent::add);
        assertEquals(1, sent.size());
    }

    @Test
    void givesUpOnASilentAgent() {
        AgentUpdate update = new AgentUpdate(binary(1000), "pc-test");
        Deque<JsonObject> sent = new ArrayDeque<>();
        update.onHello(hello("33".repeat(32), true), sent::add);
        for (int i = 0; i < AgentUpdate.TIMEOUT_TICKS; i++) {
            update.tick();
        }
        assertTrue(update.active());
        update.tick();
        assertFalse(update.active());
        sent.clear();
        update.onAck(ack(0), sent::add);
        assertTrue(sent.isEmpty());
    }
}
