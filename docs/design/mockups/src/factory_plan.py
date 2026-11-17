"""Demo factory plans (top views + sections). Run: python3 factory_plan.py  -> ../factory_*.png
Grid: 1 block = 8 px. x east, z south, y up. Building 103 x 47 blocks, origin = NW corner, floor y = 0."""
import os
from PIL import Image, ImageDraw, ImageFont
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
S = 12; MX, MY = 44, 74
F = lambda n: ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', n)
FB = lambda n: ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf', n)
C = dict(bg='#F4F5F7', wall='#1B1C21', floor='#E7E9ED', glass='#7FD3E8', grid='#DADDE3', ink='#1E2026', dim='#6D7280',
         mono='#8B6CF0', arm='#22A06B', cable='#E08A45', tour='#E5484D', mach='#FFFFFF', machedge='#1B1C21',
         hot='#F6D9C8', clean='#DDF1F7', power='#F8E8C4', it='#DCD6F7', store='#E3E6DA', office='#E9E4DA', void='#C9CDD6', solar='#27324A')
L, D = 103, 47
XW = [0, 11, 29, 43, 56, 81, 102]          # x of cross walls
ZW = [0, 11, 34, 46]                        # z of long walls

def canvas(title, sub, w=L, h=D, extra_h=70):
    im = Image.new('RGB', (MX * 2 + w * S, MY + h * S + extra_h), C['bg']); d = ImageDraw.Draw(im)
    d.text((MX, 12), title, font=FB(18), fill=C['ink']); d.text((MX, 36), sub, font=F(11), fill=C['dim'])
    return im, d
def P(x, z): return (MX + x * S, MY + z * S)
def box(d, x0, z0, x1, z1, fill=None, outline=None, width=1):
    """inclusive block ranges"""
    d.rectangle([P(x0, z0), (MX + (x1 + 1) * S - 1, MY + (z1 + 1) * S - 1)], fill=fill, outline=outline, width=width)
def label(d, x, z, text, size=10, fill=None, bold=False, anchor='mm'):
    d.multiline_text((MX + x * S, MY + z * S), text, font=(FB if bold else F)(size), fill=fill or C['ink'], anchor=anchor, align='center', spacing=2)
def grid(d, w=L, h=D):
    for x in range(0, w + 1, 8): d.line([P(x, 0), P(x, h)], fill=C['grid'])
    for z in range(0, h + 1, 8): d.line([P(0, z), P(w, z)], fill=C['grid'])
    for x in range(0, w + 1, 16): d.text((MX + x * S, MY - 12), str(x), font=F(9), fill=C['dim'], anchor='mm')
    for z in range(0, h + 1, 16): d.text((MX - 14, MY + z * S), str(z), font=F(9), fill=C['dim'], anchor='mm')
def walls(d, glass_north_band=False):
    for z in ZW: box(d, 0, z, L - 1, z, C['wall'])
    for x in XW: box(d, x, 0, x, D - 1, C['wall'])
def door(d, x0, z0, x1, z1, col=None): box(d, x0, z0, x1, z1, col or C['floor'])
def machine(d, x0, z0, w, dz, name, ports=()):
    box(d, x0, z0, x0 + w - 1, z0 + dz - 1, C['mach'], C['machedge'], 2)
    d.line([P(x0, z0), P(x0 + w, z0)], fill=C['mono'], width=3)          # operator side (north)
    label(d, x0 + w / 2, z0 + dz / 2, name, 9, bold=True)
    for (px, pz, col) in ports:
        cx, cy = MX + px * S, MY + pz * S; d.ellipse([cx - 3, cy - 3, cx + 3, cy + 3], fill=col, outline=C['ink'])
def arm(d, x, z, r=3):
    cx, cy = MX + x * S, MY + z * S
    d.ellipse([cx - r * S, cy - r * S, cx + r * S, cy + r * S], outline=C['arm'])
    d.ellipse([cx - 5, cy - 5, cx + 5, cy + 5], fill=C['arm'], outline=C['ink'])
def station(d, x, z, n):
    cx, cy = MX + x * S, MY + z * S
    d.rectangle([cx - 6, cy - 6, cx + 6, cy + 6], fill=C['mono'], outline=C['ink']); d.text((cx, cy), n, font=FB(8), fill='#FFFFFF', anchor='mm')
def poly(d, pts, col, width=3, dash=None):
    pts = [(MX + x * S, MY + z * S) for x, z in pts]
    if not dash: d.line(pts, fill=col, width=width, joint='curve'); return
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        n = max(1, int((abs(x1 - x0) + abs(y1 - y0)) / dash))
        for i in range(0, n, 2):
            a, b = i / n, min(1, (i + 1) / n)
            d.line([(x0 + (x1 - x0) * a, y0 + (y1 - y0) * a), (x0 + (x1 - x0) * b, y0 + (y1 - y0) * b)], fill=col, width=width)
def legend(d, y, items):
    x = MX
    for kind, col, text in items:
        if kind == 'line': d.line([(x, y + 6), (x + 22, y + 6)], fill=col, width=3)
        elif kind == 'dash': d.line([(x, y + 6), (x + 8, y + 6)], fill=col, width=3); d.line([(x + 14, y + 6), (x + 22, y + 6)], fill=col, width=3)
        elif kind == 'dot': d.ellipse([x + 6, y, x + 18, y + 12], fill=col, outline=C['ink'])
        else: d.rectangle([x + 4, y, x + 18, y + 12], fill=col, outline=C['ink'])
        d.text((x + 28, y), text, font=F(10), fill=C['ink']); x += 36 + int(d.textlength(text, font=F(10)))

ROOMS_PROC = [(1, 10, 'store', 'A. Приёмка\nи склад\nсырья'), (12, 28, 'hot', 'B. Плавильный цех'), (30, 42, 'hot', 'C. Рост\nкристаллов'),
              (44, 55, 'floor', 'D. Резка и\nполировка'), (57, 80, 'clean', 'E. Чистая комната (ISO 5)'), (82, 101, 'floor', 'F. Линия электроники')]
ROOMS_N = [(1, 10, 'store', 'N1. Офис\nсклада'), (12, 28, 'office', 'N2. Музей сырья\nи раздевалки'), (30, 42, 'it', 'N3. Диспет-\nчерская'),
           (44, 55, 'office', 'N4. Вестибюль\n(вход)'), (57, 66, 'clean', 'N5. Шлюз\nпереодев.'), (68, 80, 'clean', 'N6. Участок\nмасок'),
           (82, 101, 'office', 'N7. Шоурум: сборка ПК\nи отгрузка')]
ROOMS_S = [(1, 10, 'store', 'S1. Топливо\nи ЗИП'), (12, 28, 'power', 'S2. Генераторный зал'), (30, 42, 'power', 'S3. Подстанция'),
           (44, 55, 'power', 'S4. ИБП\n(батареи)'), (57, 80, 'it', 'S5. Серверный зал'), (82, 101, 'store', 'S6. Депо монорельса\nи мастерская')]

def ground():
    im, d = canvas('Демо-завод FCW: 1 этаж (y = 1…4, цеха до y = 10)', 'Здание 103 × 47 блоков. Фиолетовая кромка машины = сторона оператора (север).')
    box(d, 0, 0, L - 1, D - 1, C['floor'])
    for x0, x1, c, t in ROOMS_PROC: box(d, x0, 12, x1, 33, C[c])
    for x0, x1, c, t in ROOMS_N: box(d, x0, 1, x1, 10, C[c])
    box(d, 67, 1, 67, 10, C['wall'])
    for x0, x1, c, t in ROOMS_S: box(d, x0, 35, x1, 45, C[c])
    grid(d); walls(d)
    for x0, x1, c, t in ROOMS_PROC: label(d, (x0 + x1 + 1) / 2, 26.5 if x0 != 57 else 27, t, 10, bold=True)
    for x0, x1, c, t in ROOMS_N: label(d, (x0 + x1 + 1) / 2, 4.5, t, 9, bold=True)
    for x0, x1, c, t in ROOMS_S: label(d, (x0 + x1 + 1) / 2, 43.6, t, 9, bold=True)
    # doors and gates
    for (a, b, c2, e) in ((0, 20, 0, 24), (48, 0, 51, 0), (11, 21, 11, 23), (29, 21, 29, 23), (43, 24, 43, 26), (102, 4, 102, 7),
                          (60, 11, 62, 11), (36, 11, 37, 11), (20, 34, 21, 34), (36, 34, 37, 34), (49, 34, 50, 34), (70, 34, 71, 34),
                          (90, 34, 92, 34), (5, 34, 6, 34), (5, 11, 6, 11), (48, 11, 51, 11), (90, 11, 93, 11), (43, 5, 43, 6), (56, 5, 56, 6),
                          (29, 5, 29, 6), (81, 5, 81, 6), (11, 5, 11, 6), (66, 5, 66, 6)):
        door(d, a, b, c2, e)
    # glass: cleanroom walls, control room, lobby front
    for (a, b, c2, e) in ((56, 14, 56, 31), (81, 14, 81, 31), (64, 11, 79, 11), (30, 11, 35, 11), (38, 11, 42, 11), (44, 0, 47, 0), (52, 0, 55, 0), (83, 0, 100, 0)):
        box(d, a, b, c2, e, C['glass'])
    # pass-through airlocks for the monorail in cleanroom walls
    for x in (56, 81): box(d, x, 15, x, 15, C['mono']); box(d, x, 31, x, 31, C['mono'])
    # machines
    IT, EN, FL = '#3B82F6', C['cable'], '#14B8A6'
    machine(d, 16, 18, 8, 5, 'Дуговая печь\n8×6×5', [(16, 20.5, IT), (20.5, 19.5, IT), (23.5, 23, EN)])
    machine(d, 31, 19, 5, 4, 'Рост 1\n5×7×4', [(31, 21.5, IT), (33.5, 19, IT), (35.5, 23, EN)])
    machine(d, 37, 19, 5, 4, 'Рост 2', [(37, 21.5, IT), (39.5, 19, IT), (41.5, 23, EN)])
    machine(d, 46, 19, 8, 3, 'Резка и полировка\n8×3×3', [(46, 20.5, IT), (50.5, 19, IT), (52.5, 22, EN)])
    machine(d, 63, 18, 13, 5, 'Литограф EUV Mk2\n13×6×5', [(63, 19.5, IT), (63, 21.5, IT), (66.5, 18, IT), (74.5, 23, EN)])
    machine(d, 85, 19, 14, 3, 'Линия электроники\n14×3×3', [(85, 20.5, IT), (86.5, 22, IT), (87, 19.5, IT), (99, 20.5, IT), (92.5, 22, EN)])
    # power hall content
    for i in range(6): box(d, 13 + i * 2, 37, 13 + i * 2, 37, '#3A3D48')
    box(d, 24, 39, 26, 40, '#FFFFFF', C['ink']); label(d, 20, 39.5, 'ген. ×6 + турбина', 8)
    box(d, 33, 37, 36, 39, '#FFFFFF', C['ink']); label(d, 35, 38.5, 'ПС 4×4×3', 8)
    for i in range(8): box(d, 45 + i, 36, 45 + i, 36, '#FFFFFF', C['ink'])
    for r in range(3):
        for i in range(9): box(d, 59 + i * 2, 36 + r * 2, 59 + i * 2, 36 + r * 2, '#2A2C36')
    box(d, 77, 38, 79, 41, '#FFFFFF', C['ink']); label(d, 78.5, 39.8, 'супер-\nкомп.', 7)
    # monorail loop (y = 8) and stations
    loop = [(4, 15), (100, 15), (100, 31), (4, 31), (4, 15)]
    poly(d, loop, C['mono'], 3)
    poly(d, [(90, 31), (90, 38), (97, 38)], C['mono'], 3)          # depot spur
    poly(d, [(20.5, 15), (20.5, 19)], C['mono'], 3)                 # spur over the furnace charge port
    for (x, z, n) in ((4, 20, '01'), (20.5, 19.3, '02'), (14, 15, '03'), (33.5, 15, '04'), (39.5, 15, '05'), (50.5, 15, '06'), (60, 15, '07'), (84, 15, '08'),
                      (100, 20.5, '09'), (87, 31, '10'), (95, 38, 'D')):
        station(d, x, z, n)
    # arms
    for (x, z, r) in ((14, 20.5, 3), (30, 17, 3), (36.3, 17, 2), (44.6, 18, 3), (50.5, 17, 2), (60.5, 19.5, 3), (83.5, 19.5, 2), (100.3, 18.5, 2), (87, 25, 3)):
        arm(d, x, z, r)
    # main cable trunk (floor ducts) from the power band
    poly(d, [(12, 35), (101, 35)], C['cable'], 2)
    for x in (23.5, 35.5, 41.5, 52.5, 74.5, 92.5): poly(d, [(x, 35), (x, 23.2 if x not in (52.5, 92.5) else 22.2)], C['cable'], 2)
    # tour (ground part)
    poly(d, [(49.5, -2), (49.5, 6), (20, 6), (20, 9)], C['tour'], 2, dash=8); label(d, 22.5, 8.6, '↑ на галерею', 8, C['tour'], anchor='lm')
    poly(d, [(96, 9), (96, 6), (61, 6), (61, 9)], C['tour'], 2, dash=8)
    poly(d, [(58, 6), (49.5, 6)], C['tour'], 2, dash=8)
    poly(d, [(36.5, 6), (36.5, 12), (36.5, 34), (36.5, 41), (50, 41), (50, 36)], C['tour'], 2, dash=8)
    label(d, 49.5, -3.2, 'ВХОД', 9, C['tour'], True); label(d, -2.5, 22, 'сырьё', 8, C['dim']); label(d, 105.2, 5.5, 'отгр.', 8, C['dim'])
    y = MY + D * S + 14
    legend(d, y, [('line', C['mono'], 'монорельс (y = 8) и станции'), ('dot', C['arm'], 'манипулятор и его радиус'), ('line', C['cable'], 'кабельный канал в полу'),
                  ('dash', C['tour'], 'маршрут экскурсии'), ('dot', '#3B82F6', 'порт предметов'), ('dot', C['cable'], 'ввод питания'), ('box', C['glass'], 'стекло')])
    legend(d, y + 22, [('box', C['hot'], 'горячие цеха'), ('box', C['clean'], 'чистая зона'), ('box', C['power'], 'энергоблок'), ('box', C['it'], 'ИТ: серверы, пульты'),
                       ('box', C['store'], 'склад, депо'), ('box', C['office'], 'гости, офис')])
    im.save(os.path.join(OUT, 'factory_floor1.png'), optimize=True)

def upper():
    im, d = canvas('Демо-завод FCW: 2 этаж (пол y = 5, y = 6…10): смотровая галерея', 'Серое = проём над цехом (виден сверху через стекло). Эркеры выступают в цех на 2 блока, монорельс идёт прямо перед стеклом.')
    box(d, 0, 0, L - 1, D - 1, C['floor'])
    box(d, 1, 12, 101, 33, C['void'])
    box(d, 1, 1, 101, 10, C['office']); box(d, 1, 35, 101, 45, '#E1E3E8')
    grid(d)
    for z in ZW: box(d, 0, z, L - 1, z, C['wall'])
    for x in (0, 102): box(d, x, 0, x, D - 1, C['wall'])
    for x in XW[1:-1]: box(d, x, 12, x, 33, C['wall']); box(d, x, 35, x, 45, C['wall'])
    box(d, 1, 11, 101, 11, C['glass']); box(d, 1, 0, 101, 0, C['glass'])
    for x in XW[1:-1]: box(d, x, 11, x, 11, C['wall'])
    # machine outlines seen from above
    for (x0, z0, w, dz, n) in ((16, 18, 8, 5, 'печь'), (31, 19, 5, 4, 'рост 1'), (37, 19, 5, 4, 'рост 2'), (46, 19, 8, 3, 'резка'), (63, 18, 13, 5, 'литограф EUV'), (85, 19, 14, 3, 'линия электроники')):
        box(d, x0, z0, x0 + w - 1, z0 + dz - 1, '#DDE0E6', C['dim']); label(d, x0 + w / 2, z0 + dz / 2, n, 8, C['dim'])
    poly(d, [(4, 15), (100, 15), (100, 31), (4, 31), (4, 15)], C['mono'], 3)
    # glazed bays (2 blocks deep) hanging into the halls; the monorail passes right in front of them
    for (x0, x1) in ((17, 24), (33, 40), (64, 75), (87, 96)):
        box(d, x0, 11, x1, 13, C['office']); box(d, x0, 14, x1, 14, C['glass']); box(d, x0 - 1, 12, x0 - 1, 14, C['glass']); box(d, x1 + 1, 12, x1 + 1, 14, C['glass'])
        label(d, (x0 + x1 + 1) / 2, 12.6, 'эркер', 8, C['dim'])
    for (x, t) in ((6, 'G1\nсырьё'), (20, 'G2\nплавка:\nпечь сверху'), (36, 'G3\nрост\nкристалла'), (50, 'G4\nпластины'), (68, 'G5\nчистая комната:\nлитограф и OHT'), (92, 'G6\nчипы и платы')):
        label(d, x, 5.5, t, 9, bold=True)
    box(d, 19, 8, 21, 10, '#B9BDC7', C['ink']); label(d, 20, 9.5, 'лестн.', 7)
    box(d, 95, 8, 97, 10, '#B9BDC7', C['ink']); label(d, 96, 9.5, 'лестн.', 7)
    box(d, 53, 2, 55, 4, '#B9BDC7', C['ink']); label(d, 54.5, 3.5, 'на крышу', 7)
    label(d, 50, 40.5, 'Техэтаж: кабельные лотки, вентиляция, водоподготовка (задел под жидкости); служебный мост к крыше', 9, C['dim'])
    poly(d, [(20, 9), (20, 7.5), (4, 7.5), (4, 9.6), (19, 9.6), (19, 12.5), (23, 12.5), (23, 9.6), (35, 9.6), (35, 12.5), (39, 12.5), (39, 9.6), (66, 9.6), (66, 12.5), (74, 12.5), (74, 9.6), (89, 9.6), (89, 12.5), (95, 12.5), (95, 9.6), (96, 9.6)], C['tour'], 2, dash=8)
    y = MY + D * S + 14
    legend(d, y, [('dash', C['tour'], 'маршрут экскурсии'), ('line', C['mono'], 'монорельс под галереей (y = 8)'), ('box', C['glass'], 'стекло от пола до потолка'), ('box', C['void'], 'проём над цехом')])
    im.save(os.path.join(OUT, 'factory_floor2.png'), optimize=True)

def roof():
    im, d = canvas('Демо-завод FCW: крыша (y = 11…16): солнечное поле', 'Панели 1×1 рядами с проходами; 4 трекера 3×3 над чистой комнатой; труба печи и вытяжки выходят сквозь кровлю.')
    box(d, 0, 0, L - 1, D - 1, '#C9CDD6'); grid(d)
    box(d, 0, 0, L - 1, D - 1, None, C['wall'], 3)
    for r in range(4):                       # poly rows over the north band, mono over hot shops, hjt over east
        for x in range(2, 54): box(d, x, 2 + r * 3, x, 3 + r * 3 - 1 + 1 - 1, '#35507E' if x < 28 else '#1A1D26')
    for r in range(3):
        for x in range(2, 54):
            if 15 <= x <= 25 and r < 2: continue
            box(d, x, 36 + r * 3, x, 36 + r * 3 + 1, '#1A1D26')
    for r in range(8):
        for x in range(83, 101): box(d, x, 2 + r * 3, x, 3 + r * 3, '#0C0D12', '#8B6CF0')
    for (x, z) in ((60, 4), (70, 4), (60, 14), (70, 14)):
        box(d, x, z, x + 2, z + 2, '#0C0D12', '#8B6CF0', 2); label(d, x + 1.5, z + 4.2, 'трекер', 8, C['dim'])
    box(d, 22, 21, 23, 22, '#FFFFFF', C['ink'], 2); label(d, 23, 24.5, 'труба печи', 8)
    box(d, 12, 14, 28, 33, None, C['dim']); label(d, 20, 16, 'фонарь плавильного цеха\n(стекло, y = 11)', 8, C['dim'])
    box(d, 57, 24, 80, 33, '#B9BDC7', C['ink']); label(d, 68.5, 28.5, 'чиллеры и вентустановки\nчистой комнаты (ФВУ)', 9)
    box(d, 53, 2, 55, 4, '#FFFFFF', C['ink']); label(d, 54.5, 5.6, 'выход', 7)
    label(d, 27, 0.8, 'поликремний T1 (x 2…27)  ·  монокристалл T2 (x 28…53)', 8, C['ink']); label(d, 92, 0.8, 'высокоэффективные T3', 8, C['ink'])
    box(d, 30, 14, 53, 33, None, C['dim']); label(d, 41.5, 23.5, 'смотровая площадка,\nлавки, макет панели в разрезе', 8, C['dim'])
    poly(d, [(54.5, 5), (54.5, 12), (41, 12), (41, 20)], C['tour'], 2, dash=8)
    poly(d, [(54, 8), (56, 8), (56, 40), (54, 40)], C['cable'], 2); poly(d, [(56, 20), (83, 20)], C['cable'], 2); poly(d, [(56, 40), (36, 40), (36, 44)], C['cable'], 2)
    label(d, 36, 45.5, '↓ в подстанцию', 8, C['cable'])
    y = MY + D * S + 14
    legend(d, y, [('box', '#35507E', 'T1 поликремний'), ('box', '#1A1D26', 'T2 монокристалл'), ('box', '#0C0D12', 'T3 высокоэффективные и трекеры'), ('line', C['cable'], 'сборные кабели'), ('dash', C['tour'], 'экскурсия')])
    im.save(os.path.join(OUT, 'factory_roof.png'), optimize=True)

def sections():
    H = 18
    im = Image.new('RGB', (MX * 2 + L * S, 2 * (H * S + 90) + 20), C['bg']); d = ImageDraw.Draw(im)
    def sec(y0, title, sub):
        d.text((MX, y0), title, font=FB(16), fill=C['ink']); d.text((MX, y0 + 22), sub, font=F(11), fill=C['dim'])
        base = y0 + 50 + H * S
        def b(x0, ya, x1, yb, fill=None, outline=None, width=1):
            d.rectangle([MX + x0 * S, base - (yb + 1) * S, MX + (x1 + 1) * S - 1, base - ya * S - 1], fill=fill, outline=outline, width=width)
        def t(x, y, s, size=9, fill=None, bold=False): d.multiline_text((MX + x * S, base - y * S), s, font=(FB if bold else F)(size), fill=fill or C['ink'], anchor='mm', align='center')
        return b, t, base
    # --- longitudinal section through the process band (z = 20), looking north
    b, t, base = sec(10, 'Разрез А–А вдоль цехов (z = 20), взгляд на север', 'Пол y = 0, чистый потолок цехов y = 10, кровля y = 11, монорельс y = 8. За машинами видна стеклянная стена галереи (y = 6…10).')
    b(0, 0, L - 1, 0, C['wall']); b(0, 11, L - 1, 11, C['wall'])
    for (x0, x1, c) in ((1, 10, 'store'), (12, 28, 'hot'), (30, 42, 'hot'), (44, 55, 'floor'), (57, 80, 'clean'), (82, 101, 'floor')): b(x0, 1, x1, 10, C[c])
    b(1, 6, 101, 10, None, C['glass'], 2)
    for x in XW: b(x, 0, x, 11, C['wall'])
    b(57, 1, 80, 1, '#B9BDC7'); t(69, 1.5, 'фальшпол с перфорацией', 7, C['dim'])
    b(57, 10, 80, 10, '#FFFFFF', C['dim']); t(69, 10.5, 'потолок из ФВУ (светится)', 7, C['dim'])
    def m(x0, w, h, n, y=1): b(x0, y, x0 + w - 1, y + h - 1, C['mach'], C['machedge'], 2); t(x0 + w / 2, y + h / 2, n, 9, bold=True)
    m(16, 8, 6, 'Дуговая печь'); b(22, 7, 23, 13, '#FFFFFF', C['ink']); m(31, 5, 7, 'Рост 1'); m(37, 5, 7, 'Рост 2'); m(46, 8, 3, 'Резка'); m(63, 13, 6, 'Литограф EUV', 2); m(85, 14, 3, 'Линия электроники')
    d.line([MX + 4 * S, base - 8.5 * S, MX + 100 * S, base - 8.5 * S], fill=C['mono'], width=3)
    for x in (4, 14, 33.5, 39.5, 50.5, 60, 84, 100): d.rectangle([MX + x * S - 5, base - 8.4 * S, MX + x * S + 5, base - 7.3 * S], fill=C['mono'], outline=C['ink'])
    for x in range(2, 54, 2): b(x, 12, x, 12, C['solar'])
    for x in range(83, 101, 2): b(x, 12, x, 12, '#0C0D12')
    for x in (60, 70): b(x, 12, x + 2, 15, None, C['ink']); b(x, 15, x + 2, 15, '#0C0D12')
    b(57, 12, 80, 13, '#B9BDC7', C['ink'])
    for (x, s) in ((6, 'A'), (20, 'B'), (36, 'C'), (50, 'D'), (60, 'E'), (92, 'F')): t(x, 9.5, s, 11, C['dim'], True)
    for y in (0, 5, 10, 15): d.text((MX - 20, base - y * S - 6), 'y%d' % y, font=F(9), fill=C['dim'])
    # --- cross section through the cleanroom (x = 69), looking east
    y0 = 10 + H * S + 100
    b, t, base = sec(y0, 'Разрез Б–Б поперёк здания через чистую комнату (x = 69), взгляд на восток', 'Слева север: шлюз и участок масок, над ними галерея; в центре чистая комната с литографом; справа серверный зал и техэтаж.')
    b(0, 0, D - 1, 0, C['wall']); b(0, 11, D - 1, 11, C['wall'])
    b(1, 1, 10, 4, C['clean']); b(1, 5, 10, 5, C['wall']); b(1, 6, 10, 10, C['office']); b(12, 1, 33, 10, C['clean']); b(35, 1, 45, 4, C['it']); b(35, 5, 45, 5, C['wall']); b(35, 6, 45, 10, '#E1E3E8')
    for z in ZW: b(z, 0, z, 11, C['wall'])
    b(11, 6, 11, 10, C['glass']); b(0, 6, 0, 10, C['glass'])
    b(12, 1, 33, 1, '#B9BDC7'); b(12, 10, 33, 10, '#FFFFFF', C['dim'])
    b(18, 2, 22, 7, C['mach'], C['machedge'], 2); t(20.5, 5, 'Литограф\n(глубина 5)', 9, bold=True)
    d.rectangle([MX + 15 * S - 5, base - 8.9 * S, MX + 15 * S + 5, base - 7.6 * S], fill=C['mono'], outline=C['ink']); t(15, 9.5, 'OHT', 7, C['mono'])
    d.rectangle([MX + 31 * S - 5, base - 8.9 * S, MX + 31 * S + 5, base - 7.6 * S], fill=C['mono'], outline=C['ink'])
    t(5.5, 8, 'галерея G5', 9, bold=True); t(5.5, 2.8, 'шлюз /\nмаски', 8); t(40, 2.8, 'серверный зал', 8); t(40, 8, 'техэтаж', 8, C['dim'])
    for z in range(2, 46, 3): b(z, 12, z, 12, C['solar'])
    b(24, 12, 33, 13, '#B9BDC7', C['ink']); t(29, 13, 'ФВУ', 8)
    d.line([MX + 23 * S, base - 1.5 * S, MX + 23 * S, base - 0.5 * S, MX + 35 * S, base - 0.5 * S], fill=C['cable'], width=2)
    t(60, 9, 'Высоты:\nпол цехов y = 0 (в чистой комнате фальшпол y = 1)\nгалерея: пол y = 5, потолок y = 10\nмонорельс: низ балки y = 9, тележка y = 7…8\nкровля y = 11, панели y = 12, трекеры до y = 16', 10)
    for y in (0, 5, 10, 15): d.text((MX - 20, base - y * S - 6), 'y%d' % y, font=F(9), fill=C['dim'])
    im.save(os.path.join(OUT, 'factory_sections.png'), optimize=True)

ground(); upper(); roof(); sections()
