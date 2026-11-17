package com.fatecomputers.fcw.block.entity;

import com.fatecomputers.fcw.SidedHooks;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.PeripheralBlock;
import com.fatecomputers.fcw.block.PeripheralKind;
import com.fatecomputers.fcw.registry.ModBlockEntities;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientGamePacketListener;
import net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;

import javax.annotation.Nullable;
import java.util.UUID;

/**
 * Link of a monitor/keyboard/mouse/speaker to a computer: the computer position plus its VM id
 * (so that a different computer placed at the same spot is not mistaken for the old one).
 * Synced to clients: monitors need the VM id to show the live screen.
 */
public class PeripheralBlockEntity extends BlockEntity {
    @Nullable
    private BlockPos computerPos;
    @Nullable
    private UUID vmId;

    public PeripheralBlockEntity(BlockPos pos, BlockState state) {
        this(ModBlockEntities.PERIPHERAL.get(), pos, state);
    }

    protected PeripheralBlockEntity(BlockEntityType<?> type, BlockPos pos, BlockState state) {
        super(type, pos, state);
    }

    public PeripheralKind kind() {
        return getBlockState().getBlock() instanceof PeripheralBlock pb ? pb.kind() : PeripheralKind.MONITOR;
    }

    public boolean isLinked() {
        return computerPos != null && vmId != null;
    }

    @Nullable
    public BlockPos computerPos() {
        return computerPos;
    }

    @Nullable
    public UUID vmId() {
        return vmId;
    }

    public boolean isLinkedTo(ComputerBlockEntity computer) {
        return computer.getBlockPos().equals(computerPos) && computer.vmId().equals(vmId);
    }

    /** The linked computer if its chunk is loaded and it is still the same machine. */
    @Nullable
    public ComputerBlockEntity computer() {
        if (level == null || computerPos == null || vmId == null || !level.isLoaded(computerPos)) {
            return null;
        }
        if (level.getBlockEntity(computerPos) instanceof ComputerBlockEntity c && vmId.equals(c.vmId())) {
            return c;
        }
        return null;
    }

    /** Server side: link to a computer (and unlink from the previous one). */
    public void linkTo(ComputerBlockEntity computer) {
        ComputerBlockEntity old = computer();
        if (old != null && old != computer) {
            old.removePeripheral(worldPosition);
        }
        computerPos = computer.getBlockPos().immutable();
        vmId = computer.vmId();
        computer.addPeripheral(worldPosition, kind());
        setPoweredState(computer.isMachineOn());
        changedAndSync();
    }

    /** Server side: forget the link (computer gone). */
    public void unlink() {
        computerPos = null;
        vmId = null;
        setPoweredState(false);
        changedAndSync();
    }

    /** Called by the block when it is broken. */
    public void onBroken() {
        ComputerBlockEntity c = computer();
        if (c != null) {
            c.removePeripheral(worldPosition);
        }
    }

    public void setPoweredState(boolean on) {
        if (level == null || level.isClientSide) {
            return;
        }
        BlockState state = getBlockState();
        if (state.hasProperty(DeviceBlock.POWERED) && state.getValue(DeviceBlock.POWERED) != on) {
            level.setBlock(worldPosition, state.setValue(DeviceBlock.POWERED, on), Block.UPDATE_ALL);
        }
    }

    protected void changedAndSync() {
        setChanged();
        if (level != null && !level.isClientSide) {
            BlockState s = getBlockState();
            level.sendBlockUpdated(worldPosition, s, s, Block.UPDATE_CLIENTS);
        }
    }

    @Override
    public void onLoad() {
        super.onLoad();
        if (level != null && level.isClientSide && kind() == PeripheralKind.SPEAKER) {
            SidedHooks.client().speakerLoaded(this);
        }
    }

    @Override
    public void setRemoved() {
        super.setRemoved();
        if (level != null && level.isClientSide && kind() == PeripheralKind.SPEAKER) {
            SidedHooks.client().speakerUnloaded(this);
        }
    }

    @Override
    public void onChunkUnloaded() {
        super.onChunkUnloaded();
        if (level != null && level.isClientSide && kind() == PeripheralKind.SPEAKER) {
            SidedHooks.client().speakerUnloaded(this);
        }
    }

    @Override
    protected void saveAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.saveAdditional(tag, registries);
        writeLink(tag);
    }

    @Override
    protected void loadAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.loadAdditional(tag, registries);
        readLink(tag);
    }

    private void writeLink(CompoundTag tag) {
        if (computerPos != null && vmId != null) {
            tag.put("Computer", NbtUtils.writeBlockPos(computerPos));
            tag.putUUID("VmId", vmId);
        }
    }

    private void readLink(CompoundTag tag) {
        if (tag.contains("VmId")) {
            computerPos = NbtUtils.readBlockPos(tag, "Computer").orElse(null);
            vmId = tag.getUUID("VmId");
        } else {
            computerPos = null;
            vmId = null;
        }
    }

    @Override
    public CompoundTag getUpdateTag(HolderLookup.Provider registries) {
        CompoundTag tag = super.getUpdateTag(registries);
        writeLink(tag);
        return tag;
    }

    @Override
    public void handleUpdateTag(CompoundTag tag, HolderLookup.Provider registries) {
        readLink(tag);
    }

    @Nullable
    @Override
    public Packet<ClientGamePacketListener> getUpdatePacket() {
        return ClientboundBlockEntityDataPacket.create(this);
    }
}
