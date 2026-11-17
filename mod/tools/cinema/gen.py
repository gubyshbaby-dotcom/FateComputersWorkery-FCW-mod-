"""Кино: временные модели и текстуры экрана и кресла, значок лампы проектора, таблицы добычи,
теги инструментов и строки lang.

Экран и кресло нарисованы здесь, пока художник не сделал настоящие: матовое белое полотно с тонкой
чёрной рамкой и тёмно-красное кресло с подлокотниками. Модель и текстуры самого проектора готовые
(набор 02 художника), этот скрипт их не трогает. Рецепты кино лежат в разделе «кино» в
mod/tools/production/gen.py, устройство описано в docs/CINEMA.md.

usage: gen.py <repo root>      (нужен Pillow)
"""
import json
import os
import random
import sys

from PIL import Image, ImageDraw

ROOT = sys.argv[1]
RES = f"{ROOT}/mod/src/main/resources"
ASSETS = f"{RES}/assets/fcw"
DATA = f"{RES}/data"

FACES = ("north", "east", "south", "west", "up", "down")
ROT = {"north": 0, "east": 90, "south": 180, "west": 270}


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent="\t")
        f.write("\n")


def noise(name, base, spread, seed, size=16):
    """Ровная заливка с лёгким зерном: на ней не видно, что грани берут разные куски текстуры."""
    rnd = random.Random(seed)
    img = Image.new("RGBA", (size, size))
    for y in range(size):
        for x in range(size):
            d = rnd.randint(-spread, spread)
            img.putpixel((x, y), tuple(max(0, min(255, c + d)) for c in base) + (255,))
    os.makedirs(f"{ASSETS}/textures/block", exist_ok=True)
    img.save(f"{ASSETS}/textures/block/{name}.png")


def box(name, frm, to, tex, skip=()):
    """Элемент модели; uv каждой грани равен её размеру в пикселях, чтобы зерно не растягивалось."""
    dx, dy, dz = (to[i] - frm[i] for i in range(3))
    size = {"north": (dx, dy), "south": (dx, dy), "east": (dz, dy), "west": (dz, dy), "up": (dx, dz), "down": (dx, dz)}
    faces = {}
    for f in FACES:
        if f in skip:
            continue
        w, h = size[f]
        faces[f] = {"uv": [0, 0, min(16, max(w, 0.5)), min(16, max(h, 0.5))], "texture": tex}
    return {"name": name, "from": frm, "to": to, "faces": faces}


def model(name, textures, elements, display=None):
    m = {"parent": "minecraft:block/block", "textures": textures, "elements": elements}
    if display:
        m["display"] = display
    write_json(f"{ASSETS}/models/block/{name}.json", m)


def multipart(name, parts):
    """parts: [(условие или None, модель)]; каждая часть повторяется для четырёх сторон."""
    out = []
    for when, mdl in parts:
        for facing, y in ROT.items():
            apply = {"model": f"fcw:block/{mdl}"}
            if y:
                apply["y"] = y
            out.append({"when": {"facing": facing, **(when or {})}, "apply": apply})
    write_json(f"{ASSETS}/blockstates/{name}.json", {"multipart": out})


def loot(name, functions=None):
    entry = {"type": "minecraft:item", "name": f"fcw:{name}"}
    if functions:
        entry["functions"] = functions
    write_json(f"{DATA}/fcw/loot_table/blocks/{name}.json", {
        "type": "minecraft:block",
        "pools": [{"rolls": 1, "entries": [entry], "conditions": [{"condition": "minecraft:survives_explosion"}]}],
    })


# ------------------------------------------------------------------ экран
# Лицом на север: полотно толщиной 1/16 у южной стенки клетки. Лево и право считаются от зрителя.
def screen():
    noise("projection_screen", (236, 236, 232), 3, 1)
    noise("projection_screen_frame", (16, 16, 18), 3, 2)
    tex = {"0": "fcw:block/projection_screen", "1": "fcw:block/projection_screen_frame",
           "particle": "fcw:block/projection_screen"}
    panel = box("panel", [0, 0, 15], [16, 16, 16], "#0")
    panel["faces"]["south"]["texture"] = "#1"
    frames = {
        "up": box("frame_up", [0, 15, 14.75], [16, 16, 15], "#1", skip=("south",)),
        "down": box("frame_down", [0, 0, 14.75], [16, 1, 15], "#1", skip=("south",)),
        "left": box("frame_left", [15, 0, 14.75], [16, 16, 15], "#1", skip=("south",)),
        "right": box("frame_right", [0, 0, 14.75], [1, 16, 15], "#1", skip=("south",)),
    }
    model("projection_screen", tex, [panel])
    for side, el in frames.items():
        model(f"projection_screen_frame_{side}", tex, [el])
    model("projection_screen_item", tex, [panel] + list(frames.values()), {
        "gui": {"rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [0.9, 0.9, 0.9]},
        "ground": {"translation": [0, 2, 0], "scale": [0.4, 0.4, 0.4]},
        "fixed": {"rotation": [0, 180, 0], "translation": [0, 0, -7], "scale": [1, 1, 1]},
        "thirdperson_righthand": {"rotation": [75, 0, 0], "translation": [0, 2, -4], "scale": [0.375, 0.375, 0.375]},
        "firstperson_righthand": {"rotation": [0, 160, 0], "translation": [0, 3, 6], "scale": [0.4, 0.4, 0.4]},
    })
    # рамка рисуется с той стороны, где соседа нет
    multipart("projection_screen", [(None, "projection_screen")]
              + [({side: "false"}, f"projection_screen_frame_{side}") for side in frames])
    write_json(f"{ASSETS}/models/item/projection_screen.json", {"parent": "fcw:block/projection_screen_item"})
    loot("projection_screen")


# ------------------------------------------------------------------ кресло
# Лицом на север (туда смотрит сидящий). Лево и право считаются от сидящего: слева у него запад.
def seat():
    noise("cinema_seat_fabric", (122, 24, 30), 6, 3)
    noise("cinema_seat_frame", (26, 24, 26), 3, 4)
    tex = {"0": "fcw:block/cinema_seat_fabric", "1": "fcw:block/cinema_seat_frame",
           "particle": "fcw:block/cinema_seat_fabric"}
    body = [
        box("base", [3, 0, 4], [13, 2, 13], "#1"),
        box("cushion", [2, 2, 2], [14, 6, 13], "#0"),
        box("back", [2, 6, 11.5], [14, 16, 13.5], "#0"),
        box("back_shell", [1.5, 2, 13.5], [14.5, 16, 14.5], "#1"),
    ]
    # с краю ряда подлокотник целый (2 пикселя), между креслами каждое рисует половину общего
    arm = {
        "left_end": [box("arm", [0, 2, 3], [2, 10, 13], "#1"), box("arm_pad", [0, 10, 3], [2, 10.75, 12], "#0")],
        "right_end": [box("arm", [14, 2, 3], [16, 10, 13], "#1"), box("arm_pad", [14, 10, 3], [16, 10.75, 12], "#0")],
        "left_shared": [box("arm", [0, 2, 3], [1, 10, 13], "#1", skip=("west",)),
                        box("arm_pad", [0, 10, 3], [1, 10.75, 12], "#0", skip=("west",)),
                        box("cushion_fill", [1, 2, 2], [2, 6, 13], "#0"),
                        box("back_fill", [0, 6, 11.5], [2, 16, 13.5], "#0", skip=("west",)),
                        box("shell_fill", [0, 2, 13.5], [1.5, 16, 14.5], "#1", skip=("west",))],
        "right_shared": [box("arm", [15, 2, 3], [16, 10, 13], "#1", skip=("east",)),
                         box("arm_pad", [15, 10, 3], [16, 10.75, 12], "#0", skip=("east",)),
                         box("cushion_fill", [14, 2, 2], [15, 6, 13], "#0"),
                         box("back_fill", [14, 6, 11.5], [16, 16, 13.5], "#0", skip=("east",)),
                         box("shell_fill", [14.5, 2, 13.5], [16, 16, 14.5], "#1", skip=("east",))],
    }
    model("cinema_seat", tex, body)
    for k, els in arm.items():
        model(f"cinema_seat_arm_{k}", tex, els)
    model("cinema_seat_item", tex, body + arm["left_end"] + arm["right_end"], {
        "gui": {"rotation": [30, 45, 0], "translation": [0, 0, 0], "scale": [0.625, 0.625, 0.625]},
        "ground": {"translation": [0, 3, 0], "scale": [0.25, 0.25, 0.25]},
        "fixed": {"rotation": [0, 180, 0], "scale": [0.5, 0.5, 0.5]},
        "thirdperson_righthand": {"rotation": [75, 45, 0], "translation": [0, 2.5, 0], "scale": [0.375, 0.375, 0.375]},
        "firstperson_righthand": {"rotation": [0, 225, 0], "scale": [0.4, 0.4, 0.4]},
    })
    multipart("cinema_seat", [
        (None, "cinema_seat"),
        ({"left": "false"}, "cinema_seat_arm_left_end"), ({"left": "true"}, "cinema_seat_arm_left_shared"),
        ({"right": "false"}, "cinema_seat_arm_right_end"), ({"right": "true"}, "cinema_seat_arm_right_shared"),
    ])
    write_json(f"{ASSETS}/models/item/cinema_seat.json", {"parent": "fcw:block/cinema_seat_item"})
    loot("cinema_seat")


# ------------------------------------------------------------------ лампа: значок 32x32
def lamp():
    img = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # отражатель: чаша, раскрытая вправо вверх
    d.pieslice([4, 4, 28, 28], 100, 350, fill=(150, 154, 160, 255), outline=(70, 73, 80, 255))
    d.pieslice([7, 7, 25, 25], 100, 350, fill=(206, 210, 214, 255))
    # колба с дугой
    d.ellipse([11, 10, 21, 20], fill=(250, 244, 214, 255), outline=(190, 170, 110, 255))
    d.ellipse([14, 13, 18, 17], fill=(255, 255, 246, 255))
    # керамический цоколь и выводы
    d.rectangle([6, 22, 13, 26], fill=(214, 206, 190, 255), outline=(120, 112, 100, 255))
    d.line([8, 27, 8, 29], fill=(184, 115, 51, 255))
    d.line([11, 27, 11, 29], fill=(184, 115, 51, 255))
    os.makedirs(f"{ASSETS}/textures/item", exist_ok=True)
    img.save(f"{ASSETS}/textures/item/projector_lamp.png")
    write_json(f"{ASSETS}/models/item/projector_lamp.json",
               {"parent": "minecraft:item/generated", "textures": {"layer0": "fcw:item/projector_lamp"}})


# ------------------------------------------------------------------ добыча и инструменты
def loot_and_tags():
    # проектор уносит с собой наработку лампы (ProjectorBlockEntity.collectImplicitComponents)
    loot("projector", [{"function": "minecraft:copy_components", "source": "block_entity",
                        "include": ["minecraft:custom_data"]}])
    for tool, blocks in (("pickaxe", ["fcw:projector"]), ("axe", ["fcw:cinema_seat"])):
        path = f"{DATA}/minecraft/tags/block/mineable/{tool}.json"
        tag = json.load(open(path, encoding="utf-8"))
        for b in blocks:
            if b not in tag["values"]:
                tag["values"].append(b)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(tag, f, ensure_ascii=False, indent=2)
            f.write("\n")


# ------------------------------------------------------------------ lang
LANG = {
    "block.fcw.projector": ("Проектор", "Projector"),
    "block.fcw.projection_screen": ("Проекционный экран", "Projection Screen"),
    "block.fcw.cinema_seat": ("Кресло кинозала", "Cinema Seat"),
    "item.fcw.projector_lamp": ("Лампа проектора", "Projector Lamp"),
    "entity.fcw.seat": ("Сиденье", "Seat"),
    "device.fcw.projector": ("Проектор", "Projector"),
    "tooltip.fcw.projector": ("Проецирует экран компьютера на экран или стену. ПКМ: включить или выключить",
                              "Projects the computer screen onto a screen or a wall. Right click: switch on or off"),
    "tooltip.fcw.projector.mount": ("Ставится на пол или под потолок (ПКМ по нижней грани блока)",
                                    "Stands on the floor or hangs from the ceiling (right click the underside of a block)"),
    "tooltip.fcw.projector.lamp_used": ("Лампа отработала %s ч", "Lamp hours used: %s"),
    "tooltip.fcw.projection_screen": ("Ставится на стену. Соседние полотна сливаются в один экран до 16×9",
                                      "Goes on a wall. Adjacent panels join into one screen up to 16×9"),
    "tooltip.fcw.projection_screen.fit": ("Проектор вписывает картинку в экран; на голой стене она тусклее",
                                          "A projector fits its picture into the screen; on a bare wall it is dimmer"),
    "tooltip.fcw.cinema_seat": ("ПКМ: сесть. Shift: встать. Соседние кресла делят подлокотник",
                                "Right click: sit down. Sneak: stand up. Neighbours share an armrest"),
    "tooltip.fcw.projector_lamp": ("ПКМ по выключенному проектору: заменить лампу",
                                   "Right click a switched-off projector: replace its lamp"),
    "message.fcw.projector.on": ("Проектор включён, лампа разогревается. Наработка лампы: %s из %s ч",
                                 "Projector on, the lamp is warming up. Lamp hours: %s of %s"),
    "message.fcw.projector.on_eternal": ("Проектор включён, лампа разогревается",
                                         "Projector on, the lamp is warming up"),
    "message.fcw.projector.off": ("Проектор выключен, вентилятор остужает лампу: %s с",
                                  "Projector off, the fan is cooling the lamp: %s s"),
    "message.fcw.projector.cooling": ("Лампа остывает, включить можно через %s с",
                                      "The lamp is cooling down, it can be switched on in %s s"),
    "message.fcw.projector.lamp_dead": ("Лампа перегорела. Замените её: ПКМ лампой проектора",
                                        "The lamp has burnt out. Replace it: right click with a projector lamp"),
    "message.fcw.projector.lamp_burnt": ("%s: лампа перегорела", "%s: the lamp has burnt out"),
    "message.fcw.projector.lamp_replaced": ("Лампа заменена", "Lamp replaced"),
    "message.fcw.projector.lamp_hot": ("Сначала выключите проектор и дайте лампе остыть",
                                       "Switch the projector off and let the lamp cool down first"),
    "message.fcw.projector.lamp_new": ("Лампа ещё новая", "The lamp is still new"),
    "message.fcw.projector.no_power": ("Нет питания: проектору нужно %s FE/т",
                                       "No power: the projector needs %s FE/t"),
    "message.fcw.projector.power_lost": ("%s: погас, нет энергии", "%s: went dark, out of energy"),
    "message.fcw.seat.taken": ("Кресло занято", "This seat is taken"),
}


def lang():
    for i, name in enumerate(("ru_ru", "en_us")):
        path = f"{ASSETS}/lang/{name}.json"
        d = json.load(open(path, encoding="utf-8"))
        for k, v in LANG.items():
            d[k] = v[i]
        with open(path, "w", encoding="utf-8") as f:
            json.dump(dict(sorted(d.items())), f, ensure_ascii=False, indent=2)
            f.write("\n")


screen()
seat()
lamp()
loot_and_tags()
lang()
print("cinema: screen, seat, lamp icon, loot tables, tags,", len(LANG), "lang keys")
