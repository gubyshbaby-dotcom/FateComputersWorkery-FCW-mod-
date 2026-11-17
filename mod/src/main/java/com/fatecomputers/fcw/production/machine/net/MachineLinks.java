package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import it.unimi.dsi.fastutil.longs.LongArrayFIFOQueue;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import it.unimi.dsi.fastutil.longs.LongSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.SectionPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * Какие машины видит компьютер. Контракт: docs/MACHINE_CONTROL.md.
 *
 * <p>Сеть данных компьютера — всё, до чего от его корпуса можно дойти по кабелям данных и роутерам
 * (соседство по шести сторонам). Машина видна <b>по кабелю</b>, если любая её клетка касается узла
 * этой сети или самого корпуса, и <b>по радио</b>, если её пульт стоит в радиусе
 * {@code production.routerRadius} от любого роутера сети. Обход идёт только по загруженным чанкам и
 * не дальше {@link #MAX_NODES} узлов, так что длинный кабель сервер не повесит.
 */
public final class MachineLinks {
    /** Больше узлов одна сеть не имеет: дальше обход не идёт. */
    public static final int MAX_NODES = 4096;
    /** Больше роутеров одной сети не учитывается. */
    public static final int MAX_ROUTERS = 32;

    /** Как машина видна компьютеру. */
    public enum Via {
        CABLE, RADIO;

        /** Значение поля {@code link} в ответе {@code list}. */
        public String id() {
            return name().toLowerCase(Locale.ROOT);
        }
    }

    /** Машина и способ, которым до неё достучались; кабель важнее радио. */
    public record Link(MachineControllerBlockEntity machine, Via via) {
    }

    private MachineLinks() {
    }

    /** Пульты машин, до которых компьютер в {@code computerPos} может достучаться, ближние первыми. */
    public static List<MachineControllerBlockEntity> reachable(ServerLevel level, BlockPos computerPos) {
        List<MachineControllerBlockEntity> out = new ArrayList<>();
        for (Link l : links(level, computerPos)) {
            out.add(l.machine());
        }
        return out;
    }

    /** То же, что {@link #reachable}, но с пометкой «по кабелю» или «по радио»; без повторов. */
    public static List<Link> links(ServerLevel level, BlockPos computerPos) {
        Map<MachineControllerBlockEntity, Via> found = new LinkedHashMap<>();
        List<BlockPos> routers = new ArrayList<>();

        LongSet seen = new LongOpenHashSet();
        LongArrayFIFOQueue queue = new LongArrayFIFOQueue();
        seen.add(computerPos.asLong());
        queue.enqueue(computerPos.asLong());
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        BlockPos.MutableBlockPos next = new BlockPos.MutableBlockPos();
        int nodes = 0;
        while (!queue.isEmpty() && nodes < MAX_NODES) {
            at.set(queue.dequeueLong());
            nodes++;
            for (Direction d : Direction.values()) {
                next.setWithOffset(at, d);
                if (seen.contains(next.asLong()) || !level.isLoaded(next)) {
                    continue;
                }
                BlockState state = level.getBlockState(next);
                if (state.getBlock() instanceof MachineCellBlock cell) {
                    MachineControllerBlockEntity m = cell.controller(level, next, state);
                    if (m != null && !m.isRemoved()) {
                        found.put(m, Via.CABLE);
                    }
                } else if (DataNetwork.isNode(state)) {
                    seen.add(next.asLong());
                    queue.enqueue(next.asLong());
                    if (DataNetwork.isRouter(state) && routers.size() < MAX_ROUTERS) {
                        routers.add(next.immutable());
                    }
                }
            }
        }

        if (!routers.isEmpty()) {
            int radius = FcwConfig.SERVER.routerRadius.get();
            double r2 = (double) radius * radius;
            LongSet chunks = new LongOpenHashSet();
            for (BlockPos router : routers) {
                int x0 = SectionPos.blockToSectionCoord(router.getX() - radius), x1 = SectionPos.blockToSectionCoord(router.getX() + radius);
                int z0 = SectionPos.blockToSectionCoord(router.getZ() - radius), z1 = SectionPos.blockToSectionCoord(router.getZ() + radius);
                for (int x = x0; x <= x1; x++) {
                    for (int z = z0; z <= z1; z++) {
                        chunks.add(ChunkPos.asLong(x, z));
                    }
                }
            }
            for (long key : chunks) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(key), ChunkPos.getZ(key));
                if (chunk == null) {
                    continue;
                }
                for (BlockEntity be : chunk.getBlockEntities().values()) {
                    if (!(be instanceof MachineControllerBlockEntity m) || m.isRemoved() || found.containsKey(m)) {
                        continue;
                    }
                    for (BlockPos router : routers) {
                        if (m.getBlockPos().distSqr(router) <= r2) {
                            found.put(m, Via.RADIO);
                            break;
                        }
                    }
                }
            }
        }

        List<Link> out = new ArrayList<>(found.size());
        found.forEach((m, via) -> out.add(new Link(m, via)));
        out.sort(Comparator.comparingDouble(l -> l.machine().getBlockPos().distSqr(computerPos)));
        return out;
    }
}
