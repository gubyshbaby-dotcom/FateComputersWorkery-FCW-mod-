//! Полноэкранные программы в госте: `fcw top`, `fcw litho <имя>`, … (docs/design/CONSOLE_APPS.md).
//!
//! Экран рисует крейт `fcw-tui`; здесь — всё, что вокруг: терминал, подписка на снимки машин через
//! службу агента (`machine_watch` → `machine_frame`), команды (`machine_req`) и потеря связи.

use crate::machine::{self, NoAnswer, Resp};
use crate::term::{self, Color, Terminal};
use fcw_tui::demo;
use fcw_tui::json::{obj, s, Json};
use fcw_tui::model::{Flash, Kind};
use fcw_tui::{Action, App, Grid, Key, Lang, Program, World};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Кадры идут 4 раза в секунду; столько тишины — и на экране «НЕТ СВЯЗИ».
const STALE_AFTER: Duration = Duration::from_millis(1500);
const LIST_EVERY: Duration = Duration::from_secs(2);
const REPORT_EVERY: Duration = Duration::from_secs(5);
const FLASH_FOR: Duration = Duration::from_secs(4);

pub const USAGE: &str = "usage:\n  \
    fcw top                         all machines of this computer's network\n  \
    fcw litho|furnace|puller|saw|line [name]   the program of one machine\n  \
    fcw mon <name>                  the program that fits the machine\n  \
    fcw yield [name]                shift report\n\
    options: --mono, --color truecolor|256|16|mono, --lang ru|en, --once (print one screen), --demo [run|fault|done|off|noapc]\n\
    the same programs answer to the names fcw-top, fcw-litho, ...";

/// Что запускать: программа и её параметры из командной строки.
#[derive(Debug, Clone, PartialEq)]
pub struct Opts {
    /// `None` — `fcw mon`: программа выбирается по типу машины.
    pub program: Option<Program>,
    pub machine: Option<String>,
    pub color: Option<Color>,
    pub lang: Option<Lang>,
    pub once: bool,
    pub demo: Option<demo::Scene>,
}

/// Разбор аргументов после имени программы (`top`, `litho`, …, `mon`).
pub fn parse_args(program: &str, args: &[&str]) -> Result<Opts, String> {
    let prog = match program {
        "mon" => None,
        p => Some(Program::parse(p).ok_or_else(|| format!("unknown program: {p}"))?),
    };
    let mut o = Opts { program: prog, machine: None, color: None, lang: None, once: false, demo: None };
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match *a {
            "--mono" => o.color = Some(Color::Mono),
            "--once" => o.once = true,
            "--color" => {
                let v = it.next().ok_or("--color needs truecolor, 256, 16 or mono")?;
                o.color = Some(Color::parse(v).ok_or_else(|| format!("bad --color value: {v}"))?);
            }
            "--lang" => {
                let v = it.next().ok_or("--lang needs ru or en")?;
                o.lang = Some(Lang::from_code(v));
            }
            "--demo" => {
                let scene = it.peek().and_then(|v| demo::Scene::parse(v));
                if scene.is_some() {
                    it.next();
                }
                o.demo = Some(scene.unwrap_or(demo::Scene::Run));
            }
            x if x.starts_with('-') => return Err(format!("unknown option: {x}")),
            x if o.machine.is_none() => o.machine = Some(x.to_string()),
            x => return Err(format!("unexpected argument: {x}")),
        }
    }
    if prog.is_none() && o.machine.is_none() && o.demo.is_none() {
        return Err("mon needs a machine name".into());
    }
    if prog == Some(Program::Top) && o.machine.is_some() {
        return Err("top takes no machine name".into());
    }
    Ok(o)
}

/// Программа и машина, с которыми запускаться, по списку машин сети.
/// Ошибка — текст для пользователя; код выхода 2.
pub fn resolve(o: &Opts, list: &Json, ru: bool) -> Result<(Program, String), String> {
    let machines: Vec<(&str, Kind)> =
        list.items("machines").iter().map(|m| (m.text("name"), Kind::parse(m.text("kind")))).collect();
    let find = |name: &str| machines.iter().find(|(n, _)| *n == name).copied();
    let none = |name: &str| {
        if ru {
            format!("нет машины {name} в сети этого компьютера")
        } else {
            format!("no machine {name} in this computer's network")
        }
    };
    match (o.program, &o.machine) {
        (Some(Program::Top), _) => Ok((Program::Top, String::new())),
        (Some(Program::Yield), None) => Ok((Program::Yield, String::new())),
        (Some(Program::Yield), Some(name)) => {
            find(name).map(|_| (Program::Yield, name.clone())).ok_or_else(|| none(name))
        }
        (None, Some(name)) => {
            let (_, kind) = find(name).ok_or_else(|| none(name))?;
            let p = Program::for_kind(kind).ok_or_else(|| {
                if ru {
                    format!("для машины {name} программы нет")
                } else {
                    format!("there is no program for machine {name}")
                }
            })?;
            Ok((p, name.clone()))
        }
        (Some(p), Some(name)) => {
            let (_, kind) = find(name).ok_or_else(|| none(name))?;
            match Program::for_kind(kind) {
                Some(right) if right != p => Err(if ru {
                    format!("{name} — машина другого типа: fcw {} {name}", right.name())
                } else {
                    format!("{name} is a different kind of machine: fcw {} {name}", right.name())
                }),
                _ => Ok((p, name.clone())),
            }
        }
        (Some(p), None) => {
            // имя можно не писать, если такая машина в сети одна
            let fit: Vec<&str> = machines.iter().filter(|(_, k)| Some(*k) == p.kind()).map(|(n, _)| *n).collect();
            match fit.as_slice() {
                [one] => Ok((p, one.to_string())),
                [] => Err(if ru {
                    format!("в сети этого компьютера нет машин для fcw {}", p.name())
                } else {
                    format!("no machines for fcw {} in this computer's network", p.name())
                }),
                many => Err(if ru {
                    format!("укажите машину: fcw {} <{}>", p.name(), many.join("|"))
                } else {
                    format!("name the machine: fcw {} <{}>", p.name(), many.join("|"))
                }),
            }
        }
        (None, None) => Err("mon needs a machine name".into()),
    }
}

// ------------------------------------------------------------ события и задания

enum Event {
    Keys(Vec<Key>),
    /// Строка `machine_frame` от службы агента.
    Frame(Json),
    Reply(Job, Result<Resp, NoAnswer>),
}

/// Запрос в мир, который выполняет рабочий поток.
#[derive(Clone, Debug, PartialEq)]
enum Job {
    List,
    Log { machine: String },
    Recipes { machine: String },
    Exec { machine: String, line: String },
    Report,
}

fn job_request(job: &Job, world: &World, lang: &str) -> Json {
    match job {
        Job::List => machine::request("list", None, None, true, lang),
        Job::Log { machine } => {
            let line = format!("log --since {}", world.log_seq(machine));
            machine::request("exec", Some(machine), Some(&line), true, lang)
        }
        Job::Recipes { machine } => machine::request("exec", Some(machine), Some("recipe"), true, lang),
        Job::Exec { machine, line } => machine::request("exec", Some(machine), Some(line), false, lang),
        Job::Report => machine::request("yield", None, None, true, lang),
    }
}

/// Часы записи журнала в поясе гостя.
fn local_clock(ms: i64) -> String {
    // тип секунд берётся из сигнатуры localtime_r: в musl он меняется между версиями
    let secs = [(ms / 1000) as _];
    // SAFETY: localtime_r читает секунды из своего массива и пишет в свою структуру tm.
    let off = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(secs.as_ptr(), &mut tm).is_null() {
            0
        } else {
            tm.tm_gmtoff as i64
        }
    };
    fcw_tui::model::wall_clock(ms, off)
}

/// Подписка на снимки: соединение со службой агента, по которому она шлёт `machine_frame`.
/// Поток сам переподключается; набор машин меняется записью новой строки `watch`.
struct Watch {
    wanted: Arc<Mutex<Vec<String>>>,
    stream: Arc<Mutex<Option<UnixStream>>>,
}

fn watch_line(machines: &[String]) -> String {
    let list = Json::Arr(machines.iter().map(|m| s(m)).collect());
    format!("{}\n", obj(vec![("cmd", s("watch")), ("machines", list), ("hz", Json::Num(4.0))]))
}

impl Watch {
    fn start(sock: String, tx: Sender<Event>) -> Watch {
        let w = Watch { wanted: Arc::new(Mutex::new(Vec::new())), stream: Arc::new(Mutex::new(None)) };
        let (wanted, stream) = (w.wanted.clone(), w.stream.clone());
        std::thread::spawn(move || loop {
            if let Ok(mut st) = UnixStream::connect(&sock) {
                let line = watch_line(&wanted.lock().unwrap());
                if st.write_all(line.as_bytes()).is_ok() {
                    *stream.lock().unwrap() = st.try_clone().ok();
                    for line in BufReader::new(&st).lines() {
                        let Ok(line) = line else { break };
                        if let Some(j) = Json::parse(line.trim()) {
                            if j.text("type") == "machine_frame" && tx.send(Event::Frame(j)).is_err() {
                                return;
                            }
                        }
                    }
                }
                *stream.lock().unwrap() = None;
            }
            // службы нет или она перезапускается (самообновление): пробуем снова
            std::thread::sleep(Duration::from_secs(1));
        });
        w
    }

    fn set(&self, machines: Vec<String>) {
        let mut wanted = self.wanted.lock().unwrap();
        if *wanted == machines {
            return;
        }
        *wanted = machines;
        if let Some(st) = self.stream.lock().unwrap().as_mut() {
            let _ = st.write_all(watch_line(&wanted).as_bytes());
        }
    }
}

fn print_once(app: &App, world: &World) {
    print!("{}", app.render(world, 80, 25).to_text());
}

/// Запуск программы. Возвращает код выхода процесса.
pub fn run(o: &Opts, sock: &str, env_lang: &str) -> i32 {
    let lang = o.lang.unwrap_or_else(|| Lang::from_code(env_lang));
    let ru = lang == Lang::Ru;
    let lang_code = if ru { "ru_ru" } else { "en_us" };
    let color = o.color.unwrap_or_else(|| Color::detect(|k| std::env::var(k).ok()));
    let call = |req: &Json| crate::machine_call(sock, req);

    let mut world = World::default();
    let (program, machine) = match o.demo {
        Some(scene) => {
            world = demo::world(scene);
            let list = Json::Obj(vec![(
                "machines".into(),
                Json::Arr(
                    world.machines.iter().map(|m| obj(vec![("name", s(&m.name)), ("kind", s(&m.kind_id))])).collect(),
                ),
            )]);
            match resolve(o, &list, ru) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("fcw: {e}");
                    return 2;
                }
            }
        }
        None => {
            let list = match call(&machine::request("list", None, None, true, lang_code)) {
                Ok(Resp { json: Some(j), .. }) => j,
                Ok(r) => {
                    eprintln!("fcw: {}", r.text);
                    return 2;
                }
                Err(NoAnswer) => {
                    eprintln!("fcw: {}", machine::no_answer_text(lang_code));
                    return 2;
                }
            };
            world.apply_list(&list);
            world.host = crate::hostname();
            match resolve(o, &list, ru) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("fcw: {e}");
                    return 2;
                }
            }
        }
    };
    let mut app = if program == Program::Yield {
        let mut a = App::new(Program::Yield, "", lang);
        a.only = world.machines.iter().position(|m| m.name == machine).map_or(0, |i| i + 1);
        a
    } else {
        App::new(program, &machine, lang)
    };
    app.mono = color == Color::Mono;

    if o.demo.is_some() && o.once {
        print_once(&app, &world);
        return 0;
    }

    let (tx, rx) = mpsc::channel::<Event>();
    let (job_tx, job_rx) = mpsc::channel::<(Job, Json)>();
    let live = o.demo.is_none();
    let watch = live.then(|| Watch::start(sock.to_string(), tx.clone()));
    if live {
        // рабочий поток: запросы в мир по одному, ответ — событием
        let (tx, sock) = (tx.clone(), sock.to_string());
        std::thread::spawn(move || {
            for (job, req) in job_rx {
                let r = crate::machine_call(&sock, &req);
                if tx.send(Event::Reply(job, r)).is_err() {
                    break;
                }
            }
        });
    }

    if o.once {
        // один экран без терминала: дождаться первого кадра (и отчёта, если он нужен) и напечатать
        if let Some(w) = &watch {
            w.set(app.watch(&world));
        }
        if app.wants_report() {
            if let Ok(Resp { json: Some(j), .. }) = call(&machine::request("yield", None, None, true, lang_code)) {
                world.apply_report(&j);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(Event::Frame(j)) = rx.recv_timeout(Duration::from_millis(200)) {
                world.apply_frame(&j);
                break;
            }
        }
        for m in app.watch(&world) {
            if let Ok(Resp { json: Some(j), .. }) =
                call(&job_request(&Job::Log { machine: m.clone() }, &world, lang_code))
            {
                world.apply_log(&m, &j, &local_clock);
            }
        }
        print_once(&app, &world);
        return 0;
    }

    let Some(term) = Terminal::open() else {
        eprintln!("fcw: {}", if ru { "нужен терминал (или --once)" } else { "a terminal is needed (or --once)" });
        return 2;
    };
    {
        // клавиши: читает отдельный поток, чтобы экран обновлялся и без нажатий
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            let mut stdin = std::io::stdin().lock();
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(Event::Keys(vec![Key::F(10)]));
                        return;
                    }
                    Ok(n) => {
                        if tx.send(Event::Keys(term::parse_keys(&buf[..n]))).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }

    let mut shown: Option<Grid> = None;
    let mut last_frame = Instant::now();
    let mut got_frame = !live;
    let mut last_list = Instant::now();
    let mut last_report: Option<Instant> = None;
    let mut flash_at: Option<Instant> = None;
    let mut pending: HashSet<String> = HashSet::new();
    let mut busy = (false, false); // идут запросы списка и отчёта
    let send = |job: Job, world: &World| {
        let req = job_request(&job, world, lang_code);
        let _ = job_tx.send((job, req));
    };

    'main: loop {
        let event = rx.recv_timeout(Duration::from_millis(100));
        let mut events: Vec<Event> = event.into_iter().collect();
        events.extend(rx.try_iter());
        for ev in events {
            match ev {
                Event::Keys(keys) => {
                    for k in keys {
                        for act in app.key(k, &world) {
                            match act {
                                Action::Quit => break 'main,
                                Action::Exec { machine, line } if live => send(Job::Exec { machine, line }, &world),
                                Action::AskRecipes { machine } if live => {
                                    world.recipes = None;
                                    send(Job::Recipes { machine }, &world);
                                }
                                Action::Exec { .. } | Action::AskRecipes { .. } => {
                                    let text = if ru {
                                        "демо: команда в мир не отправлена"
                                    } else {
                                        "demo: nothing was sent to the world"
                                    };
                                    world.flash = Some(Flash { text: text.into(), error: false });
                                    flash_at = Some(Instant::now());
                                }
                                Action::Save { name, text } => {
                                    let dir = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                                    let path = std::path::Path::new(&dir).join(&name);
                                    let (text, error) = match std::fs::write(&path, text) {
                                        Ok(()) => (
                                            format!("{} {}", if ru { "записано:" } else { "saved:" }, path.display()),
                                            false,
                                        ),
                                        Err(e) => (format!("{}: {e}", path.display()), true),
                                    };
                                    world.flash = Some(Flash { text, error });
                                    flash_at = Some(Instant::now());
                                }
                            }
                        }
                    }
                }
                Event::Frame(j) => {
                    world.apply_frame(&j);
                    last_frame = Instant::now();
                    got_frame = true;
                    // новые записи журнала: у кого номер последней записи вырос
                    let behind: Vec<String> = j
                        .items("machines")
                        .iter()
                        .filter(|m| m.num("log_seq") as u64 > world.log_seq(m.text("name")))
                        .map(|m| m.text("name").to_string())
                        .collect();
                    for m in behind {
                        if pending.insert(m.clone()) {
                            send(Job::Log { machine: m }, &world);
                        }
                    }
                }
                Event::Reply(job, r) => match (job, r) {
                    (Job::List, r) => {
                        busy.0 = false;
                        if let Ok(Resp { json: Some(j), .. }) = r {
                            world.apply_list(&j);
                        }
                    }
                    (Job::Report, r) => {
                        busy.1 = false;
                        if let Ok(Resp { json: Some(j), .. }) = r {
                            world.apply_report(&j);
                        }
                    }
                    (Job::Log { machine }, r) => {
                        pending.remove(&machine);
                        if let Ok(Resp { json: Some(j), exit: 0, .. }) = r {
                            world.apply_log(&machine, &j, &local_clock);
                        }
                    }
                    (Job::Recipes { machine }, Ok(Resp { json: Some(j), .. })) => world.apply_recipes(&machine, &j),
                    (Job::Recipes { .. }, _) => {}
                    (Job::Exec { machine, .. }, r) => {
                        let (text, error) = match r {
                            Ok(r) => (format!("{machine}: {}", r.text.lines().next().unwrap_or("ok")), r.exit != 0),
                            Err(NoAnswer) => (machine::no_answer_text(lang_code).to_string(), true),
                        };
                        world.flash = Some(Flash { text, error });
                        flash_at = Some(Instant::now());
                    }
                },
            }
        }
        if live {
            world.stale = got_frame && last_frame.elapsed() > STALE_AFTER && !app.watch(&world).is_empty();
            if let Some(w) = &watch {
                w.set(app.watch(&world));
            }
            if !busy.0 && last_list.elapsed() >= LIST_EVERY {
                busy.0 = true;
                last_list = Instant::now();
                send(Job::List, &world);
            }
            if app.wants_report() && !busy.1 && last_report.is_none_or(|t| t.elapsed() >= REPORT_EVERY) {
                busy.1 = true;
                last_report = Some(Instant::now());
                send(Job::Report, &world);
            }
        }
        if flash_at.is_some_and(|t| t.elapsed() > FLASH_FOR) {
            world.flash = None;
            flash_at = None;
        }
        // размер берётся заново на каждом шаге: так же, как по SIGWINCH, но без обработчика сигнала
        let (cols, rows) = term.size();
        let next = app.render(&world, cols, rows);
        let out = term::diff(shown.as_ref(), &next, color);
        if !out.is_empty() {
            term.write(&out);
        }
        shown = Some(next);
    }
    drop(term);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Json {
        Json::parse(
            r#"{"machines":[{"name":"euv-1","kind":"euv_scanner_mk2"},{"name":"cz-1","kind":"crystal_puller"},
            {"name":"cz-2","kind":"crystal_puller"},{"name":"x-1","kind":"mystery"}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn args() {
        let o = parse_args("litho", &["euv-1", "--mono"]).unwrap();
        assert_eq!(
            (o.program, o.machine.as_deref(), o.color),
            (Some(Program::Litho), Some("euv-1"), Some(Color::Mono))
        );
        let o = parse_args("top", &["--demo", "fault", "--once"]).unwrap();
        assert_eq!((o.demo, o.once), (Some(demo::Scene::Fault), true));
        let o = parse_args("litho", &["--demo", "euv-1"]).unwrap();
        assert_eq!((o.demo, o.machine.as_deref()), (Some(demo::Scene::Run), Some("euv-1")));
        assert!(parse_args("mon", &[]).is_err());
        assert!(parse_args("top", &["euv-1"]).is_err());
        assert!(parse_args("litho", &["--frobnicate"]).is_err());
        assert!(parse_args("nope", &[]).is_err());
    }

    #[test]
    fn mon_picks_the_program_by_machine_kind() {
        let o = parse_args("mon", &["cz-2"]).unwrap();
        assert_eq!(resolve(&o, &list(), true), Ok((Program::Puller, "cz-2".into())));
        let o = parse_args("mon", &["x-1"]).unwrap();
        assert_eq!(resolve(&o, &list(), true), Err("для машины x-1 программы нет".into()));
    }

    #[test]
    fn missing_machine_is_named() {
        let o = parse_args("litho", &["euv-9"]).unwrap();
        assert_eq!(resolve(&o, &list(), true), Err("нет машины euv-9 в сети этого компьютера".into()));
        assert_eq!(resolve(&o, &list(), false), Err("no machine euv-9 in this computer's network".into()));
    }

    #[test]
    fn name_may_be_left_out_when_the_machine_is_the_only_one() {
        let o = parse_args("litho", &[]).unwrap();
        assert_eq!(resolve(&o, &list(), true), Ok((Program::Litho, "euv-1".into())));
        let o = parse_args("puller", &[]).unwrap();
        assert_eq!(resolve(&o, &list(), true), Err("укажите машину: fcw puller <cz-1|cz-2>".into()));
        let o = parse_args("saw", &[]).unwrap();
        assert!(resolve(&o, &list(), true).unwrap_err().contains("нет машин для fcw saw"));
        let o = parse_args("litho", &["cz-1"]).unwrap();
        assert_eq!(resolve(&o, &list(), true), Err("cz-1 — машина другого типа: fcw puller cz-1".into()));
    }

    #[test]
    fn jobs_become_machine_requests() {
        let w = World::default();
        let r = job_request(&Job::Log { machine: "euv-1".into() }, &w, "ru_ru");
        assert_eq!(r.text("line"), "log --since 0");
        assert_eq!(r.text("machine"), "euv-1");
        assert!(r.flag("json"));
        let r = job_request(&Job::Report, &w, "ru_ru");
        assert_eq!(r.text("op"), "yield");
        let r = job_request(&Job::Exec { machine: "euv-1".into(), line: "start".into() }, &w, "en_us");
        assert_eq!((r.text("op"), r.text("line"), r.flag("json")), ("exec", "start", false));
    }

    #[test]
    fn watch_request_names_the_machines() {
        assert_eq!(
            watch_line(&["euv-1".into(), "cz-1".into()]),
            "{\"cmd\":\"watch\",\"machines\":[\"euv-1\",\"cz-1\"],\"hz\":4}\n"
        );
    }
}
