//! Сетка программы на заготовленном кадре — текстом или ячейками (для отрисовки в PNG шрифтом мода).
//!
//! `cargo run -p fcw-tui --example dump -- <программа> <машина|-> <сцена> <вкладка> <столбцы> <строки> [ru|en] [cells] [stale]`
//! `cargo run -p fcw-tui --example dump -- frame <сцена>` печатает кадр `machine_frame` этой сцены.
//! Сцены: run, fault, done, off, noapc. С `cells` печатает строку `cols rows` и по строке
//! `знак fg bg` (шестнадцатеричные) на ячейку.

use fcw_tui::demo::{world, Scene};
use fcw_tui::{App, Lang, Program};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let usage = "dump <program> <machine|-> <scene> <tab> <cols> <rows> [ru|en] [cells] [stale]";
    if a.len() < 6 && a.first().map(String::as_str) != Some("frame") {
        eprintln!("{usage}");
        std::process::exit(2);
    }
    if a[0] == "frame" {
        // кадр сцены одной строкой: для проверок службы агента без мира
        let scene = Scene::parse(&a[1]).expect(usage);
        println!("{}", fcw_tui::json::Json::parse(&fcw_tui::demo::frame(scene)).expect("demo frame"));
        return;
    }
    let program = Program::parse(&a[0]).expect(usage);
    let machine = if a[1] == "-" { "" } else { a[1].as_str() };
    let mut w = world(Scene::parse(&a[2]).expect(usage));
    let (cols, rows): (usize, usize) = (a[4].parse().expect(usage), a[5].parse().expect(usage));
    let lang = if a.iter().any(|x| x == "en") { Lang::En } else { Lang::Ru };
    let mut app = App::new(program, machine, lang);
    app.tab = a[3].parse().expect(usage);
    if program == Program::Top {
        app.sel = 4;
    }
    w.stale = a.iter().any(|x| x == "stale");
    let g = app.render(&w, cols, rows);
    if a.iter().any(|x| x == "cells") {
        println!("{} {}", g.cols, g.rows);
        for c in g.cells() {
            println!("{:x} {:06x} {:06x}", c.ch, c.fg, c.bg);
        }
    } else {
        print!("{}", g.to_text());
    }
}
