//! `fcw yield` — отчёт за смену: годность по рецептам, причины брака с подсказкой, что исправить,
//! тренд по пластинам, простои. Данные — ответ мира на `machine_req` с `op` = `yield`.

use crate::app::{clock, keys, tabs, title, App};
use crate::grid::{fit, rfit, width, Grid, ACC_D, DIM, FAINT, FG, RED};
use crate::model::{Kind, ReportMachine, Shift, Wafer, World, CAUSES, CLASSES, FIELDS};
use crate::screens::litho::{causes_text, wafer};
use crate::text::{cause_name, mmss, pct, reason_text, Lang};

/// Машины отчёта с учётом выбора «Машина» (F3).
fn chosen<'a>(app: &App, w: &'a World) -> Vec<&'a ReportMachine> {
    let all = w.report.as_deref().unwrap_or(&[]);
    match app.only {
        0 => all.iter().collect(),
        n => all.iter().skip(n - 1).take(1).collect(),
    }
}

fn shift_of<'a>(app: &App, m: &'a ReportMachine) -> &'a Shift {
    if app.yesterday {
        &m.yesterday
    } else {
        &m.today
    }
}

/// Последние пластины выбранных машин, новые первыми: (машина, пластина).
pub(crate) fn wafer_rows<'a>(app: &App, w: &'a World) -> Vec<(&'a str, &'a Wafer)> {
    let mut out: Vec<(&str, &Wafer)> = chosen(app, w)
        .iter()
        .flat_map(|m| m.litho.iter().flat_map(move |li| li.wafers.iter().map(move |wf| (m.name.as_str(), wf))))
        .collect();
    out.sort_by_key(|(_, wf)| std::cmp::Reverse(wf.time));
    out
}

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let who = match (app.only, chosen(app, w).first()) {
        (0, _) | (_, None) => String::new(),
        (_, Some(m)) => format!("  {}", m.name),
    };
    let left = format!("fcw yield  {}{who}", l.t("отчёт за смену", "shift report"));
    let right = if app.yesterday {
        format!("{} {}  {}", l.t("день", "day"), w.day.saturating_sub(1), l.t("вчера", "yesterday"))
    } else {
        let now = clock(w);
        format!("{} {}  06:00–{}", l.t("день", "day"), w.day, &now[..5.min(now.len())])
    };
    let lost = w.stale.then_some((l.t("НЕТ СВЯЗИ", "NO LINK"), RED));
    title(g, &left, &right, lost);
    tabs(g, &app.program.tabs(l), app.tab);
    keys(
        g,
        &[
            ("F1", l.t("Справка", "Help")),
            ("F2", l.t("Смена", "Shift")),
            ("F3", l.t("Машина", "Machine")),
            ("F8", l.t("В файл", "To file")),
            ("F9", "top"),
            ("F10", l.t("Выход", "Quit")),
        ],
    );
    if w.report.is_none() {
        g.put(2, 4, l.t("запрос отчёта у машин сети", "asking the machines for the report"), DIM);
        return;
    }
    match app.tab {
        0 => summary(g, app, w),
        1 => wafers_tab(g, app, w),
        2 => causes_tab(g, app, w),
        _ => downtime_tab(g, app, w),
    }
    if let Some(f) = &w.flash {
        g.fill(0, g.rows - 2, g.cols, 1, crate::grid::BG);
        g.put_w(1, g.rows - 2, &f.text, g.cols - 2, if f.error { RED } else { FG });
    }
}

/// Сумма смен выбранных машин.
fn total(app: &App, w: &World) -> Shift {
    let mut t = Shift::default();
    for m in chosen(app, w) {
        let s = shift_of(app, m);
        t.wafers += s.wafers;
        t.good_fields += s.good_fields;
        t.products += s.products;
        for i in 0..7 {
            t.causes[i] += s.causes[i];
        }
        for i in 0..5 {
            t.classes[i] += s.classes[i];
        }
        for i in 0..3 {
            t.downtime[i] += s.downtime[i];
        }
    }
    t
}

/// Причины по убыванию числа полей: (буква, полей).
fn ranked(s: &Shift) -> Vec<(char, u32)> {
    let mut v: Vec<(char, u32)> = CAUSES.chars().enumerate().map(|(i, c)| (c, s.causes[i])).collect();
    // при равенстве — в привычном порядке макета
    let order = |c: char| "PWEMCTV".find(c).unwrap_or(9);
    v.sort_by(|a, b| b.1.cmp(&a.1).then(order(a.0).cmp(&order(b.0))));
    v
}

/// Что исправить, чтобы причины `c` стало меньше: по тому, что известно о машинах.
fn hint(l: Lang, c: char, app: &App, w: &World) -> String {
    let lithos: Vec<_> = chosen(app, w).into_iter().filter_map(|m| m.litho.as_ref()).collect();
    let no_service = w.machines.iter().any(|m| m.kind == Kind::Litho && m.apc.need > 0.0 && m.apc.no_service());
    let apc = || {
        if no_service {
            l.t("служба APC не запущена: fcw apc on", "APC service is off: fcw apc on").to_string()
        } else {
            String::new()
        }
    };
    match c {
        'P' => match lithos.iter().map(|li| li.iso).min() {
            Some(0) => l.t("открыто: нужны стены и потолок", "open room: build walls and a roof").into(),
            Some(8) => l.t("ISO 8: поставить фильтры", "ISO 8: add filters").into(),
            Some(5) => l.t("ISO 5: чище помещение не сделать", "ISO 5: the room cannot be cleaner").into(),
            Some(n) => format!("ISO {n}: {} ISO {}", l.t("поднять до", "raise to"), n - 1),
            None => String::new(),
        },
        'W' => {
            if w.machines.iter().any(|m| m.kind == Kind::Saw && m.full && !m.water) {
                l.t("дать воду резке", "give the wafering cell water").into()
            } else {
                apc()
            }
        }
        'C' => {
            let pullers: Vec<_> = w.machines.iter().filter(|m| m.kind == Kind::Puller && m.full).collect();
            if pullers.iter().any(|m| !m.water) {
                l.t("дать воду ростовой печи", "give the crystal puller water").into()
            } else if pullers.iter().any(|m| m.power_pauses > 0) {
                l.t("ростовой печи нужно ровное питание", "the crystal puller needs steady power").into()
            } else {
                String::new()
            }
        }
        'E' => {
            let n: u32 = lithos.iter().map(|li| li.brownouts).sum();
            match (l, n) {
                (_, 0) => l.t("литограф терял питание: нужен запас", "the scanner lost power: add a buffer").into(),
                (Lang::Ru, n) => format!("просадок: {n}, нужен запас энергии"),
                (Lang::En, n) => format!("brownouts: {n}, add an energy buffer"),
            }
        }
        'T' => {
            if lithos.iter().any(|li| !li.coolant) {
                l.t("нет воды в coolant_in литографа", "no water in the scanner's coolant_in").into()
            } else {
                apc()
            }
        }
        'M' => {
            let worn = lithos
                .iter()
                .filter_map(|li| li.mask.as_ref())
                .filter(|k| k.max > 0)
                .max_by_key(|k| k.used * 1000 / k.max);
            match worn {
                Some(k) if k.used * 2 > k.max => {
                    format!("{} {} {}", k.id, l.t("изношена на", "is worn by"), pct(k.used as f64 / k.max as f64))
                }
                _ => apc(),
            }
        }
        'V' => {
            if lithos.iter().any(|li| li.vibration) {
                l.t("рядом тяжёлая машина: отодвинуть", "a heavy machine is near: move it").into()
            } else {
                apc()
            }
        }
        _ => String::new(),
    }
}

fn summary(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let wd = g.cols - 2;
    let rows = wafer_rows(app, w);
    let head = format!(
        "{} {} {}  S  A  B  C  D   {}",
        fit(l.t("рецепт", "recipe"), 17),
        fit(l.t("пластин", "wafers"), 8),
        fit(l.t("годных", "good"), 6),
        l.t("по пластинам (последние)", "by wafer (recent)")
    );
    g.put_w(1, 3, &head, wd, DIM);
    g.rule(1, 4, wd, "");
    // по рецептам: из карт последних пластин (литограф хранит их, а за смену — только суммы)
    let mut recipes: Vec<&str> = Vec::new();
    for (_, wf) in rows.iter().rev() {
        if !recipes.contains(&wf.recipe.as_str()) {
            recipes.push(&wf.recipe);
        }
    }
    let causes_y = (g.rows * 2 / 5).max(10);
    if recipes.is_empty() {
        g.put(1, 5, l.t("готовых пластин ещё нет", "no finished wafers yet"), DIM);
    }
    for (i, r) in recipes.iter().enumerate().take((causes_y - 6) as usize) {
        let y = 5 + i as i32;
        let ws: Vec<&Wafer> = rows.iter().rev().filter(|(_, wf)| wf.recipe == *r).map(|(_, wf)| *wf).collect();
        let good: u32 = ws.iter().map(|wf| wf.good).sum();
        let classes: Vec<usize> =
            CLASSES.chars().map(|c| ws.iter().filter(|wf| wf.class.starts_with(c)).count()).collect();
        let share = pct(good as f64 / (ws.len() * FIELDS) as f64);
        let cls: String = classes.iter().map(|n| format!("{n:>2} ")).collect();
        g.put(1, y, &format!("{} {} {} {cls}", fit(r, 17), fit(&ws.len().to_string(), 8), fit(&share, 6)), FG);
        let mut x = 52;
        for wf in ws {
            if x >= g.cols - 1 {
                break;
            }
            let fg = match wf.class.as_str() {
                "S" | "A" => FG,
                "D" => RED,
                _ => DIM,
            };
            x = g.put(x, y, &wf.class, fg);
        }
    }
    let t = total(app, w);
    let share = t.yield_frac().map(pct).unwrap_or_else(|| "—".into());
    let line = format!(
        "{}: {} {}   {} {share}   {} {}   {} {}",
        if app.yesterday { l.t("вчера", "yesterday") } else { l.t("смена", "shift") },
        l.t("пластин", "wafers"),
        t.wafers,
        l.t("годных", "good"),
        l.t("изделий", "products"),
        t.products,
        l.t("простои", "downtime"),
        mmss(t.downtime.iter().sum())
    );
    g.put_w(1, causes_y - 1, &line, wd, FG);
    g.rule(1, causes_y, wd, l.t("Причины брака, полей", "Defect causes, fields"));
    let ranked = ranked(&t);
    let top = ranked.first().map_or(1, |c| c.1).max(1);
    let trend_y = g.rows - 6;
    for (i, (c, n)) in ranked.iter().enumerate().take((trend_y - causes_y - 2).max(0) as usize) {
        let y = causes_y + 1 + i as i32;
        let on = *n > 0;
        g.put(2, y, &format!("{c} {}", cause_name(l, *c)), if on { FG } else { DIM });
        g.bar(16, y, 20, *n as f64 / top as f64, RED);
        g.put(37, y, &rfit(&n.to_string(), 3), if on { FG } else { DIM });
        if on {
            g.put_w(43, y, &hint(l, *c, app, w), g.cols - 44, DIM);
        }
    }
    g.rule(1, trend_y, wd, l.t("Годность по пластинам, %", "Yield by wafer, %"));
    let series: Vec<(&str, &Wafer)> = rows.iter().rev().cloned().collect();
    let series = &series[series.len().saturating_sub((g.cols - 34).max(8) as usize)..];
    if !series.is_empty() {
        let vals: Vec<f64> = series.iter().map(|(_, wf)| wf.good as f64 * 100.0 / FIELDS as f64).collect();
        g.spark(2, trend_y + 1, &vals, FG, 40.0, 100.0);
        let (lo, hi) = vals.iter().fold((100.0f64, 0.0f64), |a, v| (a.0.min(*v), a.1.max(*v)));
        let avg = vals.iter().sum::<f64>() / vals.len() as f64;
        let stat = format!(
            "{} {lo:.0}   {} {avg:.0}   {} {hi:.0}",
            l.t("мин", "min"),
            l.t("средн.", "avg"),
            l.t("макс", "max")
        );
        g.put_w(4 + vals.len() as i32, trend_y + 1, &stat, g.cols - 5 - vals.len() as i32, DIM);
        // отметки под провалами: худшая причина пластины
        let mut free = 0;
        for (i, (_, wf)) in series.iter().enumerate() {
            let x = 2 + i as i32;
            if vals[i] >= 70.0 || x < free {
                continue;
            }
            let worst = causes_text(&wf.map).chars().next().unwrap_or(' ');
            let label = format!("↑{} {}", wf.good, cause_name(l, worst));
            if x + width(&label) < g.cols {
                free = g.put(x, trend_y + 2, &label, RED) + 1;
            }
        }
    }
    let made: Vec<String> = chosen(app, w)
        .iter()
        .filter(|m| shift_of(app, m).products > 0)
        .map(|m| format!("{} {}", m.name, shift_of(app, m).products))
        .collect();
    if !made.is_empty() {
        g.put_w(
            1,
            g.rows - 2,
            &format!("{}: {}", l.t("изделий по машинам", "products by machine"), made.join("  ")),
            wd,
            FG,
        );
    }
}

fn wafers_tab(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let rows = wafer_rows(app, w);
    let lw = g.cols - 41;
    let head = format!(
        " {} {} {} {} {}",
        fit(l.t("машина", "machine"), 6),
        fit(l.t("рецепт", "recipe"), 12),
        l.t("годн.", "good "),
        l.t("кл", "cl"),
        l.t("брак", "defects")
    );
    g.put_w(0, 3, &head, lw, DIM);
    g.rule(1, 4, lw - 1, "");
    if rows.is_empty() {
        g.put(1, 5, l.t("готовых пластин ещё нет", "no finished wafers yet"), DIM);
        return;
    }
    let room = (g.rows - 7) as usize;
    let sel = app.sel.min(rows.len() - 1);
    let first = if sel >= room { sel + 1 - room } else { 0 };
    for (i, (name, wf)) in rows.iter().enumerate().skip(first).take(room) {
        let y = 5 + (i - first) as i32;
        if i == sel {
            g.fill(0, y, lw, 1, ACC_D);
        }
        let line = format!(
            " {} {} {:>2}/61 {}  {}",
            fit(name, 6),
            fit(&wf.recipe, 12),
            wf.good,
            fit(&wf.class, 1),
            causes_text(&wf.map)
        );
        g.put_w(0, y, &line, lw, FG);
    }
    let (name, wf) = rows[sel];
    let x = g.cols - 39;
    g.frame(x, 2, 39, 20, &format!("{name}  {}  {} {}", wf.recipe, l.t("класс", "class"), wf.class));
    let map: Vec<char> = wf.map.chars().take(FIELDS).collect();
    wafer(g, app, x + 2, 3, &map, None, false, true);
    let share = pct(wf.good as f64 / FIELDS as f64);
    g.put_w(x + 1, 22, &format!("{} {}/61   {share}", l.t("годных", "good"), wf.good), 38, FG);
}

fn causes_tab(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let t = total(app, w);
    let bad: u32 = t.causes.iter().sum();
    let fields = t.wafers * FIELDS as u32;
    let head = format!(
        "{} {bad} {} {fields}   {} {}",
        l.t("брак: полей", "bad fields:"),
        l.t("из", "of"),
        l.t("пластин", "wafers"),
        t.wafers
    );
    g.put_w(1, 3, &head, g.cols - 2, DIM);
    g.rule(1, 4, g.cols - 2, "");
    let ranked = ranked(&t);
    let top = ranked.first().map_or(1, |c| c.1).max(1);
    let bw = (g.cols - 50).max(20);
    let two = g.rows >= 5 + 14 + 3;
    for (i, (c, n)) in ranked.iter().enumerate() {
        let y = 5 + i as i32 * if two { 2 } else { 1 };
        if y > g.rows - 3 {
            break;
        }
        let on = *n > 0;
        g.put(2, y, &format!("{c} {}", cause_name(l, *c)), if on { FG } else { DIM });
        g.bar(16, y, bw, *n as f64 / top as f64, RED);
        g.put(17 + bw, y, &rfit(&n.to_string(), 4), if on { FG } else { DIM });
        if on {
            g.put(23 + bw, y, &pct(*n as f64 / bad.max(1) as f64), DIM);
            let (hx, hy) = if two { (16, y + 1) } else { (30 + bw, y) };
            g.put_w(hx, hy, &hint(l, *c, app, w), g.cols - hx - 1, DIM);
        }
    }
}

fn downtime_tab(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let ms = chosen(app, w);
    let bw = ((g.cols - 12) / 3 - 8).max(8);
    g.fill(0, 3, g.cols, 1, FAINT);
    g.put(1, 3, l.t("МАШИНА", "MACHINE"), FG);
    for (k, reason) in ["no_power", "output_full", "no_catalyst"].iter().enumerate() {
        g.put_w(10 + k as i32 * (bw + 8), 3, &reason_text(l, reason).to_uppercase(), bw + 7, FG);
    }
    let top = ms.iter().flat_map(|m| shift_of(app, m).downtime).max().unwrap_or(0).max(1);
    for (i, m) in ms.iter().enumerate() {
        let y = 4 + i as i32;
        if y > g.rows - 6 {
            break;
        }
        let s = shift_of(app, m);
        let idle = s.downtime.iter().all(|d| *d == 0);
        g.put(1, y, &fit(&m.name, 8), if idle { DIM } else { FG });
        for (k, d) in s.downtime.iter().enumerate() {
            let x = 10 + k as i32 * (bw + 8);
            g.bar(x, y, bw, *d as f64 / top as f64, RED);
            g.put(x + bw + 1, y, &mmss(*d), if *d > 0 { FG } else { DIM });
        }
    }
    let t = total(app, w);
    let y = g.rows - 4;
    g.rule(0, y, g.cols, l.t("Всего", "Total"));
    let text = format!(
        "{} {}   {} {}   {} {}",
        reason_text(l, "no_power"),
        mmss(t.downtime[0]),
        reason_text(l, "output_full"),
        mmss(t.downtime[1]),
        reason_text(l, "no_catalyst"),
        mmss(t.downtime[2])
    );
    g.put_w(1, y + 1, &text, g.cols - 2, FG);
}
