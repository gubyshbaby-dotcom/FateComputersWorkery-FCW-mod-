"""Временная модель и текстура кабеля данных fcw:data_cable (и таблицы добычи блоков сети).

Пока художник не сделал настоящую. Кабель тонкий и тёмный, с фиолетовой полосой — чтобы не путать
с силовыми; без свечения. У роутера модель готовая (набор 01 художника), здесь её нет.

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


def loot(name):
    write_json(f"{DATA}/loot_table/blocks/{name}.json", {
        "type": "minecraft:block",
        "pools": [{"rolls": 1, "entries": [{"type": "minecraft:item", "name": f"fcw:{name}"}],
                   "conditions": [{"condition": "minecraft:survives_explosion"}]}],
    })


cable_texture().save(f"{ASSETS}/textures/block/data_cable.png")
cable_models()
loot("data_cable")
loot("router")
