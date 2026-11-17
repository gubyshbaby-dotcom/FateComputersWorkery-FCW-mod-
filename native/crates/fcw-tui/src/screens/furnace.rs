//! `fcw furnace` — дуговая печь: профиль температуры по фазам, электроды, ванна, шихта.
//! Температура и токи — числа для вида: считаются из хода рецепта и на игру не влияют.

use super::{footer, no_data, shift_rows, stock_rows};
use crate::app::{log_tab, machine_keys, machine_title, off_note, side_column, App, Panel};
use crate::grid::{Grid, ACC, DIM, FG, RED};
use crate::model::{Machine, State, World};
use crate::text::{fe, jitter, mmss, pct, phase_text, signed};

/// Доли такта: розжиг, плавка, выпуск.
const IGNITE: f64 = 0.32;
const TAP: f64 = 0.82;

/// Температура ванны в долях от 1800 °C в точке такта `t` (0..1).
fn profile(t: f64) -> f64 {
    if t < IGNITE {
        0.05 + 0.9 * (t / IGNITE).powf(0.7)
    } else if t < TAP {
        0.93 + 0.015 * ((t * 40.0).sin())
    } else {
        0.95 - 0.5 * (t - TAP) / (1.0 - TAP)
    }
}

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let m = machine_title(g, app, w);
    machine_keys(g, app);
    let Some(m) = m else { return no_data(g, app, w) };
    match app.tab {
        0 => melt(g, app, w, m),
        1 => charge(g, app, w, m),
        _ => log_tab(g, app, w, Some(&m.name)),
    }
}

fn melt(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let running = m.recipe.is_some() && m.state != State::Off;
    let p = m.progress();
    let (cw, ch) = (wd * 5 / 8, g.rows - 11);
    g.frame(0, 2, cw, ch, l.t("Температура ванны, °C", "Bath temperature, °C"));
    let (ax, ay, aw, ah) = (6, 3, cw - 8, ch - 4);
    if m.state == State::Off {
        off_note(g, l, 0, 2, cw, ch);
    } else {
        let vals: Vec<f64> = (0..aw).map(|i| profile(i as f64 / aw as f64)).collect();
        let now = (aw as f64 * p).round() as i32;
        g.area(ax, ay, aw, ah, &vals, FG, Some(if running { now } else { 0 }));
        for (j, lab) in ["1800", "1200", " 600", "   0"].iter().enumerate() {
            g.put(1, ay + (j as i32 * ah + 1) / 3, lab, DIM);
        }
        let (b1, b2) = ((aw as f64 * IGNITE) as i32, (aw as f64 * TAP) as i32);
        let axis: String = (0..=aw)
            .map(|i| match i {
                0 => '├',
                i if i == aw => '┤',
                i if i == b1 || i == b2 => '┼',
                _ => '─',
            })
            .collect();
        g.put(ax, ay + ah, &axis, DIM);
        g.put(ax + 1, ay + ah + 1, &phase_text(l, "ignite"), DIM);
        g.put(ax + b1 + 2, ay + ah + 1, &phase_text(l, "melt"), DIM);
        g.put_w(ax + b2 + 2, ay + ah + 1, &phase_text(l, "tap"), aw - b2 - 1, DIM);
        if running {
            g.put(ax + now.clamp(0, aw), ay + ah, "↑", ACC);
        }
    }
    let x = cw + 2;
    let pw = wd - x - 1;
    let mut pn = Panel { g, x, w: pw, y: 2, max_y: ch + 2, kw: 9 };
    pn.sec(l.t("Электроды", "Electrodes"));
    let amp = match m.phase.as_str() {
        _ if !running || m.state == State::Fault => 0.0,
        "ignite" => 0.6,
        "melt" => 0.8,
        _ => 0.3,
    };
    let t = m.ticks / 6;
    for i in 0..3u32 {
        let v = if amp > 0.0 { (amp + 0.08 * jitter(t + i * 31)).clamp(0.0, 1.0) } else { 0.0 };
        let ka = format!("{:.0} {}", v * 40.0, l.t("кА", "kA"));
        // подпись короткая: полоса начинается ближе, чем у остальных разделов
        let (y, bw) = (pn.y, (pw - 13).clamp(6, 20));
        pn.g.put(x + 1, y, &format!("L{}", i + 1), DIM);
        pn.g.bar(x + 4, y, bw, v, FG);
        pn.g.put(x + 5 + bw, y, &ka, FG);
        pn.y += 1;
    }
    if amp > 0.0 {
        let h: Vec<String> = (0..3u32).map(|i| signed(-(2.0 + jitter(t / 4 + i * 17)).round(), 0)).collect();
        pn.row(l.t("высота", "height"), &format!("{} /16", h.join("  ")), FG);
    } else {
        pn.row(l.t("высота", "height"), l.t("подняты", "raised"), DIM);
    }
    pn.gap();
    pn.sec(l.t("Ванна", "Bath"));
    if running {
        pn.row("T", &format!("{:.0} °C", profile(p) * 1800.0), FG);
        pn.row(l.t("шихта", "charge"), &format!("{:.0} %", (1.0 - (p / TAP).min(1.0)) * 100.0), FG);
        pn.row(l.t("такт", "takt"), &format!("{} / {} t", m.ticks, m.total), FG);
        let tap_at = (m.total as f64 * TAP) as u32;
        let text = if m.ticks >= tap_at {
            l.t("идёт", "now").to_string()
        } else {
            format!("{} {}", l.t("через", "in"), mmss(tap_at - m.ticks))
        };
        pn.row(l.t("выпуск", "tapping"), &text, FG);
    } else {
        pn.row("T", "—", DIM);
        pn.row(l.t("шихта", "charge"), "—", DIM);
        pn.row(l.t("такт", "takt"), "—", DIM);
        pn.row(l.t("выпуск", "tapping"), "—", DIM);
    }
    pn.gap();
    pn.sec(l.t("Питание", "Power"));
    pn.row(l.t("расход", "use"), &format!("{} FE/t", fe(m.usage)), if m.reason == "no_power" { RED } else { FG });
    if m.power_required && m.capacity > 0.0 {
        pn.row(l.t("буфер", "buffer"), &pct(m.energy / m.capacity), FG);
    }
    let y = ch + 3;
    let rows = g.rows;
    let mut pn = Panel { g, x: 0, w: wd, y, max_y: rows - 5, kw: 11 };
    pn.sec(l.t("Загрузка и выход", "Load and output"));
    stock_rows(&mut pn, l, m);
    shift_rows(&mut pn, l, m);
    footer(g, app, w, &m.name, rows - 3, wd);
}

/// Вкладка «Шихта»: что загружено, что вышло, итоги смены.
fn charge(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let rows = g.rows;
    let mut pn = Panel { g, x: 0, w: wd, y: 2, max_y: rows - 5, kw: 11 };
    for (title, zones) in [(l.t("Загружено", "Loaded"), &m.load), (l.t("Выход", "Output"), &m.out)] {
        pn.sec(title);
        let mut any = false;
        for z in zones.iter() {
            for s in &z.items {
                any = true;
                let bw = (wd - 46).clamp(10, 40);
                pn.bar(&z.id, s.count as f64 / 64.0, bw, &format!("{} x{}", s.id, s.count), FG);
            }
        }
        if !any {
            pn.row(l.t("пусто", "empty"), "", DIM);
        }
        pn.gap();
    }
    pn.sec(l.t("Смена", "Shift"));
    shift_rows(&mut pn, l, m);
    footer(g, app, w, &m.name, rows - 3, wd);
}
