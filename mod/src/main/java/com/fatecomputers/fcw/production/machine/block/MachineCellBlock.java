package com.fatecomputers.fcw.production.machine.block;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineState;
import com.fatecomputers.fcw.production.machine.MachineStructure;
import com.fatecomputers.fcw.production.machine.ProductionMachines;
import net.minecraft.ChatFormatting;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DirectionProperty;
import net.minecraft.world.level.block.state.properties.EnumProperty;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

import javax.annotation.Nullable;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * One cell of a multiblock machine. The block state alone says which cell it is, so only the terminal
 * cell carries a block entity (the controller); every other cell finds it by arithmetic.
 *
 * <p>Breaking: only while sneaking (so that a stray click does not take a machine apart); the whole
 * machine then comes apart, see {@link MachineStructure#onCellRemoved}.
 */
public abstract class MachineCellBlock extends Block implements EntityBlock {
    public static final DirectionProperty FACING = BlockStateProperties.HORIZONTAL_FACING;
    public static final EnumProperty<MachineState> STATE = EnumProperty.create("state", MachineState.class);

    /** The definition for the block being constructed: the state definition is built inside Block's constructor. */
    protected static final ThreadLocal<MachineDef> PENDING = new ThreadLocal<>();
    /** Set while a creative player breaks a cell: the machine then leaves no kit behind. */
    private static final ThreadLocal<Boolean> CREATIVE_BREAK = ThreadLocal.withInitial(() -> false);

    protected final MachineDef def;
    private final Map<BlockState, VoxelShape> shapes = new ConcurrentHashMap<>();

    protected MachineCellBlock(BlockBehaviour.Properties properties, MachineDef def) {
        super(properties);
        this.def = def;
    }

    public MachineDef def() {
        return def;
    }

    /** The cell this state stands for; null for the unused x/y/z combinations of the part block. */
    @Nullable
    public abstract MachineDef.Cell cell(BlockState state);

    public BlockPos origin(BlockPos pos, BlockState state) {
        MachineDef.Cell c = cell(state);
        return c == null ? pos : MachineDef.origin(pos, state.getValue(FACING), c.x(), c.y(), c.z());
    }

    public boolean isTerminal(BlockState state) {
        MachineDef.Cell c = cell(state);
        return c != null && c.x() == def.terminal.x() && c.y() == def.terminal.y() && c.z() == def.terminal.z();
    }

    @Nullable
    public MachineControllerBlockEntity controller(BlockGetter level, BlockPos pos, BlockState state) {
        if (cell(state) == null) {
            return null;
        }
        BlockPos at = def.controllerPos(origin(pos, state), state.getValue(FACING));
        if (level instanceof Level l && !l.isLoaded(at)) {
            return null;
        }
        return level.getBlockEntity(at) instanceof MachineControllerBlockEntity be && be.def() == def ? be : null;
    }

    // ───────────────────────── shape

    private VoxelShape shape(BlockState state) {
        return shapes.computeIfAbsent(state, s -> {
            MachineDef.Cell c = cell(s);
            if (c == null) {
                return Shapes.block();
            }
            double x0 = clamp(c.bboxMin()[0]), y0 = clamp(c.bboxMin()[1]), z0 = clamp(c.bboxMin()[2]);
            double x1 = clamp(c.bboxMax()[0]), y1 = clamp(c.bboxMax()[1]), z1 = clamp(c.bboxMax()[2]);
            if (x1 - x0 < 1 || y1 - y0 < 1 || z1 - z0 < 1) {
                // geometry that only pokes in from a neighbour: keep something to click on
                x0 = Math.min(x0, 15);
                y0 = Math.min(y0, 15);
                z0 = Math.min(z0, 15);
                x1 = Math.max(x1, x0 + 1);
                y1 = Math.max(y1, y0 + 1);
                z1 = Math.max(z1, z0 + 1);
            }
            return switch (s.getValue(FACING)) {
                case EAST -> Block.box(16 - z1, y0, x0, 16 - z0, y1, x1);
                case SOUTH -> Block.box(16 - x1, y0, 16 - z1, 16 - x0, y1, 16 - z0);
                case WEST -> Block.box(z0, y0, 16 - x1, z1, y1, 16 - x0);
                default -> Block.box(x0, y0, z0, x1, y1, z1);
            };
        });
    }

    private static double clamp(double v) {
        return Math.max(0, Math.min(16, v));
    }

    @Override
    protected VoxelShape getShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        return shape(state);
    }

    @Override
    protected RenderShape getRenderShape(BlockState state) {
        return RenderShape.MODEL;
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
    protected boolean isPathfindable(BlockState state, PathComputationType type) {
        return false;
    }

    // ───────────────────────── block entity

    @Nullable
    @Override
    public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
        return isTerminal(state) ? new MachineControllerBlockEntity(pos, state) : null;
    }

    @Nullable
    @Override
    public <T extends BlockEntity> BlockEntityTicker<T> getTicker(Level level, BlockState state, BlockEntityType<T> type) {
        if (type != ProductionMachines.CONTROLLER.get() || !isTerminal(state)) {
            return null;
        }
        return level.isClientSide
                ? (l, p, s, be) -> ((MachineControllerBlockEntity) be).clientTick()
                : (l, p, s, be) -> ((MachineControllerBlockEntity) be).serverTick();
    }

    // ───────────────────────── use

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        MachineDef.Cell c = cell(state);
        if (c == null) {
            return InteractionResult.PASS;
        }
        boolean terminal = isTerminal(state) || (c.hatch() == null && def.opensTerminal(c));
        if (!terminal && c.hatch() == null) {
            return InteractionResult.PASS;
        }
        if (level.isClientSide) {
            return InteractionResult.SUCCESS;
        }
        MachineControllerBlockEntity be = controller(level, pos, state);
        if (be == null) {
            return InteractionResult.FAIL;
        }
        if (terminal) {
            if (player instanceof ServerPlayer sp) {
                sp.openMenu(be, buf -> {
                    buf.writeBlockPos(be.getBlockPos());
                    buf.writeUtf(def.name);
                });
            }
        } else {
            be.toggleHatch(c.hatch());
        }
        return InteractionResult.CONSUME;
    }

    // ───────────────────────── breaking

    @Override
    protected float getDestroyProgress(BlockState state, Player player, BlockGetter level, BlockPos pos) {
        return player.isShiftKeyDown() ? super.getDestroyProgress(state, player, level, pos) : 0.0f;
    }

    @Override
    protected void attack(BlockState state, Level level, BlockPos pos, Player player) {
        if (!level.isClientSide && !player.isShiftKeyDown()) {
            player.displayClientMessage(Component.translatable("message.fcw.machine.sneak_to_break").withStyle(ChatFormatting.YELLOW), true);
        }
    }

    @Override
    public boolean onDestroyedByPlayer(BlockState state, Level level, BlockPos pos, Player player, boolean willHarvest, FluidState fluid) {
        if (!player.isShiftKeyDown()) {
            return false; // creative players break instantly: same rule for them
        }
        CREATIVE_BREAK.set(player.isCreative());
        try {
            return super.onDestroyedByPlayer(state, level, pos, player, willHarvest, fluid);
        } finally {
            CREATIVE_BREAK.set(false);
        }
    }

    @Override
    protected void onRemove(BlockState state, Level level, BlockPos pos, BlockState newState, boolean movedByPiston) {
        if (!state.is(newState.getBlock())) {
            if (!level.isClientSide && cell(state) != null) {
                MachineStructure.onCellRemoved(level, pos, state, this, CREATIVE_BREAK.get());
            }
            super.onRemove(state, level, pos, newState, movedByPiston);
        }
    }

    @Override
    public ItemStack getCloneItemStack(BlockState state, HitResult target, LevelReader level, BlockPos pos, Player player) {
        return ProductionMachines.sourceItem(def);
    }

    protected static BlockState withFacing(BlockState state, Direction facing) {
        return state.setValue(FACING, facing);
    }
}
