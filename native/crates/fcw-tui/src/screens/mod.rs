//! Экраны программ. Каждый рисует себя в сетку любого размера от 80×24.

pub(crate) mod furnace;
pub(crate) mod line;
pub(crate) mod litho;
pub(crate) mod puller;
pub(crate) mod saw;
pub(crate) mod top;
pub(crate) mod yield_;

use crate::app::{log_tail, App, Panel};
use crate::grid::{Grid, DIM, FG, RED};
use crate::model::{Machine, State, World};
use crate::text::{mmss, reason_text, Lang};

/// Низ экрана программы машины: хвост журнала в строках `y0..`; в широком терминале журнал уже
/// стоит в правой колонке, и здесь остаётся только ответ на команду.
pub(crate) fn footer(g: &mut Grid, app: &App, w: &World, machine: &str, y0: i32, wd: i32) {
    let y1 = g.rows - 2;
    if g.cols >= 120 {
        if let Some(f) = &w.flash {
            g.put_w(1, y1, &f.text, wd - 2, if f.error { RED } else { FG });
        }
    } else {
        log_tail(g, app, w, Some(machine), 1, y0, y1, wd - 2);
    }
}

/// Нет снимка машины: экран ждёт данные.
pub(crate) fn no_data(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let site = w.machine(&app.machine).is_some_and(|m| m.state == State::Site);
    let t = if site {
        l.t(
            "машина ещё не построена: стройка и пусконаладка идут на пульте",
            "the machine is not built yet: see its panel",
        )
    } else {
        l.t("нет данных от машины", "no data from the machine")
    };
    g.put(2, 4, t, DIM);
}

/// Строки зон загрузки и выхода: `charge  quartz x28  coal x14`.
pub(crate) fn stock_rows(p: &mut Panel, l: Lang, m: &Machine) {
    for z in m.load.iter().chain(m.out.iter()) {
        if z.items.is_empty() {
            p.row(&z.id, l.t("пусто", "empty"), DIM);
        } else {
            p.row(&z.id, &z.text(), FG);
        }
    }
}

/// Строка итогов смены: сколько сделано и сколько простояла (с главной причиной).
pub(crate) fn shift_rows(p: &mut Panel, l: Lang, m: &Machine) {
    let s = &m.shift;
    let down: u32 = s.downtime.iter().sum();
    let mut text = format!("{} {}   {} {}", l.t("сделано", "made"), s.products, l.t("простои", "downtime"), mmss(down));
    if down > 0 {
        let top = (0..3).max_by_key(|i| s.downtime[*i]).unwrap_or(0);
        let why = reason_text(l, ["no_power", "output_full", "no_catalyst"][top]);
        text.push_str(&format!(" ({why})"));
    }
    p.row(l.t("за смену", "shift"), &text, FG);
}
