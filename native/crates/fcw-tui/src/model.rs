//! Данные программ: снимки машин из `machine_frame`, журнал, отчёт за смену.
//! Состав снимка — то, что отдаёт мост мода (`status --json`, docs/CONTRACT.md §7).

use crate::json::Json;

/// Число полей на пластине и ряды карты (docs/design/DEFECTS.md, раздел 2).
pub const FIELDS: usize = 61;
pub const ROWS61: [i32; 9] = [3, 7, 7, 9, 9, 9, 7, 7, 3];
/// Причины брака в том порядке, в каком их считает мод.
pub const CAUSES: &str = "EPWCTMV";
pub const CLASSES: &str = "SABCD";

/// Поля в порядке экспозиции (змейкой): (номер, столбец, ряд).
pub fn fields() -> Vec<(usize, i32, i32)> {
    let mut out = Vec::with_capacity(FIELDS);
    for (r, w) in ROWS61.iter().enumerate() {
        let first = (9 - w) / 2;
        for i in 0..*w {
            let c = if r % 2 == 0 { first + i } else { first + w - 1 - i };
            out.push((out.len(), c, r as i32));
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Furnace,
    Puller,
    Saw,
    Litho,
    Line,
    Other,
}

impl Kind {
    pub fn parse(s: &str) -> Kind {
        match s {
            "arc_furnace" => Kind::Furnace,
            "crystal_puller" => Kind::Puller,
            "wafering_cell" => Kind::Saw,
            "chip_line" => Kind::Line,
            x if x.starts_with("euv_scanner") => Kind::Litho,
            _ => Kind::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off,
    Idle,
    Run,
    Fault,
    /// Стройка, разбор, пусконаладка: машина ещё не машина.
    Site,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stack {
    /// Предмет без пространства имён: `silicon_wafer`.
    pub id: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zone {
    pub id: String,
    pub items: Vec<Stack>,
}

impl Zone {
    pub fn count(&self) -> u32 {
        self.items.iter().map(|s| s.count).sum()
    }
    /// `silicon_wafer x5  tin x16` или пусто.
    pub fn text(&self) -> String {
        self.items.iter().map(|s| format!("{} x{}", s.id, s.count)).collect::<Vec<_>>().join("  ")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApcHost {
    pub host: String,
    pub cu: f64,
    pub max: f64,
    pub service: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Apc {
    pub cu: f64,
    pub need: f64,
    pub coverage: f64,
    pub hosts: Vec<ApcHost>,
}

impl Apc {
    /// Получено, но не больше нужного: так это показывают экраны.
    pub fn got(&self) -> f64 {
        self.cu.min(self.need)
    }
    /// Компьютер есть, а служба `fcw apc` на нём не работает.
    pub fn no_service(&self) -> bool {
        !self.hosts.is_empty() && self.hosts.iter().all(|h| !h.service)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wafer {
    pub recipe: String,
    pub good: u32,
    pub class: String,
    pub map: String,
    /// Когда готова, мс эпохи (часы сервера); 0 — мост старой версии.
    pub time: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mask {
    pub id: String,
    pub used: u32,
    pub max: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Litho {
    /// Сколько пластин эта машина уже сделала.
    pub lot: u32,
    /// Идущая пластина: по знаку на решённое поле, `.` — годно, буква — причина брака.
    pub map: String,
    pub mask: Option<Mask>,
    /// 0 — помещение открыто, иначе класс ISO.
    pub iso: u32,
    pub coolant: bool,
    pub vibration: bool,
    pub brownouts: u32,
    /// Последние готовые пластины, старые первыми.
    pub wafers: Vec<Wafer>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shift {
    pub day: u32,
    pub wafers: u32,
    pub good_fields: u32,
    pub products: u32,
    /// По порядку [`CAUSES`].
    pub causes: [u32; 7],
    /// По порядку [`CLASSES`].
    pub classes: [u32; 5],
    /// Тики простоя: нет питания, выход заполнен, нет маски.
    pub downtime: [u32; 3],
}

impl Shift {
    pub fn parse(j: &Json) -> Shift {
        let mut s = Shift {
            day: j.num("day") as u32,
            wafers: j.num("wafers") as u32,
            good_fields: j.num("good_fields") as u32,
            products: j.num("products") as u32,
            ..Shift::default()
        };
        if let Some(c) = j.get("causes") {
            for (i, k) in CAUSES.chars().enumerate() {
                s.causes[i] = c.num(&k.to_string()) as u32;
            }
        }
        if let Some(c) = j.get("classes") {
            for (i, k) in CLASSES.chars().enumerate() {
                s.classes[i] = c.num(&k.to_string()) as u32;
            }
        }
        if let Some(d) = j.get("downtime_ticks") {
            s.downtime = [d.num("no_power") as u32, d.num("output_full") as u32, d.num("no_catalyst") as u32];
        }
        s
    }
    /// Доля годных полей, 0..1; пластин не было — `None`.
    pub fn yield_frac(&self) -> Option<f64> {
        (self.wafers > 0).then(|| self.good_fields as f64 / (self.wafers as f64 * FIELDS as f64))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub id: String,
    /// `off`, `idle`, `working`, `no_power`, `output_full`, `no_catalyst`.
    pub state: String,
    pub recipe: Option<String>,
    pub ticks: u32,
    pub total: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    pub name: String,
    pub kind: Kind,
    /// Тип как его называет мод: `euv_scanner_mk2`.
    pub kind_id: String,
    pub state: State,
    /// Слово состояния из мода: для стройки и пусконаладки.
    pub state_id: String,
    /// Причина отказа: `no_power`, `output_full`, `no_catalyst`.
    pub reason: String,
    pub phase: String,
    pub stopping: bool,
    pub recipe: Option<String>,
    pub ticks: u32,
    pub total: u32,
    pub energy: f64,
    pub capacity: f64,
    /// Приход и расход, FE/t.
    pub income: f64,
    pub usage: f64,
    pub power_required: bool,
    pub load: Vec<Zone>,
    pub out: Vec<Zone>,
    /// `cable` или `radio`.
    pub link: String,
    pub log_seq: u64,
    /// Модель брака включена (уровни engineer и fab).
    pub defects: bool,
    pub apc: Apc,
    pub litho: Option<Litho>,
    pub water: bool,
    pub power_pauses: u32,
    pub shift: Shift,
    pub sections: Vec<Section>,
    /// Есть полный снимок (а не одна строка списка сети).
    pub full: bool,
}

fn zones(j: &Json, key: &str) -> Vec<Zone> {
    j.fields(key)
        .iter()
        .map(|(id, v)| Zone {
            id: id.clone(),
            items: match v {
                Json::Arr(a) => a
                    .iter()
                    .map(|s| Stack { id: short_id(s.text("id")).to_string(), count: s.num("count") as u32 })
                    .collect(),
                _ => Vec::new(),
            },
        })
        .collect()
}

/// `fcw:reticle_cpu` → `reticle_cpu`.
pub fn short_id(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

fn litho(j: &Json) -> Litho {
    let env = j.get("env").cloned().unwrap_or(Json::Null);
    Litho {
        lot: j.num("lot") as u32,
        map: j.text("map").to_string(),
        mask: j.get("mask").map(|m| Mask {
            id: short_id(m.text("id")).to_string(),
            used: m.num("used") as u32,
            max: m.num("max") as u32,
        }),
        iso: env.num("iso") as u32,
        coolant: env.flag("coolant"),
        vibration: env.flag("vibration"),
        brownouts: env.num("brownouts") as u32,
        wafers: j
            .items("lot_wafers")
            .iter()
            .map(|w| Wafer {
                recipe: w.text("recipe").to_string(),
                good: w.num("good") as u32,
                class: w.text("class").to_string(),
                map: w.text("map").to_string(),
                time: w.num("time") as i64,
            })
            .collect(),
    }
}

impl Machine {
    /// Машина, о которой известна только строка списка сети.
    pub fn stub(name: &str, kind_id: &str, state: &str, link: &str) -> Machine {
        Machine {
            name: name.to_string(),
            kind: Kind::parse(kind_id),
            kind_id: kind_id.to_string(),
            state: state_of(state),
            state_id: state.to_string(),
            reason: String::new(),
            phase: String::new(),
            stopping: false,
            recipe: None,
            ticks: 0,
            total: 0,
            energy: 0.0,
            capacity: 0.0,
            income: 0.0,
            usage: 0.0,
            power_required: false,
            load: Vec::new(),
            out: Vec::new(),
            link: link.to_string(),
            log_seq: 0,
            defects: false,
            apc: Apc::default(),
            litho: None,
            water: false,
            power_pauses: 0,
            shift: Shift::default(),
            sections: Vec::new(),
            full: false,
        }
    }

    /// Один элемент `machines` из `machine_frame`.
    pub fn parse(j: &Json) -> Option<Machine> {
        let name = j.text("name");
        if name.is_empty() {
            return None;
        }
        let kind_id = if j.text("kind").is_empty() { j.text("type") } else { j.text("kind") };
        let mut m = Machine::stub(name, kind_id, j.text("state"), j.text("link"));
        m.reason = j.text("reason").to_string();
        m.phase = j.text("phase").to_string();
        m.stopping = j.flag("stopping");
        m.recipe = j.get("recipe").and_then(Json::as_str).map(str::to_string);
        m.ticks = j.num("ticks") as u32;
        m.total = j.num("total") as u32;
        m.energy = j.num("energy");
        m.capacity = j.num("capacity");
        m.income = j.num("in");
        m.usage = j.num("use");
        m.power_required = j.flag("power_required");
        m.load = zones(j, "load");
        m.out = zones(j, "out");
        m.log_seq = j.num("log_seq") as u64;
        m.defects = j.flag("defects");
        if let Some(a) = j.get("apc") {
            m.apc = Apc {
                cu: a.num("cu"),
                need: a.num("need"),
                coverage: a.num("coverage"),
                hosts: a
                    .items("hosts")
                    .iter()
                    .map(|h| ApcHost {
                        host: h.text("host").to_string(),
                        cu: h.num("cu"),
                        max: h.num("max"),
                        service: h.flag("service"),
                    })
                    .collect(),
            };
        }
        m.litho = j.get("litho").map(litho);
        if let Some(e) = j.get("env") {
            m.water = e.flag("water");
            m.power_pauses = e.num("power_pauses") as u32;
        }
        if let Some(s) = j.get("shift") {
            m.shift = Shift::parse(s);
        }
        m.sections = j
            .items("sections")
            .iter()
            .map(|s| Section {
                id: s.text("id").to_string(),
                state: s.text("state").to_string(),
                recipe: s.get("recipe").and_then(Json::as_str).map(str::to_string),
                ticks: s.num("ticks") as u32,
                total: s.num("total") as u32,
            })
            .collect();
        // стройка и пусконаладка отдают короткий снимок без рецепта и зон
        m.full = j.get("load").is_some();
        Some(m)
    }

    /// Ход рецепта, 0..1.
    pub fn progress(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.ticks as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }

    pub fn zone<'a>(zones: &'a [Zone], id: &str) -> Option<&'a Zone> {
        zones.iter().find(|z| z.id == id)
    }
}

fn state_of(s: &str) -> State {
    match s {
        "off" => State::Off,
        "run" => State::Run,
        "fault" | "error" => State::Fault,
        "idle" => State::Idle,
        _ => State::Site,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogEntry {
    pub machine: String,
    pub seq: u64,
    /// Мс эпохи по часам сервера.
    pub time: i64,
    /// `14:20:07` — уже в поясе терминала (форматирует тот, кто принёс запись).
    pub clock: String,
    pub code: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    pub name: String,
    pub section: String,
    pub available: bool,
    pub ticks: u32,
    pub energy_per_tick: u32,
}

/// Строка отчёта `op: yield` — одна машина.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportMachine {
    pub name: String,
    pub kind: Kind,
    pub today: Shift,
    pub yesterday: Shift,
    pub litho: Option<Litho>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flash {
    pub text: String,
    pub error: bool,
}

/// Всё, что программа знает о мире в момент отрисовки.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    /// Имя этого компьютера в сети: `fedora-1.mc`.
    pub host: String,
    /// Машины сети; порядок — как в списке сети.
    pub machines: Vec<Machine>,
    /// Игровое время суток в тиках (0 — 06:00) и номер дня; `None` — кадров ещё не было.
    pub time: Option<u32>,
    pub day: u32,
    /// Кадры перестали приходить: данные на экране — последние известные.
    pub stale: bool,
    /// Журнал всех машин по возрастанию времени.
    pub logs: Vec<LogEntry>,
    pub report: Option<Vec<ReportMachine>>,
    /// Ответ на запрос списка рецептов: машина и её рецепты.
    pub recipes: Option<(String, Vec<Recipe>)>,
    /// Ответ мира на последнюю команду.
    pub flash: Option<Flash>,
}

impl World {
    pub fn machine(&self, name: &str) -> Option<&Machine> {
        self.machines.iter().find(|m| m.name == name)
    }

    /// Ответ `list --json`: новые машины добавляются, пропавшие убираются, известные сохраняют снимок.
    pub fn apply_list(&mut self, j: &Json) {
        let mut next = Vec::new();
        for e in j.items("machines") {
            let name = e.text("name");
            if name.is_empty() {
                continue;
            }
            match self.machines.iter().position(|m| m.name == name) {
                Some(i) => {
                    let mut m = self.machines.swap_remove(i);
                    m.link = e.text("link").to_string();
                    if !m.full {
                        m.state = state_of(e.text("state"));
                        m.state_id = e.text("state").to_string();
                    }
                    next.push(m);
                }
                None => next.push(Machine::stub(name, e.text("kind"), e.text("state"), e.text("link"))),
            }
        }
        self.machines = next;
    }

    /// Кадр `machine_frame`: снимки заменяют прежние; машины, которых в кадре нет, не трогаются.
    pub fn apply_frame(&mut self, j: &Json) {
        if let Some(t) = j.get("time").and_then(Json::as_f64) {
            self.time = Some(t as u32 % 24000);
            self.day = j.num("day") as u32;
        }
        for e in j.items("machines") {
            let Some(mut m) = Machine::parse(e) else { continue };
            match self.machines.iter_mut().find(|x| x.name == m.name) {
                Some(old) => {
                    if m.link.is_empty() {
                        m.link = std::mem::take(&mut old.link);
                    }
                    *old = m;
                }
                None => self.machines.push(m),
            }
            for h in e.get("apc").map(|a| a.items("hosts")).unwrap_or(&[]) {
                if self.host.is_empty() || !self.host.contains('.') {
                    self.host = h.text("host").to_string();
                }
            }
        }
        self.stale = false;
    }

    /// Ответ `log --json`: новые записи машины.
    pub fn apply_log(&mut self, machine: &str, j: &Json, clock: &dyn Fn(i64) -> String) {
        let known = self.logs.iter().filter(|e| e.machine == machine).map(|e| e.seq).max().unwrap_or(0);
        for e in j.items("log") {
            let seq = e.num("seq") as u64;
            if seq <= known {
                continue;
            }
            let time = e.num("time") as i64;
            self.logs.push(LogEntry {
                machine: machine.to_string(),
                seq,
                time,
                clock: clock(time),
                code: e.text("code").to_string(),
                text: e.text("text").to_string(),
            });
        }
        self.logs.sort_by_key(|e| (e.time, e.seq));
        // хватит и последних записей: машина сама хранит 64
        let extra = self.logs.len().saturating_sub(512);
        self.logs.drain(..extra);
    }

    /// Наибольший известный номер записи журнала машины.
    pub fn log_seq(&self, machine: &str) -> u64 {
        self.logs.iter().filter(|e| e.machine == machine).map(|e| e.seq).max().unwrap_or(0)
    }

    /// Ответ `recipe --json`.
    pub fn apply_recipes(&mut self, machine: &str, j: &Json) {
        let list = j
            .items("recipes")
            .iter()
            .map(|r| Recipe {
                name: r.text("name").to_string(),
                section: r.text("section").to_string(),
                available: r.flag("available"),
                ticks: r.num("ticks") as u32,
                energy_per_tick: r.num("energy_per_tick") as u32,
            })
            .collect();
        self.recipes = Some((machine.to_string(), list));
    }

    /// Ответ `op: yield`.
    pub fn apply_report(&mut self, j: &Json) {
        let list = j
            .items("machines")
            .iter()
            .map(|m| ReportMachine {
                name: m.text("name").to_string(),
                kind: Kind::parse(m.text("kind")),
                today: m.get("today").map(Shift::parse).unwrap_or_default(),
                yesterday: m.get("yesterday").map(Shift::parse).unwrap_or_default(),
                litho: m.get("litho").map(litho),
            })
            .collect();
        self.report = Some(list);
    }
}

/// Игровое время суток (тики, 0 — 06:00) как `14:20:07`.
pub fn game_clock(ticks: u32) -> String {
    let secs = (ticks as u64 % 24000) * 86400 / 24000 + 6 * 3600;
    format!("{:02}:{:02}:{:02}", secs / 3600 % 24, secs / 60 % 60, secs % 60)
}

/// Мс эпохи как `14:20:07` со сдвигом пояса в секундах.
pub fn wall_clock(ms: i64, offset_secs: i64) -> String {
    let s = (ms.div_euclid(1000) + offset_secs).rem_euclid(86400);
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serpentine_matches_the_mod() {
        let f = fields();
        assert_eq!(f.len(), FIELDS);
        assert_eq!(f[0], (0, 3, 0));
        assert_eq!(f[2], (2, 5, 0));
        // второй ряд идёт справа налево
        assert_eq!(f[3], (3, 7, 1));
        assert_eq!(f[9], (9, 1, 1));
        assert_eq!(f[60], (60, 5, 8));
    }

    #[test]
    fn clocks() {
        assert_eq!(game_clock(0), "06:00:00");
        assert_eq!(game_clock(6000), "12:00:00");
        assert_eq!(game_clock(18000), "00:00:00");
        assert_eq!(wall_clock(1000 * (14 * 3600 + 20 * 60 + 7), 0), "14:20:07");
        assert_eq!(wall_clock(0, 3 * 3600), "03:00:00");
    }

    #[test]
    fn frame_replaces_and_list_prunes() {
        let mut w = World::default();
        let list = Json::parse(
            r#"{"machines":[{"name":"euv-1","kind":"euv_scanner_mk2","state":"run","link":"cable"},
                {"name":"cz-1","kind":"crystal_puller","state":"idle","link":"radio"}]}"#,
        )
        .unwrap();
        w.apply_list(&list);
        assert_eq!(w.machines.len(), 2);
        assert!(!w.machines[0].full);
        let frame = Json::parse(
            r#"{"type":"machine_frame","id":1,"seq":2,"time":8338,"day":12,"machines":[{"name":"euv-1","kind":"euv_scanner_mk2",
            "state":"run","recipe":"wafer_cpu","ticks":468,"total":1200,"load":{},"out":{},
            "apc":{"cu":8,"max":8,"need":8,"coverage":1,"hosts":[{"host":"fedora-1.mc","cu":8,"max":8,"service":true}]},
            "litho":{"lot":42,"field":3,"map":".P.","env":{"iso":6,"coolant":true},"lot_wafers":[]}}]}"#,
        )
        .unwrap();
        w.apply_frame(&frame);
        let m = w.machine("euv-1").unwrap();
        assert!(m.full);
        assert_eq!(m.link, "cable");
        assert_eq!(m.litho.as_ref().unwrap().map, ".P.");
        assert_eq!(w.host, "fedora-1.mc");
        assert_eq!(w.day, 12);
        let list =
            Json::parse(r#"{"machines":[{"name":"euv-1","kind":"euv_scanner_mk2","state":"run","link":"cable"}]}"#)
                .unwrap();
        w.apply_list(&list);
        assert_eq!(w.machines.len(), 1);
        assert!(w.machines[0].full);
    }
}
