//! Состояние программы (вкладка, выделение, окно поверх экрана), клавиши и общий каркас экрана:
//! шапка, вкладки, строка клавиш, хвост журнала.

use crate::grid::{width, Grid, ACC, BG, DIM, FAINT, FG, RED};
use crate::model::{game_clock, Kind, LogEntry, Machine, State, World};
use crate::screens;
use crate::text::{kind_name, state_badge, Lang};

/// Наименьший терминал, в котором программы рисуют экран.
pub const MIN_COLS: usize = 80;
pub const MIN_ROWS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Top,
    Litho,
    Furnace,
    Puller,
    Saw,
    Line,
    Yield,
}

impl Program {
    pub const ALL: [Program; 7] =
        [Program::Top, Program::Litho, Program::Furnace, Program::Puller, Program::Saw, Program::Line, Program::Yield];

    pub fn name(self) -> &'static str {
        match self {
            Program::Top => "top",
            Program::Litho => "litho",
            Program::Furnace => "furnace",
            Program::Puller => "puller",
            Program::Saw => "saw",
            Program::Line => "line",
            Program::Yield => "yield",
        }
    }

    pub fn parse(s: &str) -> Option<Program> {
        Program::ALL.into_iter().find(|p| p.name() == s)
    }

    /// Программа для машины этого типа (`fcw mon`).
    pub fn for_kind(kind: Kind) -> Option<Program> {
        match kind {
            Kind::Litho => Some(Program::Litho),
            Kind::Furnace => Some(Program::Furnace),
            Kind::Puller => Some(Program::Puller),
            Kind::Saw => Some(Program::Saw),
            Kind::Line => Some(Program::Line),
            Kind::Other => None,
        }
    }

    /// Тип машины, с которой программа умеет работать; у `top` и `yield` машины нет.
    pub fn kind(self) -> Option<Kind> {
        match self {
            Program::Litho => Some(Kind::Litho),
            Program::Furnace => Some(Kind::Furnace),
            Program::Puller => Some(Kind::Puller),
            Program::Saw => Some(Kind::Saw),
            Program::Line => Some(Kind::Line),
            Program::Top | Program::Yield => None,
        }
    }

    pub fn tabs(self, l: Lang) -> Vec<&'static str> {
        let j = l.t("Журнал", "Log");
        match self {
            Program::Top => vec![l.t("Машины", "Machines"), l.t("Питание", "Power"), j, l.t("Годность", "Yield")],
            Program::Litho => vec![l.t("Пластина", "Wafer"), l.t("Партия", "Lot"), l.t("Маска", "Mask"), j],
            Program::Furnace => vec![l.t("Плавка", "Melt"), l.t("Шихта", "Charge"), j],
            Program::Puller => vec![l.t("Рост", "Growth"), l.t("Тепло", "Heat"), j],
            Program::Saw => vec![l.t("Пластины", "Wafers"), l.t("Инструмент", "Tool"), j],
            Program::Line => vec![l.t("Линия", "Line"), l.t("Годность", "Yield"), j],
            Program::Yield => {
                vec![
                    l.t("Сводка", "Summary"),
                    l.t("Пластины", "Wafers"),
                    l.t("Причины", "Causes"),
                    l.t("Простои", "Downtime"),
                ]
            }
        }
    }

    /// Номер вкладки с журналом.
    fn log_tab(self) -> Option<usize> {
        match self {
            Program::Top => Some(2),
            Program::Litho => Some(3),
            Program::Yield => None,
            _ => Some(2),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    F(u8),
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    PgUp,
    PgDn,
    Home,
    End,
    Backspace,
}

/// Что программа просит сделать снаружи: сама она ничего не шлёт и не пишет.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    /// Команда оболочки машины: `start`, `stop`, `recipe wafer_cpu`.
    Exec {
        machine: String,
        line: String,
    },
    /// Запросить список рецептов (`recipe --json`) и положить его в [`World::recipes`].
    AskRecipes {
        machine: String,
    },
    /// Записать отчёт в файл в домашнем каталоге: `fcw-yield-<день>.txt`.
    Save {
        name: String,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modal {
    None,
    Help,
    ConfirmStop(String),
    Recipes {
        machine: String,
        sel: usize,
    },
    /// Ввод фильтра журнала.
    Filter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    pub program: Program,
    /// Машина программы; у `top` и `yield` пусто.
    pub machine: String,
    pub tab: usize,
    /// Выбранная строка таблицы.
    pub sel: usize,
    /// Прокрутка журнала: сколько записей от конца скрыто.
    pub scroll: usize,
    pub modal: Modal,
    pub filter: String,
    pub lang: Lang,
    /// Без цвета: текущее поле инверсией, брак буквой в скобках.
    pub mono: bool,
    /// `yield`: показывать вчерашнюю смену.
    pub yesterday: bool,
    /// `yield`: 0 — все машины, иначе номер машины отчёта с единицы.
    pub only: usize,
}

/// Буква на той же клавише в русской раскладке: `ы` — это `s`.
fn latin(c: char) -> char {
    const RU: &str = "йцукенгшщзфывапролдячсмить";
    const EN: &str = "qwertyuiopasdfghjklzxcvbnm";
    let lower = c.to_lowercase().next().unwrap_or(c);
    match RU.chars().position(|r| r == lower) {
        Some(i) => EN.chars().nth(i).unwrap_or(lower),
        None => lower,
    }
}

impl App {
    pub fn new(program: Program, machine: &str, lang: Lang) -> App {
        App {
            program,
            machine: machine.to_string(),
            tab: 0,
            sel: 0,
            scroll: 0,
            modal: Modal::None,
            filter: String::new(),
            lang,
            mono: false,
            yesterday: false,
            only: 0,
        }
    }

    /// Машины, снимки которых программе нужны (подписка `machine_watch`, не больше восьми).
    pub fn watch(&self, w: &World) -> Vec<String> {
        match self.program {
            Program::Top | Program::Yield => {
                // окно из восьми машин вокруг выбранной: мост больше не отдаёт
                let n = w.machines.len();
                let first = if n > 8 { self.sel.saturating_sub(4).min(n - 8) } else { 0 };
                w.machines.iter().skip(first).take(8).map(|m| m.name.clone()).collect()
            }
            _ => vec![self.machine.clone()],
        }
    }

    /// Нужен ли программе отчёт за смену (`op: yield`).
    pub fn wants_report(&self) -> bool {
        self.program == Program::Yield
    }

    /// Машина, к которой относятся пуск, стоп и рецепт.
    pub fn target<'a>(&'a self, w: &'a World) -> Option<&'a str> {
        match self.program {
            Program::Top => w.machines.get(self.sel).map(|m| m.name.as_str()),
            Program::Yield => None,
            _ => Some(self.machine.as_str()),
        }
    }

    fn on_log_tab(&self) -> bool {
        self.program.log_tab() == Some(self.tab)
    }

    /// Сколько строк в таблице, по которой ходит выделение.
    fn rows(&self, w: &World) -> usize {
        match (self.program, self.tab) {
            (Program::Top, _) => w.machines.len(),
            (Program::Yield, 1) => screens::yield_::wafer_rows(self, w).len(),
            _ => 0,
        }
    }

    fn set_tab(&mut self, tab: usize) {
        if tab < self.program.tabs(self.lang).len() {
            self.tab = tab;
            self.scroll = 0;
            if self.program != Program::Top {
                self.sel = 0;
            }
        }
    }

    fn open(&mut self, program: Program, machine: &str) {
        self.program = program;
        self.machine = machine.to_string();
        self.tab = 0;
        self.scroll = 0;
        self.filter.clear();
    }

    /// Нажатие клавиши. Возвращает то, что нужно сделать снаружи.
    pub fn key(&mut self, k: Key, w: &World) -> Vec<Action> {
        match std::mem::replace(&mut self.modal, Modal::None) {
            Modal::None => {}
            Modal::Help => return Vec::new(),
            Modal::ConfirmStop(machine) => {
                return match k {
                    Key::Char(c) if latin(c) == 'y' => vec![Action::Exec { machine, line: "stop".into() }],
                    _ => Vec::new(),
                };
            }
            Modal::Recipes { machine, sel } => {
                let names = recipe_choices(w, &machine);
                let last = names.len().saturating_sub(1);
                match k {
                    Key::Up => self.modal = Modal::Recipes { machine, sel: sel.saturating_sub(1) },
                    Key::Down => self.modal = Modal::Recipes { machine, sel: (sel + 1).min(last) },
                    Key::Enter => {
                        if let Some(name) = names.get(sel) {
                            return vec![Action::Exec { machine, line: format!("recipe {name}") }];
                        }
                    }
                    Key::Esc | Key::F(10) => {}
                    Key::Char(c) if latin(c) == 'q' => {}
                    _ => self.modal = Modal::Recipes { machine, sel },
                }
                return Vec::new();
            }
            Modal::Filter => {
                match k {
                    Key::Char(c) => {
                        self.filter.push(c);
                        self.modal = Modal::Filter;
                    }
                    Key::Backspace => {
                        self.filter.pop();
                        self.modal = Modal::Filter;
                    }
                    Key::Esc => self.filter.clear(),
                    _ => {}
                }
                self.scroll = 0;
                return Vec::new();
            }
        }
        let k = match k {
            Key::Char(c) => Key::Char(latin(c)),
            other => other,
        };
        let tabs = self.program.tabs(self.lang).len();
        let rows = self.rows(w);
        match k {
            Key::F(1) | Key::Char('?') => self.modal = Modal::Help,
            Key::F(10) | Key::Char('q') | Key::Esc => return vec![Action::Quit],
            Key::Tab => self.set_tab((self.tab + 1) % tabs),
            Key::BackTab => self.set_tab((self.tab + tabs - 1) % tabs),
            Key::Char(c @ '1'..='4') => self.set_tab(c as usize - '1' as usize),
            Key::F(2) | Key::Char('p') if self.program == Program::Yield => self.yesterday = !self.yesterday,
            Key::F(3) | Key::Char('m') if self.program == Program::Yield => {
                let n = w.report.as_ref().map_or(0, Vec::len);
                self.only = (self.only + 1) % (n + 1);
                self.sel = 0;
            }
            Key::F(8) | Key::Char('w') if self.program == Program::Yield => {
                let name = format!("fcw-yield-{}.txt", w.day);
                // в файл идут те же экраны, только шире (подсказки не режутся) и без вкладок и клавиш
                let mut view = self.clone();
                let mut text = String::new();
                for tab in [0, 2, 3] {
                    view.tab = tab;
                    let screen = view.render(w, 100, 25).to_text();
                    let lines: Vec<&str> = screen.lines().collect();
                    let body = &lines[if tab == 0 { 0 } else { 2 }..lines.len() - 1];
                    let kept: Vec<&str> =
                        body.iter().enumerate().filter(|(i, _)| tab != 0 || *i != 1).map(|(_, l)| *l).collect();
                    text.push_str(kept.join("\n").trim_end());
                    text.push_str("\n\n");
                }
                return vec![Action::Save { name, text: text.trim_end().to_string() + "\n" }];
            }
            Key::F(n @ 2..=4) => {
                // повторное нажатие возвращает на первую вкладку
                let t = n as usize - 1;
                self.set_tab(if self.tab == t { 0 } else { t });
            }
            Key::F(5) | Key::Char('s') => {
                if let Some(m) = self.target(w) {
                    return vec![Action::Exec { machine: m.to_string(), line: "start".into() }];
                }
            }
            Key::F(6) | Key::Char('x') => {
                if let Some(m) = self.target(w) {
                    self.modal = Modal::ConfirmStop(m.to_string());
                }
            }
            Key::F(7) | Key::Char('r') => {
                if let Some(m) = self.target(w) {
                    let machine = m.to_string();
                    self.modal = Modal::Recipes { machine: machine.clone(), sel: 0 };
                    return vec![Action::AskRecipes { machine }];
                }
            }
            Key::F(9) | Key::Char('t') => {
                if self.program != Program::Top {
                    let from = std::mem::take(&mut self.machine);
                    self.open(Program::Top, "");
                    self.sel = w.machines.iter().position(|m| m.name == from).unwrap_or(0);
                }
            }
            Key::Enter => {
                if self.program == Program::Top {
                    if let Some(m) = w.machines.get(self.sel) {
                        if let Some(p) = Program::for_kind(m.kind) {
                            let name = m.name.clone();
                            self.open(p, &name);
                        }
                    }
                }
            }
            Key::Char('/') if self.on_log_tab() => self.modal = Modal::Filter,
            Key::Up | Key::Down | Key::PgUp | Key::PgDn | Key::Home | Key::End => {
                if self.on_log_tab() {
                    // журнал листается от конца: вверх — к старым записям
                    self.scroll = match k {
                        Key::Up => self.scroll + 1,
                        Key::Down => self.scroll.saturating_sub(1),
                        Key::PgUp => self.scroll + 10,
                        Key::PgDn => self.scroll.saturating_sub(10),
                        Key::Home => usize::MAX / 2,
                        _ => 0,
                    };
                } else if rows > 0 {
                    self.sel = match k {
                        Key::Up => self.sel.saturating_sub(1),
                        Key::Down => self.sel + 1,
                        Key::PgUp => self.sel.saturating_sub(10),
                        Key::PgDn => self.sel + 10,
                        Key::Home => 0,
                        _ => rows - 1,
                    }
                    .min(rows - 1);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Нарисовать экран в сетку `cols`×`rows`.
    pub fn render(&self, w: &World, cols: usize, rows: usize) -> Grid {
        let mut g = Grid::new(cols, rows);
        let l = self.lang;
        if cols < MIN_COLS || rows < MIN_ROWS {
            g.put(0, 0, l.t("нужно не меньше 80×24", "needs at least 80×24"), FG);
            return g;
        }
        match self.program {
            Program::Top => screens::top::draw(&mut g, self, w),
            Program::Yield => screens::yield_::draw(&mut g, self, w),
            Program::Litho => screens::litho::draw(&mut g, self, w),
            Program::Furnace => screens::furnace::draw(&mut g, self, w),
            Program::Puller => screens::puller::draw(&mut g, self, w),
            Program::Saw => screens::saw::draw(&mut g, self, w),
            Program::Line => screens::line::draw(&mut g, self, w),
        }
        // стройка и пусконаладка — не потеря связи: у такой машины снимок короткий
        let lost = w.stale
            || (self.program.kind().is_some()
                && w.machine(&self.machine).is_none_or(|m| !m.full && m.state != State::Site));
        if lost && w.time.is_some() {
            g.dim_rows(1, g.rows - 1);
            let y = g.rows - 2;
            g.fill(0, y, g.cols, 1, BG);
            let at = w.time.map(game_clock).unwrap_or_default();
            g.put(1, y, &format!("{} {at}", l.t("последние данные", "last data")), FG);
        }
        self.draw_modal(&mut g, w);
        g
    }

    fn draw_modal(&self, g: &mut Grid, w: &World) {
        let l = self.lang;
        match &self.modal {
            Modal::None => {}
            Modal::Filter => {
                let y = g.rows - 2;
                g.fill(0, y, g.cols, 1, FAINT);
                g.put(1, y, &format!("/{}", self.filter), FG);
                g.put_on(2 + width(&self.filter), y, " ", BG, FG);
            }
            Modal::ConfirmStop(m) => {
                let text = format!(
                    "{} {m}?  y — {}   {}",
                    l.t("остановить", "stop"),
                    l.t("после текущего изделия", "after the current product"),
                    l.t("любая другая клавиша — отмена", "any other key cancels")
                );
                let wd = width(&text) + 4;
                let (x, y) = ((g.cols - wd) / 2, g.rows / 2 - 1);
                g.fill(x, y, wd, 3, FAINT);
                g.put(x + 2, y + 1, &text, FG);
            }
            Modal::Recipes { machine, sel } => {
                let names = recipe_choices(w, machine);
                let loaded = w.recipes.as_ref().is_some_and(|(m, _)| m == machine);
                let wd = 46.min(g.cols - 4);
                let h = (names.len() as i32 + 4).min(g.rows - 4);
                let (x, y) = ((g.cols - wd) / 2, (g.rows - h) / 2);
                g.fill(x, y, wd, h, FAINT);
                g.put(x + 2, y, &format!("{} {machine}", l.t("Рецепт", "Recipe")), FG);
                if !loaded {
                    g.put(x + 2, y + 2, l.t("запрос…", "asking…"), DIM);
                }
                let list = w.recipes.as_ref().filter(|_| loaded).map(|(_, r)| r.as_slice()).unwrap_or(&[]);
                for (i, name) in names.iter().enumerate().take((h - 3).max(0) as usize) {
                    let row = y + 2 + i as i32;
                    let r = list.iter().find(|r| &r.name == name);
                    let note = match r {
                        None => l.t("автовыбор", "automatic").to_string(),
                        Some(r) => format!(
                            "{}  {} FE/t{}",
                            crate::text::mmss(r.ticks),
                            r.energy_per_tick,
                            if r.available { "" } else { l.t("  нет сырья", "  no input") }
                        ),
                    };
                    let avail = r.is_none_or(|r| r.available);
                    if i == *sel {
                        g.fill(x + 1, row, wd - 2, 1, ACC);
                        g.put(x + 2, row, name, BG);
                        g.put(x + 22, row, &note, BG);
                    } else {
                        g.put(x + 2, row, name, if avail { FG } else { DIM });
                        g.put_w(x + 22, row, &note, wd - 24, DIM);
                    }
                }
                g.put(x + 2, y + h - 1, l.t("Enter — закрепить   Esc — отмена", "Enter — pin   Esc — cancel"), DIM);
            }
            Modal::Help => {
                let lines: Vec<(&str, &str)> = vec![
                    ("F1  ?", l.t("эта справка", "this help")),
                    ("1…4  Tab", l.t("вкладки (F2–F4 — то же)", "tabs (F2–F4 do the same)")),
                    ("F5  s", l.t("пуск", "start")),
                    ("F6  x", l.t("стоп, с подтверждением y", "stop, confirmed with y")),
                    ("F7  r", l.t("рецепт: список, Enter", "recipe: list, Enter")),
                    ("F9  t", l.t("перейти в fcw top", "go to fcw top")),
                    ("F10  q  Esc", l.t("выход", "quit")),
                    ("↑ ↓ PgUp PgDn", l.t("строка таблицы, прокрутка журнала", "table row, log scrolling")),
                    ("Enter", l.t("top: открыть программу машины", "top: open the machine's program")),
                    ("/", l.t("журнал: фильтр", "log: filter")),
                    (
                        "p  m  w",
                        l.t("yield: смена, машина, в файл (F2 F3 F8)", "yield: shift, machine, to file (F2 F3 F8)"),
                    ),
                    ("", ""),
                    ("P W C M T V E", l.t("причины брака:", "defect causes:")),
                    ("", l.t("частицы, полировка, кристалл, маска,", "particles, polish, crystal, mask,")),
                    ("", l.t("тепло, вибрация, питание", "heat, vibration, power")),
                ];
                let wd = 62.min(g.cols - 4);
                let h = lines.len() as i32 + 4;
                let (x, y) = ((g.cols - wd) / 2, ((g.rows - h) / 2).max(1));
                g.fill(x, y, wd, h, FAINT);
                g.put(x + 2, y, &format!("fcw {} — {}", self.program.name(), l.t("клавиши", "keys")), FG);
                for (i, (k, v)) in lines.iter().enumerate() {
                    g.put(x + 2, y + 2 + i as i32, k, FG);
                    g.put_w(x + 18, y + 2 + i as i32, v, wd - 20, DIM);
                }
                g.put(x + 2, y + h - 1, l.t("любая клавиша — закрыть", "any key closes"), DIM);
            }
        }
    }
}

/// Пункты списка рецептов: `auto` и всё, что прислал мир.
fn recipe_choices(w: &World, machine: &str) -> Vec<String> {
    let mut out = vec!["auto".to_string()];
    if let Some((m, list)) = &w.recipes {
        if m == machine {
            for r in list {
                if !out.contains(&r.name) {
                    out.push(r.name.clone());
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------ каркас экрана

/// Шапка: инверсная строка с программой слева, рецептом и временем справа и состоянием перед ними.
pub(crate) fn title(g: &mut Grid, left: &str, right: &str, badge: Option<(&str, u32)>) {
    g.fill(0, 0, g.cols, 1, FG);
    let rx = g.cols - 1 - width(right);
    let room = match badge {
        Some((s, _)) => rx - 2 - width(s) - 2 - 2,
        None => rx - 2,
    };
    let cut: String = left.chars().take(room.max(0) as usize).collect();
    g.put_on(1, 0, &cut, BG, FG);
    g.put_on(rx, 0, right, BG, FG);
    if let Some((s, bg)) = badge {
        g.put_on(rx - 2 - width(s) - 2, 0, &format!(" {s} "), BG, bg);
    }
}

pub(crate) fn clock(w: &World) -> String {
    w.time.map(game_clock).unwrap_or_else(|| "--:--:--".into())
}

/// Шапка программы машины. Возвращает машину, если о ней есть полный снимок.
pub(crate) fn machine_title<'a>(g: &mut Grid, app: &App, w: &'a World) -> Option<&'a Machine> {
    let l = app.lang;
    let m = w.machine(&app.machine);
    let kind = m.map(|m| kind_name(l, m)).unwrap_or_default();
    let left = format!("fcw {}  {}  {kind}", app.program.name(), app.machine);
    let recipe = m.and_then(|m| m.recipe.clone()).unwrap_or_default();
    let right = if recipe.is_empty() { clock(w) } else { format!("{recipe}  {}", clock(w)) };
    let lost = w.stale || m.is_none_or(|m| !m.full && m.state != State::Site);
    let badge = match m {
        _ if lost => (l.t("НЕТ СВЯЗИ", "NO LINK"), RED),
        Some(m) => {
            let bg = match m.state {
                State::Run => ACC,
                State::Fault => RED,
                _ => FG,
            };
            (state_badge(l, m), bg)
        }
        None => ("", FG),
    };
    title(g, &left, &right, Some(badge));
    tabs(g, &app.program.tabs(l), app.tab);
    m.filter(|m| m.full)
}

pub(crate) fn tabs(g: &mut Grid, names: &[&str], active: usize) {
    let mut x = 1;
    for (i, n) in names.iter().enumerate() {
        let lab = format!(" {} {n} ", i + 1);
        if i == active {
            g.put_on(x, 1, &lab, BG, ACC);
        } else {
            g.put(x, 1, &lab, DIM);
        }
        x += width(&lab) + 1;
    }
}

/// Строка клавиш, как в htop: клавиша инверсией, подпись на подложке.
pub(crate) fn keys(g: &mut Grid, items: &[(&str, &str)]) {
    let y = g.rows - 1;
    g.fill(0, y, g.cols, 1, FAINT);
    let mut x = 0;
    for (k, lab) in items {
        x = g.put_on(x, y, k, BG, FG);
        x = g.put_on(x, y, &format!("{lab} "), FG, FAINT);
    }
}

/// Клавиши программы машины.
pub(crate) fn machine_keys(g: &mut Grid, app: &App) {
    let l = app.lang;
    let tabs = app.program.tabs(l);
    let mut items: Vec<(&str, &str)> = vec![("F1", l.t("Справка", "Help"))];
    const FK: [&str; 3] = ["F2", "F3", "F4"];
    for (i, t) in tabs.iter().enumerate().skip(1) {
        items.push((FK[i - 1], t));
    }
    items.extend([
        ("F5", l.t("Пуск", "Start")),
        ("F6", l.t("Стоп", "Stop")),
        ("F7", l.t("Рецепт", "Recipe")),
        ("F9", "top"),
        ("F10", l.t("Выход", "Quit")),
    ]);
    keys(g, &items);
}

/// Записи журнала, которые видит программа: своей машины (или всех) и с учётом фильтра.
pub(crate) fn log_entries<'a>(app: &App, w: &'a World, machine: Option<&str>) -> Vec<&'a LogEntry> {
    let f = app.filter.to_lowercase();
    w.logs
        .iter()
        .filter(|e| machine.is_none_or(|m| e.machine == m))
        .filter(|e| f.is_empty() || e.text.to_lowercase().contains(&f) || e.machine.contains(&f))
        .collect()
}

fn log_line(e: &LogEntry, with_machine: bool) -> String {
    if with_machine {
        format!("{}  {:<7} {}", e.clock, e.machine, e.text)
    } else {
        format!("{}  {}", e.clock, e.text)
    }
}

/// Хвост журнала в строках `y0..=y1`: последняя запись обычным цветом, отказ красным, остальное приглушено.
/// Ответ мира на команду занимает последнюю строку.
#[allow(clippy::too_many_arguments)]
pub(crate) fn log_tail(g: &mut Grid, app: &App, w: &World, machine: Option<&str>, x: i32, y0: i32, y1: i32, wd: i32) {
    if y1 < y0 {
        return;
    }
    let mut room = (y1 - y0 + 1) as usize;
    if let Some(f) = &w.flash {
        g.put_w(x, y1, &f.text, wd, if f.error { RED } else { FG });
        room -= 1;
    }
    let all = log_entries(app, w, machine);
    let skip = app.scroll.min(all.len().saturating_sub(room));
    let end = all.len() - skip;
    let shown = &all[end.saturating_sub(room)..end];
    for (i, e) in shown.iter().enumerate() {
        let last = i + 1 == shown.len() && skip == 0;
        let fg = if e.code == "fault" {
            RED
        } else if last {
            FG
        } else {
            DIM
        };
        g.put_w(x, y0 + i as i32, &log_line(e, machine.is_none()), wd, fg);
    }
}

/// Вкладка «Журнал»: весь журнал с прокруткой и фильтром.
pub(crate) fn log_tab(g: &mut Grid, app: &App, w: &World, machine: Option<&str>) {
    let l = app.lang;
    let all = log_entries(app, w, machine);
    let head = if app.filter.is_empty() {
        format!("{}: {}", l.t("записей", "records"), all.len())
    } else {
        format!("{}: {}   {} /{}", l.t("записей", "records"), all.len(), l.t("фильтр", "filter"), app.filter)
    };
    g.put(1, 3, &head, DIM);
    g.put(g.cols - 1 - width(l.t("/ — фильтр", "/ — filter")), 3, l.t("/ — фильтр", "/ — filter"), DIM);
    g.rule(1, 4, g.cols - 2, "");
    if all.is_empty() {
        g.put(2, 6, l.t("журнал пуст", "the log is empty"), DIM);
    }
    log_tail(g, app, w, machine, 1, 5, g.rows - 2, g.cols - 2);
}

/// Колонка разделов «подпись — значение» под заголовками на линейке.
pub(crate) struct Panel<'a> {
    pub g: &'a mut Grid,
    pub x: i32,
    pub w: i32,
    pub y: i32,
    /// Последняя строка, в которую можно писать.
    pub max_y: i32,
    /// Ширина столбца подписей.
    pub kw: i32,
}

impl Panel<'_> {
    pub fn sec(&mut self, title: &str) {
        if self.y <= self.max_y {
            self.g.rule(self.x, self.y, self.w, title);
        }
        self.y += 1;
    }
    pub fn row(&mut self, key: &str, value: &str, fg: u32) {
        if self.y <= self.max_y {
            self.g.kv(self.x + 1, self.y, key, value, self.kw, self.w - self.kw - 1, fg);
        }
        self.y += 1;
    }
    /// Строка с полосой: подпись, полоса, значение.
    pub fn bar(&mut self, key: &str, frac: f64, bw: i32, value: &str, fg: u32) {
        if self.y <= self.max_y {
            self.g.put_w(self.x + 1, self.y, key, self.kw - 1, DIM);
            self.g.bar(self.x + 1 + self.kw, self.y, bw, frac, fg);
            self.g.put_w(self.x + 2 + self.kw + bw, self.y, value, self.w - self.kw - bw - 3, FG);
        }
        self.y += 1;
    }
    pub fn gap(&mut self) {
        self.y += 1;
    }
    /// Значение с приглушённой припиской: `6 / 8 CU` + `без службы (75 %)`.
    pub fn row_note(&mut self, key: &str, value: &str, note: &str) {
        if self.y <= self.max_y {
            self.g.kv(self.x + 1, self.y, key, value, self.kw, self.w - self.kw - 1, FG);
            let nx = self.x + 1 + self.kw + width(value) + 2;
            self.g.put_w(nx, self.y, note, self.x + self.w - nx, DIM);
        }
        self.y += 1;
    }
}

/// Строка APC раздела «Среда»: получено / нужно и причина, если меньше максимума.
pub(crate) fn apc_row(p: &mut Panel, l: Lang, m: &Machine, extra: &str) {
    use crate::text::cu;
    let a = &m.apc;
    if a.need <= 0.0 {
        return;
    }
    let Some(h) = a.hosts.first() else {
        p.row_note("APC", &format!("0 / {} CU", cu(a.need)), l.t("нет компьютера", "no computer"));
        return;
    };
    let amount = format!("{} / {} CU", cu(a.got()), cu(a.need));
    let (short, long) = if a.no_service() {
        (l.t("без службы", "no service"), l.t("без службы (75 %)", "no service (75 %)"))
    } else if a.got() + 1e-6 < a.need {
        (l.t("слабый CPU", "small CPU"), l.t("процессор слабее", "CPU too small"))
    } else {
        ("", extra)
    };
    // в узкой колонке остаётся главное: сколько получено и почему меньше
    let room = p.w - p.kw - 1;
    let full = format!("{}  {amount}", h.host);
    if width(&full) + 2 + width(long) <= room || short.is_empty() && width(&full) <= room {
        p.row_note("APC", &full, long);
    } else if width(&amount) + 2 + width(long) <= room {
        p.row_note("APC", &amount, long);
    } else {
        p.row_note("APC", &amount, short);
    }
}

/// Строка «питание» раздела условий.
pub(crate) fn power_row(p: &mut Panel, l: Lang, m: &Machine) {
    use crate::text::{fe, pct};
    let key = l.t("питание", "power");
    if !m.power_required {
        p.row(key, l.t("встроенный трансформатор", "built-in transformer"), FG);
    } else if m.state == State::Fault && m.reason == "no_power" {
        p.row(key, &format!("{} {} FE/t", l.t("приход", "income"), fe(m.income)), RED);
    } else {
        let buf = if m.capacity > 0.0 { m.energy / m.capacity } else { 0.0 };
        p.row(key, &format!("{} FE/t   {} {}", fe(m.usage), l.t("буфер", "buffer"), pct(buf)), FG);
    }
}

/// Пустой графический объект выключенной машины: надпись в центре рамки.
pub(crate) fn off_note(g: &mut Grid, l: Lang, x: i32, y: i32, w: i32, h: i32) {
    let t = l.t("рубильник выключен", "switched off");
    g.put(x + (w - width(t)) / 2, y + h / 2, t, DIM);
}

/// Правая колонка широкого терминала (от 120 столбцов): журнал и итоги смены видны без переключения.
/// Возвращает ширину, которая остаётся основной раскладке.
pub(crate) fn side_column(g: &mut Grid, app: &App, w: &World, m: &Machine) -> i32 {
    if g.cols < 120 {
        return g.cols;
    }
    let l = app.lang;
    let main = (g.cols * 2 / 3).max(80);
    let x = main + 1;
    let wd = g.cols - x - 1;
    let mut p = Panel { g, x, w: wd, y: 2, max_y: 8, kw: 12 };
    p.sec(l.t("Смена", "Shift"));
    let s = &m.shift;
    if m.kind == Kind::Litho {
        let y = s.yield_frac().map(crate::text::pct).unwrap_or_else(|| "—".into());
        p.row(l.t("пластин", "wafers"), &format!("{}   {} {y}", s.wafers, l.t("годных", "good")), FG);
    } else {
        p.row(l.t("изделий", "products"), &s.products.to_string(), FG);
    }
    let down = s.downtime.iter().sum::<u32>();
    p.row(l.t("простои", "downtime"), &crate::text::mmss(down), FG);
    let top = p.y + 1;
    let rows = g.rows;
    g.rule(x, top, wd, l.t("Журнал", "Log"));
    log_tail(g, app, w, Some(&m.name), x + 1, top + 1, rows - 2, wd - 1);
    main
}
