//! Снимки экранов: каждая программа рисуется на заготовленных кадрах в 80×25 и 160×48, текст сетки
//! сравнивается с файлом в `tests/snapshots/`. Обновить снимки: `FCW_TUI_BLESS=1 cargo test -p fcw-tui`.

use fcw_tui::demo::{world, Scene};
use fcw_tui::grid::{ACC, RED};
use fcw_tui::{App, Grid, Key, Lang, Program, World};
use std::collections::HashSet;
use std::path::PathBuf;

struct Shot {
    name: &'static str,
    program: Program,
    machine: &'static str,
    scene: Scene,
    tab: usize,
}

const SHOTS: &[Shot] = &[
    Shot { name: "top", program: Program::Top, machine: "", scene: Scene::Run, tab: 0 },
    Shot { name: "top_power", program: Program::Top, machine: "", scene: Scene::Run, tab: 1 },
    Shot { name: "top_log", program: Program::Top, machine: "", scene: Scene::Run, tab: 2 },
    Shot { name: "top_yield", program: Program::Top, machine: "", scene: Scene::Run, tab: 3 },
    Shot { name: "top_noapc", program: Program::Top, machine: "", scene: Scene::NoApc, tab: 0 },
    Shot { name: "litho_run", program: Program::Litho, machine: "euv-1", scene: Scene::Run, tab: 0 },
    Shot { name: "litho_fault", program: Program::Litho, machine: "euv-1", scene: Scene::Fault, tab: 0 },
    Shot { name: "litho_done", program: Program::Litho, machine: "euv-1", scene: Scene::Done, tab: 0 },
    Shot { name: "litho_off", program: Program::Litho, machine: "euv-1", scene: Scene::Off, tab: 0 },
    Shot { name: "litho_noapc", program: Program::Litho, machine: "euv-1", scene: Scene::NoApc, tab: 0 },
    Shot { name: "litho_lot", program: Program::Litho, machine: "euv-1", scene: Scene::Run, tab: 1 },
    Shot { name: "litho_mask", program: Program::Litho, machine: "euv-1", scene: Scene::Run, tab: 2 },
    Shot { name: "litho_log", program: Program::Litho, machine: "euv-1", scene: Scene::Run, tab: 3 },
    Shot { name: "furnace", program: Program::Furnace, machine: "eaf-1", scene: Scene::Run, tab: 0 },
    Shot { name: "furnace_charge", program: Program::Furnace, machine: "eaf-1", scene: Scene::Run, tab: 1 },
    Shot { name: "puller", program: Program::Puller, machine: "cz-1", scene: Scene::Run, tab: 0 },
    Shot { name: "puller_heat", program: Program::Puller, machine: "cz-1", scene: Scene::Run, tab: 1 },
    Shot { name: "saw", program: Program::Saw, machine: "saw-1", scene: Scene::Run, tab: 0 },
    Shot { name: "saw_tool", program: Program::Saw, machine: "saw-1", scene: Scene::Run, tab: 1 },
    Shot { name: "line", program: Program::Line, machine: "line-1", scene: Scene::Run, tab: 0 },
    Shot { name: "line_yield", program: Program::Line, machine: "line-1", scene: Scene::Run, tab: 1 },
    Shot { name: "yield", program: Program::Yield, machine: "", scene: Scene::Run, tab: 0 },
    Shot { name: "yield_wafers", program: Program::Yield, machine: "", scene: Scene::Run, tab: 1 },
    Shot { name: "yield_causes", program: Program::Yield, machine: "", scene: Scene::Run, tab: 2 },
    Shot { name: "yield_downtime", program: Program::Yield, machine: "", scene: Scene::Fault, tab: 3 },
];

fn app(s: &Shot, lang: Lang) -> App {
    let mut a = App::new(s.program, s.machine, lang);
    a.tab = s.tab;
    if s.program == Program::Top {
        a.sel = 4; // euv-1, как на макете
    }
    a
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots")
}

fn check(name: &str, text: &str, failed: &mut Vec<String>) {
    let path = dir().join(format!("{name}.txt"));
    if std::env::var_os("FCW_TUI_BLESS").is_some() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    if want != text {
        failed.push(format!("{name}: снимок отличается\n--- было\n{want}--- стало\n{text}"));
    }
}

#[test]
fn screens_match_snapshots() {
    let mut failed = Vec::new();
    for s in SHOTS {
        let w = world(s.scene);
        for (cols, rows) in [(80, 25), (160, 48)] {
            let g = app(s, Lang::Ru).render(&w, cols, rows);
            check(&format!("{}_{cols}x{rows}", s.name), &g.to_text(), &mut failed);
        }
    }
    // английский — на двух экранах: слова другой длины не должны ломать раскладку
    for s in SHOTS.iter().filter(|s| matches!(s.name, "top" | "litho_run")) {
        let g = app(s, Lang::En).render(&world(s.scene), 80, 25);
        check(&format!("{}_en_80x25", s.name), &g.to_text(), &mut failed);
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Знаки, которые умеет шрифт мода: таблица PSF1 плюс то, что клиент дорисовывает сам (`font.rs`).
fn font_glyphs() -> HashSet<u32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fcw-client/assets/terminus-uni2-8x16.psf");
    let b = std::fs::read(path).expect("шрифт мода");
    assert_eq!((b[0], b[1]), (0x36, 0x04));
    let n = if b[2] & 1 != 0 { 512 } else { 256 };
    let mut have = HashSet::new();
    let mut p = 4 + n * b[3] as usize;
    let mut glyph = 0;
    while p + 1 < b.len() && glyph < n {
        let u = u16::from_le_bytes([b[p], b[p + 1]]);
        p += 2;
        match u {
            0xFFFF => glyph += 1,
            0xFFFE => {}
            u => {
                have.insert(u as u32);
            }
        }
    }
    // блоки и штриховки клиент рисует сам
    have.extend(0x2580..=0x2593);
    have
}

#[test]
fn every_glyph_is_in_the_mod_font() {
    let have = font_glyphs();
    let mut missing = HashSet::new();
    for s in SHOTS {
        let w = world(s.scene);
        for lang in [Lang::Ru, Lang::En] {
            for (cols, rows) in [(80, 24), (80, 25), (132, 43), (160, 48)] {
                let mut a = app(s, lang);
                let mut grids = vec![a.render(&w, cols, rows)];
                // окна поверх экрана
                for k in [Key::F(1), Key::F(6), Key::F(7)] {
                    a.key(k, &w);
                    grids.push(a.render(&w, cols, rows));
                    a.key(Key::Esc, &w);
                }
                for g in grids {
                    for c in g.cells() {
                        if !have.contains(&c.ch) {
                            missing.insert(char::from_u32(c.ch).unwrap_or('?'));
                        }
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "нет в шрифте мода: {missing:?}");
}

fn count_bg(g: &Grid, bg: u32) -> usize {
    g.cells().iter().filter(|c| c.bg == bg).count()
}

#[test]
fn small_terminal_gets_one_line() {
    let w = world(Scene::Run);
    let g = App::new(Program::Litho, "euv-1", Lang::Ru).render(&w, 79, 25);
    assert_eq!(g.to_text().trim_end(), "нужно не меньше 80×24");
    let g = App::new(Program::Top, "", Lang::Ru).render(&w, 80, 23);
    assert_eq!(g.to_text().trim_end(), "нужно не меньше 80×24");
    // а 80×24 уже рисуется целиком
    let g = App::new(Program::Litho, "euv-1", Lang::Ru).render(&w, 80, 24);
    assert!(g.to_text().lines().next().unwrap().contains("fcw litho"));
}

#[test]
fn lost_link_dims_the_screen_and_says_since_when() {
    let mut w = world(Scene::Run);
    w.stale = true;
    let g = App::new(Program::Litho, "euv-1", Lang::Ru).render(&w, 80, 25);
    let text = g.to_text();
    assert!(text.lines().next().unwrap().contains("НЕТ СВЯЗИ"));
    assert!(text.contains("последние данные 14:20:"), "{text}");
    // шапка красная, акцента на карте больше нет
    assert!(count_bg(&g, RED) > 0);
    let body_acc = (2..24).flat_map(|y| (0..80).map(move |x| (x, y))).filter(|(x, y)| g.cell(*x, *y).bg == ACC).count();
    assert_eq!(body_acc, 0);
}

#[test]
fn unknown_machine_waits_for_data() {
    let w = World::default();
    let g = App::new(Program::Litho, "euv-9", Lang::Ru).render(&w, 80, 25);
    assert!(g.to_text().contains("нет данных от машины"));
}

#[test]
fn machine_under_construction_is_not_a_lost_link() {
    use fcw_tui::json::Json;
    let mut w = world(Scene::Run);
    let frame = r#"{"type":"machine_frame","id":1,"seq":58,"time":8400,"day":12,
        "machines":[{"name":"euv-1","type":"euv_scanner_mk2","state":"build","stage":2,"stages":6}]}"#;
    w.apply_frame(&Json::parse(frame).unwrap());
    let text = App::new(Program::Litho, "euv-1", Lang::Ru).render(&w, 80, 25).to_text();
    assert!(text.lines().next().unwrap().contains("СТРОЙКА"), "{text}");
    assert!(text.contains("машина ещё не построена"), "{text}");
    assert!(!text.contains("НЕТ СВЯЗИ") && !text.contains("последние данные"), "{text}");
}

#[test]
fn keys_switch_tabs_and_ask_for_actions() {
    use fcw_tui::Action;
    let w = world(Scene::Run);
    let mut a = App::new(Program::Litho, "euv-1", Lang::Ru);
    assert!(a.key(Key::F(2), &w).is_empty());
    assert_eq!(a.tab, 1);
    a.key(Key::F(2), &w);
    assert_eq!(a.tab, 0);
    a.key(Key::Char('4'), &w);
    assert_eq!(a.tab, 3);
    a.key(Key::Tab, &w);
    assert_eq!(a.tab, 0);
    a.key(Key::BackTab, &w);
    assert_eq!(a.tab, 3);
    // пуск: F5 и буква, в том числе в русской раскладке
    let start = vec![Action::Exec { machine: "euv-1".into(), line: "start".into() }];
    assert_eq!(a.key(Key::F(5), &w), start);
    assert_eq!(a.key(Key::Char('s'), &w), start);
    assert_eq!(a.key(Key::Char('ы'), &w), start);
    // стоп только с подтверждением
    assert!(a.key(Key::Char('x'), &w).is_empty());
    assert!(a.key(Key::Char('n'), &w).is_empty());
    a.key(Key::F(6), &w);
    assert_eq!(a.key(Key::Char('y'), &w), vec![Action::Exec { machine: "euv-1".into(), line: "stop".into() }]);
    // рецепт: запрос списка, выбор стрелками
    assert_eq!(a.key(Key::F(7), &w), vec![Action::AskRecipes { machine: "euv-1".into() }]);
    a.key(Key::Down, &w);
    assert_eq!(a.key(Key::Enter, &w), vec![Action::Exec { machine: "euv-1".into(), line: "recipe wafer_cpu".into() }]);
    // в top и обратно
    a.key(Key::Char('t'), &w);
    assert_eq!((a.program, a.sel), (Program::Top, 4));
    a.key(Key::Up, &w);
    a.key(Key::Enter, &w);
    assert_eq!((a.program, a.machine.as_str()), (Program::Saw, "saw-1"));
    assert_eq!(a.key(Key::Char('q'), &w), vec![Action::Quit]);
}

#[test]
fn log_filter_takes_letters_that_are_commands_elsewhere() {
    let w = world(Scene::Run);
    let mut a = App::new(Program::Litho, "euv-1", Lang::Ru);
    a.key(Key::Char('4'), &w);
    a.key(Key::Char('/'), &w);
    for c in "пуск".chars() {
        assert!(a.key(Key::Char(c), &w).is_empty());
    }
    a.key(Key::Enter, &w);
    assert_eq!(a.filter, "пуск");
    let text = a.render(&w, 80, 25).to_text();
    assert!(text.contains("пуск wafer_cpu"));
    assert!(!text.contains("совмещение ok"));
}

#[test]
fn yield_saves_a_text_report() {
    use fcw_tui::Action;
    let w = world(Scene::Run);
    let mut a = App::new(Program::Yield, "", Lang::Ru);
    let acts = a.key(Key::F(8), &w);
    let [Action::Save { name, text }] = acts.as_slice() else { panic!("{acts:?}") };
    assert_eq!(name, "fcw-yield-12.txt");
    assert!(text.contains("Причины брака"));
    assert!(!text.contains("F10"));
}

#[test]
fn apc_without_service_is_a_quiet_note() {
    let w = world(Scene::NoApc);
    let litho = App::new(Program::Litho, "euv-1", Lang::Ru);
    let g = litho.render(&w, 80, 25);
    let text = g.to_text();
    assert!(text.contains("APC         6 / 8 CU  без службы"), "{text}");
    // в широком терминале — полная запись, как в документе
    let wide = litho.render(&w, 160, 48).to_text();
    assert!(wide.contains("fedora-1.mc  6 / 8 CU  без службы (75 %)"), "{wide}");
    // работа без службы — не отказ: красным остаётся только брак на карте
    let red: String = g.cells().iter().filter(|c| c.fg == RED).filter_map(|c| char::from_u32(c.ch)).collect();
    assert_eq!(red, "P09W18");
    let mut top = App::new(Program::Top, "", Lang::Ru);
    top.sel = 4;
    let text = top.render(&w, 80, 25).to_text();
    assert!(text.contains("12.8 / 17 CU  служба не запущена"), "{text}");
    assert!(text.contains("fedora-1.mc 6/8 CU, без службы"), "{text}");
}
