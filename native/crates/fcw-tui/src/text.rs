//! Слова и числа на экране: два языка, единицы, время.

use crate::model::{Kind, Machine, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    /// По коду языка Minecraft (`ru_ru`, `en_us`).
    pub fn from_code(code: &str) -> Lang {
        if code.starts_with("ru") {
            Lang::Ru
        } else {
            Lang::En
        }
    }
    pub fn t<'a>(self, ru: &'a str, en: &'a str) -> &'a str {
        match self {
            Lang::Ru => ru,
            Lang::En => en,
        }
    }
}

pub fn kind_name(l: Lang, m: &Machine) -> String {
    match m.kind {
        Kind::Furnace => l.t("дуговая печь", "arc furnace").into(),
        Kind::Puller => l.t("ростовая печь", "crystal puller").into(),
        Kind::Saw => l.t("резка и полировка", "wafering cell").into(),
        Kind::Line => l.t("линия электроники", "electronics line").into(),
        Kind::Litho if m.kind_id == "euv_scanner_mk2" => l.t("литограф EUV Mk2", "EUV scanner Mk2").into(),
        _ => m.kind_id.replace('_', " "),
    }
}

/// Слово состояния для шапки.
pub fn state_badge(l: Lang, m: &Machine) -> &'static str {
    match m.state {
        State::Off => l.t("ВЫКЛ", "OFF"),
        State::Idle => l.t("ОЖИДАНИЕ", "IDLE"),
        State::Run => l.t("РАБОТА", "RUN"),
        State::Fault => l.t("ОТКАЗ", "FAULT"),
        State::Site => match m.state_id.as_str() {
            "build" => l.t("СТРОЙКА", "BUILD"),
            "teardown" => l.t("РАЗБОР", "TEARDOWN"),
            _ => l.t("ПУСКОНАЛАДКА", "COMMISSIONING"),
        },
    }
}

pub fn reason_text(l: Lang, reason: &str) -> String {
    match reason {
        "no_power" => l.t("нет питания", "no power").into(),
        "output_full" => l.t("выход заполнен", "output full").into(),
        "no_catalyst" => l.t("нет маски", "no mask").into(),
        "" => String::new(),
        other => other.replace('_', " "),
    }
}

pub fn phase_text(l: Lang, phase: &str) -> String {
    let (ru, en) = match phase {
        "ignite" => ("розжиг", "ignition"),
        "melt" => ("плавка", "melt"),
        "tap" => ("выпуск", "tapping"),
        "ladle_return" => ("возврат ковша", "ladle return"),
        "heatup" => ("разогрев", "heat-up"),
        "grow" => ("рост", "growth"),
        "unload" => ("выгрузка", "unload"),
        "slice" => ("резка", "slicing"),
        "polish" => ("полировка", "polish"),
        "wafer_load" => ("загрузка пластины", "wafer load"),
        "expose_enter" | "expose" | "expose_exit" => ("экспозиция", "exposure"),
        "takt" => ("такт", "takt"),
        "" | "null" => ("", ""),
        other => return other.replace('_', " "),
    };
    l.t(ru, en).into()
}

/// Состояние одной строкой для таблицы `top`: `работа: плавка`, `отказ: нет питания`.
pub fn state_line(l: Lang, m: &Machine) -> String {
    match m.state {
        State::Off => l.t("выкл.", "off").into(),
        State::Idle => {
            let empty = m.full && m.load.iter().all(|z| z.items.is_empty());
            if empty {
                l.t("ожидание: нет сырья", "idle: no input").into()
            } else {
                l.t("ожидание", "idle").into()
            }
        }
        State::Run => {
            let detail = match &m.litho {
                Some(li) if m.phase.starts_with("expose") && m.defects => {
                    format!("{} {}/61", l.t("поле", "field"), (li.map.chars().count() + 1).min(61))
                }
                _ => phase_text(l, &m.phase),
            };
            if detail.is_empty() {
                l.t("работа", "running").into()
            } else {
                format!("{}: {detail}", l.t("работа", "running"))
            }
        }
        State::Fault => format!("{}: {}", l.t("отказ", "fault"), reason_text(l, &m.reason)),
        State::Site => state_badge(l, m).to_lowercase(),
    }
}

pub fn cause_name(l: Lang, c: char) -> &'static str {
    match c {
        'P' => l.t("частицы", "particles"),
        'W' => l.t("полировка", "polish"),
        'C' => l.t("кристалл", "crystal"),
        'M' => l.t("маска", "mask"),
        'T' => l.t("тепло", "heat"),
        'V' => l.t("вибрация", "vibration"),
        'E' => l.t("питание", "power"),
        _ => "",
    }
}

/// Три буквы причины для крупной карты.
pub fn cause_short(l: Lang, c: char) -> &'static str {
    match c {
        'P' => l.t("ЧАС", "PRT"),
        'W' => l.t("ПОЛ", "POL"),
        'C' => l.t("КРИ", "CRY"),
        'M' => l.t("МАС", "MSK"),
        'T' => l.t("ТЕП", "HOT"),
        'V' => l.t("ВИБ", "VIB"),
        'E' => l.t("ПИТ", "PWR"),
        _ => "",
    }
}

/// Энергия как есть до 9999, дальше тысячами: `2048`, `312k`.
pub fn fe(v: f64) -> String {
    if v < 10_000.0 {
        format!("{:.0}", v.max(0.0))
    } else {
        fek(v)
    }
}

/// Коротко для сводок: `4.1k`, `312k`, `256`.
pub fn fek(v: f64) -> String {
    let v = v.max(0.0);
    if v >= 100_000.0 {
        format!("{:.0}k", v / 1000.0)
    } else if v >= 1000.0 {
        format!("{:.1}k", v / 1000.0)
    } else {
        format!("{v:.0}")
    }
}

/// Вычислительные единицы: `8`, `0.5`.
pub fn cu(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// Тики как `м:сс` (20 тиков в секунде).
pub fn mmss(ticks: u32) -> String {
    let s = ticks / 20;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn pct(frac: f64) -> String {
    format!("{:.0} %", (frac * 100.0).round())
}

/// Имя зоны покороче: `wafer_dock_1` → `dock1`.
pub fn zone_label(id: &str) -> String {
    id.replace("wafer_dock_", "dock")
}

/// Детерминированный «шум» в -1..1: числа для вида (доза, токи) не должны прыгать случайно.
pub fn jitter(seed: u32) -> f64 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x % 2001) as f64 / 1000.0 - 1.0
}

/// Минус типографский: в шрифте мода он есть, а на экране ровнее дефиса.
pub fn signed(v: f64, digits: usize) -> String {
    if v < 0.0 {
        format!("−{:.*}", digits, -v)
    } else {
        format!("+{v:.digits$}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert_eq!(fe(4096.0), "4096");
        assert_eq!(fek(4096.0), "4.1k");
        assert_eq!(fe(312_000.0), "312k");
        assert_eq!(fe(256.0), "256");
        assert_eq!(cu(8.0), "8");
        assert_eq!(cu(0.5), "0.5");
        assert_eq!(mmss(720), "0:36");
        assert_eq!(pct(0.385), "39 %");
        assert_eq!(signed(-2.0, 0), "−2");
        assert!((-1.0..=1.0).contains(&jitter(42)));
    }
}
