"""Console program mockups (fcw top, fcw litho, ...) drawn into a cell grid with the mod's own font.
Run: python3 tui.py  -> ../tui_*.png
Font: native/crates/fcw-client/assets/terminus-uni2-8x16.psf, with the same fixes the client applies
(font.rs: cyrillic_i() and synth_blocks())."""
import os, random
from PIL import Image
HERE = os.path.dirname(os.path.abspath(__file__)); OUT = os.path.join(HERE, '..')
PSF = os.environ.get('FCW_TERM_PSF', os.path.join(HERE, '..', '..', '..', '..', 'native', 'crates', 'fcw-client', 'assets', 'terminus-uni2-8x16.psf'))
CW, CH = 8, 16

def load_font():
    b = open(PSF, 'rb').read(); assert b[0] == 0x36 and b[1] == 0x04
    mode, h = b[2], b[3]; n = 512 if mode & 1 else 256
    gl = [list(b[4 + i * h: 4 + (i + 1) * h]) for i in range(n)]; t = {}
    p = 4 + n * h; i = 0
    while p + 1 < len(b) and i < n:
        u = b[p] | (b[p + 1] << 8); p += 2
        if u == 0xFFFF: i += 1
        elif u != 0xFFFE: t.setdefault(chr(u), gl[i])
    # font.rs cyrillic_i(): и / й with a diagonal
    body = [0x42, 0x46, 0x4a, 0x52, 0x62, 0x42, 0x42]
    g = [0] * 16; g[5:12] = body; t['и'] = g
    g2 = list(g); g2[2] = 0x24; g2[3] = 0x18; t['й'] = g2
    # font.rs synth_blocks()
    rows = lambda f: [f(r) for r in range(16)]
    for cp, gg in ((0x2580, rows(lambda r: 0xff if r < 8 else 0)), (0x2584, rows(lambda r: 0xff if r >= 8 else 0)), (0x2588, [0xff] * 16),
                   (0x258c, [0xf0] * 16), (0x2590, [0x0f] * 16), (0x2591, rows(lambda r: 0x88 if r % 2 == 0 else 0x22)),
                   (0x2592, rows(lambda r: 0xaa if r % 2 == 0 else 0x55)), (0x2593, rows(lambda r: 0x77 if r % 2 == 0 else 0xdd))):
        t.setdefault(chr(cp), gg)
    for k in range(1, 8):
        t.setdefault(chr(0x2580 + k), rows(lambda r, k=k: 0xff if r >= 16 - 2 * k else 0))
        t.setdefault(chr(0x2588 + k), [(0xff << k) & 0xff] * 16)
    return t
FONT = load_font()

BG, FG, DIM, FAINT = 0x0B0C0F, 0xC9CDD6, 0x6B7080, 0x1A1C22
ACC, ACC_D, RED, RED_D = 0xA88BFF, 0x2C2650, 0xE5534B, 0x4A1B19

class Grid:
    def __init__(s, cols, rows):
        s.c, s.r = cols, rows; s.cells = [[(' ', FG, BG) for _ in range(cols)] for _ in range(rows)]
    def put(s, x, y, text, fg=FG, bg=None, inv=False):
        for i, ch in enumerate(text):
            if 0 <= x + i < s.c and 0 <= y < s.r:
                b = s.cells[y][x + i][2] if bg is None else bg
                s.cells[y][x + i] = (ch, b, fg) if inv else (ch, fg, b)
        return x + len(text)
    def fill(s, x, y, w, h, bg, ch=' ', fg=FG):
        for j in range(h):
            s.put(x, y + j, ch * w, fg, bg)
    def rule(s, x, y, w, title='', fg=DIM, tfg=FG):
        s.put(x, y, '─' * w, fg)
        if title: s.put(x + 2, y, ' ' + title + ' ', tfg)
    def box(s, x, y, w, h, title='', fg=DIM, tfg=FG):
        s.put(x, y, '┌' + '─' * (w - 2) + '┐', fg); s.put(x, y + h - 1, '└' + '─' * (w - 2) + '┘', fg)
        for j in range(1, h - 1): s.put(x, y + j, '│', fg); s.put(x + w - 1, y + j, '│', fg)
        if title: s.put(x + 2, y, ' ' + title + ' ', tfg)
    def bar(s, x, y, w, frac, fg=ACC, back=FAINT):
        """horizontal bar, half a cell high so that bars in adjacent rows do not merge"""
        n = int(round(max(0, min(1, frac)) * w))
        for i in range(w): s.put(x + i, y, '▄', fg if i < n else back)
    def kv(s, x, y, k, v, kw=11, vfg=FG):
        s.put(x, y, k, DIM); s.put(x + kw, y, v, vfg)
    def title(s, left, right, state=None, sfg=ACC):
        s.fill(0, 0, s.c, 1, FG); s.put(1, 0, left, BG, FG); s.put(s.c - 1 - len(right), 0, right, BG, FG)
        if state:
            x = s.c - 3 - len(right) - len(state) - 2
            s.put(x, 0, ' ' + state + ' ', FG if sfg == BG else BG, sfg if sfg != BG else BG)
    def tabs(s, names, active, y=1):
        x = 1
        for i, n in enumerate(names):
            lab = ' %d %s ' % (i + 1, n)
            s.put(x, y, lab, BG if i == active else DIM, ACC if i == active else None); x += len(lab) + 1
    def keys(s, items):
        y = s.r - 1; x = 0; s.fill(0, y, s.c, 1, FAINT)
        for k, lab in items:
            s.put(x, y, k, BG, FG); x += len(k); s.put(x, y, lab + ' ', FG, FAINT); x += len(lab) + 1
    def save(s, name):
        im = Image.new('RGB', (s.c * CW, s.r * CH)); px = im.load()
        col = lambda v: ((v >> 16) & 255, (v >> 8) & 255, v & 255)
        for y, row in enumerate(s.cells):
            for x, (ch, fg, bg) in enumerate(row):
                g = FONT.get(ch); f, b = col(fg), col(bg)
                assert g is not None or ch == ' ', 'no glyph %r' % ch
                for r in range(16):
                    bits = g[r] if g else 0
                    for c in range(8): px[x * CW + c, y * CH + r] = f if bits & (0x80 >> c) else b
        im.save(os.path.join(OUT, name), optimize=True)

# ------------------------------------------------------------------ wafer map
ROWS61 = [3, 7, 7, 9, 9, 9, 7, 7, 3]           # 61 fields
def fields():
    """(index, col, row) in exposure order: serpentine by rows"""
    out = []; n = 0
    for r, w in enumerate(ROWS61):
        cols = list(range((9 - w) // 2, (9 - w) // 2 + w))
        if r % 2: cols.reverse()
        for c in cols: out.append((n, c, r)); n += 1
    return out
EDGE = set()
for (n, c, r) in fields():
    if (c - 4) ** 2 + (r - 4) ** 2 >= 13: EDGE.add(n)

def wafer(g, x, y, done, cur=None, bad=None, big=False, final=False):
    """small: field = 3x1 cells + half-row gap (24x24 px on a 32 px pitch); big: 7x3 cells + gap"""
    bad = bad or {}
    fw, fh = (8, 4) if big else (4, 2)
    for (n, c, r) in fields():
        cx, cy = x + c * fw, y + r * fh
        if n in bad and n < done: bgc, fgc, lab = RED_D, RED, bad[n]
        elif cur is not None and n == cur: bgc, fgc, lab = ACC, BG, ' '
        elif n < done: bgc, fgc, lab = ACC_D, ACC, ' '
        else: bgc, fgc, lab = FAINT, DIM, ' '
        if big:
            g.fill(cx, cy, 7, 3, bgc); g.put(cx, cy + 3, '▀' * 7, bgc)
            g.put(cx + 1, cy, '%02d' % (n + 1), fgc, bgc)
            if n in bad and n < done: g.put(cx + 4, cy, {'P': 'ЧАСТ', 'W': 'ПОЛ', 'C': 'КРИС', 'M': 'МАСК', 'T': 'ТЕПЛ', 'V': 'ВИБР', 'E': 'ПИТ'}[bad[n]][:3], fgc, bgc); g.put(cx + 1, cy + 1, 'брак ' + bad[n], fgc, bgc)
            elif n < done: g.put(cx + 1, cy + 1, 'ok', fgc, bgc); g.put(cx + 1, cy + 2, '%+03d нм' % ((n * 7) % 9 - 4), DIM if not final else fgc, bgc)
            elif cur is not None and n == cur: g.put(cx + 1, cy + 1, 'эксп.', fgc, bgc); g.put(cx + 1, cy + 2, '30 мДж', fgc, bgc)
        else:
            g.put(cx, cy, '%s%02d' % (lab, n + 1), fgc, bgc); g.put(cx, cy + 1, '▀▀▀', bgc)

def spark(g, x, y, vals, fg=FG, lo=None, hi=None):
    lo = min(vals) if lo is None else lo; hi = max(vals) if hi is None else hi
    for i, v in enumerate(vals):
        k = int(round((v - lo) / max(1e-9, hi - lo) * 7)); g.put(x + i, y, chr(0x2581 + k) if k < 7 else '█', fg)

def area(g, x, y, w, h, vals, fg=FG, lo=0.0, hi=1.0, upto=None):
    """area chart, h rows, 8 levels per row"""
    for i in range(w):
        v = vals[int(i * len(vals) / w)]; lev = int(round((v - lo) / (hi - lo) * h * 8))
        c = fg if (upto is None or i < upto) else FAINT
        for j in range(h):
            k = max(0, min(8, lev - j * 8))
            if k: g.put(x + i, y + h - 1 - j, '█' if k == 8 else chr(0x2580 + k), c)

KEYS_M = [('F1', 'Справка'), ('F5', 'Пуск'), ('F6', 'Стоп'), ('F7', 'Рецепт'), ('F9', 'top'), ('F10', 'Выход')]

# ------------------------------------------------------------------ fcw litho
def litho(state, big=False):
    C, R = (160, 48) if big else (80, 25)
    g = Grid(C, R)
    st = {'run': ('РАБОТА', ACC), 'fault': ('ОТКАЗ', RED), 'done': ('ОЖИДАНИЕ', FG)}[state]
    g.title('fcw litho  euv-1  литограф EUV Mk2', 'wafer_cpu  лот 0042  14:20:07', st[0], st[1])
    g.tabs(['Пластина', 'Партия', 'Маска', 'Журнал'], 0)
    bad_run = {8: 'P', 17: 'W'}
    bad_done = {0: 'W', 8: 'P', 17: 'W', 29: 'E', 30: 'E', 31: 'E', 44: 'P', 57: 'W', 60: 'M'}
    if state == 'run': done, cur, bad = 23, 23, bad_run
    elif state == 'fault': done, cur, bad = 32, None, {8: 'P', 17: 'W', 29: 'E', 30: 'E', 31: 'E'}
    else: done, cur, bad = 61, None, bad_done
    good = done - len([k for k in bad if k < done])
    if big:
        mw = 9 * 8 + 3; g.box(0, 2, mw, 39, 'Пластина 3/8'); wafer(g, 2, 4, done, cur, bad, big=True, final=state == 'done')
        px = mw + 2; pw = C - px - 1
    else:
        g.box(0, 2, 39, 20, 'Пластина 3/8'); wafer(g, 2, 3, done, cur, bad)
        px = 41; pw = 38
    y = 2
    def sec(t):
        nonlocal y; g.rule(px, y, pw, t); y += 1
    def row(k, v, vfg=FG):
        nonlocal y; g.kv(px + 1, y, k, v, 12, vfg); y += 1
    sec('Экспозиция')
    if state == 'run':
        row('поле', '24 / 61   змейка, ряд 4'); row('доза', '30.0 мДж/см²   +0.2 %'); row('фокус', '+12 нм   наклон 0.3 мкрад'); row('совмещение', '1.8 нм (3σ)')
    elif state == 'fault':
        row('поле', '33 / 61   остановлено', RED); row('причина', 'нет питания, приход 0 FE/t', RED); row('буфер', '1.2k / 400k FE'); row('потеряно', 'поля 30–32 (E)')
    else:
        row('поле', '61 / 61   пластина готова'); row('годных', '%d / 61   %d %%   класс B' % (good, round(good * 100 / 61))); row('брак', 'W 3  P 2  E 3  M 1', RED); row('выгрузка', 'dock2: wafer_cpu x3')
    if big: y += 1
    sec('Маска')
    row('шаблон', 'reticle_cpu'); g.put(px + 1, y, 'ресурс', DIM); g.bar(px + 13, y, 14 if not big else 30, 187 / 256, FG); g.put(px + (28 if not big else 44), y, '187/256'); y += 1
    if big: row('износ', '27 %   брак по маске 0 %   (растёт после 50 %)'); y += 1
    sec('Среда')
    row('чистота', 'ISO 6   двери закрыты'); row('охлаждение', '21.4 °C   в норме'); row('вибрация', 'нет')
    if state == 'fault': row('питание', 'приход 0 FE/t   ИБП нет', RED)
    else: row('питание', '2048 FE/t   буфер 78 %')
    row('APC', 'fedora-1.mc   8 / 8 CU')
    if big: y += 1
    sec('Очередь')
    row('dock1', 'silicon_wafer x5   A/B'); row('осталось', {'run': 'пласт. 0:36  партия 4:10', 'fault': 'ожидание питания', 'done': 'следующая через 0:03'}[state])
    if big:
        y += 1; sec('Партия 0042')
        g.put(px + 1, y, ' №  годных  класс  брак              время', DIM); y += 1
        for i, (gd, cl, br) in enumerate(((57, 'A', 'P 2  W 2'), (59, 'S', 'P 1  W 1'), ((52, 'B', 'W 3  P 2  E 3  M 1') if state == 'done' else (None, '', '')))):
            if gd: g.put(px + 1, y, ' %d   %d/61   %s      %-16s  1:00' % (i + 1, gd, cl, br))
            else: g.put(px + 1, y, ' 3   в работе', ACC)
            y += 1
        y += 1; sec('Годность по партиям')
        vals = [52, 55, 49, 57, 58, 56, 59, 57, 60, 58, 59, 57, 59]
        spark(g, px + 1, y, vals, FG, 40, 61); g.put(px + 16, y, 'последние 13 пластин   среднее 93 %', DIM); y += 2
        sec('Журнал')
        for ln in ('14:18:41  ожидание: нет пластин', '14:19:52  пуск wafer_cpu (fedora-1.mc)', '14:19:58  пластина 3: совмещение ok', '14:20:07  поле 24'):
            g.put(px + 1, y, ln, DIM); y += 1
        # legend under the map
        ly = 42
        g.put(2, ly, '  ', FG, FAINT); g.put(5, ly, 'ждёт'); g.put(12, ly, '  ', FG, ACC_D); g.put(15, ly, 'годен'); g.put(23, ly, '  ', FG, ACC); g.put(26, ly, 'идёт'); g.put(33, ly, '  ', FG, RED_D); g.put(36, ly, 'брак:')
        g.put(42, ly, 'P частицы  W полировка  C кристалл', DIM); g.put(42, ly + 1, 'M маска  T тепло  V вибрация  E питание', DIM)
        g.put(2, R - 4, 'пластина', DIM); g.bar(12, R - 4, 50, done / 61, ACC if state != 'fault' else RED); g.put(64, R - 4, '%d %%' % round(done * 100 / 61))
        g.put(2, R - 3, 'партия', DIM); g.bar(12, R - 3, 50, (2 + done / 61) / 8, FG); g.put(64, R - 3, '%d %%' % round((2 + done / 61) / 8 * 100))
    else:
        g.put(1, 22, 'пластина', DIM); g.bar(10, 22, 22, done / 61, ACC if state != 'fault' else RED); g.put(33, 22, '%3d %%' % round(done * 100 / 61))
        g.put(41, 22, 'P частицы W полир. E питание M маска', DIM)
        last = {'run': '14:20:07  поле 24', 'fault': '14:20:31  отказ: нет питания, поля 30–32 потеряны', 'done': '14:20:52  пластина 3 готова: 52/61, класс B'}[state]
        g.put(1, 23, last, RED if state == 'fault' else FG)
    g.keys([('F1', 'Справка'), ('F2', 'Партия'), ('F3', 'Маска'), ('F4', 'Журнал'), ('F5', 'Пуск'), ('F6', 'Стоп'), ('F7', 'Рецепт'), ('F10', 'Выход')])
    g.save('tui_litho_%s_%dx%d.png' % (state, C, R))

def litho_lot():
    g = Grid(80, 25); g.title('fcw litho  euv-1  литограф EUV Mk2', 'wafer_cpu  лот 0042  14:20:07', 'РАБОТА', ACC)
    g.tabs(['Пластина', 'Партия', 'Маска', 'Журнал'], 1)
    g.put(1, 3, ' №  годных   %   класс  брак по причинам        время   сырьё', DIM); g.rule(1, 4, 78)
    rowsd = [(1, 57, 'A', 'P 2  W 2', '1:00', 'A/B'), (2, 59, 'S', 'P 1  W 1', '1:00', 'A/B'), (3, None, '', '', '', 'A/B')]
    for i, (n, gd, cl, br, tm, sub) in enumerate(rowsd):
        if gd: g.put(1, 5 + i, ' %d   %d/61  %3d   %s      %-22s  %s    %s' % (n, gd, round(gd * 100 / 61), cl, br, tm, sub))
        else: g.put(1, 5 + i, ' %d   идёт экспозиция, поле 24/61                              %s' % (n, sub), ACC)
    for i in range(4, 9): g.put(1, 4 + i, ' %d   в очереди (dock1)' % i, DIM)
    g.rule(1, 14, 78, 'Причины брака в партии')
    for i, (k, n, v) in enumerate((('P частицы', 3, .5), ('W полировка', 3, .5), ('M маска', 0, 0), ('E питание', 0, 0))):
        g.put(2, 15 + i, k, FG if n else DIM); g.bar(16, 15 + i, 30, v, RED); g.put(48, 15 + i, '%d' % n)
    g.rule(1, 20, 78, 'Итог')
    g.put(2, 21, 'годных 116 из 122   95 %   классы: S 1  A 1   партия: 8 пластин за 8:00')
    g.put(2, 22, 'в корпусировке даст: cpu_lga ≈ 17 из 8 пластин (номинал 16)', DIM)
    g.keys([('F1', 'Справка'), ('F2', 'Партия'), ('F3', 'Маска'), ('F4', 'Журнал'), ('F5', 'Пуск'), ('F6', 'Стоп'), ('F7', 'Рецепт'), ('F10', 'Выход')])
    g.save('tui_litho_lot_80x25.png')

# ------------------------------------------------------------------ fcw top
def top():
    g = Grid(80, 25); g.title('fcw top  сеть fedora-1.mc', '6 машин  14:20:07')
    g.put(1, 2, 'Питание', DIM); g.bar(10, 2, 24, 4096 / 6000, FG); g.put(35, 2, '4.1k / 6.0k FE/t')
    g.put(56, 2, 'работа 4  ожид. 1', FG); 
    g.put(1, 3, 'ИБП', DIM); g.bar(10, 3, 24, .74, FG); g.put(35, 3, '74 %  +1.2k FE/t')
    g.put(56, 3, 'отказ 1', RED)
    g.put(1, 4, 'APC', DIM); g.bar(10, 4, 24, 12 / 16, ACC); g.put(35, 4, '12 / 16 CU')
    g.put(56, 4, 'годных за смену 91 %')
    g.fill(0, 6, 80, 1, FAINT); g.put(1, 6, 'ИМЯ    МАШИНА            СОСТОЯНИЕ             РЕЦЕПТ        ХОД   FE/t СВЯЗЬ', FG, FAINT)
    data = [('eaf-1', 'дуговая печь', 'работа: плавка', 'met_silicon', .37, '256', 'кабель', 0),
            ('cz-1', 'ростовая печь', 'работа: рост', 'silicon_boule', .71, '512', 'кабель', 0),
            ('cz-2', 'ростовая печь', 'ожидание: нет сырья', '—', 0, '0', 'кабель', 1),
            ('saw-1', 'резка и полировка', 'работа: полировка', 'silicon_wafer', .88, '256', 'радио', 0),
            ('euv-1', 'литограф EUV Mk2', 'работа: поле 24/61', 'wafer_cpu', .39, '2048', 'кабель', 2),
            ('line-1', 'линия электроники', 'отказ: выход заполнен', 'cpu_lga', 1.0, '0', 'радио', 3)]
    for i, (n, kind, st, rc, pr, fe, link, mode) in enumerate(data):
        y = 7 + i
        if mode == 2: g.fill(0, y, 80, 1, ACC_D)
        fg = RED if mode == 3 else (DIM if mode == 1 else FG)
        g.put(1, y, '%-6s %-17s ' % (n, kind), FG if mode != 1 else DIM); g.put(26, y, '%-21s' % st, ACC if mode == 2 else fg); g.put(48, y, '%-13s' % rc, FG if mode != 1 else DIM)
        g.bar(62, y, 5, pr, RED if mode == 3 else (ACC if mode == 2 else FG), FAINT if mode != 2 else BG); g.put(68, y, '%4s' % fe); g.put(73, y, link, DIM)
    g.rule(0, 14, 80, 'euv-1')
    g.kv(1, 15, 'рецепт', 'wafer_cpu  468/1200 t'); g.kv(41, 15, 'маска', 'reticle_cpu  187/256')
    g.kv(1, 16, 'загрузка', 'dock1: silicon_wafer x5'); g.kv(41, 16, 'выход', 'dock2: wafer_cpu x2')
    g.kv(1, 17, 'годность', 'пластина 21/23   партия 95 %'); g.kv(41, 17, 'питание', '312k/400k FE')
    g.rule(0, 19, 80, 'Журнал сети')
    for i, (ln, c) in enumerate((('14:19:52  euv-1   пуск wafer_cpu (fedora-1.mc)', DIM), ('14:20:01  saw-1   пластина 7/8 в полировке', DIM), ('14:20:04  line-1  отказ: выход заполнен (out)', RED))):
        g.put(1, 20 + i, ln, c)
    g.keys([('F1', 'Справка'), ('F2', 'Питание'), ('F3', 'Журнал'), ('F4', 'Годность'), ('F5', 'Пуск'), ('F6', 'Стоп'), ('Enter', 'Открыть'), ('F10', 'Выход')])
    g.save('tui_top_80x25.png')

# ------------------------------------------------------------------ fcw furnace
def furnace():
    g = Grid(80, 25); g.title('fcw furnace  eaf-1  дуговая печь', 'metallurgical_silicon  14:20:07', 'РАБОТА', ACC)
    g.tabs(['Плавка', 'Шихта', 'Журнал'], 0)
    g.box(0, 2, 50, 14, 'Температура ванны, °C')
    prof = [0.05 + 0.9 * min(1, i / 12) ** 0.7 for i in range(14)] + [0.93 + 0.03 * ((i * 7) % 5 - 2) / 2 for i in range(22)] + [0.95 - 0.5 * (i / 8) for i in range(8)]
    for j, lab in enumerate(('1800', '1200', ' 600', '   0')): g.put(1, 3 + j * 3 + (1 if j else 0), lab, DIM)
    area(g, 6, 3, 42, 10, prof, FG, 0, 1, upto=int(42 * 0.37)); 
    g.put(6, 13, '├─────────────┼─────────────────────┼──────┤'[:43], DIM)
    g.put(6, 14, ' розжиг        плавка                выпуск', DIM); g.put(6 + int(42 * .37), 13, '↑', ACC)
    x = 52
    g.rule(x, 2, 27, 'Электроды'); 
    for i, (v, cur) in enumerate(((.82, 32), (.74, 29), (.88, 35))):
        g.put(x + 1, 3 + i, 'L%d' % (i + 1), DIM); g.bar(x + 4, 3 + i, 14, v, FG); g.put(x + 19, 3 + i, '%d кА' % cur)
    g.kv(x + 1, 6, 'высота', '−2  −1  −3 /16', 9)
    g.rule(x, 8, 27, 'Ванна')
    g.kv(x + 1, 9, 'T', '1640 °C', 9); g.kv(x + 1, 10, 'шихта', '62 %', 9); g.kv(x + 1, 11, 'такт', '148 / 400 t', 9); g.kv(x + 1, 12, 'выпуск', 'через 0:12', 9)
    g.rule(x, 14, 27, 'Питание'); g.kv(x + 1, 15, 'расход', '256 FE/t', 9)
    g.rule(0, 17, 80, 'Загрузка и выход')
    g.kv(1, 18, 'charge', 'quartz x28   coal x14      хватит на 7 плавок'); g.kv(1, 19, 'tap', 'metallurgical_silicon x6')
    g.kv(1, 20, 'за смену', '11 плавок   22 слитка   простои 0:42 (нет шихты)')
    g.put(1, 22, '14:19:40  розжиг: дуга устойчива', DIM); g.put(1, 23, '14:19:46  плавка')
    g.keys(KEYS_M); g.save('tui_furnace_80x25.png')

# ------------------------------------------------------------------ fcw puller
def puller():
    g = Grid(80, 25); g.title('fcw puller  cz-1  ростовая печь', 'silicon_boule  14:20:07', 'РАБОТА', ACC)
    g.tabs(['Рост', 'Тепло', 'Журнал'], 0)
    g.box(0, 2, 30, 21, 'Слиток, мм')
    # boule drawn top-down: seed, neck, shoulder, body; grown part solid, planned part faint
    widths = [1, 1, 1, 3, 7, 11, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 11, 5]
    grown = 12
    for i, w in enumerate(widths):
        y = 4 + i; cx = 16
        g.put(2, y, '%3d' % (i * 20), DIM); g.put(6, y, '┤', DIM)
        g.put(cx - w // 2, y, '█' * w, FG if i < grown else FAINT)
        if i == grown: g.put(cx - 9, y, '~' * 18, ACC); g.put(cx - w // 2, y, '▀' * w, FG, None)
    g.put(8, 3, 'затравка', DIM); g.put(24, 4 + grown, 'мениск', ACC)
    x = 32
    g.rule(x, 2, 47, 'Вытягивание')
    g.kv(x + 1, 3, 'длина', '236 / 340 мм'); g.bar(x + 26, 3, 14, 236 / 340, ACC); g.put(x + 41, 3, '71 %')
    g.kv(x + 1, 4, 'диаметр', '150.2 мм   цель 150 ±1'); g.kv(x + 1, 5, 'скорость', '1.2 мм/мин   вращение 12 об/мин')
    g.kv(x + 1, 6, 'такт', '1704 / 2400 t   осталось 0:35')
    g.rule(x, 8, 47, 'Диаметр по длине')
    spark(g, x + 1, 9, [1, 1, 2, 9, 30, 70, 100, 101, 100, 99, 100, 100, 101, 100, 100, 99, 100, 100, 101, 100, 100, 100, 99, 100], FG, 0, 105)
    g.put(x + 27, 9, 'шейка  плечо  тело', DIM)
    g.rule(x, 11, 47, 'Условия')
    g.kv(x + 1, 12, 'расплав', '1420 °C   ±0.4'); g.kv(x + 1, 13, 'охлаждение', 'вода 21 °C   в норме'); g.kv(x + 1, 14, 'питание', '512 FE/t   пауз 0')
    g.kv(x + 1, 15, 'APC', 'fedora-1.mc   2 / 2 CU   время −20 %')
    g.rule(x, 17, 47, 'Прогноз')
    g.kv(x + 1, 18, 'кристалл', 'класс A   (вода + без пауз)'); g.kv(x + 1, 19, 'за смену', '3 слитка: A 2  B 1')
    g.put(x + 1, 21, '14:02:10  нагрев завершён, затравка', DIM); g.put(x + 1, 22, '14:03:30  рост тела')
    g.keys(KEYS_M); g.save('tui_puller_80x25.png')

# ------------------------------------------------------------------ fcw saw
def saw():
    g = Grid(80, 25); g.title('fcw saw  saw-1  резка и полировка', 'silicon_wafer  14:20:07', 'РАБОТА', ACC)
    g.tabs(['Пластины', 'Инструмент', 'Журнал'], 0)
    g.box(0, 2, 80, 8, 'Слиток: класс кристалла A')
    # boule lying on its side, 8 slices + kerf
    for i in range(8):
        x = 6 + i * 8; st = 'done' if i < 7 else 'cut'
        for r in range(4): g.put(x, 4 + r, '█' * 7, FG if i < 7 else ACC)
        g.put(x + 7, 4, '│', DIM); g.put(x + 7, 7, '│', DIM)
        g.put(x + 2, 8, ' %d ' % (i + 1), DIM)
    g.put(2, 5, 'хвост', DIM); g.put(72, 5, 'затр.', DIM); g.put(62, 3, '↓ пила', ACC)
    g.put(1, 11, ' №  резка   полировка        толщина   TTV      класс', DIM); g.rule(1, 12, 78)
    rowsd = [(1, 'ok', 3, '775 мкм', '1.1 мкм', 'A'), (2, 'ok', 3, '774 мкм', '0.9 мкм', 'A'), (3, 'ok', 3, '776 мкм', '1.4 мкм', 'A'), (4, 'ok', 3, '775 мкм', '1.0 мкм', 'A'),
             (5, 'ok', 3, '775 мкм', '1.2 мкм', 'A'), (6, 'ok', 2, '781 мкм', '—', ''), (7, 'ok', 0, '—', '—', ''), (8, 'идёт', 0, '—', '—', '')]
    for i, (n, cut, pol, th, ttv, cl) in enumerate(rowsd):
        y = 13 + i; cur = n in (6, 8)
        g.put(1, y, ' %d  %-6s' % (n, cut), ACC if n == 8 else FG)
        g.bar(13, y, 9, pol / 3, ACC if n == 6 else FG); g.put(23, y, '%d/3' % pol, DIM if not pol else FG)
        g.put(30, y, '%-9s %-8s %s' % (th, ttv, cl), FG if cl else DIM)
    x = 57
    g.rule(x, 11, 22, 'Условия')
    g.kv(x + 1, 13, 'вода', 'есть', 10); g.kv(x + 1, 14, 'питание', 'пауз 0', 10); g.kv(x + 1, 15, 'APC', '1/1 CU', 10); g.kv(x + 1, 16, 'выход', '8 + 1 шт.', 10)
    g.kv(x + 1, 18, 'полировка', 'класс A', 10); g.kv(x + 1, 19, 'такт', '704/800 t', 10)
    g.put(1, 22, '14:19:58  пластина 6: проход полировки 2', DIM); g.put(1, 23, '14:20:05  рез 8')
    g.keys(KEYS_M); g.save('tui_saw_80x25.png')

# ------------------------------------------------------------------ fcw line
def line():
    g = Grid(80, 25); g.title('fcw line  line-1  линия электроники', 'cpu_lga / motherboard  14:20:07', 'РАБОТА', ACC)
    g.tabs(['Линия', 'Годность', 'Журнал'], 0)
    def strip(y, title, names, counts, active, frac, takt):
        g.rule(0, y, 80, title)
        x = 1
        for i, (n, c) in enumerate(zip(names, counts)):
            w = len(n) + 2
            g.put(x, y + 2, ' ' + n + ' ', BG if i == active else FG, ACC if i == active else FAINT)
            g.put(x + (w - len(c)) // 2, y + 3, c, DIM if c == '·' else FG)
            x += w
            if i < len(names) - 1: g.put(x, y + 2, '→', DIM); x += 1
        g.put(1, y + 5, 'такт', DIM); g.bar(7, y + 5, 30, frac, ACC); g.put(38, y + 5, takt)
    strip(2, 'Корпусировка   cpu_lga', ['вход', 'резка', 'посадка', 'разварка', 'корпус', 'тест', 'лоток'], ['3', '·', '1', '1', '·', '1', '24'], 3, .27, '212 / 800 t')
    g.kv(42 + 12, 7, 'пластина', 'класс B', 9)
    strip(9, 'Сборка плат   motherboard', ['магазин', 'паста', 'установка', 'печь', 'контроль', 'выход'], ['5', '1', '·', '1', '·', '3'], 3, .78, '310 / 400 t')
    g.kv(42 + 12, 14, 'печь', '245 °C', 9)
    g.rule(0, 16, 80, 'Выход годных, эта смена')
    g.put(1, 17, 'изделие        пластин  классы            номинал  вышло   %', DIM)
    for i, (n, w, cls, nom, out, pc) in enumerate((('cpu_lga', 6, 'S 1  A 3  B 2', 12, 11, 92), ('chip_memory', 4, 'A 2  B 2', 24, 21, 88), ('motherboard', 0, '—', 5, 5, 100))):
        g.put(1, 18 + i, '%-14s %-8s %-17s %-8d %-7d' % (n, w or '—', cls, nom, out)); g.bar(60, 18 + i, 12, pc / 100, FG); g.put(73, 18 + i, '%d' % pc)
    g.put(1, 22, '14:19:31  корпусировка: пластина класса B, выход 1.6 из 2', DIM); g.put(1, 23, '14:20:04  сборка: плата 3 в печи')
    g.keys(KEYS_M); g.save('tui_line_80x25.png')

# ------------------------------------------------------------------ fcw yield
def yield_():
    g = Grid(80, 25); g.title('fcw yield  отчёт за смену', 'день 12  06:00–14:20')
    g.tabs(['Сводка', 'Пластины', 'Причины', 'Простои'], 0)
    g.put(1, 3, 'рецепт            пластин  годных  S  A  B  C  D   по пластинам', DIM); g.rule(1, 4, 78)
    for i, (n, w, pc, cl) in enumerate((('wafer_cpu', 14, 91, (3, 7, 3, 1, 0)), ('wafer_memory', 8, 88, (1, 4, 3, 0, 0)), ('wafer_controller', 6, 79, (0, 2, 3, 1, 0)))):
        y = 5 + i
        g.put(1, y, '%-17s %-8d %2d %%   %2d %2d %2d %2d %2d' % ((n, w, pc) + cl))
        x = 50
        for k, c in enumerate(cl):
            g.put(x, y, 'SABCD'[k] * c, (FG, FG, DIM, DIM, RED)[k]); x += c
    g.rule(1, 10, 78, 'Причины брака, полей')
    causes = (('P частицы', 61, 'ISO 6: поднять до ISO 5'), ('W полировка', 48, 'пластины класса B: дать воду резке'), ('E питание', 22, '3 просадки: нужен ИБП'),
              ('M маска', 9, 'reticle_memory изношена на 71 %'), ('C кристалл', 6, ''), ('T тепло', 0, ''), ('V вибрация', 0, ''))
    for i, (k, n, hint) in enumerate(causes):
        y = 11 + i; g.put(2, y, k, FG if n else DIM); g.bar(16, y, 20, n / 61, RED); g.put(37, y, '%3d' % n, FG if n else DIM); g.put(43, y, hint, DIM)
    g.rule(1, 19, 78, 'Годность по пластинам, %')
    vals = [72, 80, 85, 84, 88, 90, 86, 52, 89, 92, 93, 90, 94, 95, 93, 91, 96, 93, 94, 95, 61, 92, 94, 96, 95, 93, 97, 95]
    spark(g, 2, 20, vals, FG, 40, 100); g.put(9, 21, '↑09:14', RED); g.put(22, 21, '↑12:40 просадки питания', RED)
    g.put(32, 20, 'мин 52   средн. 89   макс 97', DIM)
    g.put(1, 23, 'вышло: cpu_lga 26 (номинал 28)  chip_memory 42 (48)  chip_controller 38 (48)')
    g.keys([('F1', 'Справка'), ('F2', 'Смена'), ('F3', 'Машина'), ('F8', 'В файл'), ('F9', 'top'), ('F10', 'Выход')])
    g.save('tui_yield_80x25.png')

if __name__ == '__main__':
    for st in ('run', 'fault', 'done'): litho(st)
    litho('run', big=True); litho('done', big=True); litho_lot()
    top(); furnace(); puller(); saw(); line(); yield_()
