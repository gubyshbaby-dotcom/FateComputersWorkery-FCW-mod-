package com.fatecomputers.fcw.block.entity;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.DeviceBlock;
import com.fatecomputers.fcw.block.PeripheralKind;
import com.fatecomputers.fcw.menu.ComputerMenu;
import com.fatecomputers.fcw.proto.ControlProto.AgentSend;
import com.fatecomputers.fcw.proto.ControlProto.CreateVm;
import com.fatecomputers.fcw.proto.ControlProto.DeleteVm;
import com.fatecomputers.fcw.proto.ControlProto.GetVm;
import com.fatecomputers.fcw.proto.ControlProto.ImageInfo;
import com.fatecomputers.fcw.proto.ControlProto.PauseVm;
import com.fatecomputers.fcw.proto.ControlProto.Request;
import com.fatecomputers.fcw.proto.ControlProto.ResumeVm;
import com.fatecomputers.fcw.proto.ControlProto.RevokeTokens;
import com.fatecomputers.fcw.proto.ControlProto.SetRedstoneInputs;
import com.fatecomputers.fcw.proto.ControlProto.StartVm;
import com.fatecomputers.fcw.proto.ControlProto.StopVm;
import com.fatecomputers.fcw.proto.ControlProto.VmInfo;
import com.fatecomputers.fcw.proto.ControlProto.VmState;
import com.fatecomputers.fcw.registry.ModBlockEntities;
import com.fatecomputers.fcw.server.ComputerTracker;
import com.fatecomputers.fcw.server.FcwServerEvents;
import com.fatecomputers.fcw.server.daemon.DaemonManager;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponents;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Containers;
import net.minecraft.world.MenuProvider;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;

import javax.annotation.Nullable;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.stream.Stream;

/**
 * A computer. Owns the VM id (UUID, also the daemon's machine id), the installed hardware, the
 * chosen OS image, the links to peripherals and the redstone I/O. All daemon interaction happens
 * on the server thread through {@link DaemonManager}.
 */
public class ComputerBlockEntity extends BlockEntity implements MenuProvider {
    /** Relative sides in the order of SetRedstoneInputs / RedstoneOutput. */
    public static final String[] SIDES = {"front", "back", "left", "right", "top", "bottom"};

    private final ComputerInventory inventory = new ComputerInventory(this::onInventoryChanged);
    @Nullable
    private UUID vmId;
    @Nullable
    private UUID owner;
    private String imageId = "";
    private String installedImage = "";
    private boolean wantRunning;
    private int vmState = VmState.VM_STATE_UNKNOWN_VALUE;
    private String lastError = "";
    @Nullable
    private Component lastErrorComponent;
    private String controller = "";
    private String agentInfo = "";
    private long uptimeMs;
    private long uptimeAt;
    private boolean agentConnected;
    @Nullable
    private Component customName;
    private String netHostname = "";
    private String ipAddress = "";
    private String imageVersion = "";
    private boolean imageUpdateAvailable;
    private final int[] redstoneIn = new int[6];
    private final int[] redstoneOut = new int[6];
    private final Map<BlockPos, PeripheralKind> peripherals = new HashMap<>();
    private boolean busy;
    private boolean pausedByUnload;
    private int statusVersion;

    public ComputerBlockEntity(BlockPos pos, BlockState state) {
        super(ModBlockEntities.COMPUTER.get(), pos, state);
    }

    // ------------------------------------------------------------------ identity

    public UUID vmId() {
        if (vmId == null) {
            vmId = UUID.randomUUID();
            setChanged();
        }
        return vmId;
    }

    public String vmIdString() {
        return vmId().toString();
    }

    public void setOwner(UUID owner) {
        this.owner = owner;
        setChanged();
    }

    @Nullable
    public UUID owner() {
        return owner;
    }

    public ComputerInventory inventory() {
        return inventory;
    }

    // ------------------------------------------------------------------ lifecycle

    @Override
    public void onLoad() {
        super.onLoad();
        if (level == null || level.isClientSide) {
            return;
        }
        ComputerTracker.add(this);
        vmState = VmState.VM_STATE_UNKNOWN_VALUE;
        if (DaemonManager.isReady()) {
            syncWithDaemon();
        }
    }

    @Override
    public void onChunkUnloaded() {
        super.onChunkUnloaded();
        if (level == null || level.isClientSide) {
            return;
        }
        ComputerTracker.remove(this);
        DaemonManager d = DaemonManager.get();
        if (d != null && DaemonManager.isReady() && !FcwServerEvents.isStopping()
                && (vmState == VmState.VM_STATE_RUNNING_VALUE || vmState == VmState.VM_STATE_STARTING_VALUE)) {
            pausedByUnload = true;
            d.send(Request.newBuilder().setPauseVm(PauseVm.newBuilder().setVmId(vmIdString())).build());
        }
    }

    @Override
    public void setRemoved() {
        super.setRemoved();
        if (level != null && !level.isClientSide) {
            ComputerTracker.remove(this);
        }
    }

    /** The block was broken: drop the parts, stop the machine and delete its disk. */
    public void onBroken() {
        if (level == null) {
            return;
        }
        for (int i = 0; i < inventory.getSlots(); i++) {
            ItemStack s = inventory.getStackInSlot(i);
            if (!s.isEmpty()) {
                Containers.dropItemStack(level, worldPosition.getX() + 0.5, worldPosition.getY() + 0.5, worldPosition.getZ() + 0.5, s.copy());
                inventory.setStackInSlot(i, ItemStack.EMPTY);
            }
        }
        ComputerTracker.remove(this);
        for (BlockPos p : new ArrayList<>(peripherals.keySet())) {
            if (level.isLoaded(p) && level.getBlockEntity(p) instanceof PeripheralBlockEntity pbe && pbe.isLinkedTo(this)) {
                pbe.unlink();
            }
        }
        peripherals.clear();
        String id = vmIdString();
        DaemonManager d = DaemonManager.get();
        if (d != null && DaemonManager.isReady()) {
            d.send(Request.newBuilder().setRevokeTokens(RevokeTokens.newBuilder().setVmId(id)).build());
            d.request(Request.newBuilder().setStopVm(StopVm.newBuilder().setVmId(id).setForce(true)).build(), r ->
                    d.request(Request.newBuilder().setDeleteVm(DeleteVm.newBuilder().setVmId(id)).build(), r2 -> {
                        if (!r2.getOk()) {
                            FcwMod.LOGGER.warn("Could not delete VM {}: {}", id, r2.getError());
                        }
                    }));
        } else if (d != null) {
            // No daemon: no process can hold the disk, remove the machine directory directly.
            deleteRecursively(d.vmsDir().resolve(id));
        }
    }

    private static void deleteRecursively(Path dir) {
        if (!Files.isDirectory(dir)) {
            return;
        }
        try (Stream<Path> files = Files.walk(dir)) {
            files.sorted(Comparator.reverseOrder()).forEach(f -> {
                try {
                    Files.deleteIfExists(f);
                } catch (IOException ignored) {
                }
            });
        } catch (IOException e) {
            FcwMod.LOGGER.warn("Could not delete {}: {}", dir, e.toString());
        }
    }

    public void onDaemonReady() {
        syncWithDaemon();
    }

    public void onDaemonLost(Component reason) {
        busy = false;
        if (isMachineOn()) {
            lastErrorComponent = Component.translatable("gui.fcw.error.daemon_lost", reason);
            lastError = "";
        }
        applyState(VmState.VM_STATE_STOPPED_VALUE, false);
    }

    /** Ask the daemon what it knows about this machine and resume/restart it if it should run. */
    private void syncWithDaemon() {
        DaemonManager d = DaemonManager.get();
        if (d == null || isRemoved()) {
            return;
        }
        d.request(Request.newBuilder().setGetVm(GetVm.newBuilder().setVmId(vmIdString())).build(), r -> {
            if (isRemoved()) {
                return;
            }
            int state = VmState.VM_STATE_STOPPED_VALUE;
            if (r.getOk() && r.hasVm()) {
                applyVmInfo(r.getVm());
                state = r.getVm().getStateValue();
            } else {
                applyState(VmState.VM_STATE_STOPPED_VALUE, false);
            }
            if (!wantRunning) {
                if (isOnState(state)) {
                    wantRunning = true; // the daemon kept it running (e.g. only the mod was reloaded)
                }
                return;
            }
            if (state == VmState.VM_STATE_PAUSED_VALUE) {
                pausedByUnload = false;
                d.request(Request.newBuilder().setResumeVm(ResumeVm.newBuilder().setVmId(vmIdString())).build(), rr -> {
                    if (rr.getOk() && rr.hasVm()) {
                        applyVmInfo(rr.getVm());
                    } else if (rr.getOk()) {
                        applyState(VmState.VM_STATE_RUNNING_VALUE, true);
                    } else {
                        FcwMod.LOGGER.warn("ResumeVm {} failed: {}", vmIdString(), rr.getError());
                    }
                });
            } else if (!isOnState(state)) {
                pausedByUnload = false;
                if (d.qemuMissing()) {
                    // Keep wantRunning: started once the portable QEMU is there (the daemon resyncs us).
                    // The GUI shows the live QEMU state, so no error is stored here.
                    clearError();
                    bump();
                    return;
                }
                if (FcwConfig.SERVER.restartRunningOnLoad.get() && validate(null) == null) {
                    createAndStart(false, null);
                } else {
                    wantRunning = false;
                    applyState(VmState.VM_STATE_STOPPED_VALUE, false);
                }
            }
        });
    }

    // ------------------------------------------------------------------ hardware and validation

    public int effectiveCpus() {
        int cpus = inventory.cpuCores();
        DaemonManager d = DaemonManager.get();
        int host = d == null ? 0 : d.hostCpus();
        if (host > 0) {
            cpus = Math.min(cpus, host);
        }
        return Math.max(cpus, 0);
    }

    public int effectiveMemoryMb() {
        return Math.min(inventory.memoryMb(), FcwConfig.SERVER.maxMemoryMb.get());
    }

    public boolean hasPeripheral(PeripheralKind kind) {
        cleanPeripherals();
        return peripherals.containsValue(kind);
    }

    /** @return a reason why the machine cannot start, or null if it can */
    @Nullable
    public Component validate(@Nullable String forImage) {
        String image = forImage != null ? forImage : imageId;
        if (!inventory.hasMotherboard()) {
            return Component.translatable("gui.fcw.error.no_motherboard");
        }
        if (inventory.cpuCores() <= 0) {
            return Component.translatable("gui.fcw.error.no_cpu");
        }
        if (inventory.memoryMb() <= 0) {
            return Component.translatable("gui.fcw.error.no_ram");
        }
        DaemonManager d = DaemonManager.get();
        if (d == null || !DaemonManager.isReady()) {
            return d == null ? Component.translatable("daemon.fcw.stopped") : Component.translatable("gui.fcw.error.daemon", d.reason());
        }
        Component qemu = d.qemuProblem();
        if (qemu != null) {
            return qemu;
        }
        if (image.isEmpty()) {
            return Component.translatable("gui.fcw.error.no_image");
        }
        ImageInfo info = d.image(image);
        if (info == null) {
            return Component.translatable("gui.fcw.error.unknown_image", image);
        }
        if (!info.getInstalled()) {
            return Component.translatable("gui.fcw.error.image_not_installed", info.getName().isEmpty() ? image : info.getName(), image);
        }
        if (forImage == null && !installedImage.isEmpty() && !installedImage.equals(image)) {
            return Component.translatable("gui.fcw.error.needs_reinstall", installedImage, image);
        }
        int mem = effectiveMemoryMb();
        if (info.getMinMemoryMb() > 0 && mem < info.getMinMemoryMb()) {
            return Component.translatable("gui.fcw.error.not_enough_memory",
                    info.getName().isEmpty() ? image : info.getName(), info.getMinMemoryMb(), mem);
        }
        return null;
    }

    private static boolean networkSwitchUp() {
        DaemonManager d = DaemonManager.get();
        return d != null && d.networkEnabled();
    }

    private CreateVm buildCreate(boolean reinstall) {
        boolean gpu = inventory.hasGpu();
        return CreateVm.newBuilder()
                .setVmId(vmIdString())
                .setImageId(imageId)
                .setCpus(Math.max(1, effectiveCpus()))
                .setMemoryMb(effectiveMemoryMb())
                .setGpu(gpu)
                .setDiskGb(0)
                .setLang(FcwConfig.SERVER.guestLang.get())
                .setKbdLayout(FcwConfig.SERVER.guestKeyboardLayouts.get())
                .setSound(hasPeripheral(PeripheralKind.SPEAKER))
                .setNetwork(FcwConfig.SERVER.guestNetwork.get() && networkSwitchUp())
                .setReinstall(reinstall)
                .setDisplayWidth(gpu ? FcwConfig.SERVER.displayWidth.get() : 0)
                .setDisplayHeight(gpu ? FcwConfig.SERVER.displayHeight.get() : 0)
                .setHostname(hostname())
                .build();
    }

    // ------------------------------------------------------------------ actions (server thread)

    public void powerOn(@Nullable ServerPlayer actor) {
        if (busy || isMachineOn()) {
            return;
        }
        Component problem = validate(null);
        if (problem != null) {
            setError(problem);
            if (actor != null) {
                actor.displayClientMessage(problem, false);
            }
            return;
        }
        createAndStart(false, actor);
    }

    private void createAndStart(boolean reinstall, @Nullable ServerPlayer actor) {
        DaemonManager d = DaemonManager.get();
        if (d == null) {
            return;
        }
        busy = true;
        wantRunning = true;
        clearError();
        bump();
        String image = imageId;
        d.request(Request.newBuilder().setCreateVm(buildCreate(reinstall)).build(), r -> {
            if (isRemoved()) {
                return;
            }
            if (!r.getOk()) {
                busy = false;
                wantRunning = false;
                failWith(Component.translatable("gui.fcw.error.create_failed", r.getError()), actor);
                return;
            }
            installedImage = image;
            setChanged();
            d.request(Request.newBuilder().setStartVm(StartVm.newBuilder().setVmId(vmIdString())).build(), r2 -> {
                if (isRemoved()) {
                    return;
                }
                busy = false;
                if (!r2.getOk()) {
                    wantRunning = false;
                    failWith(Component.translatable("gui.fcw.error.start_failed", r2.getError()), actor);
                    return;
                }
                if (r2.hasVm()) {
                    applyVmInfo(r2.getVm());
                } else if (!isMachineOn()) {
                    applyState(VmState.VM_STATE_STARTING_VALUE, true);
                }
                bump();
            });
        });
    }

    public void powerOff(boolean force) {
        DaemonManager d = DaemonManager.get();
        wantRunning = false;
        setChanged();
        if (d == null || !DaemonManager.isReady() || !isMachineOn()) {
            bump();
            return;
        }
        int graceMs = force ? 0 : 60_000;
        // The daemon may answer only once the machine is down: wait longer than the ACPI grace period.
        d.request(Request.newBuilder().setStopVm(StopVm.newBuilder().setVmId(vmIdString()).setForce(force)
                .setTimeoutMs(graceMs)).build(), graceMs + 30_000L, r -> {
            if (isRemoved()) {
                return;
            }
            if (!r.getOk()) {
                setError(Component.translatable("gui.fcw.error.stop_failed", r.getError()));
            } else if (r.hasVm()) {
                applyVmInfo(r.getVm());
            } else if (vmState == VmState.VM_STATE_RUNNING_VALUE) {
                applyState(VmState.VM_STATE_STOPPING_VALUE, true);
            }
            bump();
        });
    }

    public void selectImage(String id, @Nullable ServerPlayer actor) {
        if (isMachineOn() || busy) {
            return;
        }
        imageId = id;
        clearError();
        setChanged();
        bump();
    }

    /** Recreates the disk from the chosen image (the GUI asks for confirmation first). */
    public void reinstall(String id, @Nullable ServerPlayer actor) {
        if (isMachineOn() || busy) {
            if (actor != null) {
                actor.displayClientMessage(Component.translatable("gui.fcw.error.stop_first"), false);
            }
            return;
        }
        String previous = imageId;
        imageId = id;
        Component problem = validate(id);
        if (problem != null) {
            imageId = previous;
            setError(problem);
            if (actor != null) {
                actor.displayClientMessage(problem, false);
            }
            return;
        }
        DaemonManager d = DaemonManager.get();
        if (d == null) {
            return;
        }
        busy = true;
        clearError();
        bump();
        d.request(Request.newBuilder().setCreateVm(buildCreate(true)).build(), 10 * 60_000L, r -> {
            if (isRemoved()) {
                return;
            }
            busy = false;
            if (!r.getOk()) {
                failWith(Component.translatable("gui.fcw.error.reinstall_failed", r.getError()), actor);
                return;
            }
            installedImage = id;
            setChanged();
            if (actor != null) {
                actor.displayClientMessage(Component.translatable("message.fcw.reinstalled", id), false);
            }
            bump();
        });
    }

    /** Refresh controller/uptime/agent info (while someone has the GUI open). */
    public void refreshInfo() {
        DaemonManager d = DaemonManager.get();
        if (d == null || !DaemonManager.isReady() || busy) {
            return;
        }
        d.request(Request.newBuilder().setGetVm(GetVm.newBuilder().setVmId(vmIdString())).build(), r -> {
            if (!isRemoved() && r.getOk() && r.hasVm()) {
                applyVmInfo(r.getVm());
            }
        });
    }

    // ------------------------------------------------------------------ state

    public static boolean isOnState(int state) {
        return state == VmState.VM_STATE_STARTING_VALUE || state == VmState.VM_STATE_RUNNING_VALUE
                || state == VmState.VM_STATE_PAUSED_VALUE || state == VmState.VM_STATE_STOPPING_VALUE;
    }

    public boolean isMachineOn() {
        return isOnState(vmState);
    }

    public boolean isBusy() {
        return busy;
    }

    public int vmState() {
        return vmState;
    }

    public void applyVmInfo(VmInfo info) {
        controller = info.getControllerPlayer();
        netHostname = info.getHostname();
        ipAddress = info.getIpAddress();
        imageVersion = info.getImageVersion();
        imageUpdateAvailable = info.getImageUpdateAvailable();
        uptimeMs = info.getUptimeMs();
        uptimeAt = System.currentTimeMillis();
        agentConnected = info.getAgentConnected();
        if (!info.getLastError().isEmpty() && info.getState() == VmState.VM_STATE_CRASHED) {
            lastError = info.getLastError();
            lastErrorComponent = null;
        }
        applyState(info.getStateValue(), true);
    }

    public void onVmStateEvent(int state, String reason) {
        if (state == VmState.VM_STATE_CRASHED_VALUE && !reason.isEmpty()) {
            lastError = reason;
            lastErrorComponent = null;
        }
        if (state == VmState.VM_STATE_STOPPED_VALUE || state == VmState.VM_STATE_CRASHED_VALUE) {
            // Shut down from inside the guest, crashed, or stopped by us: either way it should stay off.
            wantRunning = false;
            busy = false;
        }
        applyState(state, true);
    }

    private void applyState(int state, boolean fromDaemon) {
        int old = vmState;
        vmState = state;
        if (!isOnState(state)) {
            controller = "";
            ipAddress = "";
            agentConnected = false;
            uptimeMs = 0;
            if (Arrays.stream(redstoneOut).anyMatch(v -> v != 0)) {
                Arrays.fill(redstoneOut, 0);
                notifyRedstone();
            }
        }
        if (state == VmState.VM_STATE_RUNNING_VALUE && old != VmState.VM_STATE_RUNNING_VALUE && fromDaemon) {
            updateRedstoneInputs(true);
            sendAgentTime();
        }
        updatePowered();
        if (old != state) {
            setChanged();
        }
        bump();
    }

    private void updatePowered() {
        if (level == null || level.isClientSide || isRemoved()) {
            return;
        }
        boolean on = isMachineOn();
        BlockState s = getBlockState();
        if (s.hasProperty(DeviceBlock.POWERED) && s.getValue(DeviceBlock.POWERED) != on) {
            level.setBlock(worldPosition, s.setValue(DeviceBlock.POWERED, on), Block.UPDATE_ALL);
        }
        for (BlockPos p : new ArrayList<>(peripherals.keySet())) {
            if (level.isLoaded(p) && level.getBlockEntity(p) instanceof PeripheralBlockEntity pbe && pbe.isLinkedTo(this)) {
                pbe.setPoweredState(on);
            }
        }
    }

    private void setError(Component c) {
        lastErrorComponent = c;
        lastError = "";
        bump();
    }

    private void clearError() {
        lastErrorComponent = null;
        lastError = "";
    }

    private void failWith(Component c, @Nullable ServerPlayer actor) {
        setError(c);
        applyState(VmState.VM_STATE_STOPPED_VALUE, false);
        if (actor != null) {
            actor.displayClientMessage(c, false);
        }
    }

    public Component lastError() {
        if (lastErrorComponent != null) {
            return lastErrorComponent;
        }
        return lastError.isEmpty() ? Component.empty() : Component.literal(lastError);
    }

    public String controller() {
        return controller;
    }

    // ------------------------------------------------------------------ name and network

    @Nullable
    public Component customName() {
        return customName;
    }

    /**
     * Network name requested from the daemon: the anvil name of the computer reduced to
     * [a-z0-9-] (Cyrillic is transliterated), at most 63 characters. Empty = daemon default "pc-xxxxxxxx".
     */
    public String hostname() {
        return customName == null ? "" : sanitizeHostname(customName.getString());
    }

    /** Name in the guest network: what the daemon reported, otherwise what it will use. */
    public String effectiveHostname() {
        if (!netHostname.isEmpty()) {
            return netHostname;
        }
        String h = hostname();
        return h.isEmpty() ? "pc-" + vmIdString().substring(0, 8) : h;
    }

    public String ipAddress() {
        return ipAddress;
    }

    public String imageVersion() {
        return imageVersion;
    }

    public boolean imageUpdateAvailable() {
        return imageUpdateAvailable;
    }

    private static final String[] CYRILLIC = {
            "а", "a", "б", "b", "в", "v", "г", "g", "д", "d", "е", "e", "ё", "e", "ж", "zh", "з", "z", "и", "i",
            "й", "y", "к", "k", "л", "l", "м", "m", "н", "n", "о", "o", "п", "p", "р", "r", "с", "s", "т", "t",
            "у", "u", "ф", "f", "х", "h", "ц", "ts", "ч", "ch", "ш", "sh", "щ", "sch", "ъ", "", "ы", "y", "ь", "",
            "э", "e", "ю", "yu", "я", "ya", "і", "i", "ї", "yi", "є", "ye", "ґ", "g"};

    public static String sanitizeHostname(String name) {
        String lower = name.toLowerCase(java.util.Locale.ROOT);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < lower.length(); i++) {
            char c = lower.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
                sb.append(c);
                continue;
            }
            String repl = "-";
            for (int k = 0; k < CYRILLIC.length; k += 2) {
                if (CYRILLIC[k].charAt(0) == c) {
                    repl = CYRILLIC[k + 1];
                    break;
                }
            }
            sb.append(repl);
        }
        String h = sb.toString().replaceAll("-{2,}", "-").replaceAll("^-+|-+$", "");
        if (h.length() > 63) {
            h = h.substring(0, 63).replaceAll("-+$", "");
        }
        return h;
    }

    @Override
    protected void applyImplicitComponents(DataComponentInput input) {
        super.applyImplicitComponents(input);
        customName = input.get(DataComponents.CUSTOM_NAME);
    }

    @Override
    protected void collectImplicitComponents(DataComponentMap.Builder components) {
        super.collectImplicitComponents(components);
        if (customName != null) {
            components.set(DataComponents.CUSTOM_NAME, customName);
        }
    }

    @SuppressWarnings("deprecation")
    @Override
    public void removeComponentsFromTag(CompoundTag tag) {
        super.removeComponentsFromTag(tag);
        tag.remove("CustomName");
    }

    public String agentInfo() {
        return agentInfo;
    }

    public boolean agentConnected() {
        return agentConnected;
    }

    public long uptimeMs() {
        if (!isMachineOn() || uptimeAt == 0) {
            return 0;
        }
        return uptimeMs + (vmState == VmState.VM_STATE_RUNNING_VALUE ? System.currentTimeMillis() - uptimeAt : 0);
    }

    public String imageId() {
        return imageId;
    }

    public String installedImage() {
        return installedImage;
    }

    public boolean wantRunning() {
        return wantRunning;
    }

    /** Incremented whenever something shown in the GUI changes. */
    public int statusVersion() {
        return statusVersion;
    }

    private void bump() {
        statusVersion++;
    }

    private void onInventoryChanged() {
        setChanged();
        bump();
    }

    // ------------------------------------------------------------------ peripherals

    public void addPeripheral(BlockPos pos, PeripheralKind kind) {
        peripherals.put(pos.immutable(), kind);
        setChanged();
        bump();
    }

    public void removePeripheral(BlockPos pos) {
        if (peripherals.remove(pos) != null) {
            setChanged();
            bump();
        }
    }

    public Map<BlockPos, PeripheralKind> peripherals() {
        return peripherals;
    }

    /** Drops links whose device is gone (only checks loaded positions). */
    private void cleanPeripherals() {
        if (level == null) {
            return;
        }
        boolean changed = peripherals.entrySet().removeIf(e -> level.isLoaded(e.getKey())
                && !(level.getBlockEntity(e.getKey()) instanceof PeripheralBlockEntity pbe && pbe.isLinkedTo(this)));
        if (changed) {
            setChanged();
        }
    }

    // ------------------------------------------------------------------ redstone and agent

    private Direction facing() {
        BlockState s = getBlockState();
        return s.hasProperty(DeviceBlock.FACING) ? s.getValue(DeviceBlock.FACING) : Direction.NORTH;
    }

    /** Absolute direction of a relative side: 0 front, 1 back, 2 left, 3 right, 4 top, 5 bottom (computer's own left/right). */
    public static Direction toAbsolute(Direction facing, int side) {
        return switch (side) {
            case 0 -> facing;
            case 1 -> facing.getOpposite();
            case 2 -> facing.getCounterClockWise();
            case 3 -> facing.getClockWise();
            case 4 -> Direction.UP;
            default -> Direction.DOWN;
        };
    }

    public static int toRelative(Direction facing, Direction dir) {
        for (int side = 0; side < 6; side++) {
            if (toAbsolute(facing, side) == dir) {
                return side;
            }
        }
        return 0;
    }

    public void updateRedstoneInputs(boolean force) {
        if (level == null || level.isClientSide) {
            return;
        }
        Direction facing = facing();
        boolean changed = false;
        for (int side = 0; side < 6; side++) {
            Direction dir = toAbsolute(facing, side);
            int v = level.getSignal(worldPosition.relative(dir), dir);
            if (v != redstoneIn[side]) {
                redstoneIn[side] = v;
                changed = true;
            }
        }
        if ((changed || force) && vmState == VmState.VM_STATE_RUNNING_VALUE && DaemonManager.isReady()) {
            SetRedstoneInputs.Builder b = SetRedstoneInputs.newBuilder().setVmId(vmIdString());
            for (int v : redstoneIn) {
                b.addLevels(v);
            }
            DaemonManager d = DaemonManager.get();
            if (d != null) {
                d.send(Request.newBuilder().setSetRedstoneInputs(b).build());
            }
        }
    }

    public int getRedstoneOutput(Direction face) {
        return redstoneOut[toRelative(facing(), face)];
    }

    public void setRedstoneOutput(int side, int value) {
        if (side < 0 || side >= 6) {
            return;
        }
        int v = Math.max(0, Math.min(15, value));
        if (redstoneOut[side] != v) {
            redstoneOut[side] = v;
            setChanged();
            notifyRedstone();
        }
    }

    private void notifyRedstone() {
        if (level == null || level.isClientSide || isRemoved()) {
            return;
        }
        Block block = getBlockState().getBlock();
        level.updateNeighborsAt(worldPosition, block);
        for (Direction d : Direction.values()) {
            level.updateNeighborsAtExceptFromFacing(worldPosition.relative(d), block, d.getOpposite());
        }
    }

    public void onAgentMessage(String json) {
        try {
            JsonObject o = JsonParser.parseString(json).getAsJsonObject();
            String type = o.has("type") ? o.get("type").getAsString() : "";
            switch (type) {
                case "hello" -> {
                    agentConnected = true;
                    String os = o.has("os") ? o.get("os").getAsString() : "";
                    String host = o.has("hostname") ? o.get("hostname").getAsString() : "";
                    agentInfo = host.isEmpty() ? os : os + " (" + host + ")";
                    bump();
                    updateRedstoneInputs(true);
                    sendAgentTime();
                }
                case "redstone_set" -> {
                    String side = o.has("side") ? o.get("side").getAsString() : "";
                    int idx = Arrays.asList(SIDES).indexOf(side);
                    if (idx >= 0 && o.has("level")) {
                        setRedstoneOutput(idx, o.get("level").getAsInt());
                    }
                }
                default -> {
                }
            }
        } catch (RuntimeException e) {
            FcwMod.LOGGER.debug("Bad agent message from {}: {}", vmId, json);
        }
    }

    private void sendAgentTime() {
        DaemonManager d = DaemonManager.get();
        if (d != null && DaemonManager.isReady()) {
            JsonObject o = new JsonObject();
            o.addProperty("type", "time");
            o.addProperty("unix_ms", System.currentTimeMillis());
            d.send(Request.newBuilder().setAgentSend(AgentSend.newBuilder().setVmId(vmIdString()).setJson(o.toString())).build());
        }
    }

    // ------------------------------------------------------------------ menu

    @Override
    public Component getDisplayName() {
        return customName != null ? customName : getBlockState().getBlock().getName();
    }

    @Nullable
    @Override
    public AbstractContainerMenu createMenu(int containerId, Inventory playerInventory, Player player) {
        return new ComputerMenu(containerId, playerInventory, this);
    }

    public boolean stillValid(Player player) {
        return level != null && !isRemoved() && level.getBlockEntity(worldPosition) == this
                && player.distanceToSqr(worldPosition.getX() + 0.5, worldPosition.getY() + 0.5, worldPosition.getZ() + 0.5) <= 64.0;
    }

    // ------------------------------------------------------------------ NBT

    @Override
    protected void saveAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.saveAdditional(tag, registries);
        tag.putUUID("VmId", vmId());
        if (customName != null) {
            tag.putString("CustomName", Component.Serializer.toJson(customName, registries));
        }
        if (owner != null) {
            tag.putUUID("Owner", owner);
        }
        tag.put("Inventory", inventory.serializeNBT(registries));
        tag.putString("Image", imageId);
        tag.putString("InstalledImage", installedImage);
        tag.putBoolean("WantRunning", wantRunning);
        tag.putIntArray("RedstoneOut", redstoneOut);
        if (lastErrorComponent != null) {
            tag.putString("LastErrorJson", Component.Serializer.toJson(lastErrorComponent, registries));
        } else if (!lastError.isEmpty()) {
            tag.putString("LastError", lastError);
        }
        ListTag links = new ListTag();
        for (Map.Entry<BlockPos, PeripheralKind> e : peripherals.entrySet()) {
            CompoundTag t = new CompoundTag();
            t.put("Pos", NbtUtils.writeBlockPos(e.getKey()));
            t.putString("Kind", e.getValue().id());
            links.add(t);
        }
        tag.put("Peripherals", links);
    }

    @Override
    protected void loadAdditional(CompoundTag tag, HolderLookup.Provider registries) {
        super.loadAdditional(tag, registries);
        if (tag.hasUUID("VmId")) {
            vmId = tag.getUUID("VmId");
        }
        owner = tag.hasUUID("Owner") ? tag.getUUID("Owner") : null;
        customName = tag.contains("CustomName", Tag.TAG_STRING) ? parseCustomNameSafe(tag.getString("CustomName"), registries) : null;
        if (tag.contains("Inventory")) {
            inventory.deserializeNBT(registries, tag.getCompound("Inventory"));
        }
        imageId = tag.getString("Image");
        installedImage = tag.getString("InstalledImage");
        wantRunning = tag.getBoolean("WantRunning");
        int[] out = tag.getIntArray("RedstoneOut");
        Arrays.fill(redstoneOut, 0);
        System.arraycopy(out, 0, redstoneOut, 0, Math.min(out.length, 6));
        lastErrorComponent = tag.contains("LastErrorJson") ? Component.Serializer.fromJson(tag.getString("LastErrorJson"), registries) : null;
        lastError = tag.getString("LastError");
        peripherals.clear();
        for (Tag t : tag.getList("Peripherals", Tag.TAG_COMPOUND)) {
            CompoundTag c = (CompoundTag) t;
            PeripheralKind kind = PeripheralKind.byId(c.getString("Kind"));
            NbtUtils.readBlockPos(c, "Pos").ifPresent(p -> {
                if (kind != null) {
                    peripherals.put(p, kind);
                }
            });
        }
    }

    /** For /fcw vms and the GUI. */
    public List<String> describeHardware() {
        List<String> l = new ArrayList<>();
        l.add(effectiveCpus() + " vCPU");
        l.add(effectiveMemoryMb() + " MB");
        if (inventory.hasGpu()) {
            l.add("GPU");
        }
        return l;
    }
}
