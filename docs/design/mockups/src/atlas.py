"""First-pass GUI atlas (256x256) + terminal font atlas, generated from the same primitives as the mockups.
Output: ../atlas/machine_gui.png, machine_gui.json (regions), term_font.png, term_font.json, ../atlas_annotated@3x.png"""
import os, json
from fcwui import *

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
os.makedirs(os.path.join(OUT, 'atlas'), exist_ok=True)
A = Gui(256, 256); R = {}
def reg(name, x, y, w, h, **kw): R[name] = dict(x=x, y=y, w=w, h=h, **kw)

def chassis_tile(x, y, skin):
    g = Gui(48, 48, skin=skin); g.bevel(0, 0, 48, 48, 'panel', 'panel_hi', 'panel_lo', 'out')
    for cx, cy in ((0, 0), (47, 0), (0, 47), (47, 47)): g.im.putpixel((cx, cy), (0, 0, 0, 0))
    A.im.paste(g.im, (x, y))
chassis_tile(0, 0, T); reg('chassis_mk2', 0, 0, 48, 48, slice=4)
chassis_tile(48, 0, DARK); reg('chassis_dark', 48, 0, 48, 48, slice=4)
A.glass(96, 0, 32, 32); reg('glass', 96, 0, 32, 32, slice=12)
A.tray(128, 0, 24, 24); reg('tray', 128, 0, 24, 24, slice=2)
for i, k in enumerate(('in', 'out', 'mask', 'fuel', 'cell', 'filter', 'off')):
    A.slot(i * 18, 48, k); reg('slot_' + k, i * 18, 48, 18, 18)
for i, k in enumerate(('start', 'stop', 'ghost', 'warn')):
    for j, p in enumerate((False, True)):
        x = (i * 2 + j) * 16
        A.button(x, 66, 16, 16, '', k, p); reg('button_%s%s' % (k, '_pressed' if p else ''), x, 66, 16, 16, slice=2)
A.rect(1, 84, 14, 11, 'anod'); A.rect(0, 85, 16, 9, 'anod'); reg('pill', 0, 84, 16, 11, slice=3)
for i, k in enumerate(('off', 'ok', 'run', 'err', 'warn', 'idle', 'data')):
    A.led(i * 4, 96, k); reg('led_' + k, i * 4, 96, 4, 4)
A.arrow(0, 104, 40, 0); reg('arrow_empty', 0, 104, 40, 9)
for i, k in enumerate(('run', 'err', 'warn')):
    A.arrow(40 * (i + 1), 104, 40, 1, k); reg('arrow_' + k, 40 * (i + 1), 104, 40, 9)
for i, (k, a, b) in enumerate((('pending', 'faint', None), ('done', 'run_deep', 'run'), ('current', 'idle', None), ('bad', 'err', None))):
    A.rect(i * 5, 114, 5, 5, a)
    if b: A.rect(i * 5 + 1, 115, 3, 3, b)
    reg('field_' + k, i * 5, 114, 5, 5)
for i, k in enumerate(('off', 'idle', 'run', 'err', 'warn')):
    A.rect(i * 4, 120, 4, 2, k); reg('line_' + k, i * 4, 120, 4, 2)
# 9x9 status icons (placeholders, the artist redraws them)
def icon(i, name, pts, col):
    x = i * 9
    for (px_, py_) in pts: A.px(x + px_, 124 + py_, col)
    reg('icon_' + name, x, 124, 9, 9, placeholder=True)
ring = [(x, y) for x in range(9) for y in range(9) if 9 <= (x - 4) ** 2 + (y - 4) ** 2 <= 16]
icon(0, 'off', ring + [(4, y) for y in range(0, 5)], 'off')
icon(1, 'idle', ring + [(4, 4)], 'idle')
icon(2, 'run', [(x, y) for x in range(2, 8) for y in range(9) if abs(y - 4) <= (7 - x) * 0.8], 'run')
icon(3, 'error', [(k, k) for k in range(1, 8)] + [(k, 8 - k) for k in range(1, 8)], 'err')
icon(4, 'no_power', [(5, 0), (4, 1), (3, 2), (2, 3), (3, 4), (4, 4), (5, 4), (6, 4), (5, 5), (4, 6), (3, 7), (2, 8), (4, 3), (3, 3), (5, 3), (4, 5)], 'warn')
icon(5, 'no_mask', [(x, y) for x in range(1, 8) for y in range(1, 8) if x in (1, 7) or y in (1, 7)] + [(k, 8 - k) for k in range(2, 7)], 'warn')
icon(6, 'output_full', [(x, y) for x in range(1, 8) for y in range(4, 8)] + [(4, 0), (4, 1), (3, 2), (4, 2), (5, 2)], 'err')
# ghost icons for empty special slots (16x16, placeholders)
for i, k in enumerate(('mask', 'wafer', 'coal', 'cell')):
    item(A, i * 16, 136, k); reg('ghost_' + k, i * 16, 136, 16, 16, placeholder=True)
A.im.save(os.path.join(OUT, 'atlas', 'machine_gui.png'), optimize=True)
json.dump(R, open(os.path.join(OUT, 'atlas', 'machine_gui.json'), 'w'), indent=1)

# annotated crop (top 160 px) at 3x
an = A.im.crop((0, 0, 256, 156)); bg = Image.new('RGBA', an.size, rgb('#8F96A6')); bg.alpha_composite(an)
big = bg.resize((256 * 3, 156 * 3), Image.NEAREST); d = ImageDraw.Draw(big)
for n, r in R.items():
    d.rectangle([r['x'] * 3, r['y'] * 3, (r['x'] + r['w']) * 3 - 1, (r['y'] + r['h']) * 3 - 1], outline=(255, 64, 160, 255))
big.convert('RGB').save(os.path.join(OUT, 'atlas_annotated@3x.png'), optimize=True)

# terminal font atlas: 6x12 cells, 32 columns
chars = ''.join(chr(c) for c in range(0x20, 0x7F)) + 'АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ' + '°±·×→←↑↓│─┌┐└┘├┤┬┴┼█▌▐░▒▓▲▼◄►●○■□'
cols = 32; rows = (len(chars) + cols - 1) // cols
fim = Image.new('RGBA', (192, rows * 12), (0, 0, 0, 0))
for i, ch in enumerate(chars):
    for gx, gy in term().glyph(ch): fim.putpixel(((i % cols) * 6 + gx, (i // cols) * 12 + gy), (255, 255, 255, 255))
fim.save(os.path.join(OUT, 'atlas', 'term_font.png'), optimize=True)
json.dump({'cell': [6, 12], 'line_pitch': 10, 'columns': cols, 'chars': chars, 'texture': [192, rows * 12],
           'source': 'Terminus Font 6x12, SIL OFL 1.1 (rename the derived font, ship OFL.txt)'},
          open(os.path.join(OUT, 'atlas', 'term_font.json'), 'w'), ensure_ascii=False, indent=1)
fb = Image.new('RGBA', fim.size, rgb('#06070B')); fb.alpha_composite(fim)
fb.resize((192 * 4, rows * 48), Image.NEAREST).convert('RGB').save(os.path.join(OUT, 'term_font_preview@4x.png'), optimize=True)
print(len(chars), rows)
for n, r in R.items(): print('| `%s` | %d, %d | %d×%d | %s |' % (n, r['x'], r['y'], r['w'], r['h'], ('9-slice, край %d' % r['slice']) if 'slice' in r else ('заглушка' if r.get('placeholder') else '')))
