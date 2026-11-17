"""Bill of materials for building the machines (docs/design/MACHINE_BUILD.md).
Recipes and per-machine lists live here; the script expands them to raw materials and prints markdown tables.
Run: python3 bom.py"""
import math, collections
from fractions import Fraction as F

# component -> (output count, {input: count}, where it is made)
R = {
 'iron_plate': (2, {'iron': 3}, 'верстак'),
 'steel_plate': (2, {'steel': 3}, 'верстак'),
 'iron_bars': (16, {'iron': 6}, 'верстак (ваниль)'),
 'iron_frame': (2, {'iron_plate': 4, 'iron_bars': 4}, 'верстак'),
 'steel_frame': (2, {'steel_plate': 4, 'iron_bars': 4}, 'верстак'),
 'enclosure_panel': (2, {'iron_plate': 2, 'white_concrete_powder': 1}, 'верстак'),
 'firebrick': (2, {'clay': 2, 'quartz': 1, 'sand': 1}, 'верстак (шамотная масса), затем печь'),
 'refractory_lining': (1, {'firebrick': 4}, 'верстак'),
 'graphite': (1, {'coal': 9}, 'плавильная печь (блок угля)'),
 'graphite_electrode': (1, {'graphite': 6, 'copper_bar': 1}, 'верстак'),
 'copper_bar': (2, {'copper': 3}, 'верстак'),
 'copper_pipe': (4, {'copper': 3}, 'верстак'),
 'copper_coil': (2, {'copper': 8, 'iron': 1}, 'верстак'),
 'water_cooled_panel': (2, {'iron_plate': 4, 'copper_pipe': 4}, 'верстак'),
 'furnace_transformer': (1, {'iron_plate': 2, 'copper_coil': 4, 'copper_bar': 2, 'iron_frame': 1}, 'верстак'),  # ужат в сетку 3×3
 'electric_motor': (1, {'copper_coil': 2, 'iron': 1, 'iron_plate': 1, 'redstone': 2}, 'верстак'),
 'pump': (1, {'electric_motor': 1, 'copper_pipe': 2, 'iron_plate': 2}, 'верстак'),
 'hydraulic_cylinder': (1, {'piston': 1, 'iron_plate': 2, 'copper_pipe': 1}, 'верстак'),
 'control_cabinet_1': (1, {'iron_plate': 3, 'cpu_dip': 1, 'ram_dip': 1, 'comparator': 2, 'redstone': 9, 'lever': 1}, 'верстак'),  # блок редстоуна
 'control_cabinet_2': (1, {'steel_plate': 4, 'cpu_qfp': 1, 'ram_simm': 2, 'motherboard': 1, 'crt_monitor': 1}, 'верстак'),
 'control_cabinet_3': (1, {'steel_plate': 2, 'cpu_pga': 2, 'chip_memory': 2, 'chip_controller': 2, 'motherboard': 1}, 'верстак'),  # ужат в сетку 3×3
 'quartz_crucible': (1, {'quartz': 28}, 'верстак (заготовка из 7 блоков кварца), затем плавильная печь'),
 'graphite_heater': (1, {'graphite': 6, 'copper_bar': 2}, 'верстак'),
 'vacuum_chamber_section': (1, {'steel_plate': 6, 'tinted_glass': 1, 'slime_ball': 1}, 'верстак'),
 'pulling_head': (1, {'electric_motor': 2, 'steel_frame': 1, 'chain': 3}, 'верстак'),
 'steel_wire': (4, {'steel': 1}, 'камнерез'),
 'diamond_wire': (1, {'steel_wire': 8, 'diamond': 1}, 'верстак'),
 'diamond_blade': (1, {'diamond': 4, 'steel_plate': 1}, 'верстак'),
 'granite_plate': (1, {'polished_granite': 4}, 'верстак, затем камнерез (доводка)'),
 'polishing_head': (1, {'electric_motor': 1, 'steel_plate': 1, 'wool': 2}, 'верстак'),
 'linear_stage': (1, {'electric_motor': 1, 'steel_plate': 2, 'rail': 2, 'granite_plate': 1}, 'верстак'),
 'conveyor_section': (2, {'steel_plate': 3, 'electric_motor': 1, 'leather': 2}, 'верстак'),
 'oven_module': (1, {'firebrick': 4, 'graphite_heater': 1, 'steel_plate': 2}, 'верстак'),
 'placement_head': (1, {'linear_stage': 1, 'cpu_qfp': 1, 'hopper': 1}, 'верстак'),
 'bonding_head': (1, {'linear_stage': 1, 'gold': 2, 'diamond': 1}, 'верстак'),
 'polished_mirror_blank': (1, {'quartz': 16, 'diamond': 1}, 'резка и полировка (новый рецепт)'),
 'multilayer_mirror': (1, {'polished_mirror_blank': 1, 'silicon_wafer': 4, 'amethyst_shard': 4}, 'верстак'),
 'laser_module': (1, {'steel_frame': 1, 'copper_coil': 3, 'tinted_glass': 2, 'amethyst_shard': 2, 'chip_controller': 1}, 'верстак'),
 'precision_stage': (1, {'linear_stage': 2, 'granite_plate': 1, 'chip_controller': 1}, 'верстак'),
 'turbo_pump': (1, {'pump': 1, 'electric_motor': 1, 'steel_plate': 2, 'chip_controller': 1}, 'верстак'),
 'chiller_unit': (1, {'pump': 1, 'copper_pipe': 4, 'steel_plate': 2, 'packed_ice': 2}, 'верстак'),
 # EUV kit blocks
 'euv_casing': (2, {'steel_frame': 1, 'enclosure_panel': 2}, 'верстак'),
 'euv_casing_glass': (1, {'euv_casing': 1, 'tinted_glass': 1}, 'верстак'),
 'euv_casing_vent': (1, {'euv_casing': 1, 'iron_bars': 2}, 'верстак'),
 'euv_frame': (1, {'steel_frame': 1}, 'верстак'),
 'euv_vacuum_chamber': (1, {'vacuum_chamber_section': 1}, 'верстак'),
 'euv_mirror': (1, {'multilayer_mirror': 1, 'steel_frame': 1}, 'верстак'),
 'euv_collector': (1, {'multilayer_mirror': 4, 'euv_casing': 1, 'diamond': 1}, 'верстак'),
 'euv_droplet_generator': (1, {'pump': 1, 'hopper': 1, 'copper_pipe': 2, 'euv_casing': 1, 'chip_controller': 1}, 'верстак'),
 'euv_laser_amplifier': (1, {'laser_module': 1}, 'верстак'),
 'euv_chiller': (1, {'chiller_unit': 1}, 'верстак'),
 'euv_wafer_stage': (4, {'precision_stage': 1, 'granite_plate': 2, 'steel_plate': 2}, 'верстак'),
 'euv_reticle_stage': (3, {'precision_stage': 1, 'steel_plate': 2, 'tinted_glass': 1}, 'верстак'),
 'euv_wafer_handler': (2, {'linear_stage': 1, 'electric_motor': 1, 'steel_plate': 2, 'chip_controller': 1}, 'верстак'),
 'euv_load_port': (1, {'euv_casing': 1, 'hopper': 1, 'electric_motor': 1, 'tinted_glass': 1}, 'верстак'),
 'euv_vacuum_pump': (1, {'turbo_pump': 1, 'euv_casing': 1}, 'верстак'),
 'euv_power_unit': (1, {'furnace_transformer': 1, 'copper_bar': 4, 'euv_casing': 1}, 'верстак'),
 'euv_controller': (1, {'control_cabinet_3': 1, 'crt_monitor': 1, 'euv_casing': 1}, 'верстак'),
 'euv_port_energy': (1, {'euv_casing': 1, 'copper_bar': 2}, 'верстак'),
 'euv_port_coolant': (1, {'euv_casing': 1, 'copper_pipe': 2}, 'верстак'),
 'euv_port_gas': (1, {'euv_casing': 1, 'copper_pipe': 1, 'glass_bottle': 1}, 'верстак'),
 'euv_port_exhaust': (1, {'euv_casing': 1, 'iron_bars': 1, 'iron_trapdoor': 1}, 'верстак'),
 'euv_port_data': (1, {'euv_casing': 1, 'comparator': 1}, 'верстак'),
 'euv_port_item': (1, {'euv_casing': 1, 'hopper': 1}, 'верстак'),
}

# machine -> stages: (name, seconds, {component: count})
M = {
 'arc_furnace': [
  ('Фундамент и башня', 180, {'iron_frame': 24, 'iron_plate': 24}),
  ('Горн и футеровка', 240, {'refractory_lining': 36, 'water_cooled_panel': 8, 'iron_plate': 16}),
  ('Электроды и механика', 180, {'graphite_electrode': 3, 'hydraulic_cylinder': 4, 'electric_motor': 4, 'pump': 2}),
  ('Трансформатор и шины', 120, {'furnace_transformer': 1, 'copper_bar': 12}),
  ('Обшивка и пульт', 120, {'enclosure_panel': 32, 'tinted_glass': 8, 'control_cabinet_1': 1}),
 ],
 'crystal_puller': [
  ('Рама и основание', 120, {'steel_frame': 12, 'steel_plate': 16}),
  ('Камера', 180, {'vacuum_chamber_section': 8, 'water_cooled_panel': 6, 'refractory_lining': 8}),
  ('Тепловой узел', 120, {'quartz_crucible': 1, 'graphite_heater': 4, 'furnace_transformer': 1, 'copper_bar': 6}),
  ('Вытягивание и вакуум', 120, {'pulling_head': 1, 'pump': 2, 'electric_motor': 2}),
  ('Обшивка и пульт', 90, {'enclosure_panel': 20, 'tinted_glass': 6, 'control_cabinet_1': 1}),
 ],
 'wafering_cell': [
  ('Рама и плита', 120, {'steel_frame': 8, 'steel_plate': 12, 'granite_plate': 6}),
  ('Пила', 120, {'diamond_wire': 8, 'diamond_blade': 2, 'linear_stage': 3, 'electric_motor': 4}),
  ('Полировка и вода', 90, {'polishing_head': 3, 'pump': 2, 'copper_pipe': 8}),
  ('Обшивка и пульт', 90, {'enclosure_panel': 16, 'tinted_glass': 8, 'control_cabinet_2': 1}),
 ],
 'chip_line': [
  ('Рама', 150, {'steel_frame': 14, 'steel_plate': 20}),
  ('Конвейер', 150, {'conveyor_section': 14, 'electric_motor': 6, 'hopper': 4}),
  ('Корпусировка', 180, {'bonding_head': 2, 'linear_stage': 2, 'pump': 1}),
  ('Сборка плат', 180, {'placement_head': 2, 'oven_module': 2, 'linear_stage': 2}),
  ('Обшивка, пульты, наладка', 150, {'enclosure_panel': 30, 'tinted_glass': 12, 'control_cabinet_2': 2, 'silicon_wafer': 8}),
 ],
}
EUV_COUNT = {"euv_casing": 97, "euv_casing_glass": 29, "euv_casing_vent": 12, "euv_chiller": 9, "euv_collector": 1, "euv_controller": 1,
             "euv_droplet_generator": 1, "euv_frame": 24, "euv_laser_amplifier": 22, "euv_load_port": 2, "euv_mirror": 22, "euv_port_coolant": 2,
             "euv_port_data": 1, "euv_port_energy": 1, "euv_port_exhaust": 1, "euv_port_gas": 2, "euv_port_item": 2, "euv_power_unit": 1,
             "euv_reticle_stage": 9, "euv_vacuum_chamber": 23, "euv_vacuum_pump": 2, "euv_wafer_handler": 13, "euv_wafer_stage": 24}
M['euv_scanner_mk2'] = [
  ('Рама и корпус', 600, {k: EUV_COUNT[k] for k in ('euv_frame', 'euv_casing', 'euv_casing_vent')}),
  ('Вакуумная камера', 600, {k: EUV_COUNT[k] for k in ('euv_vacuum_chamber', 'euv_vacuum_pump')}),
  ('Драйв-лазер и источник', 900, {k: EUV_COUNT[k] for k in ('euv_laser_amplifier', 'euv_droplet_generator', 'euv_collector', 'euv_chiller', 'euv_power_unit')}),
  ('Оптика', 900, {k: EUV_COUNT[k] for k in ('euv_mirror',)}),
  ('Столики и загрузчик', 600, {k: EUV_COUNT[k] for k in ('euv_wafer_stage', 'euv_reticle_stage', 'euv_wafer_handler', 'euv_load_port')}),
  ('Остекление, порты, пусконаладка', 600, {k: EUV_COUNT[k] for k in ('euv_casing_glass', 'euv_port_coolant', 'euv_port_data', 'euv_port_energy', 'euv_port_exhaust', 'euv_port_gas', 'euv_port_item', 'euv_controller')}),
]

RU = {'iron': 'железо', 'copper': 'медь', 'gold': 'золото', 'coal': 'уголь', 'redstone': 'редстоун', 'quartz': 'кварц', 'clay': 'глина (комки)', 'sand': 'песок',
      'diamond': 'алмазы', 'amethyst_shard': 'осколки аметиста', 'tinted_glass': 'тонированное стекло', 'piston': 'поршни', 'white_concrete_powder': 'белый цемент',
      'slime_ball': 'слизь', 'chain': 'цепи', 'wool': 'шерсть', 'rail': 'рельсы', 'hopper': 'воронки', 'comparator': 'компараторы', 'lever': 'рычаги',
      'polished_granite': 'полированный гранит', 'packed_ice': 'плотный лёд', 'leather': 'кожа', 'glass_bottle': 'пузырьки', 'iron_trapdoor': 'железные люки', 'steel': 'сталь (дуговая печь)', 'silicon_wafer': 'пластины кремния',
      'cpu_dip': 'cpu_dip', 'ram_dip': 'ram_dip', 'cpu_qfp': 'cpu_qfp', 'ram_simm': 'ram_simm', 'cpu_pga': 'cpu_pga', 'chip_controller': 'chip_controller',
      'chip_memory': 'chip_memory', 'motherboard': 'motherboard', 'crt_monitor': 'crt_monitor'}
ORDER = list(RU)

def crafts(need):
    """expand a {component: count} list down to raw materials; whole crafts only (batches are rounded up per component)"""
    raw = collections.Counter(); todo = collections.Counter(need)
    # topological: repeat until no craftable left; aggregate demand per component before rounding
    depth = {}
    def d(k):
        if k not in R: return 0
        if k not in depth: depth[k] = 1 + max(d(i) for i in R[k][1])
        return depth[k]
    for k in list(R): d(k)
    for k in sorted(R, key=lambda k: -depth[k]):
        n = todo.pop(k, 0)
        if not n: continue
        out, ins, _ = R[k]; batches = math.ceil(n / out)
        for i, c in ins.items(): todo[i] += c * batches
    return todo

if __name__ == '__main__':
    for name, stages in M.items():
        tot = collections.Counter()
        for (_, _, comp) in stages: tot.update(comp)
        raw = crafts(tot)
        secs = sum(s for _, s, _ in stages)
        print('\n###', name, ' компонентов:', sum(tot.values()), ' время сборки:', secs // 60, 'мин')
        for (st, s, comp) in stages:
            print('| %s | %d с | %s |' % (st, s, ', '.join('`%s` ×%d' % kv for kv in comp.items())))
        print('сырьё: ' + ', '.join('%s %d' % (RU[k], raw[k]) for k in ORDER if raw.get(k)))
        assert not set(raw) - set(RU), set(raw) - set(RU)
