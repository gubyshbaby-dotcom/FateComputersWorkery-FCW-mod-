"""Schematic concept sheets for the artist brief: energy blocks and automation (front/top elevations, 32 px per block).
These are diagrams of proportions, colours and indicator placement, not models. Run: python3 concepts.py"""
import os, random
from PIL import Image, ImageDraw, ImageFont
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
B = 32
F = lambda n: ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', n)
FB = lambda n: ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf', n)
W, K, G, GL, P, PD, CU, AL, ST, OKC, WARN, ERR, CY, MG = ('#E7E9ED', '#1B1C21', '#BCC0C9', '#06070B', '#A88BFF', '#5B43B8', '#E08A45', '#C9CEDA', '#3A3D48',
                                                    '#58D68D', '#F2B33D', '#FF5A52', '#22D3EE', '#E040C8')
class Sheet:
    def __init__(self, w, h, title, sub):
        self.px = Image.new('RGB', (w, h), '#D9DDE4'); self.d = ImageDraw.Draw(self.px); self.items = []; self.w, self.h = w, h
        self.title, self.sub = title, sub
    def r(self, x, y, w, h, c, o=None):
        if w > 0 and h > 0: self.d.rectangle([x, y, x + w - 1, y + h - 1], fill=c, outline=o)
    def cap(self, x, y, t): self.items.append((x, y, t))
    def panel(self, x, y, w, h, seams=True, line=P):
        """white Mk2 housing with panel seams, state light line on top and glowing skirt at the bottom"""
        self.r(x, y, w, h, W); self.r(x, y, w, 1, '#FFFFFF'); self.r(x + w - 1, y, 1, h, G)
        if seams:
            for sx in range(B, w, B): self.r(x + sx, y, 1, h, G)
            for sy in range(B, h, B): self.r(x, y + sy, w, 1, G)
        self.r(x, y + h - 4, w, 2, K); self.r(x, y + h - 2, w, 2, line)
    def save(self, name, scale=2):
        im = self.px.resize((self.w * scale, self.h * scale), Image.NEAREST)
        out = Image.new('RGB', (im.width, im.height + 64), '#F4F5F7'); out.paste(im, (0, 64)); d = ImageDraw.Draw(out)
        d.text((16, 10), self.title, font=FB(18), fill='#1E2026'); d.text((16, 36), self.sub, font=F(11), fill='#6D7280')
        for x, y, t in self.items: d.multiline_text((x * scale, 64 + y * scale), t, font=F(11), fill='#1E2026', spacing=2)
        out.save(os.path.join(OUT, name), optimize=True)

def solar_top(s, x, y, tier, n=3, m=2):
    rnd = random.Random(tier)
    for j in range(m):
        for i in range(n):
            px, py = x + i * B, y + j * B
            s.r(px, py, B, B, AL); s.r(px + 1, py + 1, B - 2, B - 2, K)
            for cy in range(4):
                for cx in range(4):
                    qx, qy = px + 2 + cx * 7, py + 2 + cy * 7
                    if tier == 1:
                        s.r(qx, qy, 6, 6, '#2F4F86')
                        for _ in range(5): s.r(qx + rnd.randrange(6), qy + rnd.randrange(6), 1, 1, rnd.choice(('#4E74B8', '#27406E', '#6B8FD0')))
                    elif tier == 2:
                        s.r(qx, qy, 6, 6, '#141821')
                        for (a, b) in ((0, 0), (5, 0), (0, 5), (5, 5)): s.r(qx + a, qy + b, 1, 1, '#8E96AA')
                        s.r(qx, qy + 3, 6, 1, '#2A3140')
                    else:
                        s.r(qx, qy, 6, 6, '#07080C'); s.r(qx, qy, 6, 1, '#12141C')

def energy():
    s = Sheet(730, 520, 'Энергоблоки: схемы пропорций и индикации (не модели)', '32 px = 1 блок (2 текселя на 1/16). Белый корпус, чёрное стекло, чёрные рамы, медь только на шинах. Светятся только линии состояния и лампы.')
    # solar fields
    for i, (t, n) in enumerate(((1, 'T1 поликремний: синие ячейки с «искрой»'), (2, 'T2 монокристалл: чёрные ячейки, срезанные углы'), (3, 'T3 высокоэффективная: сплошная\nчёрная, чёрная рама'))):
        solar_top(s, 16 + i * 112, 16, t); s.cap(16 + i * 112, 84, n.replace(': ', ':\n'))
    # side view tilted row
    x, y = 372, 30
    for i in range(3):
        px = x + i * B
        s.d.polygon([(px + 2, y + 44), (px + 30, y + 30), (px + 30, y + 33), (px + 2, y + 47)], fill=K)
        s.d.line([(px + 2, y + 43), (px + 30, y + 29)], fill='#35507E', width=2)
        s.r(px + 6, y + 46, 2, 6, K); s.r(px + 24, y + 34, 2, 18, K); s.r(px + 4, y + 52, 24, 2, AL)
    s.r(x, y + 54, 3 * B, 2, ST); s.cap(x, 92, 'вид сбоку, tilt=tilted:\nнаклон 22.5°, общая рейка,\nвысота 12/16')
    # tracker 3x3 on mast
    x, y = 560, 8
    s.r(x + 44, y + 36, 8, 56, K); s.r(x + 34, y + 88, 28, 8, W); s.r(x + 34, y + 94, 28, 2, P)
    s.d.polygon([(x, y + 44), (x + 96, y + 12), (x + 96, y + 18), (x, y + 50)], fill='#07080C'); s.d.line([(x, y + 43), (x + 96, y + 11)], fill=AL, width=1)
    s.r(x + 40, y + 28, 16, 12, W, K); s.cap(x - 6, 112, 'трекер 3×4×3: мачта,\nстол 3×3 панели T3,\nдве оси (азимут, наклон)')
    # fuel generator
    x, y = 16, 220
    s.r(x, y, B, B, ST); s.r(x, y, B, 2, '#565A68'); s.r(x + 3, y + 12, 16, 14, K); 
    for k in range(4): s.r(x + 5, y + 14 + k * 3, 12, 2, CU if k % 2 == 0 else WARN)
    s.r(x + 22, y + 6, 7, 7, GL, K); s.r(x + 24, y + 9, 3, 1, OKC); s.r(x + 22, y + 18, 7, 8, K); s.r(x + 24, y + 20, 3, 4, CU)
    s.r(x + 8, y - 10, 8, 10, K); s.r(x + 6, y - 12, 12, 3, ST); s.r(x, y + B - 2, B, 2, K)
    s.cap(x - 4, y + 38, 'топливный генератор\n1×1×1 (+ труба 6/16):\nтёмная сталь, топка,\nманометр, медный вывод')
    # turbine genset 3x2
    x = 96
    s.panel(x, y - B, 3 * B, 2 * B); s.r(x + 6, y - 24, 52, 30, GL); s.r(x + 10, y - 20, 44, 22, '#12141C')
    for k in range(6): s.r(x + 14 + k * 7, y - 18, 4, 18, ST)
    s.r(x + 66, y - 26, 24, 24, K); s.d.ellipse([x + 69, y - 23, x + 86, y - 6], fill=ST, outline=AL); s.r(x + 70, y + 6, 20, 3, P)
    s.r(x + 8, y - B - 12, 12, 12, AL); s.r(x + 6, y - B - 14, 16, 3, K); s.r(x + 80, y + 14, 10, 8, K); s.r(x + 82, y + 16, 6, 4, CU)
    s.cap(x, y + 38, 'турбогенератор 3×2×2 (Mk2):\nокно на турбину, вентилятор,\nвыхлоп, FE-вывод сзади')
    # pad transformer 1x2
    x = 216
    s.panel(x, y - B, B, 2 * B)
    for k in range(5): s.r(x + 4 + k * 5, y - 6, 3, 26, K)
    s.r(x + 6, y - 26, 20, 12, GL); s.r(x + 9, y - 22, 6, 1, P); s.r(x + 9, y - 19, 10, 1, W)
    for k in range(3): s.r(x + 5 + k * 9, y - B - 10, 4, 10, AL); s.r(x + 4 + k * 9, y - B - 12, 6, 3, CU)
    s.cap(x - 4, y + 38, 'трансформатор\n1×2×1: рёбра,\n3 ввода ВН сверху')
    # substation 4x3
    x = 290
    s.r(x, y - 2 * B, 4 * B, 3 * B, None); s.r(x, y + B - 4, 4 * B, 2, K); s.r(x, y + B - 2, 4 * B, 2, P)
    s.r(x + 2, y - 2 * B, 4, 3 * B - 4, K); s.r(x + 4 * B - 6, y - 2 * B, 4, 3 * B - 4, K); s.r(x + 2, y - 2 * B, 4 * B - 4, 4, K)
    for k in range(3):
        ix = x + 20 + k * 18
        for j in range(4): s.r(ix - (j % 2), y - 2 * B + 4 + j * 4, 6 + 2 * (j % 2), 3, AL)
        s.r(ix + 2, y - 2 * B + 20, 2, 16, CU)
    s.r(x + 12, y - 28, 60, 54, W); s.r(x + 12, y - 28, 60, 1, '#FFFFFF'); s.r(x + 44, y - 28, 1, 54, G)
    for k in range(8): s.r(x + 14 + k * 7, y - 6, 4, 28, K)
    s.r(x + 18, y - 24, 22, 12, GL); s.r(x + 21, y - 20, 8, 1, P); s.r(x + 21, y - 17, 14, 1, W); s.r(x + 50, y - 22, 16, 3, P)
    s.r(x + 80, y - 20, 36, 46, W); s.r(x + 84, y - 16, 28, 24, GL); s.r(x + 87, y - 12, 10, 1, P); s.r(x + 87, y - 8, 18, 1, W); s.r(x + 87, y - 4, 14, 1, OKC)
    s.r(x + 86, y + 12, 6, 10, K); s.r(x + 88, y + 13, 2, 4, P); s.r(x + 72, y - 10, 8, 3, CU); s.r(x + 100, y + 14, 12, 8, K); s.r(x + 102, y + 16, 8, 4, CU)
    s.cap(x, y + 38, 'подстанция 4×3×3 (Mk2):\nпортал с изоляторами ВН,\nбак с радиаторами, шкаф РУ,\nмедные шины')
    # hv pylon
    x = 450
    s.r(x + 13, y - 2 * B, 6, 3 * B - 2, K); s.r(x, y - 2 * B + 6, B, 4, K)
    for ix in (x + 1, x + 25):
        for j in range(3): s.r(ix - (j % 2), y - 2 * B + 10 + j * 4, 6 + 2 * (j % 2), 3, AL)
    s.r(x + 8, y + B - 4, 16, 4, W); s.d.line([(x + 4, y - 2 * B + 22), (x + 70, y - 2 * B + 34)], fill=K, width=1); s.d.line([(x + 28, y - 2 * B + 22), (x + 94, y - 2 * B + 34)], fill=K, width=1)
    s.cap(x - 2, y + 38, 'опора ВН 1×3×1:\nпровода рисует\nрендерер (провис)')
    # UPS home
    x = 566
    s.r(x + 8, y + 4, 16, 28, '#26282E'); s.r(x + 8, y + 4, 16, 1, '#3C3F49'); s.r(x + 11, y + 8, 10, 6, GL)
    for k in range(4): s.r(x + 11, y + 26 - k * 3, 10, 2, OKC if k < 3 else '#15161A')
    s.r(x + 12, y + 10, 4, 1, W); s.cap(x - 6, y + 38, 'ИБП бытовой 1×1:\nтёмный, шкала заряда\nиз 4 сегментов')
    # battery cabinets 3 in a row 1x2
    x = 650
    for i in range(2):
        cx = x + i * B
        s.panel(cx, y - B, B, 2 * B, seams=False); s.r(cx + 4, y - B + 6, 24, 48, GL)
        for k in range(4):
            s.r(cx + 6, y - B + 9 + k * 11, 20, 9, '#12141C'); s.r(cx + 8, y - B + 15 + k * 11, 12 if (i, k) != (1, 0) else 5, 2, OKC if (i, k) != (1, 0) else WARN)
            s.r(cx + 8, y - B + 11 + k * 11, 3, 2, W)
        s.r(cx + 1, y - B + 2, 1, 56, P)
    s.cap(x - 8, y + 38, 'шкаф ИБП 1×2×1,\nстыкуется в ряд:\n4 модуля за стеклом')
    # cables
    x, y = 16, 370
    for i, (dia, name, stripe) in enumerate(((8, 'медный 512 FE/t\nØ 4/16, белая метка', W), (12, 'силовой 2048 FE/t\nØ 6/16, фиолетовая', P), (16, 'шина 8192 FE/t\nØ 8/16, медные кольца', CU))):
        cx = x + i * 150; cy = y + 40
        s.r(cx, cy - dia // 2, 100, dia, K); s.r(cx, cy - dia // 2, 100, 1, ST)
        for k in range(12, 100, 32): s.r(cx + k, cy - dia // 2, 2, dia, stripe)
        s.r(cx + 100, cy - dia // 2 - 2, dia + 4, dia + 4, ST); s.r(cx + 102, cy - dia // 2, dia, dia, K)       # core / junction
        s.r(cx + 102, cy - 40, dia, 40 - dia // 2, K); s.r(cx + 102 , cy - 40, dia, 3, CU)
        s.cap(cx, y + 60, name)
    cx = x + 470
    s.r(cx, y + 20, 3 * B, B, ST)
    for i in range(3):
        s.r(cx + i * B, y + 20, 1, B, K)
        for k in range(4): s.r(cx + i * B + 5, y + 26 + k * 6, 22, 2, K)
        s.r(cx + i * B + 5, y + 35, 22, 1, CU)
    s.cap(cx, y + 60, 'кабельный канал в полу (вид сверху):\nполный блок, рифлёная крышка,\nв прорезях виден кабель, не светится')
    s.cap(16, y + 96, 'Кабель: multipart. Прямой участок без утолщения; ядро-муфта (на 2/16 толще) появляется на поворотах и отводах;\nв торце, входящем в порт машины, видно медное кольцо-наконечник. Рядом со стеной добавляется скоба (косметика).')
    s.save('concept_energy.png')

def automation():
    s = Sheet(700, 420, 'Автоматика: монорельс и манипуляторы, схемы (не модели)', '32 px = 1 блок. Балка белая, подвесы и рамы чёрные, световая линия по низу балки; тележка белая с чёрным стеклом, груз виден.')
    # side elevation: ceiling, hangers, beam, carrier, station with hoist over a machine port
    y = 20
    s.r(10, y, 440, 6, ST)
    for hx in range(26, 440, 4 * B): s.r(hx, y + 6, 4, 14, K)
    s.r(10, y + 20, 440, 10, W); s.r(10, y + 20, 440, 1, '#FFFFFF'); s.r(10, y + 30, 440, 2, P)
    def carrier(cx, lowered=0, load=True):
        s.r(cx + 4, y + 32, 6, 4, K); s.r(cx + 22, y + 32, 6, 4, K)
        s.r(cx, y + 36, B, 14, W); s.r(cx + 3, y + 39, 26, 6, GL); s.r(cx + 5, y + 41, 6, 1, P); s.r(cx, y + 48, B, 2, K)
        if lowered: s.r(cx + 6, y + 50, 1, lowered, AL); s.r(cx + 25, y + 50, 1, lowered, AL)
        if load:
            py = y + 50 + lowered
            s.r(cx + 4, py, 24, 4, K); s.r(cx + 6, py + 4, 20, 16, W, K); s.r(cx + 9, py + 7, 14, 10, GL)
            for k in range(4): s.r(cx + 10, py + 8 + k * 2, 12, 1, '#8E96AA')
    carrier(70); carrier(330, lowered=42)
    # station frame + machine with port
    s.r(318, y + 32, 3, 12, K); s.r(371, y + 32, 3, 12, K); s.r(318, y + 14, 56, 4, P)
    s.panel(300, y + 136, 3 * B, 2 * B - 10); s.r(322, y + 132, 48, 6, K); s.r(326, y + 130, 40, 3, P)
    s.cap(300, y + 200, 'станция над портом машины:\nподъёмник опускает капсулу на порт')
    s.cap(40, y + 96, 'тележка с капсулой (FOUP, ящик, бак):\nкапсула = сменная модель груза')
    # rail pieces top view
    y2 = 150; x = 16
    def beam(x0, y0, x1, y1): s.d.line([(x0, y0), (x1, y1)], fill=W, width=8); s.d.line([(x0, y0), (x1, y1)], fill=P, width=2)
    beam(x, y2 + 16, x + 64, y2 + 16); s.cap(x, y2 + 40, 'прямой')
    s.d.arc([x + 80, y2 - 48, x + 80 + 128, y2 + 80], 90, 180, fill=W, width=8); s.d.arc([x + 83, y2 - 45, x + 80 + 125, y2 + 77], 90, 180, fill=P, width=2); s.cap(x + 84, y2 + 40, 'поворот R2\n(2×2 клетки)')
    tx, ty = x + 208, y2 + 28
    beam(tx - 44, ty, tx - 18, ty); beam(tx + 18, ty, tx + 44, ty); beam(tx, ty - 44, tx, ty - 18)
    s.d.ellipse([tx - 16, ty - 16, tx + 16, ty + 16], fill=K, outline=AL); beam(tx - 13, ty, tx + 13, ty)
    s.r(tx + 8, ty + 8, 4, 4, OKC); s.cap(x + 160, y2 + 62, 'поворотный круг 1×1:\nотрезок балки на круге,\nшаг 90°, огонь пути')
    # arms: three sizes, side view
    def arm(x, yb, base_h, l1, l2, name, hold=True):
        s.r(x, yb - 6, 28, 6, K); s.r(x, yb - 2, 28, 2, P); s.r(x + 6, yb - 6 - base_h, 16, base_h, W, K); s.r(x + 9, yb - base_h - 2, 10, 3, GL)
        sx, sy = x + 14, yb - 6 - base_h
        ex, ey = sx + int(l1 * 0.45), sy - int(l1 * 0.9)
        s.d.line([(sx, sy), (ex, ey)], fill=W, width=9); s.d.line([(sx + 2, sy), (ex + 2, ey)], fill=P, width=1)
        wx, wy = ex + int(l2 * 0.95), ey + int(l2 * 0.3)
        s.d.line([(ex, ey), (wx, wy)], fill=W, width=7)
        for (jx, jy, r) in ((sx, sy, 6), (ex, ey, 5), (wx, wy, 4)): s.d.ellipse([jx - r, jy - r, jx + r, jy + r], fill=K, outline=AL)
        s.r(wx - 1, wy + 3, 3, 8, K); s.r(wx - 7, wy + 10, 15, 3, K); s.r(wx - 7, wy + 12, 2, 5, K); s.r(wx + 6, wy + 12, 2, 5, K)
        if hold: s.r(wx - 5, wy + 14, 11, 2, '#8E96AA')
        s.cap(x - 6, yb + 6, name)
    arm(300, 330, 10, 26, 24, 'S: база 1×1,\nвылет 2')
    arm(390, 330, 18, 44, 40, 'M: база 1×1,\nвылет 4')
    arm(510, 330, 26, 66, 58, 'L: база 2×2, вылет 6,\nберёт слиток и кассету')
    # arm on rail
    s.r(16, 300, 230, 6, W); s.r(16, 306, 230, 2, P); s.r(16, 294, 230, 6, K)
    s.r(90, 284, 36, 12, K)
    sx, sy = 108, 284; ex, ey = sx + 16, sy - 34; wx, wy = ex + 34, ey + 10
    s.d.line([(sx, sy), (ex, ey)], fill=W, width=9); s.d.line([(ex, ey), (wx, wy)], fill=W, width=7)
    for (jx, jy, r) in ((sx, sy, 6), (ex, ey, 5), (wx, wy, 4)): s.d.ellipse([jx - r, jy - r, jx + r, jy + r], fill=K, outline=AL)
    s.r(wx - 1, wy + 3, 3, 8, K); s.r(wx - 7, wy + 10, 15, 3, K)
    s.cap(16, 336, 'M на напольном рельсе (каретка 1×1):\nобслуживает ряд портов')
    s.save('concept_automation.png')

energy(); automation()
