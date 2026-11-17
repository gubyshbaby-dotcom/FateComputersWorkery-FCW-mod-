package com.fatecomputers.fcw.production.quality;

/**
 * Geometry of a patterned wafer: 61 exposure fields in rows of 3-7-7-9-9-9-7-7-3, exposed in a
 * serpentine (docs/design/DEFECTS.md, section 2). A field index is its place in the exposure order.
 * The 24 fields of the outer ring are "edge" fields, the other 37 "inner".
 */
public final class WaferMap {
    public static final int FIELDS = 61;
    public static final int[] ROWS = {3, 7, 7, 9, 9, 9, 7, 7, 3};
    /** Causes of a bad field, in the order they are checked; '.' is a good field. */
    public static final String CAUSES = "EPWCTMV";

    private static final int[] ROW_OF = new int[FIELDS];
    private static final boolean[] EDGE = new boolean[FIELDS];

    static {
        int k = 0;
        for (int r = 0; r < ROWS.length; r++) {
            int n = ROWS[r];
            for (int i = 0; i < n; i++) {
                // column from the left; odd rows are exposed right to left
                int col = r % 2 == 0 ? i : n - 1 - i;
                ROW_OF[k] = r;
                boolean edge = r == 0 || r == ROWS.length - 1 || col == 0 || col == n - 1
                        // the corners of the rows that are wider than their neighbour towards the rim
                        || ((r == 1 || r == ROWS.length - 2) && (col == 1 || col == n - 2));
                EDGE[k] = edge;
                k++;
            }
        }
    }

    private WaferMap() {
    }

    public static int row(int field) {
        return ROW_OF[field];
    }

    public static boolean edge(int field) {
        return EDGE[field];
    }

    /** Good fields in a map string ('.' = good, a letter = the cause of the defect). */
    public static int good(String map) {
        int n = 0;
        for (int i = 0; i < map.length(); i++) {
            if (map.charAt(i) == '.') {
                n++;
            }
        }
        return n;
    }

    /** Bad fields by cause, in the order of {@link #CAUSES}. */
    public static int[] causes(String map) {
        int[] n = new int[CAUSES.length()];
        for (int i = 0; i < map.length(); i++) {
            int c = CAUSES.indexOf(map.charAt(i));
            if (c >= 0) {
                n[c]++;
            }
        }
        return n;
    }
}
