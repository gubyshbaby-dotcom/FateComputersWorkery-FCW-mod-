package com.fatecomputers.fcw.cinema;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.registry.ModBlockEntities;
import com.fatecomputers.fcw.registry.ModBlocks;
import com.fatecomputers.fcw.registry.ModItems;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.material.MapColor;
import net.minecraft.world.level.material.PushReaction;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;

/**
 * Всё для кино: проектор, лампа к нему, проекционный экран, кресло и сущность-сиденье.
 * Как это работает вместе: docs/CINEMA.md.
 */
public final class Cinema {
    private static final DeferredRegister<EntityType<?>> ENTITIES = DeferredRegister.create(Registries.ENTITY_TYPE, FcwMod.MODID);

    public static final DeferredBlock<ProjectorBlock> PROJECTOR = ModBlocks.BLOCKS.registerBlock("projector", ProjectorBlock::new,
            small(MapColor.METAL, SoundType.METAL, 1.0f).lightLevel(s -> DeviceBlock.isPowered(s) ? 4 : 0));
    public static final DeferredBlock<ProjectionScreenBlock> PROJECTION_SCREEN = ModBlocks.BLOCKS.registerBlock("projection_screen",
            ProjectionScreenBlock::new, small(MapColor.SNOW, SoundType.WOOL, 0.3f).ignitedByLava());
    public static final DeferredBlock<CinemaSeatBlock> CINEMA_SEAT = ModBlocks.BLOCKS.registerBlock("cinema_seat",
            CinemaSeatBlock::new, small(MapColor.COLOR_RED, SoundType.WOOD, 0.8f).ignitedByLava());

    public static final DeferredItem<TooltipBlockItem> PROJECTOR_ITEM = ModItems.ITEMS.registerItem("projector",
            p -> new TooltipBlockItem(PROJECTOR.get(), p, "tooltip.fcw.projector", "tooltip.fcw.projector.mount", "tooltip.fcw.peripheral.link"),
            new Item.Properties());
    public static final DeferredItem<TooltipBlockItem> PROJECTION_SCREEN_ITEM = ModItems.ITEMS.registerItem("projection_screen",
            p -> new TooltipBlockItem(PROJECTION_SCREEN.get(), p, "tooltip.fcw.projection_screen", "tooltip.fcw.projection_screen.fit"),
            new Item.Properties());
    public static final DeferredItem<TooltipBlockItem> CINEMA_SEAT_ITEM = ModItems.ITEMS.registerItem("cinema_seat",
            p -> new TooltipBlockItem(CINEMA_SEAT.get(), p, "tooltip.fcw.cinema_seat"), new Item.Properties());
    public static final DeferredItem<ProjectorLampItem> PROJECTOR_LAMP = ModItems.ITEMS.registerItem("projector_lamp",
            ProjectorLampItem::new, new Item.Properties().stacksTo(16));

    @SuppressWarnings("DataFlowIssue")
    public static final DeferredHolder<BlockEntityType<?>, BlockEntityType<ProjectorBlockEntity>> PROJECTOR_BE =
            ModBlockEntities.BLOCK_ENTITIES.register("projector",
                    () -> BlockEntityType.Builder.of(ProjectorBlockEntity::new, PROJECTOR.get()).build(null));

    public static final DeferredHolder<EntityType<?>, EntityType<SeatEntity>> SEAT = ENTITIES.register("seat",
            () -> EntityType.Builder.<SeatEntity>of(SeatEntity::new, MobCategory.MISC)
                    .sized(0.001f, 0.001f)
                    .noSummon()
                    .clientTrackingRange(8)
                    .updateInterval(20)
                    .build("seat"));

    static {
        ModItems.TAB_ORDER.add(PROJECTOR_ITEM);
        ModItems.TAB_ORDER.add(PROJECTOR_LAMP);
        ModItems.TAB_ORDER.add(PROJECTION_SCREEN_ITEM);
        ModItems.TAB_ORDER.add(CINEMA_SEAT_ITEM);
    }

    private Cinema() {
    }

    public static void register(IEventBus modBus) {
        ENTITIES.register(modBus);
        modBus.addListener(ProjectorBlockEntity::registerCapabilities);
    }

    /** Небольшой неполный блок, как прочие устройства мода. */
    private static BlockBehaviour.Properties small(MapColor color, SoundType sound, float strength) {
        return BlockBehaviour.Properties.of()
                .mapColor(color)
                .strength(strength, 3.0f)
                .sound(sound)
                .noOcclusion()
                .isRedstoneConductor((s, l, p) -> false)
                .isSuffocating((s, l, p) -> false)
                .isViewBlocking((s, l, p) -> false)
                .pushReaction(PushReaction.BLOCK);
    }
}
