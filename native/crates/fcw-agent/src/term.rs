//! Терминал для программ `fcw top`, `fcw litho`, …: raw-режим на `termios`, разбор клавиш и вывод
//! сетки ANSI-последовательностями с разницей кадров. Без curses: бинарник остаётся статическим и
//! маленьким (он ездит в гостя кусками по virtio).

use fcw_tui::grid::{Cell, Grid, ACC, ACC_D, BG, DIM, FAINT, FG, RED, RED_D};
use fcw_tui::Key;
use std::fmt::Write as _;
use std::io::Write as _;

/// Сколько цветов умеет терминал.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// 24 бита: Alacritty, пульт.
    True,
    /// 256 цветов: ближайшие из куба и серой шкалы.
    C256,
    /// 16 цветов (`TERM=linux`): фиолетовый → magenta, красный → red, подложки → яркий чёрный.
    C16,
    /// `NO_COLOR` или `--mono`: только инверсия и приглушение.
    Mono,
}

impl Color {
    pub fn parse(s: &str) -> Option<Color> {
        Some(match s {
            "truecolor" | "24bit" => Color::True,
            "256" => Color::C256,
            "16" => Color::C16,
            "mono" | "none" => Color::Mono,
            _ => return None,
        })
    }

    /// По окружению: `NO_COLOR`, `COLORTERM`, `TERM`.
    pub fn detect(get: impl Fn(&str) -> Option<String>) -> Color {
        if get("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return Color::Mono;
        }
        let colorterm = get("COLORTERM").unwrap_or_default();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return Color::True;
        }
        let term = get("TERM").unwrap_or_default();
        if term.contains("256color") {
            Color::C256
        } else if term == "dumb" || term.is_empty() {
            Color::Mono
        } else {
            Color::C16
        }
    }
}

// ------------------------------------------------------------ клавиши

/// Разбор байтов терминала в клавиши. Одинокий ESC — клавиша Esc: последовательности терминал
/// присылает одним куском.
pub fn parse_keys(bytes: &[u8]) -> Vec<Key> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b {
            let (key, used) = escape(&bytes[i + 1..]);
            out.extend(key);
            i += 1 + used;
            continue;
        }
        i += 1;
        match b {
            b'\r' | b'\n' => out.push(Key::Enter),
            b'\t' => out.push(Key::Tab),
            0x7f | 0x08 => out.push(Key::Backspace),
            // Ctrl+C и Ctrl+D в raw-режиме приходят байтами: это выход
            0x03 | 0x04 => out.push(Key::F(10)),
            0x00..=0x1f => {}
            0x20..=0x7e => out.push(Key::Char(b as char)),
            _ => {
                // UTF-8: длина по первому байту
                let len = match b {
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf7 => 4,
                    _ => 1,
                };
                let end = (i - 1 + len).min(bytes.len());
                if let Ok(s) = std::str::from_utf8(&bytes[i - 1..end]) {
                    out.extend(s.chars().map(Key::Char));
                }
                i = end;
            }
        }
    }
    out
}

/// Что стоит после ESC: клавиша и сколько байтов она заняла.
fn escape(rest: &[u8]) -> (Option<Key>, usize) {
    let fkey = |n: u8| Some(Key::F(n));
    match rest {
        [] => (Some(Key::Esc), 0),
        // консоль Linux: F1–F5 как ESC [ [ A..E
        [b'[', b'[', c @ b'A'..=b'E', ..] => (fkey(c - b'A' + 1), 3),
        [b'[', tail @ ..] => {
            let n = tail.iter().take_while(|b| b.is_ascii_digit() || **b == b';').count();
            let Some(last) = tail.get(n) else { return (None, 1 + n) };
            let num: u32 =
                std::str::from_utf8(&tail[..n]).ok().and_then(|s| s.split(';').next()?.parse().ok()).unwrap_or(0);
            let key = match (last, num) {
                (b'A', _) => Some(Key::Up),
                (b'B', _) => Some(Key::Down),
                (b'C', _) => Some(Key::Right),
                (b'D', _) => Some(Key::Left),
                (b'H', _) => Some(Key::Home),
                (b'F', _) => Some(Key::End),
                (b'Z', _) => Some(Key::BackTab),
                (b'P'..=b'S', _) => fkey(last - b'P' + 1),
                (b'~', 1 | 7) => Some(Key::Home),
                (b'~', 4 | 8) => Some(Key::End),
                (b'~', 5) => Some(Key::PgUp),
                (b'~', 6) => Some(Key::PgDn),
                (b'~', 11..=15) => fkey(num as u8 - 10),
                (b'~', 17..=21) => fkey(num as u8 - 11),
                (b'~', 23 | 24) => fkey(num as u8 - 12),
                _ => None,
            };
            (key, 2 + n)
        }
        [b'O', c, ..] => {
            let key = match c {
                b'P'..=b'S' => fkey(c - b'P' + 1),
                b'A' => Some(Key::Up),
                b'B' => Some(Key::Down),
                b'C' => Some(Key::Right),
                b'D' => Some(Key::Left),
                b'H' => Some(Key::Home),
                b'F' => Some(Key::End),
                _ => None,
            };
            (key, 2)
        }
        // Alt+клавиша и прочее: сам ESC считаем нажатием, остальное разберётся дальше
        _ => (Some(Key::Esc), 0),
    }
}

// ------------------------------------------------------------ вывод сетки

/// Номер цвета в палитре 256: ближайший из куба 6×6×6 и серой шкалы.
fn to_256(rgb: u32) -> u8 {
    let (r, g, b) = ((rgb >> 16 & 255) as i32, (rgb >> 8 & 255) as i32, (rgb & 255) as i32);
    const STEPS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let near = |v: i32| (0..6).min_by_key(|i| (STEPS[*i as usize] - v).abs()).unwrap_or(0);
    let (cr, cg, cb) = (near(r), near(g), near(b));
    let cube = |i: i32| STEPS[i as usize];
    let dist = |x: i32, y: i32, z: i32| (x - r).pow(2) + (y - g).pow(2) + (z - b).pow(2);
    let gray = (((r + g + b) / 3 - 8) as f64 / 10.0).round().clamp(0.0, 23.0) as i32;
    let gv = 8 + gray * 10;
    if dist(gv, gv, gv) < dist(cube(cr), cube(cg), cube(cb)) {
        (232 + gray) as u8
    } else {
        (16 + 36 * cr + 6 * cg + cb) as u8
    }
}

/// Цвета палитры программ в 16 цветах: (код текста, код фона).
fn to_16(rgb: u32) -> (u8, u8) {
    match rgb {
        BG => (30, 40),
        FG => (37, 47),
        DIM => (90, 100),
        // подложки — яркий чёрный; консоль Linux яркого фона не умеет и покажет чёрный
        FAINT | ACC_D | RED_D => (90, 100),
        ACC => (35, 45),
        RED => (31, 41),
        _ => (37, 40),
    }
}

/// SGR для ячейки в заданном режиме цвета.
fn sgr(out: &mut String, c: &Cell, mode: Color) {
    match mode {
        Color::True => {
            let rgb = |v: u32| (v >> 16 & 255, v >> 8 & 255, v & 255);
            let (f, b) = (rgb(c.fg), rgb(c.bg));
            let _ = write!(out, "\x1b[38;2;{};{};{};48;2;{};{};{}m", f.0, f.1, f.2, b.0, b.1, b.2);
        }
        Color::C256 => {
            let _ = write!(out, "\x1b[38;5;{};48;5;{}m", to_256(c.fg), to_256(c.bg));
        }
        Color::C16 => {
            let (mut fg, _) = to_16(c.fg);
            let (_, bg) = to_16(c.bg);
            // текст на подложке цветом самой подложки был бы невидим
            if fg == 90 && bg == 100 {
                fg = 37;
            }
            // цвет на тёмной подложке: фиолетовое поле карты читается и без яркого фона
            let fg = match (c.bg, c.fg) {
                (ACC_D, ACC) => 95,
                (RED_D, RED) => 91,
                _ => fg,
            };
            let _ = write!(out, "\x1b[0;{fg};{bg}m");
        }
        Color::Mono => {
            let inverse = matches!(c.bg, FG | ACC | RED);
            let dim = !inverse && matches!(c.fg, DIM | FAINT);
            out.push_str(match (inverse, dim) {
                (true, _) => "\x1b[0;7m",
                (false, true) => "\x1b[0;2m",
                _ => "\x1b[0m",
            });
        }
    }
}

/// Знак ячейки: без цвета подложка полосы (`▄` цветом подложки) должна исчезнуть.
fn glyph(c: &Cell, mode: Color) -> char {
    if mode == Color::Mono && c.fg == FAINT && c.bg == BG {
        return ' ';
    }
    char::from_u32(c.ch).unwrap_or(' ')
}

/// ANSI для перехода от кадра `prev` к кадру `next`: только изменившиеся ячейки.
/// `prev` = `None` (или другой размер) — полная перерисовка.
pub fn diff(prev: Option<&Grid>, next: &Grid, mode: Color) -> String {
    let mut out = String::new();
    let full = prev.is_none_or(|p| p.cols != next.cols || p.rows != next.rows);
    if full {
        out.push_str("\x1b[0m\x1b[2J");
    }
    let mut pen: Option<(u32, u32)> = None;
    for y in 0..next.rows {
        let mut x = 0;
        while x < next.cols {
            let c = next.cell(x, y);
            if !full && prev.is_some_and(|p| p.cell(x, y) == c) {
                x += 1;
                continue;
            }
            let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);
            // подряд идущие изменившиеся ячейки — одним куском
            while x < next.cols {
                let c = next.cell(x, y);
                if !full && prev.is_some_and(|p| p.cell(x, y) == c) {
                    break;
                }
                if pen != Some((c.fg, c.bg)) {
                    sgr(&mut out, &c, mode);
                    pen = Some((c.fg, c.bg));
                }
                out.push(glyph(&c, mode));
                x += 1;
            }
        }
    }
    if !out.is_empty() {
        out.push_str("\x1b[0m");
    }
    out
}

// ------------------------------------------------------------ raw-режим

/// Терминал в raw-режиме на запасном экране; при выходе всё возвращается как было.
pub struct Terminal {
    saved: libc::termios,
}

static mut SAVED: Option<libc::termios> = None;

extern "C" fn on_signal(_: libc::c_int) {
    // SAFETY: только async-signal-safe вызовы; SAVED записан до установки обработчика и больше не меняется.
    unsafe {
        let restore = b"\x1b[0m\x1b[?25h\x1b[?1049l";
        libc::write(1, restore.as_ptr().cast(), restore.len());
        if let Some(t) = (*std::ptr::addr_of!(SAVED)).as_ref() {
            libc::tcsetattr(0, libc::TCSANOW, t);
        }
        libc::_exit(130);
    }
}

impl Terminal {
    /// `None` — stdin или stdout не терминал.
    pub fn open() -> Option<Terminal> {
        // SAFETY: обычные вызовы termios с корректными указателями на свои структуры.
        unsafe {
            if libc::isatty(0) != 1 || libc::isatty(1) != 1 {
                return None;
            }
            let mut saved: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut saved) != 0 {
                return None;
            }
            let mut raw = saved;
            libc::cfmakeraw(&mut raw);
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(0, libc::TCSANOW, &raw) != 0 {
                return None;
            }
            *std::ptr::addr_of_mut!(SAVED) = Some(saved);
            for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
                libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
            }
            let t = Terminal { saved };
            t.write("\x1b[?1049h\x1b[?25l\x1b[2J");
            Some(t)
        }
    }

    /// Размер в знакоместах; терминал не отвечает — 80×25.
    pub fn size(&self) -> (usize, usize) {
        // SAFETY: ioctl TIOCGWINSZ пишет в свою структуру winsize.
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
                return (ws.ws_col as usize, ws.ws_row as usize);
            }
        }
        (80, 25)
    }

    pub fn write(&self, s: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(s.as_bytes());
        let _ = out.flush();
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.write("\x1b[0m\x1b[?25h\x1b[?1049l");
        // SAFETY: возвращаем сохранённые настройки терминала.
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &self.saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_of_xterm_and_linux_console() {
        assert_eq!(parse_keys(b"\x1bOP"), vec![Key::F(1)]);
        assert_eq!(parse_keys(b"\x1b[[A"), vec![Key::F(1)]);
        assert_eq!(parse_keys(b"\x1b[[E"), vec![Key::F(5)]);
        assert_eq!(parse_keys(b"\x1b[15~"), vec![Key::F(5)]);
        assert_eq!(parse_keys(b"\x1b[17~"), vec![Key::F(6)]);
        assert_eq!(parse_keys(b"\x1b[21~"), vec![Key::F(10)]);
        assert_eq!(parse_keys(b"\x1b[A\x1b[B"), vec![Key::Up, Key::Down]);
        assert_eq!(parse_keys(b"\x1b[5~\x1b[6~"), vec![Key::PgUp, Key::PgDn]);
        assert_eq!(parse_keys(b"\x1b[Z\t\r"), vec![Key::BackTab, Key::Tab, Key::Enter]);
        assert_eq!(parse_keys(b"\x1b[1;5A"), vec![Key::Up]);
        assert_eq!(parse_keys(b"\x1b"), vec![Key::Esc]);
        assert_eq!(parse_keys(b"q"), vec![Key::Char('q')]);
        assert_eq!(parse_keys("ы/".as_bytes()), vec![Key::Char('ы'), Key::Char('/')]);
        assert_eq!(parse_keys(b"\x03"), vec![Key::F(10)]);
        assert_eq!(parse_keys(b"\x7f"), vec![Key::Backspace]);
    }

    #[test]
    fn color_mode_from_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(Color::detect(env(&[("COLORTERM", "truecolor"), ("TERM", "alacritty")])), Color::True);
        assert_eq!(Color::detect(env(&[("TERM", "xterm-256color")])), Color::C256);
        assert_eq!(Color::detect(env(&[("TERM", "linux")])), Color::C16);
        assert_eq!(Color::detect(env(&[("TERM", "linux"), ("NO_COLOR", "1")])), Color::Mono);
        assert_eq!(Color::detect(env(&[])), Color::Mono);
    }

    #[test]
    fn palette_in_256_colors() {
        assert_eq!(to_256(0xFFFFFF), 231);
        assert_eq!(to_256(0x000000), 16);
        // фон программ — тёмно-серый из серой шкалы, акцент — из куба
        assert!((232..=255).contains(&to_256(BG)));
        assert!((16..232).contains(&to_256(ACC)));
    }

    #[test]
    fn diff_sends_only_changed_cells() {
        let mut a = Grid::new(10, 2);
        a.put(0, 0, "abc", FG);
        let first = diff(None, &a, Color::True);
        assert!(first.starts_with("\x1b[0m\x1b[2J"));
        assert!(first.contains("abc"));
        let mut b = a.clone();
        b.put(1, 0, "X", RED);
        let d = diff(Some(&a), &b, Color::True);
        assert_eq!(d, "\x1b[1;2H\x1b[38;2;229;83;75;48;2;11;12;15mX\x1b[0m");
        assert_eq!(diff(Some(&b), &b, Color::True), "");
        // другой размер — полная перерисовка
        assert!(diff(Some(&b), &Grid::new(12, 2), Color::C16).starts_with("\x1b[0m\x1b[2J"));
    }

    #[test]
    fn mono_uses_inverse_and_hides_bar_track() {
        let mut g = Grid::new(4, 1);
        g.bar(0, 0, 4, 0.5, ACC);
        let d = diff(None, &g, Color::Mono);
        assert!(d.contains("▄▄"), "{d:?}");
        assert!(!d.contains("▄▄▄"), "{d:?}");
        let mut g = Grid::new(2, 1);
        g.put_on(0, 0, "24", BG, ACC);
        assert!(diff(None, &g, Color::Mono).contains("\x1b[0;7m24"));
    }

    #[test]
    fn sixteen_colors_keep_text_visible_on_pads() {
        let mut g = Grid::new(2, 1);
        g.put_on(0, 0, "26", DIM, FAINT);
        let d = diff(None, &g, Color::C16);
        assert!(d.contains("\x1b[0;37;100m26"), "{d:?}");
        let mut g = Grid::new(2, 1);
        g.put_on(0, 0, "09", RED, RED_D);
        assert!(diff(None, &g, Color::C16).contains("\x1b[0;91;100m09"));
    }
}
