"""Machine terminal mockups. Run: FCW_MC_JAR=<minecraft client jar> python3 screens.py  (output: ../*.png)"""
import os, sys
from fcwui import *

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
PILL = {'off': ('ВЫКЛ', 'off'), 'idle': ('ОЖИДАНИЕ', 'idle'), 'run': ('РАБОТА', 'run'), 'error': ('ОШИБКА', 'err'),
        'no_power': ('НЕТ ПИТАНИЯ', 'warn'), 'no_mask': ('НЕТ МАСКИ', 'warn')}

def header(g, x, y, cols, left, right, col='fg'):
    """inverse status line across the top of a screen"""
    g.rect(x - 1, y, cols * CELL + 2, LINE, col)
    g.mtext(x, y, left, 'glass'); g.mtext(x + (cols - len(right)) * CELL, y, right, 'glass')

def side_columns(g, y, energy, state, fe='312k', rate='2048'):
    """L frame: energy on the left of the player inventory, breaker button and LEDs on the right"""
    g.text(9, y + 1, 'FE', 'ink_dim')
    g.vbar(18, y + 13, 10, 42, energy)
    g.text(9, y + 60, fe, 'ink'); g.text(9, y + 70, rate + '/t', 'ink_dim')
    x = 216
    for i, (lab, col, on) in enumerate((('ПИТ', 'ok', state not in ('off', 'no_power')), ('РАБ', 'run', state == 'run'),
                                        ('ОШБ', 'err', state in ('error', 'no_mask', 'no_power')))):
        g.led(x, y + 3 + i * 11, col, on); g.text(x + 7, y + 1 + i * 11, lab, 'ink_dim')
    if state == 'off': g.button(214, y + 58, 36, 18, 'ПУСК', 'start')
    else: g.button(214, y + 58, 36, 18, 'СТОП', 'stop')

def litho(state, blink=True):
    g = Gui(256, 236, state=state); g.chassis('Литограф EUV Mk2')
    g.pill(248, 3, *PILL[state])
    X, Y, C = 11, 22, 39
    g.glass(8, 20, 240, 94)
    tray_y = 116
    g.tray(8, tray_y, 240, 34)
    g.mtext(12, tray_y + 2, 'ВХОД', 'dim'); g.mtext(99, tray_y + 2, 'МАСКА', 'dim'); g.mtext(172, tray_y + 2, 'ВЫХОД', 'dim')
    sy = tray_y + 13
    g.slots(12, sy, 4, 1, 'in'); g.slot(105, sy, 'mask'); g.slots(172, sy, 4, 1, 'out')
    g.inventory(45, 152)
    has_mask = state != 'no_mask'
    if state == 'off':
        side_columns(g, 152, 0.78, state, '312k', '0')
        g.arrow(128, sy + 4, 40, 0, 'off')
        item(g, 106, sy + 1, 'mask'); item(g, 13, sy + 1, 'wafer'); count(g, 13, sy + 1, 8); item(g, 31, sy + 1, 'tin'); count(g, 31, sy + 1, 16)
        g.mtext(X, Y + 4 * LINE, '         РУБИЛЬНИК ВЫКЛЮЧЕН', 'faint')
        return g
    hdr = {'idle': 'ГОТОВ', 'run': 'ЭКСПОЗИЦИЯ', 'error': 'ОСТАНОВ', 'no_power': 'НЕТ ПИТАНИЯ', 'no_mask': 'НЕТ МАСКИ'}[state]
    hc = {'idle': 'fg', 'run': 'run', 'error': 'err', 'no_power': 'warn', 'no_mask': 'warn'}[state]
    header(g, X, Y, C, ' EUV-13  ' + hdr, 'Д12 14:20 ', hc)
    wx, wy = X + 3, Y + LINE + 4
    tx = X + 11 * CELL
    def row(i, k, v, vc='fg'):
        g.mtext(tx, Y + (1 + i) * LINE + 1, k, 'dim'); g.mtext(tx + 7 * CELL, Y + (1 + i) * LINE + 1, v, vc)
    if state == 'run':
        wafer_map(g, wx, wy, 23, 23, blink=blink)
        row(0, 'МАСКА', 'CPU   рес.187/256'); row(1, 'ПОЛЕ', '24/61  доза 30mJ')
        row(2, 'СТОЛ W', 'X+012.4 Y-031.0', 'data'); row(3, 'СТОЛ R', 'X-003.1 Z+0.02', 'data')
        row(4, 'EUV', '13.5nm 250W Sn96%')
        g.tbar(tx, Y + 6 * LINE + 1, 22, 0.39); g.mtext(tx + 23 * CELL, Y + 6 * LINE + 1, '39%', 'run')
        g.mtext(X, Y + 7 * LINE + 1, '14:19:52 пластина 3/8 на столе, совм.OK', 'dim')
        g.mtext(X, Y + 8 * LINE + 1, '14:20:07 экспонирование поля 24' + ('█' if blink else ''), 'fg')
        prog, e, fe, rate = 0.39, 0.78, '312k', '2048'
    elif state == 'idle':
        wafer_map(g, wx, wy, 0)
        row(0, 'МАСКА', 'CPU   рес.187/256'); row(1, 'ПОЛЕ', '--/61')
        row(2, 'СТОЛ W', 'парковка', 'dim'); row(3, 'СТОЛ R', 'парковка', 'dim')
        row(4, 'EUV', 'дежурный режим', 'dim')
        g.tbar(tx, Y + 6 * LINE + 1, 22, 0); g.mtext(tx + 23 * CELL, Y + 6 * LINE + 1, ' 0%', 'dim')
        g.mtext(X, Y + 7 * LINE + 1, '14:18:40 партия завершена: 8 пластин', 'dim')
        g.mtext(X, Y + 8 * LINE + 1, '14:18:41 жду пластины в доке' + ('█' if blink else ''), 'fg')
        prog, e, fe, rate = 0, 1.0, '400k', '0'
    elif state == 'error':
        wafer_map(g, wx, wy, 23, 23, bad=(9, 17), blink=False)
        row(0, 'МАСКА', 'CPU   рес.187/256'); row(1, 'ПОЛЕ', '24/61  пауза', 'warn')
        row(2, 'ОШИБКА', 'E21 ВЫХОД ЗАПОЛНЕН', 'err'); row(3, '', 'освободите слоты', 'fg'); row(4, '', 'выхода или порт дока', 'fg')
        g.tbar(tx, Y + 6 * LINE + 1, 22, 0.39, 'err'); g.mtext(tx + 23 * CELL, Y + 6 * LINE + 1, '39%', 'err')
        g.mtext(X, Y + 7 * LINE + 1, '14:20:07 экспонирование поля 24', 'dim')
        g.mtext(X, Y + 8 * LINE + 1, '14:20:31 E21 некуда выгрузить' + ('█' if blink else ''), 'err')
        prog, e, fe, rate = 0.39, 0.9, '360k', '0'
    elif state == 'no_power':
        wafer_map(g, wx, wy, 23, None, dim=True)
        row(0, 'МАСКА', 'CPU   рес.187/256', 'dim'); row(1, 'ПОЛЕ', '24/61  пауза', 'warn')
        row(2, 'ПИТАНИЕ', '  0 / 2048 FE/t', 'warn'); row(3, 'БУФЕР', '  1.2k / 400k FE', 'warn'); row(4, '', 'проверьте ввод power', 'fg')
        g.tbar(tx, Y + 6 * LINE + 1, 22, 0.39, 'warn'); g.mtext(tx + 23 * CELL, Y + 6 * LINE + 1, '39%', 'warn')
        g.mtext(X, Y + 7 * LINE + 1, '14:20:07 экспонирование поля 24', 'dim')
        g.mtext(X, Y + 8 * LINE + 1, '14:20:19 E10 просадка питания, пауза' + ('█' if blink else ''), 'warn')
        prog, e, fe, rate = 0.39, 0.004, '1.2k', '0'
    else:  # no_mask
        wafer_map(g, wx, wy, 0)
        row(0, 'МАСКА', '-- не установлена --', 'warn'); row(1, 'ПОЛЕ', '--/61')
        row(2, '', 'вставьте фотошаблон', 'fg'); row(3, '', 'в слот МАСКА или', 'fg'); row(4, '', 'в порт reticle', 'fg')
        g.tbar(tx, Y + 6 * LINE + 1, 22, 0); g.mtext(tx + 23 * CELL, Y + 6 * LINE + 1, ' 0%', 'dim')
        g.mtext(X, Y + 7 * LINE + 1, '14:21:02 пластины в доке: 8', 'dim')
        g.mtext(X, Y + 8 * LINE + 1, '14:21:02 E30 нет маски, пуск запрещён' + ('█' if blink else ''), 'warn')
        prog, e, fe, rate = 0, 1.0, '400k', '0'
    side_columns(g, 152, e, state, fe, rate)
    g.arrow(128, sy + 4, 40, prog, {'run': 'run', 'error': 'err', 'no_power': 'warn'}.get(state, 'off'))
    if has_mask: item(g, 106, sy + 1, 'mask')
    else:
        for k in range(0, 16, 2): g.px(106 + k, sy + 1 + k, 'run_deep'); g.px(121 - k, sy + 1 + k, 'run_deep')
    item(g, 13, sy + 1, 'wafer'); count(g, 13, sy + 1, 6 if state != 'idle' else 0) if state != 'idle' else None
    if state == 'idle': g.rect(13, sy + 1, 16, 16, 'well')
    item(g, 31, sy + 1, 'tin'); count(g, 31, sy + 1, 16)
    if state in ('run', 'error', 'no_power'): item(g, 173, sy + 1, 'wafer_cpu'); count(g, 173, sy + 1, 2)
    if state == 'idle': item(g, 173, sy + 1, 'wafer_cpu'); count(g, 173, sy + 1, 8)
    if state == 'error':
        for q in range(4):
            item(g, 173 + q * 18, sy + 1, 'wafer_cpu'); count(g, 173 + q * 18, sy + 1, 64)
    return g

def chip_line(blink=True):
    g = Gui(256, 236, state='run'); g.chassis('Линия электроники'); g.pill(248, 3, *PILL['run'])
    g.glass(8, 20, 240, 74)
    X, Y = 11, 22
    header(g, X, Y, 39, ' КОРПУСИРОВКА', 'СБОРКА ПЛАТ  LINE-2 ', 'run')
    g.rect(128, Y + LINE, 1, 62, 'faint')
    # left pane: packaging
    def pane(px, rows, frac, pct, col='run'):
        for i, (k, v, vc) in enumerate(rows):
            g.mtext(px, Y + (1 + i) * LINE + 1, k, 'dim'); g.mtext(px + 8 * CELL, Y + (1 + i) * LINE + 1, v, vc)
        g.tbar(px, Y + 5 * LINE + 1, 14, frac, col); g.mtext(px + 15 * CELL, Y + 5 * LINE + 1, pct, col)
    pane(X, (('РЕЦЕПТ', 'cpu_lga', 'fg'), ('ТАКТ', '212/800 t', 'fg'), ('ПОЗИЦИЯ', 'разварка', 'data'), ('ГОДНЫХ', '61/64 95%', 'ok')), 0.27, '27%')
    pane(X + 20 * CELL, (('РЕЦЕПТ', 'motherboard', 'fg'), ('ТАКТ', '310/400 t', 'fg'), ('ПОЗИЦИЯ', 'печь 245°', 'data'), ('ПЛАТ', '3 готово', 'fg')), 0.78, '78%')
    g.mtext(X, Y + 6 * LINE + 1, '14:20 микросхемы > участок сборки: 4' + ('█' if blink else ''), 'dim')
    ty = 96
    g.tray(8, ty, 240, 54)
    g.mtext(12, ty + 2, 'КОРПУСИРОВКА', 'dim'); g.mtext(132, ty + 2, 'СБОРКА ПЛАТ', 'dim')
    sy = ty + 14
    g.slots(12, sy, 2, 2, 'in'); g.arrow(52, sy + 13, 28, 0.27); g.slots(84, sy, 2, 2, 'out')
    g.rect(127, ty + 4, 1, 46, 'anod_hi')
    g.slots(132, sy, 3, 2, 'in'); g.arrow(190, sy + 13, 28, 0.78); g.slots(222, sy, 1, 2, 'out')
    item(g, 13, sy + 1, 'wafer_cpu'); count(g, 13, sy + 1, 3)
    item(g, 85, sy + 1, 'chip'); count(g, 85, sy + 1, 24)
    item(g, 133, sy + 1, 'pcb'); count(g, 133, sy + 1, 5); item(g, 151, sy + 1, 'chip'); count(g, 151, sy + 1, 12)
    item(g, 223, sy + 1, 'pcb'); count(g, 223, sy + 1, 3)
    g.inventory(45, 152); side_columns(g, 152, 0.62, 'run', '248k', '640')
    return g

def simple_frame(title, state, energy=0.7, fe='280k'):
    g = Gui(176, 212, state=state); g.chassis(title, seams=False); g.pill(168, 3, *PILL[state])
    g.glass(8, 20, 160, 58)
    g.tray(8, 80, 160, 46)
    g.inventory(5, 128)
    return g

def arc_furnace(blink=True):
    g = simple_frame('Дуговая печь', 'run')
    X, Y = 11, 22
    header(g, X, Y, 25, ' EAF-1  ПЛАВКА', '1640°C ', 'run')
    # three electrode current bars
    for i, (lab, f) in enumerate((('L1', .82), ('L2', .74), ('L3', .88))):
        g.mtext(X, Y + (1 + i) * LINE + 2, lab, 'dim'); g.tbar(X + 3 * CELL, Y + (1 + i) * LINE + 2, 10, f, 'copper')
        g.mtext(X + 14 * CELL, Y + (1 + i) * LINE + 2, '%2d кА' % (f * 40), 'fg')
    g.mtext(X + 21 * CELL, Y + 1 * LINE + 2, 'ШИХТА', 'dim'); g.mtext(X + 21 * CELL, Y + 2 * LINE + 2, ' 62%', 'fg')
    g.mtext(X, Y + 4 * LINE + 3, 'кварц+уголь>Si 148/400t' + ('█' if blink else ''), 'fg')
    sy = 85
    g.slots(12, sy, 2, 2, 'in'); g.arrow(52, sy + 13, 30, 0.37); g.slots(86, sy, 2, 2, 'out')
    item(g, 13, sy + 1, 'quartz'); count(g, 13, sy + 1, 28); item(g, 31, sy + 1, 'coal'); count(g, 31, sy + 1, 14)
    item(g, 87, sy + 1, 'silicon'); count(g, 87, sy + 1, 6)
    g.mtext(128, sy - 1, 'FE', 'dim'); g.mtext(142, sy - 1, '280k', 'fg')
    g.hbar(128, sy + 11, 36, 5, 0.7)
    g.button(127, sy + 21, 38, 15, 'СТОП', 'stop')
    return g

def generator(blink=True):
    g = simple_frame('Топливный генератор', 'run')
    X, Y = 11, 22
    header(g, X, Y, 25, ' GEN-1  ГОРЕНИЕ', '80 FE/t ', 'run')
    g.mtext(X, Y + 1 * LINE + 2, 'ТОПЛИВО', 'dim'); g.mtext(X + 8 * CELL, Y + 1 * LINE + 2, 'уголь  x11', 'fg')
    g.mtext(X, Y + 2 * LINE + 2, 'ГОРИТ', 'dim'); g.tbar(X + 8 * CELL, Y + 2 * LINE + 2, 10, 0.6, 'warn'); g.mtext(X + 19 * CELL, Y + 2 * LINE + 2, '48 с', 'fg')
    g.mtext(X, Y + 3 * LINE + 2, 'БУФЕР', 'dim'); g.tbar(X + 8 * CELL, Y + 3 * LINE + 2, 10, 0.9, 'copper'); g.mtext(X + 19 * CELL, Y + 3 * LINE + 2, '36k', 'fg')
    g.mtext(X, Y + 4 * LINE + 3, 'нагрузка 64, запас 14 мин', 'dim')
    sy = 85
    g.mtext(12, sy - 1, 'ТОПЛИВО', 'dim'); g.slot(21, sy + 12, 'fuel'); item(g, 22, sy + 13, 'coal'); count(g, 22, sy + 13, 11)
    # flame
    fx, fy = 62, sy + 12
    for k, (a, b) in enumerate(((6, 2), (5, 4), (4, 6), (3, 8), (2, 10), (2, 10), (1, 12), (1, 12), (1, 12), (2, 10), (3, 8))):
        g.rect(fx + a, fy + 3 + k, b, 1, 'warn' if k < 6 else 'copper')
    g.rect(fx + 5, fy + 9, 4, 4, 'idle')
    g.mtext(88, sy - 1, 'ЗОЛА', 'dim'); g.slot(91, sy + 12, 'out')
    g.mtext(128, sy - 1, 'FE', 'dim'); g.mtext(146, sy - 1, '36k', 'fg')
    g.hbar(128, sy + 11, 36, 5, 0.9)
    g.button(127, sy + 21, 38, 15, 'СТОП', 'stop')
    return g

def ups(skin=None, blink=True):
    g = Gui(176, 212, skin=skin, state='run'); g.chassis('Шкаф ИБП' if skin is None else 'ИБП-250', seams=False)
    g.pill(168, 3, 'ОТ БАТАРЕИ', 'warn')
    g.glass(8, 20, 160, 58); g.tray(8, 80, 160, 46); g.inventory(5, 128)
    X, Y = 11, 22
    header(g, X, Y, 25, ' UPS-A  РАЗРЯД', '74% ', 'warn')
    # big segmented battery
    for i in range(10):
        g.rect(X + 1 + i * 9, Y + LINE + 4, 8, 12, ('ok' if i < 7 else 'faint'))
    g.rect(X + 91, Y + LINE + 7, 2, 6, 'faint')
    home = skin is not None
    g.mtext(X + 17 * CELL, Y + 1 * LINE + 2, '185k FE' if home else '4.44M FE', 'fg'); g.mtext(X + 17 * CELL, Y + 2 * LINE + 2, 'из 250k' if home else 'из 6.0M', 'dim')
    g.mtext(X, Y + 3 * LINE + 3, 'ВХОД', 'dim'); g.mtext(X + 5 * CELL, Y + 3 * LINE + 3, '   0', 'warn'); g.mtext(X + 13 * CELL, Y + 3 * LINE + 3, 'ВЫХОД', 'dim'); g.mtext(X + 19 * CELL, Y + 3 * LINE + 3, ' 60' if home else '412', 'fg')
    g.mtext(X, Y + 4 * LINE + 3, ('нет сети 14:02 ост.02:34' if home else 'нет сети 14:02 ост.08:59') + ('█' if blink else ''), 'warn')
    sy = 85
    g.mtext(12, sy - 1, 'БАТАРЕЯ' if home else 'БАТАРЕЙНЫЕ МОДУЛИ', 'dim')
    g.slots(12, sy + 12, 1 if home else 4, 1, 'cell')
    for q in range(1 if home else 3): item(g, 13 + q * 18, sy + 13, 'cell')
    g.button(127, sy + 3, 38, 15, 'ВЫХОД', 'start'); g.button(127, sy + 21, 38, 15, 'ТЕСТ', 'stop')
    g.led(92, sy + 15, 'ok', False); g.mtext(99, sy + 12, 'СЕТЬ', 'dim')
    g.led(92, sy + 25, 'warn'); g.mtext(99, sy + 22, 'БАТ', 'fg')
    return g

def manipulator(blink=True):
    g = simple_frame('Манипулятор M2', 'run')
    X, Y = 11, 22
    header(g, X, Y, 25, ' ARM-07  ЦИКЛ', 'шаг 2/4 ', 'run')
    # top view of the reach: 9x... grid 5 px, arm in the centre
    gx, gy = X + 2, Y + LINE + 3
    for j in range(7):
        for i in range(9):
            d2 = (i - 4) ** 2 + (j - 3) ** 2
            g.rect(gx + i * 6, gy + j * 6, 5, 5, 'faint' if d2 <= 13 else 'glass_hi')
    g.rect(gx + 4 * 6, gy + 3 * 6, 5, 5, 'idle')
    g.rect(gx + 1 * 6, gy + 2 * 6, 5, 5, 'data'); g.rect(gx + 7 * 6, gy + 4 * 6, 5, 5, 'run'); g.rect(gx + 6 * 6, gy + 1 * 6, 5, 5, 'ok')
    tx = X + 10 * CELL
    for i, (s, col) in enumerate((('1 ВЗЯТЬ A порт ', 'dim'), ('2 ПОЛОЖ B dock1', 'fg'), ('3 ВЗЯТЬ B dock1', 'dim'), ('4 ПОЛОЖ C лента', 'dim'))):
        if i == 1: g.rect(tx - 2, Y + (1 + i) * LINE + 2, 15 * CELL + 3, LINE, 'run_deep')
        g.mtext(tx, Y + (1 + i) * LINE + 2, s, col)
    sy = 85
    g.mtext(12, sy - 1, 'ФИЛЬТР', 'dim'); g.slots(12, sy + 12, 3, 1, 'filter'); item(g, 13, sy + 13, 'wafer')
    g.mtext(74, sy - 1, 'ЗАХВАТ', 'dim'); g.slot(83, sy + 12, 'in'); item(g, 84, sy + 13, 'wafer')
    g.button(127, sy + 3, 38, 15, 'ТОЧКИ', 'ghost'); g.button(127, sy + 21, 38, 15, 'СТОП', 'stop')
    return g

def station(blink=True):
    g = simple_frame('Станция ST-12', 'idle')
    X, Y = 11, 22
    header(g, X, Y, 25, ' ЛИТО-ВХОД', 'свободна ', 'fg')
    g.mtext(X, Y + 1 * LINE + 2, 'АДРЕС', 'dim'); g.mtext(X + 7 * CELL, Y + 1 * LINE + 2, '12  линия A', 'fg')
    g.mtext(X, Y + 2 * LINE + 2, 'РЕЖИМ', 'dim'); g.mtext(X + 7 * CELL, Y + 2 * LINE + 2, 'ВЫГРУЗКА В ПОРТ', 'data')
    g.mtext(X, Y + 3 * LINE + 2, 'ЗАПРОС', 'dim'); g.mtext(X + 7 * CELL, Y + 3 * LINE + 2, 'пластины < 8', 'fg')
    g.mtext(X, Y + 4 * LINE + 3, 'тележка #3 в пути, 6 с' + ('█' if blink else ''), 'dim')
    sy = 85
    g.mtext(12, sy - 1, 'ФИЛЬТР', 'dim'); g.slots(12, sy + 12, 3, 1, 'filter'); item(g, 13, sy + 13, 'wafer')
    g.mtext(74, sy - 1, 'БУФЕР', 'dim'); g.slots(74, sy + 12, 2, 1, 'in')
    g.button(127, sy + 3, 38, 15, 'РЕЖИМ', 'ghost'); g.button(127, sy + 21, 38, 15, 'ВЫЗОВ', 'start')
    return g

if __name__ == '__main__':
    out = lambda n: os.path.join(OUT, n)
    L = {}
    for st in ('off', 'idle', 'run', 'error', 'no_power', 'no_mask'):
        L[st] = litho(st); L[st].save(out('litho_%s.png' % st))
    scenes = {'chip_line_run': chip_line(), 'arc_furnace_run': arc_furnace(), 'generator_run': generator(),
              'ups_mk2': ups(), 'ups_home_dark': ups(DARK), 'manipulator': manipulator(), 'station': station()}
    for n, g in scenes.items(): g.save(out(n + '.png'))
    sheet([L[s].im for s in ('off', 'idle', 'run', 'error', 'no_power', 'no_mask')], out('sheet_litho_states.png'), cols=3,
          labels=['off: рубильник выключен', 'idle: ожидание', 'run: экспозиция', 'ошибка: выход заполнен', 'нет питания', 'нет маски'])
    sheet([scenes[n].im for n in ('arc_furnace_run', 'generator_run', 'ups_mk2', 'ups_home_dark', 'manipulator', 'station')], out('sheet_simple_frames.png'),
          cols=3, labels=['дуговая печь (каркас S)', 'топливный генератор', 'ИБП, скин Mk2', 'ИБП бытовой, тёмный скин', 'манипулятор', 'станция монорельса'])
    sheet([scenes['chip_line_run'].im, L['run'].im], out('sheet_console_frames.png'), cols=2, labels=['линия электроники: два участка', 'литограф: работа'])
    # animation frames: cursor blink + current field blink (two frames, 10 ticks each)
    a, b = litho('run', True).im, litho('run', False).im
    a.resize((512, 472), Image.NEAREST).save(out('litho_run_anim@2x.gif'), save_all=True, append_images=[b.resize((512, 472), Image.NEAREST)], duration=500, loop=0)
