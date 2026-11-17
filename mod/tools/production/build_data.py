"""Machine construction data (docs/design/MACHINE_BUILD.md): components, their recipes, the bills of
materials by stage, blueprints and the wrench. Imported by gen.py, which writes recipes, lang, docs and
the data files the game reads (data/fcw/construction/*.json).

Recipes of the difficulty level "fab" (the column of the design's tables) are not here yet: when the
workshops of docs/design/HARDCORE.md exist, they join as a second recipe set next to RECIPES, chosen by
a built-in datapack."""

STEEL = "#c:ingots/steel"
I, CU = "minecraft:iron_ingot", "minecraft:copper_ingot"
BARS, TG = "minecraft:iron_bars", "minecraft:tinted_glass"


def f(s):
    return "fcw:" + s


# id -> (ru, en, stack size, group)
COMPONENTS = {
    # structure and casing
    "iron_plate": ("Железный лист", "Iron Plate", 64, "structure"),
    "steel_plate": ("Стальной лист", "Steel Plate", 64, "structure"),
    "iron_frame": ("Железная рама", "Iron Frame", 64, "structure"),
    "steel_frame": ("Стальная рама", "Steel Frame", 64, "structure"),
    "enclosure_panel": ("Панель обшивки", "Enclosure Panel", 64, "structure"),
    "granite_plate_blank": ("Заготовка гранитной плиты", "Granite Plate Blank", 64, "structure"),
    "granite_plate": ("Гранитная плита", "Granite Plate", 64, "structure"),
    # heat
    "fireclay": ("Шамотная масса", "Fireclay", 64, "heat"),
    "firebrick": ("Шамотный кирпич", "Firebrick", 64, "heat"),
    "refractory_lining": ("Огнеупорная футеровка", "Refractory Lining", 64, "heat"),
    "graphite": ("Графит", "Graphite", 64, "heat"),
    "graphite_electrode": ("Графитовый электрод", "Graphite Electrode", 16, "heat"),
    "graphite_heater": ("Графитовый нагреватель", "Graphite Heater", 16, "heat"),
    "crucible_blank": ("Заготовка тигля", "Crucible Blank", 16, "heat"),
    "quartz_crucible": ("Кварцевый тигель", "Quartz Crucible", 16, "heat"),
    "water_cooled_panel": ("Водоохлаждаемая панель", "Water-Cooled Panel", 64, "heat"),
    "oven_module": ("Печной модуль", "Oven Module", 16, "heat"),
    # electrics and mechanics
    "copper_bar": ("Медная шина", "Copper Busbar", 64, "electro"),
    "copper_pipe": ("Медная трубка", "Copper Pipe", 64, "electro"),
    "copper_coil": ("Обмотка", "Copper Coil", 64, "electro"),
    "furnace_transformer": ("Печной трансформатор", "Furnace Transformer", 16, "electro"),
    "electric_motor": ("Электродвигатель", "Electric Motor", 64, "electro"),
    "pump": ("Насос", "Pump", 16, "electro"),
    "hydraulic_cylinder": ("Гидроцилиндр", "Hydraulic Cylinder", 16, "electro"),
    "pulling_head": ("Механизм вытягивания", "Pulling Head", 16, "electro"),
    "linear_stage": ("Линейный привод", "Linear Stage", 16, "electro"),
    "conveyor_section": ("Секция конвейера", "Conveyor Section", 64, "electro"),
    "vacuum_chamber_section": ("Секция вакуумной камеры", "Vacuum Chamber Section", 64, "electro"),
    # control
    "control_cabinet_1": ("Шкаф управления I", "Control Cabinet I", 16, "control"),
    "control_cabinet_2": ("Шкаф управления II", "Control Cabinet II", 16, "control"),
    "control_cabinet_3": ("Шкаф управления III", "Control Cabinet III", 16, "control"),
    # wafering and line units
    "steel_wire": ("Стальная проволока", "Steel Wire", 64, "units"),
    "diamond_wire": ("Алмазная проволока", "Diamond Wire", 16, "units"),
    "diamond_blade": ("Алмазный диск", "Diamond Blade", 16, "units"),
    "polishing_head": ("Полировальная голова", "Polishing Head", 16, "units"),
    "placement_head": ("Голова установщика", "Placement Head", 16, "units"),
    "bonding_head": ("Голова разварки", "Bonding Head", 16, "units"),
    # scanner units
    "polished_mirror_blank": ("Заготовка зеркала", "Polished Mirror Blank", 16, "litho"),
    "multilayer_mirror": ("Многослойное зеркало", "Multilayer Mirror", 16, "litho"),
    "laser_module": ("Модуль лазера", "Laser Module", 16, "litho"),
    "precision_stage": ("Прецизионный столик", "Precision Stage", 16, "litho"),
    "turbo_pump": ("Турбомолекулярный насос", "Turbomolecular Pump", 16, "litho"),
    "chiller_unit": ("Чиллер", "Chiller Unit", 16, "litho"),
}
GROUP_RU = {"structure": "Конструкция и корпус", "heat": "Тепло", "electro": "Электрика и механика",
            "control": "Управление", "units": "Узлы резки и линии", "litho": "Узлы литографа"}

MACHINES = ["arc_furnace", "crystal_puller", "wafering_cell", "chip_line", "euv_scanner_mk2"]
MACHINE_NAMES = {
    "arc_furnace": ("дуговая печь", "arc furnace"), "crystal_puller": ("ростовая печь", "crystal puller"),
    "wafering_cell": ("резка и полировка пластин", "wafering cell"), "chip_line": ("линия электроники", "electronics line"),
    "euv_scanner_mk2": ("литограф EUV Mk2", "EUV scanner Mk2"),
}

# crafting-table recipes: (result, count, pattern or None, key or shapeless list)
# A crafting grid holds nine items. Four recipes of the design's tables need more (furnace transformer 19,
# control cabinet I 17, control cabinet III 15, laser module 10); they are fitted into the grid here.
CRAFT = [
    ("iron_plate", 2, ["I", "I", "I"], {"I": I}),
    ("steel_plate", 2, ["S", "S", "S"], {"S": STEEL}),
    ("iron_frame", 2, ["PBP", "B B", "PBP"], {"P": f("iron_plate"), "B": BARS}),
    ("steel_frame", 2, ["PBP", "B B", "PBP"], {"P": f("steel_plate"), "B": BARS}),
    ("enclosure_panel", 2, None, [f("iron_plate"), f("iron_plate"), "minecraft:white_concrete_powder"]),
    ("granite_plate_blank", 1, ["GG", "GG"], {"G": "minecraft:polished_granite"}),
    ("fireclay", 2, None, ["minecraft:clay_ball", "minecraft:clay_ball", "minecraft:quartz", "minecraft:sand"]),
    ("refractory_lining", 1, ["FF", "FF"], {"F": f("firebrick")}),
    ("graphite_electrode", 1, ["GBG", "G G", "G G"], {"G": f("graphite"), "B": f("copper_bar")}),
    ("graphite_heater", 1, ["GBG", "GBG", "G G"], {"G": f("graphite"), "B": f("copper_bar")}),
    ("crucible_blank", 1, ["Q Q", "Q Q", "QQQ"], {"Q": "minecraft:quartz_block"}),
    ("water_cooled_panel", 2, ["PTP", "T T", "PTP"], {"P": f("iron_plate"), "T": f("copper_pipe")}),
    ("oven_module", 1, None, [f("firebrick")] * 4 + [f("graphite_heater")] + [f("steel_plate")] * 2),
    ("copper_bar", 2, ["CCC"], {"C": CU}),
    ("copper_pipe", 4, ["C C", " C "], {"C": CU}),
    ("copper_coil", 2, ["CCC", "CIC", "CCC"], {"C": CU, "I": I}),
    ("furnace_transformer", 1, ["PCP", "CFC", "BCB"],
     {"P": f("iron_plate"), "C": f("copper_coil"), "F": f("iron_frame"), "B": f("copper_bar")}),
    ("electric_motor", 1, None, [f("copper_coil")] * 2 + [I, f("iron_plate"), "minecraft:redstone", "minecraft:redstone"]),
    ("pump", 1, None, [f("electric_motor")] + [f("copper_pipe")] * 2 + [f("iron_plate")] * 2),
    ("hydraulic_cylinder", 1, None, ["minecraft:piston", f("iron_plate"), f("iron_plate"), f("copper_pipe")]),
    ("pulling_head", 1, None, [f("electric_motor")] * 2 + [f("steel_frame")] + ["minecraft:chain"] * 3),
    ("linear_stage", 1, None, [f("electric_motor")] + [f("steel_plate")] * 2 + ["minecraft:rail"] * 2 + [f("granite_plate")]),
    ("conveyor_section", 2, None, [f("steel_plate")] * 3 + [f("electric_motor")] + ["minecraft:leather"] * 2),
    ("vacuum_chamber_section", 1, None, [f("steel_plate")] * 6 + [TG, "minecraft:slime_ball"]),
    ("control_cabinet_1", 1, ["PLP", "CXC", "PMB"],
     {"P": f("iron_plate"), "L": "minecraft:lever", "C": "minecraft:comparator", "X": f("cpu_dip"), "M": f("ram_dip"),
      "B": "minecraft:redstone_block"}),
    ("control_cabinet_2", 1, None, [f("steel_plate")] * 4 + [f("cpu_qfp"), f("ram_simm"), f("ram_simm"), f("motherboard"), f("crt_monitor")]),
    ("control_cabinet_3", 1, ["SKS", "MBM", "CKC"],
     {"S": f("steel_plate"), "K": f("cpu_pga"), "M": f("chip_memory"), "B": f("motherboard"), "C": f("chip_controller")}),
    ("diamond_wire", 1, None, [f("steel_wire")] * 8 + ["minecraft:diamond"]),
    ("diamond_blade", 1, [" D ", "DSD", " D "], {"D": "minecraft:diamond", "S": f("steel_plate")}),
    ("polishing_head", 1, None, [f("electric_motor"), f("steel_plate"), "#minecraft:wool", "#minecraft:wool"]),
    ("placement_head", 1, None, [f("linear_stage"), f("cpu_qfp"), "minecraft:hopper"]),
    ("bonding_head", 1, None, [f("linear_stage"), "minecraft:gold_ingot", "minecraft:gold_ingot", "minecraft:diamond"]),
    ("multilayer_mirror", 1, None, [f("polished_mirror_blank")] + [f("silicon_wafer")] * 4 + ["minecraft:amethyst_shard"] * 4),
    ("laser_module", 1, None, [f("steel_frame")] + [f("copper_coil")] * 3 + [TG, TG] + ["minecraft:amethyst_shard"] * 2 + [f("chip_controller")]),
    ("precision_stage", 1, None, [f("linear_stage"), f("linear_stage"), f("granite_plate"), f("chip_controller")]),
    ("turbo_pump", 1, None, [f("pump"), f("electric_motor"), f("steel_plate"), f("steel_plate"), f("chip_controller")]),
    ("chiller_unit", 1, None, [f("pump")] + [f("copper_pipe")] * 4 + [f("steel_plate")] * 2 + ["minecraft:packed_ice"] * 2),
]
# (recipe type, input, result, experience, ticks)
COOK = [
    ("smelting", f("fireclay"), "firebrick", 0.1, 200),
    ("blasting", "minecraft:coal_block", "graphite", 0.3, 200),
    ("blasting", f("crucible_blank"), "quartz_crucible", 0.5, 400),
]
# (input, result, count)
STONECUT = [
    (STEEL, "steel_wire", 4),
    (f("granite_plate_blank"), "granite_plate", 1),
]
# new machine recipe: (type, name, inputs, catalyst, results, ticks, FE/t)
MACHINE_RECIPES = [
    ("wafering", "polished_mirror_blank", [("minecraft:quartz_block", 4), ("minecraft:diamond", 1)], None,
     [("fcw:polished_mirror_blank", 1, 1)], 800, 256),
]

# scanner building blocks, now made of components: (block, count, shapeless ingredients)
CASING = f("euv_casing")
EUV = [
    ("euv_casing", 2, [f("steel_frame"), f("enclosure_panel"), f("enclosure_panel")]),
    ("euv_casing_glass", 1, [CASING, TG]),
    ("euv_casing_vent", 1, [CASING, BARS, BARS]),
    ("euv_frame", 1, [f("steel_frame")]),
    ("euv_vacuum_chamber", 1, [f("vacuum_chamber_section")]),
    ("euv_mirror", 1, [f("multilayer_mirror"), f("steel_frame")]),
    ("euv_collector", 1, [f("multilayer_mirror")] * 4 + [CASING, "minecraft:diamond"]),
    ("euv_droplet_generator", 1, [f("pump"), "minecraft:hopper", f("copper_pipe"), f("copper_pipe"), CASING, f("chip_controller")]),
    ("euv_laser_amplifier", 1, [f("laser_module")]),
    ("euv_chiller", 1, [f("chiller_unit")]),
    ("euv_wafer_stage", 4, [f("precision_stage"), f("granite_plate"), f("granite_plate"), f("steel_plate"), f("steel_plate")]),
    ("euv_reticle_stage", 3, [f("precision_stage"), f("steel_plate"), f("steel_plate"), TG]),
    ("euv_wafer_handler", 2, [f("linear_stage"), f("electric_motor"), f("steel_plate"), f("steel_plate"), f("chip_controller")]),
    ("euv_load_port", 1, [CASING, "minecraft:hopper", f("electric_motor"), TG]),
    ("euv_vacuum_pump", 1, [f("turbo_pump"), CASING]),
    ("euv_power_unit", 1, [f("furnace_transformer")] + [f("copper_bar")] * 4 + [CASING]),
    ("euv_controller", 1, [f("control_cabinet_3"), f("crt_monitor"), CASING]),
    ("euv_port_energy", 1, [CASING, f("copper_bar"), f("copper_bar")]),
    ("euv_port_coolant", 1, [CASING, f("copper_pipe"), f("copper_pipe")]),
    ("euv_port_gas", 1, [CASING, f("copper_pipe"), "minecraft:glass_bottle"]),
    ("euv_port_exhaust", 1, [CASING, BARS, "minecraft:iron_trapdoor"]),
    ("euv_port_data", 1, [CASING, "minecraft:comparator"]),
    ("euv_port_item", 1, [CASING, "minecraft:hopper"]),
]
EUV_COUNT = {"euv_casing": 97, "euv_casing_glass": 29, "euv_casing_vent": 12, "euv_chiller": 9, "euv_collector": 1,
             "euv_controller": 1, "euv_droplet_generator": 1, "euv_frame": 24, "euv_laser_amplifier": 22,
             "euv_load_port": 2, "euv_mirror": 22, "euv_port_coolant": 2, "euv_port_data": 1, "euv_port_energy": 1,
             "euv_port_exhaust": 1, "euv_port_gas": 2, "euv_port_item": 2, "euv_power_unit": 1, "euv_reticle_stage": 9,
             "euv_vacuum_chamber": 23, "euv_vacuum_pump": 2, "euv_wafer_handler": 13, "euv_wafer_stage": 24}

# The first scanner: chips it needs cannot be made without a scanner, so bench chips stand in for them,
# three for one on "engineer", one for one on "standard", not at all on "fab" (recipe type fcw:legacy_chip).
# (name, bench chip, extra ingredient or None, result)
LEGACY_CHIPS = [
    ("chip_controller_from_bench", f("cpu_qfp"), None, "chip_controller"),
    ("chip_memory_from_bench", f("ram_simm"), None, "chip_memory"),
    ("cpu_pga_from_bench", f("cpu_qfp"), "minecraft:gold_ingot", "cpu_pga"),
]

# machine -> stages: (id, ru, en, seconds, [(item, count)])
STAGES = {
    "arc_furnace": [
        ("foundation", "Фундамент и башня", "Foundation and tower", 180, [("iron_frame", 24), ("iron_plate", 24)]),
        ("hearth", "Горн и футеровка", "Hearth and lining", 240, [("refractory_lining", 36), ("water_cooled_panel", 8), ("iron_plate", 16)]),
        ("electrodes", "Электроды и механика", "Electrodes and mechanics", 180,
         [("graphite_electrode", 3), ("hydraulic_cylinder", 4), ("electric_motor", 4), ("pump", 2)]),
        ("transformer", "Трансформатор и шины", "Transformer and busbars", 120, [("furnace_transformer", 1), ("copper_bar", 12)]),
        ("casing", "Обшивка и пульт", "Casing and terminal", 120, [("enclosure_panel", 32), (TG, 8), ("control_cabinet_1", 1)]),
    ],
    "crystal_puller": [
        ("frame", "Рама и основание", "Frame and base", 120, [("steel_frame", 12), ("steel_plate", 16)]),
        ("chamber", "Камера", "Chamber", 180, [("vacuum_chamber_section", 8), ("water_cooled_panel", 6), ("refractory_lining", 8)]),
        ("heat", "Тепловой узел", "Hot zone", 120, [("quartz_crucible", 1), ("graphite_heater", 4), ("furnace_transformer", 1), ("copper_bar", 6)]),
        ("pulling", "Вытягивание и вакуум", "Pulling and vacuum", 120, [("pulling_head", 1), ("pump", 2), ("electric_motor", 2)]),
        ("casing", "Обшивка и пульт", "Casing and terminal", 90, [("enclosure_panel", 20), (TG, 6), ("control_cabinet_1", 1)]),
    ],
    "wafering_cell": [
        ("frame", "Рама и плита", "Frame and plate", 120, [("steel_frame", 8), ("steel_plate", 12), ("granite_plate", 6)]),
        ("saw", "Пила", "Saw", 120, [("diamond_wire", 8), ("diamond_blade", 2), ("linear_stage", 3), ("electric_motor", 4)]),
        ("polish", "Полировка и вода", "Polishing and water", 90, [("polishing_head", 3), ("pump", 2), ("copper_pipe", 8)]),
        ("casing", "Обшивка и пульт", "Casing and terminal", 90, [("enclosure_panel", 16), (TG, 8), ("control_cabinet_2", 1)]),
    ],
    "chip_line": [
        ("frame", "Рама", "Frame", 150, [("steel_frame", 14), ("steel_plate", 20)]),
        ("conveyor", "Конвейер", "Conveyor", 150, [("conveyor_section", 14), ("electric_motor", 6), ("minecraft:hopper", 4)]),
        ("packaging", "Корпусировка", "Packaging", 180, [("bonding_head", 2), ("linear_stage", 2), ("pump", 1)]),
        ("assembly", "Сборка плат", "Board assembly", 180, [("placement_head", 2), ("oven_module", 2), ("linear_stage", 2)]),
        ("casing", "Обшивка, пульты, наладка", "Casing, terminals, set-up", 150,
         [("enclosure_panel", 30), (TG, 12), ("control_cabinet_2", 2), ("silicon_wafer", 8)]),
    ],
    "euv_scanner_mk2": [
        ("frame", "Рама и корпус", "Frame and casing", 600, [(k, EUV_COUNT[k]) for k in ("euv_frame", "euv_casing", "euv_casing_vent")]),
        ("vacuum", "Вакуумная камера", "Vacuum chamber", 600, [(k, EUV_COUNT[k]) for k in ("euv_vacuum_chamber", "euv_vacuum_pump")]),
        ("source", "Драйв-лазер и источник", "Drive laser and source", 900,
         [(k, EUV_COUNT[k]) for k in ("euv_laser_amplifier", "euv_droplet_generator", "euv_collector", "euv_chiller", "euv_power_unit")]),
        ("optics", "Оптика", "Optics", 900, [("euv_mirror", EUV_COUNT["euv_mirror"])]),
        ("stages", "Столики и загрузчик", "Stages and handler", 600,
         [(k, EUV_COUNT[k]) for k in ("euv_wafer_stage", "euv_reticle_stage", "euv_wafer_handler", "euv_load_port")]),
        ("glazing", "Остекление, порты, пусконаладка", "Glazing, ports, commissioning", 600,
         [(k, EUV_COUNT[k]) for k in ("euv_casing_glass", "euv_port_coolant", "euv_port_data", "euv_port_energy", "euv_port_exhaust",
                                      "euv_port_gas", "euv_port_item", "euv_controller")]),
    ],
}

# blueprint: paper x4 + blue dye + a bench CPU
BLUEPRINT_CPU = {m: f("cpu_qfp") if m == "euv_scanner_mk2" else f("cpu_dip") for m in MACHINES}
