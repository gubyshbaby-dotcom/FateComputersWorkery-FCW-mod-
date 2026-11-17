package com.fatecomputers.fcw.production.machine.block;

import com.fatecomputers.fcw.production.machine.MachineControllerBlockEntity;
import com.fatecomputers.fcw.production.machine.MachineDef;
import com.fatecomputers.fcw.production.machine.MachineSpec;
import com.fatecomputers.fcw.production.machine.construction.ConstructionSite;
import com.fatecomputers.fcw.production.machine.console.MachineConsoleNet;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.ItemInteractionResult;
import java.util.List;
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

    /**
     * Right click with an item on a loading zone puts the stack into the machine. An item that does not
     * belong there is refused with a dull click and a line above the hotbar; on a hatch cell that is not
     * an attempt to load at all, the click falls through to the hatch.
     */
    @Override
    protected ItemInteractionResult useItemOn(ItemStack stack, BlockState state, Level level, BlockPos pos, Player player,
                                              InteractionHand hand, BlockHitResult hit) {
        MachineDef.Cell c = cell(state);
        if (stack.isEmpty() || c == null || hand != InteractionHand.MAIN_HAND) {
            return ItemInteractionResult.PASS_TO_DEFAULT_BLOCK_INTERACTION;
        }
        MachineControllerBlockEntity machine = controller(level, pos, state);
        if (machine != null && machine.site() != null) {
            // a construction site: any of its cells takes components, the wrench cancels or resumes
            if (!level.isClientSide) {
                useOnSite(machine, level, pos, player, stack);
            }
            return ItemInteractionResult.sidedSuccess(level.isClientSide);
        }
        if (machine != null && stack.is(ProductionMachines.WRENCH.get()) && player.isShiftKeyDown() && isTerminal(state)) {
            if (!level.isClientSide && player instanceof ServerPlayer sp) {
                wrenchMachine(machine, sp);
            }
            return ItemInteractionResult.sidedSuccess(level.isClientSide);
        }
        if (machine != null && stack.getCapability(net.neoforged.neoforge.capabilities.Capabilities.FluidHandler.ITEM) != null) {
            // a bucket (or any fluid container) on a fluid port: water for cooling and rinsing goes in by hand too
            for (MachineDef.Port p : def.ports) {
                if (p.kind().equals("fluid") && p.ax() == c.x() && p.ay() == c.y() && p.az() == c.z() && machine.tank(p.id()) != null) {
                    if (!level.isClientSide) {
                        net.neoforged.neoforge.fluids.FluidUtil.interactWithFluidHandler(player, hand, machine.tank(p.id()));
                    }
                    return ItemInteractionResult.sidedSuccess(level.isClientSide);
                }
            }
        }
        MachineSpec.Zone zone = def.zoneAt(c);
        if (zone == null) {
            return ItemInteractionResult.PASS_TO_DEFAULT_BLOCK_INTERACTION;
        }
        boolean fits = MachineControllerBlockEntity.zoneAccepts(level, def, zone, stack);
        if (!fits && c.hatch() != null) {
            return ItemInteractionResult.PASS_TO_DEFAULT_BLOCK_INTERACTION;
        }
        if (level.isClientSide) {
            return ItemInteractionResult.SUCCESS;
        }
        MachineControllerBlockEntity be = controller(level, pos, state);
        if (be == null) {
            return ItemInteractionResult.FAIL;
        }
        MachineControllerBlockEntity.Reject reject = be.handInsert(zone, stack);
        if (reject == null) {
            level.playSound(null, pos, SoundEvents.IRON_TRAPDOOR_CLOSE, SoundSource.BLOCKS, 0.4f, 1.3f);
            if (zone.role() == MachineSpec.Role.CATALYST) {
                level.playSound(null, pos, SoundEvents.IRON_TRAPDOOR_CLOSE, SoundSource.BLOCKS, 0.4f, 1.6f);
            }
        } else {
            refuse(level, pos, player, zone, reject);
        }
        return ItemInteractionResult.CONSUME;
    }

    /** Shift + right click with the wrench on the terminal of a machine that is switched off: asked twice, then it comes apart. */
    private static void wrenchMachine(MachineControllerBlockEntity machine, ServerPlayer player) {
        Component name = Component.translatable("machine.fcw." + machine.def().name);
        if (machine.isEnabled() || machine.isCommissioning()) {
            MachineConsoleNet.notice(player, Component.translatable("interact.fcw.wrench.switch_off", name));
        } else if (!machine.confirmWrench()) {
            MachineConsoleNet.notice(player, Component.translatable("interact.fcw.wrench.confirm", name));
        } else if (machine.beginTeardown(player.getGameProfile().getName())) {
            player.level().playSound(null, machine.getBlockPos(), SoundEvents.ANVIL_LAND, SoundSource.BLOCKS, 0.4f, 1.4f);
        }
    }

    /** An item in the hand on a construction site (pallet, scaffolding or a cell already built). */
    public static void useOnSite(MachineControllerBlockEntity machine, Level level, BlockPos pos, Player player, ItemStack stack) {
        ConstructionSite site = machine.site();
        String who = player.getGameProfile().getName();
        ServerPlayer sp = player instanceof ServerPlayer p ? p : null;
        if (stack.is(ProductionMachines.WRENCH.get())) {
            if (site.mode() == ConstructionSite.Mode.TEARDOWN) {
                site.resumeBuild(who);
            } else if (machine.confirmWrench()) {
                site.beginTeardown(who);
            } else if (sp != null) {
                MachineConsoleNet.notice(sp, Component.translatable("interact.fcw.wrench.confirm_site"));
            }
            return;
        }
        int taken = site.deliver(stack, false);
        if (taken > 0) {
            stack.shrink(taken);
            machine.siteChanged();
            level.playSound(null, pos, SoundEvents.IRON_TRAPDOOR_CLOSE, SoundSource.BLOCKS, 0.4f, 1.3f);
        } else {
            level.playSound(null, pos, SoundEvents.DISPENSER_FAIL, SoundSource.BLOCKS, 0.6f, 1.0f);
            if (sp != null) {
                MachineConsoleNet.notice(sp, Component.translatable(site.mode() == ConstructionSite.Mode.TEARDOWN
                        ? "interact.fcw.site.teardown" : "interact.fcw.site.not_needed", stack.getHoverName()));
            }
        }
    }

    private void refuse(Level level, BlockPos pos, Player player, MachineSpec.Zone zone, MachineControllerBlockEntity.Reject reject) {
        level.playSound(null, pos, SoundEvents.DISPENSER_FAIL, SoundSource.BLOCKS, 0.6f, 1.0f);
        if (player instanceof ServerPlayer sp) {
            Component name = Component.translatable("interact.fcw." + def.name + "." + zone.id());
            Component text = switch (reject) {
                case NOT_ACCEPTED -> Component.translatable("interact.fcw.reject.not_accepted", name,
                        Component.translatable("interact.fcw." + def.name + "." + zone.id() + ".accepts"));
                case FULL -> Component.translatable("interact.fcw.reject.full", name);
                case BUSY -> Component.translatable("interact.fcw.reject.busy", name);
                case OUTPUT_ONLY -> Component.translatable("interact.fcw.reject.output_only", name);
            };
            MachineConsoleNet.notice(sp, text);
        }
    }

    /**
     * Empty hand (or an item that is of no use here): a zone gives one stack back (everything when
     * sneaking), the terminal opens the console, a hatch opens or closes, the breaker switches the machine.
     */
    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        MachineDef.Cell c = cell(state);
        if (c == null) {
            return InteractionResult.PASS;
        }
        MachineSpec.Zone zone = def.zoneAt(c);
        boolean terminal = isTerminal(state) || (c.hatch() == null && zone == null && def.opensTerminal(c));
        if (!terminal && c.hatch() == null && zone == null) {
            return InteractionResult.PASS;
        }
        if (level.isClientSide) {
            return InteractionResult.SUCCESS;
        }
        MachineControllerBlockEntity be = controller(level, pos, state);
        if (be == null) {
            return InteractionResult.FAIL;
        }
        boolean emptyHand = player.getMainHandItem().isEmpty();
        boolean sneaking = player.isShiftKeyDown();
        if (be.site() != null) {
            if (!isTerminal(state)) {
                return InteractionResult.PASS;
            }
            // the pallet: an empty hand opens the site's console; sneaking, it takes back the stack delivered last
            ItemStack back = emptyHand && sneaking ? be.site().takeBack() : ItemStack.EMPTY;
            if (!back.isEmpty()) {
                be.siteChanged();
                if (!player.getInventory().add(back)) {
                    player.drop(back, false);
                }
                level.playSound(null, pos, SoundEvents.ITEM_PICKUP, SoundSource.PLAYERS, 0.3f, 1.2f);
            } else if (player instanceof ServerPlayer sp) {
                MachineConsoleNet.open(sp, be);
            }
            return InteractionResult.CONSUME;
        }
        if (zone != null && !terminal && emptyHand && (zone.handTakes() || sneaking)) {
            if (be.handCanTake(zone)) {
                List<ItemStack> taken = be.handTake(zone, sneaking);
                for (ItemStack s : taken) {
                    if (!player.getInventory().add(s)) {
                        player.drop(s, false);
                    }
                }
                level.playSound(null, pos, SoundEvents.ITEM_PICKUP, SoundSource.PLAYERS, 0.3f, 1.2f);
                return InteractionResult.CONSUME;
            }
            if (zone.role() == MachineSpec.Role.CATALYST && !be.zoneContents(zone).isEmpty()) {
                refuse(level, pos, player, zone, MachineControllerBlockEntity.Reject.BUSY);
                return InteractionResult.CONSUME;
            }
        }
        if (terminal) {
            if (player instanceof ServerPlayer sp) {
                MachineConsoleNet.open(sp, be);
            }
        } else if (c.hatch() != null) {
            be.toggleHatch(c.hatch(), player.getGameProfile().getName());
            if (c.hatch().equals(MachineDef.BREAKER)) {
                level.playSound(null, pos, SoundEvents.LEVER_CLICK, SoundSource.BLOCKS, 0.5f, be.isEnabled() ? 0.6f : 0.5f);
                level.playSound(null, pos, SoundEvents.PISTON_CONTRACT, SoundSource.BLOCKS, 0.25f, 1.6f);
            }
        }
        return InteractionResult.CONSUME;
    }

    // ───────────────────────── breaking

    @Override
    protected float getDestroyProgress(BlockState state, Player player, BlockGetter level, BlockPos pos) {
        return 0.0f; // machines are taken apart with the wrench, not mined
    }

    @Override
    protected void attack(BlockState state, Level level, BlockPos pos, Player player) {
        if (!level.isClientSide && !player.isShiftKeyDown()) {
            player.displayClientMessage(Component.translatable("message.fcw.machine.use_wrench").withStyle(ChatFormatting.YELLOW), true);
        }
    }

    @Override
    public boolean onDestroyedByPlayer(BlockState state, Level level, BlockPos pos, Player player, boolean willHarvest, FluidState fluid) {
        if (!player.isShiftKeyDown() || !player.isCreative()) {
            return false; // only a sneaking creative player (a map maker) removes a machine by hand
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
