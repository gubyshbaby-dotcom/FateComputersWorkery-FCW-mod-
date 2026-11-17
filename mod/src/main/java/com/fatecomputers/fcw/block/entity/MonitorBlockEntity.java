package com.fatecomputers.fcw.block.entity;

import com.fatecomputers.fcw.SidedHooks;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.registry.ModBlockEntities;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** A monitor: has its own type so that only monitors get the live-screen renderer. */
public class MonitorBlockEntity extends PeripheralBlockEntity {
    public MonitorBlockEntity(BlockPos pos, BlockState state) {
        super(ModBlockEntities.MONITOR.get(), pos, state);
    }

    public boolean isScreenOn() {
        return isLinked() && DeviceBlock.isPowered(getBlockState());
    }

    @Override
    public void onLoad() {
        super.onLoad();
        if (level != null && level.isClientSide) {
            SidedHooks.client().monitorLoaded(this);
        }
    }

    @Override
    public void setRemoved() {
        super.setRemoved();
        if (level != null && level.isClientSide) {
            SidedHooks.client().monitorUnloaded(this);
        }
    }

    @Override
    public void onChunkUnloaded() {
        super.onChunkUnloaded();
        if (level != null && level.isClientSide) {
            SidedHooks.client().monitorUnloaded(this);
        }
    }
}
