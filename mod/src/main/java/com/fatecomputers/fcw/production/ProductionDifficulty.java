package com.fatecomputers.fcw.production;

import com.fatecomputers.fcw.FcwConfig;

/**
 * The difficulty of the production chain: server setting {@code production.difficulty}
 * (docs/MACHINE_CONTROL.md, docs/design/MACHINE_BUILD.md section 7). Everything that depends on the level
 * (construction now, yield and the rest later) asks {@link #current()}.
 */
public enum ProductionDifficulty {
    /** a quarter of the bills of materials and of the build time; nothing is lost on teardown */
    STANDARD(0.25, 0.25, 0, false, 1.0, 1),
    /** the designed numbers */
    ENGINEER(1.0, 1.0, 64, false, 0.75, 3),
    /**
     * Twice the build time, building always needs power, bench chips do not stand in for scanner chips.
     * The component recipes of this level (workshops, generations) are not in the game yet: they are the
     * same as on engineer for now.
     */
    FAB(1.0, 2.0, 256, true, 0.6, 0);

    private final double bomScale, timeScale, refund;
    private final int buildEnergy, benchChipsPerChip;
    private final boolean buildAlwaysNeedsPower;

    ProductionDifficulty(double bomScale, double timeScale, int buildEnergy, boolean buildAlwaysNeedsPower, double refund, int benchChipsPerChip) {
        this.bomScale = bomScale;
        this.timeScale = timeScale;
        this.buildEnergy = buildEnergy;
        this.buildAlwaysNeedsPower = buildAlwaysNeedsPower;
        this.refund = refund;
        this.benchChipsPerChip = benchChipsPerChip;
    }

    public static ProductionDifficulty current() {
        return FcwConfig.SERVER.productionDifficulty.get();
    }

    /** How many of a component a build needs when the bill says {@code count}. */
    public int scale(int count) {
        return Math.max(1, (int) Math.ceil(count * bomScale - 1e-9));
    }

    /** Ticks a build stage takes when the bill says {@code seconds}. */
    public int stageTicks(int seconds) {
        return Math.max(20, (int) Math.round(seconds * 20 * timeScale));
    }

    /** FE per tick a build site draws right now (0: building is free). */
    public int buildEnergyPerTick() {
        return buildAlwaysNeedsPower || FcwConfig.SERVER.productionRequirePower.get() ? buildEnergy : 0;
    }

    /** Share of the components a teardown gives back. */
    public double refund() {
        return refund;
    }

    /** Bench chips needed in place of one scanner chip for the first scanner; 0 = not allowed. */
    public int benchChipsPerChip() {
        return benchChipsPerChip;
    }
}
