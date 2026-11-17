package com.fatecomputers.fcw.cinema;

import com.fatecomputers.fcw.block.DeviceBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;

import javax.annotation.Nullable;

/**
 * Геометрия проекции: где и какого размера картинка.
 *
 * <p>Луч идёт из линзы вдоль {@code facing} до первой твёрдой грани, не дальше {@code range}.
 * Картинка лежит в вертикальной плоскости, перпендикулярной лучу, в точке попадания.
 * <ul>
 * <li>На голой стене ширина равна расстоянию / {@link #THROW_RATIO}, высота — по пропорциям экрана
 * ВМ. У модели объектив не наклонён, поэтому картинка сдвинута, как у настоящих проекторов:
 * у напольного нижний край на уровне линзы, у потолочного (он висит вверх ногами) — верхний.</li>
 * <li>Если луч упёрся в проекционный экран или экран висит в пределах пятна над или под точкой
 * попадания, картинка вписывается в прямоугольник экрана с полями {@link #SCREEN_MARGIN}. Зум
 * объектива не бесконечен: шире расстояния / {@link #WIDE_RATIO} картинка не станет.</li>
 * </ul>
 *
 * @param lens     центр линзы
 * @param center   центр картинки
 * @param right    единичный вектор вправо для зрителя, который смотрит вдоль луча
 * @param normal   единичный вектор от поверхности к зрителю
 * @param halfW    половина ширины картинки
 * @param halfH    половина высоты
 * @param distance расстояние от линзы до поверхности
 * @param onScreen картинка на проекционном экране (иначе на стене: она тусклее)
 */
public record Projection(Vec3 lens, Vec3 center, Vec3 right, Vec3 normal, double halfW, double halfH,
                         double distance, boolean onScreen) {
    public static final double THROW_RATIO = 1.5;
    public static final double WIDE_RATIO = 1.0;
    /** Поля от края экрана до картинки: рамка 1/16 и столько же белого. */
    public static final double SCREEN_MARGIN = 2.0 / 16.0;
    public static final int MAX_SCREEN_WIDTH = 16;
    public static final int MAX_SCREEN_HEIGHT = 9;
    private static final double MIN_DISTANCE = 1.0;

    /** Угол картинки: {@code u}, {@code v} от -1 до 1 (вправо и вверх), {@code lift} — отступ от поверхности. */
    public Vec3 corner(double u, double v, double lift) {
        return new Vec3(center.x + right.x * halfW * u + normal.x * lift,
                center.y + halfH * v,
                center.z + right.z * halfW * u + normal.z * lift);
    }

    /** Центр линзы в мире: у модели она на передней грани, левее середины. */
    public static Vec3 lens(BlockPos pos, BlockState state) {
        Direction facing = state.getValue(DeviceBlock.FACING);
        boolean ceiling = state.getValue(ProjectorBlock.MOUNT) == ProjectorBlock.Mount.CEILING;
        double x = 6.0 / 16.0, y = (ceiling ? 7.25 : 3.25) / 16.0, z = 2.0 / 16.0;
        // тот же поворот, что у blockstate: north 0, east 90, south 180, west 270
        double rx, rz;
        switch (facing) {
            case EAST -> {
                rx = 1 - z;
                rz = x;
            }
            case SOUTH -> {
                rx = 1 - x;
                rz = 1 - z;
            }
            case WEST -> {
                rx = z;
                rz = 1 - x;
            }
            default -> {
                rx = x;
                rz = z;
            }
        }
        return new Vec3(pos.getX() + rx, pos.getY() + y, pos.getZ() + rz);
    }

    @Nullable
    public static Projection compute(Level level, BlockPos pos, BlockState state, float aspect, int range) {
        if (!(state.getBlock() instanceof ProjectorBlock) || aspect <= 0f) {
            return null;
        }
        Direction facing = state.getValue(DeviceBlock.FACING);
        boolean ceiling = state.getValue(ProjectorBlock.MOUNT) == ProjectorBlock.Mount.CEILING;
        Vec3 dir = Vec3.atLowerCornerOf(facing.getNormal());
        Vec3 lens = lens(pos, state);
        BlockHitResult hit = level.clip(new ClipContext(lens.add(dir.scale(0.03)), lens.add(dir.scale(range)),
                ClipContext.Block.COLLIDER, ClipContext.Fluid.NONE, CollisionContext.empty()));
        if (hit.getType() != HitResult.Type.BLOCK) {
            return null;
        }
        double distance = hit.getLocation().subtract(lens).dot(dir);
        if (distance < MIN_DISTANCE) {
            return null;
        }
        Direction rightDir = facing.getClockWise();
        Vec3 right = Vec3.atLowerCornerOf(rightDir.getNormal());
        Vec3 normal = dir.scale(-1);
        double w = distance / THROW_RATIO;
        double h = w / aspect;

        BlockPos screen = findScreen(level, hit, facing, ceiling, h);
        if (screen != null) {
            return onScreen(level, screen, facing, rightDir, lens, right, normal, aspect);
        }
        Vec3 p = hit.getLocation();
        Vec3 center = new Vec3(p.x, ceiling ? lens.y - h / 2 : lens.y + h / 2, p.z);
        return new Projection(lens, center, right, normal, w / 2, h / 2, distance, false);
    }

    private static boolean isScreen(BlockGetter level, BlockPos pos, Direction projectorFacing) {
        BlockState s = level.getBlockState(pos);
        return s.getBlock() instanceof ProjectionScreenBlock && s.getValue(DeviceBlock.FACING) == projectorFacing.getOpposite();
    }

    /**
     * Экран, в который упёрся луч, либо ближайший экран над (напольный проектор) или под
     * (потолочный) точкой попадания в пределах высоты пятна: экран не обязан висеть ровно на
     * уровне линзы.
     */
    @Nullable
    private static BlockPos findScreen(BlockGetter level, BlockHitResult hit, Direction facing, boolean ceiling, double pictureHeight) {
        if (isScreen(level, hit.getBlockPos(), facing)) {
            return hit.getBlockPos();
        }
        // клетка перед стеной: полотно висит в ней, у дальней от зрителя стенки
        Vec3 p = hit.getLocation();
        BlockPos front = BlockPos.containing(p.x - facing.getStepX() * 0.5, p.y, p.z - facing.getStepZ() * 0.5);
        int steps = (int) Math.ceil(pictureHeight);
        for (int k = 1; k <= steps; k++) {
            BlockPos c = ceiling ? front.below(k) : front.above(k);
            if (isScreen(level, c, facing)) {
                return c;
            }
        }
        return null;
    }

    /** Прямоугольник из полотен вокруг {@code start}: сначала ряд, потом ряды выше и ниже той же ширины. */
    private static Projection onScreen(BlockGetter level, BlockPos start, Direction facing, Direction rightDir,
                                       Vec3 lens, Vec3 right, Vec3 normal, float aspect) {
        int left = 0, rightN = 0, up = 0, down = 0;
        while (left + rightN + 1 < MAX_SCREEN_WIDTH && isScreen(level, start.relative(rightDir, -(left + 1)), facing)) {
            left++;
        }
        while (left + rightN + 1 < MAX_SCREEN_WIDTH && isScreen(level, start.relative(rightDir, rightN + 1), facing)) {
            rightN++;
        }
        while (up + down + 1 < MAX_SCREEN_HEIGHT && rowIsScreen(level, start.above(up + 1), rightDir, left, rightN, facing)) {
            up++;
        }
        while (up + down + 1 < MAX_SCREEN_HEIGHT && rowIsScreen(level, start.below(down + 1), rightDir, left, rightN, facing)) {
            down++;
        }
        int cols = left + rightN + 1, rows = up + down + 1;
        // середина прямоугольника; поверхность полотна в 1/16 от дальней стенки клетки
        Vec3 c = Vec3.atCenterOf(start)
                .add(right.scale((rightN - left) / 2.0))
                .add(0, (up - down) / 2.0, 0)
                .add(normal.scale(-(0.5 - 1.0 / 16.0)));
        double distance = c.subtract(lens).dot(normal.scale(-1));
        double w = cols - 2 * SCREEN_MARGIN, h = rows - 2 * SCREEN_MARGIN;
        if (aspect > w / h) {
            h = w / aspect;
        } else {
            w = h * aspect;
        }
        double widest = Math.max(distance, MIN_DISTANCE) / WIDE_RATIO;
        if (w > widest) {
            h *= widest / w;
            w = widest;
        }
        return new Projection(lens, c, right, normal, w / 2, h / 2, distance, true);
    }

    private static boolean rowIsScreen(BlockGetter level, BlockPos rowStart, Direction rightDir, int left, int right, Direction facing) {
        for (int i = -left; i <= right; i++) {
            if (!isScreen(level, rowStart.relative(rightDir, i), facing)) {
                return false;
            }
        }
        return true;
    }
}
