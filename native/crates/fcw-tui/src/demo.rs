//! Заготовленные кадры: тот же завод, что на макетах (docs/design/mockups). По ним идут тесты на
//! снимках экрана, скриншоты и режим `fcw <программа> --demo`, в котором программу можно
//! посмотреть без мира.

use crate::json::Json;
use crate::model::{wall_clock, World, FIELDS};

/// Сцена завода.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    /// Идёт экспозиция, поле 24.
    Run,
    /// Литограф встал без питания, три поля потеряны.
    Fault,
    /// Пластина готова.
    Done,
    /// Литограф выключен рубильником.
    Off,
    /// Служба `fcw apc` не запущена: машины получают 75 % CU.
    NoApc,
}

impl Scene {
    pub fn parse(s: &str) -> Option<Scene> {
        Some(match s {
            "run" => Scene::Run,
            "fault" => Scene::Fault,
            "done" => Scene::Done,
            "off" => Scene::Off,
            "noapc" => Scene::NoApc,
            _ => return None,
        })
    }
}

/// Карта с `good` годными полями; брак расставлен равномерно, причины — по кругу из `causes`.
fn map_with(good: usize, causes: &str) -> String {
    let bad = FIELDS - good.min(FIELDS);
    let letters: Vec<char> = causes.chars().collect();
    let mut m = vec!['.'; FIELDS];
    for k in 0..bad {
        let at = (k * FIELDS / bad.max(1) + 5 + k * 3) % FIELDS;
        let at = (0..FIELDS).map(|d| (at + d) % FIELDS).find(|i| m[*i] == '.').unwrap_or(at);
        m[at] = letters[k % letters.len().max(1)];
    }
    m.into_iter().collect()
}

fn map_from(decided: usize, bad: &[(usize, char)]) -> String {
    (0..decided).map(|i| bad.iter().find(|(n, _)| *n == i).map_or('.', |(_, c)| *c)).collect()
}

fn class(good: usize) -> &'static str {
    match good {
        58.. => "S",
        52..=57 => "A",
        43..=51 => "B",
        31..=42 => "C",
        _ => "D",
    }
}

const T0: i64 = 1_700_000_000_000;

fn lot_wafers(extra: Option<&str>) -> String {
    let goods = [52usize, 55, 49, 57, 58, 56, 59, 57, 60, 58, 59, 57, 59];
    let causes = ["PWE", "PW", "EEEPW", "PW", "PW", "PWM", "P", "PW", "W", "PW", "PW", "PW", "PW"];
    let mut out: Vec<String> = goods
        .iter()
        .zip(causes)
        .enumerate()
        .map(|(i, (g, c))| {
            format!(
                r#"{{"recipe":"wafer_cpu","good":{g},"class":"{}","map":"{}","time":{}}}"#,
                class(*g),
                map_with(*g, c),
                T0 + i as i64 * 60_000
            )
        })
        .collect();
    if let Some(map) = extra {
        let g = map.chars().filter(|c| *c == '.').count();
        out.push(format!(
            r#"{{"recipe":"wafer_cpu","good":{g},"class":"{}","map":"{map}","time":{}}}"#,
            class(g),
            T0 + 13 * 60_000 + 52_000
        ));
    }
    out.join(",")
}

fn apc(need: f64, service: bool) -> String {
    let got = if service { need } else { need * 0.75 };
    format!(
        r#"{{"cu":{got},"max":{need},"need":{need},"coverage":{},"hosts":[{{"host":"fedora-1.mc","cu":{got},"max":{need},"service":{service}}}]}}"#,
        got / need
    )
}

const NO_APC: &str = r#"{"cu":0,"max":0,"need":0,"coverage":0,"hosts":[]}"#;

fn shift(wafers: u32, good: u32, products: u32, causes: [u32; 7], classes: [u32; 5], down: [u32; 3]) -> String {
    format!(
        r#"{{"day":12,"wafers":{wafers},"good_fields":{good},"products":{products},
        "causes":{{"E":{},"P":{},"W":{},"C":{},"T":{},"M":{},"V":{}}},
        "classes":{{"S":{},"A":{},"B":{},"C":{},"D":{}}},
        "downtime_ticks":{{"no_power":{},"output_full":{},"no_catalyst":{}}}}}"#,
        causes[0],
        causes[1],
        causes[2],
        causes[3],
        causes[4],
        causes[5],
        causes[6],
        classes[0],
        classes[1],
        classes[2],
        classes[3],
        classes[4],
        down[0],
        down[1],
        down[2]
    )
}

fn litho(scene: Scene) -> String {
    let done_bad = [(0, 'W'), (8, 'P'), (17, 'W'), (29, 'E'), (30, 'E'), (31, 'E'), (44, 'P'), (57, 'W'), (60, 'M')];
    let (state, reason, phase, ticks, map, energy, income, usage) = match scene {
        Scene::Run | Scene::NoApc => {
            ("run", "", "expose", 468, map_from(23, &[(8, 'P'), (17, 'W')]), 312_000, 2048, 2048)
        }
        Scene::Fault => ("fault", "no_power", "expose", 640, map_from(32, &done_bad[..6]), 1200, 0, 0),
        Scene::Done => ("idle", "", "", 0, String::new(), 356_000, 2048, 0),
        Scene::Off => ("off", "", "", 0, String::new(), 356_000, 2048, 0),
    };
    let finished = matches!(scene, Scene::Done | Scene::Off);
    let recipe = if finished { "null".to_string() } else { r#""wafer_cpu""#.to_string() };
    let total = if finished { 0 } else { 1200 };
    let last = finished.then(|| map_from(FIELDS, &done_bad));
    let (lot, out) = if finished { (43, 3) } else { (42, 2) };
    let brownouts = if scene == Scene::Fault { 1 } else { 0 };
    let sh = shift(
        14,
        777,
        0,
        [3, 30, 24, 3, 0, 4, 0],
        [3, 7, 3, 1, 0],
        [if scene == Scene::Fault { 620 } else { 0 }, 0, 0],
    );
    format!(
        r#"{{"name":"euv-1","type":"euv_scanner_mk2","kind":"euv_scanner_mk2","state":"{state}","detail":"","stopping":false,
        "recipe":{recipe},"ticks":{ticks},"total":{total},"energy":{energy},"capacity":400000,"in":{income},"use":{usage},
        "power_required":true,"link":"cable",
        "load":{{"wafer_dock_1":[{{"id":"fcw:silicon_wafer","count":5}}],"tin":[{{"id":"fcw:tin_ingot","count":16}}]}},
        "out":{{"wafer_dock_2":[{{"id":"fcw:wafer_cpu","count":{out}}}]}},
        "reason":"{reason}","phase":"{phase}","log_seq":311,"defects":true,"apc":{},
        "litho":{{"lot":{lot},"field":{},"map":"{map}","mask":{{"id":"fcw:reticle_cpu","used":69,"max":256}},
        "env":{{"iso":6,"sealed":true,"coolant":true,"vibration":false,"brownouts":{brownouts}}},
        "lot_wafers":[{}]}},
        "sections":[{{"id":"main","state":"{}","recipe":{recipe},"ticks":{ticks},"total":{total}}}],
        "shift":{sh}}}"#,
        apc(8.0, scene != Scene::NoApc),
        map.chars().count(),
        lot_wafers(last.as_deref()),
        if state == "run" {
            "working"
        } else if state == "fault" {
            "no_power"
        } else {
            "idle"
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn simple(
    name: &str,
    kind: &str,
    state: &str,
    reason: &str,
    phase: &str,
    recipe: Option<&str>,
    ticks: u32,
    total: u32,
    usage: u32,
    link: &str,
    load: &str,
    out: &str,
    apc: &str,
    water: bool,
    products: u32,
    down: [u32; 3],
    sections: &str,
) -> String {
    let recipe = recipe.map_or("null".to_string(), |r| format!(r#""{r}""#));
    let sh = shift(0, 0, products, [0; 7], [0; 5], down);
    format!(
        r#"{{"name":"{name}","type":"{kind}","kind":"{kind}","state":"{state}","detail":"","stopping":false,
        "recipe":{recipe},"ticks":{ticks},"total":{total},"energy":310000,"capacity":400000,"in":{},"use":{usage},
        "power_required":true,"link":"{link}","load":{load},"out":{out},"reason":"{reason}","phase":"{phase}",
        "log_seq":40,"defects":true,"apc":{apc},"env":{{"water":{water},"power_pauses":0}},"sections":[{sections}],"shift":{sh}}}"#,
        if usage > 0 { usage + usage / 2 } else { 0 }
    )
}

/// Кадр `machine_frame` сцены, как его прислал бы мост мода.
pub fn frame(scene: Scene) -> String {
    let service = scene != Scene::NoApc;
    let machines = [
        simple(
            "eaf-1",
            "arc_furnace",
            "run",
            "",
            "melt",
            Some("metallurgical_silicon"),
            148,
            400,
            256,
            "cable",
            r#"{"charge":[{"id":"minecraft:quartz","count":28},{"id":"minecraft:coal","count":14}]}"#,
            r#"{"tap":[{"id":"fcw:metallurgical_silicon","count":6}]}"#,
            NO_APC,
            false,
            22,
            [840, 0, 0],
            "",
        ),
        simple(
            "cz-1",
            "crystal_puller",
            "run",
            "",
            "grow",
            Some("silicon_boule"),
            1704,
            2400,
            512,
            "cable",
            r#"{"charge":[{"id":"fcw:polysilicon","count":12}]}"#,
            r#"{"ingot":[{"id":"fcw:silicon_boule","count":2}]}"#,
            &apc(2.0, service),
            true,
            3,
            [0; 3],
            "",
        ),
        simple(
            "cz-2",
            "crystal_puller",
            "idle",
            "",
            "",
            None,
            0,
            0,
            0,
            "cable",
            r#"{"charge":[]}"#,
            r#"{"ingot":[]}"#,
            &apc(2.0, service),
            true,
            0,
            [0; 3],
            "",
        ),
        simple(
            "saw-1",
            "wafering_cell",
            "run",
            "",
            "polish",
            Some("silicon_wafer"),
            704,
            800,
            256,
            "radio",
            r#"{"ingot":[{"id":"fcw:silicon_boule","count":1}]}"#,
            r#"{"wafers":[{"id":"fcw:silicon_wafer","count":16}]}"#,
            &apc(1.0, service),
            true,
            24,
            [0; 3],
            "",
        ),
        litho(scene),
        simple(
            "line-1",
            "chip_line",
            "run",
            "",
            "takt",
            Some("cpu_lga"),
            212,
            800,
            384,
            "radio",
            r#"{"wafer":[{"id":"fcw:wafer_cpu","count":3}],"boards":[{"id":"fcw:pcb_blank","count":5}]}"#,
            r#"{"chips":[{"id":"fcw:cpu_lga","count":24}],"out":[{"id":"fcw:motherboard","count":3}]}"#,
            &apc(4.0, service),
            false,
            26,
            [0, 300, 0],
            r#"{"id":"packaging","state":"working","recipe":"cpu_lga","ticks":212,"total":800},
               {"id":"assembly","state":"working","recipe":"motherboard","ticks":310,"total":400}"#,
        ),
    ];
    format!(r#"{{"type":"machine_frame","id":1,"seq":57,"time":8336,"day":12,"machines":[{}]}}"#, machines.join(","))
}

fn log(seq: u32, at: &str, code: &str, text: &str) -> String {
    let secs: Vec<i64> = at.split(':').map(|p| p.parse().unwrap_or(0)).collect();
    let ms = (secs[0] * 3600 + secs[1] * 60 + secs[2]) * 1000;
    format!(r#"{{"seq":{seq},"time":{ms},"code":"{code}","arg":"","text":"{text}"}}"#)
}

/// Мир сцены: шесть машин сети `fedora-1.mc`, журнал и отчёт за смену.
pub fn world(scene: Scene) -> World {
    let mut w = World::default();
    let frame = Json::parse(&frame(scene)).expect("demo frame");
    w.apply_frame(&frame);
    w.host = "fedora-1.mc".into();
    let clock = |ms: i64| wall_clock(ms, 0);
    let mut euv = vec![
        log(308, "14:18:41", "wait", "ожидание: нет пластин"),
        log(309, "14:19:52", "start", "пуск wafer_cpu (fedora-1.mc)"),
        log(310, "14:19:58", "info", "пластина 43: совмещение ok"),
    ];
    match scene {
        Scene::Fault => euv.push(log(311, "14:20:31", "fault", "отказ: нет питания, поля 30–32 потеряны")),
        Scene::Done | Scene::Off => euv.push(log(311, "14:20:52", "wafer", "пластина 43 готова: 52/61, класс A")),
        _ => euv.push(log(311, "14:20:07", "info", "поле 24")),
    }
    let logs = [
        ("euv-1", euv),
        ("eaf-1", vec![log(39, "14:19:40", "info", "розжиг: дуга устойчива"), log(40, "14:19:46", "info", "плавка")]),
        (
            "cz-1",
            vec![log(39, "14:02:10", "info", "нагрев завершён, затравка"), log(40, "14:03:30", "info", "рост тела")],
        ),
        (
            "saw-1",
            vec![log(39, "14:19:58", "info", "пластина 6: проход полировки 2"), log(40, "14:20:05", "info", "рез 8")],
        ),
        (
            "line-1",
            vec![
                log(39, "14:19:31", "info", "корпусировка: пластина класса B, выход 1.6 из 2"),
                log(40, "14:20:04", "info", "сборка: плата 3 в печи"),
            ],
        ),
    ];
    for (name, entries) in logs {
        let j = Json::parse(&format!(r#"{{"log":[{}]}}"#, entries.join(","))).expect("demo log");
        w.apply_log(name, &j, &clock);
    }
    let report: Vec<String> = frame
        .items("machines")
        .iter()
        .map(|m| {
            let litho = m.get("litho").map_or(String::new(), |l| format!(r#","litho":{l}"#));
            let today = m.get("shift").map_or("{}".to_string(), Json::to_string);
            let yesterday = shift(11, 590, 0, [0, 41, 30, 6, 0, 4, 0], [1, 5, 4, 1, 0], [0, 0, 0]);
            let yesterday =
                if m.get("litho").is_some() { yesterday } else { shift(0, 0, 18, [0; 7], [0; 5], [200, 0, 0]) };
            format!(
                r#"{{"name":"{}","kind":"{}","today":{today},"yesterday":{yesterday}{litho}}}"#,
                m.text("name"),
                m.text("kind")
            )
        })
        .collect();
    w.apply_report(&Json::parse(&format!(r#"{{"machines":[{}]}}"#, report.join(","))).expect("demo report"));
    w.apply_recipes(
        "euv-1",
        &Json::parse(
            r#"{"recipes":[{"name":"wafer_cpu","section":"main","available":true,"ticks":1200,"energy_per_tick":2048},
            {"name":"wafer_memory","section":"main","available":false,"ticks":1200,"energy_per_tick":2048}]}"#,
        )
        .expect("demo recipes"),
    );
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_have_the_asked_number_of_good_fields() {
        for good in [30, 49, 57, 61] {
            let m = map_with(good, "PW");
            assert_eq!(m.chars().count(), FIELDS);
            assert_eq!(m.chars().filter(|c| *c == '.').count(), good);
        }
    }

    #[test]
    fn every_scene_parses() {
        for s in [Scene::Run, Scene::Fault, Scene::Done, Scene::Off, Scene::NoApc] {
            let w = world(s);
            assert_eq!(w.machines.len(), 6);
            assert!(w.machines.iter().all(|m| m.full));
            assert_eq!(w.report.as_ref().map(Vec::len), Some(6));
        }
    }
}
