package com.fatecomputers.fcw.block;

import net.minecraft.core.Direction;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

import java.util.EnumMap;
import java.util.Map;

/**
 * Block outline/collision shapes, taken from the bounding boxes of the main elements of each
 * block model (models face north, coordinates in model pixels 0..16), plus rotation by facing.
 */
public enum DeviceModel {
    SYSTEM_UNIT(box(4, 0, 0, 12, 16, 15)),
    CASE_DESKTOP(box(1, 0, 2, 15, 5.5, 14)),
    CASE_TOWER_RETRO(box(4.5, 0, 1.5, 11.5, 14.5, 14.5)),
    CASE_TOWER(box(4, 0, 1, 12, 15.5, 15)),
    CASE_GAMING(box(4, 0, 1, 12, 15.5, 15)),
    CASE_CUBE(box(3.5, 0, 3.5, 12.5, 9.5, 12.5)),
    CASE_SERVER(box(0, 0, 0, 16, 7, 15)),
    MONITOR(box(4.5, 0, 5.5, 11.5, 1, 11.5), box(7.5, 1, 8.5, 8.5, 4.5, 9.5), box(1, 4.5, 6, 15, 15, 7.5), box(3, 6, 7.5, 13, 13.5, 8.5)),
    CRT_MONITOR(box(4, 0, 3.5, 12, 2, 11.5), box(2, 2, 0, 14, 13, 6.5), box(2.5, 2.5, 6.5, 13.5, 12.5, 9.5), box(3.5, 3.5, 9.5, 12.5, 11.5, 12), box(4.5, 4.5, 12, 11.5, 10.5, 15)),
    KEYBOARD(box(0.5, 0, 5, 15.5, 1, 11)),
    MOUSE(box(2.5, 0, 3, 13.5, 0.5, 13), box(6.5, 0.5, 5.5, 9.5, 2.5, 10.5)),
    SPEAKER_DESKTOP(box(1, 0, 5.5, 6.5, 9.5, 11.5), box(9.5, 0, 5.5, 15, 9.5, 11.5)),
    SPEAKER_BOOKSHELF(box(4.5, 0, 5, 11.5, 10.5, 12)),
    SPEAKER_TOWER(box(3.5, 0, 4, 12.5, 15.5, 13)),
    SUBWOOFER(box(2.5, 0, 2.5, 13.5, 12, 13.5)),
    SOUNDBAR(box(0.5, 0, 6, 15.5, 3, 9.5));

    private final Map<Direction, VoxelShape> shapes = new EnumMap<>(Direction.class);

    DeviceModel(double[]... boxes) {
        for (Direction d : Direction.Plane.HORIZONTAL) {
            VoxelShape s = Shapes.empty();
            for (double[] b : boxes) {
                double[] r = rotate(b, d);
                s = Shapes.or(s, Shapes.box(r[0] / 16, r[1] / 16, r[2] / 16, r[3] / 16, r[4] / 16, r[5] / 16));
            }
            shapes.put(d, s.optimize());
        }
    }

    public VoxelShape shape(Direction facing) {
        VoxelShape s = shapes.get(facing);
        return s != null ? s : shapes.get(Direction.NORTH);
    }

    private static double[] box(double x0, double y0, double z0, double x1, double y1, double z1) {
        return new double[]{x0, y0, z0, x1, y1, z1};
    }

    /**
     * Same rotation as the blockstate "y" rotation of the model (clockwise seen from above):
     * north 0, east 90, south 180, west 270.
     */
    static double[] rotate(double[] b, Direction facing) {
        double x0 = b[0], z0 = b[2], x1 = b[3], z1 = b[5];
        double nx0, nz0, nx1, nz1;
        switch (facing) {
            case EAST -> { // (x, z) -> (16 - z, x)
                nx0 = 16 - z1; nx1 = 16 - z0; nz0 = x0; nz1 = x1;
            }
            case SOUTH -> { // (x, z) -> (16 - x, 16 - z)
                nx0 = 16 - x1; nx1 = 16 - x0; nz0 = 16 - z1; nz1 = 16 - z0;
            }
            case WEST -> { // (x, z) -> (z, 16 - x)
                nx0 = z0; nx1 = z1; nz0 = 16 - x1; nz1 = 16 - x0;
            }
            default -> {
                nx0 = x0; nx1 = x1; nz0 = z0; nz1 = z1;
            }
        }
        return new double[]{nx0, b[1], nz0, nx1, b[4], nz1};
    }
}
