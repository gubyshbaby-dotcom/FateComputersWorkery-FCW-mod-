package com.fatecomputers.fcw.client.audio;

import com.fatecomputers.fcw.client.audio.SpeakerLayout.Anchor;
import com.fatecomputers.fcw.client.audio.SpeakerLayout.Role;
import com.fatecomputers.fcw.client.audio.SpeakerLayout.Speaker;
import net.minecraft.core.BlockPos;
import net.minecraft.world.phys.Vec3;
import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Раскладка каналов по колонкам и фильтр сабвуфера: то, что в игре на слух не проверить. */
class SpeakerLayoutTest {
    /** Проектор в (0, 5, 0) светит на север (-z): для зрителя право — восток (+x). */
    private static final Anchor NORTH = new Anchor(new Vec3(0.5, 5.5, 0.5), new Vec3(1, 0, 0), "projector");

    private static Speaker full(int x, int z) {
        return new Speaker(new BlockPos(x, 1, z), false);
    }

    private static Speaker sub(int x, int z) {
        return new Speaker(new BlockPos(x, 1, z), true);
    }

    @Test
    void oneSpeakerPlaysEverything() {
        assertEquals(Map.of(new BlockPos(4, 1, -10), Role.MONO), SpeakerLayout.assign(List.of(full(4, -10)), NORTH));
        // одинокий сабвуфер тоже играет всё, иначе у машины пропал бы звук
        assertEquals(Map.of(new BlockPos(4, 1, -10), Role.MONO), SpeakerLayout.assign(List.of(sub(4, -10)), NORTH));
    }

    @Test
    void hallWithTwoSpeakersAndSubwoofer() {
        Map<BlockPos, Role> roles = SpeakerLayout.assign(List.of(full(-5, -12), full(5, -12), sub(0, -12)), NORTH);
        assertEquals(Role.LEFT, roles.get(new BlockPos(-5, 1, -12)));
        assertEquals(Role.RIGHT, roles.get(new BlockPos(5, 1, -12)));
        assertEquals(Role.SUB, roles.get(new BlockPos(0, 1, -12)));
        String text = SpeakerLayout.describe(roles, NORTH);
        assertTrue(text.startsWith("stereo by projector"), text);
    }

    @Test
    void sidesFollowTheAnchor() {
        // тот же зал, но проектор смотрит на юг: лево и право меняются местами
        Anchor south = new Anchor(new Vec3(0.5, 5.5, 0.5), new Vec3(-1, 0, 0), "projector");
        Map<BlockPos, Role> roles = SpeakerLayout.assign(List.of(full(-5, 12), full(5, 12)), south);
        assertEquals(Role.RIGHT, roles.get(new BlockPos(-5, 1, 12)));
        assertEquals(Role.LEFT, roles.get(new BlockPos(5, 1, 12)));
    }

    @Test
    void centreSpeakerGetsBothChannels() {
        Map<BlockPos, Role> roles = SpeakerLayout.assign(List.of(full(-5, -12), full(0, -12), full(5, -12)), NORTH);
        assertEquals(Role.MONO, roles.get(new BlockPos(0, 1, -12)));
        assertEquals(Role.LEFT, roles.get(new BlockPos(-5, 1, -12)));
    }

    @Test
    void noStereoWithoutBothSidesOrAnchor() {
        // обе колонки справа: канал не должен пропасть
        Map<BlockPos, Role> oneSide = SpeakerLayout.assign(List.of(full(3, -12), full(5, -12), sub(4, -12)), NORTH);
        assertEquals(Role.MONO, oneSide.get(new BlockPos(3, 1, -12)));
        assertEquals(Role.MONO, oneSide.get(new BlockPos(5, 1, -12)));
        assertEquals(Role.SUB, oneSide.get(new BlockPos(4, 1, -12)));
        Map<BlockPos, Role> noAnchor = SpeakerLayout.assign(List.of(full(-5, -12), full(5, -12)), null);
        assertTrue(noAnchor.values().stream().allMatch(r -> r == Role.MONO));
        assertEquals("mono; L+R: [(-5, 1, -12), (5, 1, -12)]", SpeakerLayout.describe(noAnchor, null));
    }

    private static double level(double hz) {
        LowPass f = new LowPass(120, 48_000);
        double peak = 0;
        for (int i = 0; i < 48_000; i++) {
            double y = f.step(Math.sin(2 * Math.PI * hz * i / 48_000));
            if (i > 24_000) {
                peak = Math.max(peak, Math.abs(y));
            }
        }
        return peak;
    }

    @Test
    void subwooferFilterPassesLowsOnly() {
        assertTrue(level(40) > 0.95, "40 Hz passes");
        assertEquals(Math.sqrt(0.5), level(120), 0.02, "-3 dB at the cutoff");
        assertTrue(level(1_000) < 0.02, "1 kHz is cut");
        assertTrue(level(5_000) < 0.001, "5 kHz is gone");
    }
}
