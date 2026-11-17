package com.fatecomputers.fcw.production.machine.construction;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.Machines;
import com.fatecomputers.fcw.production.machine.block.MachineCellBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.ItemInteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DirectionProperty;
import net.minecraft.world.level.block.state.properties.IntegerProperty;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.List;

/**
 * Scaffolding of a construction site: stands in every cell of the future machine until the cell is built.
 * Like the cells of a machine, its state says which cell of which machine it is (x, y, z, facing and the
 * machine's number), so it finds the site's controller by arithmetic and needs no block entity. It can be
 * climbed and walked through; components may be delivered by clicking any of it. It cannot be broken: a
 * site is removed with the wrench or "cancel".
 */
public class ConstructionScaffoldBlock extends Block {
    public static final IntegerProperty X = IntegerProperty.create("x", 0, 15);
    public static final IntegerProperty Y = IntegerProperty.create("y", 0, 7);
    public static final IntegerProperty Z = IntegerProperty.create("z", 0, 7);
    public static final IntegerProperty MACHINE = IntegerProperty.create("machine", 0, 7);
    public static final DirectionProperty FACING = BlockStateProperties.HORIZONTAL_FACING;
    private static final List<MachineDef> KINDS = new ArrayList<>(Machines.ALL.values());

    public ConstructionScaffoldBlock(BlockBehaviour.Properties properties) {
        super(properties);
        registerDefaultState(stateDefinition.any().setValue(FACING, Direction.NORTH));
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(X, Y, Z, MACHINE, FACING);
    }

    public BlockState stateFor(MachineDef def, MachineDef.Cell cell, Direction facing) {
        return defaultBlockState().setValue(X, cell.x()).setValue(Y, cell.y()).setValue(Z, cell.z())
                .setValue(MACHINE, KINDS.indexOf(def)).setValue(FACING, facing);
    }

    @Nullable
    public static MachineDef def(BlockState state) {
        int i = state.getValue(MACHINE);
        return i < KINDS.size() ? KINDS.get(i) : null;
    }

    /** The controller of the site this scaffolding belongs to. */
    @Nullable
    public static MachineControllerBlockEntity controller(BlockGetter level, BlockPos pos, BlockState state) {
        MachineDef def = def(state);
        if (def == null) {
            return null;
        }
        Direction facing = state.getValue(FACING);
        BlockPos origin = MachineDef.origin(pos, facing, state.getValue(X), state.getValue(Y), state.getValue(Z));
        BlockPos at = def.controllerPos(origin, facing);
        if (level instanceof Level l && !l.isLoaded(at)) {
            return null;
        }
        return level.getBlockEntity(at) instanceof MachineControllerBlockEntity be && be.def() == def && be.site() != null ? be : null;
    }

    @Override
    protected ItemInteractionResult useItemOn(ItemStack stack, BlockState state, Level level, BlockPos pos, Player player,
                                              InteractionHand hand, BlockHitResult hit) {
        if (stack.isEmpty() || hand != InteractionHand.MAIN_HAND) {
            return ItemInteractionResult.PASS_TO_DEFAULT_BLOCK_INTERACTION;
        }
        if (level.isClientSide) {
            return ItemInteractionResult.SUCCESS;
        }
        MachineControllerBlockEntity be = controller(level, pos, state);
        if (be == null) {
            return ItemInteractionResult.FAIL;
        }
        MachineCellBlock.useOnSite(be, level, pos, player, stack);
        return ItemInteractionResult.CONSUME;
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        return InteractionResult.PASS;
    }

    @Override
    protected VoxelShape getCollisionShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        return Shapes.empty();
    }

    @Override
    protected float getShadeBrightness(BlockState state, BlockGetter level, BlockPos pos) {
        return 1.0f;
    }

    @Override
    protected boolean propagatesSkylightDown(BlockState state, BlockGetter level, BlockPos pos) {
        return true;
    }

    @Override
    protected float getDestroyProgress(BlockState state, Player player, BlockGetter level, BlockPos pos) {
        return 0.0f;
    }

    @Override
    public boolean onDestroyedByPlayer(BlockState state, Level level, BlockPos pos, Player player, boolean willHarvest, FluidState fluid) {
        // only an abandoned piece (its site is gone) may be cleared, by a creative player
        return player.isCreative() && controller(level, pos, state) == null && super.onDestroyedByPlayer(state, level, pos, player, willHarvest, fluid);
    }
}
