package com.fatecomputers.fcw.server;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.proto.ControlProto.Event;
import com.fatecomputers.fcw.proto.ControlProto.FetchImage;
import com.fatecomputers.fcw.proto.ControlProto.HelloResult;
import com.fatecomputers.fcw.proto.ControlProto.ImageInfo;
import com.fatecomputers.fcw.proto.ControlProto.ImageProgress;
import com.fatecomputers.fcw.proto.ControlProto.ListImages;
import com.fatecomputers.fcw.proto.ControlProto.ListVms;
import com.fatecomputers.fcw.proto.ControlProto.Request;
import com.fatecomputers.fcw.proto.ControlProto.VmInfo;
import com.fatecomputers.fcw.server.daemon.DaemonManager;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import net.minecraft.ChatFormatting;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.regex.Pattern;

/** /fcw status | images | images fetch &lt;id&gt; | vms */
public final class FcwCommands {
    private static final Pattern IMAGE_ID = Pattern.compile("[a-z0-9][a-z0-9._-]{0,63}");

    private record FetchWatch(CommandSourceStack source, int[] lastStep) {
    }

    private static final Map<String, List<FetchWatch>> FETCHES = new HashMap<>();
    private static DaemonManager listeningTo;

    private FcwCommands() {
    }

    public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("fcw")
                .then(Commands.literal("status").executes(FcwCommands::status))
                .then(Commands.literal("images")
                        .executes(FcwCommands::images)
                        .then(Commands.literal("fetch")
                                .requires(s -> s.hasPermission(2))
                                .then(Commands.argument("id", StringArgumentType.word()).executes(FcwCommands::fetch))))
                .then(Commands.literal("vms").requires(s -> s.hasPermission(2)).executes(FcwCommands::vms))
                .then(Commands.literal("qemu")
                        .executes(FcwCommands::qemu)
                        .then(Commands.literal("fetch").requires(s -> s.hasPermission(2)).executes(FcwCommands::qemuFetch))));
    }

    private static String human(long mb) {
        return mb >= 1024 && mb % 1024 == 0 ? (mb / 1024) + " GB" : mb + " MB";
    }

    private static int status(CommandContext<CommandSourceStack> ctx) {
        CommandSourceStack src = ctx.getSource();
        DaemonManager d = DaemonManager.get();
        if (d == null) {
            src.sendSuccess(() -> Component.translatable("command.fcw.status.none").withStyle(ChatFormatting.RED), false);
            return 0;
        }
        DaemonManager.Status st = d.status();
        ChatFormatting color = st == DaemonManager.Status.READY ? ChatFormatting.GREEN
                : st == DaemonManager.Status.STARTING ? ChatFormatting.YELLOW : ChatFormatting.RED;
        src.sendSuccess(() -> Component.translatable("command.fcw.status.daemon",
                Component.literal(st.name().toLowerCase(Locale.ROOT)).withStyle(color), d.reason()), false);
        src.sendSuccess(() -> d.isRemote()
                ? Component.translatable("command.fcw.status.mode_remote", d.remoteAddress())
                : Component.translatable("command.fcw.status.mode_local"), false);
        if (d.daemonBinary() != null) {
            src.sendSuccess(() -> Component.translatable("command.fcw.status.binary", d.daemonBinary().toString()).withStyle(ChatFormatting.GRAY), false);
        }
        HelloResult h = d.hello();
        if (h != null) {
            src.sendSuccess(() -> Component.translatable("command.fcw.status.hello", h.getDaemonVersion(), h.getProtocolVersion(),
                    d.accelerator(), h.getQuicPort()), false);
            String cert = h.getCertSha256();
            src.sendSuccess(() -> Component.translatable("command.fcw.status.host", h.getHostCpus(), human(h.getHostMemoryMb()),
                    h.getMaxVms(), cert.length() > 16 ? cert.substring(0, 16) + "…" : cert).withStyle(ChatFormatting.GRAY), false);
            src.sendSuccess(() -> h.getNetworkEnabled()
                    ? Component.translatable("command.fcw.status.network", h.getNetworkSubnet(), h.getDnsZone(),
                    Component.translatable(h.getInternetAllowed() ? "command.fcw.status.internet_on" : "command.fcw.status.internet_off"))
                    : Component.translatable("command.fcw.status.network_off"), false);
        }
        if (h != null) {
            qemuLines(src, d);
            String accel = d.accelerator();
            if ("tcg".equals(accel)) {
                src.sendSuccess(() -> Component.translatable("command.fcw.status.accel_tcg").withStyle(ChatFormatting.YELLOW), false);
                String why = h.getAcceleratorNote();
                if (!why.isEmpty()) {
                    src.sendSuccess(() -> Component.translatable("command.fcw.status.accel_why", why).withStyle(ChatFormatting.GRAY), false);
                }
            } else if (!accel.isEmpty()) {
                src.sendSuccess(() -> Component.translatable("command.fcw.status.accel", accel).withStyle(ChatFormatting.GREEN), false);
            }
        }
        String publicHost = FcwConfig.SERVER.publicHost.get();
        // the port the daemon really listens on (in remote mode it is set on the daemon's host)
        int daemonPort = h != null && h.getQuicPort() != 0 ? h.getQuicPort() : FcwConfig.SERVER.quicPort.get();
        int publicPort = FcwConfig.SERVER.publicPort.get() != 0 ? FcwConfig.SERVER.publicPort.get() : daemonPort;
        src.sendSuccess(() -> Component.translatable("command.fcw.status.public_host",
                publicHost.isEmpty() ? Component.translatable("command.fcw.status.public_host.auto") : Component.literal(publicHost),
                publicPort, daemonPort).withStyle(ChatFormatting.GRAY), false);
        src.sendSuccess(() -> (d.isRemote()
                ? Component.translatable("command.fcw.status.dirs_remote")
                : Component.translatable("command.fcw.status.dirs", d.imagesDir().toString(), d.vmsDir().toString())).withStyle(ChatFormatting.GRAY), false);
        int loaded = ComputerTracker.all().size();
        long running = ComputerTracker.all().stream().filter(ComputerBlockEntity::isMachineOn).count();
        src.sendSuccess(() -> Component.translatable("command.fcw.status.computers", loaded, running), false);
        return st == DaemonManager.Status.READY ? 1 : 0;
    }

    private static boolean requireReady(CommandSourceStack src, DaemonManager d) {
        if (d == null || !DaemonManager.isReady()) {
            src.sendFailure(Component.translatable("gui.fcw.error.daemon",
                    d == null ? Component.translatable("daemon.fcw.stopped") : d.reason()));
            return false;
        }
        return true;
    }

    private static int images(CommandContext<CommandSourceStack> ctx) {
        CommandSourceStack src = ctx.getSource();
        DaemonManager d = DaemonManager.get();
        if (!requireReady(src, d)) {
            return 0;
        }
        d.request(Request.newBuilder().setListImages(ListImages.getDefaultInstance()).build(), r -> {
            if (!r.getOk()) {
                src.sendFailure(Component.translatable("command.fcw.error", r.getError()));
                return;
            }
            List<ImageInfo> list = r.getImages().getImagesList();
            if (list.isEmpty()) {
                src.sendSuccess(() -> Component.translatable("command.fcw.images.empty",
                        d.isRemote() ? Component.translatable("command.fcw.on_daemon_host") : Component.literal(d.imagesDir().toString())), false);
                return;
            }
            src.sendSuccess(() -> Component.translatable("command.fcw.images.header", list.size()), false);
            for (ImageInfo i : list) {
                MutableComponent line = Component.literal(" " + i.getId()).withStyle(ChatFormatting.AQUA)
                        .append(Component.literal(" — " + i.getName() + " " + i.getVersion()).withStyle(ChatFormatting.WHITE));
                if (!i.getDesktop().isEmpty()) {
                    line.append(Component.literal(", " + i.getDesktop()).withStyle(ChatFormatting.GRAY));
                }
                line.append(Component.literal(", ").withStyle(ChatFormatting.GRAY))
                        .append(Component.translatable("command.fcw.images.memory", i.getMinMemoryMb(), i.getRecommendedMemoryMb()).withStyle(ChatFormatting.GRAY))
                        .append(Component.literal(" "))
                        .append(i.getInstalled()
                                ? Component.translatable("command.fcw.images.installed").withStyle(ChatFormatting.GREEN)
                                : Component.translatable("command.fcw.images.not_installed", i.getId()).withStyle(ChatFormatting.YELLOW));
                src.sendSuccess(() -> line, false);
            }
        });
        return 1;
    }

    private static int fetch(CommandContext<CommandSourceStack> ctx) {
        CommandSourceStack src = ctx.getSource();
        String id = StringArgumentType.getString(ctx, "id").toLowerCase(Locale.ROOT);
        if (!IMAGE_ID.matcher(id).matches()) {
            src.sendFailure(Component.translatable("command.fcw.images.bad_id", id));
            return 0;
        }
        DaemonManager d = DaemonManager.get();
        if (!requireReady(src, d)) {
            return 0;
        }
        String url = FcwConfig.SERVER.imageUrlTemplate.get().replace("<id>", id).replace("{id}", id);
        ensureListener(d);
        FETCHES.computeIfAbsent(id, k -> new ArrayList<>()).add(new FetchWatch(src, new int[]{-1}));
        src.sendSuccess(() -> Component.translatable("command.fcw.images.fetch_start", id, url), true);
        d.request(Request.newBuilder().setFetchImage(FetchImage.newBuilder().setImageId(id).setUrl(url)).build(), 60 * 60_000L, r -> {
            if (!r.getOk()) {
                FETCHES.remove(id);
                src.sendFailure(Component.translatable("command.fcw.images.fetch_failed", id, r.getError()));
            }
        });
        return 1;
    }

    private static void ensureListener(DaemonManager d) {
        if (listeningTo == d) {
            return;
        }
        listeningTo = d;
        d.addListener(FcwCommands::onEvent);
    }

    private static void onEvent(Event e) {
        if (e.getKindCase() != Event.KindCase.IMAGE_PROGRESS) {
            return;
        }
        ImageProgress p = e.getImageProgress();
        List<FetchWatch> watches = FETCHES.get(p.getImageId());
        if (watches == null) {
            return;
        }
        if (!p.getError().isEmpty()) {
            FETCHES.remove(p.getImageId());
            watches.forEach(w -> w.source.sendFailure(Component.translatable("command.fcw.images.fetch_failed", p.getImageId(), p.getError())));
            return;
        }
        if (p.getFinished()) {
            FETCHES.remove(p.getImageId());
            watches.forEach(w -> w.source.sendSuccess(() -> Component.translatable("command.fcw.images.fetch_done", p.getImageId()).withStyle(ChatFormatting.GREEN), true));
            return;
        }
        long total = p.getTotalBytes();
        int percent = total > 0 ? (int) (p.getDoneBytes() * 100 / total) : -1;
        int step = percent >= 0 ? percent / 10 : (int) (p.getDoneBytes() / (100L << 20));
        for (FetchWatch w : watches) {
            if (step > w.lastStep[0]) {
                w.lastStep[0] = step;
                Component msg = percent >= 0
                        ? Component.translatable("command.fcw.images.fetch_progress", p.getImageId(), percent, p.getDoneBytes() >> 20, total >> 20)
                        : Component.translatable("command.fcw.images.fetch_progress_unknown", p.getImageId(), p.getDoneBytes() >> 20);
                w.source.sendSuccess(() -> msg.copy().withStyle(ChatFormatting.GRAY), false);
            }
        }
    }

    private static String stateName(VmInfo v) {
        return v.getState().name().replace("VM_STATE_", "").toLowerCase(Locale.ROOT);
    }

    private static int vms(CommandContext<CommandSourceStack> ctx) {
        CommandSourceStack src = ctx.getSource();
        DaemonManager d = DaemonManager.get();
        if (!requireReady(src, d)) {
            return 0;
        }
        d.request(Request.newBuilder().setListVms(ListVms.getDefaultInstance()).build(), r -> {
            if (!r.getOk()) {
                src.sendFailure(Component.translatable("command.fcw.error", r.getError()));
                return;
            }
            List<VmInfo> list = r.getVms().getVmsList();
            src.sendSuccess(() -> Component.translatable("command.fcw.vms.header", list.size()), false);
            for (VmInfo v : list) {
                ComputerBlockEntity be = ComputerTracker.get(v.getVmId());
                String where = be == null ? "-" : be.getBlockPos().toShortString() + " " + be.getLevel().dimension().location();
                MutableComponent line = Component.literal(" " + v.getVmId().substring(0, Math.min(8, v.getVmId().length())))
                        .withStyle(ChatFormatting.AQUA)
                        .append(Component.literal(" " + stateName(v)).withStyle(ChatFormatting.WHITE))
                        .append(Component.literal(" " + v.getImageId()
                                + (v.getImageVersion().isEmpty() ? "" : " [" + v.getImageVersion() + "]")
                                + ", " + v.getCpus() + " vCPU, " + human(v.getMemoryMb())
                                + (v.getGpu() ? ", GPU" : "") + ", " + (v.getUptimeMs() / 1000) + " s"
                                + (v.getHostname().isEmpty() ? "" : ", " + v.getHostname() + (d.dnsZone().isEmpty() ? "" : "." + d.dnsZone()))
                                + (v.getIpAddress().isEmpty() ? "" : " " + v.getIpAddress())
                                + (v.getControllerPlayer().isEmpty() ? "" : ", @" + v.getControllerPlayer())
                                + (v.getLastError().isEmpty() ? "" : ", " + v.getLastError())
                                + ", " + where).withStyle(ChatFormatting.GRAY));
                if (v.getImageUpdateAvailable()) {
                    line.append(Component.literal(" ")).append(Component.translatable("command.fcw.vms.update").withStyle(ChatFormatting.GOLD));
                }
                src.sendSuccess(() -> line, false);
            }
        });
        return 1;
    }

    /** QEMU line(s) for /fcw status and /fcw qemu. */
    private static void qemuLines(CommandSourceStack src, DaemonManager d) {
        HelloResult h = d.hello();
        if (h == null) {
            return;
        }
        if (!d.qemuSupported()) {
            src.sendSuccess(() -> Component.translatable("command.fcw.qemu.unknown").withStyle(ChatFormatting.GRAY), false);
        } else if (h.getQemuAvailable()) {
            src.sendSuccess(() -> Component.translatable("command.fcw.qemu.found", h.getQemuVersion(), h.getQemuPath(),
                    Component.translatable(h.getQemuBundled() ? "command.fcw.qemu.bundled" : "command.fcw.qemu.system")), false);
        } else {
            Component why = switch (d.qemuFetchState()) {
                case RUNNING -> d.qemuProgressText();
                case FAILED -> Component.translatable("command.fcw.qemu.failed", d.qemuFetchError());
                default -> Component.translatable(d.qemuPlatformPublished() ? "command.fcw.qemu.hint_fetch" : "command.fcw.qemu.hint_install");
            };
            src.sendSuccess(() -> Component.translatable("command.fcw.qemu.missing", why).withStyle(ChatFormatting.RED), false);
        }
    }

    private static int qemu(CommandContext<CommandSourceStack> ctx) {
        CommandSourceStack src = ctx.getSource();
        DaemonManager d = DaemonManager.get();
        if (!requireReady(src, d)) {
            return 0;
        }
        qemuLines(src, d);
        src.sendSuccess(() -> Component.translatable("command.fcw.qemu.dir",
                d.isRemote() ? Component.translatable("command.fcw.on_daemon_host") : Component.literal(d.qemuDir().toString()),
                Component.translatable(FcwConfig.SERVER.qemuAutoDownload.get() ? "options.on" : "options.off")).withStyle(ChatFormatting.GRAY), false);
        return 1;
    }

    private static int qemuFetch(CommandContext<CommandSourceStack> ctx) {
        CommandSourceStack src = ctx.getSource();
        DaemonManager d = DaemonManager.get();
        if (!requireReady(src, d)) {
            return 0;
        }
        if (d.qemuFetchState() == DaemonManager.QemuFetch.RUNNING) {
            src.sendSuccess(d::qemuProgressText, false);
            return 1;
        }
        d.addQemuListener(msg -> src.sendSuccess(() -> msg, true));
        Component problem = d.fetchQemu();
        if (problem != null) {
            src.sendFailure(problem);
            return 0;
        }
        return 1;
    }

    public static void reset() {
        FETCHES.clear();
        listeningTo = null;
    }
}
