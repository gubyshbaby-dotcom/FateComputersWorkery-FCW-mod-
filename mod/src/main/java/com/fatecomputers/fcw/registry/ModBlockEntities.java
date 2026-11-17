package com.fatecomputers.fcw.registry;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.block.entity.PeripheralBlockEntity;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;

public final class ModBlockEntities {
    public static final DeferredRegister<BlockEntityType<?>> BLOCK_ENTITIES =
            DeferredRegister.create(Registries.BLOCK_ENTITY_TYPE, FcwMod.MODID);

    @SuppressWarnings("DataFlowIssue")
    public static final DeferredHolder<BlockEntityType<?>, BlockEntityType<ComputerBlockEntity>> COMPUTER =
            BLOCK_ENTITIES.register("computer", () -> BlockEntityType.Builder.of(ComputerBlockEntity::new, ModBlocks.computerBlocks()).build(null));

    @SuppressWarnings("DataFlowIssue")
    public static final DeferredHolder<BlockEntityType<?>, BlockEntityType<MonitorBlockEntity>> MONITOR =
            BLOCK_ENTITIES.register("monitor", () -> BlockEntityType.Builder.of(MonitorBlockEntity::new, ModBlocks.monitorBlocks()).build(null));

    @SuppressWarnings("DataFlowIssue")
    public static final DeferredHolder<BlockEntityType<?>, BlockEntityType<PeripheralBlockEntity>> PERIPHERAL =
            BLOCK_ENTITIES.register("peripheral", () -> BlockEntityType.Builder.of(PeripheralBlockEntity::new, ModBlocks.otherPeripheralBlocks()).build(null));

    private ModBlockEntities() {
    }
}
