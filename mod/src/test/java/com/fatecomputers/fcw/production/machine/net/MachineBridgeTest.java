package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.google.gson.JsonObject;
import org.junit.jupiter.api.Test;

import java.nio.charset.StandardCharsets;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Части моста и питания, которым не нужен мир: ограничитель частоты, длина ответа, расход. */
class MachineBridgeTest {
    private static final long SECOND = 1_000_000_000L;

    @Test
    void bucketLimitsARunawayScript() {
        MachineBridge.Bucket bucket = new MachineBridge.Bucket();
        long now = 5 * SECOND;
        int passed = 0;
        for (int i = 0; i < 1000; i++) {      // тысяча запросов в один миг
            if (bucket.take(20, now)) {
                passed++;
            }
        }
        assertEquals(40, passed, "burst = two seconds of the rate");
        assertFalse(bucket.take(20, now));

        passed = 0;
        for (int i = 1; i <= 1000; i++) {     // ещё секунда: 1000 запросов, проходит 20
            if (bucket.take(20, now + i * SECOND / 1000)) {
                passed++;
            }
        }
        assertTrue(passed == 19 || passed == 20, "about one second of the rate, got " + passed);

        // после долгой паузы запас не больше двух секунд
        now += 3600 * SECOND;
        passed = 0;
        for (int i = 0; i < 1000; i++) {
            if (bucket.take(20, now)) {
                passed++;
            }
        }
        assertEquals(40, passed);
    }

    @Test
    void responseFitsIntoOneAgentLine() {
        JsonObject small = MachineBridge.response(18, 0, "состояние  работа", null);
        assertEquals("{\"type\":\"machine_resp\",\"id\":18,\"exit\":0,\"text\":\"состояние  работа\",\"json\":null}", small.toString());

        JsonObject cut = MachineBridge.response(1, 0, "я".repeat(100_000), null);
        assertTrue(cut.toString().getBytes(StandardCharsets.UTF_8).length <= MachineBridge.MAX_LINE_BYTES);
        assertEquals(0, cut.get("exit").getAsInt());
        assertTrue(cut.get("text").getAsString().endsWith("…"));

        JsonObject big = new JsonObject();
        big.addProperty("blob", "x".repeat(100_000));
        JsonObject refused = MachineBridge.response(2, 0, "ok", big);
        assertEquals(2, refused.get("exit").getAsInt());
        assertTrue(refused.get("json").isJsonNull());
    }

    @Test
    void langCodes() {
        assertEquals("ru_ru", ServerLang.normalize("ru_ru"));
        assertEquals("en_us", ServerLang.normalize(""));
        assertEquals("en_us", ServerLang.normalize(null));
        assertEquals("en_us", ServerLang.normalize("../../etc/passwd"));
        assertEquals("нет машины euv-9 в сети этого компьютера", ServerLang.get("ru_ru", "fcw.machine.net.none", "euv-9"));
        assertEquals("no machine euv-9 in this computer's network", ServerLang.get("xx_yy", "fcw.machine.net.none", "euv-9"));
        assertEquals("Дуговая печь", ServerLang.get("ru_ru", "machine.fcw.arc_furnace"));
    }

    @Test
    void computerPowerDraw() {
        assertEquals(20, ComputerBlockEntity.powerDraw(1, 512, false));
        assertEquals(28, ComputerBlockEntity.powerDraw(2, 2048, false));
        assertEquals(60, ComputerBlockEntity.powerDraw(4, 4096, true));
        assertEquals(92, ComputerBlockEntity.powerDraw(8, 8192, true));
    }
}
