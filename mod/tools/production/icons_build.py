"""Temporary 32x32 icons of the machine components, blueprints and the wrench (docs/design/MACHINE_BUILD.md),
in the manner of icons.py, until the artist's archive 16 arrives. usage: icons_build.py <textures/item dir>"""
import math
import sys
from PIL import Image

OUT = sys.argv[1]
IRON, STEEL, COPPER = (200, 203, 210), (122, 130, 142), (214, 128, 84)
GRAPHITE, FIRE, WHITE = (62, 64, 70), (196, 150, 110), (236, 238, 242)
GRANITE, QUARTZ, DIAMOND = (172, 112, 98), (238, 232, 224), (110, 220, 215)
VIOLET, BLUE, GOLD, DARK = (168, 139, 255), (58, 98, 200), (240, 196, 80), (24, 25, 30)
RED, GREEN, LEATHER = (230, 80, 70), (90, 210, 120), (120, 84, 60)


def sh(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c)


class Icon:
    def __init__(self):
        self.im = Image.new("RGBA", (32, 32), (0, 0, 0, 0))

    def px(self, x, y, c):
        if 0 <= x < 32 and 0 <= y < 32:
            self.im.putpixel((int(x), int(y)), tuple(c) + (255,))

    def rect(self, x0, y0, x1, y1, c, edge=True):
        """a box with a lit top-left and a shaded bottom-right edge"""
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                col = c
                if edge and (x == x0 or y == y0):
                    col = sh(c, 1.18)
                if edge and (x == x1 or y == y1):
                    col = sh(c, 0.62)
                self.px(x, y, col)

    def frame(self, x0, y0, x1, y1, c, t=2):
        self.rect(x0, y0, x1, y0 + t - 1, c)
        self.rect(x0, y1 - t + 1, x1, y1, c)
        self.rect(x0, y0, x0 + t - 1, y1, c)
        self.rect(x1 - t + 1, y0, x1, y1, c)

    def disc(self, cx, cy, r, c, ring=None, hole=0):
        for y in range(32):
            for x in range(32):
                d = math.hypot(x - cx, y - cy)
                if hole <= d <= r:
                    col = sh(c, 1.15 - 0.45 * ((x - cx + y - cy) / (2 * r) + 0.5))
                    if ring and d > r - 2:
                        col = ring
                    if d > r - 0.8:
                        col = sh(col, 0.6)
                    self.px(x, y, col)

    def line(self, x0, y0, x1, y1, c, w=1):
        n = int(max(abs(x1 - x0), abs(y1 - y0))) or 1
        for i in range(n + 1):
            x, y = x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n
            for dx in range(w):
                for dy in range(w):
                    self.px(round(x) + dx, round(y) + dy, c)

    def save(self, name):
        self.im.save(f"{OUT}/{name}.png")


def plate(name, c):
    i = Icon()
    for y in range(10, 23):
        off = (22 - y) // 2
        for x in range(4 + off, 22 + off):
            col = sh(c, 1.1 if y < 12 else 0.95)
            if x in (4 + off, 21 + off) or y in (10, 22):
                col = sh(c, 0.6)
            i.px(x, y, col)
    i.line(8, 19, 22, 19, sh(c, 0.8))
    i.save(name)


def frame(name, c):
    i = Icon()
    i.frame(4, 4, 27, 27, c, 3)
    i.line(7, 7, 24, 24, sh(c, 0.75), 2)
    i.line(24, 7, 7, 24, sh(c, 0.75), 2)
    i.save(name)


def box(name, c, deco):
    i = Icon()
    i.rect(5, 6, 26, 26, c)
    deco(i)
    i.save(name)


def cabinet(name, c, lamps):
    def deco(i):
        i.rect(8, 9, 23, 15, DARK)
        i.line(10, 12, 18, 12, GREEN)
        for k in range(lamps):
            i.rect(9 + k * 5, 19, 11 + k * 5, 21, [GREEN, GOLD, RED][k], False)
        i.rect(5, 25, 26, 26, VIOLET, False)
    box(name, c, deco)


def blob(name, c, seed):
    i = Icon()
    for y in range(32):
        for x in range(32):
            r = 10 + 2.2 * math.sin(seed + math.atan2(y - 16, x - 16) * 3) + 1.2 * math.sin(seed * 2 + math.atan2(y - 16, x - 16) * 7)
            d = math.hypot(x - 16, y - 17)
            if d < r:
                col = sh(c, 1.2 - 0.5 * ((x + y) / 44))
                if d > r - 1.2:
                    col = sh(c, 0.55)
                elif (x * 7 + y * 13 + int(seed * 10)) % 17 == 0:
                    col = sh(c, 1.3)
                i.px(x, y, col)
    i.save(name)


def bricks(name, c, rows):
    i = Icon()
    h = 20 // rows
    for r in range(rows):
        y0 = 6 + r * h
        off = 5 if r % 2 else 0
        for b in range(-1, 3):
            x0 = max(4, 4 + off + b * 12)
            x1 = min(27, 4 + off + b * 12 + 10)
            if x1 - x0 > 2:
                i.rect(x0, y0, x1, y0 + h - 2, c)
    i.save(name)


def rod(name, c, cap=None, zigzag=False):
    i = Icon()
    if zigzag:
        pts = [(8, 26), (8, 8), (13, 8), (13, 24), (18, 24), (18, 8), (23, 8), (23, 26)]
        for a, b in zip(pts, pts[1:]):
            i.line(a[0], a[1], b[0], b[1], c, 3)
        i.rect(6, 26, 12, 28, cap)
        i.rect(21, 26, 27, 28, cap)
    else:
        i.rect(12, 3, 19, 28, c)
        i.rect(10, 5, 21, 9, cap)
        i.rect(13, 27, 18, 29, (255, 150, 60), False)
    i.save(name)


def bowl(name, c):
    i = Icon()
    for y in range(8, 27):
        w = 11 - max(0, (y - 16)) * 0.55
        for x in range(32):
            if abs(x - 15.5) <= w:
                inner = abs(x - 15.5) <= w - 2 and y < 24 - 0
                col = sh(c, 0.7 if inner and y < 12 else 1.1 - 0.4 * (x / 32))
                if abs(x - 15.5) > w - 1 or y == 26:
                    col = sh(c, 0.6)
                i.px(x, y, col)
    i.save(name)


def bar(name, c):
    i = Icon()
    i.rect(3, 12, 28, 19, c)
    for x in (6, 25):
        i.rect(x, 15, x + 1, 16, sh(c, 0.5), False)
    i.save(name)


def pipe(name, c):
    i = Icon()
    i.line(5, 24, 24, 5, sh(c, 0.6), 5)
    i.line(6, 24, 24, 6, c, 3)
    i.line(6, 23, 23, 6, sh(c, 1.25), 1)
    i.disc(25.5, 6.5, 3.4, c, hole=1.5)
    i.save(name)


def coil(name, c, core):
    i = Icon()
    i.rect(13, 3, 18, 28, core)
    for y in range(6, 26, 3):
        i.rect(7, y, 24, y + 1, c)
    i.save(name)


def motor(name, extra=None):
    i = Icon()
    i.rect(6, 9, 21, 23, STEEL)
    for x in range(8, 20, 3):
        i.line(x, 10, x, 22, sh(STEEL, 0.75))
    i.rect(21, 14, 27, 18, IRON)
    i.rect(8, 23, 19, 25, DARK)
    i.rect(9, 6, 14, 9, COPPER)
    if extra:
        extra(i)
    i.save(name)


def disc(name, c, ring=None, hole=0, deco=None):
    i = Icon()
    i.disc(15.5, 15.5, 12, c, ring, hole)
    if deco:
        deco(i)
    i.save(name)


def blueprint(name, glyph):
    i = Icon()
    i.rect(5, 4, 26, 27, BLUE)
    for y in range(8, 26, 6):
        i.line(7, y, 24, y, sh(BLUE, 1.35))
    for x in range(10, 26, 6):
        i.line(x, 6, x, 25, sh(BLUE, 1.35))
    for (x0, y0, x1, y1) in glyph:
        i.line(x0, y0, x1, y1, WHITE)
    i.save(name)


def wrench():
    i = Icon()
    i.line(8, 24, 21, 11, sh(STEEL, 0.6), 4)
    i.line(9, 24, 21, 12, IRON, 2)
    i.disc(22.5, 9.5, 5.5, IRON, hole=0)
    i.rect(22, 4, 25, 9, (0, 0, 0), False)
    for y in range(4, 10):
        for x in range(22, 26):
            i.im.putpixel((x, y), (0, 0, 0, 0))
    i.disc(8.5, 24.5, 3, IRON, hole=1.2)
    i.save("wrench")


plate("iron_plate", IRON)
plate("steel_plate", STEEL)
frame("iron_frame", IRON)
frame("steel_frame", STEEL)
box("enclosure_panel", WHITE, lambda i: (i.rect(8, 9, 23, 17, DARK), i.rect(5, 23, 26, 24, VIOLET, False)))
box("granite_plate_blank", GRANITE, lambda i: [i.px(x, y, sh(GRANITE, 0.7)) for x in range(7, 25, 3) for y in range(8, 25, 4)])
plate("granite_plate", GRANITE)
blob("fireclay", FIRE, 1.0)
bricks("firebrick", sh(FIRE, 1.05), 1)
bricks("refractory_lining", FIRE, 3)
blob("graphite", GRAPHITE, 2.3)
rod("graphite_electrode", GRAPHITE, COPPER)
rod("graphite_heater", GRAPHITE, COPPER, zigzag=True)
bowl("crucible_blank", (190, 186, 180))
bowl("quartz_crucible", QUARTZ)
box("water_cooled_panel", IRON, lambda i: [i.line(7, y, 24, y, COPPER, 2) for y in (9, 14, 19, 23)])
box("oven_module", STEEL, lambda i: (i.rect(8, 10, 23, 18, DARK), [i.line(9, y, 22, y, (255, 140, 50)) for y in (12, 14, 16)]))
bar("copper_bar", COPPER)
pipe("copper_pipe", COPPER)
coil("copper_coil", COPPER, IRON)
box("furnace_transformer", STEEL, lambda i: ([i.rect(x, 8, x + 4, 20, COPPER) for x in (7, 14, 21)], i.rect(5, 22, 26, 26, sh(STEEL, 0.8))))
motor("electric_motor")
motor("pump", lambda i: i.disc(23.5, 22.5, 5, COPPER, hole=2.2))
box("hydraulic_cylinder", STEEL, lambda i: (i.rect(13, 1, 18, 8, IRON), i.rect(8, 12, 23, 13, sh(STEEL, 0.7), False)))
motor("pulling_head", lambda i: [i.disc(24, 24 - k * 4, 1.6, IRON, hole=0.6) for k in range(4)])
box("linear_stage", GRANITE, lambda i: (i.rect(5, 12, 26, 14, STEEL), i.rect(5, 19, 26, 21, STEEL), i.rect(11, 9, 20, 23, IRON)))
box("conveyor_section", DARK, lambda i: (i.rect(5, 12, 26, 18, LEATHER), [i.disc(x, 22.5, 2.2, IRON) for x in (9, 16, 23)]))
disc("vacuum_chamber_section", STEEL, hole=0, deco=lambda i: (i.disc(15.5, 15.5, 6.5, DARK), i.disc(15.5, 15.5, 4, (60, 50, 90))))
cabinet("control_cabinet_1", IRON, 1)
cabinet("control_cabinet_2", STEEL, 2)
cabinet("control_cabinet_3", WHITE, 3)
disc("steel_wire", STEEL, hole=7, deco=lambda i: i.disc(15.5, 15.5, 9.5, sh(STEEL, 1.25), hole=8.5))
disc("diamond_wire", STEEL, hole=7, deco=lambda i: [i.px(15.5 + 10 * math.cos(a / 3), 15.5 + 10 * math.sin(a / 3), DIAMOND) for a in range(19)])
disc("diamond_blade", STEEL, ring=DIAMOND, hole=2.5)
disc("polishing_head", WHITE, ring=STEEL, deco=lambda i: i.disc(15.5, 15.5, 3.5, STEEL))
box("placement_head", STEEL, lambda i: (i.rect(13, 26, 18, 30, IRON), i.rect(9, 9, 22, 14, DARK), i.rect(14, 29, 17, 31, GREEN, False)))
box("bonding_head", STEEL, lambda i: (i.rect(14, 26, 17, 30, GOLD), i.rect(9, 9, 22, 14, DARK), i.line(11, 12, 20, 12, GOLD)))
disc("polished_mirror_blank", QUARTZ)
disc("multilayer_mirror", (190, 170, 240), ring=STEEL, deco=lambda i: i.line(9, 20, 20, 9, WHITE, 2))
box("laser_module", STEEL, lambda i: (i.rect(8, 10, 23, 20, DARK), i.line(8, 15, 23, 15, VIOLET, 2), i.rect(24, 13, 28, 18, sh(VIOLET, 1.1))))
box("precision_stage", GRANITE, lambda i: (i.rect(8, 9, 23, 20, IRON), i.disc(15.5, 14.5, 4, (150, 190, 230))))
motor("turbo_pump", lambda i: [i.line(7, y, 20, y, IRON) for y in (11, 14, 17, 20)])
box("chiller_unit", WHITE, lambda i: ([i.line(8, y, 23, y, (120, 180, 255)) for y in range(9, 21, 2)], i.rect(5, 23, 26, 24, VIOLET, False)))

def proto(name, c, pins):
    i = Icon()
    i.rect(8, 8, 23, 23, DARK)
    for k in range(pins):
        for (x0, y0, x1, y1) in ((10 + k * 4, 5, 11 + k * 4, 7), (10 + k * 4, 24, 11 + k * 4, 26), (5, 10 + k * 4, 7, 11 + k * 4), (24, 10 + k * 4, 26, 11 + k * 4)):
            i.rect(x0, y0, x1, y1, c, False)
    i.rect(11, 11, 20, 20, sh(c, 0.7))
    i.line(13, 18, 13, 13, WHITE)
    i.line(13, 13, 17, 13, WHITE)
    i.line(17, 13, 17, 15, WHITE)
    i.line(13, 15, 17, 15, WHITE)
    i.save(name)


proto("proto_chip_controller", COPPER, 4)
proto("proto_chip_memory", (120, 180, 255), 4)
proto("proto_cpu_pga", GOLD, 4)

G = {
    "arc_furnace": [(11, 20, 21, 20), (11, 20, 11, 14), (21, 20, 21, 14), (14, 8, 14, 16), (18, 8, 18, 16)],
    "crystal_puller": [(16, 6, 16, 18), (12, 18, 20, 18), (12, 18, 12, 24), (20, 18, 20, 24), (12, 24, 20, 24)],
    "wafering_cell": [(10, 15, 22, 15), (16, 9, 16, 21), (12, 11, 20, 19), (20, 11, 12, 19)],
    "chip_line": [(8, 18, 24, 18), (8, 21, 24, 21), (11, 12, 11, 18), (16, 10, 16, 18), (21, 12, 21, 18)],
    "euv_scanner_mk2": [(8, 22, 24, 22), (8, 22, 8, 14), (24, 22, 24, 10), (8, 14, 16, 14), (16, 14, 16, 10), (16, 10, 24, 10)],
}
for m, glyph in G.items():
    blueprint(f"{m}_blueprint", glyph)
wrench()
print("icons:", 43 + len(G) + 1)
