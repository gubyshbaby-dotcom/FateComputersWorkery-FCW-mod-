package com.fatecomputers.fcw.block;

import com.mojang.serialization.MapCodec;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;

/** Monitor or speaker: powered follows the linked computer (the screen/LEDs light up). */
public class PoweredPeripheralBlock extends PeripheralBlock {
    private final MapCodec<PoweredPeripheralBlock> codec;

    public PoweredPeripheralBlock(Properties properties, DeviceModel model, PeripheralKind kind) {
        super(properties, model, kind);
        this.codec = simpleCodec(p -> new PoweredPeripheralBlock(p, model, kind));
    }

    @Override
    protected MapCodec<? extends Block> codec() {
        return codec;
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(FACING, POWERED);
    }
}
