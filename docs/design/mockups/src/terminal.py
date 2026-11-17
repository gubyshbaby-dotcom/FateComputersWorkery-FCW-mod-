"""Machine console mockups: plain 80x25 text screen, the mod's own Terminus 8x16 PSF font, monochrome + one accent.
Run: python3 terminal.py  -> ../terminal_*.png
     FCW_MC_JAR=<minecraft client jar> FCW_SHOT=<screenshot.png> python3 terminal.py   also builds ../hint_overlay.png"""
import os, sys, json, io, zipfile
from PIL import Image, ImageDraw, ImageFont
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
PSF = os.environ.get('FCW_TERM_PSF', os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..', '..',
                                                 'native', 'crates', 'fcw-client', 'assets', 'terminus-uni2-8x16.psf'))
BG, FG, ACC = '#0B0C0F', '#C9CDD6', '#A88BFF'
COLS, ROWS, CW, CH = 80, 25, 8, 16

def load_psf(path):
    """PSF1 font of the mod's own text screen (the same glyphs the client draws)"""
    b = open(path, 'rb').read(); assert b[0] == 0x36 and b[1] == 0x04
    mode, h = b[2], b[3]; n = 512 if mode & 1 else 256
    glyphs = [b[4 + i * h: 4 + (i + 1) * h] for i in range(n)]; table = {}
    p = 4 + n * h; i = 0
    while p + 1 < len(b) and i < n:
        u = b[p] | (b[p + 1] << 8); p += 2
        if u == 0xFFFF: i += 1
        elif u != 0xFFFE: table.setdefault(chr(u), glyphs[i])
    return table
_psf = load_psf(PSF)
class _D:
    def __init__(self, im): self.im = im
    def text(self, xy, s, font=None, fill=FG):
        x, y = xy; d = ImageDraw.Draw(self.im)
        for ch in s:
            g = _psf.get(ch)
            if g:
                for r, byte in enumerate(g):
                    for c in range(8):
                        if byte & (0x80 >> c): d.point((x + c, y + r), fill=fill)
            x += CW
    def rectangle(self, *a, **k): ImageDraw.Draw(self.im).rectangle(*a, **k)

def screen(lines, name, cursor=True):
    """lines: str or (str, accent_from_col, accent_to_col) -> the accent colour for that span only"""
    f = None
    im = Image.new('RGB', (COLS * CW + 16, ROWS * CH + 16), '#000000'); d = _D(im)
    d.rectangle([4, 4, im.width - 5, im.height - 5], fill=BG)
    assert len(lines) <= ROWS, len(lines)
    for r, ln in enumerate(lines):
        s, a, b = (ln, 0, 0) if isinstance(ln, str) else ln
        assert len(s) <= COLS, s
        d.text((8, 8 + r * CH), s, font=f, fill=FG)
        if b > a: d.rectangle([8 + a * CW, 8 + r * CH, 8 + b * CW - 1, 8 + (r + 1) * CH - 1], fill=BG); d.text((8 + a * CW, 8 + r * CH), s[a:b], font=f, fill=ACC)
    if cursor:
        last = lines[-1] if isinstance(lines[-1], str) else lines[-1][0]
        x = 8 + len(last) * CW; y = 8 + (len(lines) - 1) * CH
        d.rectangle([x, y + 2, x + CW - 1, y + CH - 2], fill=FG)
    im.save(os.path.join(OUT, name), optimize=True)

def acc(s, word):
    i = s.index(word); return (s, i, i + len(word))

screen([
    'FCW machine console   euv-1: литограф EUV Mk2',
    "Введите 'help' для списка команд.",
    '',
    'euv-1> status',
    acc('состояние  работа: экспозиция, поле 24/61', 'работа'),
    'рецепт     wafer_cpu  468/1200 t  2048 FE/t',
    'маска      reticle_cpu  ресурс 187/256',
    'загрузка   dock1: silicon_wafer x6   tin: tin_ingot x16',
    'выход      dock2: wafer_cpu x2',
    'питание    312k/400k FE  приход 2048 FE/t',
    '',
    'euv-1> stop',
    'останов после текущей пластины... ожидание',
    '',
    'euv-1> recipe',
    'wafer_cpu   silicon_wafer + tin_ingot -> wafer_cpu   1200 t  2048 FE/t',
    'выбор: auto (по маске)',
    '',
    'euv-1> log 3',
    '14:18:41  ожидание: нет пластин',
    '14:19:52  пуск wafer_cpu',
    '14:20:31  останов оператором',
    '',
    'euv-1> start',
    'euv-1> ',
], 'terminal_litho.png')

screen([
    'FCW machine console   eaf-1: дуговая печь',
    "Введите 'help' для списка команд.",
    '',
    'eaf-1> start',
    acc('отказ: нет питания (приход 0 из 256 FE/t)', 'отказ'),
    '',
    'eaf-1> status',
    acc('состояние  ошибка: нет питания', 'ошибка'),
    'рецепт     metallurgical_silicon  0/400 t  256 FE/t',
    'загрузка   charge: quartz x28, coal x14',
    'выход      tap: пусто',
    'питание    1.2k/400k FE  приход 0 FE/t',
    '',
    'eaf-1> help',
    'status          состояние, рецепт, загрузка, выход, питание',
    'start | stop    пуск и останов (то же, что рубильник)',
    'recipe [имя|auto]   показать или закрепить рецепт',
    'log [n]         последние события',
    'name [имя]      имя машины для fcw machine',
    'exit            закрыть консоль',
    '',
    'eaf-1> ',
], 'terminal_furnace_fault.png')

# ---- in-world hint montage over a real screenshot
shot = os.environ.get('FCW_SHOT'); jar = os.environ.get('FCW_MC_JAR'); assets = os.environ.get('FCW_MC_ASSETS')
if shot and (jar or assets):
    rd = (lambda p: open(os.path.join(assets, p), 'rb').read()) if assets else (lambda p, z=zipfile.ZipFile(jar): z.read('assets/minecraft/' + p))
    glyphs = {}
    for p in json.loads(rd('font/include/default.json'))['providers']:
        if p['type'] != 'bitmap': continue
        g = Image.open(io.BytesIO(rd('textures/font/' + p['file'].split('/')[-1]))).convert('RGBA'); rows = p['chars']
        cw, ch = g.width // len(rows[0]), g.height // len(rows); yo = 7 - p['ascent']
        for r, row in enumerate(rows):
            for c, k in enumerate(row):
                if k in glyphs or k in ' \x00': continue
                pts = [(x, y + yo) for y in range(ch) for x in range(cw) if g.getpixel((c * cw + x, r * ch + y))[3]]
                if pts: glyphs[k] = pts
    def mc_text(im, x, y, s, col, scale=2):
        d = ImageDraw.Draw(im)
        def run(ox, oy, c):
            cx = ox
            for k in s:
                pts = glyphs.get(k)
                if pts:
                    for gx, gy in pts: d.rectangle([cx + gx * scale, oy + gy * scale, cx + gx * scale + scale - 1, oy + gy * scale + scale - 1], fill=c)
                    cx += (max(q[0] for q in pts) + 2) * scale
                else: cx += 4 * scale
            return cx
        run(x + scale, y + scale, '#3F3F3F'); return run(x, y, col)
    def mc_width(s, scale=2): return sum(((max(q[0] for q in glyphs[k]) + 2) if k in glyphs else 4) for k in s) * scale
    src = Image.open(shot).convert('RGB')                      # 854x480, GUI scale 2
    cx, cy = 362, 221                                          # the tin tray on the scanner's front
    view = src.crop((cx - 320, cy - 180, cx + 320, cy + 180)).resize((854, 480), Image.NEAREST)
    hot = src.crop((240, 430, 614, 480)); view.paste(hot, (240, 430))
    d = ImageDraw.Draw(view)
    d.rectangle([500, 252, 528, 280], fill=view.getpixel((496, 248)))               # old crosshair
    g = view.getpixel((150, 424)); d.rectangle([0, 426, 239, 479], fill=g); d.rectangle([614, 426, 742, 479], fill=g)   # chat lines of the test run
    x0, y0, x1, y1 = 398, 208, 456, 268                        # port zone outline, vanilla selection style
    d.rectangle([x0, y0, x1, y1], outline='#FFFFFF', width=2)
    d.line([427 - 9, 240, 427 + 9, 240], fill='#FFFFFF', width=2); d.line([427, 240 - 9, 427, 240 + 9], fill='#FFFFFF', width=2)
    s = 'Лоток олова: 16/64  ·  ПКМ: положить'
    mc_text(view, (854 - mc_width(s)) // 2, 396, s, '#FFFFFF')
    view.save(os.path.join(OUT, 'hint_overlay.png'), optimize=True)
