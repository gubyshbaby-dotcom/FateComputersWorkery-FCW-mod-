package com.fatecomputers.fcw.client.view;

import com.fatecomputers.fcw.FcwConfig;
import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.block.entity.MonitorBlockEntity;
import com.fatecomputers.fcw.client.gui.DisplayScreen;
import com.fatecomputers.fcw.client.nativelib.FcwNative;
import com.fatecomputers.fcw.client.nativelib.NativeLoader;
import com.fatecomputers.fcw.network.ScreenAccessPayload;
import com.fatecomputers.fcw.network.ScreenAccessRequestPayload;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.network.PacketDistributor;

import javax.annotation.Nullable;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Client side live screens. For every powered monitor in range it asks the server for a view
 * token, connects to the daemon over QUIC (one session per daemon endpoint), attaches the token,
 * opens a view and every frame copies the changed pixels into a texture. Views of machines that
 * are no longer needed are closed. The fullscreen {@link DisplayScreen} uses the same machinery
 * with a control token.
 */
public final class ScreenViewManager {
    private static final ScreenViewManager INSTANCE = new ScreenViewManager();

    private static final long LINGER_MS = 5_000;
    private static final long REQUEST_TIMEOUT_MS = 10_000;
    private static final long RETRY_MS = 8_000;
    private static final long ATTACH_FALLBACK_MS = 2_000;
    private static final long SESSION_IDLE_MS = 30_000;
    private static final int MAX_RECTS = 1024;

    public enum LinkState { REQUESTING, ATTACHING, ATTACHED, FAILED }

    static final class Session {
        final String key;
        final long handle;
        final Set<String> vms = new java.util.HashSet<>();
        long idleSince;

        Session(String key, long handle) {
            this.key = key;
            this.handle = handle;
        }
    }

    public static final class Link {
        final String vmId;
        LinkState state = LinkState.REQUESTING;
        long stateTime = System.currentTimeMillis();
        @Nullable
        Session session;
        int permissions;
        List<ScreenAccessPayload.Screen> screens = List.of();
        String screenId = "";
        long view;
        int viewFps;
        long viewOpenedAt;
        int wantedFps;
        int lastWantedFps;
        long lastWanted;
        long lastVisible;
        boolean wantedByDisplay;
        /** This client holds the machine's input (InputControl from the daemon); optimistic after a control token. */
        boolean inputActive = true;
        String controllerName = "";
        @Nullable
        BlockPos requestPos;
        Component error = Component.empty();
        long retryAt;
        final int[] info = new int[7];
        final ViewTexture texture;

        Link(String vmId) {
            this.vmId = vmId;
            this.texture = new ViewTexture(ResourceLocation.fromNamespaceAndPath(FcwMod.MODID, "screen/" + vmId));
        }

        public LinkState state() {
            return state;
        }

        public Component error() {
            return error;
        }

        public int permissions() {
            return permissions;
        }

        public String vmId() {
            return vmId;
        }

        public boolean inputActive() {
            return inputActive;
        }

        public String controllerName() {
            return controllerName;
        }

        public long sessionHandle() {
            return session == null ? 0 : session.handle;
        }

        public ViewTexture texture() {
            return texture;
        }

        public String screenId() {
            return screenId;
        }

        public boolean hasView() {
            return view != 0;
        }

        /** Guest screen size in pixels (0 until the first frame). */
        public int guestWidth() {
            return info[0];
        }

        public int guestHeight() {
            return info[1];
        }

        void setState(LinkState s) {
            state = s;
            stateTime = System.currentTimeMillis();
        }
    }

    private final Map<String, Session> sessions = new HashMap<>();
    private final Map<String, Link> links = new HashMap<>();
    private final Set<MonitorBlockEntity> monitors = Collections.newSetFromMap(new IdentityHashMap<>());
    private final int[] rects = new int[MAX_RECTS * 4];
    @Nullable
    private String pendingDisplayVm;

    private ScreenViewManager() {
    }

    public static ScreenViewManager get() {
        return INSTANCE;
    }

    // ------------------------------------------------------------------ monitors

    public void monitorLoaded(MonitorBlockEntity be) {
        monitors.add(be);
    }

    public void monitorUnloaded(MonitorBlockEntity be) {
        monitors.remove(be);
    }

    @Nullable
    public Link link(String vmId) {
        return links.get(vmId);
    }

    /** Texture for a monitor if a live frame is available, otherwise null (the model picture stays). */
    @Nullable
    public ViewTexture textureFor(MonitorBlockEntity be) {
        if (be.vmId() == null) {
            return null;
        }
        Link l = links.get(be.vmId().toString());
        if (l == null || l.view == 0 || !l.texture.hasFrame()) {
            return null;
        }
        l.lastVisible = System.currentTimeMillis();
        return l.texture;
    }

    // ------------------------------------------------------------------ tick

    public void tick() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || mc.player == null) {
            if (!links.isEmpty() || !sessions.isEmpty()) {
                closeAll();
            }
            return;
        }
        if (!NativeLoader.isLoaded()) {
            return;
        }
        long now = System.currentTimeMillis();
        pollSessions(now);

        // Which machines do the monitors around us want to show?
        Vec3 eye = mc.player.getEyePosition();
        int range = FcwConfig.CLIENT.screenRenderDistance.get();
        int farDistance = FcwConfig.CLIENT.farDistance.get();
        for (Iterator<MonitorBlockEntity> it = monitors.iterator(); it.hasNext(); ) {
            MonitorBlockEntity m = it.next();
            if (m.isRemoved() || m.getLevel() != mc.level) {
                it.remove();
                continue;
            }
            if (!m.isScreenOn() || m.vmId() == null) {
                continue;
            }
            double dist = Math.sqrt(m.getBlockPos().getCenter().distanceToSqr(eye));
            if (dist > range) {
                continue;
            }
            int fps = dist > farDistance ? FcwConfig.CLIENT.farFps.get() : FcwConfig.CLIENT.nearFps.get();
            want(m.vmId().toString(), m.getBlockPos(), fps, now);
        }

        for (Iterator<Link> it = links.values().iterator(); it.hasNext(); ) {
            Link l = it.next();
            if (l.wantedByDisplay) {
                l.lastWanted = now;
                l.wantedFps = Math.max(l.wantedFps, FcwConfig.CLIENT.controlFps.get());
            }
            if (now - l.lastWanted > LINGER_MS) {
                closeLink(l);
                it.remove();
                continue;
            }
            switch (l.state) {
                case REQUESTING -> {
                    if (now - l.stateTime > REQUEST_TIMEOUT_MS) {
                        fail(l, Component.translatable("screen.fcw.error.no_answer"));
                    }
                }
                case ATTACHING -> {
                    // Fallback if the library does not report the attach result as an event.
                    if (now - l.stateTime > ATTACH_FALLBACK_MS) {
                        tryOpenView(l);
                    }
                }
                case ATTACHED -> {
                    if (l.view == 0 && now > l.retryAt) {
                        tryOpenView(l);
                    } else if (l.view != 0 && shouldReopen(l, now)) {
                        FcwNative.closeView(l.view);
                        l.view = 0;
                        tryOpenView(l);
                    }
                }
                case FAILED -> {
                    if (now > l.retryAt && l.requestPos != null && !l.wantedByDisplay) {
                        request(l, l.requestPos, false);
                    }
                }
            }
            l.lastWantedFps = l.wantedFps;
            l.wantedFps = 0;
        }

        // Close idle sessions.
        for (Iterator<Session> it = sessions.values().iterator(); it.hasNext(); ) {
            Session s = it.next();
            if (s.vms.isEmpty()) {
                if (s.idleSince == 0) {
                    s.idleSince = now;
                } else if (now - s.idleSince > SESSION_IDLE_MS) {
                    FcwNative.close(s.handle);
                    it.remove();
                }
            } else {
                s.idleSince = 0;
            }
        }
    }

    private boolean shouldReopen(Link l, long now) {
        if (now - l.viewOpenedAt < 3_000 || l.wantedFps <= 0) {
            return false;
        }
        // Much faster wanted (got closer / started using it) or much slower for a while.
        return l.wantedFps >= l.viewFps * 2 || (l.wantedFps * 3 <= l.viewFps && now - l.viewOpenedAt > 15_000);
    }

    /** Speakers: sound comes with a view token, keep a minimal view of the machine open. */
    public void wantAudio(String vmId, BlockPos speakerPos) {
        want(vmId, speakerPos, 1, System.currentTimeMillis());
    }

    private void want(String vmId, BlockPos monitorPos, int fps, long now) {
        Link l = links.get(vmId);
        if (l == null) {
            l = new Link(vmId);
            links.put(vmId, l);
            request(l, monitorPos, false);
        }
        if (l.requestPos == null) {
            l.requestPos = monitorPos.immutable();
        }
        l.lastWanted = now;
        l.wantedFps = Math.max(l.wantedFps, fps);
    }

    private void request(Link l, BlockPos pos, boolean control) {
        l.requestPos = pos.immutable();
        l.setState(LinkState.REQUESTING);
        FcwMod.LOGGER.debug("Requesting a view of {} through {}", l.vmId, pos);
        PacketDistributor.sendToServer(new ScreenAccessRequestPayload(pos, control));
    }

    private void fail(Link l, Component why) {
        if (l.state != LinkState.FAILED || !why.getString().equals(l.error.getString())) {
            FcwMod.LOGGER.info("Live screen of {} unavailable: {}", l.vmId, why.getString());
        }
        l.error = why;
        l.setState(LinkState.FAILED);
        l.retryAt = System.currentTimeMillis() + RETRY_MS;
        if (l.view != 0) {
            FcwNative.closeView(l.view);
            l.view = 0;
        }
    }

    // ------------------------------------------------------------------ server answers

    /** The player used a monitor/keyboard: the answer opens the fullscreen display. */
    public void requestControl(BlockPos pos) {
        PacketDistributor.sendToServer(new ScreenAccessRequestPayload(pos, true));
    }

    public void onScreenAccess(ScreenAccessPayload p) {
        Minecraft mc = Minecraft.getInstance();
        boolean control = p.kind() == ScreenAccessPayload.KIND_CONTROL;
        if (!p.ok()) {
            if (control) {
                if (mc.player != null) {
                    mc.player.displayClientMessage(p.error(), true);
                }
            } else {
                for (Link l : links.values()) {
                    if (l.state == LinkState.REQUESTING && p.pos().equals(l.requestPos)) {
                        fail(l, p.error());
                    }
                }
            }
            return;
        }
        if (control && !NativeLoader.isLoaded()) {
            // Still open the display: it explains why there is no picture.
            openDisplay(mc, p);
            return;
        }
        if (!NativeLoader.isLoaded()) {
            return;
        }
        Link l = links.get(p.vmId());
        if (l == null) {
            if (!control) {
                return; // nobody wants it any more
            }
            l = new Link(p.vmId());
            l.requestPos = p.pos().immutable();
            links.put(p.vmId(), l);
        }
        l.lastWanted = System.currentTimeMillis();
        l.screens = p.screens();
        if (control) {
            l.permissions |= p.permissions();
            l.wantedByDisplay = true;
            // Presenting a control token makes us the controller (the daemon confirms with InputControl).
            l.inputActive = true;
            l.controllerName = "";
        } else {
            l.permissions |= p.permissions() & ScreenAccessPayload.PERM_VIEW;
        }
        String host = p.host().isEmpty() ? serverHost() : p.host();
        Session s = session(host, p.quicPort(), p.certSha256());
        if (l.session != null && l.session != s) {
            if (l.view != 0) {
                FcwNative.closeView(l.view);
                l.view = 0;
            }
            l.session.vms.remove(l.vmId);
        }
        l.session = s;
        s.vms.add(l.vmId);
        FcwNative.attach(s.handle, p.token());
        if (l.view == 0) {
            l.setState(LinkState.ATTACHING);
        }
        if (control) {
            openDisplay(mc, p);
        }
    }

    private static void openDisplay(Minecraft mc, ScreenAccessPayload p) {
        if (mc.screen instanceof DisplayScreen ds && ds.vmId().equals(p.vmId())) {
            ds.grant(p.permissions()); // took the control back from inside the display
        } else {
            mc.setScreen(new DisplayScreen(p.vmId(), p.permissions(), p.pos()));
        }
    }

    private Session session(String host, int port, String cert) {
        String key = host + "|" + port + "|" + cert;
        Session s = sessions.get(key);
        if (s != null) {
            int st = FcwNative.sessionState(s.handle);
            if (st == 2 || st == 3) {
                FcwNative.close(s.handle);
                sessions.remove(key);
                s = null;
            }
        }
        if (s == null) {
            FcwMod.LOGGER.info("Connecting to the VM daemon at {}:{} (QUIC)", host, port);
            s = new Session(key, FcwNative.connect(host, port, cert));
            sessions.put(key, s);
        }
        return s;
    }

    /** Address the client used to join this Minecraft server (127.0.0.1 in single player). */
    public static String serverHost() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.hasSingleplayerServer()) {
            return "127.0.0.1";
        }
        ClientPacketListener conn = mc.getConnection();
        if (conn != null) {
            SocketAddress a = conn.getConnection().getRemoteAddress();
            if (a instanceof InetSocketAddress isa) {
                InetAddress ia = isa.getAddress();
                return ia != null ? ia.getHostAddress() : isa.getHostString();
            }
        }
        ServerData sd = mc.getCurrentServer();
        if (sd != null) {
            return ServerAddress.parseString(sd.ip).getHost();
        }
        return "127.0.0.1";
    }

    // ------------------------------------------------------------------ native events

    private void pollSessions(long now) {
        for (Iterator<Session> it = sessions.values().iterator(); it.hasNext(); ) {
            Session s = it.next();
            String json;
            int guard = 0;
            while (guard++ < 256 && (json = FcwNative.pollEvent(s.handle)) != null) {
                handleEvent(s, json);
            }
            int st = FcwNative.sessionState(s.handle);
            if (st == 2 || st == 3) {
                String err = st == 2 ? FcwNative.sessionError(s.handle) : "closed";
                FcwMod.LOGGER.warn("VM daemon session {} ended: {}", s.key, err);
                for (String vm : new ArrayList<>(s.vms)) {
                    Link l = links.get(vm);
                    if (l != null && l.session == s) {
                        l.session = null;
                        if (l.view != 0) {
                            FcwNative.closeView(l.view);
                            l.view = 0;
                        }
                        fail(l, Component.translatable("screen.fcw.error.connection", err == null ? "" : err));
                    }
                }
                FcwNative.close(s.handle);
                it.remove();
            }
        }
    }

    private static String str(JsonObject o, String... keys) {
        for (String k : keys) {
            JsonElement e = o.get(k);
            if (e != null && e.isJsonPrimitive()) {
                return e.getAsString();
            }
        }
        return "";
    }

    private void handleEvent(Session s, String json) {
        JsonObject o;
        try {
            o = JsonParser.parseString(json).getAsJsonObject();
        } catch (RuntimeException e) {
            FcwMod.LOGGER.debug("Unreadable event from the FCW client library: {}", json);
            return;
        }
        String type = str(o, "type", "event", "kind");
        String vm = str(o, "vm_id", "vmId", "vm");
        switch (type) {
            case "attach", "attach_result" -> {
                boolean ok = !o.has("ok") || o.get("ok").getAsBoolean();
                String error = str(o, "error", "message");
                List<ScreenAccessPayload.Screen> attached = screensOf(o);
                for (Link l : linksOf(s, vm)) {
                    if (ok && !attached.isEmpty()) {
                        l.screens = attached;
                    }
                    if (ok) {
                        if (o.has("permissions") && o.get("permissions").isJsonPrimitive()) {
                            l.permissions |= o.get("permissions").getAsInt();
                        }
                        if (l.state != LinkState.ATTACHED || l.view == 0) {
                            l.setState(LinkState.ATTACHED);
                            tryOpenView(l);
                        } else if (!chooseScreen(l.screens).equals(l.screenId)) {
                            // the real screen list differs from the guess (e.g. a graphics card after all)
                            FcwNative.closeView(l.view);
                            l.view = 0;
                            tryOpenView(l);
                        }
                    } else if (l.view == 0) {
                        fail(l, Component.translatable("screen.fcw.error.attach", error));
                    }
                }
            }
            case "error" -> {
                String error = str(o, "message", "error");
                FcwMod.LOGGER.warn("FCW client library error{}: {}", vm.isEmpty() ? "" : " (" + vm + ")", error);
                if (!vm.isEmpty()) {
                    Link l = links.get(vm);
                    if (l != null && l.session == s && l.view == 0) {
                        fail(l, Component.literal(error));
                    }
                }
            }
            case "input_control" -> {
                Link l = links.get(vm);
                if (l != null && l.session == s) {
                    l.inputActive = !o.has("active") || o.get("active").getAsBoolean();
                    l.controllerName = str(o, "controller_player", "controller");
                    FcwMod.LOGGER.debug("Input of {}: {} (controller {})", vm, l.inputActive ? "ours" : "lost", l.controllerName);
                }
            }
            case "screen_changed" -> {
                Link l = links.get(vm);
                if (l == null || l.session != s) {
                    return;
                }
                JsonObject screen = o.has("screen") && o.get("screen").isJsonObject() ? o.getAsJsonObject("screen") : o;
                String sid = str(screen, "screen_id", "screenId", "id");
                boolean removed = o.has("removed") && o.get("removed").getAsBoolean();
                updateScreens(l, sid, screen, removed);
            }
            default -> {
                // vm_state and anything newer: the block states already tell us whether the machine runs.
            }
        }
    }

    private static List<ScreenAccessPayload.Screen> screensOf(JsonObject o) {
        List<ScreenAccessPayload.Screen> out = new ArrayList<>();
        if (o.has("screens") && o.get("screens").isJsonArray()) {
            for (JsonElement e : o.getAsJsonArray("screens")) {
                if (!e.isJsonObject()) {
                    continue;
                }
                JsonObject so = e.getAsJsonObject();
                String id = str(so, "id", "screen_id", "screenId");
                if (id.isEmpty()) {
                    continue;
                }
                int kind = so.has("kind") && so.get("kind").isJsonPrimitive() ? parseKind(so.get("kind").getAsString()) : 0;
                int w = so.has("width") ? so.get("width").getAsInt() : 0;
                int h = so.has("height") ? so.get("height").getAsInt() : 0;
                out.add(new ScreenAccessPayload.Screen(id, kind, w, h));
            }
        }
        return out;
    }

    private List<Link> linksOf(Session s, String vm) {
        List<Link> out = new ArrayList<>();
        if (!vm.isEmpty()) {
            Link l = links.get(vm);
            if (l != null && l.session == s) {
                out.add(l);
            }
        } else {
            for (Link l : links.values()) {
                if (l.session == s && l.state == LinkState.ATTACHING) {
                    out.add(l);
                }
            }
        }
        return out;
    }

    private void updateScreens(Link l, String sid, JsonObject screen, boolean removed) {
        if (sid.isEmpty()) {
            return;
        }
        List<ScreenAccessPayload.Screen> list = new ArrayList<>(l.screens);
        list.removeIf(x -> x.id().equals(sid));
        if (!removed) {
            int kind = screen.has("kind") && screen.get("kind").isJsonPrimitive() ? parseKind(screen.get("kind").getAsString()) : (sid.startsWith("gfx") ? 1 : 2);
            int w = screen.has("width") ? screen.get("width").getAsInt() : 0;
            int h = screen.has("height") ? screen.get("height").getAsInt() : 0;
            list.add(new ScreenAccessPayload.Screen(sid, kind, w, h));
        }
        l.screens = list;
        String best = chooseScreen(list);
        if (!best.equals(l.screenId) && l.view != 0) {
            FcwNative.closeView(l.view);
            l.view = 0;
            tryOpenView(l);
        }
    }

    private static int parseKind(String k) {
        return switch (k) {
            case "1", "graphical", "SCREEN_KIND_GRAPHICAL" -> 1;
            case "2", "text", "SCREEN_KIND_TEXT" -> 2;
            default -> 0;
        };
    }

    /** Graphical screen if there is one (machine with a graphics card), otherwise the text console. */
    private static String chooseScreen(List<ScreenAccessPayload.Screen> screens) {
        for (ScreenAccessPayload.Screen s : screens) {
            if (s.kind() == 1 || s.id().startsWith("gfx")) {
                return s.id();
            }
        }
        for (ScreenAccessPayload.Screen s : screens) {
            return s.id();
        }
        return "tty0";
    }

    private void tryOpenView(Link l) {
        if (l.session == null) {
            return;
        }
        int fps = l.wantedByDisplay ? FcwConfig.CLIENT.controlFps.get()
                : l.wantedFps > 0 ? l.wantedFps
                : l.lastWantedFps > 0 ? l.lastWantedFps : FcwConfig.CLIENT.nearFps.get();
        String screen = chooseScreen(l.screens);
        long v = FcwNative.openView(l.session.handle, l.vmId, screen, fps);
        if (v == 0) {
            l.retryAt = System.currentTimeMillis() + 1_000;
            if (l.state == LinkState.ATTACHING && System.currentTimeMillis() - l.stateTime > REQUEST_TIMEOUT_MS) {
                fail(l, Component.translatable("screen.fcw.error.view"));
            }
            return;
        }
        FcwMod.LOGGER.debug("Opened view {} of {} ({} fps)", screen, l.vmId, fps);
        l.view = v;
        l.viewFps = fps;
        l.viewOpenedAt = System.currentTimeMillis();
        l.screenId = screen;
        l.error = Component.empty();
        l.setState(LinkState.ATTACHED);
    }

    // ------------------------------------------------------------------ frames

    /** Render thread, start of every frame: pull changed pixels of all open views into textures. */
    public void updateTextures() {
        if (links.isEmpty() || !NativeLoader.isLoaded()) {
            return;
        }
        for (Link l : links.values()) {
            if (l.view == 0) {
                continue;
            }
            if (!FcwNative.viewInfo(l.view, l.info)) {
                continue;
            }
            int w = l.info[0];
            int h = l.info[1];
            if (w <= 0 || h <= 0 || (long) w * h > 8192L * 8192L) {
                continue;
            }
            boolean fresh = l.texture.ensureSize(w, h);
            int n = FcwNative.copyDirty(l.view, l.texture.buffer(), rects);
            if (n < 0 || (fresh && n > 0)) {
                // A newly allocated texture needs the whole frame; the library's buffer copy only had the dirty parts.
                l.texture.upload(rects, -1);
            } else if (n > 0) {
                l.texture.upload(rects, Math.min(n, MAX_RECTS));
            }
        }
    }

    // ------------------------------------------------------------------ display screen

    public void displayClosed(String vmId) {
        Link l = links.get(vmId);
        if (l != null) {
            l.wantedByDisplay = false;
            if (l.session != null) {
                FcwNative.releaseAll(l.session.handle, vmId);
            }
        }
    }

    // ------------------------------------------------------------------ shutdown

    private void closeLink(Link l) {
        if (l.view != 0) {
            FcwNative.closeView(l.view);
            l.view = 0;
        }
        if (l.session != null) {
            l.session.vms.remove(l.vmId);
            l.session = null;
        }
        l.texture.close();
    }

    public void closeAll() {
        boolean nativeOk = NativeLoader.isLoaded();
        for (Link l : links.values()) {
            if (nativeOk) {
                closeLink(l);
            } else {
                l.texture.close();
            }
        }
        links.clear();
        if (nativeOk) {
            for (Session s : sessions.values()) {
                FcwNative.close(s.handle);
            }
        }
        sessions.clear();
        monitors.clear();
    }

    public int openViews() {
        int n = 0;
        for (Link l : links.values()) {
            if (l.view != 0) {
                n++;
            }
        }
        return n;
    }
}
