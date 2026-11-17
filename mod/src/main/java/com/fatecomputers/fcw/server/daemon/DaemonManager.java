package com.fatecomputers.fcw.server.daemon;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.proto.ControlProto.Envelope;
import com.fatecomputers.fcw.proto.ControlProto.Event;
import com.fatecomputers.fcw.proto.ControlProto.Hello;
import com.fatecomputers.fcw.proto.ControlProto.HelloResult;
import com.fatecomputers.fcw.proto.ControlProto.ImageInfo;
import com.fatecomputers.fcw.proto.ControlProto.ListImages;
import com.fatecomputers.fcw.proto.ControlProto.Request;
import com.fatecomputers.fcw.proto.ControlProto.Response;
import com.fatecomputers.fcw.proto.ControlProto.Shutdown;
import com.fatecomputers.fcw.server.ComputerTracker;
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
import java.util.concurrent.CompletableFuture;
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

    public static final int PROTOCOL_VERSION = 1;
    public static final String CLIENT_NAME = "fcw-mod " + FcwMod.VERSION;
    public static final long DEFAULT_TIMEOUT_MS = 60_000;

    private static volatile DaemonManager current;

    private final MinecraftServer server;
    private final Path imagesDir;
    private final Path vmsDir;
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
        status = Status.STARTING;
        reason = Component.translatable("daemon.fcw.starting");
        Thread t = new Thread(() -> startupThread(cmd, socket, startupTimeout), "fcw-daemon-start");
        t.setDaemon(true);
        t.start();
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

        CompletableFuture<Response> helloReply = new CompletableFuture<>();
        long id = ids.incrementAndGet();
        pending.put(id, new Pending(System.currentTimeMillis() + 15_000, helloReply::complete, true));
        conn.send(Envelope.newBuilder().setId(id).setRequest(Request.newBuilder()
                .setHello(Hello.newBuilder().setClientName(CLIENT_NAME).setProtocolVersion(PROTOCOL_VERSION))).build());
        Response r;
        try {
            r = helloReply.get(15, TimeUnit.SECONDS);
        } catch (Exception e) {
            pending.remove(id);
            failLater(Status.FAILED, Component.translatable("daemon.fcw.hello_failed", "timeout"));
            conn.close();
            p.destroy();
            return;
        }
        if (!r.getOk() || !r.hasHello()) {
            failLater(Status.FAILED, Component.translatable("daemon.fcw.hello_failed", r.getError()));
            conn.close();
            p.destroy();
            return;
        }
        HelloResult h = r.getHello();
        hello = h;
        if (!h.getAccelerator().isEmpty()) {
            accelerator = h.getAccelerator();
        }
        if (h.getProtocolVersion() != PROTOCOL_VERSION) {
            FcwMod.LOGGER.warn("VM daemon speaks protocol {}, the mod expects {}", h.getProtocolVersion(), PROTOCOL_VERSION);
        }
        FcwMod.LOGGER.info("Connected to VM daemon {} (protocol {}, accelerator {}, QUIC port {}, {} host CPUs, {} MB)",
                h.getDaemonVersion(), h.getProtocolVersion(), accelerator, h.getQuicPort(), h.getHostCpus(), h.getHostMemoryMb());
        mainThreadQueue.add(() -> {
            if (status != Status.STARTING) {
                return;
            }
            status = Status.READY;
            reason = Component.translatable("daemon.fcw.ready");
            refreshImages();
            ComputerTracker.onDaemonReady();
        });
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
        ControlConnection c = connection;
        if (before == Status.READY && c != null && !c.isClosed()) {
            FcwMod.LOGGER.info("Asking the VM daemon to shut down");
            c.send(Envelope.newBuilder().setId(ids.incrementAndGet())
                    .setRequest(Request.newBuilder().setShutdown(Shutdown.getDefaultInstance())).build());
            c.flush(1000);
        }
    }

    private void stopNow() {
        if (status != Status.STOPPED) {
            requestShutdown();
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
                if (e.getImageProgress().getFinished()) {
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
        if (imagesLoading || status != Status.READY) {
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
        });
    }

    /** Refreshes the image list if it is older than {@code maxAgeMs}. */
    public void refreshImagesIfOlder(long maxAgeMs) {
        if (System.currentTimeMillis() - imagesUpdatedAt > maxAgeMs) {
            refreshImages();
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
