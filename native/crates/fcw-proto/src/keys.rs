//! Коды клавиш Linux evdev (linux/input-event-codes.h), общие для клиента,
//! демона и fcw-probe.

#![allow(missing_docs)]

pub const KEY_ESC: u32 = 1;
pub const KEY_1: u32 = 2;
pub const KEY_2: u32 = 3;
pub const KEY_3: u32 = 4;
pub const KEY_4: u32 = 5;
pub const KEY_5: u32 = 6;
pub const KEY_6: u32 = 7;
pub const KEY_7: u32 = 8;
pub const KEY_8: u32 = 9;
pub const KEY_9: u32 = 10;
pub const KEY_0: u32 = 11;
pub const KEY_MINUS: u32 = 12;
pub const KEY_EQUAL: u32 = 13;
pub const KEY_BACKSPACE: u32 = 14;
pub const KEY_TAB: u32 = 15;
pub const KEY_Q: u32 = 16;
pub const KEY_W: u32 = 17;
pub const KEY_E: u32 = 18;
pub const KEY_R: u32 = 19;
pub const KEY_T: u32 = 20;
pub const KEY_Y: u32 = 21;
pub const KEY_U: u32 = 22;
pub const KEY_I: u32 = 23;
pub const KEY_O: u32 = 24;
pub const KEY_P: u32 = 25;
pub const KEY_LEFTBRACE: u32 = 26;
pub const KEY_RIGHTBRACE: u32 = 27;
pub const KEY_ENTER: u32 = 28;
pub const KEY_LEFTCTRL: u32 = 29;
pub const KEY_A: u32 = 30;
pub const KEY_S: u32 = 31;
pub const KEY_D: u32 = 32;
pub const KEY_F: u32 = 33;
pub const KEY_G: u32 = 34;
pub const KEY_H: u32 = 35;
pub const KEY_J: u32 = 36;
pub const KEY_K: u32 = 37;
pub const KEY_L: u32 = 38;
pub const KEY_SEMICOLON: u32 = 39;
pub const KEY_APOSTROPHE: u32 = 40;
pub const KEY_GRAVE: u32 = 41;
pub const KEY_LEFTSHIFT: u32 = 42;
pub const KEY_BACKSLASH: u32 = 43;
pub const KEY_Z: u32 = 44;
pub const KEY_X: u32 = 45;
pub const KEY_C: u32 = 46;
pub const KEY_V: u32 = 47;
pub const KEY_B: u32 = 48;
pub const KEY_N: u32 = 49;
pub const KEY_M: u32 = 50;
pub const KEY_COMMA: u32 = 51;
pub const KEY_DOT: u32 = 52;
pub const KEY_SLASH: u32 = 53;
pub const KEY_RIGHTSHIFT: u32 = 54;
pub const KEY_KPASTERISK: u32 = 55;
pub const KEY_LEFTALT: u32 = 56;
pub const KEY_SPACE: u32 = 57;
pub const KEY_CAPSLOCK: u32 = 58;
pub const KEY_F1: u32 = 59;
pub const KEY_F2: u32 = 60;
pub const KEY_F3: u32 = 61;
pub const KEY_F4: u32 = 62;
pub const KEY_F5: u32 = 63;
pub const KEY_F6: u32 = 64;
pub const KEY_F7: u32 = 65;
pub const KEY_F8: u32 = 66;
pub const KEY_F9: u32 = 67;
pub const KEY_F10: u32 = 68;
pub const KEY_NUMLOCK: u32 = 69;
pub const KEY_SCROLLLOCK: u32 = 70;
pub const KEY_KP7: u32 = 71;
pub const KEY_KP8: u32 = 72;
pub const KEY_KP9: u32 = 73;
pub const KEY_KPMINUS: u32 = 74;
pub const KEY_KP4: u32 = 75;
pub const KEY_KP5: u32 = 76;
pub const KEY_KP6: u32 = 77;
pub const KEY_KPPLUS: u32 = 78;
pub const KEY_KP1: u32 = 79;
pub const KEY_KP2: u32 = 80;
pub const KEY_KP3: u32 = 81;
pub const KEY_KP0: u32 = 82;
pub const KEY_KPDOT: u32 = 83;
pub const KEY_102ND: u32 = 86;
pub const KEY_F11: u32 = 87;
pub const KEY_F12: u32 = 88;
pub const KEY_RO: u32 = 89;
pub const KEY_KPENTER: u32 = 96;
pub const KEY_RIGHTCTRL: u32 = 97;
pub const KEY_KPSLASH: u32 = 98;
pub const KEY_SYSRQ: u32 = 99;
pub const KEY_RIGHTALT: u32 = 100;
pub const KEY_HOME: u32 = 102;
pub const KEY_UP: u32 = 103;
pub const KEY_PAGEUP: u32 = 104;
pub const KEY_LEFT: u32 = 105;
pub const KEY_RIGHT: u32 = 106;
pub const KEY_END: u32 = 107;
pub const KEY_DOWN: u32 = 108;
pub const KEY_PAGEDOWN: u32 = 109;
pub const KEY_INSERT: u32 = 110;
pub const KEY_DELETE: u32 = 111;
pub const KEY_MUTE: u32 = 113;
pub const KEY_VOLUMEDOWN: u32 = 114;
pub const KEY_VOLUMEUP: u32 = 115;
pub const KEY_POWER: u32 = 116;
pub const KEY_KPEQUAL: u32 = 117;
pub const KEY_PAUSE: u32 = 119;
pub const KEY_KPCOMMA: u32 = 121;
pub const KEY_YEN: u32 = 124;
pub const KEY_LEFTMETA: u32 = 125;
pub const KEY_RIGHTMETA: u32 = 126;
pub const KEY_COMPOSE: u32 = 127;
pub const KEY_F13: u32 = 183;
pub const KEY_F14: u32 = 184;
pub const KEY_F15: u32 = 185;
pub const KEY_F16: u32 = 186;
pub const KEY_F17: u32 = 187;
pub const KEY_F18: u32 = 188;
pub const KEY_F19: u32 = 189;
pub const KEY_F20: u32 = 190;
pub const KEY_F21: u32 = 191;
pub const KEY_F22: u32 = 192;
pub const KEY_F23: u32 = 193;
pub const KEY_F24: u32 = 194;

/// Буквы a..z в порядке алфавита.
pub const LETTERS: [u32; 26] = [
    KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O,
    KEY_P, KEY_Q, KEY_R, KEY_S, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
];

/// Цифры 0..9.
pub const DIGITS: [u32; 10] = [KEY_0, KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9];

/// F1..F12.
pub const FKEYS: [u32; 12] =
    [KEY_F1, KEY_F2, KEY_F3, KEY_F4, KEY_F5, KEY_F6, KEY_F7, KEY_F8, KEY_F9, KEY_F10, KEY_F11, KEY_F12];

pub fn is_modifier(k: u32) -> bool {
    matches!(
        k,
        KEY_LEFTCTRL | KEY_RIGHTCTRL | KEY_LEFTSHIFT | KEY_RIGHTSHIFT | KEY_LEFTALT | KEY_RIGHTALT | KEY_LEFTMETA
            | KEY_RIGHTMETA
    )
}

/// Символ ASCII -> (клавиша evdev, нужен Shift) для раскладки US.
pub fn char_to_evdev_us(c: char) -> Option<(u32, bool)> {
    Some(match c {
        'a'..='z' => (LETTERS[(c as u8 - b'a') as usize], false),
        'A'..='Z' => (LETTERS[(c as u8 - b'A') as usize], true),
        '0'..='9' => (DIGITS[(c as u8 - b'0') as usize], false),
        ' ' => (KEY_SPACE, false),
        '\n' | '\r' => (KEY_ENTER, false),
        '\t' => (KEY_TAB, false),
        '\x08' | '\x7f' => (KEY_BACKSPACE, false),
        '\x1b' => (KEY_ESC, false),
        '-' => (KEY_MINUS, false),
        '_' => (KEY_MINUS, true),
        '=' => (KEY_EQUAL, false),
        '+' => (KEY_EQUAL, true),
        '[' => (KEY_LEFTBRACE, false),
        '{' => (KEY_LEFTBRACE, true),
        ']' => (KEY_RIGHTBRACE, false),
        '}' => (KEY_RIGHTBRACE, true),
        ';' => (KEY_SEMICOLON, false),
        ':' => (KEY_SEMICOLON, true),
        '\'' => (KEY_APOSTROPHE, false),
        '"' => (KEY_APOSTROPHE, true),
        '`' => (KEY_GRAVE, false),
        '~' => (KEY_GRAVE, true),
        '\\' => (KEY_BACKSLASH, false),
        '|' => (KEY_BACKSLASH, true),
        ',' => (KEY_COMMA, false),
        '<' => (KEY_COMMA, true),
        '.' => (KEY_DOT, false),
        '>' => (KEY_DOT, true),
        '/' => (KEY_SLASH, false),
        '?' => (KEY_SLASH, true),
        '!' => (KEY_1, true),
        '@' => (KEY_2, true),
        '#' => (KEY_3, true),
        '$' => (KEY_4, true),
        '%' => (KEY_5, true),
        '^' => (KEY_6, true),
        '&' => (KEY_7, true),
        '*' => (KEY_8, true),
        '(' => (KEY_9, true),
        ')' => (KEY_0, true),
        _ => return None,
    })
}

/// Имя клавиши («enter», «f5», «a», «leftctrl», «ctrl», …) -> код evdev.
pub fn evdev_from_name(name: &str) -> Option<u32> {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_prefix("key_").unwrap_or(&n);
    if n.len() == 1 {
        let c = n.chars().next()?;
        if c.is_ascii_alphanumeric() {
            return char_to_evdev_us(c).map(|(k, _)| k);
        }
    }
    if let Some(f) = n.strip_prefix('f') {
        if let Ok(i) = f.parse::<usize>() {
            if (1..=12).contains(&i) {
                return Some(FKEYS[i - 1]);
            }
        }
    }
    Some(match n {
        "esc" | "escape" => KEY_ESC,
        "enter" | "return" => KEY_ENTER,
        "tab" => KEY_TAB,
        "backspace" | "bs" => KEY_BACKSPACE,
        "space" => KEY_SPACE,
        "up" => KEY_UP,
        "down" => KEY_DOWN,
        "left" => KEY_LEFT,
        "right" => KEY_RIGHT,
        "home" => KEY_HOME,
        "end" => KEY_END,
        "pageup" | "pgup" => KEY_PAGEUP,
        "pagedown" | "pgdn" => KEY_PAGEDOWN,
        "insert" | "ins" => KEY_INSERT,
        "delete" | "del" => KEY_DELETE,
        "ctrl" | "control" | "leftctrl" => KEY_LEFTCTRL,
        "rightctrl" => KEY_RIGHTCTRL,
        "shift" | "leftshift" => KEY_LEFTSHIFT,
        "rightshift" => KEY_RIGHTSHIFT,
        "alt" | "leftalt" => KEY_LEFTALT,
        "rightalt" | "altgr" => KEY_RIGHTALT,
        "meta" | "super" | "win" | "leftmeta" => KEY_LEFTMETA,
        "rightmeta" => KEY_RIGHTMETA,
        "menu" | "compose" => KEY_COMPOSE,
        "capslock" => KEY_CAPSLOCK,
        "numlock" => KEY_NUMLOCK,
        "scrolllock" => KEY_SCROLLLOCK,
        "sysrq" | "print" => KEY_SYSRQ,
        "pause" => KEY_PAUSE,
        "minus" => KEY_MINUS,
        "equal" => KEY_EQUAL,
        "slash" => KEY_SLASH,
        "backslash" => KEY_BACKSLASH,
        "dot" | "period" => KEY_DOT,
        "comma" => KEY_COMMA,
        "semicolon" => KEY_SEMICOLON,
        "apostrophe" => KEY_APOSTROPHE,
        "grave" => KEY_GRAVE,
        "leftbrace" => KEY_LEFTBRACE,
        "rightbrace" => KEY_RIGHTBRACE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_chars() {
        assert_eq!(char_to_evdev_us('a'), Some((KEY_A, false)));
        assert_eq!(char_to_evdev_us('Q'), Some((KEY_Q, true)));
        assert_eq!(char_to_evdev_us('0'), Some((KEY_0, false)));
        assert_eq!(char_to_evdev_us('?'), Some((KEY_SLASH, true)));
        assert_eq!(char_to_evdev_us('ж'), None);
    }

    #[test]
    fn names() {
        assert_eq!(evdev_from_name("Enter"), Some(KEY_ENTER));
        assert_eq!(evdev_from_name("F12"), Some(KEY_F12));
        assert_eq!(evdev_from_name("c"), Some(KEY_C));
        assert_eq!(evdev_from_name("KEY_UP"), Some(KEY_UP));
        assert_eq!(evdev_from_name("nope"), None);
    }
}
