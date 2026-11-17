"""Turn a machine 'dyn' model into a standalone item model: crop the faces it uses out of the
machine atlas into a small private texture, recentre the elements, add display transforms.

usage: port3d.py <src model json> <atlas png> <out model json> <out texture png> <texture id> <kind>
"""
import json
import sys
from PIL import Image

src, atlas_path, out_model, out_tex, tex_id, kind = sys.argv[1:7]
m = json.load(open(src))
atlas = Image.open(atlas_path).convert("RGBA")
W, H = atlas.size

# ---- collect face rectangles (uv is in 0..16 units of the whole atlas)
faces = []
for e in m["elements"]:
    for side, f in e["faces"].items():
        u0, v0, u1, v1 = f["uv"]
        x0, x1 = sorted((round(u0 / 16 * W), round(u1 / 16 * W)))
        y0, y1 = sorted((round(v0 / 16 * H), round(v1 / 16 * H)))
        faces.append((f, (x0, y0, max(x1, x0 + 1), max(y1, y0 + 1)), u0 > u1, v0 > v1))

# ---- shelf-pack unique rectangles into a square power-of-two texture
rects = sorted({r for _, r, _, _ in faces}, key=lambda r: -(r[3] - r[1]))
size = 16
while True:
    pos, x, y, row = {}, 0, 0, 0
    ok = True
    for r in rects:
        w, h = r[2] - r[0], r[3] - r[1]
        if x + w > size:
            x, y, row = 0, y + row, 0
        if w > size or y + h > size:
            ok = False
            break
        pos[r] = (x, y)
        x, row = x + w, max(row, h)
    if ok:
        break
    size *= 2
tex = Image.new("RGBA", (size, size), (0, 0, 0, 0))
for r, (x, y) in pos.items():
    tex.paste(atlas.crop(r), (x, y))
tex.save(out_tex)

for f, r, flip_u, flip_v in faces:
    x, y = pos[r]
    u0, v0 = x / size * 16, y / size * 16
    u1, v1 = (x + r[2] - r[0]) / size * 16, (y + r[3] - r[1]) / size * 16
    if flip_u:
        u0, u1 = u1, u0
    if flip_v:
        v0, v1 = v1, v0
    f["uv"] = [round(u0, 4), round(v0, 4), round(u1, 4), round(v1, 4)]
    f["texture"] = "#main"

# ---- recentre on the block centre
lo = [min(e["from"][i] for e in m["elements"]) for i in range(3)]
hi = [max(e["to"][i] for e in m["elements"]) for i in range(3)]
shift = [8 - (lo[i] + hi[i]) / 2 for i in range(3)]
for e in m["elements"]:
    e["from"] = [round(e["from"][i] + shift[i], 3) for i in range(3)]
    e["to"] = [round(e["to"][i] + shift[i], 3) for i in range(3)]
    if "rotation" in e:
        e["rotation"]["origin"] = [round(e["rotation"]["origin"][i] + shift[i], 3) for i in range(3)]
extent = max(hi[i] - lo[i] for i in range(3))
s = round(min(1.0, 15.0 / extent), 3)

if kind == "tall":      # a long vertical piece: lean it in the slot
    display = {
        "gui": {"rotation": [20, 30, -40], "translation": [0, 0, 0], "scale": [s * 1.15] * 3},
        "ground": {"translation": [0, 2, 0], "scale": [s * 0.6] * 3},
        "fixed": {"rotation": [0, 0, -45], "scale": [s] * 3},
        "thirdperson_righthand": {"rotation": [0, 0, 0], "translation": [0, 3, 1], "scale": [s * 0.6] * 3},
        "thirdperson_lefthand": {"rotation": [0, 0, 0], "translation": [0, 3, 1], "scale": [s * 0.6] * 3},
        "firstperson_righthand": {"rotation": [0, 30, 0], "translation": [0, 4, 0], "scale": [s * 0.7] * 3},
        "firstperson_lefthand": {"rotation": [0, 30, 0], "translation": [0, 4, 0], "scale": [s * 0.7] * 3},
    }
else:                   # a flat plate: show its top face
    display = {
        "gui": {"rotation": [60, 0, 0], "translation": [0, 0, 0], "scale": [s * 1.5] * 3},
        "ground": {"translation": [0, 2, 0], "scale": [s * 0.6] * 3},
        "fixed": {"rotation": [90, 0, 0], "scale": [s * 1.4] * 3},
        "thirdperson_righthand": {"rotation": [70, 0, 0], "translation": [0, 2, 1], "scale": [s * 0.6] * 3},
        "thirdperson_lefthand": {"rotation": [70, 0, 0], "translation": [0, 2, 1], "scale": [s * 0.6] * 3},
        "firstperson_righthand": {"rotation": [20, 20, 0], "translation": [0, 4, 0], "scale": [s * 0.8] * 3},
        "firstperson_lefthand": {"rotation": [20, 20, 0], "translation": [0, 4, 0], "scale": [s * 0.8] * 3},
    }
for d in display.values():
    for k, v in d.items():
        d[k] = [round(x, 3) for x in v]

out = {
    "credit": "Fate Computers Workery: item model made from a machine part",
    "parent": "minecraft:block/block",
    "ambientocclusion": False,
    "gui_light": "front",
    "texture_size": [size, size],
    "textures": {"particle": tex_id, "main": tex_id},
    "elements": m["elements"],
    "display": display,
}
json.dump(out, open(out_model, "w"), indent="\t")
print(out_model, "texture", size, "elements", len(m["elements"]), "extent", extent, "scale", s)
