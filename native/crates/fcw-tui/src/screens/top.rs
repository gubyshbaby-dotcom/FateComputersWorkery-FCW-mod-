//! `fcw top` — машины сети: сводка питания и APC, таблица с ходом, подробности выбранной, журнал.

use crate::app::{clock, keys, log_tab, log_tail, tabs, title, App};
use crate::grid::{fit, rfit, Grid, ACC, ACC_D, BG, DIM, FAINT, FG, RED};
use crate::model::{Kind, Machine, State, World, CAUSES, CLASSES, FIELDS};
use crate::text::{cu, fe, fek, kind_name, mmss, pct, state_line, zone_label, Lang};

pub(crate) fn draw(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let n = w.machines.len();
    let left = format!("fcw top  {} {}", l.t("сеть", "network"), w.host);
    let count = match l {
        Lang::Ru => format!("{n} {}", plural_ru(n, "машина", "машины", "машин")),
        Lang::En => format!("{n} machine{}", if n == 1 { "" } else { "s" }),
    };
    let right = format!("{count}  {}", clock(w));
    let lost = w.stale.then_some((l.t("НЕТ СВЯЗИ", "NO LINK"), RED));
    title(g, &left, &right, lost);
    tabs(g, &app.program.tabs(l), app.tab);
    keys(
        g,
        &[
            ("F1", l.t("Справка", "Help")),
            ("F2", l.t("Питание", "Power")),
            ("F3", l.t("Журнал", "Log")),
            ("F4", l.t("Годность", "Yield")),
            ("F5", l.t("Пуск", "Start")),
            ("F6", l.t("Стоп", "Stop")),
            ("Enter", l.t("Открыть", "Open")),
            ("F10", l.t("Выход", "Quit")),
        ],
    );
    match app.tab {
        0 => machines_tab(g, app, w),
        1 => power_tab(g, app, w),
        2 => log_tab(g, app, w, None),
        _ => yield_tab(g, app, w),
    }
}

fn plural_ru<'a>(n: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    match (n % 10, n % 100) {
        (1, x) if x != 11 => one,
        (2..=4, x) if !(12..=14).contains(&x) => few,
        _ => many,
    }
}

/// Три полосы сводки: питание, буфер, APC; справа счётчики состояний.
fn summary(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let bw = (g.cols * 3 / 10).max(24);
    let tx = 11 + bw;
    let sx = (tx + 21).max(g.cols * 7 / 10);
    let ms: Vec<&Machine> = w.machines.iter().filter(|m| m.full).collect();
    let powered: Vec<&&Machine> = ms.iter().filter(|m| m.power_required).collect();
    let (usage, income): (f64, f64) = powered.iter().fold((0.0, 0.0), |a, m| (a.0 + m.usage, a.1 + m.income));
    let (energy, cap): (f64, f64) = powered.iter().fold((0.0, 0.0), |a, m| (a.0 + m.energy, a.1 + m.capacity));
    g.put(1, 2, l.t("Питание", "Power"), DIM);
    g.put(1, 3, l.t("Буфер", "Buffer"), DIM);
    if powered.is_empty() {
        g.bar(10, 2, bw, 0.0, FG);
        g.bar(10, 3, bw, 0.0, FG);
        g.put(tx, 2, l.t("встроенные трансф.", "built-in transformers"), DIM);
        g.put(tx, 3, "—", DIM);
    } else {
        g.bar(10, 2, bw, if income > 0.0 { usage / income } else { 0.0 }, FG);
        g.put(tx, 2, &format!("{} / {} FE/t", fek(usage), fek(income)), FG);
        g.bar(10, 3, bw, if cap > 0.0 { energy / cap } else { 0.0 }, FG);
        let net = income - usage;
        let sign = if net < 0.0 { "−" } else { "+" };
        g.put(
            tx,
            3,
            &format!("{}  {sign}{} FE/t", pct(if cap > 0.0 { energy / cap } else { 0.0 }), fek(net.abs())),
            FG,
        );
    }
    let need: f64 = ms.iter().map(|m| m.apc.need).sum();
    let got: f64 = ms.iter().map(|m| m.apc.got()).sum();
    g.put(1, 4, "APC", DIM);
    g.bar(10, 4, bw, if need > 0.0 { got / need } else { 0.0 }, ACC);
    let mut end = g.put(tx, 4, &format!("{} / {} CU", cu(got), cu(need)), FG);
    if need > 0.0 && ms.iter().any(|m| m.apc.need > 0.0 && m.apc.no_service()) {
        end = g.put(end + 2, 4, l.t("служба не запущена", "service is not running"), DIM);
    }
    let count = |s: State| w.machines.iter().filter(|m| m.state == s).count();
    g.put(
        sx,
        2,
        &format!("{} {}  {} {}", l.t("работа", "running"), count(State::Run), l.t("ожид.", "idle"), count(State::Idle)),
        FG,
    );
    let faults = count(State::Fault);
    if faults > 0 {
        g.put(sx, 3, &format!("{} {faults}", l.t("отказ", "fault")), RED);
    } else {
        g.put(sx, 3, &format!("{} {}", l.t("выкл.", "off"), count(State::Off)), DIM);
    }
    let (good, wafers) = ms.iter().fold((0u32, 0u32), |a, m| (a.0 + m.shift.good_fields, a.1 + m.shift.wafers));
    if wafers > 0 {
        let y = pct(good as f64 / (wafers as f64 * FIELDS as f64));
        // приписка про службу APC занимает правую часть строки: годность тогда строкой выше
        if end + 2 > sx {
            g.put_w(sx + 10, 3, &format!("{} {y}", l.t("годных", "yield")), g.cols - sx - 11, FG);
        } else {
            g.put_w(sx, 4, &format!("{} {y}", l.t("годных за смену", "shift yield")), g.cols - sx - 1, FG);
        }
    }
}

/// Столбцы таблицы машин: начало и ширина. Ширины — доли от ширины терминала.
struct Cols {
    name: (i32, i32),
    kind: (i32, i32),
    state: (i32, i32),
    recipe: (i32, i32),
    bar: (i32, i32),
    fe: i32,
    link: i32,
}

fn cols(c: i32) -> Cols {
    let extra = c - 80;
    let name = 6 + extra / 8;
    let kind = 17 + extra / 8;
    let state = 21 + extra / 4;
    let recipe = 13 + extra / 4;
    let bar = 5 + extra / 8;
    let mut x = 1;
    let mut next = |w: i32| {
        let at = x;
        x += w + 1;
        (at, w)
    };
    let (name, kind, state, recipe, bar) = (next(name), next(kind), next(state), next(recipe), next(bar));
    Cols { name, kind, state, recipe, bar, fe: bar.0 + bar.1 + 1, link: bar.0 + bar.1 + 6 }
}

fn machines_tab(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    summary(g, app, w);
    let c = cols(g.cols);
    g.fill(0, 6, g.cols, 1, FAINT);
    g.put(c.name.0, 6, l.t("ИМЯ", "NAME"), FG);
    g.put(c.kind.0, 6, l.t("МАШИНА", "MACHINE"), FG);
    g.put(c.state.0, 6, l.t("СОСТОЯНИЕ", "STATE"), FG);
    g.put(c.recipe.0, 6, l.t("РЕЦЕПТ", "RECIPE"), FG);
    g.put(c.bar.0, 6, l.t("ХОД", "RUN"), FG);
    g.put(c.fe, 6, "FE/t", FG);
    g.put(c.link, 6, l.t("СВЯЗЬ", "LINK"), FG);
    // снизу: подробности выбранной (5 строк с линейкой) и журнал сети
    let log_rows = 3.min(g.rows - 22) + (g.rows - 25).max(0) / 3;
    let log_y = g.rows - 3 - log_rows;
    let detail_y = log_y - 5;
    let room = (detail_y - 8).max(1) as usize;
    let first = if app.sel >= room { app.sel + 1 - room } else { 0 };
    if w.machines.is_empty() {
        g.put(1, 8, l.t("в сети этого компьютера нет машин", "no machines in this computer's network"), DIM);
    }
    for (i, m) in w.machines.iter().enumerate().skip(first).take(room) {
        let y = 7 + (i - first) as i32;
        let sel = i == app.sel;
        if sel {
            g.fill(0, y, g.cols, 1, ACC_D);
        }
        let quiet = matches!(m.state, State::Idle | State::Off | State::Site);
        let base = if quiet { DIM } else { FG };
        let state_fg = if m.state == State::Fault {
            RED
        } else if sel {
            ACC
        } else {
            base
        };
        g.put(c.name.0, y, &fit(&m.name, c.name.1), if sel { FG } else { base });
        g.put(c.kind.0, y, &fit(&kind_name(l, m), c.kind.1), if sel { FG } else { base });
        g.put(c.state.0, y, &fit(&state_line(l, m), c.state.1), state_fg);
        g.put(c.recipe.0, y, &fit(m.recipe.as_deref().unwrap_or("—"), c.recipe.1), if sel { FG } else { base });
        let bar_fg = if m.state == State::Fault {
            RED
        } else if sel {
            ACC
        } else {
            FG
        };
        g.bar_on(c.bar.0, y, c.bar.1, m.progress(), bar_fg, if sel { BG } else { FAINT });
        let fe_text = if m.full { fe(m.usage) } else { "·".into() };
        g.put(c.fe, y, &rfit(&fe_text, 4), if sel { FG } else { base });
        let link = match m.link.as_str() {
            "cable" => l.t("кабель", "cable"),
            "radio" => l.t("радио", "radio"),
            _ => "",
        };
        g.put(c.link, y, link, DIM);
    }
    if let Some(m) = w.machines.get(app.sel) {
        detail(g, app, m, detail_y);
    }
    g.rule(0, log_y, g.cols, l.t("Журнал сети", "Network log"));
    log_tail(g, app, w, None, 1, log_y + 1, log_y + log_rows, g.cols - 2);
}

/// Подробности выбранной машины: четыре строки в две колонки.
fn detail(g: &mut Grid, app: &App, m: &Machine, y: i32) {
    let l = app.lang;
    g.rule(0, y, g.cols, &m.name);
    if !m.full {
        g.put(1, y + 1, l.t("снимка машины ещё нет", "no snapshot of the machine yet"), DIM);
        return;
    }
    let half = g.cols / 2;
    let (kw, vw) = (10, half - 12);
    let zones = |zs: &[crate::model::Zone]| {
        let parts: Vec<String> =
            zs.iter().filter(|z| !z.items.is_empty()).map(|z| format!("{}: {}", zone_label(&z.id), z.text())).collect();
        if parts.is_empty() {
            "—".to_string()
        } else {
            parts.join("  ")
        }
    };
    let recipe = match &m.recipe {
        Some(r) => format!("{r}  {}/{} t", m.ticks, m.total),
        None => "—".into(),
    };
    g.kv(1, y + 1, l.t("рецепт", "recipe"), &recipe, kw, vw, FG);
    g.kv(1, y + 2, l.t("загрузка", "load"), &zones(&m.load), kw, vw, FG);
    let x = half + 1;
    match &m.litho {
        Some(li) => {
            let mask = match &li.mask {
                Some(k) => format!("{}  {}/{}", k.id, k.max.saturating_sub(k.used), k.max),
                None => l.t("нет", "none").into(),
            };
            g.kv(x, y + 1, l.t("маска", "mask"), &mask, kw, vw, FG);
            let cur = if li.map.is_empty() {
                li.wafers.last().map(|wf| format!("{} {}/61", l.t("последняя", "last"), wf.good))
            } else {
                let good = li.map.chars().filter(|c| *c == '.').count();
                Some(format!("{} {good}/{}", l.t("пластина", "wafer"), li.map.chars().count()))
            };
            let shift = m.shift.yield_frac().map(|f| format!("{} {}", l.t("смена", "shift"), pct(f)));
            let text = [cur, shift].into_iter().flatten().collect::<Vec<_>>().join("   ");
            g.kv(1, y + 3, l.t("годность", "yield"), if text.is_empty() { "—" } else { &text }, kw, vw, FG);
        }
        None => {
            let water = match m.kind {
                Kind::Furnace | Kind::Line | Kind::Other => "—",
                _ if m.water => l.t("есть", "on"),
                _ => l.t("нет", "none"),
            };
            g.kv(x, y + 1, l.t("вода", "water"), water, kw, vw, FG);
            let text = format!(
                "{}   {} {}",
                m.shift.products,
                l.t("простои", "downtime"),
                mmss(m.shift.downtime.iter().sum())
            );
            g.kv(1, y + 3, l.t("за смену", "shift"), &text, kw, vw, FG);
        }
    }
    g.kv(x, y + 2, l.t("выход", "output"), &zones(&m.out), kw, vw, FG);
    let power = if m.power_required {
        format!("{}/{} FE", fek(m.energy), fek(m.capacity))
    } else {
        l.t("встроенный трансф.", "built-in transformer").into()
    };
    g.kv(x, y + 3, l.t("питание", "power"), &power, kw, vw, if m.reason == "no_power" { RED } else { FG });
    if m.apc.need > 0.0 {
        let text = match m.apc.hosts.first() {
            Some(h) => format!("{} {}/{} CU", h.host, cu(m.apc.got()), cu(m.apc.need)),
            None => format!("0/{} CU", cu(m.apc.need)),
        };
        g.put(1, y + 4, "apc", DIM);
        let end = g.put_w(1 + kw, y + 4, &text, vw, FG);
        if m.apc.no_service() {
            g.put(end, y + 4, l.t(", без службы", ", no service"), DIM);
        } else if m.apc.hosts.is_empty() {
            g.put(end, y + 4, l.t(", нет компьютера", ", no computer"), DIM);
        }
    }
}

/// Вкладка «Питание»: расход и буфер по машинам.
fn power_tab(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    let bw = (g.cols - 50) / 2;
    g.fill(0, 3, g.cols, 1, FAINT);
    let (x1, x2) = (10, 10 + bw + 12);
    g.put(1, 3, l.t("ИМЯ", "NAME"), FG);
    g.put(x1, 3, l.t("РАСХОД, FE/t", "USE, FE/t"), FG);
    g.put(x2, 3, l.t("БУФЕР", "BUFFER"), FG);
    g.put(x2 + bw + 12, 3, l.t("ПРИХОД", "INCOME"), FG);
    let top = w.machines.iter().map(|m| m.usage).fold(1.0, f64::max);
    let room = (g.rows - 9).max(1) as usize;
    let first = if app.sel >= room { app.sel + 1 - room } else { 0 };
    for (i, m) in w.machines.iter().enumerate().skip(first).take(room) {
        let y = 4 + (i - first) as i32;
        let sel = i == app.sel;
        if sel {
            g.fill(0, y, g.cols, 1, ACC_D);
        }
        let back = if sel { BG } else { FAINT };
        g.put(1, y, &fit(&m.name, 8), if m.full { FG } else { DIM });
        if !m.full {
            continue;
        }
        if !m.power_required {
            g.put(x1, y, l.t("встроенный трансформатор", "built-in transformer"), DIM);
            continue;
        }
        let fault = m.reason == "no_power";
        g.bar_on(x1, y, bw, m.usage / top, if sel { ACC } else { FG }, back);
        g.put(x1 + bw + 1, y, &rfit(&fe(m.usage), 6), FG);
        let buf = if m.capacity > 0.0 { m.energy / m.capacity } else { 0.0 };
        g.bar_on(x2, y, bw, buf, if fault { RED } else { FG }, back);
        g.put(x2 + bw + 1, y, &rfit(&pct(buf), 6), if fault { RED } else { FG });
        g.put(x2 + bw + 12, y, &format!("{} FE/t", fe(m.income)), if fault { RED } else { FG });
    }
    let y = g.rows - 4;
    g.rule(0, y, g.cols, l.t("Всего", "Total"));
    let ms: Vec<&Machine> = w.machines.iter().filter(|m| m.full && m.power_required).collect();
    let (u, i, e, c) =
        ms.iter().fold((0.0, 0.0, 0.0, 0.0), |a, m| (a.0 + m.usage, a.1 + m.income, a.2 + m.energy, a.3 + m.capacity));
    let text = format!(
        "{} {} FE/t   {} {} FE/t   {} {} / {} FE",
        l.t("расход", "use"),
        fe(u),
        l.t("приход", "income"),
        fe(i),
        l.t("в буферах", "in buffers"),
        fek(e),
        fek(c)
    );
    g.put_w(1, y + 1, &text, g.cols - 2, FG);
    // хватит ли буферов, если приход пропадёт
    if u > 0.0 {
        let text = format!(
            "{} {}",
            l.t("без прихода буферов хватит на", "without income the buffers last"),
            mmss((e / u) as u32)
        );
        g.put_w(1, y + 2, &text, g.cols - 2, DIM);
    }
}

/// Вкладка «Годность»: литографы сети за смену.
fn yield_tab(g: &mut Grid, app: &App, w: &World) {
    let l = app.lang;
    g.fill(0, 3, g.cols, 1, FAINT);
    let head = format!(
        "{} {} {}   S  A  B  C  D   {}",
        fit(l.t("ИМЯ", "NAME"), 8),
        fit(l.t("ПЛАСТИН", "WAFERS"), 8),
        fit(l.t("ГОДНЫХ", "GOOD"), 7),
        l.t("БРАК ПО ПРИЧИНАМ, ПОЛЕЙ", "DEFECTS BY CAUSE, FIELDS")
    );
    g.put_w(1, 3, &head, g.cols - 2, FG);
    let mut y = 4;
    for (i, m) in w.machines.iter().enumerate() {
        if m.kind != Kind::Litho || y > g.rows - 3 {
            continue;
        }
        if i == app.sel {
            g.fill(0, y, g.cols, 1, ACC_D);
        }
        let s = &m.shift;
        let good = s.yield_frac().map(pct).unwrap_or_else(|| "—".into());
        let classes: String = (0..CLASSES.len()).map(|k| format!("{:>2} ", s.classes[k])).collect();
        let mut causes: Vec<(u32, char)> =
            CAUSES.chars().enumerate().map(|(k, c)| (s.causes[k], c)).filter(|x| x.0 > 0).collect();
        causes.sort_by_key(|a| std::cmp::Reverse(a.0));
        let causes: Vec<String> = causes.iter().map(|(n, c)| format!("{c} {n}")).collect();
        let line = format!("{} {} {}  {}  ", fit(&m.name, 8), fit(&s.wafers.to_string(), 8), fit(&good, 7), classes);
        let x = g.put_w(1, y, &line, g.cols - 2, if m.full { FG } else { DIM });
        g.put_w(x, y, &causes.join("  "), g.cols - x - 1, if causes.is_empty() { DIM } else { RED });
        y += 1;
    }
    if y == 4 {
        g.put(1, 5, l.t("в сети нет литографов", "no scanners in the network"), DIM);
    }
    g.put(1, g.rows - 3, l.t("подробный отчёт за смену: fcw yield", "the full shift report: fcw yield"), DIM);
}
