"""Временные модели и текстуры сети данных: кабель данных fcw:data_cable и роутер fcw:router.

Пока художник не сделал настоящие. Стиль мода: белый корпус, чёрные детали, фиолетовый акцент,
без свечения. Кабель тонкий и тёмный, с фиолетовой полосой — чтобы не путать с силовыми.

usage: gen.py <repo root>      (нужен Pillow)
"""
import json
import os
import sys

from PIL import Image

ROOT = sys.argv[1]
RES = f"{ROOT}/mod/src/main/resources"
ASSETS = f"{RES}/assets/fcw"
DATA = f"{RES}/data/fcw"

JACKET = (29, 30, 35, 255)
JACKET_HI = (44, 46, 53, 255)
JACKET_LO = (20, 21, 25, 255)
VIOLET = (168, 139, 255, 255)
VIOLET_LO = (126, 102, 204, 255)
WHITE = (232, 234, 238, 255)
WHITE_HI = (246, 247, 249, 255)
WHITE_LO = (198, 201, 208, 255)
BLACK = (24, 25, 29, 255)
BLACK_HI = (48, 50, 57, 255)
GREY = (120, 124, 134, 255)


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent="\t")
        f.write("\n")


def rect(img, x0, y0, x1, y1, color):
    for y in range(y0, y1):
        for x in range(x0, x1):
            img.putpixel((x, y), color)


# ------------------------------------------------------------------ кабель: текстура 32x32
def cable_texture():
    img = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
    # отвод, вид сбоку 12x8: оболочка, блик сверху, тень снизу, полоса посередине
    rect(img, 0, 0, 12, 8, JACKET)
    rect(img, 0, 0, 12, 1, JACKET_HI)
    rect(img, 0, 7, 12, 8, JACKET_LO)
    rect(img, 0, 3, 12, 4, VIOLET)
    rect(img, 0, 4, 12, 5, VIOLET_LO)
    # торец 8x8: срез с жилой
    rect(img, 12, 0, 20, 8, JACKET_LO)
    rect(img, 13, 1, 19, 7, JACKET)
    rect(img, 15, 3, 17, 5, VIOLET_LO)
    # узел 8x8: муфта
    rect(img, 20, 0, 28, 8, JACKET_HI)
    rect(img, 21, 1, 27, 7, JACKET)
    rect(img, 23, 3, 25, 5, VIOLET)
    return img


SIDE = [0, 0, 6, 4]
CAP = [6, 0, 10, 4]
CORE = [10, 0, 14, 4]


def cable_models():
    tex = {"0": "fcw:block/data_cable", "particle": "fcw:block/data_cable"}
    core = {
        "parent": "minecraft:block/block", "texture_size": [32, 32], "textures": tex,
        "elements": [{
            "name": "core", "from": [6, 6, 6], "to": [10, 10, 10],
            "faces": {d: {"uv": CORE, "texture": "#0"} for d in ("north", "east", "south", "west", "up", "down")},
        }],
    }
    arm = {
        "parent": "minecraft:block/block", "texture_size": [32, 32], "textures": tex,
        "elements": [{
            "name": "arm_north", "from": [6, 6, 0], "to": [10, 10, 6],
            "faces": {
                "north": {"uv": CAP, "texture": "#0", "cullface": "north"},
                "east": {"uv": SIDE, "texture": "#0"},
                "west": {"uv": SIDE, "texture": "#0"},
                "up": {"uv": SIDE, "texture": "#0", "rotation": 90},
                "down": {"uv": SIDE, "texture": "#0", "rotation": 90},
            },
        }],
    }
    # в инвентаре: прямой отрезок от стенки до стенки
    item = {
        "parent": "minecraft:block/block", "texture_size": [32, 32], "textures": tex,
        "elements": [
            {"name": "run", "from": [0, 6, 6], "to": [16, 10, 10],
             "faces": {
                 "north": {"uv": [0, 0, 6, 4], "texture": "#0"},
                 "south": {"uv": [0, 0, 6, 4], "texture": "#0"},
                 "up": {"uv": [0, 0, 6, 4], "texture": "#0"},
                 "down": {"uv": [0, 0, 6, 4], "texture": "#0"},
                 "east": {"uv": CAP, "texture": "#0"},
                 "west": {"uv": CAP, "texture": "#0"},
             }},
            {"name": "joint", "from": [5.5, 5.5, 5.5], "to": [10.5, 10.5, 10.5],
             "faces": {d: {"uv": CORE, "texture": "#0"} for d in ("north", "east", "south", "west", "up", "down")}},
        ],
    }
    write_json(f"{ASSETS}/models/block/data_cable_core.json", core)
    write_json(f"{ASSETS}/models/block/data_cable_arm.json", arm)
    write_json(f"{ASSETS}/models/block/data_cable_inventory.json", item)
    write_json(f"{ASSETS}/models/item/data_cable.json", {"parent": "fcw:block/data_cable_inventory"})
    rot = {"north": {}, "east": {"y": 90}, "south": {"y": 180}, "west": {"y": 270}, "up": {"x": 270}, "down": {"x": 90}}
    parts = [{"apply": {"model": "fcw:block/data_cable_core"}}]
    for side, r in rot.items():
        parts.append({"when": {side: "true"}, "apply": {"model": "fcw:block/data_cable_arm", **r}})
    write_json(f"{ASSETS}/blockstates/data_cable.json", {"multipart": parts})


# ------------------------------------------------------------------ роутер: текстура 64x64
def router_texture():
    img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    # крышка 28x28: белая, по краю кант, сзади вентиляционные прорези
    rect(img, 0, 0, 28, 28, WHITE)
    rect(img, 0, 0, 28, 1, WHITE_HI)
    rect(img, 0, 0, 1, 28, WHITE_HI)
    rect(img, 0, 27, 28, 28, WHITE_LO)
    rect(img, 27, 0, 28, 28, WHITE_LO)
    for i in range(5):
        rect(img, 8 + i * 3, 19, 9 + i * 3, 25, BLACK)
    rect(img, 4, 3, 24, 4, WHITE_LO)
    # передняя стенка 28x16: чёрная панель, фиолетовая метка, три серых индикатора
    rect(img, 28, 0, 56, 16, WHITE)
    rect(img, 28, 0, 56, 1, WHITE_HI)
    rect(img, 28, 15, 56, 16, WHITE_LO)
    rect(img, 30, 5, 54, 12, BLACK)
    rect(img, 30, 5, 54, 6, BLACK_HI)
    rect(img, 32, 7, 36, 10, VIOLET)
    rect(img, 32, 9, 36, 10, VIOLET_LO)
    for i in range(3):
        rect(img, 44 + i * 3, 8, 46 + i * 3, 9, GREY)
    # боковая и задняя стенки 28x16: белые, тёмный шов у основания
    rect(img, 28, 16, 56, 32, WHITE)
    rect(img, 28, 16, 56, 17, WHITE_HI)
    rect(img, 28, 29, 56, 30, WHITE_LO)
    rect(img, 28, 30, 56, 32, BLACK)
    # дно 28x28
    rect(img, 0, 28, 28, 56, BLACK)
    rect(img, 2, 30, 26, 54, BLACK_HI)
    # антенна 2x16 и её торец
    rect(img, 56, 0, 58, 16, BLACK)
    rect(img, 56, 0, 57, 16, BLACK_HI)
    rect(img, 56, 14, 58, 16, GREY)
    rect(img, 58, 0, 60, 2, BLACK_HI)
    return img


def router_models():
    tex = {"0": "fcw:block/router", "particle": "fcw:block/router"}
    side = {"uv": [7, 4, 14, 8], "texture": "#0"}
    ant = {"uv": [14, 0, 14.5, 4], "texture": "#0"}

    def antenna(x):
        return {"name": "antenna", "from": [x, 8, 7.5], "to": [x + 1, 16, 8.5],
                "faces": {"north": ant, "east": ant, "south": ant, "west": ant,
                          "up": {"uv": [14.5, 0, 15, 0.5], "texture": "#0", "cullface": "up"}}}

    model = {
        "parent": "minecraft:block/block", "texture_size": [64, 64], "textures": tex,
        "elements": [
            {"name": "body", "from": [1, 0, 1], "to": [15, 8, 15],
             "faces": {
                 "north": {"uv": [7, 0, 14, 4], "texture": "#0"},
                 "east": side, "south": side, "west": side,
                 "up": {"uv": [0, 0, 7, 7], "texture": "#0"},
                 "down": {"uv": [0, 7, 7, 14], "texture": "#0", "cullface": "down"},
             }},
            antenna(2.5), antenna(12.5),
        ],
    }
    write_json(f"{ASSETS}/models/block/router.json", model)
    write_json(f"{ASSETS}/models/item/router.json", {"parent": "fcw:block/router"})
    write_json(f"{ASSETS}/blockstates/router.json", {"variants": {
        "facing=north": {"model": "fcw:block/router"},
        "facing=east": {"model": "fcw:block/router", "y": 90},
        "facing=south": {"model": "fcw:block/router", "y": 180},
        "facing=west": {"model": "fcw:block/router", "y": 270},
    }})


def loot(name):
    write_json(f"{DATA}/loot_table/blocks/{name}.json", {
        "type": "minecraft:block",
        "pools": [{"rolls": 1, "entries": [{"type": "minecraft:item", "name": f"fcw:{name}"}],
                   "conditions": [{"condition": "minecraft:survives_explosion"}]}],
    })


cable_texture().save(f"{ASSETS}/textures/block/data_cable.png")
router_texture().save(f"{ASSETS}/textures/block/router.png")
cable_models()
router_models()
loot("data_cable")
loot("router")
