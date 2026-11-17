//! Сетка знакомест: то, во что рисуют программы. Ячейка — та же четвёрка, что у консоли клиента
//! (`fcw-client`, `view.rs`): знак, цвет текста, цвет фона, флаги. Терминала здесь нет.

/// Палитра (docs/design/CONSOLE_APPS.md, раздел 3). Других цветов программы не используют.
pub const BG: u32 = 0x0B0C0F;
pub const FG: u32 = 0xC9CDD6;
pub const DIM: u32 = 0x6B7080;
pub const FAINT: u32 = 0x1A1C22;
pub const ACC: u32 = 0xA88BFF;
pub const ACC_D: u32 = 0x2C2650;
pub const RED: u32 = 0xE5534B;
pub const RED_D: u32 = 0x4A1B19;

/// Флаги ячейки — те же биты, что в `fcw-client` (`font.rs`).
pub const FLAG_BOLD: u32 = 1;
pub const FLAG_UNDERLINE: u32 = 4;
pub const FLAG_INVERSE: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: u32,
    pub fg: u32,
    pub bg: u32,
    pub flags: u32,
}

pub const BLANK: Cell = Cell { ch: ' ' as u32, fg: FG, bg: BG, flags: 0 };

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cols: i32,
    pub rows: i32,
    cells: Vec<Cell>,
}

/// Число знакомест в строке (все знаки шрифта одинарной ширины).
pub fn width(s: &str) -> i32 {
    s.chars().count() as i32
}

/// Обрезать или дополнить пробелами справа до ширины `w`.
pub fn fit(s: &str, w: i32) -> String {
    let w = w.max(0) as usize;
    let mut out: String = s.chars().take(w).collect();
    for _ in out.chars().count()..w {
        out.push(' ');
    }
    out
}

/// То же, но с выравниванием вправо.
pub fn rfit(s: &str, w: i32) -> String {
    let w = w.max(0) as usize;
    let n = s.chars().count();
    if n >= w {
        return s.chars().take(w).collect();
    }
    format!("{}{s}", " ".repeat(w - n))
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Grid {
        Grid { cols: cols as i32, rows: rows as i32, cells: vec![BLANK; cols * rows] }
    }

    pub fn cell(&self, x: i32, y: i32) -> Cell {
        if x < 0 || y < 0 || x >= self.cols || y >= self.rows {
            return BLANK;
        }
        self.cells[(y * self.cols + x) as usize]
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    fn at(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if x < 0 || y < 0 || x >= self.cols || y >= self.rows {
            return None;
        }
        self.cells.get_mut((y * self.cols + x) as usize)
    }

    /// Текст поверх того фона, что уже есть. Возвращает столбец после текста.
    pub fn put(&mut self, x: i32, y: i32, text: &str, fg: u32) -> i32 {
        let mut i = x;
        for ch in text.chars() {
            if let Some(c) = self.at(i, y) {
                c.ch = ch as u32;
                c.fg = fg;
            }
            i += 1;
        }
        i
    }

    /// Текст со своим фоном.
    pub fn put_on(&mut self, x: i32, y: i32, text: &str, fg: u32, bg: u32) -> i32 {
        let mut i = x;
        for ch in text.chars() {
            if let Some(c) = self.at(i, y) {
                *c = Cell { ch: ch as u32, fg, bg, flags: 0 };
            }
            i += 1;
        }
        i
    }

    /// Текст не шире `w` знакомест.
    pub fn put_w(&mut self, x: i32, y: i32, text: &str, w: i32, fg: u32) -> i32 {
        let cut: String = text.chars().take(w.max(0) as usize).collect();
        self.put(x, y, &cut, fg)
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, bg: u32) {
        for j in y..y + h {
            for i in x..x + w {
                if let Some(c) = self.at(i, j) {
                    *c = Cell { ch: ' ' as u32, fg: FG, bg, flags: 0 };
                }
            }
        }
    }

    /// Заголовок раздела на линейке: `── Маска ─────`.
    pub fn rule(&mut self, x: i32, y: i32, w: i32, title: &str) {
        for i in 0..w {
            self.put(x + i, y, "─", DIM);
        }
        if !title.is_empty() && w > 6 {
            let t: String = title.chars().take((w - 6) as usize).collect();
            self.put(x + 2, y, &format!(" {t} "), FG);
        }
    }

    /// Рамка главного графического объекта.
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, title: &str) {
        if w < 2 || h < 2 {
            return;
        }
        let line = "─".repeat((w - 2) as usize);
        self.put(x, y, &format!("┌{line}┐"), DIM);
        self.put(x, y + h - 1, &format!("└{line}┘"), DIM);
        for j in 1..h - 1 {
            self.put(x, y + j, "│", DIM);
            self.put(x + w - 1, y + j, "│", DIM);
        }
        if !title.is_empty() && w > 6 {
            let t: String = title.chars().take((w - 6) as usize).collect();
            self.put(x + 2, y, &format!(" {t} "), FG);
        }
    }

    /// Полоса в полвысоты знакоместа: соседние строки не слипаются.
    pub fn bar(&mut self, x: i32, y: i32, w: i32, frac: f64, fg: u32) {
        self.bar_on(x, y, w, frac, fg, FAINT);
    }

    pub fn bar_on(&mut self, x: i32, y: i32, w: i32, frac: f64, fg: u32, back: u32) {
        let frac = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
        let n = (frac * w as f64).round() as i32;
        for i in 0..w {
            self.put(x + i, y, "▄", if i < n { fg } else { back });
        }
    }

    /// Пара «подпись — значение»; значение не шире `vw`.
    #[allow(clippy::too_many_arguments)]
    pub fn kv(&mut self, x: i32, y: i32, key: &str, value: &str, kw: i32, vw: i32, vfg: u32) {
        self.put_w(x, y, key, kw - 1, DIM);
        self.put_w(x + kw, y, value, vw, vfg);
    }

    /// График в одну строку блоками в восьмую долю высоты.
    pub fn spark(&mut self, x: i32, y: i32, vals: &[f64], fg: u32, lo: f64, hi: f64) {
        for (i, v) in vals.iter().enumerate() {
            let k = ((v - lo) / (hi - lo).max(1e-9) * 7.0).round().clamp(0.0, 7.0) as u32;
            let ch = if k >= 7 { '█' } else { char::from_u32(0x2581 + k).unwrap_or(' ') };
            self.put(x + i as i32, y, &ch.to_string(), fg);
        }
    }

    /// График с заливкой: `h` строк по 8 уровней. Столбцы от `upto` и правее — подложкой (план).
    #[allow(clippy::too_many_arguments)]
    pub fn area(&mut self, x: i32, y: i32, w: i32, h: i32, vals: &[f64], fg: u32, upto: Option<i32>) {
        if vals.is_empty() || w <= 0 {
            return;
        }
        for i in 0..w {
            let v = vals[(i as usize * vals.len() / w as usize).min(vals.len() - 1)].clamp(0.0, 1.0);
            let lev = (v * h as f64 * 8.0).round() as i32;
            let c = if upto.is_none_or(|u| i < u) { fg } else { FAINT };
            for j in 0..h {
                let k = (lev - j * 8).clamp(0, 8);
                if k > 0 {
                    let ch = if k == 8 { '█' } else { char::from_u32(0x2580 + k as u32).unwrap_or(' ') };
                    self.put(x + i, y + h - 1 - j, &ch.to_string(), c);
                }
            }
        }
    }

    /// Приглушить строки `y0..y1`: данные устарели. Инверсные и цветные подложки остаются, цвет текста гаснет.
    pub fn dim_rows(&mut self, y0: i32, y1: i32) {
        for y in y0.max(0)..y1.min(self.rows) {
            for x in 0..self.cols {
                if let Some(c) = self.at(x, y) {
                    if c.bg == ACC || c.bg == RED_D || c.bg == ACC_D {
                        c.bg = FAINT;
                    }
                    // знаки, нарисованные цветом подложки (зазор под полем карты), остаются подложкой
                    if c.fg == ACC || c.fg == RED_D || c.fg == ACC_D {
                        c.fg = if c.ch == '▀' as u32 { FAINT } else { DIM };
                    }
                    if c.fg != BG && c.fg != FAINT {
                        c.fg = DIM;
                    }
                    if c.fg == BG && c.bg == FAINT {
                        c.fg = DIM;
                    }
                }
            }
        }
    }

    /// Текст сетки без цвета: строки без хвостовых пробелов. Для снимков в тестах и вывода «в файл».
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.rows {
            let mut line = String::new();
            for x in 0..self.cols {
                line.push(char::from_u32(self.cell(x, y).ch).unwrap_or(' '));
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_clips_and_keeps_background() {
        let mut g = Grid::new(4, 1);
        g.fill(0, 0, 4, 1, FAINT);
        assert_eq!(g.put(2, 0, "abc", ACC), 5);
        assert_eq!(g.cell(3, 0), Cell { ch: 'b' as u32, fg: ACC, bg: FAINT, flags: 0 });
        assert_eq!(g.to_text(), "  ab\n");
    }

    #[test]
    fn bar_rounds_to_cells() {
        let mut g = Grid::new(10, 1);
        g.bar(0, 0, 10, 0.74, ACC);
        let lit = (0..10).filter(|x| g.cell(*x, 0).fg == ACC).count();
        assert_eq!(lit, 7);
        g.bar(0, 0, 10, f64::NAN, ACC);
        assert_eq!((0..10).filter(|x| g.cell(*x, 0).fg == ACC).count(), 0);
    }

    #[test]
    fn fit_pads_and_cuts() {
        assert_eq!(fit("абв", 5), "абв  ");
        assert_eq!(fit("абвгд", 3), "абв");
        assert_eq!(rfit("12", 4), "  12");
    }
}
