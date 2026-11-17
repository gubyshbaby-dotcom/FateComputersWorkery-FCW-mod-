package com.fatecomputers.fcw.client.audio;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import com.fatecomputers.fcw.client.nativelib.FcwNative;
import com.fatecomputers.fcw.client.nativelib.NativeLoader;
import com.fatecomputers.fcw.client.view.ScreenViewManager;
import net.minecraft.client.Minecraft;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.phys.Vec3;
import org.lwjgl.BufferUtils;
import org.lwjgl.openal.AL10;

import javax.annotation.Nullable;
import java.nio.ShortBuffer;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;

/**
 * Guest sound through linked speakers: PCM from {@link FcwNative#readAudio} (48 kHz, stereo s16)
 * is mixed down to mono and streamed into a positional OpenAL source at the nearest speaker of
 * the machine. Deliberately small: one source per machine, a few queued buffers, no resampling.
 */
public final class SpeakerAudio {
    private static final SpeakerAudio INSTANCE = new SpeakerAudio();
    private static final int RATE = 48_000;
    private static final int HEARING_DISTANCE = 32;
    private static final int MAX_QUEUED = 8;

    private final Set<PeripheralBlockEntity> speakers = Collections.newSetFromMap(new IdentityHashMap<>());
    private final Map<String, Stream> streams = new HashMap<>();
    private final short[] pcm = new short[RATE / 5 * 2]; // up to 200 ms of stereo
    private boolean broken;

    private static final class Stream {
        final int source;
        long lastUsed;

        Stream(int source) {
            this.source = source;
        }
    }

    private SpeakerAudio() {
    }

    public static SpeakerAudio get() {
        return INSTANCE;
    }

    public void speakerLoaded(PeripheralBlockEntity be) {
        speakers.add(be);
    }

    public void speakerUnloaded(PeripheralBlockEntity be) {
        speakers.remove(be);
    }

    public void tick() {
        Minecraft mc = Minecraft.getInstance();
        if (broken || mc.player == null || mc.level == null || !FcwConfig.CLIENT.audioEnabled.get() || !NativeLoader.isLoaded()) {
            stopAll();
            return;
        }
        long now = System.currentTimeMillis();
        Vec3 ear = mc.player.getEyePosition();
        // nearest powered speaker per machine
        Map<String, PeripheralBlockEntity> nearest = new HashMap<>();
        for (Iterator<PeripheralBlockEntity> it = speakers.iterator(); it.hasNext(); ) {
            PeripheralBlockEntity s = it.next();
            if (s.isRemoved() || s.getLevel() != mc.level) {
                it.remove();
                continue;
            }
            if (!s.isLinked() || s.vmId() == null || !DeviceBlock.isPowered(s.getBlockState())) {
                continue;
            }
            double d = s.getBlockPos().getCenter().distanceToSqr(ear);
            if (d > HEARING_DISTANCE * HEARING_DISTANCE) {
                continue;
            }
            String vm = s.vmId().toString();
            PeripheralBlockEntity cur = nearest.get(vm);
            if (cur == null || cur.getBlockPos().getCenter().distanceToSqr(ear) > d) {
                nearest.put(vm, s);
            }
        }
        float volume = (float) (FcwConfig.CLIENT.audioVolume.get()
                * mc.options.getSoundSourceVolume(SoundSource.MASTER)
                * mc.options.getSoundSourceVolume(SoundSource.RECORDS));
        try {
            for (Map.Entry<String, PeripheralBlockEntity> e : nearest.entrySet()) {
                String vm = e.getKey();
                // A view token also grants sound; keep a minimal (1 fps) view open for it.
                ScreenViewManager.get().wantAudio(vm, e.getValue().getBlockPos());
                ScreenViewManager.Link link = ScreenViewManager.get().link(vm);
                if (link == null || link.state() != ScreenViewManager.LinkState.ATTACHED || link.sessionHandle() == 0) {
                    continue;
                }
                Stream st = streams.computeIfAbsent(vm, k -> new Stream(AL10.alGenSources()));
                st.lastUsed = now;
                Vec3 c = e.getValue().getBlockPos().getCenter();
                AL10.alSource3f(st.source, AL10.AL_POSITION, (float) c.x, (float) c.y, (float) c.z);
                AL10.alSourcef(st.source, AL10.AL_GAIN, volume);
                AL10.alSourcef(st.source, AL10.AL_REFERENCE_DISTANCE, 4f);
                AL10.alSourcef(st.source, AL10.AL_MAX_DISTANCE, HEARING_DISTANCE);
                pump(st, link.sessionHandle(), vm);
            }
        } catch (Throwable t) {
            FcwMod.LOGGER.error("Computer audio failed, disabling it for this session", t);
            broken = true;
            stopAll();
            return;
        }
        for (Iterator<Map.Entry<String, Stream>> it = streams.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<String, Stream> e = it.next();
            if (now - e.getValue().lastUsed > 2_000) {
                release(e.getValue());
                it.remove();
            }
        }
    }

    private void pump(Stream st, long session, String vm) {
        // recycle played buffers
        int processed = AL10.alGetSourcei(st.source, AL10.AL_BUFFERS_PROCESSED);
        while (processed-- > 0) {
            int b = AL10.alSourceUnqueueBuffers(st.source);
            AL10.alDeleteBuffers(b);
        }
        int queued = AL10.alGetSourcei(st.source, AL10.AL_BUFFERS_QUEUED);
        while (queued < MAX_QUEUED) {
            int n = FcwNative.readAudio(session, vm, pcm);
            if (n <= 1) {
                break;
            }
            int frames = Math.min(n, pcm.length) / 2;
            ShortBuffer mono = BufferUtils.createShortBuffer(frames);
            for (int i = 0; i < frames; i++) {
                mono.put((short) ((pcm[2 * i] + pcm[2 * i + 1]) / 2));
            }
            mono.flip();
            int buf = AL10.alGenBuffers();
            AL10.alBufferData(buf, AL10.AL_FORMAT_MONO16, mono, RATE);
            AL10.alSourceQueueBuffers(st.source, buf);
            queued++;
        }
        if (queued > 0 && AL10.alGetSourcei(st.source, AL10.AL_SOURCE_STATE) != AL10.AL_PLAYING) {
            AL10.alSourcePlay(st.source);
        }
    }

    private static void release(@Nullable Stream st) {
        if (st == null) {
            return;
        }
        try {
            AL10.alSourceStop(st.source);
            int queued = AL10.alGetSourcei(st.source, AL10.AL_BUFFERS_QUEUED);
            while (queued-- > 0) {
                AL10.alDeleteBuffers(AL10.alSourceUnqueueBuffers(st.source));
            }
            AL10.alDeleteSources(st.source);
        } catch (Throwable ignored) {
        }
    }

    public void stopAll() {
        if (streams.isEmpty()) {
            return;
        }
        streams.values().forEach(SpeakerAudio::release);
        streams.clear();
    }

    public void reset() {
        stopAll();
        speakers.clear();
        broken = false;
    }
}
