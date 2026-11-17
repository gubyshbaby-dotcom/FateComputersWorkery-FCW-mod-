//! `fcw puller` — ростовая печь: слиток в разрезе по длине, диаметр, условия, прогноз класса.
//! Длина, диаметр и температура — числа для вида; класс кристалла считается как в моде.

use super::{no_data, shift_rows};
use crate::app::{apc_row, log_tab, log_tail, machine_keys, machine_title, off_note, side_column, App, Panel};
use crate::grid::{rfit, Grid, ACC, DIM, FAINT, FG, RED};
use crate::model::{Machine, State, World};
use crate::text::{fe, jitter, mmss, pct, Lang};

const LENGTH_MM: f64 = 340.0;

/// Доля выращенной длины: разогрев и выгрузка роста не дают.
fn grown(m: &Machine) -> f64 {
    if m.recipe.is_none() {
        return 0.0;
    }
    match m.phase.as_str() {
        "heatup" => 0.0,
        "unload" => 1.0,
        _ => ((m.progress() - 0.1) / 0.8).clamp(0.0, 1.0),
    }
}

/// Ширина слитка по строкам сверху вниз: затравка, шейка и плечо, тело, хвост.
fn shape(n: usize) -> Vec<i32> {
    let seed = (n / 6).max(1);
    let mut out = vec![1; seed];
    out.extend([3, 7, 11]);
    let body = n.saturating_sub(seed + 5);
    out.extend(std::iter::repeat_n(13, body));
    out.extend([11, 5]);
    out.truncate(n);
    out
}

/// Класс слитка: A — вода и без остановок по питанию, B — одно нарушение, C — два.
pub(crate) fn grade(m: &Machine) -> char {
    match (m.water, m.power_pauses == 0) {
        (true, true) => 'A',
        (false, false) => 'C',
        _ => 'B',
    }
}

pub(crate) fn grade_note(l: Lang, m: &Machine) -> String {
    let water = if m.water { l.t("вода", "water") } else { l.t("без воды", "no water") };
    let power = if m.power_pauses == 0 {
        l.t("без пауз", "no pauses").to_string()
    } else {
        format!("{} {}", l.t("пауз", "pauses"), m.power_pauses)
    };
    format!("({water} + {power})")
}

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let m = machine_title(g, app, w);
    machine_keys(g, app);
    let Some(m) = m else { return no_data(g, app, w) };
    match app.tab {
        0 => growth(g, app, w, m),
        1 => heat(g, app, w, m),
        _ => log_tab(g, app, w, Some(&m.name)),
    }
}

fn growth(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let fh = g.rows - 4;
    g.frame(0, 2, 30, fh, l.t("Слиток, мм", "Boule, mm"));
    let frac = grown(m);
    let n = (fh - 3) as usize;
    let widths = shape(n);
    let at = (frac * (n - 1) as f64).round() as usize;
    if m.state == State::Off {
        off_note(g, l, 0, 2, 30, fh);
    } else {
        let cx = 16;
        for (i, wdt) in widths.iter().enumerate() {
            let y = 4 + i as i32;
            let mm = (i as f64 * LENGTH_MM / (n - 1) as f64).round();
            g.put(2, y, &rfit(&format!("{mm:.0}"), 3), DIM);
            g.put(6, y, "┤", DIM);
            let block = "█".repeat(*wdt as usize);
            g.put(cx - wdt / 2, y, &block, if i < at { FG } else { FAINT });
            if i == at && m.recipe.is_some() {
                // мениск: линия расплава, под ней слиток ещё не вырос
                g.put(cx - 9, y, &"~".repeat(16), ACC);
                g.put(cx - wdt / 2, y, &"▀".repeat(*wdt as usize), FG);
                g.put(23, y, l.t("мениск", "melt"), ACC);
            }
        }
        g.put(8, 3, l.t("затравка", "seed"), DIM);
    }
    let x = 32;
    let pw = wd - x - 1;
    let max_y = g.rows - 6;
    let mut p = Panel { g, x, w: pw, y: 2, max_y, kw: 12 };
    p.sec(l.t("Вытягивание", "Pulling"));
    let running = m.recipe.is_some();
    let len = format!("{:.0} / {LENGTH_MM:.0} {}", frac * LENGTH_MM, l.t("мм", "mm"));
    let y = p.y;
    p.row(l.t("длина", "length"), &len, FG);
    p.g.bar(x + 26, y, (pw - 33).clamp(6, 14), frac, ACC);
    p.g.put(x + 27 + (pw - 33).clamp(6, 14), y, &pct(frac), FG);
    if running && m.state == State::Run {
        let t = m.ticks / 10;
        let dia = 150.0 + 0.4 * jitter(t);
        p.row(
            l.t("диаметр", "diameter"),
            &format!("{dia:.1} {}   {} 150 ±1", l.t("мм", "mm"), l.t("цель", "target")),
            FG,
        );
        let text =
            format!("1.2 {}   {} 12 {}", l.t("мм/мин", "mm/min"), l.t("вращение", "rotation"), l.t("об/мин", "rpm"));
        p.row(l.t("скорость", "rate"), &text, FG);
    } else {
        p.row(l.t("диаметр", "diameter"), "—", DIM);
        p.row(l.t("скорость", "rate"), "—", DIM);
    }
    if running {
        let left = mmss(m.total.saturating_sub(m.ticks));
        p.row(l.t("такт", "takt"), &format!("{} / {} t   {} {left}", m.ticks, m.total, l.t("осталось", "left")), FG);
    } else {
        p.row(l.t("такт", "takt"), "—", DIM);
    }
    p.gap();
    p.sec(l.t("Диаметр по длине", "Diameter along the length"));
    let prof =
        [1, 1, 2, 9, 30, 70, 100, 101, 100, 99, 100, 100, 101, 100, 100, 99, 100, 100, 101, 100, 100, 100, 99, 100];
    let shown = (prof.len() as f64 * frac).ceil() as usize;
    let vals: Vec<f64> = prof.iter().take(shown).map(|v| *v as f64).collect();
    let y = p.y;
    p.g.spark(x + 1, y, &vals, FG, 0.0, 105.0);
    p.g.put_w(x + 27, y, l.t("шейка  плечо  тело", "neck  shoulder  body"), pw - 27, DIM);
    p.y += 2;
    p.sec(l.t("Условия", "Conditions"));
    if running && m.state == State::Run {
        p.row(l.t("расплав", "melt"), &format!("{:.0} °C   ±0.4", 1420.0 + jitter(m.ticks / 20)), FG);
    } else {
        p.row(l.t("расплав", "melt"), "—", DIM);
    }
    p.row(
        l.t("охлаждение", "cooling"),
        if m.water { l.t("вода есть", "water on") } else { l.t("нет воды", "no water") },
        FG,
    );
    let pauses = format!("{} FE/t   {} {}", fe(m.usage), l.t("пауз", "pauses"), m.power_pauses);
    p.row(l.t("питание", "power"), &pauses, if m.reason == "no_power" { RED } else { FG });
    let gain = format!("{} −{:.0} %", l.t("время", "time"), 20.0 * m.apc.coverage.clamp(0.0, 1.0));
    apc_row(&mut p, l, m, &gain);
    p.gap();
    p.sec(l.t("Прогноз", "Forecast"));
    p.row(l.t("кристалл", "crystal"), &format!("{} {}   {}", l.t("класс", "class"), grade(m), grade_note(l, m)), FG);
    shift_rows(&mut p, l, m);
    if g.cols < 120 {
        log_tail(g, app, w, Some(&m.name), x + 1, g.rows - 4, g.rows - 3, pw - 1);
    } else if let Some(f) = &w.flash {
        g.put_w(x + 1, g.rows - 3, &f.text, pw - 1, if f.error { RED } else { FG });
    }
}

/// Вкладка «Тепло»: температура расплава и скорость вытягивания по ходу такта.
fn heat(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let ch = g.rows - 12;
    g.frame(0, 2, wd, ch, l.t("Температура расплава, °C", "Melt temperature, °C"));
    let (ax, ay, aw, ah) = (7, 3, wd - 9, ch - 3);
    let temp = |t: f64| {
        if t < 0.1 {
            0.2 + 7.0 * t
        } else if t < 0.9 {
            0.9 + 0.02 * (t * 30.0).sin()
        } else {
            0.9 - 5.0 * (t - 0.9)
        }
    };
    let vals: Vec<f64> = (0..aw).map(|i| temp(i as f64 / aw as f64)).collect();
    let running = m.recipe.is_some() && m.state != State::Off;
    let now = (aw as f64 * m.progress()).round() as i32;
    if m.state == State::Off {
        off_note(g, l, 0, 2, wd, ch);
    } else {
        g.area(ax, ay, aw, ah, &vals, FG, Some(if running { now } else { 0 }));
        for (j, lab) in ["1500", "1000", " 500"].iter().enumerate() {
            g.put(2, ay + (j as i32 * ah) / 3, lab, DIM);
        }
        g.put(ax, ay + ah, &"─".repeat(aw as usize), DIM);
        if running {
            g.put(ax + now.clamp(0, aw - 1), ay + ah, "↑", ACC);
        }
    }
    let y = ch + 3;
    g.rule(0, y, wd, l.t("Скорость вытягивания, мм/мин", "Pull rate, mm/min"));
    let rate: Vec<f64> = (0..aw)
        .map(|i| i as f64 / aw as f64)
        .map(|t| if (0.1..0.9).contains(&t) { 1.2 + 0.1 * (t * 50.0).sin() } else { 0.0 })
        .collect();
    let shown = if running { now.clamp(0, aw) as usize } else { 0 };
    g.spark(ax, y + 1, &rate[..shown], FG, 0.0, 1.5);
    g.put(1, y + 1, "1.2", DIM);
    let rows = g.rows;
    let mut p = Panel { g, x: 0, w: wd, y: y + 3, max_y: rows - 5, kw: 12 };
    p.sec(l.t("Условия", "Conditions"));
    p.row(
        l.t("охлаждение", "cooling"),
        if m.water { l.t("вода есть", "water on") } else { l.t("нет воды", "no water") },
        FG,
    );
    p.row(l.t("кристалл", "crystal"), &format!("{} {}   {}", l.t("класс", "class"), grade(m), grade_note(l, m)), FG);
    super::footer(g, app, w, &m.name, rows - 3, wd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_matches_the_mockup() {
        assert_eq!(shape(18), vec![1, 1, 1, 3, 7, 11, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 11, 5]);
        assert_eq!(shape(17).len(), 17);
    }
}
