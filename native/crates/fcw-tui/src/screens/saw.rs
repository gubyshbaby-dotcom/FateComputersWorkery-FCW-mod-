//! `fcw saw` — резка и полировка: слиток по резам, таблица пластин.
//! Какой рез идёт и сколько проходов полировки сделано, считается из хода рецепта (для вида);
//! вода, паузы питания и класс полировки — как в моде.

use super::{footer, no_data, shift_rows};
use crate::app::{apc_row, log_tab, machine_keys, machine_title, off_note, side_column, App, Panel};
use crate::grid::{fit, Grid, ACC, DIM, FAINT, FG, RED};
use crate::model::{Machine, State, World};
use crate::screens::puller::{grade, grade_note};
use crate::text::{cu, jitter, pct};

const WAFERS: usize = 8;
/// Доля такта на один рез и на один проход полировки.
const CUT: f64 = 0.9 / WAFERS as f64;

#[derive(Clone, Copy, PartialEq)]
enum Cut {
    Wait,
    Now,
    Done,
}

/// Состояние пластины `i` при ходе `p`: рез и число проходов полировки (0..3).
fn wafer_state(i: usize, p: f64, running: bool) -> (Cut, u32) {
    if !running {
        return (Cut::Wait, 0);
    }
    let end = (i + 1) as f64 * CUT;
    if p < i as f64 * CUT {
        return (Cut::Wait, 0);
    }
    if p < end {
        return (Cut::Now, 0);
    }
    let pass = ((1.0 - end) / 3.0).clamp(0.01, 0.1);
    (Cut::Done, (((p - end) / pass) as u32).min(3))
}

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let m = machine_title(g, app, w);
    machine_keys(g, app);
    let Some(m) = m else { return no_data(g, app, w) };
    match app.tab {
        0 => wafers(g, app, w, m),
        1 => tool(g, app, w, m),
        _ => log_tab(g, app, w, Some(&m.name)),
    }
}

fn wafers(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let running = m.recipe.is_some() && m.state != State::Off;
    let p = m.progress();
    let title = match &m.recipe {
        Some(r) => format!("{}: {r}", l.t("Слиток", "Boule")),
        None => l.t("Слиток", "Boule").to_string(),
    };
    g.frame(0, 2, wd, 8, &title);
    let sw = (wd - 16) / WAFERS as i32;
    if m.state == State::Off {
        off_note(g, l, 0, 2, wd, 8);
    } else {
        for i in 0..WAFERS {
            let x = 7 + i as i32 * sw;
            let (cut, _) = wafer_state(i, p, running);
            let fg = match cut {
                Cut::Done => FG,
                Cut::Now => ACC,
                Cut::Wait => FAINT,
            };
            for r in 0..4 {
                g.put(x, 4 + r, &"█".repeat((sw - 1) as usize), fg);
            }
            g.put(x + sw - 1, 4, "│", DIM);
            g.put(x + sw - 1, 7, "│", DIM);
            g.put(x + sw / 2 - 2, 8, &format!(" {} ", i + 1), DIM);
            if cut == Cut::Now {
                g.put(x, 3, l.t("↓ пила", "↓ saw"), ACC);
            }
        }
        g.put(1, 5, l.t("хвост", "tail"), DIM);
        g.put(7 + WAFERS as i32 * sw + 1, 5, l.t("затр.", "seed"), DIM);
    }
    let class = grade(m);
    let head = format!(
        " №  {} {} {} {} {}",
        fit(l.t("резка", "cut"), 7),
        fit(l.t("полировка", "polish"), 16),
        fit(l.t("толщина", "thickness"), 9),
        fit("TTV", 8),
        l.t("класс", "class")
    );
    g.put_w(1, 11, &head, wd - 26, DIM);
    g.rule(1, 12, wd - 26, "");
    for i in 0..WAFERS {
        let y = 13 + i as i32;
        if y > g.rows - 4 {
            break;
        }
        let (cut, pol) = wafer_state(i, p, running);
        let word = match cut {
            Cut::Done => "ok",
            Cut::Now => l.t("идёт", "now"),
            Cut::Wait => "—",
        };
        g.put(
            1,
            y,
            &format!(" {}  {}", i + 1, fit(word, 6)),
            if cut == Cut::Now {
                ACC
            } else if cut == Cut::Wait {
                DIM
            } else {
                FG
            },
        );
        let polishing = cut == Cut::Done && pol < 3 && pol > 0;
        g.bar(13, y, 9, pol as f64 / 3.0, if polishing { ACC } else { FG });
        g.put(23, y, &format!("{pol}/3"), if pol == 0 { DIM } else { FG });
        if pol == 3 {
            let um = l.t("мкм", "µm");
            let th = 775.0 + (jitter(i as u32 * 7 + 3) * 1.4).round();
            let ttv = 1.1 + 0.3 * jitter(i as u32 * 13 + 5);
            g.put(
                30,
                y,
                &format!("{} {} {class}", fit(&format!("{th:.0} {um}"), 9), fit(&format!("{ttv:.1} {um}"), 8)),
                FG,
            );
        } else {
            g.put(30, y, &format!("{} {}", fit("—", 9), fit("—", 8)), DIM);
        }
    }
    let x = wd - 23;
    let max_y = g.rows - 4;
    let mut pn = Panel { g, x, w: 22, y: 11, max_y, kw: 10 };
    pn.sec(l.t("Условия", "Conditions"));
    pn.gap();
    pn.row(l.t("вода", "water"), if m.water { l.t("есть", "on") } else { l.t("нет", "none") }, FG);
    pn.row(
        l.t("питание", "power"),
        &format!("{} {}", l.t("пауз", "pauses"), m.power_pauses),
        if m.reason == "no_power" { RED } else { FG },
    );
    if m.apc.need > 0.0 {
        let note = if m.apc.no_service() { l.t(" без сл.", " no svc") } else { "" };
        pn.row("APC", &format!("{}/{} CU{note}", cu(m.apc.got()), cu(m.apc.need)), FG);
        // девятая пластина со слитка выходит с шансом, равным покрытию APC
        let c = m.apc.coverage.clamp(0.0, 1.0);
        let extra = if c >= 0.995 { format!("8 + 1 {}", l.t("шт.", "pcs")) } else { format!("8 (+1: {})", pct(c)) };
        pn.row(l.t("выход", "output"), &extra, FG);
    } else {
        pn.row(l.t("выход", "output"), &format!("8 {}", l.t("шт.", "pcs")), FG);
    }
    pn.gap();
    pn.row(l.t("полировка", "polish"), &format!("{} {class}", l.t("класс", "class")), FG);
    if running {
        pn.row(l.t("такт", "takt"), &format!("{}/{} t", m.ticks, m.total), FG);
    }
    footer(g, app, w, &m.name, g.rows - 3, wd);
}

/// Вкладка «Инструмент»: вода, паузы, итоги смены, загрузка и выход.
fn tool(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let rows = g.rows;
    let mut p = Panel { g, x: 0, w: wd, y: 2, max_y: rows - 5, kw: 12 };
    p.sec(l.t("Резка и полировка", "Slicing and polishing"));
    p.row(
        l.t("вода", "water"),
        if m.water {
            l.t("есть, 1 мБ за тик работы", "on, 1 mB per working tick")
        } else {
            l.t("нет: класс полировки ниже", "none: the polish class drops")
        },
        FG,
    );
    p.row(
        l.t("паузы", "pauses"),
        &format!("{} {}", m.power_pauses, l.t("за этот слиток (по питанию)", "on this boule (power)")),
        FG,
    );
    p.row(l.t("полировка", "polish"), &format!("{} {}   {}", l.t("класс", "class"), grade(m), grade_note(l, m)), FG);
    apc_row(&mut p, l, m, "");
    p.gap();
    p.sec(l.t("Загрузка и выход", "Load and output"));
    super::stock_rows(&mut p, l, m);
    p.gap();
    p.sec(l.t("Смена", "Shift"));
    shift_rows(&mut p, l, m);
    footer(g, app, w, &m.name, rows - 3, wd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wafers_follow_the_takt() {
        // 704/800: семь резов готовы, восьмой идёт, шестая пластина в полировке
        let p = 0.88;
        assert!(wafer_state(6, p, true).0 == Cut::Done);
        assert!(wafer_state(7, p, true).0 == Cut::Now);
        assert_eq!(wafer_state(0, p, true).1, 3);
        assert_eq!(wafer_state(5, p, true).1, 2);
        assert!(wafer_state(0, p, false).0 == Cut::Wait);
        // к концу такта отполированы все
        assert_eq!(wafer_state(7, 0.9999, true).1, 2);
        assert_eq!(wafer_state(6, 1.0, true).1, 3);
    }
}
