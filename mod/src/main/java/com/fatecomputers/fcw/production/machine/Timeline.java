package com.fatecomputers.fcw.production.machine;

import com.fatecomputers.fcw.production.machine.MachineAnimation.Clip;
import com.fatecomputers.fcw.production.machine.MachineSpec.Mode;
import com.fatecomputers.fcw.production.machine.MachineSpec.Phase;

import javax.annotation.Nullable;
import java.util.List;

/**
 * Maps the progress of a recipe to the clip that plays at that moment. The work sequence of a machine
 * always takes exactly the recipe's time: fixed clips keep their own speed (and are sped up together
 * when they would take more than 60 % of a short recipe), the loop or stretch phase fills the rest.
 * Used by the server (clip events: hatches, state) and by the client (poses, particles).
 */
public final class Timeline {
    /** Share of the recipe the fixed clips may take before they are sped up. */
    private static final double MAX_FIXED_SHARE = 0.6;

    /**
     * @param phase    index in the work sequence
     * @param clipTime seconds inside the clip
     * @param rate     clip seconds per real second (for the particle rate)
     */
    public record At(int phase, Clip clip, double clipTime, double rate) {
    }

    private Timeline() {
    }

    @Nullable
    public static At at(MachineDef def, double progressTicks, int totalTicks) {
        List<Phase> phases = def.spec.phases();
        if (phases.isEmpty() || totalTicks <= 0) {
            return null;
        }
        double total = totalTicks / 20.0;
        double t = Math.max(0, Math.min(progressTicks / 20.0, total));
        double fixed = 0;
        boolean flexible = false;
        for (Phase p : phases) {
            if (p.mode() == Mode.FIXED) {
                fixed += def.anim.clips.get(p.clip()).length;
            } else {
                flexible = true;
            }
        }
        // speed of the fixed clips
        double speed = 1;
        if (!flexible) {
            speed = fixed / total;
        } else if (fixed > total * MAX_FIXED_SHARE) {
            speed = fixed / (total * MAX_FIXED_SHARE);
        }
        double flexTime = total - fixed / speed;
        double start = 0;
        for (int i = 0; i < phases.size(); i++) {
            Phase p = phases.get(i);
            Clip clip = def.anim.clips.get(p.clip());
            double dur = p.mode() == Mode.FIXED ? clip.length / speed : flexTime;
            boolean last = i == phases.size() - 1;
            if (t < start + dur || last) {
                double local = Math.max(0, t - start);
                switch (p.mode()) {
                    case FIXED:
                        return new At(i, clip, Math.min(local * speed, clip.length), speed);
                    case STRETCH: {
                        double rate = clip.length / dur;
                        return new At(i, clip, Math.min(local * rate, clip.length), rate);
                    }
                    default: {
                        long loops = Math.max(1, Math.round(dur / clip.length));
                        double rate = loops * clip.length / dur;
                        return new At(i, clip, (local * rate) % clip.length, rate);
                    }
                }
            }
            start += dur;
        }
        return null;
    }
}
