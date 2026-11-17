//! `fcw line` — линия электроники: два участка цепочками позиций, такт, выход.
//! Изделие на средних позициях — по ходу такта (для вида); вход, выход и такт — из снимка машины.

use super::{footer, no_data, shift_rows};
use crate::app::{apc_row, log_tab, machine_keys, machine_title, side_column, App, Panel};
use crate::grid::{width, Grid, ACC, BG, DIM, FAINT, FG, RED};
use crate::model::{Machine, Section, State, World};
use crate::screens::litho::class_mult;
use crate::text::{reason_text, Lang};

/// Участок линии: его рецепт и ход. Мост старой версии участки не присылает — тогда первый берётся из общих полей.
fn section(m: &Machine, id: &str, first: bool) -> Section {
    if let Some(s) = m.sections.iter().find(|s| s.id == id) {
        return s.clone();
    }
    if first && m.sections.is_empty() {
        let state = if m.recipe.is_some() { "working" } else { "idle" };
        return Section {
            id: id.into(),
            state: state.into(),
            recipe: m.recipe.clone(),
            ticks: m.ticks,
            total: m.total,
        };
    }
    Section { id: id.into(), state: "idle".into(), ..Section::default() }
}

/// Цепочка позиций участка с числом изделий на каждой, полоса такта и состояние.
#[allow(clippy::too_many_arguments)]
fn strip(
    g: &mut Grid,
    l: Lang,
    y: i32,
    wd: i32,
    title: &str,
    names: &[&str],
    first: u32,
    last: u32,
    s: &Section,
    off: bool,
) {
    let head = match &s.recipe {
        Some(r) => format!("{title}   {r}"),
        None => title.to_string(),
    };
    g.rule(0, y, wd, &head);
    let working = s.state == "working" && s.total > 0 && !off;
    let frac = if s.total > 0 { s.ticks as f64 / s.total as f64 } else { 0.0 };
    let inner = names.len() - 2;
    let active = working.then(|| 1 + ((frac * inner as f64) as usize).min(inner - 1));
    let mut x = 1;
    for (i, n) in names.iter().enumerate() {
        let lab = format!(" {n} ");
        let wl = width(&lab);
        if active == Some(i) {
            g.put_on(x, y + 2, &lab, BG, ACC);
        } else {
            g.put_on(x, y + 2, &lab, FG, FAINT);
        }
        let count = if i == 0 {
            first.to_string()
        } else if i + 1 == names.len() {
            last.to_string()
        } else if active == Some(i) {
            "1".to_string()
        } else {
            "·".to_string()
        };
        let fg = if count == "·" || count == "0" { DIM } else { FG };
        g.put(x + (wl - width(&count)) / 2, y + 3, &count, fg);
        x += wl;
        if i + 1 < names.len() {
            g.put(x, y + 2, "→", DIM);
            x += 1;
        }
    }
    g.put(1, y + 5, l.t("такт", "takt"), DIM);
    let bw = (wd - 50).max(30);
    let fault = matches!(s.state.as_str(), "no_power" | "output_full" | "no_catalyst");
    g.bar(7, y + 5, bw, if working || fault { frac } else { 0.0 }, if fault { RED } else { ACC });
    if s.total > 0 {
        g.put(8 + bw, y + 5, &format!("{} / {} t", s.ticks, s.total), FG);
    }
    let note = match s.state.as_str() {
        _ if off => (l.t("рубильник выключен", "switched off").to_string(), DIM),
        "working" => (String::new(), FG),
        st if fault => (reason_text(l, st), RED),
        _ if first == 0 => (l.t("ожидание: нет сырья", "idle: no input").to_string(), DIM),
        _ => (l.t("ожидание", "idle").to_string(), DIM),
    };
    g.put_w(8 + bw + 16, y + 5, &note.0, wd - bw - 25, note.1);
}

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let m = machine_title(g, app, w);
    machine_keys(g, app);
    let Some(m) = m else { return no_data(g, app, w) };
    match app.tab {
        0 => line(g, app, w, m),
        1 => yield_tab(g, app, w, m),
        _ => log_tab(g, app, w, Some(&m.name)),
    }
}

fn zone_count(m: &Machine, input: bool, id: &str) -> u32 {
    let zones = if input { &m.load } else { &m.out };
    Machine::zone(zones, id).map_or(0, |z| z.count())
}

fn line(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    let off = m.state == State::Off;
    let pack = section(m, "packaging", true);
    let names: [&str; 7] = [
        l.t("вход", "in"),
        l.t("резка", "dicing"),
        l.t("посадка", "attach"),
        l.t("разварка", "bonding"),
        l.t("корпус", "molding"),
        l.t("тест", "test"),
        l.t("лоток", "tray"),
    ];
    strip(
        g,
        l,
        2,
        wd,
        l.t("Корпусировка", "Packaging"),
        &names,
        zone_count(m, true, "wafer"),
        zone_count(m, false, "chips"),
        &pack,
        off,
    );
    let asm = section(m, "assembly", false);
    let names: [&str; 6] = [
        l.t("магазин", "magazine"),
        l.t("паста", "paste"),
        l.t("установка", "placement"),
        l.t("печь", "oven"),
        l.t("контроль", "inspection"),
        l.t("выход", "out"),
    ];
    strip(
        g,
        l,
        9,
        wd,
        l.t("Сборка плат", "Board assembly"),
        &names,
        zone_count(m, true, "boards"),
        zone_count(m, false, "out"),
        &asm,
        off,
    );
    let rows = g.rows;
    let mut p = Panel { g, x: 0, w: wd, y: 16, max_y: rows - 5, kw: 11 };
    p.sec(l.t("Выход, эта смена", "Output, this shift"));
    for z in &m.out {
        if z.items.is_empty() {
            p.row(&z.id, l.t("пусто", "empty"), DIM);
        } else {
            p.row(&z.id, &z.text(), FG);
        }
    }
    shift_rows(&mut p, l, m);
    let gain = format!("{} −{:.0} %", l.t("время", "time"), 20.0 * m.apc.coverage.clamp(0.0, 1.0));
    apc_row(&mut p, l, m, &gain);
    footer(g, app, w, &m.name, rows - 3, wd);
}

/// Вкладка «Годность»: что даёт корпусировка с пластины каждого класса.
fn yield_tab(g: &mut Grid, app: &App, w: &World, m: &Machine) {
    let l = app.lang;
    let wd = side_column(g, app, w, m);
    g.rule(0, 2, wd, l.t("Выход корпусировки по классу пластины", "Packaging output by wafer class"));
    let head = format!(
        "{}  {}  {}",
        l.t("класс", "class"),
        l.t("годных полей", "good fields "),
        l.t("выход от номинала", "share of nominal")
    );
    g.put(1, 3, &head, DIM);
    for (i, (c, range)) in
        [("S", "58–61"), ("A", "52–57"), ("B", "43–51"), ("C", "31–42"), ("D", "0–30")].iter().enumerate()
    {
        let y = 4 + i as i32;
        let k = class_mult(c);
        g.put(1, y, &format!("  {c}    {range}"), FG);
        g.bar(22, y, 25, k / 1.25, FG);
        g.put(48, y, &format!("×{k:.2}"), FG);
    }
    g.put(
        1,
        10,
        l.t(
            "дробная часть разыгрывается: ×1.25 от двух чипов — это два и ещё один с шансом 50 %",
            "the fraction is rolled: ×1.25 of two chips is two and one more at 50 %",
        ),
        DIM,
    );
    let rows = g.rows;
    let mut p = Panel { g, x: 0, w: wd, y: 12, max_y: rows - 5, kw: 11 };
    p.sec(l.t("Сейчас", "Now"));
    super::stock_rows(&mut p, l, m);
    shift_rows(&mut p, l, m);
    footer(g, app, w, &m.name, rows - 3, wd);
}
