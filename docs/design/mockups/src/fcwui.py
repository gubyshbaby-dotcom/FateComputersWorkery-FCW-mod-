"""FCW GUI mockup toolkit (PIL). Draws machine-terminal GUIs pixel-for-pixel at Minecraft GUI scale 1.

Text uses the real vanilla bitmap font, read from a Minecraft client jar or an extracted assets folder:
    FCW_MC_ASSETS=/path/with/assets/minecraft  (or)  FCW_MC_JAR=/path/minecraft_1.21.1_client.jar
Mojang's font textures are not stored in this repository.
"""
import json, os, zipfile, io
from PIL import Image, ImageDraw

# ---------------------------------------------------------------- tokens
def rgb(h):
    h = h.lstrip('#'); return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4)) + (255,)

T = {k: rgb(v) for k, v in {
    # chassis, skin "mk2" (white housing)
    'out': '#121317', 'panel': '#E7E9ED', 'panel_hi': '#FFFFFF', 'panel_lo': '#BCC0C9', 'seam': '#D0D3DA',
    'ink': '#1E2026', 'ink_dim': '#6D7280',
    # black anodized trays
    'anod': '#1B1C21', 'anod_hi': '#30323B', 'anod_lo': '#0B0C0F',
    # black glass screen
    'glass': '#06070B', 'glass_hi': '#12141C', 'glass_edge': '#000000',
    'fg': '#D5D9E4', 'dim': '#737990', 'faint': '#2E3242',
    # state / semantic
    'run': '#A88BFF', 'run_deep': '#5B43B8', 'idle': '#F2F4FA', 'off': '#3A3D48',
    'ok': '#58D68D', 'warn': '#F2B33D', 'err': '#FF5A52', 'data': '#57C7E3',
    'copper': '#E08A45', 'copper_lo': '#8A4E22',
    # slots
    'well': '#0A0B0F', 'well_edge': '#3A3D48', 'well_dark': '#000000',
}.items()}

DARK = dict(T)  # skin "dark" (household PC devices): dark chassis, RGB accents
DARK.update({k: rgb(v) for k, v in {
    'panel': '#26282E', 'panel_hi': '#3C3F49', 'panel_lo': '#141519', 'seam': '#1D1F24',
    'ink': '#E6E8EE', 'ink_dim': '#8C91A0', 'run': '#22D3EE', 'run_deep': '#0E7C8C', 'idle': '#C9CEDA',
}.items()})

STATE_LINE = {'off': 'off', 'idle': 'idle', 'run': 'run', 'error': 'err', 'no_power': 'warn', 'no_mask': 'warn'}

# ---------------------------------------------------------------- vanilla font
class McFont:
    def __init__(self):
        self.g = {}
        base = os.environ.get('FCW_MC_ASSETS'); jar = os.environ.get('FCW_MC_JAR')
        if base:
            rd = lambda p: open(os.path.join(base, p), 'rb').read()
        else:
            z = zipfile.ZipFile(jar); rd = lambda p: z.read('assets/minecraft/' + p)
        d = json.loads(rd('font/include/default.json'))
        for p in d['providers']:
            if p['type'] != 'bitmap': continue
            yoff = 7 - p['ascent']
            im = Image.open(io.BytesIO(rd('textures/font/' + p['file'].split('/')[-1]))).convert('RGBA')
            rows = p['chars']; cw = im.width // len(rows[0]); ch = im.height // len(rows)
            px = im.load()
            for r, row in enumerate(rows):
                for c, chr_ in enumerate(row):
                    if chr_ in ('\x00', ' ') or chr_ in self.g: continue
                    pts = [(x, y) for y in range(ch) for x in range(cw) if px[c * cw + x, r * ch + y][3] > 0]
                    if pts: self.g[chr_] = ([(x, y + yoff) for x, y in pts], max(x for x, _ in pts) + 1)
    def width(self, s):
        return sum((self.g[c][1] + 1) if c in self.g else 4 for c in s)
    def draw(self, im, x, y, s, col, cell=None, shadow=None):
        """cell=None: proportional (vanilla advance); cell=N: monospace, glyph centred in N px."""
        px = im.load(); W, H = im.size
        for c in s:
            g = self.g.get(c)
            if g:
                ox = x + ((cell - g[1] + 1) // 2 if cell else 0)
                for gx, gy in g[0]:
                    if shadow and 0 <= ox + gx + 1 < W and 0 <= y + gy + 1 < H: px[ox + gx + 1, y + gy + 1] = shadow
                for gx, gy in g[0]:
                    if 0 <= ox + gx < W and 0 <= y + gy < H: px[ox + gx, y + gy] = col
            x += cell if cell else ((g[1] + 1) if g else 4)
        return x

_font = None
def font():
    global _font
    if _font is None: _font = McFont()
    return _font

CELL, LINE = 6, 10   # terminal grid: 6 x 10 px per character (glyphs 6 x 12, rows overlap by 2 blank px)

class TermFont:
    """Monospace terminal font: Terminus 6x12 (SIL OFL 1.1), rendered 1-bit. Glyph cell 6 x 12, baseline at row 10."""
    def __init__(self):
        from PIL import ImageFont
        self.f = ImageFont.truetype(os.environ.get('FCW_TERM_TTF', '/usr/share/fonts/truetype/terminus/TerminusTTF-4.46.0.ttf'), 12)
        self.cache = {}
    def glyph(self, ch):
        if ch not in self.cache:
            im = Image.new('L', (6, 12), 0); d = ImageDraw.Draw(im); d.fontmode = '1'
            d.text((0, 0), ch, font=self.f, fill=255)
            self.cache[ch] = [(x, y) for y in range(12) for x in range(6) if im.getpixel((x, y))]
        return self.cache[ch]
    def draw(self, im, x, y, s, col):
        px = im.load(); W, H = im.size
        for ch in s:
            if ch != ' ':
                for gx, gy in self.glyph(ch):
                    if 0 <= x + gx < W and 0 <= y + gy < H: px[x + gx, y + gy] = col
            x += 6
        return x

_term = None
def term():
    global _term
    if _term is None: _term = TermFont()
    return _term

# ---------------------------------------------------------------- primitives
class Gui:
    def __init__(self, w, h, skin=None, state='idle'):
        self.w, self.h, self.t, self.state = w, h, skin or T, state
        self.im = Image.new('RGBA', (w, h), (0, 0, 0, 0)); self.d = ImageDraw.Draw(self.im)
    def c(self, name): return self.t[name] if isinstance(name, str) else name
    def rect(self, x, y, w, h, col):
        if w <= 0 or h <= 0: return
        self.d.rectangle([x, y, x + w - 1, y + h - 1], fill=self.c(col))
    def px(self, x, y, col): self.im.putpixel((x, y), self.c(col))
    def bevel(self, x, y, w, h, fill, hi, lo, out=None):
        if out: self.rect(x, y, w, h, out); x += 1; y += 1; w -= 2; h -= 2
        self.rect(x, y, w, h, fill)
        self.rect(x, y, w, 1, hi); self.rect(x, y, 1, h, hi)
        self.rect(x, y + h - 1, w, 1, lo); self.rect(x + w - 1, y, 1, h, lo)
    # chassis: 9-slice body + state light line under the title + glowing skirt at the bottom
    def chassis(self, title=None, seams=True):
        w, h = self.w, self.h
        self.bevel(0, 0, w, h, 'panel', 'panel_hi', 'panel_lo', 'out')
        for cx, cy in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)): self.im.putpixel((cx, cy), (0, 0, 0, 0))
        if seams:
            for sx in range(64, w - 8, 64): self.rect(sx, 2, 1, 13, 'seam')
        line = self.c(STATE_LINE[self.state])
        self.rect(6, 16, w - 12, 2, line)
        self.rect(4, h - 3, w - 8, 2, line)
        if title: self.text(8, 5, title, 'ink')
    def text(self, x, y, s, col='fg', mono=False, shadow=None):
        return font().draw(self.im, x, y, s, self.c(col), CELL if mono else None, self.c(shadow) if shadow else None)
    def mtext(self, x, y, s, col='fg'):
        """terminal text; y = top of the 10 px line (the 12 px glyph cell starts 1 px above)"""
        s = ''.join(ch.upper() if ('а' <= ch <= 'я' or ch == 'ё') else ch for ch in s)   # rule: Cyrillic on the glass is upper case
        return term().draw(self.im, x, y - 1, s, self.c(col))
    def tray(self, x, y, w, h): self.bevel(x, y, w, h, 'anod', 'anod_lo', 'anod_hi')
    def glass(self, x, y, w, h):
        self.rect(x, y, w, h, 'glass_edge'); self.rect(x + 1, y + 1, w - 2, h - 2, 'glass')
        self.rect(x, y + h - 1, w, 1, 'panel_hi'); self.rect(x + w - 1, y, 1, h, 'panel_hi')
        for i in range(10):                       # sheen in the top-right corner, like the machines' glass
            self.rect(x + w - 12 + i, y + 1 + i, 2, 1, 'glass_hi')
    def slot(self, x, y, kind='in'):
        """x, y = top-left of the 18x18 box; the item sits at x+1, y+1 (vanilla convention)."""
        edge = {'in': 'well_edge', 'out': 'well_edge', 'mask': 'run', 'fuel': 'copper', 'cell': 'ok', 'filter': 'data',
                'off': 'anod_hi'}[kind]
        self.rect(x, y, 18, 18, 'well_dark'); self.rect(x + 1, y + 1, 16, 16, 'well')
        self.rect(x, y + 17, 18, 1, edge); self.rect(x + 17, y, 1, 18, edge)
        if kind == 'out':
            for i in range(3): self.px(x + 15 - i, y + 15, 'well_edge'); self.px(x + 15, y + 15 - i, 'well_edge')
        if kind in ('mask', 'fuel', 'cell', 'filter'):
            self.rect(x, y, 18, 1, edge); self.rect(x, y, 1, 18, edge)
    def slots(self, x, y, cols, rows, kind='in'):
        for r in range(rows):
            for q in range(cols): self.slot(x + q * 18, y + r * 18, kind)
    def inventory(self, x, y):
        """tray 166 x 80 with the player inventory; slot boxes start at x+2, y+2."""
        self.tray(x, y, 166, 80)
        self.slots(x + 2, y + 2, 9, 3); self.slots(x + 2, y + 60, 9, 1)
    def button(self, x, y, w, h, label, kind='stop', pressed=False):
        fill, ink = {'start': ('run', 'ink'), 'stop': ('anod', 'fg'), 'ghost': ('panel', 'ink'), 'warn': ('err', 'ink')}[kind]
        self.rect(x, y, w, h, 'out'); self.rect(x + 1, y + 1, w - 2, h - 2, fill)
        hi = 'panel_hi' if kind != 'stop' else 'anod_hi'
        if not pressed: self.rect(x + 1, y + 1, w - 2, 1, hi); self.rect(x + 1, y + 1, 1, h - 2, hi)
        tw = font().width(label) - 1
        self.text(x + (w - tw) // 2, y + (h - 7) // 2, label, ink)
    def led(self, x, y, col, on=True):
        self.rect(x, y, 4, 4, 'out'); self.rect(x + 1, y + 1, 2, 2, col if on else 'off')
    def pill(self, x_right, y, label, col):
        tw = font().width(label) - 1; w = tw + 13; x = x_right - w
        self.rect(x + 1, y, w - 2, 11, 'anod'); self.rect(x, y + 1, w, 9, 'anod')
        self.rect(x + 3, y + 3, 5, 5, col); self.text(x + 10, y + 2, label, 'fg')
    def hbar(self, x, y, w, h, frac, col='copper', back='well'):
        self.rect(x - 1, y - 1, w + 2, h + 2, 'well_dark'); self.rect(x, y, w, h, back)
        self.rect(x, y, int(round(w * frac)), h, col)
    def vbar(self, x, y, w, h, frac, col='copper', ticks=True):
        self.rect(x - 1, y - 1, w + 2, h + 2, 'well_dark'); self.rect(x, y, w, h, 'well')
        fh = int(round(h * frac)); self.rect(x, y + h - fh, w, fh, col)
        if fh > 1: self.rect(x, y + h - fh, 1, fh, 'copper_lo' if col == 'copper' else col)
        if ticks:
            for i in range(1, 4): self.rect(x + w + 2, y + h * i // 4, 2, 1, 'ink_dim')
    def tbar(self, x, y, cols, frac, col='run'):
        """terminal progress bar made of 5x7 character blocks"""
        n = int(round(cols * frac))
        for i in range(cols): self.rect(x + i * CELL, y + 1, 5, 8, col if i < n else 'faint')
    def arrow(self, x, y, w, frac, col='run'):
        """progress chevron track, 9 px high"""
        self.rect(x, y + 3, w - 5, 3, 'well');
        for k in range(5): self.rect(x + w - 5 + k, y + k, 1, 9 - 2 * k, 'well')
        f = int(round(w * frac)); 
        self.rect(x, y + 3, min(f, w - 5), 3, col)
        for k in range(5):
            if f >= w - 5 + k + 1: self.rect(x + w - 5 + k, y + k, 1, 9 - 2 * k, col)
    def save(self, path, scales=(1, 2)):
        for s in scales:
            im = self.im if s == 1 else self.im.resize((self.w * s, self.h * s), Image.NEAREST)
            im.save(path if s == 1 else path[:-4] + '@%dx.png' % s, optimize=True)

def wafer_map(g, x, y, done, current=None, bad=(), n=9, cell=6, blink=True, dim=False):
    """round wafer of n x n exposure fields; fields are 5x5 px on a 6 px pitch. Returns field count."""
    r = n / 2.0; idx = 0; total = 0
    cx = x + n * cell // 2; 
    # wafer disc
    for yy in range(n * cell + 4):
        for xx in range(n * cell + 4):
            dx = xx - (n * cell + 3) / 2.0; dy = yy - (n * cell + 3) / 2.0
            if dx * dx + dy * dy <= (n * cell / 2.0 + 1.6) ** 2 and yy < n * cell + 2:
                g.px(x - 2 + xx, y - 2 + yy, 'glass_hi')
    for j in range(n):
        for i in range(n):
            dx = i + .5 - r; dy = j + .5 - r
            if dx * dx + dy * dy > (r - .15) ** 2: continue
            col = 'faint'
            if idx < done: col = 'run_deep' if not dim else 'faint'
            if idx in bad and idx < done: col = 'err'
            if current is not None and idx == current: col = 'idle' if blink else 'run'
            g.rect(x + i * cell, y + j * cell, cell - 1, cell - 1, col)
            if idx < done and idx not in bad and not dim: g.rect(x + i * cell + 1, y + j * cell + 1, cell - 3, cell - 3, 'run')
            idx += 1; total += 1
    return total

def item(g, x, y, kind):
    """tiny placeholder item icons (16x16 at x,y) so the mockups read as a live GUI"""
    R = g.rect
    if kind == 'wafer':
        for yy in range(12):
            for xx in range(12):
                if (xx - 5.5) ** 2 + (yy - 5.5) ** 2 <= 36: g.px(x + 2 + xx, y + 2 + yy, rgb('#8E96AA') if (xx + yy) % 5 else rgb('#C9CEDA'))
    elif kind == 'wafer_cpu':
        for yy in range(12):
            for xx in range(12):
                if (xx - 5.5) ** 2 + (yy - 5.5) ** 2 <= 36: g.px(x + 2 + xx, y + 2 + yy, rgb('#7A5CD6') if (xx % 3 and yy % 3) else rgb('#3B2C6E'))
    elif kind == 'mask':
        R(x + 2, y + 2, 12, 12, rgb('#2A2C36')); R(x + 3, y + 3, 10, 10, rgb('#B9A6FF')); R(x + 5, y + 5, 6, 6, rgb('#3B2C6E'))
        R(x + 6, y + 6, 2, 2, rgb('#E9E2FF')); R(x + 9, y + 8, 1, 2, rgb('#E9E2FF'))
    elif kind == 'tin':
        R(x + 3, y + 6, 10, 6, rgb('#AEB6C4')); R(x + 4, y + 5, 8, 1, rgb('#DDE2EA')); R(x + 3, y + 11, 10, 1, rgb('#7C8494'))
    elif kind == 'quartz':
        R(x + 4, y + 3, 8, 10, rgb('#E8E1DA')); R(x + 5, y + 4, 3, 4, rgb('#FFFFFF')); R(x + 9, y + 9, 3, 4, rgb('#C9BDB2'))
    elif kind == 'coal':
        R(x + 3, y + 4, 10, 9, rgb('#22232A')); R(x + 5, y + 5, 3, 2, rgb('#4A4D58')); R(x + 9, y + 9, 2, 2, rgb('#3A3D48'))
    elif kind == 'silicon':
        R(x + 3, y + 5, 10, 8, rgb('#596072')); R(x + 4, y + 4, 8, 1, rgb('#8A93A8')); R(x + 5, y + 7, 3, 2, rgb('#AEB6C8'))
    elif kind == 'chip':
        R(x + 4, y + 4, 8, 8, rgb('#17181D')); R(x + 6, y + 6, 4, 4, rgb('#3A3D48'))
        for k in range(3): R(x + 2, y + 5 + k * 3, 2, 1, rgb('#D7B45A')); R(x + 12, y + 5 + k * 3, 2, 1, rgb('#D7B45A'))
    elif kind == 'pcb':
        R(x + 2, y + 4, 12, 9, rgb('#1F7A4A')); R(x + 4, y + 6, 3, 3, rgb('#17181D')); R(x + 9, y + 7, 3, 4, rgb('#C9CEDA')); R(x + 2, y + 12, 12, 1, rgb('#D7B45A'))
    elif kind == 'cell':
        R(x + 4, y + 2, 8, 12, rgb('#2A2C36')); R(x + 6, y + 1, 4, 1, rgb('#AEB6C4')); R(x + 5, y + 5, 6, 8, rgb('#58D68D')); R(x + 5, y + 5, 6, 2, rgb('#12141C'))
    elif kind == 'count':
        pass

def count(g, x, y, n):
    s = str(n); w = font().width(s) - 1
    g.text(x + 17 - w, y + 9, s, rgb('#FFFFFF'), shadow=rgb('#3F3F3F'))

def sheet(images, path, pad=12, bg='#D9DDE4', scale=2, cols=3, labels=None):
    """contact sheet of several mockups (scaled, nearest)"""
    ims = [im.resize((im.width * scale, im.height * scale), Image.NEAREST) for im in images]
    cw = max(i.width for i in ims); rows = (len(ims) + cols - 1) // cols
    rh = [max(i.height for i in ims[r * cols:(r + 1) * cols]) for r in range(rows)]
    lab = 14 * (scale if labels else 0) // 1 if labels else 0
    out = Image.new('RGBA', (pad + cols * (cw + pad), pad + sum(rh) + rows * (pad + lab)), rgb(bg))
    y = pad
    for r in range(rows):
        for q, im in enumerate(ims[r * cols:(r + 1) * cols]):
            x = pad + q * (cw + pad) + (cw - im.width) // 2
            out.alpha_composite(im, (x, y))
            if labels:
                t = Image.new('RGBA', (cw // scale, 10), (0, 0, 0, 0))
                font().draw(t, 0, 1, labels[r * cols + q], rgb('#1E2026'))
                out.alpha_composite(t.resize((t.width * scale, t.height * scale), Image.NEAREST), (pad + q * (cw + pad), y + rh[r] + 4))
        y += rh[r] + pad + lab
    out.convert('RGB').save(path, optimize=True)
