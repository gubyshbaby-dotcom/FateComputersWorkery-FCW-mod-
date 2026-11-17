"""Pixel icons 32x32 for the production chain items, in the style of the CPU/RAM icons."""
import math
import random
import sys
from PIL import Image

OUT = sys.argv[1]
T = (0, 0, 0, 0)


def new():
    return Image.new("RGBA", (32, 32), T)


def rect(im, x0, y0, x1, y1, c):
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            if 0 <= x < 32 and 0 <= y < 32:
                im.putpixel((x, y), c + (255,) if len(c) == 3 else c)


def px(im, x, y, c):
    rect(im, x, y, x, y, c)


def shade(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c)


def mix(a, b, t):
    return tuple(int(a[i] * (1 - t) + b[i] * t) for i in range(3))


# ---------------------------------------------------------------- chunks
def chunk(name, base, hi, dark, seed, sparkle=None, pieces=None):
    """A lumpy faceted piece of material (or several small ones)."""
    rnd = random.Random(seed)
    im = new()
    blobs = pieces or [(16, 17, 11, 9)]
    for (cx, cy, rx, ry) in blobs:
        # irregular outline from a few random radial bumps
        bumps = [rnd.uniform(0.78, 1.08) for _ in range(10)]
        inside = {}
        for y in range(32):
            for x in range(32):
                dx, dy = (x - cx) / rx, (y - cy) / ry
                a = (math.atan2(dy, dx) + math.pi) / (2 * math.pi) * 10
                i = int(a) % 10
                t = a - int(a)
                r = bumps[i] * (1 - t) + bumps[(i + 1) % 10] * t
                if dx * dx + dy * dy <= r * r:
                    inside[(x, y)] = (dx, dy)
        for (x, y), (dx, dy) in inside.items():
            # facets: quantised light from the top-left
            light = -(dx * 0.6 + dy * 0.8)
            facet = ((x * 3 + y * 5) // 7 + (x - y) // 5) % 3
            v = light + (facet - 1) * 0.22
            c = hi if v > 0.45 else base if v > -0.15 else dark
            if v > 0.8:
                c = shade(hi, 1.15)
            edge = any((x + ox, y + oy) not in inside for ox, oy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
            if edge:
                c = shade(dark, 0.6)
            px(im, x, y, c)
        if sparkle:
            pts = [p for p, (dx, dy) in inside.items() if dx * dx + dy * dy < 0.5]
            for p in rnd.sample(pts, min(len(pts), 4)):
                px(im, p[0], p[1], sparkle)
    im.save(f"{OUT}/{name}.png")


chunk("metallurgical_silicon", (92, 96, 108), (140, 146, 160), (58, 60, 72), 3, sparkle=(196, 202, 214))
chunk("polysilicon", (150, 160, 178), (206, 214, 228), (98, 106, 126), 8, sparkle=(240, 246, 255),
      pieces=[(11, 12, 7, 6), (21, 14, 7, 6), (15, 22, 8, 6)])


# ---------------------------------------------------------------- steel ingot
def ingot(name, base):
    im = new()
    hi, lo, out = shade(base, 1.3), shade(base, 0.72), shade(base, 0.38)
    # vanilla-like slanted ingot, doubled
    top = [(8, 9, 23, 9), (6, 10, 25, 11), (4, 12, 27, 13), (2, 14, 29, 15)]
    for (x0, y0, x1, y1) in top:
        rect(im, x0, y0, x1, y1, hi)
    rect(im, 10, 11, 21, 13, base)          # recessed top face
    rect(im, 2, 16, 29, 21, base)           # front face
    rect(im, 2, 20, 29, 21, lo)
    rect(im, 24, 16, 29, 21, lo)
    for x in range(2, 30):                  # outline
        px(im, x, 22, out)
    for (x0, y0, x1, y1) in top:
        px(im, x0 - 1, y0, out), px(im, x0 - 1, y1, out), px(im, x1 + 1, y0, out), px(im, x1 + 1, y1, out)
    rect(im, 8, 8, 23, 8, out)
    rect(im, 1, 16, 1, 21, out), rect(im, 30, 16, 30, 21, out)
    rect(im, 4, 17, 9, 17, hi)              # glint
    px(im, 11, 17, hi)
    im.save(f"{OUT}/{name}.png")


ingot("steel_ingot", (128, 138, 156))


# ---------------------------------------------------------------- wafers
def disc_mask(cx=15.5, cy=15.5, r=13.2, flat=27):
    return {(x, y) for y in range(32) for x in range(32)
            if (x - cx) ** 2 + (y - cy) ** 2 <= r * r and y <= flat}


def wafer(name, die=None, pitch=4):
    im = new()
    m = disc_mask()
    base, hi, lo, out = (150, 158, 176), (214, 222, 236), (112, 120, 140), (58, 62, 80)
    for (x, y) in m:
        # mirror sheen: diagonal bands
        d = (x + y)
        c = hi if 22 <= d <= 25 or 31 <= d <= 32 else base if d < 40 else lo
        px(im, x, y, c)
    if die:
        dark = shade(die, 0.62)
        inner = {(x, y) for (x, y) in m if (x - 15.5) ** 2 + (y - 15.5) ** 2 <= 11.6 ** 2 and y <= 25}
        for (x, y) in inner:
            gx, gy = (x - 2) % pitch, (y - 2) % pitch
            if gx == pitch - 1 or gy == pitch - 1:
                continue                                    # scribe lines stay silicon
            # whole dies only
            x0, y0 = x - gx, y - gy
            if all((x0 + i, y0 + j) in inner for i in (0, pitch - 2) for j in (0, pitch - 2)):
                c = shade(die, 1.25) if (gx, gy) == (0, 0) else die if gy < pitch - 2 else dark
                if 22 <= x + y <= 25:
                    c = mix(c, (255, 255, 255), 0.35)
                px(im, x, y, c)
    for (x, y) in m:
        if any((x + ox, y + oy) not in m for ox, oy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
            px(im, x, y, out)
    im.save(f"{OUT}/{name}.png")


COL = {
    "controller": (62, 124, 196),   # blue
    "memory": (52, 150, 84),        # green
    "gpu": (150, 84, 190),          # violet
    "cpu": (214, 168, 52),          # gold
}
wafer("silicon_wafer")
wafer("wafer_controller", COL["controller"], 3)
wafer("wafer_memory", COL["memory"], 4)
wafer("wafer_gpu", COL["gpu"], 6)
wafer("wafer_cpu", COL["cpu"], 5)


# ---------------------------------------------------------------- reticles
def reticle(name, col, kind):
    im = new()
    frame, frame_hi, frame_lo = (58, 62, 74), (96, 102, 118), (30, 32, 42)
    glass, glass_hi = (34, 44, 66), (70, 92, 130)
    rect(im, 2, 3, 29, 28, frame_lo)
    rect(im, 3, 4, 28, 27, frame)
    rect(im, 3, 4, 28, 4, frame_hi), rect(im, 3, 4, 3, 27, frame_hi)
    rect(im, 6, 7, 25, 24, frame_lo)
    rect(im, 7, 8, 24, 23, glass)
    hi, lo = shade(col, 1.3), shade(col, 0.6)
    if kind == "cpu":           # one big die: core + caches
        rect(im, 9, 10, 22, 21, lo)
        rect(im, 10, 11, 15, 16, col), rect(im, 17, 11, 21, 16, col)
        rect(im, 10, 18, 21, 20, hi)
    elif kind == "memory":      # regular cell arrays
        for y in range(10, 22, 4):
            for x in range(9, 22, 5):
                rect(im, x, y, x + 3, y + 2, col)
                rect(im, x, y, x + 3, y, hi)
    elif kind == "gpu":         # many identical units in a grid
        rect(im, 9, 10, 22, 21, lo)
        for y in range(11, 21, 3):
            for x in range(10, 22, 3):
                rect(im, x, y, x + 1, y + 1, col if (x + y) % 2 else hi)
    else:                       # controller: small die with bond pads and traces
        rect(im, 12, 12, 19, 19, col)
        rect(im, 13, 13, 18, 13, hi)
        for i in range(10, 22, 3):
            px(im, i, 10, hi), px(im, i, 21, hi), px(im, 9, i, hi), px(im, 22, i, hi)
        rect(im, 16, 20, 16, 21, lo)
    # glass glare
    for i in range(5):
        px(im, 8 + i, 12 - i, glass_hi)
    for i in range(3):
        px(im, 8 + i, 15 - i, glass_hi)
    # alignment marks in the frame corners + handle notch
    for (x, y) in ((4, 5), (27, 5), (4, 26), (27, 26)):
        px(im, x, y, hi)
    rect(im, 13, 26, 18, 26, col)
    im.save(f"{OUT}/{name}.png")


for k, c in COL.items():
    reticle(f"reticle_{k}", c, k)


# ---------------------------------------------------------------- chips
BODY, BODY_HI, BODY_LO, PIN, PIN_LO = (34, 36, 44), (58, 62, 74), (16, 17, 22), (176, 180, 190), (120, 124, 136)


def chip_controller():
    """SOIC: pins on two long sides."""
    im = new()
    for x in range(8, 25, 3):
        rect(im, x, 7, x + 1, 9, PIN), rect(im, x, 22, x + 1, 24, PIN)
        px(im, x + 1, 9, PIN_LO), px(im, x + 1, 22, PIN_LO)
    rect(im, 5, 10, 26, 21, BODY_LO)
    rect(im, 6, 10, 25, 20, BODY)
    rect(im, 6, 10, 25, 10, BODY_HI)
    rect(im, 8, 17, 9, 18, COL["controller"])                       # pin 1 dot
    rect(im, 12, 13, 22, 13, PIN_LO), rect(im, 12, 15, 19, 15, BODY_HI)   # marking
    rect(im, 12, 17, 16, 17, BODY_HI)
    im.save(f"{OUT}/chip_controller.png")


def chip_memory():
    """TSOP: wide flat package, fine pins left and right."""
    im = new()
    for y in range(8, 24, 2):
        rect(im, 3, y, 5, y, PIN), rect(im, 26, y, 28, y, PIN)
    rect(im, 6, 6, 25, 25, BODY_LO)
    rect(im, 6, 6, 25, 24, BODY)
    rect(im, 6, 6, 25, 6, BODY_HI)
    rect(im, 8, 8, 9, 9, BODY_HI)                                   # notch dot
    rect(im, 9, 12, 22, 13, COL["memory"])                          # marking stripe
    rect(im, 9, 12, 22, 12, shade(COL["memory"], 1.3))
    rect(im, 9, 16, 20, 16, PIN_LO), rect(im, 9, 18, 17, 18, BODY_HI), rect(im, 9, 20, 14, 20, BODY_HI)
    im.save(f"{OUT}/chip_memory.png")


def chip_gpu():
    """Flip-chip BGA: green substrate, bare die with a mirror finish, capacitors around."""
    im = new()
    sub, sub_hi, sub_lo = (38, 120, 70), (64, 160, 96), (20, 70, 42)
    rect(im, 3, 3, 28, 28, sub_lo)
    rect(im, 4, 4, 27, 27, sub)
    rect(im, 4, 4, 27, 4, sub_hi), rect(im, 4, 4, 4, 27, sub_hi)
    px(im, 5, 26, (214, 168, 52)), px(im, 6, 26, (214, 168, 52)), px(im, 5, 25, (214, 168, 52))   # corner mark
    for i in range(7, 26, 3):                                       # decoupling capacitors
        for (x, y) in ((i, 6), (i, 25), (6, i), (25, i)):
            px(im, x, y, (196, 170, 120))
    v = COL["gpu"]
    rect(im, 9, 9, 22, 22, shade(v, 0.5))
    rect(im, 10, 10, 21, 21, v)
    rect(im, 10, 10, 21, 10, shade(v, 1.35)), rect(im, 10, 10, 10, 21, shade(v, 1.35))
    for i in range(6):                                              # die glare
        px(im, 12 + i, 18 - i, shade(v, 1.5))
    for i in range(3):
        px(im, 16 + i, 19 - i, shade(v, 1.3))
    im.save(f"{OUT}/chip_gpu.png")


chip_controller()
chip_memory()
chip_gpu()
