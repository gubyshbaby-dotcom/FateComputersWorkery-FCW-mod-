//! Коды клавиш GLFW -> Linux evdev.

use fcw_proto::keys::*;

/// GLFW_KEY_* -> evdev. Если клавиша неизвестна GLFW (-1), пробуем
/// платформенный скан-код.
pub fn to_evdev(glfw_key: i32, scancode: i32) -> Option<u32> {
    by_key(glfw_key).or_else(|| by_scancode(scancode))
}

pub fn by_key(k: i32) -> Option<u32> {
    Some(match k {
        32 => KEY_SPACE,
        39 => KEY_APOSTROPHE,
        44 => KEY_COMMA,
        45 => KEY_MINUS,
        46 => KEY_DOT,
        47 => KEY_SLASH,
        48..=57 => DIGITS[(k - 48) as usize],
        59 => KEY_SEMICOLON,
        61 => KEY_EQUAL,
        65..=90 => LETTERS[(k - 65) as usize],
        91 => KEY_LEFTBRACE,
        92 => KEY_BACKSLASH,
        93 => KEY_RIGHTBRACE,
        96 => KEY_GRAVE,
        161 | 162 => KEY_102ND,
        256 => KEY_ESC,
        257 => KEY_ENTER,
        258 => KEY_TAB,
        259 => KEY_BACKSPACE,
        260 => KEY_INSERT,
        261 => KEY_DELETE,
        262 => KEY_RIGHT,
        263 => KEY_LEFT,
        264 => KEY_DOWN,
        265 => KEY_UP,
        266 => KEY_PAGEUP,
        267 => KEY_PAGEDOWN,
        268 => KEY_HOME,
        269 => KEY_END,
        280 => KEY_CAPSLOCK,
        281 => KEY_SCROLLLOCK,
        282 => KEY_NUMLOCK,
        283 => KEY_SYSRQ,
        284 => KEY_PAUSE,
        290..=301 => FKEYS[(k - 290) as usize],
        302..=313 => KEY_F13 + (k - 302) as u32,
        320 => KEY_KP0,
        321 => KEY_KP1,
        322 => KEY_KP2,
        323 => KEY_KP3,
        324 => KEY_KP4,
        325 => KEY_KP5,
        326 => KEY_KP6,
        327 => KEY_KP7,
        328 => KEY_KP8,
        329 => KEY_KP9,
        330 => KEY_KPDOT,
        331 => KEY_KPSLASH,
        332 => KEY_KPASTERISK,
        333 => KEY_KPMINUS,
        334 => KEY_KPPLUS,
        335 => KEY_KPENTER,
        336 => KEY_KPEQUAL,
        340 => KEY_LEFTSHIFT,
        341 => KEY_LEFTCTRL,
        342 => KEY_LEFTALT,
        343 => KEY_LEFTMETA,
        344 => KEY_RIGHTSHIFT,
        345 => KEY_RIGHTCTRL,
        346 => KEY_RIGHTALT,
        347 => KEY_RIGHTMETA,
        348 => KEY_COMPOSE,
        _ => return None,
    })
}

/// Платформенный скан-код GLFW -> evdev (для клавиш, которых нет в GLFW).
pub fn by_scancode(sc: i32) -> Option<u32> {
    if sc <= 0 {
        return None;
    }
    if cfg!(any(target_os = "linux", target_os = "freebsd")) {
        // X11 и Wayland: keycode = evdev + 8
        return (sc > 8).then(|| (sc - 8) as u32);
    }
    if cfg!(windows) {
        // скан-код set 1, расширенные клавиши с битом 0x100
        let base = (sc & 0xff) as u32;
        if sc & 0x100 == 0 {
            return (1..=88).contains(&base).then_some(base);
        }
        return Some(match base {
            0x1c => KEY_KPENTER,
            0x1d => KEY_RIGHTCTRL,
            0x35 => KEY_KPSLASH,
            0x37 => KEY_SYSRQ,
            0x38 => KEY_RIGHTALT,
            0x47 => KEY_HOME,
            0x48 => KEY_UP,
            0x49 => KEY_PAGEUP,
            0x4b => KEY_LEFT,
            0x4d => KEY_RIGHT,
            0x4f => KEY_END,
            0x50 => KEY_DOWN,
            0x51 => KEY_PAGEDOWN,
            0x52 => KEY_INSERT,
            0x53 => KEY_DELETE,
            0x5b => KEY_LEFTMETA,
            0x5c => KEY_RIGHTMETA,
            0x5d => KEY_COMPOSE,
            _ => return None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(by_key(65), Some(KEY_A));
        assert_eq!(by_key(90), Some(KEY_Z));
        assert_eq!(by_key(48), Some(KEY_0));
        assert_eq!(by_key(49), Some(KEY_1));
        assert_eq!(by_key(257), Some(KEY_ENTER));
        assert_eq!(by_key(265), Some(KEY_UP));
        assert_eq!(by_key(290), Some(KEY_F1));
        assert_eq!(by_key(301), Some(KEY_F12));
        assert_eq!(by_key(341), Some(KEY_LEFTCTRL));
        assert_eq!(by_key(-1), None);
        #[cfg(target_os = "linux")]
        assert_eq!(to_evdev(-1, 38), Some(KEY_A));
    }
}
