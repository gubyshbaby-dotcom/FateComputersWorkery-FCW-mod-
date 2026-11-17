package com.fatecomputers.fcw.client.audio;

import net.minecraft.core.BlockPos;
import net.minecraft.world.phys.Vec3;

import javax.annotation.Nullable;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Раскладка каналов по колонкам одной машины. Чистая логика без OpenAL и без мира, чтобы её можно
 * было проверить тестом.
 *
 * <ul>
 * <li>Одна колонка (любая, хоть сабвуфер) играет всё: моно, как раньше.</li>
 * <li>Сабвуферы играют только низы, если кроме них есть обычные колонки.</li>
 * <li>Обычные колонки делятся на левые и правые по тому, с какой стороны от опорной оси они стоят.
 * Ось задаёт то, на что смотрит зритель: проектор, иначе монитор, иначе сам компьютер. Колонка на
 * самой оси (ближе {@link #CENTER_BAND} блока) — центральная и играет сумму каналов.</li>
 * <li>Если опоры нет или все колонки оказались с одной стороны, стерео не получается: все играют
 * сумму, чтобы канал не пропал.</li>
 * </ul>
 */
public final class SpeakerLayout {
    public enum Role { MONO, LEFT, RIGHT, SUB }

    /** Колонка: где стоит и сабвуфер ли это. */
    public record Speaker(BlockPos pos, boolean subwoofer) {
    }

    /**
     * Опора стереобазы.
     *
     * @param origin точка, от которой меряется сторона
     * @param right  единичный вектор вправо для зрителя
     * @param what   что это, для журнала: projector, monitor, computer
     */
    public record Anchor(Vec3 origin, Vec3 right, String what) {
    }

    public static final double CENTER_BAND = 0.3;

    private SpeakerLayout() {
    }

    public static Map<BlockPos, Role> assign(List<Speaker> speakers, @Nullable Anchor anchor) {
        Map<BlockPos, Role> out = new LinkedHashMap<>();
        if (speakers.size() <= 1) {
            speakers.forEach(s -> out.put(s.pos(), Role.MONO));
            return out;
        }
        long full = speakers.stream().filter(s -> !s.subwoofer()).count();
        boolean left = false, right = false;
        for (Speaker s : speakers) {
            Role role;
            if (s.subwoofer()) {
                role = full > 0 ? Role.SUB : Role.MONO;
            } else if (anchor == null || full < 2) {
                role = Role.MONO;
            } else {
                double side = Vec3.atCenterOf(s.pos()).subtract(anchor.origin()).dot(anchor.right());
                role = side > CENTER_BAND ? Role.RIGHT : side < -CENTER_BAND ? Role.LEFT : Role.MONO;
                left |= role == Role.LEFT;
                right |= role == Role.RIGHT;
            }
            out.put(s.pos(), role);
        }
        if (!(left && right)) {
            out.replaceAll((pos, role) -> role == Role.SUB ? Role.SUB : Role.MONO);
        }
        return out;
    }

    /** Строка для журнала: «stereo by projector: L [..] R [..] sub [..]». */
    public static String describe(Map<BlockPos, Role> roles, @Nullable Anchor anchor) {
        StringBuilder sb = new StringBuilder();
        boolean stereo = roles.containsValue(Role.LEFT);
        sb.append(stereo ? "stereo by " + (anchor == null ? "?" : anchor.what()) : "mono");
        for (Role r : Role.values()) {
            StringBuilder list = new StringBuilder();
            roles.forEach((pos, role) -> {
                if (role == r) {
                    list.append(list.isEmpty() ? "" : ", ").append('(').append(pos.toShortString()).append(')');
                }
            });
            if (!list.isEmpty()) {
                sb.append("; ").append(switch (r) {
                    case MONO -> stereo ? "centre (L+R)" : "L+R";
                    case LEFT -> "left";
                    case RIGHT -> "right";
                    case SUB -> "subwoofer (lows)";
                }).append(": [").append(list).append(']');
            }
        }
        return sb.toString();
    }
}
