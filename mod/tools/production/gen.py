"""Generates recipes, tags, item models, lang strings and the player docs of the production chain.
usage: gen.py <repo root>"""
import json
import os
import math
import shutil
import sys

sys.path.insert(0, __import__("os").path.dirname(__file__))
import build_data as BD  # noqa: E402

ROOT = sys.argv[1]
RES = f"{ROOT}/mod/src/main/resources"
DATA = f"{RES}/data"
ASSETS = f"{RES}/assets/fcw"


def write(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)
        f.write("\n")


def ing(s):
    return {"tag": s[1:]} if s.startswith("#") else {"item": s}


# =============================================================== names
STEEL, SILICON, COALS = "#c:ingots/steel", "#c:silicon", "#minecraft:coals"

# id -> (ru, en, ru from, ru to, en from, en to)
ITEMS = {
    "metallurgical_silicon": ("Технический кремний", "Metallurgical Silicon",
                              "Дуговая печь: кварц + уголь", "В дуговую печь с редстоуном: поликремний. Детали машин",
                              "Arc furnace: quartz + coal", "Arc furnace with redstone: polysilicon. Machine parts"),
    "steel_ingot": ("Стальной слиток", "Steel Ingot",
                    "Дуговая печь: железо + уголь", "Корпуса, машины, блоки литографа",
                    "Arc furnace: iron + coal", "Cases, machines, lithography scanner blocks"),
    "polysilicon": ("Поликремний", "Polysilicon",
                    "Дуговая печь: технический кремний + редстоун", "В ростовую печь: монокристалл",
                    "Arc furnace: metallurgical silicon + redstone", "Crystal puller: silicon boule"),
    "silicon_boule": ("Монокристалл кремния", "Silicon Boule",
                      "Ростовая печь: 8 поликремния", "Резка и полировка: 8 пластин",
                      "Crystal puller: 8 polysilicon", "Wafering cell: 8 wafers"),
    "silicon_wafer": ("Кремниевая пластина", "Silicon Wafer",
                      "Резка и полировка монокристалла", "В литограф вместе с фотошаблоном",
                      "Wafering cell: silicon boule", "Lithography scanner, together with a photomask"),
    "pcb_blank": ("Заготовка платы", "Blank Circuit Board",
                  "Верстак: медь, бумага, зелёный краситель", "Линия электроники: сборка плат и модулей памяти",
                  "Crafting table: copper, paper, green dye", "Electronics line: boards and memory modules"),
    "reticle_controller": ("Фотошаблон: контроллер", "Photomask: Controller", None, None, None, None),
    "reticle_memory": ("Фотошаблон: память", "Photomask: Memory", None, None, None, None),
    "reticle_gpu": ("Фотошаблон: графический процессор", "Photomask: Graphics Chip", None, None, None, None),
    "reticle_cpu": ("Фотошаблон: процессор", "Photomask: Processor", None, None, None, None),
    "wafer_controller": ("Пластина с контроллерами", "Controller Wafer", None,
                         "Корпусировка: микросхемы контроллеров", None, "Chip packaging: controller chips"),
    "wafer_memory": ("Пластина с памятью", "Memory Wafer", None,
                     "Корпусировка: микросхемы памяти", None, "Chip packaging: memory chips"),
    "wafer_gpu": ("Пластина с графическими процессорами", "Graphics Chip Wafer", None,
                  "Корпусировка: графические процессоры", None, "Chip packaging: graphics chips"),
    "wafer_cpu": ("Пластина с процессорами", "Processor Wafer", None,
                  "Корпусировка: процессоры (чем старше, тем больше пластин)", None,
                  "Chip packaging: processors (better ones take more wafers)"),
    "chip_controller": ("Микросхема контроллера", "Controller Chip",
                        "Корпусировка пластины с контроллерами", "Сборка плат: материнская плата, видеокарта, память",
                        "Chip packaging: controller wafer", "Board assembly: motherboard, graphics card, memory"),
    "chip_memory": ("Микросхема памяти (TSOP)", "Memory Chip (TSOP)",
                    "Корпусировка пластины с памятью", "Сборка плат: модули памяти, видеокарта",
                    "Chip packaging: memory wafer", "Board assembly: memory modules, graphics card"),
    "chip_gpu": ("Графический процессор", "Graphics Chip",
                 "Корпусировка пластины с графическими процессорами", "Сборка плат: видеокарта",
                 "Chip packaging: graphics chip wafer", "Board assembly: graphics card"),
}
for k in ("controller", "memory", "gpu", "cpu"):
    ru, en = ITEMS[f"reticle_{k}"][:2]
    wru, wen = ITEMS[f"wafer_{k}"][:2]
    ITEMS[f"reticle_{k}"] = (ru, en, "Верстак: сталь, стекло, кварц и образец",
                             f"В слот шаблона литографа: {wru.lower()}. Не расходуется, но изнашивается",
                             "Crafting table: steel, glass, quartz and a sample",
                             f"Lithography scanner mask slot: {wen.lower()}. Not consumed, but wears out")
    w = ITEMS[f"wafer_{k}"]
    ITEMS[f"wafer_{k}"] = (w[0], w[1], f"Литограф: пластина + {ru.lower()}", w[3],
                           f"Lithography scanner: wafer + {en.lower()}", w[5])

# =============================================================== construction components
WHERE_RU = {"craft": "Верстак", "smelting": "Печь", "blasting": "Плавильная печь", "stonecutting": "Камнерез",
            "wafering": "Резка и полировка", "legacy": "Верстак: чипы младшего тира (для первого литографа)"}
WHERE_EN = {"craft": "Crafting table", "smelting": "Furnace", "blasting": "Blast furnace", "stonecutting": "Stonecutter",
            "wafering": "Wafering cell", "legacy": "Crafting table: bench chips (for the first scanner)"}
_made = {r[0]: "craft" for r in BD.CRAFT}
_made.update({r[2]: r[0] for r in BD.COOK})
_made.update({r[1]: "stonecutting" for r in BD.STONECUT})
_made.update({r[1]: r[0] for r in BD.MACHINE_RECIPES})
_made.update({r[3]: "legacy" for r in BD.LEGACY_CHIPS})
_used = {}
for _m, _stages in BD.STAGES.items():
    for (_sid, _ru, _en, _sec, _items) in _stages:
        for _it, _n in _items:
            _used.setdefault(_it, {}).setdefault(_m, 0)
            _used[_it][_m] += _n
for _k, (_ru, _en, _stack, _group) in BD.COMPONENTS.items():
    _u = _used.get(_k)
    if _k.startswith("proto_"):
        _u = None
    to_ru = ("Нужен: " + ", ".join(f"{BD.MACHINE_NAMES[m][0]} ×{n}" for m, n in _u.items())) if _u else "Деталь других компонентов"
    to_en = ("Needed by: " + ", ".join(f"{BD.MACHINE_NAMES[m][1]} ×{n}" for m, n in _u.items())) if _u else "A part of other components"
    ITEMS[_k] = (_ru, _en, WHERE_RU[_made[_k]], to_ru, WHERE_EN[_made[_k]], to_en)

# names of everything else that appears in the docs
RU = {k.split(".")[-1]: v for k, v in json.load(open(f"{ASSETS}/lang/ru_ru.json", encoding="utf-8")).items()
      if k.startswith(("item.fcw.", "block.fcw."))}
RU.update({k: v[0] for k, v in ITEMS.items()})
RU.update({
    "arc_furnace_blueprint": "Проект: дуговая печь", "crystal_puller_blueprint": "Проект: ростовая печь",
    "wafering_cell_blueprint": "Проект: резка и полировка", "chip_line_blueprint": "Проект: линия электроники",
    "euv_scanner_mk2_blueprint": "Проект: литограф EUV Mk2", "wrench": "Гаечный ключ",
    "euv_casing": "Корпус литографа", "euv_casing_glass": "Корпус литографа со стеклом",
    "euv_casing_vent": "Корпус литографа с решёткой", "euv_frame": "Каркас литографа",
    "euv_vacuum_chamber": "Стенка вакуумной камеры", "euv_mirror": "Многослойное зеркало",
    "euv_collector": "Коллектор EUV", "euv_droplet_generator": "Генератор капель олова",
    "euv_laser_amplifier": "Усилитель CO2-лазера", "euv_chiller": "Чиллер", "euv_wafer_stage": "Столик пластины",
    "euv_reticle_stage": "Столик маски", "euv_wafer_handler": "Модуль робота-загрузчика",
    "euv_load_port": "Порт загрузки FOUP", "euv_vacuum_pump": "Турбомолекулярный насос",
    "euv_power_unit": "Силовой шкаф", "euv_controller": "Контроллер литографа", "euv_port_energy": "Ввод питания",
    "euv_port_coolant": "Порт охлаждения", "euv_port_gas": "Порт газов", "euv_port_exhaust": "Порт вытяжки",
    "euv_port_data": "Порт данных", "euv_port_item": "Люк предметов",
})
VANILLA = {
    "quartz": "кварц", "redstone": "редстоун", "iron_ingot": "железный слиток", "gold_ingot": "золотой слиток",
    "copper_ingot": "медный слиток", "iron_nugget": "кусочек железа", "gold_nugget": "кусочек золота",
    "diamond": "алмаз", "netherite_scrap": "незеритовый лом", "glowstone_dust": "светопыль", "paper": "бумага",
    "green_dye": "зелёный краситель", "stone_button": "каменная кнопка", "glass": "стекло",
    "glass_pane": "стеклянная панель", "note_block": "нотный блок", "redstone_block": "блок редстоуна",
    "iron_bars": "железная решётка", "piston": "поршень", "hopper": "воронка", "blast_furnace": "плавильная печь",
    "bricks": "кирпичи", "iron_block": "железный блок", "copper_block": "медный блок", "quartz_block": "кварцевый блок",
    "obsidian": "обсидиан", "amethyst_shard": "осколок аметиста", "dispenser": "раздатчик", "packed_ice": "плотный лёд",
    "smooth_quartz": "гладкий кварц", "lightning_rod": "громоотвод", "cauldron": "котёл",
    "glass_bottle": "пузырёк", "iron_trapdoor": "железный люк", "comparator": "компаратор",
    "white_concrete_powder": "белый цемент", "polished_granite": "полированный гранит", "clay_ball": "глина",
    "sand": "песок", "coal_block": "блок угля", "chain": "цепь", "rail": "рельсы", "leather": "кожа",
    "tinted_glass": "тонированное стекло", "slime_ball": "слизь", "lever": "рычаг", "blue_dye": "синий краситель",
    "#fcw:build/chip_controller": "микросхема контроллера (или опытная)", "#fcw:build/chip_memory": "микросхема памяти (или опытная)",
    "#fcw:build/cpu_pga": "процессор PGA (или опытный)",
    "#minecraft:coals": "уголь", "#minecraft:planks": "доски", "#minecraft:wool": "шерсть",
    "#c:ingots/steel": "сталь", "#c:silicon": "технический кремний",
}


def name(s):
    if s in VANILLA:
        return VANILLA[s]
    p = s.split(":")[-1]
    return VANILLA.get(p) or RU[p]


# =============================================================== machine recipes
MACHINE_RU = {
    "arc_furnace": "Дуговая печь", "crystal_puller": "Ростовая печь", "wafering": "Резка и полировка",
    "lithography": "Литограф EUV", "chip_packaging": "Линия электроники: корпусировка",
    "pcb_assembly": "Линия электроники: сборка плат",
}
# (type, name, inputs [(ingredient, count)], catalyst, results [(id, count, chance)], ticks, FE/t)
M = [
    ("arc_furnace", "metallurgical_silicon", [("minecraft:quartz", 4), (COALS, 2)], None,
     [("fcw:metallurgical_silicon", 2, 1)], 400, 256),
    ("arc_furnace", "steel_ingot", [("minecraft:iron_ingot", 4), (COALS, 2)], None,
     [("fcw:steel_ingot", 4, 1)], 300, 256),
    ("arc_furnace", "polysilicon", [(SILICON, 2), ("minecraft:redstone", 1)], None,
     [("fcw:polysilicon", 1, 1)], 600, 384),
    ("crystal_puller", "silicon_boule", [("fcw:polysilicon", 8)], None,
     [("fcw:silicon_boule", 1, 1)], 2400, 512),
    ("wafering", "silicon_wafer", [("fcw:silicon_boule", 1)], None,
     [("fcw:silicon_wafer", 8, 1)], 800, 256),
    ("lithography", "wafer_controller", [("fcw:silicon_wafer", 1)], "fcw:reticle_controller",
     [("fcw:wafer_controller", 1, 1)], 600, 1024),
    ("lithography", "wafer_memory", [("fcw:silicon_wafer", 1)], "fcw:reticle_memory",
     [("fcw:wafer_memory", 1, 1)], 800, 1024),
    ("lithography", "wafer_gpu", [("fcw:silicon_wafer", 1)], "fcw:reticle_gpu",
     [("fcw:wafer_gpu", 1, 1)], 1000, 2048),
    ("lithography", "wafer_cpu", [("fcw:silicon_wafer", 1)], "fcw:reticle_cpu",
     [("fcw:wafer_cpu", 1, 1)], 1200, 2048),
    ("chip_packaging", "chip_controller", [("fcw:wafer_controller", 1), ("minecraft:copper_ingot", 2)], None,
     [("fcw:chip_controller", 8, 1)], 400, 256),
    ("chip_packaging", "chip_memory", [("fcw:wafer_memory", 1), ("minecraft:copper_ingot", 2)], None,
     [("fcw:chip_memory", 6, 1)], 400, 256),
    ("chip_packaging", "chip_gpu", [("fcw:wafer_gpu", 1), ("minecraft:gold_ingot", 2)], None,
     [("fcw:chip_gpu", 2, 1), ("fcw:chip_gpu", 1, 0.5)], 600, 384),
    ("chip_packaging", "cpu_pga", [("fcw:wafer_cpu", 1), ("minecraft:gold_ingot", 2)], None,
     [("fcw:cpu_pga", 4, 1)], 600, 384),
    ("chip_packaging", "cpu_lga", [("fcw:wafer_cpu", 1), ("minecraft:gold_ingot", 2), (STEEL, 2)], None,
     [("fcw:cpu_lga", 2, 1)], 800, 384),
    ("chip_packaging", "cpu_multicore",
     [("fcw:wafer_cpu", 2), ("minecraft:gold_ingot", 4), (STEEL, 2), ("minecraft:diamond", 1)], None,
     [("fcw:cpu_multicore", 1, 1), ("fcw:cpu_multicore", 1, 0.25)], 1200, 512),
    ("chip_packaging", "cpu_server",
     [("fcw:wafer_cpu", 4), ("minecraft:gold_ingot", 8), (STEEL, 4), ("minecraft:netherite_scrap", 2)], None,
     [("fcw:cpu_server", 1, 1)], 2400, 768),
    ("pcb_assembly", "motherboard", [("fcw:pcb_blank", 1), ("fcw:chip_controller", 4)], None,
     [("fcw:motherboard", 1, 1)], 400, 256),
    ("pcb_assembly", "graphics_card",
     [("fcw:pcb_blank", 1), ("fcw:chip_gpu", 1), ("fcw:chip_memory", 2), ("fcw:chip_controller", 1)], None,
     [("fcw:graphics_card", 1, 1)], 600, 384),
    ("pcb_assembly", "ram_dimm", [("fcw:pcb_blank", 1), ("fcw:chip_memory", 2)], None,
     [("fcw:ram_dimm", 1, 1)], 200, 256),
    ("pcb_assembly", "ram_ddr", [("fcw:pcb_blank", 1), ("fcw:chip_memory", 4), ("fcw:chip_controller", 1)], None,
     [("fcw:ram_ddr", 1, 1)], 300, 256),
    ("pcb_assembly", "ram_sodimm",
     [("fcw:pcb_blank", 1), ("fcw:chip_memory", 4), ("fcw:chip_controller", 1), ("minecraft:gold_nugget", 2)], None,
     [("fcw:ram_sodimm", 1, 1)], 300, 256),
    ("pcb_assembly", "ram_ddr_rgb",
     [("fcw:pcb_blank", 1), ("fcw:chip_memory", 8), ("fcw:chip_controller", 1), ("minecraft:glowstone_dust", 2)], None,
     [("fcw:ram_ddr_rgb", 1, 1)], 400, 384),
    ("pcb_assembly", "ram_server",
     [("fcw:pcb_blank", 1), ("fcw:chip_memory", 16), ("fcw:chip_controller", 2), (STEEL, 2)], None,
     [("fcw:ram_server", 1, 1)], 600, 512),
]

M += BD.MACHINE_RECIPES

shutil.rmtree(f"{DATA}/fcw/recipe", ignore_errors=True)
for (typ, nm, inputs, cat, results, ticks, fe) in M:
    r = {"type": f"fcw:{typ}",
         "inputs": [{"ingredient": ing(i), "count": c} for i, c in inputs]}
    if cat:
        r["catalyst"] = ing(cat)
    r["results"] = [{"id": i, "count": c, **({"chance": ch} if ch < 1 else {})} for i, c, ch in results]
    r["ticks"] = ticks
    r["energy_per_tick"] = fe
    write(f"{DATA}/fcw/recipe/{typ}/{nm}.json", r)

# =============================================================== crafting
# (section, result, count, pattern or None, key or shapeless list)
I, G, R, B = "minecraft:iron_ingot", "minecraft:glass", "minecraft:redstone", "minecraft:stone_button"
RB, P, H = "minecraft:redstone_block", "minecraft:piston", "minecraft:hopper"
C = [
    # --- early parts, no machines
    ("parts", "pcb_blank", 4, ["CCC", "PGP"], {"C": "minecraft:copper_ingot", "P": "minecraft:paper", "G": "minecraft:green_dye"}),
    ("parts", "cpu_dip", 1, ["NNN", "RQR", "NNN"], {"N": "minecraft:iron_nugget", "R": R, "Q": "minecraft:quartz"}),
    ("parts", "cpu_qfp", 1, ["GRG", "RCR", "GRG"], {"G": "minecraft:gold_nugget", "R": R, "C": "fcw:cpu_dip"}),
    ("parts", "ram_dip", 1, ["NRN", "NQN"], {"N": "minecraft:iron_nugget", "R": R, "Q": "minecraft:quartz"}),
    ("parts", "ram_simm", 1, None, ["fcw:ram_dip", "fcw:ram_dip", "fcw:pcb_blank", "minecraft:gold_nugget"]),
    ("parts", "motherboard", 1, ["RGR", "CBC", "RQR"],
     {"R": R, "G": "minecraft:gold_ingot", "C": "minecraft:copper_ingot", "B": "fcw:pcb_blank", "Q": "minecraft:quartz"}),
    # --- computers
    ("computers", "system_unit", 1, ["III", "BRI", "III"], {"I": I, "B": B, "R": R}),
    ("computers", "case_desktop", 1, ["III", "BRI"], {"I": I, "B": B, "R": R}),
    ("computers", "case_tower_retro", 1, ["IBI", "IRI", "III"], {"I": I, "B": B, "R": R}),
    ("computers", "case_tower", 1, ["SBS", "SRS", "SSS"], {"S": STEEL, "B": B, "R": R}),
    ("computers", "case_gaming", 1, ["SBS", "GRG", "SLS"],
     {"S": STEEL, "B": B, "R": R, "G": "minecraft:glass_pane", "L": "minecraft:glowstone_dust"}),
    ("computers", "case_cube", 1, ["SB", "RS"], {"S": STEEL, "B": B, "R": R}),
    ("computers", "case_server", 1, ["SSS", "VRV", "SSS"], {"S": STEEL, "V": "minecraft:iron_bars", "R": RB}),
    # --- peripherals
    ("peripherals", "crt_monitor", 1, ["III", "GLI", "IRI"], {"I": I, "G": G, "L": "minecraft:glowstone_dust", "R": R}),
    ("peripherals", "monitor", 1, ["PPP", "LRL", " S "],
     {"P": "minecraft:glass_pane", "L": "minecraft:glowstone_dust", "R": R, "S": STEEL}),
    ("peripherals", "keyboard", 1, ["BBB", "IRI"], {"B": B, "I": I, "R": R}),
    ("peripherals", "mouse", 1, ["BB", "IR"], {"B": B, "I": I, "R": R}),
    ("peripherals", "speaker_desktop", 1, ["PWP", "PNP"],
     {"P": "#minecraft:planks", "W": "#minecraft:wool", "N": "minecraft:note_block"}),
    ("peripherals", "speaker_bookshelf", 1, ["PWP", "PNP", "PRP"],
     {"P": "#minecraft:planks", "W": "#minecraft:wool", "N": "minecraft:note_block", "R": R}),
    ("peripherals", "speaker_tower", 1, ["PNP", "PWP", "PNP"],
     {"P": "#minecraft:planks", "W": "#minecraft:wool", "N": "minecraft:note_block"}),
    ("peripherals", "subwoofer", 1, ["PPP", "WNW", "PRP"],
     {"P": "#minecraft:planks", "W": "#minecraft:wool", "N": "minecraft:note_block", "R": R}),
    ("peripherals", "soundbar", 1, ["WWW", "NRN"], {"W": "#minecraft:wool", "N": "minecraft:note_block", "R": R}),
    # --- data network
    ("network", "data_cable", 8, ["WWW", "CRC", "WWW"], {"W": "#minecraft:wool", "C": "minecraft:copper_ingot", "R": R}),
    ("network", "router", 1, [" A ", "KDK", "IBI"],
     {"A": "minecraft:lightning_rod", "K": "fcw:data_cable", "D": "fcw:cpu_dip", "I": I, "B": "fcw:pcb_blank"}),
    # --- photomasks
    ("reticles", "reticle_controller", 1, ["SGS", "GXG", "SQS"],
     {"S": STEEL, "G": "minecraft:glass_pane", "Q": "minecraft:quartz", "X": "fcw:cpu_dip"}),
    ("reticles", "reticle_memory", 1, ["SGS", "GXG", "SQS"],
     {"S": STEEL, "G": "minecraft:glass_pane", "Q": "minecraft:quartz", "X": "fcw:ram_simm"}),
    ("reticles", "reticle_gpu", 1, ["SGS", "GXG", "SQS"],
     {"S": STEEL, "G": "minecraft:glass_pane", "Q": "minecraft:quartz", "X": "minecraft:diamond"}),
    ("reticles", "reticle_cpu", 1, ["SGS", "GXG", "SQS"],
     {"S": STEEL, "G": "minecraft:glass_pane", "Q": "minecraft:quartz", "X": "fcw:cpu_qfp"}),
    # --- machine blueprints and the wrench (docs/design/MACHINE_BUILD.md)
] + [("build", f"{m}_blueprint", 1, None, ["minecraft:paper"] * 4 + ["minecraft:blue_dye", BD.BLUEPRINT_CPU[m]]) for m in BD.MACHINES] + [
    ("build", "wrench", 1, ["P P", " I "], {"P": "fcw:steel_plate", "I": I}),
    ("build", "wrench_iron", 1, ["P P", " I "], {"P": "fcw:iron_plate", "I": I}),
] + [("components", res, count, pattern, key) for (res, count, pattern, key) in BD.CRAFT] + [
    ("euv", res, count, None, key) for (res, count, key) in BD.EUV
]
# items registered by the machines agent: recipes are skipped until those items exist
DEFERRED = ("euv_", "_blueprint")
CATEGORY = {"parts": "misc", "computers": "misc", "peripherals": "misc", "network": "misc", "reticles": "misc", "build": "misc", "components": "misc", "euv": "building"}
for (sec, res, count, pattern, key) in C:
    r = {}
    if res.startswith(DEFERRED[0]) or res.endswith(DEFERRED[1]):
        r["neoforge:conditions"] = [{"type": "neoforge:item_exists", "item": f"fcw:{res}"}]
    if pattern:
        used = set("".join(pattern)) - {" "}
        assert used == set(key), (res, used, set(key))
        assert len({len(p) for p in pattern}) == 1, res
        r.update({"type": "minecraft:crafting_shaped", "category": CATEGORY[sec], "pattern": pattern,
                  "key": {k: ing(v) for k, v in key.items()}})
    else:
        r.update({"type": "minecraft:crafting_shapeless", "category": CATEGORY[sec],
                  "ingredients": [ing(v) for v in key]})
    r["result"] = {"id": "fcw:wrench" if res == "wrench_iron" else f"fcw:{res}", "count": count}
    write(f"{DATA}/fcw/recipe/crafting/{res}.json", r)

# =============================================================== furnaces, stonecutter, bench chips
for (kind, inp, res, xp, ticks) in BD.COOK:
    write(f"{DATA}/fcw/recipe/{kind}/{res}.json", {"type": f"minecraft:{kind}", "category": "misc", "ingredient": ing(inp),
                                                   "result": {"id": f"fcw:{res}"}, "experience": xp, "cookingtime": ticks})
for (inp, res, count) in BD.STONECUT:
    write(f"{DATA}/fcw/recipe/stonecutting/{res}.json", {"type": "minecraft:stonecutting", "ingredient": ing(inp),
                                                         "result": {"id": f"fcw:{res}", "count": count}})
for (nm, chip, extra, res) in BD.LEGACY_CHIPS:
    r = {"type": "fcw:legacy_chip", "chip": ing(chip), "result": {"id": f"fcw:{res}", "count": 1}}
    if extra:
        r["extra"] = ing(extra)
    write(f"{DATA}/fcw/recipe/crafting/{nm}.json", r)

# =============================================================== construction data read by the game
shutil.rmtree(f"{DATA}/fcw/construction", ignore_errors=True)
for m, stages in BD.STAGES.items():
    write(f"{DATA}/fcw/construction/{m}.json", {"stages": [
        {"id": sid, "ru": ru, "en": en, "seconds": sec,
         "items": [{"item": it if ":" in it else f"fcw:{it}", "count": n} for it, n in items]}
        for (sid, ru, en, sec, items) in stages]})
write(f"{DATA}/fcw/construction/components.json", {"components": [{"id": k, "stack": v[2]} for k, v in BD.COMPONENTS.items()]})

# =============================================================== recipe book unlock
# One advancement: the first redstone, iron or quartz in the inventory shows every recipe of the mod
# in the recipe book (recipes whose items do not exist yet are skipped by the game).
write(f"{DATA}/fcw/advancement/recipes/production.json", {
    "parent": "minecraft:recipes/root",
    "criteria": {f"has_{i}": {"trigger": "minecraft:inventory_changed",
                             "conditions": {"items": [{"items": f"minecraft:{i}"}]}}
                 for i in ("redstone", "iron_ingot", "quartz")},
    "requirements": [["has_redstone", "has_iron_ingot", "has_quartz"]],
    "rewards": {"recipes": [f"fcw:crafting/{res}" for (_, res, _, _, _) in C]},
})


# =============================================================== tags
def tag(ns, path, values):
    write(f"{DATA}/{ns}/tags/item/{path}.json", {"replace": False, "values": values})


shutil.rmtree(f"{DATA}/fcw/tags", ignore_errors=True)
shutil.rmtree(f"{DATA}/c", ignore_errors=True)
tag("c", "ingots/steel", ["fcw:steel_ingot"])
tag("c", "ingots", ["#c:ingots/steel"])
tag("c", "silicon", ["fcw:metallurgical_silicon"])
MICRON = ["cpu_dip", "cpu_qfp", "ram_dip", "ram_simm"]
EUV = ["cpu_pga", "cpu_lga", "cpu_multicore", "cpu_server", "chip_controller", "chip_memory", "chip_gpu",
       "ram_dimm", "ram_ddr", "ram_ddr_rgb", "ram_server", "ram_sodimm"]
CPU = ["cpu_dip", "cpu_qfp", "cpu_pga", "cpu_lga", "cpu_multicore", "cpu_server"]
MEM = ["chip_memory", "ram_dip", "ram_simm", "ram_dimm", "ram_ddr", "ram_ddr_rgb", "ram_server", "ram_sodimm"]
TAGS = {
    "chips/micron": MICRON, "chips/duv": [], "chips/immersion": [], "chips/euv": EUV,
    "chips/cpu": CPU, "chips/memory": MEM, "chips/gpu": ["chip_gpu"], "chips/controller": ["chip_controller"],
    "reticles": [f"reticle_{k}" for k in ("controller", "memory", "gpu", "cpu")],
    "patterned_wafers": [f"wafer_{k}" for k in ("controller", "memory", "gpu", "cpu")],
}
for path, vals in TAGS.items():
    tag("fcw", path, [f"fcw:{v}" for v in vals])
for _path, _vals in BD.BUILD_TAGS.items():
    tag("fcw", _path, [f"fcw:{v}" for v in _vals])
tag("fcw", "chips/any", ["#fcw:chips/micron", "#fcw:chips/duv", "#fcw:chips/immersion", "#fcw:chips/euv"])
assert set(MICRON + EUV) == set(CPU + MEM + ["chip_gpu", "chip_controller"])

# =============================================================== item models + lang
FLAT = [k for k in ITEMS if k not in ("silicon_boule", "pcb_blank")] + [f"{m}_blueprint" for m in BD.MACHINES] + ["wrench"]
for k in FLAT:
    assert os.path.exists(f"{ASSETS}/textures/item/{k}.png"), k
    with open(f"{ASSETS}/models/item/{k}.json", "w") as f:
        f.write('{\n\t"parent": "minecraft:item/generated",\n\t"textures": {\n\t\t"layer0": "fcw:item/%s"\n\t}\n}\n' % k)

for lang, ni, fi, ti, uses in (("ru_ru", 0, 2, 3, "Осталось экспозиций: %s из %s"), ("en_us", 1, 4, 5, "Exposures left: %s of %s")):
    path = f"{ASSETS}/lang/{lang}.json"
    d = json.load(open(path, encoding="utf-8"))
    for k, v in ITEMS.items():
        d[f"item.fcw.{k}"] = v[ni]
        d[f"tooltip.fcw.{k}.from"] = v[fi]
        d[f"tooltip.fcw.{k}.to"] = v[ti]
    d["tooltip.fcw.uses_left"] = uses
    for m in BD.MACHINES:
        d[f"item.fcw.{m}_blueprint"] = ("Проект: " if ni == 0 else "Blueprint: ") + BD.MACHINE_NAMES[m][ni]
        if m != "euv_scanner_mk2":
            d[f"item.fcw.{m}_kit"] = ("Комплект (старый): " if ni == 0 else "Kit (legacy): ") + BD.MACHINE_NAMES[m][ni]
    d["item.fcw.wrench"] = "Гаечный ключ" if ni == 0 else "Wrench"
    with open(path, "w", encoding="utf-8") as f:
        json.dump(dict(sorted(d.items())), f, ensure_ascii=False, indent=2)
        f.write("\n")

# =============================================================== docs
def fmt(i, c):
    return (f"{c} × " if c != 1 else "") + name(i)


def secs(t):
    s = t / 20
    return f"{s:g} с" if s < 60 else f"{s / 60:g} мин"


L = []
A = L.append
A("# Производство комплектующих: рецепты")
A("")
A("Страница для игроков: как из кварца, угля и железа получить процессоры, память и платы.")
A("Устройство машин и формат рецептов для авторов датапаков: [PRODUCTION.md](PRODUCTION.md).")
A("Таблицы ниже и сами рецепты в `mod/src/main/resources/data/fcw/recipe/` собраны одним скриптом,")
A("`mod/tools/production/gen.py`: баланс правится в нём, затем `python3 mod/tools/production/gen.py .`")
A("из корня репозитория.")
A("")
A("## Дерево цепочки")
A("")
A("```")
A("кварц + уголь ─► [дуговая печь] ─► технический кремний ─┬─► (+ редстоун) [дуговая печь] ─► поликремний")
A("железо + уголь ─► [дуговая печь] ─► сталь               └─► детали машин")
A("")
A("8 поликремния ─► [ростовая печь] ─► монокристалл ─► [резка и полировка] ─► 8 пластин")
A("")
A("пластина + фотошаблон ─► [литограф EUV] ─► пластина с рисунком:")
A("  ├─ контроллеры ─► [корпусировка] ─► 8 микросхем контроллера")
A("  ├─ память      ─► [корпусировка] ─► 6 микросхем памяти")
A("  ├─ графика     ─► [корпусировка] ─► 2-3 графических процессора")
A("  └─ процессоры  ─► [корпусировка] ─► процессоры 32-бит, 64-бит, многоядерный, серверный")
A("")
A("заготовка платы + микросхемы ─► [сборка плат] ─► материнская плата, видеокарта,")
A("                                                 память DIMM, DDR, SO-DIMM, DDR RGB, серверная")
A("```")
A("")
A("Порядок развития:")
A("")
A("1. **Верстак.** Системный блок или корпус, ЭЛТ-монитор, клавиатура, материнская плата, процессор")
A("   8-бит (DIP) и микросхема памяти (DIP): простой текстовый компьютер собирается без машин.")
A("2. **Дуговая печь** (комплект из ванильных материалов): сталь и технический кремний.")
A("3. **Ростовая печь** и **участок резки и полировки**: поликремний, монокристалл, пластины.")
A("4. **Литограф EUV** (из блоков `euv_*`, нужны сталь и пластины) и фотошаблоны.")
A("5. **Линия электроники**: корпусировка микросхем и сборка плат. Старшие процессоры, видеокарта,")
A("   быстрая память.")
A("")
A("Одна пластина стоит 4 кварца: на монокристалл (8 пластин) уходит 32 кварца, 16 угля и 8 редстоуна.")
A("")
A("## Рецепты машин")
A("")
A("Сырьё кладут в машину руками (ПКМ предметом по доку, лотку или проёму; ПКМ пустой рукой забирает")
A("результат), порядок не важен. Если подходят несколько рецептов, выбирается тот, где больше разных")
A("входов (потом тот, где больше предметов): положите в корпусировку ровно то, что нужно для желаемого")
A("процессора, или закрепите рецепт на пульте командой `recipe <имя>` (`recipe auto` возвращает")
A("автовыбор). Фотошаблон вставляется в порт масок литографа и не расходуется, но теряет 1 прочность")
A("за пластину (64 экспозиции). Шанс в скобках: дополнительный выход.")
A("")
cur = None
for (typ, nm, inputs, cat, results, ticks, fe) in M:
    if typ != cur:
        cur = typ
        A(f"### {MACHINE_RU[typ]} (`fcw:{typ}`)")
        A("")
        A("| Вход | Шаблон | Выход | Время | FE/т | Всего FE |")
        A("|---|---|---|---|---|---|")
    outs = " + ".join(fmt(i, c) + (f" ({int(ch * 100)}%)" if ch < 1 else "") for i, c, ch in results)
    A(f"| {' + '.join(fmt(i, c) for i, c in inputs)} | {name(cat) if cat else ''} | {outs} | {secs(ticks)} | {fe} | {ticks * fe:,} |".replace(",", " "))
    if M.index((typ, nm, inputs, cat, results, ticks, fe)) + 1 == len(M) or M[M.index((typ, nm, inputs, cat, results, ticks, fe)) + 1][0] != typ:
        A("")
A("Поликремний из дуговой печи: временная замена будущему реактору очистки. В этот рецепт идёт любой")
A("кремний из тега `c:silicon`, в том числе из других модов.")
A("")
A("## Крафт на верстаке")
A("")
SEC_RU = {"parts": "Детали без машин", "computers": "Компьютеры", "peripherals": "Мониторы, ввод и звук",
          "network": "Сеть данных",
          "reticles": "Фотошаблоны", "build": "Проекты машин и ключ", "components": "Компоненты машин (верстак)",
          "euv": "Блоки литографа EUV"}
SEC_NOTE = {
    "parts": "Материнская плата на верстаке обходится дороже, чем на линии электроники, зато доступна сразу.",
    "computers": "Все корпуса работают одинаково, отличается только вид. Простые из железа, современные из стали.",
    "network": "Кабель данных соединяет компьютер с машинами; роутер добавляет в сеть машины в радиусе 32 блоков "
               "без кабеля. Управление: `fcw machine` в компьютере.",
    "reticles": "Образец в центре определяет, что будет на пластине. Прочность: 64 экспозиции.",
    "build": "Проект ставит стройплощадку машины (ПКМ по земле); цена машины в её ведомости, см. «Постройка». "
             "Ключ разбирает машину: Shift + ПКМ по пульту выключенной машины, второй раз для подтверждения.",
    "components": "Из компонентов собирают машины на стройплощадке. Часть рецептов не на верстаке: см. таблицу ниже.",
    "euv": "В скобках после названия: сколько таких блоков идёт на один литограф (всего 301).",
}
EUV_COUNT = BD.EUV_COUNT
cur = None
for (sec, res, count, pattern, key) in C:
    if sec != cur:
        if cur:
            A("")
        cur = sec
        A(f"### {SEC_RU[sec]}")
        A("")
        if sec in SEC_NOTE:
            A(SEC_NOTE[sec])
            A("")
        A("| Предмет | Выход | Сетка | Обозначения |")
        A("|---|---|---|---|")
    title = RU["wrench" if res == "wrench_iron" else res] + (f" ({EUV_COUNT[res]})" if res in EUV_COUNT else "")
    if pattern:
        grid = "<br>".join("`" + p.replace(" ", "·") + "`" for p in pattern)
        legend = ", ".join(f"`{k}` {name(v)}" for k, v in key.items())
    else:
        grid = "без формы"
        cnt = {}
        for v in key:
            cnt[v] = cnt.get(v, 0) + 1
        legend = " + ".join(fmt(v, c) for v, c in cnt.items())
    A(f"| {title} (`{res}`) | {count} | {grid} | {legend} |")
A("")
A("### Компоненты: печи, камнерез, машины")
A("")
A("| Предмет | Из чего | Где |")
A("|---|---|---|")
for (kind, inp, res, xp, ticks) in BD.COOK:
    A(f"| {RU[res]} (`{res}`) | {name(inp)} | {WHERE_RU[kind].lower()} |")
for (inp, res, count) in BD.STONECUT:
    A(f"| {fmt('fcw:' + res, count)} (`{res}`) | {name(inp)} | камнерез |")
for (typ, nm, inputs, cat, results, ticks, fe) in BD.MACHINE_RECIPES:
    A(f"| {RU[nm]} (`{nm}`) | {' + '.join(fmt(i, c) for i, c in inputs)} | {MACHINE_RU[typ].lower()}, {secs(ticks)}, {fe} FE/т |")
A("")
A("Чипы для первого литографа. Контроллеры, память и процессоры PGA нельзя сделать без литографа, поэтому")
A("первый литограф строится на чипах с верстака: на уровне `engineer` три за один, на `standard` один за один,")
A("на `fab` замены нет.")
A("")
A("Получаются «опытные» чипы: они годятся только в узлы литографа (теги `fcw:build/…`, куда входят и настоящие")
A("чипы) и в компьютер не ставятся.")
A("")
A("| Получается | Из чего (на `engineer`) |")
A("|---|---|")
for (nm, chip, extra, res) in BD.LEGACY_CHIPS:
    A(f"| {RU[res]} (`{res}`) | 3 × {name(chip)}" + (f" + {name(extra)}" if extra else "") + " |")
A("")
A("## Постройка машин")
A("")
A("Машину ставит проект: ПКМ по земле размечает стройплощадку с лесами и поддоном на месте будущего пульта.")
A("Компоненты завозят на поддон или на леса (ПКМ предметом, воронки, трубы), можно все сразу. Этап начинается")
A("сам, когда закончен предыдущий и его компоненты на месте; в конце 30 секунд пусконаладки. Литограф можно")
A("также выложить руками из блоков по схеме: тогда ПКМ по контроллеру запускает только пусконаладку (10 минут).")
A("Числа ниже для уровня `engineer` (`production.difficulty`); на `standard` четверть количества и времени.")
A("")
for m, stages in BD.STAGES.items():
    total = sum(n for st in stages for _, n in st[4])
    A(f"### {BD.MACHINE_NAMES[m][0][0].upper() + BD.MACHINE_NAMES[m][0][1:]}: {total} шт., {sum(st[3] for st in stages) // 60} мин")
    A("")
    A("| Этап | Время | Компоненты |")
    A("|---|---|---|")
    for i, (sid, ru, en, sec, items) in enumerate(stages):
        A(f"| {i + 1}. {ru} | {sec} с | " + ", ".join(f"{name(it if ':' in it else 'fcw:' + it)} ×{n}" for it, n in items) + " |")
    A("")
A("Разбор: гаечным ключом, этапы идут в обратном порядке за половину времени, возвращается 75 % компонентов")
A("(на `standard` всё, на `fab` 60 %); шкафы управления и проект возвращаются всегда.")
A("")
A("## Теги")
A("")
A("### Тиры чипов")
A("")
A("Датапаки совместимости (AE2, Mekanism, Create, TACZ) и сборки должны требовать «чип тира N», а не")
A("конкретный предмет: тогда новые чипы и литографы младших поколений подхватятся сами. Тир равен")
A("поколению литографии, через которую прошёл чип.")
A("")
A("| Тег | Что внутри | Для чего |")
A("|---|---|---|")
A(f"| `fcw:chips/micron` | {', '.join('`' + v + '`' for v in MICRON)} | младший тир: всё, что крафтится на верстаке без литографа |")
A("| `fcw:chips/duv` | пока пусто | литограф DUV (будущие версии); тег уже можно использовать в рецептах |")
A("| `fcw:chips/immersion` | пока пусто | иммерсионный литограф (будущие версии) |")
A(f"| `fcw:chips/euv` | {', '.join('`' + v + '`' for v in EUV)} | всё, что прошло через литограф EUV: процессоры и микросхемы с линии, модули памяти из них |")
A("| `fcw:chips/any` | все четыре тира | «любой чип» |")
A("")
A("По назначению (тир не учитывается; каждый предмет из тиров входит ровно в один из этих тегов):")
A("")
A("| Тег | Что внутри |")
A("|---|---|")
A(f"| `fcw:chips/cpu` | {', '.join('`' + v + '`' for v in CPU)} |")
A(f"| `fcw:chips/memory` | {', '.join('`' + v + '`' for v in MEM)} |")
A("| `fcw:chips/gpu` | `chip_gpu` |")
A("| `fcw:chips/controller` | `chip_controller` |")
A("")
A("Чтобы потребовать, например, процессор не ниже EUV, в рецепте нужен ингредиент-пересечение тегов")
A("(`neoforge:intersection` из `fcw:chips/euv` и `fcw:chips/cpu`). Готовые платы (`motherboard`,")
A("`graphics_card`) чипами не считаются и в эти теги не входят.")
A("")
A("### Остальные")
A("")
A("| Тег | Что внутри | Для чего |")
A("|---|---|---|")
A("| `c:ingots/steel` (и `c:ingots`) | `steel_ingot` | сталь других модов подходит во все рецепты мода, и наоборот |")
A("| `c:silicon` | `metallurgical_silicon` | кремний других модов можно рафинировать в поликремний и пускать на детали машин |")
A("| `fcw:reticles` | четыре фотошаблона | фильтр слота шаблона |")
A("| `fcw:patterned_wafers` | четыре пластины с рисунком | фильтр входа корпусировки |")
A("")
A("## Проверка рецептов")
A("")
A("При запуске сервера мод пишет в лог, сколько рецептов машин загружено и прошла ли самопроверка поиска")
A("(`[recipes] machine recipes loaded: ...`, `[recipes] recipe self-test: N/N passed`). То же самое по")
A("команде `/fcw recipes` (нужны права оператора): удобно после правки рецептов датапаком и `/reload`.")
with open(f"{ROOT}/docs/PRODUCTION_RECIPES.md", "w", encoding="utf-8") as f:
    f.write("\n".join(L) + "\n")
print("machine recipes", len(M), "crafting", len(C), "items", len(ITEMS))
