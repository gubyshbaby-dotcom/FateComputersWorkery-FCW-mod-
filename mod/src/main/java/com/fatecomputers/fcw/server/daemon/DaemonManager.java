package com.fatecomputers.fcw.server.daemon;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.proto.ControlProto.Envelope;
import com.fatecomputers.fcw.proto.ControlProto.Event;
import com.fatecomputers.fcw.proto.ControlProto.FetchQemu;
import com.fatecomputers.fcw.proto.ControlProto.Hello;
import com.fatecomputers.fcw.proto.ControlProto.HelloResult;
import com.fatecomputers.fcw.proto.ControlProto.ImageInfo;
import com.fatecomputers.fcw.proto.ControlProto.ImageProgress;
import com.fatecomputers.fcw.proto.ControlProto.ListImages;
import com.fatecomputers.fcw.proto.ControlProto.Request;
import com.fatecomputers.fcw.proto.ControlProto.Response;
import com.fatecomputers.fcw.proto.ControlProto.Shutdown;
import com.fatecomputers.fcw.block.entity.ComputerBlockEntity;
import com.fatecomputers.fcw.proto.ControlProto.DeleteVm;
import com.fatecomputers.fcw.proto.ControlProto.StopVm;
import com.fatecomputers.fcw.server.ComputerTracker;
import com.fatecomputers.fcw.server.FcwWorldData;
import com.fatecomputers.fcw.util.NativeFiles;
import com.fatecomputers.fcw.util.Platform;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.fml.loading.FMLPaths;

import javax.annotation.Nullable;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;
import java.util.stream.Stream;

/**
 * Starts and talks to the native VM daemon (docs/CONTRACT.md §2-3).
 * <p>
 * Process start, the wait for {@code FCW-READY} and {@code Hello} run on a background thread so the
 * server never blocks. Requests go through a queue to the writer thread; responses and events are
 * queued by the reader thread and applied on the server thread in {@link #tick()}, at most
 * {@code messagesPerTick} per tick. Callbacks therefore always run on the server thread.
 */
public final class DaemonManager {
    public enum Status { DISABLED, STARTING, READY, FAILED, STOPPED }

    public enum QemuFetch { IDLE, RUNNING, FAILED, DONE }

    /** image_id of ImageProgress events about the portable QEMU download. */
    public static final String QEMU_ID = "qemu";

    public static final int PROTOCOL_VERSION = 1;
    public static final long DEFAULT_TIMEOUT_MS = 60_000;

    private static volatile DaemonManager current;

    private final MinecraftServer server;
    private final Path imagesDir;
    private final Path vmsDir;
    private final Path qemuDir;
    /** daemon.mode = remote: connect over TCP to a daemon service on the host instead of starting one. */
    private final boolean remote;
    private final String remoteAddress;
    private volatile boolean stopping;
    @Nullable
    private volatile Thread connector;
    @Nullable
    private volatile CountDownLatch remoteStops;
    /** --help output of the daemon binary: tells which flags/requests this build knows. */
    private volatile String daemonHelp = "";
    private QemuFetch qemuFetch = QemuFetch.IDLE;
    private Component qemuFetchError = Component.empty();
    private long qemuDone;
    private long qemuTotal;
    private int qemuLoggedStep = -1;
    private final List<Consumer<Component>> qemuListeners = new CopyOnWriteArrayList<>();
    private volatile Status status = Status.STARTING;
    private volatile Component reason = Component.translatable("daemon.fcw.starting");
    @Nullable
    private volatile Process process;
    @Nullable
    private volatile ControlConnection connection;
    @Nullable
    private volatile HelloResult hello;
    @Nullable
    private volatile String accelerator;
    @Nullable
    private volatile Path controlDir;
    @Nullable
    private volatile Path daemonBinary;
    private final Deque<String> stderrTail = new ArrayDeque<>();
    private final ConcurrentLinkedQueue<Runnable> mainThreadQueue = new ConcurrentLinkedQueue<>();
    private final ConcurrentLinkedQueue<Envelope> inbound = new ConcurrentLinkedQueue<>();
    private final Map<Long, Pending> pending = new ConcurrentHashMap<>();
    private final AtomicLong ids = new AtomicLong();
    private final List<Consumer<Event>> listeners = new CopyOnWriteArrayList<>();
    private List<ImageInfo> images = List.of();
    private Component imagesError = Component.empty();
    private long imagesUpdatedAt;
    private boolean imagesLoading;
    private long tickCount;

    private record Pending(long deadline, Consumer<Response> callback, boolean direct) {
    }

    private DaemonManager(MinecraftServer server) {
        this.server = server;
        this.imagesDir = FMLPaths.GAMEDIR.get().resolve("fcw").resolve("images").toAbsolutePath().normalize();
        this.qemuDir = FMLPaths.GAMEDIR.get().resolve("fcw").resolve("qemu").toAbsolutePath().normalize();
        this.remote = "remote".equals(FcwConfig.SERVER.mode.get());
        this.remoteAddress = FcwConfig.SERVER.remoteAddress.get().trim();
        this.vmsDir = server.getWorldPath(LevelResource.ROOT).resolve("fcw").resolve("vms").toAbsolutePath().normalize();
    }

    // ------------------------------------------------------------------ lifecycle

    @Nullable
    public static DaemonManager get() {
        return current;
    }

    public static boolean isReady() {
        DaemonManager d = current;
        return d != null && d.status == Status.READY;
    }

    /** Server thread, ServerStartingEvent. */
    public static void start(MinecraftServer server) {
        DaemonManager old = current;
        if (old != null) {
            old.stopNow();
        }
        DaemonManager d = new DaemonManager(server);
        current = d;
        d.launch();
    }

    /** Server thread, ServerStoppingEvent: ask the daemon to shut its machines down. */
    public static void beginShutdown() {
        DaemonManager d = current;
        if (d != null) {
            d.requestShutdown();
        }
    }

    /** Server thread, ServerStoppedEvent: wait for the process, then destroy it. */
    public static void finishShutdown() {
        DaemonManager d = current;
        current = null;
        if (d != null) {
            d.stopNow();
        }
    }

    private void launch() {
        if (!FcwConfig.SERVER.daemonEnabled.get()) {
            fail(Status.DISABLED, Component.translatable("daemon.fcw.disabled_config"));
            return;
        }
        if (remote) {
            launchRemote();
            return;
        }
        Path binary;
        String configured = FcwConfig.SERVER.daemonPath.get().trim();
        try {
            if (!configured.isEmpty()) {
                binary = Paths.get(configured).toAbsolutePath();
                if (!Files.isRegularFile(binary)) {
                    fail(Status.DISABLED, Component.translatable("daemon.fcw.binary_missing", binary.toString()));
                    return;
                }
            } else {
                Optional<Path> extracted = NativeFiles.extract(Platform.daemonFileName(),
                        FMLPaths.GAMEDIR.get().resolve("fcw").resolve("natives"), true);
                if (extracted.isEmpty()) {
                    fail(Status.DISABLED, Component.translatable("daemon.fcw.no_binary", Platform.id(),
                            NativeFiles.resourcePath(Platform.daemonFileName())));
                    return;
                }
                binary = extracted.get();
            }
        } catch (IOException e) {
            fail(Status.FAILED, Component.translatable("daemon.fcw.start_failed", e.toString()));
            return;
        }
        daemonBinary = binary;
        List<String> cmd = new ArrayList<>();
        cmd.add(binary.toString());
        String qemu = FcwConfig.SERVER.qemuPath.get().trim();
        int quicPort = FcwConfig.SERVER.quicPort.get();
        int maxVms = FcwConfig.SERVER.maxVms.get();
        String accel = FcwConfig.SERVER.accelerator.get();
        int startupTimeout = FcwConfig.SERVER.startupTimeoutSeconds.get();
        Path socket;
        try {
            Files.createDirectories(imagesDir);
            Files.createDirectories(vmsDir);
            Path dir = Files.createTempDirectory("fcw-");
            controlDir = dir;
            socket = dir.resolve("control.sock");
        } catch (IOException e) {
            fail(Status.FAILED, Component.translatable("daemon.fcw.start_failed", e.toString()));
            return;
        }
        cmd.addAll(List.of("--images-dir", imagesDir.toString(),
                "--vms-dir", vmsDir.toString(),
                "--control", socket.toString(),
                "--quic-bind", "0.0.0.0:" + quicPort));
        if (!qemu.isEmpty()) {
            cmd.addAll(List.of("--qemu", qemu));
        }
        cmd.addAll(List.of("--max-vms", Integer.toString(maxVms), "--accel", accel));
        // Guest network (docs/CONTRACT.md §3 "Сеть гостей"); only passed if this daemon knows the flags.
        List<String[]> netArgs = new ArrayList<>();
        netArgs.add(new String[]{"--network", FcwConfig.SERVER.guestNetwork.get() ? "on" : "off"});
        netArgs.add(new String[]{"--internet", FcwConfig.SERVER.internet.get() ? "on" : "off"});
        int rate = FcwConfig.SERVER.rateLimitKiBps.get();
        if (rate > 0) {
            netArgs.add(new String[]{"--net-rate-limit", Long.toString(rate * 1024L)});
        }
        for (String cidr : FcwConfig.SERVER.netAllow.get()) {
            netArgs.add(new String[]{"--net-allow", cidr.trim()});
        }
        for (String cidr : FcwConfig.SERVER.netDeny.get()) {
            netArgs.add(new String[]{"--net-deny", cidr.trim()});
        }
        status = Status.STARTING;
        reason = Component.translatable("daemon.fcw.starting");
        final Path bin = binary;
        netArgs.add(new String[]{"--qemu-dir", qemuDir.toString()});
        Thread t = new Thread(() -> {
            daemonHelp = addSupportedFlags(bin, cmd, netArgs);
            startupThread(cmd, socket, startupTimeout);
        }, "fcw-daemon-start");
        t.setDaemon(true);
        t.start();
    }

    /**
     * Appends optional flags that this daemon build understands (it rejects unknown flags), judged
     * by its --help output. Lets the mod run with an older daemon.
     */
    private static String addSupportedFlags(Path binary, List<String> cmd, List<String[]> optional) {
        String help = "";
        try {
            Process p = new ProcessBuilder(binary.toString(), "--help").redirectErrorStream(true).start();
            p.getOutputStream().close();
            byte[] out = p.getInputStream().readAllBytes();
            p.waitFor(5, TimeUnit.SECONDS);
            help = new String(out, StandardCharsets.UTF_8);
        } catch (IOException e) {
            FcwMod.LOGGER.warn("Could not ask the VM daemon for its options: {}", e.toString());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        List<String> skipped = new ArrayList<>();
        for (String[] arg : optional) {
            if (help.contains(arg[0])) {
                cmd.addAll(List.of(arg));
            } else if (!skipped.contains(arg[0])) {
                skipped.add(arg[0]);
            }
        }
        if (!skipped.isEmpty()) {
            FcwMod.LOGGER.warn("This VM daemon does not support {}; these settings are not applied", String.join(", ", skipped));
        }
        return help;
    }

    private void startupThread(List<String> cmd, Path socket, int timeoutSeconds) {
        FcwMod.LOGGER.info("Starting VM daemon: {}", String.join(" ", cmd));
        Process p;
        try {
            ProcessBuilder pb = new ProcessBuilder(cmd);
            pb.redirectInput(ProcessBuilder.Redirect.PIPE);
            p = pb.start();
        } catch (IOException e) {
            FcwMod.LOGGER.error("Could not start the VM daemon", e);
            failLater(Status.FAILED, Component.translatable("daemon.fcw.start_failed", e.toString()));
            return;
        }
        process = p;
        CompletableFuture<JsonObject> ready = new CompletableFuture<>();
        startPump(p.getInputStream(), "fcw-daemon-stdout", line -> {
            if (line.startsWith("FCW-READY ")) {
                try {
                    ready.complete(JsonParser.parseString(line.substring("FCW-READY ".length())).getAsJsonObject());
                } catch (RuntimeException e) {
                    ready.completeExceptionally(e);
                }
            } else {
                FcwMod.LOGGER.info("[fcw-daemon] {}", line);
            }
        }, () -> ready.completeExceptionally(new IOException("stdout closed")));
        startPump(p.getErrorStream(), "fcw-daemon-stderr", line -> {
            synchronized (stderrTail) {
                stderrTail.addLast(line);
                while (stderrTail.size() > 20) {
                    stderrTail.removeFirst();
                }
            }
            FcwMod.LOGGER.info("[fcw-daemon] {}", line);
        }, () -> {
        });
        p.onExit().thenAccept(ended -> mainThreadQueue.add(() -> onProcessExit(ended)));

        JsonObject info;
        try {
            info = ready.get(timeoutSeconds, TimeUnit.SECONDS);
        } catch (TimeoutException e) {
            failLater(Status.FAILED, Component.translatable("daemon.fcw.timeout", timeoutSeconds));
            p.destroy();
            return;
        } catch (Exception e) {
            // Process died before READY; onProcessExit reports the exit code and stderr.
            if (!p.isAlive()) {
                return;
            }
            failLater(Status.FAILED, Component.translatable("daemon.fcw.start_failed", String.valueOf(e.getCause())));
            p.destroy();
            return;
        }
        Path controlPath = socket;
        if (info.has("control") && info.get("control").isJsonPrimitive()) {
            controlPath = Paths.get(info.get("control").getAsString());
        }
        if (info.has("accelerator")) {
            accelerator = info.get("accelerator").getAsString();
        }
        FcwMod.LOGGER.info("VM daemon is ready: {}", info);

        ControlConnection conn = null;
        IOException lastError = null;
        for (int attempt = 0; attempt < 20 && conn == null; attempt++) {
            try {
                conn = ControlConnection.open(controlPath, this::onInbound, why -> mainThreadQueue.add(() -> onConnectionLost(why)));
            } catch (IOException e) {
                lastError = e;
                sleep(100);
            }
        }
        if (conn == null) {
            failLater(Status.FAILED, Component.translatable("daemon.fcw.connect_failed", String.valueOf(lastError)));
            p.destroy();
            return;
        }
        connection = conn;

        Response r = helloAndWait(conn, "");
        if (r == null || !r.getOk() || !r.hasHello()) {
            failLater(Status.FAILED, Component.translatable("daemon.fcw.hello_failed", r == null ? "timeout" : r.getError()));
            conn.close();
            p.destroy();
            return;
        }
        onHelloOk(r.getHello(), Component.translatable("daemon.fcw.ready"));
    }

    /** Sends Hello (first message of a connection) and waits for the answer; null on timeout. */
    @Nullable
    private Response helloAndWait(ControlConnection conn, String token) {
        CompletableFuture<Response> helloReply = new CompletableFuture<>();
        long id = ids.incrementAndGet();
        pending.put(id, new Pending(System.currentTimeMillis() + 15_000, helloReply::complete, true));
        Hello.Builder hb = Hello.newBuilder().setClientName("fcw-mod " + FcwMod.version()).setProtocolVersion(PROTOCOL_VERSION);
        if (!token.isEmpty()) {
            hb.setAuthToken(token);
        }
        conn.send(Envelope.newBuilder().setId(id).setRequest(Request.newBuilder().setHello(hb)).build());
        try {
            return helloReply.get(15, TimeUnit.SECONDS);
        } catch (Exception e) {
            pending.remove(id);
            return null;
        }
    }

    /** A connection said Hello successfully: become READY on the server thread and resync everything. */
    private void onHelloOk(HelloResult h, Component readyReason) {
        hello = h;
        if (!h.getAccelerator().isEmpty()) {
            accelerator = h.getAccelerator();
        }
        if (h.getProtocolVersion() != PROTOCOL_VERSION) {
            FcwMod.LOGGER.warn("VM daemon speaks protocol {}, the mod expects {}", h.getProtocolVersion(), PROTOCOL_VERSION);
        }
        FcwMod.LOGGER.info("Connected to VM daemon {}{} (protocol {}, accelerator {}, QUIC port {}, {} host CPUs, {} MB, network {}{})",
                h.getDaemonVersion(), remote ? " at " + remoteAddress : "", h.getProtocolVersion(), accelerator, h.getQuicPort(),
                h.getHostCpus(), h.getHostMemoryMb(),
                h.getNetworkEnabled() ? h.getNetworkSubnet() : "off",
                h.getNetworkEnabled() ? (h.getInternetAllowed() ? ", internet" : ", no internet") : "");
        mainThreadQueue.add(() -> {
            if (stopping || status == Status.STOPPED || status == Status.DISABLED) {
                return;
            }
            status = Status.READY;
            reason = readyReason;
            // Computers validate their image against the list, so load it before resyncing them.
            refreshImages(() -> {
                ComputerTracker.onDaemonReady();
                processPendingDeletes();
                logQemu();
                // An explicit daemon.qemuPath wins over the portable QEMU, so downloading would not help then.
                if (!remote && qemuMissing() && FcwConfig.SERVER.qemuAutoDownload.get() && FcwConfig.SERVER.qemuPath.get().isBlank()) {
                    Component why = fetchQemu();
                    if (why != null) {
                        FcwMod.LOGGER.warn("QEMU not found and cannot be downloaded: {}", why.getString());
                    }
                }
            });
        });
    }

    /** Machines whose block was broken while the daemon was unreachable: delete them now. */
    private void processPendingDeletes() {
        FcwWorldData data = FcwWorldData.get(server);
        for (String id : data.pendingDeletes()) {
            request(Request.newBuilder().setStopVm(StopVm.newBuilder().setVmId(id).setForce(true)).build(), r ->
                    request(Request.newBuilder().setDeleteVm(DeleteVm.newBuilder().setVmId(id)).build(), r2 -> {
                        FcwMod.LOGGER.info("Deleted VM {} of a broken computer{}", id, r2.getOk() ? "" : ": " + r2.getError());
                        data.removePendingDelete(id);
                        data.removeMachine(id);
                    }));
        }
    }

    // ------------------------------------------------------------------ remote daemon (TCP)

    private void launchRemote() {
        if (parseAddress(remoteAddress) == null) {
            fail(Status.DISABLED, Component.translatable("daemon.fcw.remote.bad_address", remoteAddress));
            return;
        }
        status = Status.STARTING;
        reason = Component.translatable("daemon.fcw.remote.connecting", remoteAddress);
        FcwMod.LOGGER.info("Using the remote VM daemon at {}", remoteAddress);
        Thread t = new Thread(this::remoteLoop, "fcw-daemon-remote");
        t.setDaemon(true);
        connector = t;
        t.start();
    }

    /** "host:port" or "[v6]:port"; null if malformed. */
    @Nullable
    static String[] parseAddress(String a) {
        String host;
        String port;
        if (a.startsWith("[")) {
            int end = a.indexOf(']');
            if (end < 0 || end + 1 >= a.length() || a.charAt(end + 1) != ':') {
                return null;
            }
            host = a.substring(1, end);
            port = a.substring(end + 2);
        } else {
            int c = a.lastIndexOf(':');
            if (c <= 0) {
                return null;
            }
            host = a.substring(0, c);
            port = a.substring(c + 1);
        }
        try {
            int p = Integer.parseInt(port);
            return p > 0 && p < 65536 && !host.isBlank() ? new String[]{host, port} : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static final int[] BACKOFF_S = {1, 2, 5, 10, 20, 30};

    /** Connector thread: connect, Hello with the token, wait until the connection drops, repeat. */
    private void remoteLoop() {
        int failures = 0;
        while (!stopping) {
            String[] hp = parseAddress(remoteAddress);
            int delay = BACKOFF_S[Math.min(failures, BACKOFF_S.length - 1)];
            CompletableFuture<String> closed = new CompletableFuture<>();
            ControlConnection conn;
            try {
                InetSocketAddress addr = new InetSocketAddress(hp[0], Integer.parseInt(hp[1]));
                if (addr.isUnresolved()) {
                    throw new IOException("unknown host " + hp[0]);
                }
                conn = ControlConnection.openTcp(addr, 5000, this::onInbound, closed::complete);
            } catch (IOException e) {
                failures++;
                remoteDown(Component.translatable("daemon.fcw.remote.no_connection", remoteAddress, ioMessage(e), delay),
                        "no connection (" + ioMessage(e) + "), retrying in " + delay + " s");
                pause(delay);
                continue;
            }
            connection = conn;
            Response r = helloAndWait(conn, FcwConfig.SERVER.remoteToken.get().trim());
            if (r == null || !r.getOk() || !r.hasHello()) {
                conn.close();
                connection = null;
                failures++;
                String err = r == null ? "timeout" : r.getError();
                boolean unauthorized = err.toLowerCase(java.util.Locale.ROOT).contains("unauthorized");
                remoteDown(unauthorized
                        ? Component.translatable("daemon.fcw.remote.unauthorized", remoteAddress, delay)
                        : Component.translatable("daemon.fcw.remote.hello_failed", remoteAddress, err, delay),
                        (unauthorized ? "wrong token (daemon.remoteToken)" : "refused the connection (" + err + ")")
                                + ", retrying in " + delay + " s");
                pause(delay);
                continue;
            }
            failures = 0;
            lastRemoteLog = "";
            onHelloOk(r.getHello(), Component.translatable("daemon.fcw.remote.ready", remoteAddress));
            String why;
            try {
                why = closed.get();
            } catch (Exception e) {
                why = e.toString();
            }
            connection = null;
            if (stopping) {
                break;
            }
            final String lost = why;
            mainThreadQueue.add(() -> onRemoteLost(lost));
            pause(1);
        }
    }

    private static String ioMessage(IOException e) {
        String m = e.getMessage();
        return m == null || m.isBlank() ? e.getClass().getSimpleName() : m;
    }

    private String lastRemoteLog = "";

    /** Not connected (yet): visible in the GUI and /fcw status; logged (in English) when it changes. */
    private void remoteDown(Component why, String logText) {
        if (stopping) {
            return;
        }
        if (!logText.equals(lastRemoteLog)) {
            lastRemoteLog = logText;
            FcwMod.LOGGER.warn("Remote VM daemon {}: {}", remoteAddress, logText);
        }
        if (status != Status.READY) {
            status = Status.FAILED;
            reason = why;
        }
    }

    /** Server thread: the TCP connection to the remote daemon dropped; the connector reconnects. */
    private void onRemoteLost(String why) {
        if (stopping || status != Status.READY) {
            return;
        }
        FcwMod.LOGGER.error("Lost the connection to the remote VM daemon {}: {}", remoteAddress, why);
        status = Status.FAILED;
        reason = Component.translatable("daemon.fcw.remote.lost", remoteAddress, why);
        failAllPending(reason.getString());
        // The machines keep running on the host: only forget what we knew, resync after reconnecting.
        ComputerTracker.onDaemonDisconnected();
    }

    private void pause(int seconds) {
        try {
            Thread.sleep(seconds * 1000L);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    public boolean isRemote() {
        return remote;
    }

    public String remoteAddress() {
        return remoteAddress;
    }

    private static void startPump(InputStream in, String name, Consumer<String> onLine, Runnable onEof) {
        Thread t = new Thread(() -> {
            try (BufferedReader r = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
                String line;
                while ((line = r.readLine()) != null) {
                    onLine.accept(line);
                }
            } catch (IOException ignored) {
            } finally {
                onEof.run();
            }
        }, name);
        t.setDaemon(true);
        t.start();
    }

    private void onInbound(Envelope env) {
        if (env.getBodyCase() == Envelope.BodyCase.RESPONSE) {
            Pending p = pending.get(env.getId());
            if (p != null && p.direct) {
                pending.remove(env.getId());
                p.callback.accept(env.getResponse());
                return;
            }
        }
        inbound.add(env);
    }

    private void onProcessExit(Process ended) {
        if (process != ended) {
            return;
        }
        int code = ended.exitValue();
        if (status == Status.STOPPED) {
            FcwMod.LOGGER.info("VM daemon exited with code {}", code);
            return;
        }
        String tail;
        synchronized (stderrTail) {
            tail = String.join("\n", stderrTail);
        }
        FcwMod.LOGGER.error("VM daemon exited unexpectedly with code {}", code);
        ControlConnection c = connection;
        if (c != null) {
            c.close();
        }
        fail(Status.FAILED, Component.translatable("daemon.fcw.exited", code, lastLine(tail)));
    }

    private void onConnectionLost(String why) {
        if (status != Status.READY) {
            return;
        }
        FcwMod.LOGGER.error("Lost the control connection to the VM daemon: {}", why);
        fail(Status.FAILED, Component.translatable("daemon.fcw.lost", why));
        Process p = process;
        if (p != null) {
            p.destroy();
        }
    }

    private static String lastLine(String text) {
        String t = text.strip();
        int nl = t.lastIndexOf('\n');
        String last = nl >= 0 ? t.substring(nl + 1) : t;
        return last.length() > 300 ? last.substring(0, 300) + "…" : last;
    }

    private void failLater(Status s, Component why) {
        mainThreadQueue.add(() -> fail(s, why));
    }

    /** Server thread. */
    private void fail(Status s, Component why) {
        boolean wasReady = status == Status.READY;
        status = s;
        reason = why;
        if (s == Status.DISABLED) {
            FcwMod.LOGGER.warn("VM daemon is not available: {}", why.getString());
        } else {
            FcwMod.LOGGER.error("VM daemon failed: {}", why.getString());
        }
        failAllPending(why.getString());
        if (wasReady) {
            ComputerTracker.onDaemonLost(why);
        }
    }

    private void failAllPending(String error) {
        List<Pending> all = new ArrayList<>(pending.values());
        pending.clear();
        Response r = Response.newBuilder().setOk(false).setError(error).build();
        for (Pending p : all) {
            if (p.direct) {
                p.callback.accept(r);
            } else {
                mainThreadQueue.add(() -> p.callback.accept(r));
            }
        }
    }

    private void requestShutdown() {
        Status before = status;
        status = Status.STOPPED;
        reason = Component.translatable("daemon.fcw.stopped");
        stopping = true;
        ControlConnection c = connection;
        if (remote) {
            // A daemon service outlives this server: stop only this world's machines.
            if (before == Status.READY && c != null && !c.isClosed()) {
                stopOwnMachines(c);
            }
            return;
        }
        if (before == Status.READY && c != null && !c.isClosed()) {
            FcwMod.LOGGER.info("Asking the VM daemon to shut down");
            c.send(Envelope.newBuilder().setId(ids.incrementAndGet())
                    .setRequest(Request.newBuilder().setShutdown(Shutdown.getDefaultInstance())).build());
            c.flush(1000);
        }
    }

    /** Soft StopVm for every machine of this world; responses are counted on the reader thread. */
    private void stopOwnMachines(ControlConnection c) {
        Set<String> vmIds = new LinkedHashSet<>(FcwWorldData.get(server).machines());
        for (ComputerBlockEntity be : ComputerTracker.all()) {
            if (be.isMachineOn()) {
                vmIds.add(be.vmIdString());
            }
        }
        if (vmIds.isEmpty()) {
            return;
        }
        int timeoutMs = FcwConfig.SERVER.shutdownTimeoutSeconds.get() * 1000;
        CountDownLatch latch = new CountDownLatch(vmIds.size());
        remoteStops = latch;
        FcwMod.LOGGER.info("Stopping {} machine(s) of this world on the remote VM daemon", vmIds.size());
        for (String id : vmIds) {
            long rid = ids.incrementAndGet();
            pending.put(rid, new Pending(System.currentTimeMillis() + timeoutMs + 10_000L, r -> latch.countDown(), true));
            c.send(Envelope.newBuilder().setId(rid).setRequest(Request.newBuilder()
                    .setStopVm(StopVm.newBuilder().setVmId(id).setForce(false).setTimeoutMs(timeoutMs))).build());
        }
    }

    private void stopNow() {
        if (status != Status.STOPPED) {
            requestShutdown();
        }
        stopping = true;
        if (remote) {
            CountDownLatch latch = remoteStops;
            if (latch != null) {
                try {
                    if (!latch.await(FcwConfig.SERVER.shutdownTimeoutSeconds.get() + 5L, TimeUnit.SECONDS)) {
                        FcwMod.LOGGER.warn("Not all machines confirmed the shutdown; the daemon finishes it on its own");
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }
            ControlConnection c = connection;
            if (c != null) {
                c.close();
            }
            Thread t = connector;
            if (t != null) {
                t.interrupt();
            }
            pending.clear();
            return;
        }
        Process p = process;
        if (p != null) {
            long timeout = FcwConfig.SERVER.shutdownTimeoutSeconds.get();
            try {
                if (!p.waitFor(timeout, TimeUnit.SECONDS)) {
                    FcwMod.LOGGER.warn("VM daemon did not exit in {} s, terminating it", timeout);
                }
                try {
                    p.getOutputStream().close(); // closing stdin also tells the daemon to exit
                } catch (IOException ignored) {
                }
                if (p.isAlive()) {
                    p.destroy();
                    if (!p.waitFor(5, TimeUnit.SECONDS)) {
                        p.destroyForcibly();
                    }
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                p.destroyForcibly();
            }
        }
        ControlConnection c = connection;
        if (c != null) {
            c.close();
        }
        Path dir = controlDir;
        if (dir != null) {
            try (Stream<Path> files = Files.walk(dir)) {
                files.sorted(Comparator.reverseOrder()).forEach(f -> {
                    try {
                        Files.deleteIfExists(f);
                    } catch (IOException ignored) {
                    }
                });
            } catch (IOException ignored) {
            }
        }
        pending.clear();
    }

    private static void sleep(long ms) {
        try {
            Thread.sleep(ms);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    // ------------------------------------------------------------------ server thread

    /** Called every server tick (ServerTickEvent.Post). */
    public void tick() {
        Runnable r;
        while ((r = mainThreadQueue.poll()) != null) {
            try {
                r.run();
            } catch (RuntimeException e) {
                FcwMod.LOGGER.error("Error in VM daemon callback", e);
            }
        }
        int budget = FcwConfig.SERVER.messagesPerTick.get();
        Envelope env;
        while (budget-- > 0 && (env = inbound.poll()) != null) {
            try {
                dispatch(env);
            } catch (RuntimeException e) {
                FcwMod.LOGGER.error("Error while handling a VM daemon message", e);
            }
        }
        if (++tickCount % 20 == 0) {
            expirePending();
        }
    }

    private void dispatch(Envelope env) {
        switch (env.getBodyCase()) {
            case RESPONSE -> {
                Pending p = pending.remove(env.getId());
                if (p != null) {
                    p.callback.accept(env.getResponse());
                }
            }
            case EVENT -> handleEvent(env.getEvent());
            default -> {
            }
        }
    }

    private void handleEvent(Event e) {
        switch (e.getKindCase()) {
            case LOG -> {
                String level = e.getLog().getLevel().toLowerCase();
                String msg = "[fcw-daemon] " + e.getLog().getMessage();
                switch (level) {
                    case "error" -> FcwMod.LOGGER.error(msg);
                    case "warn", "warning" -> FcwMod.LOGGER.warn(msg);
                    case "debug", "trace" -> FcwMod.LOGGER.debug(msg);
                    default -> FcwMod.LOGGER.info(msg);
                }
            }
            case IMAGE_PROGRESS -> {
                if (QEMU_ID.equals(e.getImageProgress().getImageId())) {
                    onQemuProgress(e.getImageProgress());
                } else if (e.getImageProgress().getFinished()) {
                    refreshImages();
                }
            }
            default -> ComputerTracker.handleEvent(e);
        }
        for (Consumer<Event> l : listeners) {
            l.accept(e);
        }
    }

    private void expirePending() {
        long now = System.currentTimeMillis();
        pending.entrySet().removeIf(entry -> {
            Pending p = entry.getValue();
            if (p.deadline < now) {
                Response r = Response.newBuilder().setOk(false).setError("timeout").build();
                mainThreadQueue.add(() -> p.callback.accept(r));
                return true;
            }
            return false;
        });
    }

    /** Sends a request; the callback runs on the server thread (also when the daemon is unavailable). */
    public void request(Request req, Consumer<Response> callback) {
        request(req, DEFAULT_TIMEOUT_MS, callback);
    }

    public void request(Request req, long timeoutMs, Consumer<Response> callback) {
        ControlConnection c = connection;
        if (status != Status.READY || c == null || c.isClosed()) {
            Response r = Response.newBuilder().setOk(false).setError(reason.getString()).build();
            mainThreadQueue.add(() -> callback.accept(r));
            return;
        }
        long id = ids.incrementAndGet();
        pending.put(id, new Pending(System.currentTimeMillis() + timeoutMs, callback, false));
        c.send(Envelope.newBuilder().setId(id).setRequest(req).build());
    }

    /** Sends a request without waiting for its response. */
    public void send(Request req) {
        request(req, r -> {
            if (!r.getOk() && status == Status.READY) {
                FcwMod.LOGGER.debug("VM daemon request {} failed: {}", req.getKindCase(), r.getError());
            }
        });
    }

    public void addListener(Consumer<Event> listener) {
        listeners.add(listener);
    }

    public void removeListener(Consumer<Event> listener) {
        listeners.remove(listener);
    }

    public void refreshImages() {
        refreshImages(null);
    }

    /** Reloads the image list; {@code then} runs afterwards on the server thread (also on failure). */
    public void refreshImages(@Nullable Runnable then) {
        if (status != Status.READY || (imagesLoading && then == null)) {
            if (then != null) {
                mainThreadQueue.add(then);
            }
            return;
        }
        imagesLoading = true;
        request(Request.newBuilder().setListImages(ListImages.getDefaultInstance()).build(), r -> {
            imagesLoading = false;
            imagesUpdatedAt = System.currentTimeMillis();
            if (r.getOk() && r.hasImages()) {
                images = List.copyOf(r.getImages().getImagesList());
                imagesError = Component.empty();
            } else {
                imagesError = Component.literal(r.getError());
            }
            if (then != null) {
                then.run();
            }
        });
    }

    /** Refreshes the image list if it is older than {@code maxAgeMs}. */
    public void refreshImagesIfOlder(long maxAgeMs) {
        if (System.currentTimeMillis() - imagesUpdatedAt > maxAgeMs) {
            refreshImages();
        }
    }


    // ------------------------------------------------------------------ portable QEMU

    /** This daemon build knows --qemu-dir / FetchQemu / HelloResult.qemu_* (judged by its --help). */
    public boolean qemuSupported() {
        // A remote (service) daemon is newer than the portable QEMU feature; a local one is asked via --help.
        return remote || daemonHelp.contains("--qemu-dir");
    }

    /** The daemon reported that it has no QEMU (only meaningful for daemons that report it). */
    public boolean qemuMissing() {
        HelloResult h = hello;
        return status == Status.READY && h != null && qemuSupported() && !h.getQemuAvailable();
    }

    public boolean qemuPlatformPublished() {
        return FcwConfig.SERVER.qemuPlatforms.get().stream().anyMatch(p -> p.trim().equals(Platform.id()));
    }

    public String qemuUrl() {
        return FcwConfig.SERVER.qemuUrlTemplate.get().replace("<platform>", Platform.id()).replace("{platform}", Platform.id());
    }

    public Path qemuDir() {
        return qemuDir;
    }

    public QemuFetch qemuFetchState() {
        return qemuFetch;
    }

    public Component qemuFetchError() {
        return qemuFetchError;
    }

    /** Download progress 0..100, or -1 if the size is unknown. */
    public int qemuPercent() {
        return qemuTotal > 0 ? (int) Math.min(100, qemuDone * 100 / qemuTotal) : -1;
    }

    public long qemuDoneMb() {
        return qemuDone >> 20;
    }

    public void addQemuListener(Consumer<Component> l) {
        qemuListeners.add(l);
    }

    /** Short state of the QEMU download for the GUI and /fcw status, e.g. "downloading QEMU: 34%". */
    public Component qemuProgressText() {
        int p = qemuPercent();
        return p >= 0 ? Component.translatable("daemon.fcw.qemu.downloading", p)
                : Component.translatable("daemon.fcw.qemu.downloading_mb", qemuDoneMb());
    }

    /** Why machines cannot start for lack of QEMU, or null if QEMU is there (or the daemon cannot tell). */
    @Nullable
    public Component qemuProblem() {
        if (!qemuMissing()) {
            return null;
        }
        return switch (qemuFetch) {
            case RUNNING -> Component.translatable("gui.fcw.error.qemu_downloading", qemuProgressText());
            case FAILED -> Component.translatable("gui.fcw.error.qemu_failed", qemuFetchError);
            default -> qemuPlatformPublished()
                    ? Component.translatable("gui.fcw.error.qemu_missing")
                    : Component.translatable("gui.fcw.error.qemu_missing_platform", Platform.id());
        };
    }

    private void logQemu() {
        HelloResult h = hello;
        if (h == null) {
            return;
        }
        if (!qemuSupported()) {
            FcwMod.LOGGER.info("This VM daemon does not report QEMU (older build): portable QEMU download is unavailable");
        } else if (h.getQemuAvailable()) {
            FcwMod.LOGGER.info("QEMU {} at {} ({})", h.getQemuVersion(), h.getQemuPath(), h.getQemuBundled() ? "portable" : "system");
        } else {
            FcwMod.LOGGER.warn("The VM daemon found no QEMU (qemu-system-x86_64)");
        }
    }

    /**
     * Asks the daemon to download the portable QEMU into --qemu-dir.
     * @return null if the download started (or is running), otherwise why it cannot
     */
    @Nullable
    public Component fetchQemu() {
        if (status != Status.READY) {
            return Component.translatable("gui.fcw.error.daemon", reason);
        }
        if (!qemuSupported()) {
            return Component.translatable("daemon.fcw.qemu.unsupported");
        }
        if (!qemuPlatformPublished()) {
            return Component.translatable("gui.fcw.error.qemu_missing_platform", Platform.id());
        }
        if (qemuFetch == QemuFetch.RUNNING) {
            return null;
        }
        String url = qemuUrl();
        FcwMod.LOGGER.info("Downloading portable QEMU from {}", url);
        qemuFetch = QemuFetch.RUNNING;
        qemuFetchError = Component.empty();
        qemuDone = 0;
        qemuTotal = 0;
        qemuLoggedStep = -1;
        notifyQemu(Component.translatable("daemon.fcw.qemu.fetch_start", url));
        request(Request.newBuilder().setFetchQemu(FetchQemu.newBuilder().setUrl(url)).build(), 60 * 60_000L, r -> {
            if (!r.getOk() && qemuFetch == QemuFetch.RUNNING) {
                qemuFailed(r.getError().isEmpty() ? "unknown error" : r.getError());
            }
        });
        return null;
    }

    private void onQemuProgress(ImageProgress p) {
        if (!p.getError().isEmpty()) {
            qemuFailed(p.getError());
            return;
        }
        qemuDone = p.getDoneBytes();
        qemuTotal = p.getTotalBytes();
        if (p.getFinished()) {
            qemuFetch = QemuFetch.DONE;
            FcwMod.LOGGER.info("Portable QEMU downloaded");
            rehello();
            return;
        }
        qemuFetch = QemuFetch.RUNNING;
        int percent = qemuPercent();
        int step = percent >= 0 ? percent / 10 : (int) (qemuDone >> 26); // every 10% or 64 MB
        if (step > qemuLoggedStep) {
            qemuLoggedStep = step;
            Component msg = qemuProgressText();
            FcwMod.LOGGER.info("QEMU: {}", msg.getString());
            notifyQemu(msg);
        }
    }

    /** "HTTP: HTTP status client error (404 Not Found) for url (...)" -> "HTTP 404 Not Found". */
    static String shortError(String e) {
        String s = e.replaceAll(" for url \\(.*\\)\\s*$", "").trim();
        java.util.regex.Matcher m = java.util.regex.Pattern.compile("\\((\\d{3} [^)]*)\\)").matcher(s);
        if (s.startsWith("HTTP") && m.find()) {
            return "HTTP " + m.group(1);
        }
        return s.length() > 100 ? s.substring(0, 100) + "…" : s;
    }

    private void qemuFailed(String error) {
        qemuFetch = QemuFetch.FAILED;
        qemuFetchError = Component.literal(shortError(error));
        FcwMod.LOGGER.error("Downloading portable QEMU failed: {}", error);
        notifyQemu(Component.translatable("gui.fcw.error.qemu_failed", qemuFetchError));
        qemuListeners.clear();
    }

    /** Asks Hello again so that qemu_* reflect the new QEMU, then resyncs computers waiting for it. */
    private void rehello() {
        request(Request.newBuilder().setHello(Hello.newBuilder().setClientName("fcw-mod " + FcwMod.version())
                .setProtocolVersion(PROTOCOL_VERSION)).build(), r -> {
            if (r.getOk() && r.hasHello()) {
                hello = r.getHello();
            }
            logQemu();
            HelloResult h = hello;
            if (h != null && h.getQemuAvailable()) {
                notifyQemu(Component.translatable("daemon.fcw.qemu.ready", h.getQemuVersion(), h.getQemuPath()));
                qemuListeners.clear();
                ComputerTracker.onDaemonReady();
            } else {
                qemuFailed("QEMU is still not found after the download");
            }
        });
    }

    private void notifyQemu(Component msg) {
        for (Consumer<Component> l : qemuListeners) {
            l.accept(msg);
        }
    }

    // ------------------------------------------------------------------ getters

    public Status status() {
        return status;
    }

    public Component reason() {
        return reason;
    }

    @Nullable
    public HelloResult hello() {
        return hello;
    }

    public String accelerator() {
        String a = accelerator;
        return a == null ? "" : a;
    }

    public List<ImageInfo> images() {
        return images;
    }

    public Component imagesError() {
        return imagesError;
    }

    @Nullable
    public ImageInfo image(String id) {
        for (ImageInfo i : images) {
            if (i.getId().equals(id)) {
                return i;
            }
        }
        return null;
    }

    /** DNS zone of the guest network ("mc"), empty if the daemon has no network. */
    public String dnsZone() {
        HelloResult h = hello;
        return h == null || !h.getNetworkEnabled() ? "" : h.getDnsZone();
    }

    public boolean networkEnabled() {
        HelloResult h = hello;
        return h != null && h.getNetworkEnabled();
    }

    public int hostCpus() {
        HelloResult h = hello;
        return h == null ? 0 : h.getHostCpus();
    }

    public Path imagesDir() {
        return imagesDir;
    }

    public Path vmsDir() {
        return vmsDir;
    }

    @Nullable
    public Path daemonBinary() {
        return daemonBinary;
    }

    public MinecraftServer server() {
        return server;
    }

    public int pendingCount() {
        return pending.size();
    }
}
