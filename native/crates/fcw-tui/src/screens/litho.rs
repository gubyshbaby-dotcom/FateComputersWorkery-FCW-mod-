//! `fcw litho` — литограф: карта пластины на 61 поле, ход экспозиции, брак по полям.

use super::{footer, no_data};
use crate::app::{apc_row, log_tab, log_tail, machine_keys, machine_title, off_note, power_row, App, Panel};
use crate::grid::{fit, rfit, Grid, ACC, ACC_D, BG, DIM, FAINT, FG, RED, RED_D};
use crate::model::{fields, Litho, Machine, State, Wafer, World, CAUSES, CLASSES, FIELDS};
use crate::text::{cause_name, cause_short, fe, fek, jitter, mmss, pct, reason_text, signed, zone_label, Lang};

/// Что показывает карта: идущая пластина или последняя готовая.
struct MapView {
    /// По знаку на решённое поле.
    map: Vec<char>,
    /// Поле под лучом (с нуля).
    cur: Option<usize>,
    /// Пластина готова: карта итоговая.
    finished: bool,
    /// Номер пластины с единицы.
    number: u32,
}

impl MapView {
    fn done(&self) -> usize {
        self.map.len()
    }
    fn bad(&self, n: usize) -> Option<char> {
        self.map.get(n).copied().filter(|c| *c != '.')
    }
    fn good(&self) -> usize {
        self.map.iter().filter(|c| **c == '.').count()
    }
}

fn view(m: &Machine, li: &Litho) -> MapView {
    let working = matches!(m.state, State::Run | State::Fault) && m.recipe.is_some();
    if working {
        // без модели брака мод карту не ведёт: поля считаются по ходу рецепта
        let map: Vec<char> = if m.defects {
            li.map.chars().take(FIELDS).collect()
        } else {
            vec!['.'; (FIELDS as f64 * m.progress()) as usize]
        };
        let exposing = m.state == State::Run && m.phase.starts_with("expose") && map.len() < FIELDS;
        let cur = exposing.then_some(map.len());
        return MapView { map, cur, finished: false, number: li.lot + 1 };
    }
    match li.wafers.last() {
        Some(last) => {
            MapView { map: last.map.chars().take(FIELDS).collect(), cur: None, finished: true, number: li.lot }
        }
        None => MapView { map: Vec::new(), cur: None, finished: false, number: li.lot + 1 },
    }
}

/// Брак по причинам строкой: `W 3  P 2  E 3`.
pub(crate) fn causes_text(map: &str) -> String {
    let mut parts: Vec<(usize, char)> =
        CAUSES.chars().map(|c| (map.chars().filter(|x| *x == c).count(), c)).filter(|(n, _)| *n > 0).collect();
    parts.sort_by_key(|a| std::cmp::Reverse(a.0));
    parts.iter().map(|(n, c)| format!("{c} {n}")).collect::<Vec<_>>().join("  ")
}

/// Класс годности по числу годных полей (docs/PRODUCTION.md, «Брак»).
fn class_of(good: usize) -> char {
    match good {
        58.. => 'S',
        52..=57 => 'A',
        43..=51 => 'B',
        31..=42 => 'C',
        _ => 'D',
    }
}

/// Карта пластины. Малая: поле 3×1 знакоместа с зазором в полстроки; крупная: 7×3 с подписями.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wafer(
    g: &mut Grid,
    app: &App,
    x: i32,
    y: i32,
    map: &[char],
    cur: Option<usize>,
    big: bool,
    finished: bool,
) {
    let l = app.lang;
    let (fw, fh) = if big { (8, 4) } else { (4, 2) };
    for (n, c, r) in fields() {
        let (cx, cy) = (x + c * fw, y + r * fh);
        let bad = map.get(n).copied().filter(|c| *c != '.');
        let (bg, fg) = if bad.is_some() {
            (RED_D, RED)
        } else if cur == Some(n) {
            (ACC, BG)
        } else if n < map.len() {
            (ACC_D, ACC)
        } else {
            (FAINT, DIM)
        };
        if !big {
            let lab = match bad {
                Some(c) if app.mono => format!("[{c}]"),
                Some(c) => format!("{c}{:02}", n + 1),
                None => format!(" {:02}", n + 1),
            };
            g.put_on(cx, cy, &lab, fg, bg);
            g.put(cx, cy + 1, "▀▀▀", bg);
            continue;
        }
        g.fill(cx, cy, 7, 3, bg);
        g.put(cx, cy + 3, "▀▀▀▀▀▀▀", bg);
        g.put_on(cx + 1, cy, &format!("{:02}", n + 1), fg, bg);
        if let Some(c) = bad {
            g.put_on(cx + 4, cy, cause_short(l, c), fg, bg);
            let lab = if app.mono { format!("[{c}]") } else { format!("{} {c}", l.t("брак", "bad")) };
            g.put_on(cx + 1, cy + 1, &lab, fg, bg);
        } else if cur == Some(n) {
            g.put_on(cx + 1, cy + 1, l.t("эксп.", "expo"), fg, bg);
            g.put_on(cx + 1, cy + 2, l.t("30 мДж", "30 mJ"), fg, bg);
        } else if n < map.len() {
            g.put_on(cx + 1, cy + 1, "ok", fg, bg);
            let dev = (n as i32 * 7) % 9 - 4;
            let sign = if dev < 0 { '−' } else { '+' };
            let text = format!("{sign}{:02} {}", dev.abs(), l.t("нм", "nm"));
            g.put_on(cx + 1, cy + 2, &text, if finished { fg } else { DIM }, bg);
        }
    }
}

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let m = machine_title(g, app, w);
    machine_keys(g, app);
    let Some(m) = m else { return no_data(g, app, w) };
    let Some(li) = &m.litho else { return no_data(g, app, w) };
    match app.tab {
        0 => wafer_tab(g, app, w, m, li),
        1 => lot_tab(g, app, w, m, li),
        2 => mask_tab(g, app, w, m, li),
        _ => log_tab(g, app, w, Some(&m.name)),
    }
}

/// Раздел «Экспозиция»: четыре строки по состоянию.
fn exposure(p: &mut Panel, l: Lang, m: &Machine, v: &MapView) {
    p.sec(l.t("Экспозиция", "Exposure"));
    let field = l.t("поле", "field");
    if m.state == State::Fault {
        let n = (v.done() + 1).min(FIELDS);
        p.row(field, &format!("{n} / 61   {}", l.t("остановлено", "stopped")), RED);
        let why = if m.reason == "no_power" {
            format!("{}, {} FE/t", reason_text(l, &m.reason), fe(m.income))
        } else {
            reason_text(l, &m.reason)
        };
        p.row(l.t("причина", "cause"), &why, RED);
        p.row(l.t("буфер", "buffer"), &format!("{} / {} FE", fek(m.energy), fek(m.capacity)), FG);
        let lost: Vec<usize> = (0..v.done()).filter(|i| v.bad(*i) == Some('E')).map(|i| i + 1).collect();
        let text = match (lost.first(), lost.last()) {
            (Some(a), Some(b)) if a != b => format!("{} {a}–{b} (E)", l.t("поля", "fields")),
            (Some(a), _) => format!("{} {a} (E)", field),
            _ => "—".into(),
        };
        p.row(l.t("потеряно", "lost"), &text, FG);
    } else if let Some(cur) = v.cur {
        let row = fields()[cur].2 + 1;
        p.row(field, &format!("{} / 61   {} {row}", cur + 1, l.t("змейка, ряд", "serpentine, row")), FG);
        let t = m.ticks / 8;
        let dose = 30.0 + 0.1 * jitter(t);
        let dev = signed((dose - 30.0) / 30.0 * 100.0, 1);
        p.row(l.t("доза", "dose"), &format!("{dose:.1} {}   {dev} %", l.t("мДж/см²", "mJ/cm²")), FG);
        let focus = signed((12.0 * jitter(t + 7)).round(), 0);
        let tilt = 0.3 + 0.1 * jitter(t + 13);
        p.row(
            l.t("фокус", "focus"),
            &format!("{focus} {}   {} {tilt:.1} {}", l.t("нм", "nm"), l.t("наклон", "tilt"), l.t("мкрад", "µrad")),
            FG,
        );
        p.row(l.t("совмещение", "overlay"), &format!("{:.1} {} (3σ)", 1.8 + 0.2 * jitter(t + 29), l.t("нм", "nm")), FG);
    } else if v.finished {
        let good = v.good();
        p.row(field, &format!("61 / 61   {}", l.t("пластина готова", "wafer done")), FG);
        let share = pct(good as f64 / FIELDS as f64);
        p.row(
            l.t("годных", "good"),
            &format!("{good} / 61   {share}   {} {}", l.t("класс", "class"), class_of(good)),
            FG,
        );
        let bad = causes_text(&v.map.iter().collect::<String>());
        p.row(l.t("брак", "defects"), if bad.is_empty() { "—" } else { &bad }, if bad.is_empty() { FG } else { RED });
        let out = m.out.iter().find(|z| !z.items.is_empty());
        let text = out.map(|z| format!("{}: {}", zone_label(&z.id), z.text())).unwrap_or_else(|| "—".into());
        p.row(l.t("выгрузка", "unload"), &text, FG);
    } else {
        let note = match m.state {
            State::Run => crate::text::phase_text(l, &m.phase),
            State::Off => String::new(),
            _ if m.load.iter().all(|z| z.items.is_empty()) => l.t("нет пластин", "no wafers").into(),
            _ if li_no_mask(m) => l.t("нет маски", "no mask").into(),
            _ => String::new(),
        };
        p.row(field, &format!("— / 61   {note}"), FG);
        p.row(l.t("доза", "dose"), "—", DIM);
        p.row(l.t("фокус", "focus"), "—", DIM);
        p.row(l.t("совмещение", "overlay"), "—", DIM);
    }
}

fn li_no_mask(m: &Machine) -> bool {
    m.litho.as_ref().is_some_and(|l| l.mask.is_none())
}

fn mask_rows(p: &mut Panel, l: Lang, m: &Machine, li: &Litho, bw: i32, wide: bool) {
    p.sec(l.t("Маска", "Mask"));
    match &li.mask {
        Some(mask) => {
            p.row(l.t("шаблон", "reticle"), &mask.id, FG);
            let left = mask.max.saturating_sub(mask.used);
            let frac = if mask.max > 0 { left as f64 / mask.max as f64 } else { 0.0 };
            p.bar(l.t("ресурс", "life"), frac, bw, &format!("{left}/{}", mask.max), FG);
            if wide {
                let wear = 1.0 - frac;
                let text = format!(
                    "{}   {} {:.0} %   ({})",
                    pct(wear),
                    l.t("брак по маске", "mask defects"),
                    mask_defect(wear, m.apc.coverage) * 100.0,
                    l.t("растёт после 50 %", "grows after 50 %")
                );
                p.row(l.t("износ", "wear"), &text, FG);
            }
        }
        None => {
            let fg = if m.reason == "no_catalyst" { RED } else { FG };
            p.row(l.t("шаблон", "reticle"), l.t("нет маски", "no mask"), fg);
            p.row(l.t("ресурс", "life"), "—", DIM);
        }
    }
}

/// Вероятность брака `M` на поле при таком износе маски и покрытии APC (docs/PRODUCTION.md).
fn mask_defect(wear: f64, coverage: f64) -> f64 {
    if wear <= 0.5 {
        0.0
    } else {
        (wear - 0.5) / 0.5 * 0.12 * (1.0 - 0.6 * coverage.clamp(0.0, 1.0))
    }
}

fn env_rows(p: &mut Panel, l: Lang, m: &Machine, li: &Litho) {
    p.sec(l.t("Среда", "Environment"));
    let room = match li.iso {
        0 => l.t("открыто, под небом", "open to the sky").to_string(),
        n => format!("ISO {n}   {}", l.t("помещение закрыто", "room sealed")),
    };
    p.row(l.t("чистота", "cleanliness"), &room, FG);
    let cool = if li.coolant { l.t("вода есть", "water on") } else { l.t("нет воды", "no water") };
    p.row(l.t("охлаждение", "cooling"), cool, FG);
    p.row(l.t("вибрация", "vibration"), if li.vibration { l.t("есть", "yes") } else { l.t("нет", "none") }, FG);
    power_row(p, l, m);
    apc_row(p, l, m, "");
}

fn queue_rows(p: &mut Panel, l: Lang, m: &Machine, v: &MapView) {
    p.sec(l.t("Очередь", "Queue"));
    let input = m.load.iter().find(|z| !z.items.is_empty()).or(m.load.first());
    let queued = input.map_or(0, |z| z.items.iter().filter(|s| s.id.contains("wafer")).map(|s| s.count).sum::<u32>());
    match input {
        Some(z) if !z.items.is_empty() => p.row(&zone_label(&z.id), &z.text(), FG),
        Some(z) => p.row(&zone_label(&z.id), l.t("пусто", "empty"), DIM),
        None => p.row("dock1", "—", DIM),
    }
    let left = m.total.saturating_sub(m.ticks);
    let text = match m.state {
        State::Run => format!(
            "{} {}  {} {}",
            l.t("пласт.", "wafer"),
            mmss(left),
            l.t("очередь", "queue"),
            mmss(left + queued * m.total)
        ),
        State::Fault => format!("{} ({})", l.t("стоит", "halted"), reason_text(l, &m.reason)),
        State::Off => l.t("машина выключена", "machine is off").into(),
        _ if v.finished && queued > 0 => l.t("следующая из очереди", "next from the queue").into(),
        _ => "—".into(),
    };
    p.row(l.t("осталось", "remaining"), &text, FG);
}

/// Подписи причин, которые есть на карте (или основные, если брака нет), в ширину `wd`.
fn legend(l: Lang, map: &[char], wd: i32) -> String {
    let mut present: Vec<char> = "PWEMCTV".chars().filter(|c| map.contains(c)).collect();
    if present.is_empty() {
        present = "PWEM".chars().collect();
    }
    let mut out = String::new();
    for c in present {
        let name = cause_name(l, c);
        let item = format!("{c} {name}");
        let sep = if out.is_empty() { 0 } else { 1 };
        if (out.chars().count() + sep + item.chars().count()) as i32 > wd {
            break;
        }
        if sep == 1 {
            out.push(' ');
        }
        out.push_str(&item);
    }
    out
}

fn wafer_tab(g: &mut Grid, app: &App, w: &World, m: &Machine, li: &Litho) {
    let l = app.lang;
    let v = view(m, li);
    let big = g.cols >= 150 && g.rows >= 46;
    let title = format!("{} {}", l.t("Пластина", "Wafer"), v.number);
    let off = m.state == State::Off;
    let frac = if v.finished { 1.0 } else { v.done() as f64 / FIELDS as f64 };
    let bar_fg = if m.state == State::Fault { RED } else { ACC };
    if big {
        let mw = 9 * 8 + 3;
        g.frame(0, 2, mw, 39, &title);
        if off {
            off_note(g, l, 0, 2, mw, 39);
        } else {
            wafer(g, app, 2, 4, &v.map, v.cur, true, v.finished);
        }
        let px = mw + 2;
        let pw = g.cols - px - 1;
        let max_y = g.rows - 3;
        let mut p = Panel { g, x: px, w: pw, y: 2, max_y, kw: 12 };
        exposure(&mut p, l, m, &v);
        p.gap();
        mask_rows(&mut p, l, m, li, 30, true);
        p.gap();
        env_rows(&mut p, l, m, li);
        p.gap();
        queue_rows(&mut p, l, m, &v);
        p.gap();
        p.sec(l.t("Последние пластины", "Recent wafers"));
        let rows = 4.min(li.wafers.len() as i32 + 1);
        let y = p.y;
        lot_table(g, app, m, li, px + 1, y, pw - 1, rows, false);
        let mut y = y + rows + 2;
        g.rule(px, y, pw, l.t("Годность по пластинам", "Yield by wafer"));
        y += 1;
        trend(g, l, li, px + 1, y, pw - 2);
        y += 2;
        g.rule(px, y, pw, l.t("Журнал", "Log"));
        log_tail(g, app, w, Some(&m.name), px + 1, y + 1, max_y, pw - 1);
        // легенда и две полосы хода под картой
        let by = (g.rows - 4).max(43);
        let ly = by - 2;
        let mut x = 2;
        for (bg, lab) in [
            (FAINT, l.t("ждёт", "waiting")),
            (ACC_D, l.t("годен", "good")),
            (ACC, l.t("идёт", "now")),
            (RED_D, l.t("брак:", "bad:")),
        ] {
            g.put_on(x, ly, "  ", FG, bg);
            x = g.put(x + 3, ly, lab, FG) + 3;
        }
        let half = |cs: &str| cs.chars().map(|c| format!("{c} {}", cause_name(l, c))).collect::<Vec<_>>().join("  ");
        g.put_w(x - 2, ly, &half("PWC"), mw + 3 - x, DIM);
        // вторая строка причин — под красным образцом: так она помещается под картой
        g.put_w(33, ly + 1, &half("MTVE"), mw - 33, DIM);
        g.put(2, by, l.t("пластина", "wafer"), DIM);
        g.bar(12, by, 50, frac, bar_fg);
        g.put(64, by, &pct(frac), FG);
        let out: u32 = m.out.iter().map(|z| z.count()).sum();
        let queued: u32 =
            m.load.iter().flat_map(|z| &z.items).filter(|s| s.id.contains("wafer")).map(|s| s.count).sum();
        let running = if v.finished || v.map.is_empty() && m.state != State::Run { 0.0 } else { 1.0 };
        let total = out as f64 + queued as f64 + running;
        let qf = if total > 0.0 { (out as f64 + running * frac) / total } else { 0.0 };
        g.put(2, by + 1, l.t("очередь", "queue"), DIM);
        g.bar(12, by + 1, 50, qf, FG);
        g.put(64, by + 1, &pct(qf), FG);
        if let Some(f) = &w.flash {
            g.put_w(2, g.rows - 2, &f.text, mw - 4, if f.error { RED } else { FG });
        }
        return;
    }
    g.frame(0, 2, 39, 20, &title);
    if off {
        off_note(g, l, 0, 2, 39, 20);
    } else {
        wafer(g, app, 2, 3, &v.map, v.cur, false, v.finished);
    }
    let wide = g.cols >= 120;
    let pw = if wide { 38 } else { g.cols - 42 };
    let mut p = Panel { g, x: 41, w: pw, y: 2, max_y: 21, kw: 12 };
    exposure(&mut p, l, m, &v);
    mask_rows(&mut p, l, m, li, (pw - 24).clamp(8, 14), false);
    env_rows(&mut p, l, m, li);
    queue_rows(&mut p, l, m, &v);
    g.put(1, 22, l.t("пластина", "wafer"), DIM);
    g.bar(10, 22, 22, frac, bar_fg);
    g.put(33, 22, &rfit(&pct(frac), 5), FG);
    g.put(41, 22, &legend(l, &v.map, pw), DIM);
    if wide {
        // постоянная колонка: таблица пластин и журнал видны без переключения вкладок
        let x = 81;
        let wd = g.cols - x - 1;
        g.rule(x, 2, wd, l.t("Последние пластины", "Recent wafers"));
        let rows = (g.rows / 3).min(li.wafers.len() as i32 + 1).max(1);
        lot_table(g, app, m, li, x + 1, 3, wd - 1, rows, false);
        let y = 3 + rows + 2;
        g.rule(x, y, wd, l.t("Журнал", "Log"));
        log_tail(g, app, w, Some(&m.name), x + 1, y + 1, g.rows - 2, wd - 1);
    }
    footer(g, app, w, &m.name, 23, if wide { 80 } else { g.cols });
}

/// График годности последних пластин в одну строку с подписью.
fn trend(g: &mut Grid, l: Lang, li: &Litho, x: i32, y: i32, wd: i32) {
    if li.wafers.is_empty() {
        g.put(x, y, l.t("готовых пластин ещё нет", "no finished wafers yet"), DIM);
        return;
    }
    let vals: Vec<f64> = li.wafers.iter().map(|w| w.good as f64).collect();
    g.spark(x, y, &vals, FG, 30.0, FIELDS as f64);
    let avg = vals.iter().sum::<f64>() / vals.len() as f64 / FIELDS as f64;
    let text = format!("{} {}   {} {}", l.t("пластин:", "wafers:"), vals.len(), l.t("среднее", "average"), pct(avg));
    g.put_w(x + vals.len() as i32 + 2, y, &text, wd - vals.len() as i32 - 2, DIM);
}

/// Таблица последних пластин: заголовок и не больше `rows` строк; идущая пластина — последней строкой.
/// В полной таблице между заголовком и строками остаётся место для линейки.
#[allow(clippy::too_many_arguments)]
fn lot_table(g: &mut Grid, app: &App, m: &Machine, li: &Litho, x: i32, y: i32, wd: i32, rows: i32, full: bool) {
    let l = app.lang;
    let head = if full {
        format!(
            "{}  {}   %   {}  {}  {}",
            rfit("№", 4),
            l.t("годных", "good  "),
            l.t("класс", "class"),
            fit(l.t("брак по причинам", "defects by cause"), 24),
            l.t("рецепт", "recipe")
        )
    } else {
        format!("{}  {}  {}  {}", rfit("№", 4), l.t("годных", "good  "), l.t("класс", "class"), l.t("брак", "defects"))
    };
    g.put_w(x, y, &head, wd, DIM);
    let v = view(m, li);
    let live = !v.finished && matches!(m.state, State::Run | State::Fault) && m.recipe.is_some();
    let room = (rows - if live { 1 } else { 0 }).max(0) as usize;
    let first = li.wafers.len().saturating_sub(room);
    let mut yy = y + if full { 2 } else { 1 };
    for (i, wf) in li.wafers.iter().enumerate().skip(first) {
        let no = li.lot as usize + 1 + i - li.wafers.len();
        let line = wafer_line(wf, no, full);
        g.put_w(x, yy, &line, wd, FG);
        yy += 1;
    }
    if live {
        let text = if m.state == State::Fault {
            format!("{}  {}", rfit(&v.number.to_string(), 4), l.t("остановлена", "halted"))
        } else {
            format!(
                "{}  {} {}/61",
                rfit(&v.number.to_string(), 4),
                l.t("идёт экспозиция, поле", "exposing, field"),
                (v.done() + 1).min(FIELDS)
            )
        };
        g.put_w(x, yy, &text, wd, if m.state == State::Fault { RED } else { ACC });
    }
}

fn wafer_line(wf: &Wafer, no: usize, full: bool) -> String {
    let causes = causes_text(&wf.map);
    if full {
        format!(
            "{}  {:>2}/61  {:>3}   {}      {}  {}",
            rfit(&no.to_string(), 4),
            wf.good,
            (wf.good as f64 * 100.0 / FIELDS as f64).round(),
            fit(&wf.class, 1),
            fit(&causes, 24),
            wf.recipe
        )
    } else {
        format!("{}  {:>2}/61   {}      {}", rfit(&no.to_string(), 4), wf.good, fit(&wf.class, 1), causes)
    }
}

fn lot_tab(g: &mut Grid, app: &App, w: &World, m: &Machine, li: &Litho) {
    let l = app.lang;
    let wd = g.cols - 2;
    let causes_y = g.rows - 11;
    // заголовок стоит над линейкой, строки — под ней
    lot_table(g, app, m, li, 1, 3, wd, causes_y - 5, true);
    g.rule(1, 4, wd, "");
    // причины брака в показанных пластинах
    g.rule(1, causes_y, wd, l.t("Причины брака в последних пластинах", "Defect causes in the recent wafers"));
    let mut counts: Vec<(u32, char)> = CAUSES
        .chars()
        .map(|c| (li.wafers.iter().map(|wf| wf.map.chars().filter(|x| *x == c).count() as u32).sum(), c))
        .collect();
    counts.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let top = counts.first().map_or(1, |c| c.0).max(1);
    for (i, (n, c)) in counts.iter().take(4).enumerate() {
        let y = causes_y + 1 + i as i32;
        g.put(2, y, &format!("{c} {}", cause_name(l, *c)), if *n > 0 { FG } else { DIM });
        g.bar(16, y, 30, *n as f64 / top as f64, RED);
        g.put(48, y, &n.to_string(), if *n > 0 { FG } else { DIM });
    }
    let ty = g.rows - 5;
    g.rule(1, ty, wd, l.t("Итог", "Total"));
    let n = li.wafers.len();
    if n == 0 {
        g.put(2, ty + 1, l.t("готовых пластин ещё нет", "no finished wafers yet"), DIM);
    } else {
        let good: u32 = li.wafers.iter().map(|wf| wf.good).sum();
        let classes: Vec<String> = CLASSES
            .chars()
            .map(|c| (c, li.wafers.iter().filter(|wf| wf.class.starts_with(c)).count()))
            .filter(|(_, k)| *k > 0)
            .map(|(c, k)| format!("{c} {k}"))
            .collect();
        let text = format!(
            "{} {good} {} {}   {}   {}: {}",
            l.t("годных", "good"),
            l.t("из", "of"),
            n * FIELDS,
            pct(good as f64 / (n * FIELDS) as f64),
            l.t("классы", "classes"),
            classes.join("  ")
        );
        g.put_w(2, ty + 1, &text, wd - 2, FG);
        let mult: f64 = li.wafers.iter().map(|wf| class_mult(&wf.class)).sum::<f64>() / n as f64;
        let text = format!(
            "{} ×{mult:.2} {}",
            l.t("в корпусировке даст в среднем", "packaging will give on average"),
            l.t("от номинала рецепта", "of the recipe's nominal output")
        );
        g.put_w(2, ty + 2, &text, wd - 2, DIM);
    }
    if let Some(f) = &w.flash {
        g.put_w(1, g.rows - 2, &f.text, wd, if f.error { RED } else { FG });
    }
}

/// Множитель выхода корпусировки по классу годности.
pub(crate) fn class_mult(class: &str) -> f64 {
    match class {
        "S" => 1.25,
        "A" => 1.0,
        "B" => 0.8,
        "C" => 0.6,
        _ => 0.4,
    }
}

fn mask_tab(g: &mut Grid, app: &App, w: &World, m: &Machine, li: &Litho) {
    let l = app.lang;
    let cw = (g.cols * 5 / 8).min(g.cols - 30);
    let ch = g.rows - 9;
    g.frame(0, 2, cw, ch, l.t("Брак по маске от износа, %", "Mask defects by wear, %"));
    let (ax, ay, aw, ah) = (6, 3, cw - 8, ch - 4);
    let wear = li.mask.as_ref().map(|k| if k.max > 0 { k.used as f64 / k.max as f64 } else { 0.0 });
    let cov = m.apc.coverage;
    let curve: Vec<f64> = (0..aw).map(|i| mask_defect(i as f64 / (aw - 1).max(1) as f64, cov) / 0.12).collect();
    let upto = wear.map(|x| (x * aw as f64).round() as i32);
    g.area(ax, ay, aw, ah, &curve, FG, upto);
    for (j, lab) in ["12", " 8", " 4", " 0"].iter().enumerate() {
        g.put(2, ay + (j as i32 * ah + 1) / 3, lab, DIM);
    }
    let axis: String = (0..aw).map(|i| if i == 0 || i == aw / 2 || i == aw - 1 { '┼' } else { '─' }).collect();
    g.put(ax, ay + ah, &axis, DIM);
    g.put(ax, ay + ah + 1, "0 %", DIM);
    g.put(ax + aw / 2 - 1, ay + ah + 1, "50 %", DIM);
    g.put(ax + aw - 5, ay + ah + 1, "100 %", DIM);
    if let Some(u) = upto {
        g.put(ax + u.clamp(0, aw - 1), ay + ah, "↑", ACC);
    }
    let x = cw + 2;
    let pw = g.cols - x - 1;
    let max_y = g.rows - 3;
    let mut p = Panel { g, x, w: pw, y: 2, max_y, kw: 10 };
    mask_rows(&mut p, l, m, li, (pw - 20).clamp(6, 14), false);
    if let (Some(mask), Some(wear)) = (&li.mask, wear) {
        p.row(l.t("износ", "wear"), &pct(wear), FG);
        let now = mask_defect(wear, cov);
        p.row(l.t("брак M", "M defects"), &format!("{:.1} % {}", now * 100.0, l.t("на поле", "per field")), FG);
        let left = (mask.max / 2).saturating_sub(mask.used);
        let text = if left > 0 {
            format!("{left} {}", l.t("пластин", "wafers"))
        } else {
            l.t("порог пройден", "past the threshold").to_string()
        };
        p.row(l.t("до 50 %", "to 50 %"), &text, FG);
    }
    p.gap();
    p.sec("APC");
    apc_row(&mut p, l, m, "");
    p.row(
        l.t("поправка", "correction"),
        &format!("{} M ×{:.2}", l.t("брак", "defects"), 1.0 - 0.6 * cov.clamp(0.0, 1.0)),
        FG,
    );
    // сколько полей с браком M в последних пластинах
    let y = g.rows - 6;
    g.rule(0, y, g.cols, l.t("Брак M в последних пластинах, полей", "M defects in the recent wafers, fields"));
    if li.wafers.is_empty() {
        g.put(2, y + 1, l.t("готовых пластин ещё нет", "no finished wafers yet"), DIM);
    } else {
        let vals: Vec<f64> = li.wafers.iter().map(|wf| wf.map.chars().filter(|c| *c == 'M').count() as f64).collect();
        let hi = vals.iter().cloned().fold(1.0, f64::max);
        g.spark(2, y + 1, &vals, RED, 0.0, hi);
        let total: f64 = vals.iter().sum();
        g.put(
            4 + vals.len() as i32,
            y + 1,
            &format!("{} {total:.0}   {} {hi:.0}", l.t("всего", "total"), l.t("макс", "max")),
            DIM,
        );
    }
    footer(g, app, w, &m.name, g.rows - 3, g.cols);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causes_sorted_by_count() {
        assert_eq!(causes_text("..W.P.WW.E"), "W 3  E 1  P 1");
        assert_eq!(causes_text("...."), "");
    }

    #[test]
    fn classes_follow_the_mod() {
        assert_eq!(class_of(61), 'S');
        assert_eq!(class_of(57), 'A');
        assert_eq!(class_of(43), 'B');
        assert_eq!(class_of(31), 'C');
        assert_eq!(class_of(30), 'D');
    }

    #[test]
    fn mask_defects_start_after_half_life() {
        assert_eq!(mask_defect(0.5, 0.0), 0.0);
        assert!((mask_defect(1.0, 0.0) - 0.12).abs() < 1e-9);
        assert!((mask_defect(1.0, 1.0) - 0.048).abs() < 1e-9);
    }
}
