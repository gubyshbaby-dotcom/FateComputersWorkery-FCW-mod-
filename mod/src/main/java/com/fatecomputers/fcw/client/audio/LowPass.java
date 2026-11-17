package com.fatecomputers.fcw.client.audio;

/**
 * Фильтр нижних частот для сабвуфера: Баттерворт второго порядка (биквад), считается прямо по
 * отсчётам PCM. Срез настоящий, как у кроссовера сабвуфера, и не зависит от того, есть ли у
 * звуковой карты OpenAL EFX. Состояние живёт между буферами, поэтому стыки не щёлкают.
 */
public final class LowPass {
    private final double b0, b1, b2, a1, a2;
    private double x1, x2, y1, y2;

    public LowPass(double cutoffHz, double sampleRate) {
        double w = 2 * Math.PI * cutoffHz / sampleRate;
        double cos = Math.cos(w), alpha = Math.sin(w) / (2 * Math.sqrt(0.5));
        double a0 = 1 + alpha;
        b0 = (1 - cos) / 2 / a0;
        b1 = (1 - cos) / a0;
        b2 = b0;
        a1 = -2 * cos / a0;
        a2 = (1 - alpha) / a0;
    }

    public double step(double x) {
        double y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        return y;
    }

    public void reset() {
        x1 = x2 = y1 = y2 = 0;
    }
}
