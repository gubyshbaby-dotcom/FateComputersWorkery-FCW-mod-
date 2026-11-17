//! evdev -> XT-скан-коды в кодировке QEMU «qnum» (для QEMU Extended Key
//! Event: расширенные клавиши E0 xx передаются как 0x80 | xx) и keysym X11
//! (для обычного KeyEvent, если сервер не поддерживает расширенный).

use fcw_proto::keys::*;

/// evdev -> qnum. `None` — клавиша не поддерживается.
pub fn evdev_to_qnum(k: u32) -> Option<u32> {
    // Для основного блока (1..=83) коды evdev совпадают с XT set 1.
    if (1..=83).contains(&k) {
        return Some(k);
    }
    Some(match k {
        KEY_102ND => 0x56,
        KEY_F11 => 0x57,
        KEY_F12 => 0x58,
        KEY_RO => 0x73,
        85 => 0x76,  // ZENKAKUHANKAKU
        90 => 0x78,  // KATAKANA
        91 => 0x77,  // HIRAGANA
        92 => 0x79,  // HENKAN
        93 => 0x70,  // KATAKANAHIRAGANA
        94 => 0x7b,  // MUHENKAN
        95 => 0x5c,  // KPJPCOMMA
        KEY_KPENTER => 0x9c,
        KEY_RIGHTCTRL => 0x9d,
        KEY_KPSLASH => 0xb5,
        KEY_SYSRQ => 0xb7,
        KEY_RIGHTALT => 0xb8,
        KEY_HOME => 0xc7,
        KEY_UP => 0xc8,
        KEY_PAGEUP => 0xc9,
        KEY_LEFT => 0xcb,
        KEY_RIGHT => 0xcd,
        KEY_END => 0xcf,
        KEY_DOWN => 0xd0,
        KEY_PAGEDOWN => 0xd1,
        KEY_INSERT => 0xd2,
        KEY_DELETE => 0xd3,
        KEY_MUTE => 0xa0,
        KEY_VOLUMEDOWN => 0xae,
        KEY_VOLUMEUP => 0xb0,
        KEY_POWER => 0xde,
        KEY_KPEQUAL => 0x59,
        KEY_PAUSE => 0xc6,
        KEY_KPCOMMA => 0x7e,
        KEY_YEN => 0x7d,
        KEY_LEFTMETA => 0xdb,
        KEY_RIGHTMETA => 0xdc,
        KEY_COMPOSE => 0xdd,
        KEY_F13 => 0x5d,
        KEY_F14 => 0x5e,
        KEY_F15 => 0x5f,
        KEY_F16 => 0x55,
        KEY_F17 => 0x83,
        KEY_F18 => 0xf7,
        KEY_F19 => 0x84,
        KEY_F20 => 0x5a,
        KEY_F21 => 0x74,
        KEY_F22 => 0xf9,
        KEY_F23 => 0x6d,
        KEY_F24 => 0x6f,
        _ => return None,
    })
}

/// evdev -> keysym X11 (раскладка US, без Shift). 0 — нет keysym.
pub fn evdev_to_keysym(k: u32) -> u32 {
    if let Some(i) = LETTERS.iter().position(|&x| x == k) {
        return b'a' as u32 + i as u32;
    }
    if let Some(i) = DIGITS.iter().position(|&x| x == k) {
        return b'0' as u32 + i as u32;
    }
    if let Some(i) = FKEYS.iter().position(|&x| x == k) {
        return 0xffbe + i as u32;
    }
    match k {
        KEY_ESC => 0xff1b,
        KEY_MINUS => b'-' as u32,
        KEY_EQUAL => b'=' as u32,
        KEY_BACKSPACE => 0xff08,
        KEY_TAB => 0xff09,
        KEY_LEFTBRACE => b'[' as u32,
        KEY_RIGHTBRACE => b']' as u32,
        KEY_ENTER => 0xff0d,
        KEY_LEFTCTRL => 0xffe3,
        KEY_RIGHTCTRL => 0xffe4,
        KEY_SEMICOLON => b';' as u32,
        KEY_APOSTROPHE => b'\'' as u32,
        KEY_GRAVE => b'`' as u32,
        KEY_LEFTSHIFT => 0xffe1,
        KEY_RIGHTSHIFT => 0xffe2,
        KEY_BACKSLASH => b'\\' as u32,
        KEY_COMMA => b',' as u32,
        KEY_DOT => b'.' as u32,
        KEY_SLASH => b'/' as u32,
        KEY_KPASTERISK => 0xffaa,
        KEY_LEFTALT => 0xffe9,
        KEY_RIGHTALT => 0xffea,
        KEY_SPACE => b' ' as u32,
        KEY_CAPSLOCK => 0xffe5,
        KEY_NUMLOCK => 0xff7f,
        KEY_SCROLLLOCK => 0xff14,
        KEY_KP7 => 0xffb7,
        KEY_KP8 => 0xffb8,
        KEY_KP9 => 0xffb9,
        KEY_KPMINUS => 0xffad,
        KEY_KP4 => 0xffb4,
        KEY_KP5 => 0xffb5,
        KEY_KP6 => 0xffb6,
        KEY_KPPLUS => 0xffab,
        KEY_KP1 => 0xffb1,
        KEY_KP2 => 0xffb2,
        KEY_KP3 => 0xffb3,
        KEY_KP0 => 0xffb0,
        KEY_KPDOT => 0xffae,
        KEY_KPENTER => 0xff8d,
        KEY_KPSLASH => 0xffaf,
        KEY_HOME => 0xff50,
        KEY_LEFT => 0xff51,
        KEY_UP => 0xff52,
        KEY_RIGHT => 0xff53,
        KEY_DOWN => 0xff54,
        KEY_PAGEUP => 0xff55,
        KEY_PAGEDOWN => 0xff56,
        KEY_END => 0xff57,
        KEY_INSERT => 0xff63,
        KEY_DELETE => 0xffff,
        KEY_LEFTMETA => 0xffeb,
        KEY_RIGHTMETA => 0xffec,
        KEY_COMPOSE => 0xff67,
        KEY_SYSRQ => 0xff61,
        KEY_PAUSE => 0xff13,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qnum() {
        assert_eq!(evdev_to_qnum(KEY_ESC), Some(0x01));
        assert_eq!(evdev_to_qnum(KEY_A), Some(0x1e));
        assert_eq!(evdev_to_qnum(KEY_ENTER), Some(0x1c));
        assert_eq!(evdev_to_qnum(KEY_F12), Some(0x58));
        assert_eq!(evdev_to_qnum(KEY_UP), Some(0xc8));
        assert_eq!(evdev_to_qnum(KEY_RIGHTCTRL), Some(0x9d));
        assert_eq!(evdev_to_qnum(KEY_LEFTMETA), Some(0xdb));
        assert_eq!(evdev_to_qnum(500), None);
    }

    #[test]
    fn keysym() {
        assert_eq!(evdev_to_keysym(KEY_A), 'a' as u32);
        assert_eq!(evdev_to_keysym(KEY_0), '0' as u32);
        assert_eq!(evdev_to_keysym(KEY_F1), 0xffbe);
        assert_eq!(evdev_to_keysym(KEY_F12), 0xffc9);
        assert_eq!(evdev_to_keysym(KEY_ENTER), 0xff0d);
    }
}
