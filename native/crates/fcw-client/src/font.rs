//! Встроенный битмап-шрифт 8x16 (Terminus, PSF1, набор Uni2: латиница,
//! кириллица, греческий, псевдографика) и отрисовка ячеек в RGBA.

use std::collections::HashMap;
use std::sync::OnceLock;

pub const CELL_W: usize = 8;
pub const CELL_H: usize = 16;

static PSF: &[u8] = include_bytes!("../assets/terminus-uni2-8x16.psf");

pub struct Font {
    glyphs: Vec<[u8; CELL_H]>,
    map: HashMap<u32, usize>,
    fallback: usize,
    space: usize,
}

static FONT: OnceLock<Font> = OnceLock::new();

pub fn font() -> &'static Font {
    FONT.get_or_init(|| Font::parse_psf1(PSF).expect("embedded font"))
}

impl Font {
    fn parse_psf1(d: &[u8]) -> Option<Font> {
        if d.len() < 4 || d[0] != 0x36 || d[1] != 0x04 {
            return None;
        }
        let mode = d[2];
        let h = d[3] as usize;
        if h != CELL_H {
            return None;
        }
        let n = if mode & 1 != 0 { 512 } else { 256 };
        let data = d.get(4..4 + n * h)?;
        let mut glyphs: Vec<[u8; CELL_H]> =
            data.chunks_exact(h).map(|c| <[u8; CELL_H]>::try_from(c).unwrap()).collect();
        let mut map = HashMap::new();
        if mode & 2 != 0 {
            let tab = &d[4 + n * h..];
            let mut g = 0usize;
            let mut in_seq = false;
            for c in tab.as_chunks::<2>().0 {
                if g >= n {
                    break;
                }
                let v = u16::from_le_bytes([c[0], c[1]]);
                match v {
                    0xffff => {
                        g += 1;
                        in_seq = false;
                    }
                    0xfffe => in_seq = true,
                    _ if !in_seq => {
                        map.entry(v as u32).or_insert(g);
                    }
                    _ => {}
                }
            }
        } else {
            for i in 0..n {
                map.insert(i as u32, i);
            }
        }
        // В этом Terminus строчные «и» и «й» без диагонали, как латинская «u»:
        // русский текст читается как «uнформацuя». Рисуем привычную форму.
        for (cp, g) in cyrillic_i() {
            if let Some(&i) = map.get(&cp) {
                glyphs[i] = g;
            }
        }
        // блочные элементы, которых нет в шрифте, рисуем сами
        for (cp, g) in synth_blocks() {
            if let std::collections::hash_map::Entry::Vacant(e) = map.entry(cp) {
                e.insert(glyphs.len());
                glyphs.push(g);
            }
        }
        let fallback = map.get(&0xfffd).or_else(|| map.get(&('?' as u32))).copied().unwrap_or(0);
        let space = map.get(&(' ' as u32)).copied().unwrap_or(0);
        Some(Font { glyphs, map, fallback, space })
    }

    pub fn has(&self, cp: u32) -> bool {
        self.map.contains_key(&cp)
    }

    pub fn glyph(&self, cp: u32) -> &[u8; CELL_H] {
        let idx =
            if cp == 0 || cp == ' ' as u32 { self.space } else { self.map.get(&cp).copied().unwrap_or(self.fallback) };
        &self.glyphs[idx]
    }
}

/// «и» (U+0438) и «й» (U+0439) с диагональю, на высоте строчных Terminus (ряды 5..=11).
fn cyrillic_i() -> [(u32, [u8; CELL_H]); 2] {
    const BODY: [u8; 7] = [0x42, 0x46, 0x4a, 0x52, 0x62, 0x42, 0x42];
    let mut i = [0u8; CELL_H];
    i[5..12].copy_from_slice(&BODY);
    let mut short_i = i;
    short_i[2] = 0x24; // кратка, как у шрифтовой «й»
    short_i[3] = 0x18;
    [(0x438, i), (0x439, short_i)]
}

fn synth_blocks() -> Vec<(u32, [u8; CELL_H])> {
    let rows = |f: &dyn Fn(usize) -> u8| -> [u8; CELL_H] { std::array::from_fn(f) };
    let mut v: Vec<(u32, [u8; CELL_H])> = vec![
        (0x2580, rows(&|r| if r < 8 { 0xff } else { 0 })),
        (0x2584, rows(&|r| if r >= 8 { 0xff } else { 0 })),
        (0x2588, [0xff; CELL_H]),
        (0x258c, [0xf0; CELL_H]),
        (0x2590, [0x0f; CELL_H]),
        (0x2591, rows(&|r| if r % 2 == 0 { 0x88 } else { 0x22 })),
        (0x2592, rows(&|r| if r % 2 == 0 { 0xaa } else { 0x55 })),
        (0x2593, rows(&|r| if r % 2 == 0 { 0x77 } else { 0xdd })),
        (0x2594, rows(&|r| if r < 2 { 0xff } else { 0 })),
        (0x2595, [0x01; CELL_H]),
    ];
    // нижние восьмые U+2581..U+2587
    for i in 1..8u32 {
        let fill = (i * 2) as usize;
        v.push((0x2580 + i, rows(&|r| if r >= CELL_H - fill { 0xff } else { 0 })));
    }
    // левые восьмые U+2589..U+258F
    for i in 1..8u32 {
        let w = 8 - i;
        let bits = (0xffu16 << (8 - w)) as u8;
        v.push((0x2588 + i, [bits; CELL_H]));
    }
    v
}

pub const FLAG_BOLD: u32 = 1;
pub const FLAG_UNDERLINE: u32 = 4;
pub const FLAG_INVERSE: u32 = 8;

/// Рисует ячейку в буфер RGBA с шагом строки `stride` байт.
#[allow(clippy::too_many_arguments)]
pub fn draw_cell(buf: &mut [u8], stride: usize, x: usize, y: usize, cp: u32, fg: u32, bg: u32, flags: u32) {
    let (mut fg, mut bg) = (fg, bg);
    if flags & FLAG_INVERSE != 0 {
        std::mem::swap(&mut fg, &mut bg);
    }
    let f = [(fg >> 16) as u8, (fg >> 8) as u8, fg as u8, 255];
    let b = [(bg >> 16) as u8, (bg >> 8) as u8, bg as u8, 255];
    let g = font().glyph(cp);
    for (r, &row) in g.iter().enumerate() {
        let mut bits = row;
        if flags & FLAG_BOLD != 0 {
            bits |= bits >> 1;
        }
        if flags & FLAG_UNDERLINE != 0 && r == CELL_H - 1 {
            bits = 0xff;
        }
        let off = (y + r) * stride + x * 4;
        let Some(line) = buf.get_mut(off..off + CELL_W * 4) else { return };
        for (c, px) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            px.copy_from_slice(if bits & (0x80 >> c) != 0 { &f } else { &b });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage() {
        let f = font();
        for c in ['A', 'z', '0', 'Ж', 'я', 'Ё', '─', '│', '┌', '╬', '█', '░', '▒', '▓', '▀', '▄', 'é', 'ß']
        {
            assert!(f.has(c as u32), "missing {c}");
        }
        // разные глифы для разных букв
        assert_ne!(f.glyph('A' as u32), f.glyph('B' as u32));
        assert_eq!(f.glyph(0), f.glyph(' ' as u32));
        assert!(f.glyph(' ' as u32).iter().all(|b| *b == 0));
        // «и» не должна совпадать с «u», «й» — это «и» с краткой
        assert_ne!(f.glyph('и' as u32), f.glyph('u' as u32));
        assert_eq!(f.glyph('и' as u32)[5..], f.glyph('й' as u32)[5..]);
        assert_ne!(f.glyph('и' as u32)[..5], f.glyph('й' as u32)[..5]);
    }

    #[test]
    fn draw() {
        let mut buf = vec![0u8; 8 * 16 * 4];
        draw_cell(&mut buf, 8 * 4, 0, 0, '█' as u32, 0xff0000, 0x0000ff, 0);
        assert_eq!(&buf[0..4], &[255, 0, 0, 255]);
        draw_cell(&mut buf, 8 * 4, 0, 0, ' ' as u32, 0xff0000, 0x0000ff, FLAG_INVERSE);
        assert_eq!(&buf[0..4], &[255, 0, 0, 255]);
        draw_cell(&mut buf, 8 * 4, 0, 0, ' ' as u32, 0xff0000, 0x0000ff, 0);
        assert_eq!(&buf[0..4], &[0, 0, 255, 255]);
    }
}
