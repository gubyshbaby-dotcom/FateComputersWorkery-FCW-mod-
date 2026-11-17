package com.fatecomputers.fcw.production.machine.net;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;

/**
 * Какие машины видит компьютер. Контракт: docs/MACHINE_CONTROL.md. Владелец реализации — работа «сеть
 * данных» (кабель данных, роутер). Здесь временное правило «все машины в радиусе 32 блоков», чтобы
 * оболочку можно было проверять раньше.
 */
public final class MachineLinks {
    private static final int RADIUS = 32;

    private MachineLinks() {
    }

    /** Пульты машин, до которых компьютер в {@code computerPos} может достучаться, ближние первыми. */
    public static List<MachineControllerBlockEntity> reachable(ServerLevel level, BlockPos computerPos) {
        List<MachineControllerBlockEntity> out = new ArrayList<>();
        int cx = SectionPos.blockToSectionCoord(computerPos.getX());
        int cz = SectionPos.blockToSectionCoord(computerPos.getZ());
        int r = SectionPos.blockToSectionCoord(RADIUS) + 1;
        for (int x = cx - r; x <= cx + r; x++) {
            for (int z = cz - r; z <= cz + r; z++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(x, z);
                if (chunk == null) {
                    continue;
                }
                for (BlockEntity be : chunk.getBlockEntities().values()) {
                    if (be instanceof MachineControllerBlockEntity m && !m.isRemoved()
                            && m.getBlockPos().distSqr(computerPos) <= (double) RADIUS * RADIUS) {
                        out.add(m);
                    }
                }
            }
        }
        out.sort((a, b) -> Double.compare(a.getBlockPos().distSqr(computerPos), b.getBlockPos().distSqr(computerPos)));
        return out;
    }
}
