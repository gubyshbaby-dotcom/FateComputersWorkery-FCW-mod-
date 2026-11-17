package com.fatecomputers.fcw.production.machine;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;

import javax.annotation.Nullable;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Clip player for {@code <machine>_animations.json}: a port of the author's reference player
 * (EuvAnimation.java in the model archives) plus the particle data (fx, emitters) it leaves out.
 * It only evaluates poses; matrices are 4x4, row-major, in structure units (1/16 block).
 *
 * <p>Several clips may play at once (a work clip over an ambient one): a pose is asked for a list of
 * {@link Cursor}s and every part takes its channels from the first clip that animates it.
 */
public final class MachineAnimation {
    public static final int LINEAR = 0, SMOOTH = 1, STEP = 2;

    public record Key(double t, double v, int ease) {
    }

    public static final class Part {
        public String id;
        @Nullable
        public String parent;
        @Nullable
        public double[] pivot;                 // null when the part only slides
        public char axis = 'y';
        public int[] anchor;                   // cell of the part's model, in blocks
    }

    public static final class Attach {
        public double t, yaw;
        @Nullable
        public String carrier;                 // part id, "world" or null (hidden)
        public double[] offset;
        public boolean lerp;
    }

    public static final class Prop {
        public String id, model;
        public double[] center;
        public final List<Attach> attach = new ArrayList<>();
    }

    public static final class Event {
        public double t;
        public String kind;                    // "state" or "hatch"
        public String arg;                     // state name or hatch id
        public boolean open;                   // for hatches
    }

    /** Particles of one emitter while the clip time is in [t0, t1). */
    public record Fx(String emitter, double t0, double t1, double rate) {
    }

    public static final class Emitter {
        public String id, sprite;
        public double[][][] boxes;             // [box][min/max][xyz], structure space, 1/16 block
        public double rate;                    // particles per second in every box
        public double[][] vel;                 // [min/max][xyz], 1/16 block per second
        public double gravity, drag;
        public double[] life, size;
        public boolean additive;
        public int tint = 0xFFFFFF;
    }

    public static final class Clip {
        public String id, state;
        public double length;
        public boolean loop;
        public final Map<String, Map<String, Key[]>> tracks = new LinkedHashMap<>();
        public final List<Prop> props = new ArrayList<>();
        public final List<Event> events = new ArrayList<>();
        public final List<Fx> fx = new ArrayList<>();
    }

    /** A clip at a moment of its own time. */
    public record Cursor(Clip clip, double time) {
    }

    public final Map<String, Part> parts = new LinkedHashMap<>();
    public final Map<String, Clip> clips = new LinkedHashMap<>();
    public final Map<String, Emitter> emitters = new LinkedHashMap<>();

    // ───────────────────────── evaluation

    /** Time inside the clip for a clock that started when the clip did: wraps for loops, clamps otherwise. */
    public static double clipTime(Clip c, double seconds) {
        if (c.loop) {
            double t = seconds % c.length;
            return t < 0 ? t + c.length : t;
        }
        return Math.max(0, Math.min(seconds, c.length));
    }

    public static double sample(Key[] k, double t) {
        if (t <= k[0].t) {
            return k[0].v;
        }
        for (int i = 0; i + 1 < k.length; i++) {
            if (t < k[i + 1].t) {
                if (k[i].ease == STEP) {
                    return k[i].v;
                }
                double u = (t - k[i].t) / (k[i + 1].t - k[i].t);
                if (k[i].ease == SMOOTH) {
                    u = u * u * (3 - 2 * u);
                }
                return k[i].v + (k[i + 1].v - k[i].v) * u;
            }
        }
        return k[k.length - 1].v;
    }

    /** Part pose relative to its parent: Translate(tx, ty, tz) * RotateAbout(pivot, axis, rot). */
    public double[] localMatrix(List<Cursor> cursors, String part) {
        Part p = parts.get(part);
        double tx = 0, ty = 0, tz = 0, rot = 0;
        for (Cursor c : cursors) {
            Map<String, Key[]> tr = c.clip.tracks.get(part);
            if (tr != null) {
                tx = channel(tr, "tx", c.time);
                ty = channel(tr, "ty", c.time);
                tz = channel(tr, "tz", c.time);
                rot = channel(tr, "rot", c.time);
                break;
            }
        }
        double[] m = translate(tx, ty, tz);
        return p.pivot != null && rot != 0 ? mul(m, rotateAbout(p.pivot, p.axis, rot)) : m;
    }

    private static double channel(Map<String, Key[]> tracks, String ch, double t) {
        Key[] k = tracks.get(ch);
        return k == null ? 0 : sample(k, t);
    }

    /** Part pose in structure space (front = north, before the structure's facing is applied). */
    public double[] partMatrix(List<Cursor> cursors, String part) {
        double[] m = localMatrix(cursors, part);
        String parent = parts.get(part).parent;
        return parent == null ? m : mul(partMatrix(cursors, parent), m);
    }

    /** Pose of a carried thing (wafer, mask, pod) of the first cursor's clip, or null while it is hidden. */
    @Nullable
    public double[] propMatrix(List<Cursor> cursors, Prop prop) {
        double t = cursors.get(0).time;
        int i = -1;
        for (int n = 0; n < prop.attach.size(); n++) {
            if (prop.attach.get(n).t <= t) {
                i = n;
            }
        }
        if (i < 0 || prop.attach.get(i).carrier == null) {
            return null;
        }
        Attach a = prop.attach.get(i);
        double[] off = a.offset.clone();
        double yaw = a.yaw;
        if (a.lerp && i + 1 < prop.attach.size()) {
            Attach n = prop.attach.get(i + 1);
            double u = (t - a.t) / (n.t - a.t);
            for (int j = 0; j < 3; j++) {
                off[j] += (n.offset[j] - off[j]) * u;
            }
            yaw += (n.yaw - yaw) * u;
        }
        double[] m = translate(off[0], off[1], off[2]);
        if (yaw != 0) {
            m = mul(m, rotateAbout(prop.center, 'y', yaw));
        }
        return a.carrier.equals("world") ? m : mul(partMatrix(cursors, a.carrier), m);
    }

    /** Blockstate "state" property the structure should show at this moment of the clip. */
    public static String stateAt(Clip c, double t) {
        String s = c.state;
        for (Event e : c.events) {
            if (e.t <= t && e.kind.equals("state")) {
                s = e.arg;
            }
        }
        return s;
    }

    // ───────────────────────── matrices (row-major 4x4)

    public static double[] translate(double x, double y, double z) {
        return new double[] {1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z, 0, 0, 0, 1};
    }

    /** Right-handed rotation by deg degrees about an axis through the pivot. */
    public static double[] rotateAbout(double[] p, char axis, double deg) {
        double a = Math.toRadians(deg), c = Math.cos(a), s = Math.sin(a);
        double[] r = switch (axis) {
            case 'x' -> new double[] {1, 0, 0, 0, 0, c, -s, 0, 0, s, c, 0, 0, 0, 0, 1};
            case 'y' -> new double[] {c, 0, s, 0, 0, 1, 0, 0, -s, 0, c, 0, 0, 0, 0, 1};
            default -> new double[] {c, -s, 0, 0, s, c, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1};
        };
        return mul(mul(translate(p[0], p[1], p[2]), r), translate(-p[0], -p[1], -p[2]));
    }

    public static double[] mul(double[] a, double[] b) {
        double[] m = new double[16];
        for (int r = 0; r < 4; r++) {
            for (int c = 0; c < 4; c++) {
                double v = 0;
                for (int k = 0; k < 4; k++) {
                    v += a[r * 4 + k] * b[k * 4 + c];
                }
                m[r * 4 + c] = v;
            }
        }
        return m;
    }

    /** Column-major floats for JOML's Matrix4f.set(float[]), with the translation converted to blocks. */
    public static float[] toJoml(double[] m) {
        float[] f = new float[16];
        for (int r = 0; r < 4; r++) {
            for (int c = 0; c < 4; c++) {
                f[c * 4 + r] = (float) (c == 3 && r < 3 ? m[r * 4 + c] / 16.0 : m[r * 4 + c]);
            }
        }
        return f;
    }

    // ───────────────────────── loading

    public static MachineAnimation fromJson(JsonObject root) {
        MachineAnimation a = new MachineAnimation();
        for (Map.Entry<String, JsonElement> e : root.getAsJsonObject("parts").entrySet()) {
            JsonObject o = e.getValue().getAsJsonObject();
            Part p = new Part();
            p.id = e.getKey();
            p.parent = string(o, "parent");
            p.pivot = isNull(o, "pivot") ? null : doubles(o.getAsJsonArray("pivot"));
            if (!isNull(o, "axis")) {
                p.axis = o.get("axis").getAsString().charAt(0);
            }
            double[] an = doubles(o.getAsJsonArray("anchor"));
            p.anchor = new int[] {(int) an[0], (int) an[1], (int) an[2]};
            a.parts.put(p.id, p);
        }
        for (Map.Entry<String, JsonElement> e : root.getAsJsonObject("clips").entrySet()) {
            JsonObject o = e.getValue().getAsJsonObject();
            Clip c = new Clip();
            c.id = e.getKey();
            c.state = o.get("state").getAsString();
            c.length = o.get("length").getAsDouble();
            c.loop = o.get("loop").getAsBoolean();
            for (Map.Entry<String, JsonElement> te : o.getAsJsonObject("tracks").entrySet()) {
                Map<String, Key[]> chans = new LinkedHashMap<>();
                for (Map.Entry<String, JsonElement> ce : te.getValue().getAsJsonObject().entrySet()) {
                    JsonArray ks = ce.getValue().getAsJsonArray();
                    Key[] keys = new Key[ks.size()];
                    for (int i = 0; i < keys.length; i++) {
                        JsonArray k = ks.get(i).getAsJsonArray();
                        String ease = k.get(2).getAsString();
                        keys[i] = new Key(k.get(0).getAsDouble(), k.get(1).getAsDouble(),
                                ease.equals("linear") ? LINEAR : ease.equals("step") ? STEP : SMOOTH);
                    }
                    chans.put(ce.getKey(), keys);
                }
                c.tracks.put(te.getKey(), chans);
            }
            for (JsonElement po : o.getAsJsonArray("props")) {
                JsonObject pm = po.getAsJsonObject();
                Prop p = new Prop();
                p.id = pm.get("id").getAsString();
                p.model = pm.get("model").getAsString();
                p.center = doubles(pm.getAsJsonArray("center"));
                for (JsonElement ao : pm.getAsJsonArray("attach")) {
                    JsonObject am = ao.getAsJsonObject();
                    Attach at = new Attach();
                    at.t = am.get("t").getAsDouble();
                    at.carrier = string(am, "carrier");
                    at.offset = doubles(am.getAsJsonArray("offset"));
                    at.yaw = isNull(am, "yaw") ? 0 : am.get("yaw").getAsDouble();
                    at.lerp = !isNull(am, "lerp") && am.get("lerp").getAsBoolean();
                    p.attach.add(at);
                }
                c.props.add(p);
            }
            for (JsonElement eo : o.getAsJsonArray("events")) {
                JsonArray ev = eo.getAsJsonArray();
                Event x = new Event();
                x.t = ev.get(0).getAsDouble();
                x.kind = ev.get(1).getAsString();
                x.arg = ev.get(2).getAsString();
                x.open = ev.size() > 3 && ev.get(3).getAsBoolean();
                c.events.add(x);
            }
            if (!isNull(o, "fx")) {
                for (JsonElement fo : o.getAsJsonArray("fx")) {
                    JsonArray f = fo.getAsJsonArray();
                    c.fx.add(new Fx(f.get(0).getAsString(), f.get(1).getAsDouble(), f.get(2).getAsDouble(), f.get(3).getAsDouble()));
                }
            }
            a.clips.put(c.id, c);
        }
        if (root.has("emitters")) {
            for (Map.Entry<String, JsonElement> e : root.getAsJsonObject("emitters").entrySet()) {
                JsonObject o = e.getValue().getAsJsonObject();
                Emitter em = new Emitter();
                em.id = e.getKey();
                em.sprite = o.get("sprite").getAsString();
                JsonArray boxes = o.getAsJsonArray("boxes");
                em.boxes = new double[boxes.size()][][];
                for (int i = 0; i < em.boxes.length; i++) {
                    JsonArray b = boxes.get(i).getAsJsonArray();
                    em.boxes[i] = new double[][] {doubles(b.get(0).getAsJsonArray()), doubles(b.get(1).getAsJsonArray())};
                }
                em.rate = o.get("rate").getAsDouble();
                JsonArray vel = o.getAsJsonArray("vel");
                em.vel = new double[][] {doubles(vel.get(0).getAsJsonArray()), doubles(vel.get(1).getAsJsonArray())};
                em.gravity = o.get("gravity").getAsDouble();
                em.drag = o.get("drag").getAsDouble();
                em.life = doubles(o.getAsJsonArray("life"));
                em.size = doubles(o.getAsJsonArray("size"));
                em.additive = "add".equals(string(o, "blend"));
                String tint = string(o, "tint");
                if (tint != null && tint.startsWith("#")) {
                    em.tint = Integer.parseInt(tint.substring(1), 16);
                }
                a.emitters.put(em.id, em);
            }
        }
        return a;
    }

    private static boolean isNull(JsonObject o, String key) {
        return !o.has(key) || o.get(key).isJsonNull();
    }

    @Nullable
    private static String string(JsonObject o, String key) {
        return isNull(o, key) ? null : o.get(key).getAsString();
    }

    static double[] doubles(JsonArray l) {
        double[] d = new double[l.size()];
        for (int i = 0; i < d.length; i++) {
            d[i] = l.get(i).getAsDouble();
        }
        return d;
    }
}
