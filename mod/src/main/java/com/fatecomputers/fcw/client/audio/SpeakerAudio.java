package com.fatecomputers.fcw.client.audio;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import com.fatecomputers.fcw.cinema.ProjectorBlockEntity;
import com.fatecomputers.fcw.client.audio.SpeakerLayout.Anchor;
import com.fatecomputers.fcw.client.audio.SpeakerLayout.Role;
import com.fatecomputers.fcw.client.nativelib.FcwNative;
import com.fatecomputers.fcw.client.nativelib.NativeLoader;
import com.fatecomputers.fcw.client.view.ScreenViewManager;
import com.fatecomputers.fcw.registry.ModBlocks;
import net.minecraft.client.Minecraft;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import org.lwjgl.BufferUtils;
import org.lwjgl.openal.AL10;
import org.lwjgl.openal.AL11;
import org.lwjgl.openal.ALC10;

import javax.annotation.Nullable;
import java.nio.ShortBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * Звук гостя через привязанные колонки. PCM из {@link FcwNative#readAudio} (48 кГц, стерео s16)
 * читается один раз на машину и раздаётся по колонкам: у каждой свой позиционный источник OpenAL.
 *
 * <p>Кто что играет, решает {@link SpeakerLayout}: одна колонка — сумма каналов, как раньше; две и
 * больше — левый и правый каналы в колонки слева и справа от проектора, монитора или компьютера;
 * сабвуфер — только низы ({@link LowPass}). Источники одной машины получают одни и те же отсчёты
 * в одном тике и стартуют вместе, поэтому каналы не расходятся. При смене состава колонок очереди
 * сбрасываются и всё стартует заново.
 *
 * <p>Затухание линейное: в {@link #FULL_VOLUME_DISTANCE} блоках от колонки громкость полная, к
 * {@link #HEARING_DISTANCE} сходит на нет — в зале на 20–30 блоков задние ряды слышат около половины.
 */
public final class SpeakerAudio {
    private static final SpeakerAudio INSTANCE = new SpeakerAudio();
    private static final int RATE = 48_000;
    private static final int HEARING_DISTANCE = 48;
    private static final float FULL_VOLUME_DISTANCE = 6f;
    private static final int MAX_QUEUED = 8;
    private static final double SUB_CUTOFF_HZ = 120;
    /** Низы без середины тише на слух: сабвуфер слегка усилен. */
    private static final double SUB_GAIN = 1.6;
    // AL_EXT_source_distance_model: модель затухания задаётся у источника (её включает сам Minecraft)
    private static final int AL_DISTANCE_MODEL = 0xD000;

    private final Set<PeripheralBlockEntity> speakers = Collections.newSetFromMap(new IdentityHashMap<>());
    private final Map<String, Group> groups = new HashMap<>();
    /** Что уже записано в журнал про каждую машину: раскладка пишется один раз на каждое изменение. */
    private final Map<String, String> logged = new HashMap<>();
    private final short[] pcm = new short[RATE / 5 * 2]; // до 200 мс стерео
    private boolean broken;
    private boolean noDeviceLogged;

    /** Один источник OpenAL у одной колонки. */
    private static final class Voice {
        final int source;
        final Role role;
        final LowPass lowPass = new LowPass(SUB_CUTOFF_HZ, RATE);

        Voice(int source, Role role) {
            this.source = source;
            this.role = role;
        }
    }

    /** Все колонки одной машины. */
    private static final class Group {
        final Map<BlockPos, Role> roles;
        final Map<BlockPos, Voice> voices = new LinkedHashMap<>();
        long lastUsed;

        Group(Map<BlockPos, Role> roles) {
            this.roles = roles;
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
        Vec3 ear = mc.player.getEyePosition();
        // включённые колонки по машинам; машина слышна, если хоть одна её колонка в пределах слышимости
        Map<UUID, List<PeripheralBlockEntity>> byVm = new LinkedHashMap<>();
        Map<UUID, PeripheralBlockEntity> nearest = new HashMap<>();
        for (Iterator<PeripheralBlockEntity> it = speakers.iterator(); it.hasNext(); ) {
            PeripheralBlockEntity s = it.next();
            if (s.isRemoved() || s.getLevel() != mc.level) {
                it.remove();
                continue;
            }
            UUID vm = s.vmId();
            if (!s.isLinked() || vm == null || !DeviceBlock.isPowered(s.getBlockState())) {
                continue;
            }
            byVm.computeIfAbsent(vm, k -> new ArrayList<>()).add(s);
            double d = s.getBlockPos().getCenter().distanceToSqr(ear);
            PeripheralBlockEntity cur = nearest.get(vm);
            if (d <= HEARING_DISTANCE * HEARING_DISTANCE && (cur == null || cur.getBlockPos().getCenter().distanceToSqr(ear) > d)) {
                nearest.put(vm, s);
            }
        }
        // Раскладка считается и пишется в журнал даже без звукового устройства: по ней видно,
        // какой канал достанется какой колонке.
        Map<String, Map<BlockPos, Role>> layouts = new LinkedHashMap<>();
        for (Map.Entry<UUID, PeripheralBlockEntity> e : nearest.entrySet()) {
            UUID vm = e.getKey();
            List<SpeakerLayout.Speaker> list = new ArrayList<>();
            for (PeripheralBlockEntity s : byVm.get(vm)) {
                list.add(new SpeakerLayout.Speaker(s.getBlockPos(), s.getBlockState().is(ModBlocks.SUBWOOFER.get())));
            }
            list.sort((a, b) -> a.pos().compareTo(b.pos()));
            Anchor anchor = list.size() > 1 ? anchor(vm, byVm.get(vm).get(0), ear, mc.level) : null;
            Map<BlockPos, Role> roles = SpeakerLayout.assign(list, anchor);
            layouts.put(vm.toString(), roles);
            String text = SpeakerLayout.describe(roles, anchor);
            if (!text.equals(logged.put(vm.toString(), text))) {
                FcwMod.LOGGER.info("Computer sound of {}: {}", vm, text);
            }
        }
        logged.keySet().retainAll(layouts.keySet());

        if (ALC10.alcGetCurrentContext() == 0L) {
            // Звук Minecraft выключен (нет звукового устройства): играть некуда.
            if (!noDeviceLogged) {
                noDeviceLogged = true;
                FcwMod.LOGGER.info("No OpenAL context, computer sound is disabled");
            }
            return;
        }
        long now = System.currentTimeMillis();
        float volume = (float) (FcwConfig.CLIENT.audioVolume.get()
                * mc.options.getSoundSourceVolume(SoundSource.MASTER)
                * mc.options.getSoundSourceVolume(SoundSource.RECORDS));
        try {
            for (Map.Entry<String, Map<BlockPos, Role>> e : layouts.entrySet()) {
                String vm = e.getKey();
                // Токен просмотра даёт и звук: держим для него минимальный (1 кадр/с) вид экрана.
                ScreenViewManager.get().wantAudio(vm, nearest.get(UUID.fromString(vm)).getBlockPos());
                ScreenViewManager.Link link = ScreenViewManager.get().link(vm);
                if (link == null || link.state() != ScreenViewManager.LinkState.ATTACHED || link.sessionHandle() == 0) {
                    continue;
                }
                Group g = groups.get(vm);
                if (g == null || !g.roles.equals(e.getValue())) {
                    release(g);
                    g = open(e.getValue());
                    groups.put(vm, g);
                }
                g.lastUsed = now;
                for (Voice v : g.voices.values()) {
                    AL10.alSourcef(v.source, AL10.AL_GAIN, volume);
                }
                pump(g, link.sessionHandle(), vm);
            }
        } catch (Throwable t) {
            FcwMod.LOGGER.warn("Computer audio failed, disabling it for this session: {}", t.toString());
            broken = true;
            stopAll();
            return;
        }
        for (Iterator<Map.Entry<String, Group>> it = groups.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<String, Group> e = it.next();
            if (now - e.getValue().lastUsed > 2_000) {
                release(e.getValue());
                it.remove();
            }
        }
    }

    /**
     * На что смотрит зритель этой машины: горящий проектор, иначе ближайший монитор, иначе любой
     * проектор, иначе сам компьютер. Вправо от зрителя: у проектора — по часовой от направления
     * объектива; монитор и компьютер смотрят лицом на зрителя, поэтому у них наоборот.
     */
    @Nullable
    private static Anchor anchor(UUID vm, PeripheralBlockEntity anySpeaker, Vec3 ear, Level level) {
        ProjectorBlockEntity projector = null;
        for (ProjectorBlockEntity p : ScreenViewManager.get().projectors()) {
            if (vm.equals(p.vmId()) && !p.isRemoved() && p.getLevel() == level
                    && (projector == null || (p.isLit() && !projector.isLit()))) {
                projector = p;
            }
        }
        if (projector != null && projector.isLit()) {
            return anchor(projector.getBlockPos(), projector.getBlockState(), true, "projector");
        }
        MonitorBlockEntity monitor = null;
        for (MonitorBlockEntity m : ScreenViewManager.get().monitors()) {
            if (vm.equals(m.vmId()) && !m.isRemoved() && m.getLevel() == level && (monitor == null
                    || m.getBlockPos().getCenter().distanceToSqr(ear) < monitor.getBlockPos().getCenter().distanceToSqr(ear))) {
                monitor = m;
            }
        }
        if (monitor != null) {
            return anchor(monitor.getBlockPos(), monitor.getBlockState(), false, "monitor");
        }
        if (projector != null) {
            return anchor(projector.getBlockPos(), projector.getBlockState(), true, "projector");
        }
        BlockPos pc = anySpeaker.computerPos();
        return pc == null ? null : anchor(pc, level.getBlockState(pc), false, "computer");
    }

    @Nullable
    private static Anchor anchor(BlockPos pos, BlockState state, boolean looksAway, String what) {
        if (!state.hasProperty(DeviceBlock.FACING)) {
            return null;
        }
        Direction facing = state.getValue(DeviceBlock.FACING);
        Direction right = looksAway ? facing.getClockWise() : facing.getCounterClockWise();
        return new Anchor(Vec3.atCenterOf(pos), Vec3.atLowerCornerOf(right.getNormal()), what + " at " + pos.toShortString());
    }

    private static Group open(Map<BlockPos, Role> roles) {
        Group g = new Group(roles);
        roles.forEach((pos, role) -> {
            int src = AL10.alGenSources();
            Vec3 c = pos.getCenter();
            AL10.alSource3f(src, AL10.AL_POSITION, (float) c.x, (float) c.y, (float) c.z);
            AL10.alSourcei(src, AL_DISTANCE_MODEL, AL11.AL_LINEAR_DISTANCE_CLAMPED);
            AL10.alSourcef(src, AL10.AL_REFERENCE_DISTANCE, FULL_VOLUME_DISTANCE);
            AL10.alSourcef(src, AL10.AL_MAX_DISTANCE, HEARING_DISTANCE);
            AL10.alSourcef(src, AL10.AL_ROLLOFF_FACTOR, 1f);
            AL10.alGetError(); // модель у источника — расширение; без него остаётся общая модель
            g.voices.put(pos, new Voice(src, role));
        });
        return g;
    }

    private void pump(Group g, long session, String vm) {
        // отыгранные буферы — на выброс; очередь меряется по самой короткой
        int queued = Integer.MAX_VALUE;
        for (Voice v : g.voices.values()) {
            int processed = AL10.alGetSourcei(v.source, AL10.AL_BUFFERS_PROCESSED);
            while (processed-- > 0) {
                AL10.alDeleteBuffers(AL10.alSourceUnqueueBuffers(v.source));
            }
            queued = Math.min(queued, AL10.alGetSourcei(v.source, AL10.AL_BUFFERS_QUEUED));
        }
        while (queued < MAX_QUEUED) {
            int n = FcwNative.readAudio(session, vm, pcm);
            if (n <= 1) {
                break;
            }
            int frames = Math.min(n, pcm.length) / 2;
            for (Voice v : g.voices.values()) {
                int buf = AL10.alGenBuffers();
                AL10.alBufferData(buf, AL10.AL_FORMAT_MONO16, channel(pcm, frames, v), RATE);
                AL10.alSourceQueueBuffers(v.source, buf);
            }
            queued++;
        }
        if (queued == 0 || queued == Integer.MAX_VALUE) {
            return;
        }
        // Источники стартуют одним вызовом, чтобы каналы совпадали по времени. Уже играющий
        // источник трогать нельзя: alSourcePlay начал бы его очередь сначала.
        List<Voice> stopped = new ArrayList<>();
        for (Voice v : g.voices.values()) {
            if (AL10.alGetSourcei(v.source, AL10.AL_SOURCE_STATE) != AL10.AL_PLAYING) {
                stopped.add(v);
            }
        }
        if (!stopped.isEmpty()) {
            int[] ids = new int[stopped.size()];
            for (int i = 0; i < ids.length; i++) {
                ids[i] = stopped.get(i).source;
            }
            AL10.alSourcePlayv(ids);
        }
    }

    /** Моно-буфер для одной колонки из стерео-отсчётов: её канал, сумма каналов или низы суммы. */
    private static ShortBuffer channel(short[] stereo, int frames, Voice v) {
        ShortBuffer out = BufferUtils.createShortBuffer(frames);
        for (int i = 0; i < frames; i++) {
            int l = stereo[2 * i], r = stereo[2 * i + 1];
            out.put(switch (v.role) {
                case LEFT -> (short) l;
                case RIGHT -> (short) r;
                case MONO -> (short) ((l + r) / 2);
                case SUB -> (short) Math.max(Short.MIN_VALUE, Math.min(Short.MAX_VALUE,
                        Math.round(v.lowPass.step((l + r) / 2.0) * SUB_GAIN)));
            });
        }
        out.flip();
        return out;
    }

    private static void release(@Nullable Group g) {
        if (g == null) {
            return;
        }
        for (Voice v : g.voices.values()) {
            try {
                AL10.alSourceStop(v.source);
                int queued = AL10.alGetSourcei(v.source, AL10.AL_BUFFERS_QUEUED);
                while (queued-- > 0) {
                    AL10.alDeleteBuffers(AL10.alSourceUnqueueBuffers(v.source));
                }
                AL10.alDeleteSources(v.source);
            } catch (Throwable ignored) {
            }
        }
        g.voices.clear();
    }

    public void stopAll() {
        if (groups.isEmpty()) {
            return;
        }
        groups.values().forEach(SpeakerAudio::release);
        groups.clear();
    }

    public void reset() {
        stopAll();
        speakers.clear();
        logged.clear();
        broken = false;
    }
}
