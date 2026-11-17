//! Текстовый экран (tty0): байты virtio-console -> эмулятор терминала vt100
//! -> изменённые строки `TextUpdate`. Плюс перевод клавиш evdev в
//! управляющие последовательности терминала.

use crate::spb;
use fcw_proto::keys::*;
use parking_lot::Mutex;
use tokio::sync::watch;

pub const COLS: u16 = 80;
pub const ROWS: u16 = 25;

pub const DEFAULT_FG: u32 = 0xAAAAAA;
pub const DEFAULT_BG: u32 = 0x000000;

/// Флаги атрибутов (stream.proto AttrRun.flags).
pub const ATTR_BOLD: u32 = 1;
pub const ATTR_ITALIC: u32 = 2;
pub const ATTR_UNDERLINE: u32 = 4;
pub const ATTR_INVERSE: u32 = 8;

/// Палитра в духе консоли Linux (VGA) для первых 16 цветов, дальше xterm-256.
pub fn palette(idx: u8) -> u32 {
    const BASE: [u32; 16] = [
        0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA, 0xAAAAAA, 0x555555, 0xFF5555,
        0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
    ];
    match idx {
        0..=15 => BASE[idx as usize],
        16..=231 => {
            let i = idx - 16;
            let lv = |v: u8| if v == 0 { 0 } else { 55 + v as u32 * 40 };
            (lv(i / 36) << 16) | (lv((i / 6) % 6) << 8) | lv(i % 6)
        }
        _ => {
            let g = 8 + (idx as u32 - 232) * 10;
            (g << 16) | (g << 8) | g
        }
    }
}

fn color(c: vt100::Color, default: u32, bold: bool) -> u32 {
    match c {
        vt100::Color::Default => {
            if bold && default == DEFAULT_FG {
                0xFFFFFF
            } else {
                default
            }
        }
        vt100::Color::Idx(i) if bold && i < 8 => palette(i + 8),
        vt100::Color::Idx(i) => palette(i),
        vt100::Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
    }
}

/// Строка сетки из экрана vt100.
fn snapshot_row(screen: &vt100::Screen, row: u16, cols: u16) -> spb::TextRow {
    let mut chars = Vec::with_capacity(cols as usize);
    let mut runs: Vec<spb::AttrRun> = Vec::new();
    for col in 0..cols {
        let (ch, fg, bg, flags) = match screen.cell(row, col) {
            Some(cell) => {
                let ch = if cell.is_wide_continuation() {
                    0
                } else {
                    cell.contents().chars().next().map(|c| c as u32).unwrap_or(0)
                };
                let bold = cell.bold();
                let mut flags = 0;
                if bold {
                    flags |= ATTR_BOLD;
                }
                if cell.italic() {
                    flags |= ATTR_ITALIC;
                }
                if cell.underline() {
                    flags |= ATTR_UNDERLINE;
                }
                if cell.inverse() {
                    flags |= ATTR_INVERSE;
                }
                (ch, color(cell.fgcolor(), DEFAULT_FG, bold), color(cell.bgcolor(), DEFAULT_BG, false), flags)
            }
            None => (0, DEFAULT_FG, DEFAULT_BG, 0),
        };
        chars.push(ch);
        match runs.last_mut() {
            Some(r) if r.fg == fg && r.bg == bg && r.flags == flags => r.len += 1,
            _ => runs.push(spb::AttrRun { len: 1, fg, bg, flags }),
        }
    }
    spb::TextRow { y: row as u32, chars, runs }
}

struct Inner {
    parser: vt100::Parser,
    rows: Vec<spb::TextRow>,
    row_ver: Vec<u64>,
    cursor: (u32, u32, bool),
    cursor_ver: u64,
    seq: u64,
    epoch: u64,
    /// Хвост предыдущего куска — запросы терминала могут прийти разрезанными.
    tail: Vec<u8>,
}

impl Inner {
    fn fresh(epoch: u64, seq: u64) -> Self {
        let parser = vt100::Parser::new(ROWS, COLS, 0);
        let rows = (0..ROWS).map(|r| snapshot_row(parser.screen(), r, COLS)).collect();
        Inner {
            parser,
            rows,
            row_ver: vec![seq; ROWS as usize],
            cursor: (0, 0, true),
            cursor_ver: seq,
            seq,
            epoch,
            tail: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SubCursor {
    pub seq: u64,
    pub epoch: u64,
    pub started: bool,
}

pub struct TextScreen {
    inner: Mutex<Inner>,
    notify: watch::Sender<u64>,
}

impl Default for TextScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl TextScreen {
    pub fn new() -> Self {
        let (notify, _) = watch::channel(1);
        TextScreen { inner: Mutex::new(Inner::fresh(1, 1)), notify }
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notify.subscribe()
    }

    /// Чистый экран (новый запуск машины).
    pub fn reset(&self) {
        let seq = {
            let mut g = self.inner.lock();
            let epoch = g.epoch + 1;
            let seq = g.seq + 1;
            *g = Inner::fresh(epoch, seq);
            seq
        };
        self.notify.send_replace(seq);
    }

    pub fn application_cursor(&self) -> bool {
        self.inner.lock().parser.screen().application_cursor()
    }

    /// Обрабатывает вывод гостя. Возвращает ответы терминала на запросы
    /// (DSR, DA), которые нужно отправить обратно в консоль.
    pub fn feed(&self, data: &[u8]) -> Vec<u8> {
        let (seq, reply) = {
            let mut g = self.inner.lock();
            g.parser.process(data);
            // запросы терминала: ESC[6n (позиция курсора), ESC[c / ESC[0c (DA)
            let mut scan = std::mem::take(&mut g.tail);
            scan.extend_from_slice(data);
            let mut reply = Vec::new();
            let mut i = 0;
            while i + 1 < scan.len() {
                if scan[i] == 0x1b && scan.get(i + 1) == Some(&b'[') {
                    let rest = &scan[i + 2..];
                    if rest.starts_with(b"6n") {
                        let (r, c) = g.parser.screen().cursor_position();
                        reply.extend_from_slice(format!("\x1b[{};{}R", r + 1, c + 1).as_bytes());
                    } else if rest.starts_with(b"c") || rest.starts_with(b"0c") {
                        reply.extend_from_slice(b"\x1b[?6c");
                    } else if rest.starts_with(b"5n") {
                        reply.extend_from_slice(b"\x1b[0n");
                    }
                }
                i += 1;
            }
            // незавершённый запрос в конце куска сохраняем до следующего
            g.tail = match scan.iter().rposition(|&b| b == 0x1b) {
                Some(p) if scan.len() - p < 4 && &scan[p..] != b"\x1b[c" => scan[p..].to_vec(),
                _ => Vec::new(),
            };

            let next = g.seq + 1;
            let mut changed = false;
            for r in 0..ROWS {
                let row = snapshot_row(g.parser.screen(), r, COLS);
                if row != g.rows[r as usize] {
                    g.rows[r as usize] = row;
                    g.row_ver[r as usize] = next;
                    changed = true;
                }
            }
            let (cr, cc) = g.parser.screen().cursor_position();
            let cursor = (cc as u32, cr as u32, !g.parser.screen().hide_cursor());
            if cursor != g.cursor {
                g.cursor = cursor;
                g.cursor_ver = next;
                changed = true;
            }
            if changed {
                g.seq = next;
                (Some(next), reply)
            } else {
                (None, reply)
            }
        };
        if let Some(seq) = seq {
            self.notify.send_replace(seq);
        }
        reply
    }

    /// Изменённые строки после `cur.seq` (или все при `keyframe`).
    pub fn collect(&self, cur: &mut SubCursor, keyframe: bool) -> Option<spb::TextUpdate> {
        let g = self.inner.lock();
        let full = keyframe || !cur.started || cur.epoch != g.epoch;
        if !full && cur.seq >= g.seq {
            return None;
        }
        let rows_changed: Vec<spb::TextRow> = g
            .rows
            .iter()
            .zip(&g.row_ver)
            .filter(|(_, v)| full || **v > cur.seq)
            .map(|(r, _)| r.clone())
            .collect();
        cur.seq = g.seq;
        cur.epoch = g.epoch;
        cur.started = true;
        Some(spb::TextUpdate {
            seq: g.seq,
            cols: COLS as u32,
            rows: ROWS as u32,
            cursor_x: g.cursor.0,
            cursor_y: g.cursor.1,
            cursor_visible: g.cursor.2,
            full,
            rows_changed,
        })
    }

    /// Содержимое экрана как текст (для отладки и тестов).
    pub fn contents(&self) -> String {
        self.inner.lock().parser.screen().contents()
    }
}

/// Состояние модификаторов для текстового экрана.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Mods {
    pub fn update(&mut self, key: u32, down: bool) -> bool {
        match key {
            KEY_LEFTCTRL | KEY_RIGHTCTRL => self.ctrl = down,
            KEY_LEFTSHIFT | KEY_RIGHTSHIFT => self.shift = down,
            KEY_LEFTALT | KEY_RIGHTALT => self.alt = down,
            _ => return false,
        }
        true
    }
    /// Параметр модификатора xterm (1 + shift + 2*alt + 4*ctrl).
    fn xterm_param(self) -> u32 {
        1 + self.shift as u32 + 2 * self.alt as u32 + 4 * self.ctrl as u32
    }
}

/// Нажатие клавиши -> байты для терминала. Печатные символы приходят
/// отдельно событием Text, поэтому для них возвращается `None`
/// (кроме сочетаний с Ctrl).
pub fn key_sequence(key: u32, mods: Mods, app_cursor: bool) -> Option<Vec<u8>> {
    let m = mods.xterm_param();
    let csi_letter = |c: char| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[1;{m}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde = |n: u32| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[{n};{m}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let ss3 = |c: char| -> Vec<u8> {
        if m > 1 {
            format!("\x1b[1;{m}{c}").into_bytes()
        } else {
            format!("\x1bO{c}").into_bytes()
        }
    };
    let seq = match key {
        KEY_UP => csi_letter('A'),
        KEY_DOWN => csi_letter('B'),
        KEY_RIGHT => csi_letter('C'),
        KEY_LEFT => csi_letter('D'),
        KEY_HOME => csi_letter('H'),
        KEY_END => csi_letter('F'),
        KEY_INSERT => tilde(2),
        KEY_DELETE => tilde(3),
        KEY_PAGEUP => tilde(5),
        KEY_PAGEDOWN => tilde(6),
        KEY_F1 => ss3('P'),
        KEY_F2 => ss3('Q'),
        KEY_F3 => ss3('R'),
        KEY_F4 => ss3('S'),
        KEY_F5 => tilde(15),
        KEY_F6 => tilde(17),
        KEY_F7 => tilde(18),
        KEY_F8 => tilde(19),
        KEY_F9 => tilde(20),
        KEY_F10 => tilde(21),
        KEY_F11 => tilde(23),
        KEY_F12 => tilde(24),
        KEY_ENTER | KEY_KPENTER => b"\r".to_vec(),
        KEY_BACKSPACE => {
            if mods.ctrl {
                vec![0x08]
            } else {
                vec![0x7f]
            }
        }
        KEY_TAB => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        KEY_ESC => vec![0x1b],
        _ if mods.ctrl && !mods.alt => {
            if let Some(i) = LETTERS.iter().position(|&k| k == key) {
                vec![i as u8 + 1]
            } else {
                match key {
                    KEY_SPACE | KEY_2 => vec![0],
                    KEY_LEFTBRACE | KEY_3 => vec![0x1b],
                    KEY_BACKSLASH | KEY_4 => vec![0x1c],
                    KEY_RIGHTBRACE | KEY_5 => vec![0x1d],
                    KEY_6 => vec![0x1e],
                    KEY_MINUS | KEY_SLASH | KEY_7 => vec![0x1f],
                    KEY_8 => vec![0x7f],
                    _ => return None,
                }
            }
        }
        _ => return None,
    };
    Some(seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_cursor() {
        let t = TextScreen::new();
        let mut cur = SubCursor::default();
        let full = t.collect(&mut cur, false).unwrap();
        assert!(full.full);
        assert_eq!(full.rows_changed.len(), ROWS as usize);
        assert!(t.collect(&mut cur, false).is_none());

        t.feed(b"hello\r\n\x1b[31mred\x1b[0m");
        let u = t.collect(&mut cur, false).unwrap();
        assert!(!u.full);
        assert_eq!(u.rows_changed.len(), 2);
        let r0 = &u.rows_changed[0];
        assert_eq!(r0.y, 0);
        assert_eq!(r0.chars.len(), COLS as usize);
        assert_eq!(&r0.chars[..5], &['h' as u32, 'e' as u32, 'l' as u32, 'l' as u32, 'o' as u32]);
        assert_eq!(r0.chars[5], 0);
        let r1 = &u.rows_changed[1];
        assert_eq!(r1.runs[0].fg, 0xAA0000);
        assert_eq!(r1.runs[0].len, 3);
        assert_eq!(r1.runs.iter().map(|r| r.len).sum::<u32>(), COLS as u32);
        assert_eq!((u.cursor_x, u.cursor_y), (3, 1));
    }

    #[test]
    fn unicode_and_dsr() {
        let t = TextScreen::new();
        let reply = t.feed("Привет ─┐\x1b[6n".as_bytes());
        assert_eq!(reply, b"\x1b[1;10R");
        let mut cur = SubCursor::default();
        let u = t.collect(&mut cur, false).unwrap();
        assert_eq!(u.rows_changed[0].chars[0], 'П' as u32);
        assert_eq!(u.rows_changed[0].chars[8], '┐' as u32);
        // запрос, разрезанный между кусками
        assert!(t.feed(b"\x1b[").is_empty());
        assert_eq!(t.feed(b"6n"), b"\x1b[1;10R");
    }

    #[test]
    fn keys() {
        let none = Mods::default();
        let ctrl = Mods { ctrl: true, ..Default::default() };
        assert_eq!(key_sequence(KEY_UP, none, false).unwrap(), b"\x1b[A");
        assert_eq!(key_sequence(KEY_UP, none, true).unwrap(), b"\x1bOA");
        assert_eq!(key_sequence(KEY_UP, ctrl, false).unwrap(), b"\x1b[1;5A");
        assert_eq!(key_sequence(KEY_ENTER, none, false).unwrap(), b"\r");
        assert_eq!(key_sequence(KEY_BACKSPACE, none, false).unwrap(), b"\x7f");
        assert_eq!(key_sequence(KEY_C, ctrl, false).unwrap(), vec![3]);
        assert_eq!(key_sequence(KEY_F5, none, false).unwrap(), b"\x1b[15~");
        assert_eq!(key_sequence(KEY_F1, none, false).unwrap(), b"\x1bOP");
        assert_eq!(key_sequence(KEY_DELETE, none, false).unwrap(), b"\x1b[3~");
        assert!(key_sequence(KEY_A, none, false).is_none());
        let shift = Mods { shift: true, ..Default::default() };
        assert_eq!(key_sequence(KEY_TAB, shift, false).unwrap(), b"\x1b[Z");
    }

    #[test]
    fn palette_values() {
        assert_eq!(palette(196), 0xFF0000);
        assert_eq!(palette(232), 0x080808);
        assert_eq!(palette(15), 0xFFFFFF);
    }
}
