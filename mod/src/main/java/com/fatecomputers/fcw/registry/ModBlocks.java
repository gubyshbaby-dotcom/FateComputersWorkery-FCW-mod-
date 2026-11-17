package com.fatecomputers.fcw.registry;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.ComputerBlock;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.DeviceModel;
import com.fatecomputers.fcw.block.PeripheralBlock;
import com.fatecomputers.fcw.block.PeripheralKind;
import com.fatecomputers.fcw.block.PoweredPeripheralBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.material.MapColor;
import net.minecraft.world.level.material.PushReaction;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.ArrayList;
import java.util.List;

public final class ModBlocks {
    public static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks(FcwMod.MODID);

    public static final List<DeferredBlock<ComputerBlock>> COMPUTERS = new ArrayList<>();
    public static final List<DeferredBlock<? extends PeripheralBlock>> PERIPHERALS = new ArrayList<>();
    public static final List<DeferredBlock<? extends PeripheralBlock>> MONITORS = new ArrayList<>();

    // Computers: all functionally identical, only the case differs.
    public static final DeferredBlock<ComputerBlock> SYSTEM_UNIT = computer("system_unit", DeviceModel.SYSTEM_UNIT);
    public static final DeferredBlock<ComputerBlock> CASE_DESKTOP = computer("case_desktop", DeviceModel.CASE_DESKTOP);
    public static final DeferredBlock<ComputerBlock> CASE_TOWER_RETRO = computer("case_tower_retro", DeviceModel.CASE_TOWER_RETRO);
    public static final DeferredBlock<ComputerBlock> CASE_TOWER = computer("case_tower", DeviceModel.CASE_TOWER);
    public static final DeferredBlock<ComputerBlock> CASE_GAMING = computer("case_gaming", DeviceModel.CASE_GAMING);
    public static final DeferredBlock<ComputerBlock> CASE_CUBE = computer("case_cube", DeviceModel.CASE_CUBE);
    public static final DeferredBlock<ComputerBlock> CASE_SERVER = computer("case_server", DeviceModel.CASE_SERVER);

    // Monitors
    public static final DeferredBlock<PoweredPeripheralBlock> MONITOR = monitor("monitor", DeviceModel.MONITOR);
    public static final DeferredBlock<PoweredPeripheralBlock> CRT_MONITOR = monitor("crt_monitor", DeviceModel.CRT_MONITOR);

    // Input devices
    public static final DeferredBlock<PeripheralBlock> KEYBOARD = input("keyboard", DeviceModel.KEYBOARD, PeripheralKind.KEYBOARD);
    public static final DeferredBlock<PeripheralBlock> MOUSE = input("mouse", DeviceModel.MOUSE, PeripheralKind.MOUSE);

    // Sound output
    public static final DeferredBlock<PoweredPeripheralBlock> SPEAKER_DESKTOP = speaker("speaker_desktop", DeviceModel.SPEAKER_DESKTOP);
    public static final DeferredBlock<PoweredPeripheralBlock> SPEAKER_BOOKSHELF = speaker("speaker_bookshelf", DeviceModel.SPEAKER_BOOKSHELF);
    public static final DeferredBlock<PoweredPeripheralBlock> SPEAKER_TOWER = speaker("speaker_tower", DeviceModel.SPEAKER_TOWER);
    public static final DeferredBlock<PoweredPeripheralBlock> SUBWOOFER = speaker("subwoofer", DeviceModel.SUBWOOFER);
    public static final DeferredBlock<PoweredPeripheralBlock> SOUNDBAR = speaker("soundbar", DeviceModel.SOUNDBAR);

    private ModBlocks() {
    }

    private static BlockBehaviour.Properties deviceProps(SoundType sound, float strength) {
        return BlockBehaviour.Properties.of()
                .mapColor(MapColor.METAL)
                .strength(strength, 6.0f)
                .sound(sound)
                .noOcclusion()
                .isRedstoneConductor((s, l, p) -> false)
                .isSuffocating((s, l, p) -> false)
                .isViewBlocking((s, l, p) -> false)
                .pushReaction(PushReaction.BLOCK);
    }

    private static DeferredBlock<ComputerBlock> computer(String name, DeviceModel model) {
        DeferredBlock<ComputerBlock> b = BLOCKS.registerBlock(name, p -> new ComputerBlock(p, model),
                deviceProps(SoundType.METAL, 1.5f).lightLevel(s -> DeviceBlock.isPowered(s) ? 2 : 0));
        COMPUTERS.add(b);
        return b;
    }

    private static DeferredBlock<PoweredPeripheralBlock> monitor(String name, DeviceModel model) {
        DeferredBlock<PoweredPeripheralBlock> b = BLOCKS.registerBlock(name, p -> new PoweredPeripheralBlock(p, model, PeripheralKind.MONITOR),
                deviceProps(SoundType.METAL, 1.0f).lightLevel(s -> DeviceBlock.isPowered(s) ? 6 : 0));
        PERIPHERALS.add(b);
        MONITORS.add(b);
        return b;
    }

    private static DeferredBlock<PeripheralBlock> input(String name, DeviceModel model, PeripheralKind kind) {
        DeferredBlock<PeripheralBlock> b = BLOCKS.registerBlock(name, p -> new PeripheralBlock(p, model, kind),
                deviceProps(SoundType.STONE, 0.5f));
        PERIPHERALS.add(b);
        return b;
    }

    private static DeferredBlock<PoweredPeripheralBlock> speaker(String name, DeviceModel model) {
        DeferredBlock<PoweredPeripheralBlock> b = BLOCKS.registerBlock(name, p -> new PoweredPeripheralBlock(p, model, PeripheralKind.SPEAKER),
                deviceProps(SoundType.WOOD, 1.0f).mapColor(MapColor.COLOR_BLACK));
        PERIPHERALS.add(b);
        return b;
    }

    public static Block[] computerBlocks() {
        return COMPUTERS.stream().map(DeferredBlock::get).toArray(Block[]::new);
    }

    public static Block[] monitorBlocks() {
        return MONITORS.stream().map(DeferredBlock::get).toArray(Block[]::new);
    }

    public static Block[] otherPeripheralBlocks() {
        return PERIPHERALS.stream().filter(b -> !MONITORS.contains(b)).map(DeferredBlock::get).toArray(Block[]::new);
    }
}
