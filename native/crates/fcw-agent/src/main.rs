//! `fcw` — агент гостя FCW (CONTRACT.md §7).
//!
//! * `fcw agent` — служба: порт virtio-serial `fcw.agent` (строки JSON с
//!   хостом) и локальный сокет `/run/fcw/agent.sock` для команд;
//! * `fcw redstone get [side]`, `fcw redstone set <side> <0-15>`;
//! * `fcw machine …` — производственные машины мира (модуль `machine`);
//! * `fcw top`, `fcw litho <имя>`, …, `fcw yield`, `fcw mon <имя>` — полноэкранные программы
//!   (модуль `tui`; экраны рисует крейт `fcw-tui`); те же программы отзываются на имена
//!   `fcw-top`, `fcw-litho`, … — ссылки на этот бинарник;
//! * `fcw apc on|off|status` — служба вычислений для машин (модуль `apc`);
//! * `fcw info`, `fcw send '<json>'` (произвольное сообщение хосту).
//!
//! Служба ещё и обновляет сама себя: мод присылает свежий бинарник (модуль `update`).

mod apc;
mod hash;
mod machine;
mod term;
mod tui;
mod update;

use fcw_tui::json::{n, obj, s, Json};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PORT: &str = "/dev/virtio-ports/fcw.agent";
const SOCK_DIR: &str = "/run/fcw";
const SOCK: &str = "/run/fcw/agent.sock";
const SIDES: [&str; 6] = ["front", "back", "left", "right", "top", "bottom"];

fn usage() -> ! {
    eprintln!(
        "fcw {VERSION} — Fate Computers Workery guest agent\n\n\
         usage:\n  \
         fcw agent [--port PATH] [--socket PATH]   run the service\n  \
         fcw redstone get [side]                    input levels (all sides or one)\n  \
         fcw redstone set <side> <0-15>             set output level\n  \
         fcw machine list [--json]                  production machines this computer can reach\n  \
         fcw machine <name> <command...> [--json]   status, start, stop, recipe, log, help\n  \
         fcw machine <name> log [n] --follow        print new log records as they appear\n  \
         fcw machine <name> wait <state> [--timeout N]   wait for off, idle, run or fault\n  \
         fcw top | litho | furnace | puller | saw | line [name]   full-screen programs (also fcw-top, ...)\n  \
         fcw mon <name>                             the program that fits the machine\n  \
         fcw yield [name]                           shift report\n  \
         fcw apc on|off|status                      compute service for the machines\n  \
         fcw info                                   agent status\n  \
         fcw send '<json>'                          send a JSON message to the world\n\n\
         sides: front back left right top bottom"
    );
    std::process::exit(2)
}

/// Имена, под которыми бинарник запускает программу сразу: `fcw-litho euv-1` = `fcw litho euv-1`.
const APPLETS: [&str; 9] = ["top", "litho", "furnace", "puller", "saw", "line", "yield", "mon", "apc"];

/// Программа по имени, под которым вызвали бинарник (`argv[0]`): `/usr/local/bin/fcw-litho` → `litho`.
fn applet(argv0: &str) -> Option<&'static str> {
    let name = Path::new(argv0).file_name()?.to_str()?;
    let rest = name.strip_prefix("fcw-")?;
    APPLETS.iter().find(|a| **a == rest).copied()
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(a) = std::env::args().next().as_deref().and_then(applet) {
        args.insert(0, a.to_string());
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match a.as_slice() {
        ["agent", rest @ ..] => {
            let mut port = PORT.to_string();
            let mut sock = SOCK.to_string();
            let mut it = rest.iter();
            while let Some(o) = it.next() {
                match *o {
                    "--port" => port = it.next().unwrap_or_else(|| usage()).to_string(),
                    "--socket" => sock = it.next().unwrap_or_else(|| usage()).to_string(),
                    _ => usage(),
                }
            }
            agent(&port, &sock)
        }
        ["redstone", "get"] => cli(obj(vec![("cmd", s("redstone_get"))]), |r| {
            let inputs = r.get("inputs");
            let line: Vec<String> = SIDES
                .iter()
                .map(|sd| format!("{sd}={}", inputs.and_then(|i| i.get(sd)).and_then(Json::as_f64).unwrap_or(0.0)))
                .collect();
            println!("{}", line.join(" "));
        }),
        ["redstone", "get", side] => {
            let side = norm_side(side).unwrap_or_else(|| usage());
            cli(obj(vec![("cmd", s("redstone_get"))]), |r| {
                let v = r.get("inputs").and_then(|i| i.get(side)).and_then(Json::as_f64).unwrap_or(0.0);
                println!("{v}");
            })
        }
        ["redstone", "set", side, level] => {
            let side = norm_side(side).unwrap_or_else(|| usage());
            let level: u32 = level.parse().ok().filter(|l| *l <= 15).unwrap_or_else(|| usage());
            cli(obj(vec![("cmd", s("redstone_set")), ("side", s(side)), ("level", n(level))]), |_| {})
        }
        ["info"] => cli(obj(vec![("cmd", s("info"))]), |r| {
            if let Json::Obj(o) = r {
                for (k, v) in o {
                    if k != "ok" {
                        match v {
                            Json::Str(x) => println!("{k}: {x}"),
                            other => println!("{k}: {other}"),
                        }
                    }
                }
            }
        }),
        ["machine", rest @ ..] => machine_cli(rest),
        ["apc", rest @ ..] => apc_cli(rest),
        [program @ ("top" | "litho" | "furnace" | "puller" | "saw" | "line" | "yield" | "mon"), rest @ ..] => {
            match tui::parse_args(program, rest) {
                Ok(o) => tui::run(&o, &sock_path(), &machine::lang_from_env(|k| std::env::var(k).ok())),
                Err(e) => {
                    eprintln!("fcw: {e}\n{}", tui::USAGE);
                    2
                }
            }
        }
        ["send", msg] => {
            if Json::parse(msg).is_none() {
                eprintln!("fcw: not valid JSON");
                std::process::exit(2);
            }
            cli(obj(vec![("cmd", s("send")), ("json", s(msg))]), |_| {})
        }
        ["--version"] | ["version"] => {
            println!("fcw {VERSION}");
            0
        }
        _ => usage(),
    };
    std::process::exit(code);
}

fn norm_side(side: &str) -> Option<&'static str> {
    let l = side.to_ascii_lowercase();
    let l = match l.as_str() {
        "up" => "top",
        "down" => "bottom",
        x => x,
    };
    SIDES.iter().find(|s| **s == l).copied()
}

// ------------------------------------------------------------ CLI

/// Сокет службы; `FCW_AGENT_SOCK` — для проверок вне гостя.
fn sock_path() -> String {
    std::env::var("FCW_AGENT_SOCK").ok().filter(|p| !p.is_empty()).unwrap_or_else(|| SOCK.to_string())
}

/// Один запрос к службе: строка JSON туда, строка JSON обратно.
fn ask(sock: &str, req: &Json, timeout: Duration) -> Result<Json, String> {
    let mut st = UnixStream::connect(sock)
        .map_err(|e| format!("cannot connect to agent ({sock}): {e}. Is `fcw agent` running?"))?;
    let _ = st.set_read_timeout(Some(timeout));
    writeln!(st, "{req}").map_err(|_| "agent connection failed".to_string())?;
    let mut line = String::new();
    if BufReader::new(&st).read_line(&mut line).is_err() || line.is_empty() {
        return Err("no answer from agent".into());
    }
    Json::parse(line.trim()).ok_or_else(|| "bad answer from agent".to_string())
}

fn cli(req: Json, show: impl FnOnce(&Json)) -> i32 {
    let resp = match ask(&sock_path(), &req, Duration::from_secs(5)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fcw: {e}");
            return 1;
        }
    };
    if resp.get("ok").and_then(Json::as_bool) != Some(true) {
        eprintln!("fcw: {}", resp.get("error").and_then(Json::as_str).unwrap_or("error"));
        return 1;
    }
    show(&resp);
    0
}

/// Запрос `machine_req` через службу; ответа нет за 5 с — [`machine::NoAnswer`].
pub(crate) fn machine_call(sock: &str, req: &Json) -> Result<machine::Resp, machine::NoAnswer> {
    let wrapped = obj(vec![("cmd", s("machine")), ("req", req.clone())]);
    // служба сама ждёт мир 5 с; секунда сверху — чтобы её ответ «timeout» успел дойти
    let resp = ask(sock, &wrapped, machine::TIMEOUT + Duration::from_secs(1)).map_err(|_| machine::NoAnswer)?;
    if resp.get("ok").and_then(Json::as_bool) != Some(true) {
        return Err(machine::NoAnswer);
    }
    resp.get("resp").and_then(machine::parse_resp).ok_or(machine::NoAnswer)
}

/// `fcw apc on|off|status`: флаг ставит служба агента (у неё есть права на `/var/lib/fcw`);
/// если службы нет — пробуем записать сами.
fn apc_cli(args: &[&str]) -> i32 {
    let cmd = match apc::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("fcw: {e}\n{}", apc::USAGE);
            return 2;
        }
    };
    let lang = machine::lang_from_env(|k| std::env::var(k).ok());
    let ru = lang.starts_with("ru");
    let sock = sock_path();
    let set = |on: bool, machines: &[String]| -> Result<(), String> {
        let list = Json::Arr(machines.iter().map(|m| s(m)).collect());
        let req = obj(vec![("cmd", s("apc")), ("on", Json::Bool(on)), ("machines", list)]);
        match ask(&sock, &req, Duration::from_secs(5)) {
            Ok(r) if r.flag("ok") => Ok(()),
            Ok(r) => Err(r.text("error").to_string()),
            // службы нет: флаг всё равно можно поставить, она подхватит его при запуске
            Err(_) => {
                let path = apc::flag_path();
                let r = if on { apc::write_flag(&path, machines) } else { apc::clear_flag(&path) };
                r.map_err(|e| format!("{}: {e}", path.display()))
            }
        }
    };
    match &cmd {
        apc::Cmd::On(machines) => {
            if let Err(e) = set(true, machines) {
                eprintln!("fcw: {e}");
                return 1;
            }
        }
        apc::Cmd::Off => {
            if let Err(e) = set(false, &[]) {
                eprintln!("fcw: {e}");
                return 1;
            }
        }
        apc::Cmd::Status => {}
    }
    let flag = apc::read_flag(&apc::flag_path());
    match (&flag, ru) {
        (Some(m), true) if m.is_empty() => println!("служба APC: включена, для всех машин сети"),
        (Some(m), true) => println!("служба APC: включена, для машин: {}", m.join(" ")),
        (None, true) => println!("служба APC: выключена — машины получают 75 % вычислительной мощности"),
        (Some(m), false) if m.is_empty() => println!("APC service: on, for every machine of the network"),
        (Some(m), false) => println!("APC service: on, for machines: {}", m.join(" ")),
        (None, false) => println!("APC service: off — machines get 75 % of the compute units"),
    }
    if cmd != apc::Cmd::Status {
        // мир считает CU раз в секунду, а остановку службы замечает через три пропуска
        println!(
            "{}",
            if ru {
                "машины учтут это за несколько секунд"
            } else {
                "the machines follow within a few seconds"
            }
        );
    }
    let call = |req: &Json| machine_call(&sock, req);
    match call(&machine::request("list", None, None, true, &lang)) {
        Ok(machine::Resp { json: Some(list), .. }) => {
            for m in list.items("machines") {
                let name = m.text("name");
                if let Ok(machine::Resp { json: Some(st), .. }) =
                    call(&machine::request("exec", Some(name), Some("status"), true, &lang))
                {
                    if let Some(line) = apc::machine_line(name, &st, ru) {
                        println!("{line}");
                    }
                }
            }
        }
        _ => eprintln!("fcw: {}", machine::no_answer_text(&lang)),
    }
    if flag.is_some() {
        0
    } else {
        1
    }
}

fn machine_cli(args: &[&str]) -> i32 {
    let cmd = match machine::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("fcw: {e}\n{}", machine::USAGE);
            return 2;
        }
    };
    let lang = machine::lang_from_env(|k| std::env::var(k).ok());
    let sock = sock_path();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    machine::run(
        &cmd,
        &lang,
        &mut machine::Io {
            out: &mut stdout.lock(),
            err: &mut stderr.lock(),
            call: &mut |req| machine_call(&sock, req),
            sleep: &mut |d| {
                std::thread::sleep(d);
                true
            },
        },
    )
}

// ------------------------------------------------------------ служба

struct State {
    inputs: [u32; 6],
    outputs: [u32; 6],
    last_host: Option<Instant>,
    started: Instant,
}

struct Agent {
    port: Mutex<Option<File>>,
    st: Mutex<State>,
    /// Запросы `machine_req`, ждущие ответа мира, по `id`.
    pending: Mutex<HashMap<u64, mpsc::Sender<Json>>>,
    next_id: AtomicU64,
    /// Свой бинарник: его хэш уходит в `hello`, его же подменяет обновление.
    exe: PathBuf,
    exe_sha256: String,
    update: Mutex<Option<update::Update>>,
    /// Перезапуск после обновления (в тестах — заглушка).
    restart: Box<dyn Fn() + Send + Sync>,
    /// Сколько ждать ответа мира на `machine_req`.
    world_timeout: Duration,
    /// Программы, подписанные на снимки машин (`machine_frame`).
    watchers: Mutex<Vec<Watcher>>,
    /// Файл-флаг службы вычислений (`fcw apc on`).
    apc_flag: PathBuf,
}

/// Одна программа (`fcw top`, `fcw litho`, …) на локальном сокете: какие машины ей нужны и куда слать кадры.
struct Watcher {
    id: u64,
    machines: Vec<String>,
    out: UnixStream,
}

/// Мост мода отдаёт снимки не больше чем восьми машин и не чаще 4 раз в секунду.
const MAX_WATCHED: usize = 8;

impl Agent {
    fn new(port: File, exe: PathBuf, restart: Box<dyn Fn() + Send + Sync>) -> Agent {
        Agent {
            port: Mutex::new(Some(port)),
            st: Mutex::new(State { inputs: [0; 6], outputs: [0; 6], last_host: None, started: Instant::now() }),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            exe_sha256: hash::sha256_file(&exe).unwrap_or_default(),
            exe,
            update: Mutex::new(None),
            restart,
            world_timeout: machine::TIMEOUT,
            watchers: Mutex::new(Vec::new()),
            apc_flag: apc::flag_path(),
        }
    }

    /// Подписка у мира одна на компьютер: шлём объединение того, что нужно всем программам.
    fn rewatch(&self) {
        let mut all: Vec<String> = Vec::new();
        let any = {
            let ws = self.watchers.lock().unwrap();
            for m in ws.iter().flat_map(|w| &w.machines) {
                if all.len() < MAX_WATCHED && !all.contains(m) {
                    all.push(m.clone());
                }
            }
            !ws.is_empty()
        };
        if any && !all.is_empty() {
            let list = Json::Arr(all.iter().map(|m| s(m)).collect());
            self.send(&obj(vec![("type", s("machine_watch")), ("id", n(1)), ("machines", list), ("hz", n(4))]));
        } else {
            self.send(&obj(vec![("type", s("machine_unwatch"))]));
        }
    }

    /// Кадр `machine_frame` от мира — всем подписанным программам; кто не читает, тот отключается.
    fn forward_frame(&self, line: &str) {
        let mut ws = self.watchers.lock().unwrap();
        let before = ws.len();
        ws.retain_mut(|w| w.out.write_all(line.as_bytes()).and_then(|_| w.out.write_all(b"\n")).is_ok());
        let dropped = ws.len() != before;
        drop(ws);
        if dropped {
            self.rewatch();
        }
    }

    /// Программа на локальном сокете подписалась на снимки: соединение живёт, пока она работает.
    /// Новая строка `watch` в том же соединении меняет набор машин.
    fn serve_watch(&self, conn: &UnixStream, first: &Json) {
        let Ok(out) = conn.try_clone() else { return };
        let _ = out.set_write_timeout(Some(Duration::from_secs(1)));
        let _ = conn.set_read_timeout(None);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let names = |req: &Json| -> Vec<String> {
            req.items("machines").iter().filter_map(Json::as_str).take(MAX_WATCHED).map(str::to_string).collect()
        };
        self.watchers.lock().unwrap().push(Watcher { id, machines: names(first), out });
        self.rewatch();
        for line in BufReader::new(conn).lines() {
            let Ok(line) = line else { break };
            let Some(req) = Json::parse(line.trim()) else { continue };
            if req.text("cmd") == "watch" {
                if let Some(w) = self.watchers.lock().unwrap().iter_mut().find(|w| w.id == id) {
                    w.machines = names(&req);
                }
                self.rewatch();
            }
        }
        self.watchers.lock().unwrap().retain(|w| w.id != id);
        self.rewatch();
    }

    /// Раз в секунду, пока стоит флаг `fcw apc on`: подтверждение миру, что завод под управлением.
    fn apc_beat(&self) {
        if let Some(machines) = apc::read_flag(&self.apc_flag) {
            self.send(&apc::beat(&machines));
        }
    }

    fn send(&self, msg: &Json) {
        let mut line = msg.to_string();
        line.push('\n');
        let mut p = self.port.lock().unwrap();
        if let Some(f) = p.as_mut() {
            if let Err(e) = f.write_all(line.as_bytes()).and_then(|_| f.flush()) {
                eprintln!("fcw agent: write to host: {e}");
            }
        }
    }

    fn hello(&self) {
        self.send(&obj(vec![
            ("type", s("hello")),
            ("agent", s(VERSION)),
            ("os", s(&os_name())),
            ("hostname", s(&hostname())),
            ("sha256", s(&self.exe_sha256)),
            ("update", n(1)),
        ]));
    }

    /// Переслать `machine_req` в мир под своим `id` и дождаться `machine_resp`.
    fn machine_request(&self, req: &Json) -> Option<Json> {
        let Json::Obj(fields) = req else { return None };
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut out: Vec<(String, Json)> = vec![("type".into(), s("machine_req")), ("id".into(), n(id as f64))];
        out.extend(fields.iter().filter(|(k, _)| k != "type" && k != "id").cloned());
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.send(&Json::Obj(out));
        let resp = rx.recv_timeout(self.world_timeout).ok();
        self.pending.lock().unwrap().remove(&id);
        resp
    }

    fn update_failed(&self, error: &str) {
        eprintln!("fcw agent: update failed: {error}");
        if let Some(u) = self.update.lock().unwrap().take() {
            u.abort();
        }
        self.send(&obj(vec![("type", s("update_result")), ("ok", Json::Bool(false)), ("error", s(error))]));
    }

    fn update_ack(&self, seq: u64) {
        self.send(&obj(vec![("type", s("update_ack")), ("seq", n(seq as f64))]));
    }

    fn on_update(&self, kind: &str, m: &Json) {
        match kind {
            "update_begin" => {
                if let Some(old) = self.update.lock().unwrap().take() {
                    old.abort();
                }
                match update::Update::begin(&self.exe, m) {
                    Ok(u) => {
                        *self.update.lock().unwrap() = Some(u);
                        self.update_ack(0);
                    }
                    Err(e) => self.update_failed(&e),
                }
            }
            "update_chunk" => {
                let r = match self.update.lock().unwrap().as_mut() {
                    Some(u) => u.chunk(m),
                    None => Err("no update in progress".into()),
                };
                match r {
                    Ok(next) => self.update_ack(next),
                    Err(e) => self.update_failed(&e),
                }
            }
            _ => {
                let Some(u) = self.update.lock().unwrap().take() else {
                    return self.update_failed("no update in progress");
                };
                match u.finish() {
                    Ok(()) => {
                        eprintln!("fcw agent: updated, restarting");
                        self.send(&obj(vec![("type", s("update_result")), ("ok", Json::Bool(true))]));
                        (self.restart)();
                    }
                    Err(e) => self.update_failed(&e),
                }
            }
        }
    }

    fn levels_json(levels: &[u32; 6]) -> Json {
        Json::Obj(SIDES.iter().zip(levels).map(|(k, v)| (k.to_string(), n(*v))).collect())
    }

    fn on_host_line(&self, line: &str) {
        let Some(m) = Json::parse(line.trim()) else { return };
        self.st.lock().unwrap().last_host = Some(Instant::now());
        match m.get("type").and_then(Json::as_str).unwrap_or("") {
            "redstone_inputs" => {
                if let Some(l) = m.get("levels") {
                    let mut st = self.st.lock().unwrap();
                    for (i, side) in SIDES.iter().enumerate() {
                        if let Some(v) = l.get(side).and_then(Json::as_f64) {
                            st.inputs[i] = v.clamp(0.0, 15.0) as u32;
                        }
                    }
                    let _ = write_redstone_files(&st.inputs);
                }
            }
            "shutdown" => {
                eprintln!("fcw agent: shutdown requested by host");
                let ok = std::process::Command::new("systemctl").arg("poweroff").status().map(|s| s.success());
                if !matches!(ok, Ok(true)) {
                    let _ = std::process::Command::new("poweroff").status();
                }
            }
            "ping" => {
                self.hello();
                // хост мог забыть подписку (перезапуск мира при работающей ВМ)
                if !self.watchers.lock().unwrap().is_empty() {
                    self.rewatch();
                }
            }
            "machine_frame" => self.forward_frame(line.trim()),
            "machine_resp" => {
                if let Some(id) = m.get("id").and_then(Json::as_f64) {
                    if let Some(tx) = self.pending.lock().unwrap().remove(&(id as u64)) {
                        let _ = tx.send(m);
                    }
                }
            }
            kind @ ("update_begin" | "update_chunk" | "update_end") => self.on_update(kind, &m),
            "time" => {
                if let Some(ms) = m.get("unix_ms").and_then(Json::as_f64) {
                    set_clock(ms as i64);
                }
            }
            _ => {}
        }
    }

    fn on_local(&self, req: &Json) -> Json {
        let err = |e: &str| obj(vec![("ok", Json::Bool(false)), ("error", s(e))]);
        match req.get("cmd").and_then(Json::as_str).unwrap_or("") {
            "redstone_get" => {
                let st = self.st.lock().unwrap();
                obj(vec![
                    ("ok", Json::Bool(true)),
                    ("inputs", Self::levels_json(&st.inputs)),
                    ("outputs", Self::levels_json(&st.outputs)),
                ])
            }
            "redstone_set" => {
                let Some(side) = req.get("side").and_then(Json::as_str).and_then(norm_side) else {
                    return err("bad side");
                };
                let Some(level) = req.get("level").and_then(Json::as_f64).filter(|l| (0.0..=15.0).contains(l)) else {
                    return err("level must be 0-15");
                };
                let idx = SIDES.iter().position(|x| *x == side).unwrap();
                self.st.lock().unwrap().outputs[idx] = level as u32;
                self.send(&obj(vec![("type", s("redstone_set")), ("side", s(side)), ("level", n(level))]));
                obj(vec![("ok", Json::Bool(true))])
            }
            "info" => {
                let st = self.st.lock().unwrap();
                let connected = st.last_host.map(|t| t.elapsed() < Duration::from_secs(60)).unwrap_or(false);
                obj(vec![
                    ("ok", Json::Bool(true)),
                    ("agent", s(VERSION)),
                    ("sha256", s(&self.exe_sha256)),
                    ("os", s(&os_name())),
                    ("hostname", s(&hostname())),
                    ("port", Json::Bool(self.port.lock().unwrap().is_some())),
                    ("host_seen", Json::Bool(connected)),
                    ("uptime_s", n(st.started.elapsed().as_secs() as f64)),
                    ("apc", Json::Bool(self.apc_flag.exists())),
                    ("inputs", Self::levels_json(&st.inputs)),
                    ("outputs", Self::levels_json(&st.outputs)),
                ])
            }
            "machine" => match req.get("req").and_then(|r| self.machine_request(r)) {
                Some(resp) => obj(vec![("ok", Json::Bool(true)), ("resp", resp)]),
                None => err("timeout"),
            },
            "apc" => {
                let machines: Vec<String> =
                    req.items("machines").iter().filter_map(Json::as_str).map(str::to_string).collect();
                let r = if req.flag("on") {
                    apc::write_flag(&self.apc_flag, &machines)
                } else {
                    apc::clear_flag(&self.apc_flag)
                };
                match r {
                    Ok(()) => {
                        // первое подтверждение сразу, не дожидаясь секунды
                        self.apc_beat();
                        obj(vec![("ok", Json::Bool(true))])
                    }
                    Err(e) => err(&format!("{}: {e}", self.apc_flag.display())),
                }
            }
            "send" => {
                let Some(m) = req.get("json").and_then(Json::as_str).and_then(Json::parse) else {
                    return err("bad json");
                };
                self.send(&m);
                obj(vec![("ok", Json::Bool(true))])
            }
            _ => err("unknown command"),
        }
    }
}

/// Уровни входов ещё и файлами: /run/fcw/redstone/<side> (удобно для скриптов).
fn write_redstone_files(levels: &[u32; 6]) -> std::io::Result<()> {
    let dir = Path::new(SOCK_DIR).join("redstone");
    std::fs::create_dir_all(&dir)?;
    for (i, side) in SIDES.iter().enumerate() {
        std::fs::write(dir.join(side), format!("{}\n", levels[i]))?;
    }
    Ok(())
}

fn os_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| "Linux".into())
}

pub(crate) fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname").map(|s| s.trim().to_string()).unwrap_or_default()
}

fn set_clock(unix_ms: i64) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    if (now - unix_ms).abs() < 2000 {
        return;
    }
    let ts = libc::timespec { tv_sec: (unix_ms / 1000) as _, tv_nsec: ((unix_ms % 1000) * 1_000_000) as _ };
    // SAFETY: clock_settime с корректной структурой timespec.
    let r = unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) };
    if r != 0 {
        eprintln!("fcw agent: clock_settime: {}", std::io::Error::last_os_error());
    }
}

fn open_port(path: &str) -> File {
    let mut warned = false;
    loop {
        match std::fs::OpenOptions::new().read(true).write(true).open(path) {
            Ok(f) => return f,
            Err(e) => {
                if !warned {
                    eprintln!("fcw agent: waiting for {path}: {e}");
                    warned = true;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

/// Ссылки `fcw-top`, `fcw-litho`, … рядом с бинарником (`/usr/local/bin`): программы доступны и под
/// отдельными именами. Создаются при старте службы, если их ещё нет; чужие файлы не трогаются.
fn install_links(exe: &Path) {
    let (Some(dir), Some(target)) = (exe.parent(), exe.file_name()) else { return };
    for a in APPLETS {
        let link = dir.join(format!("fcw-{a}"));
        if link.symlink_metadata().is_ok() {
            continue;
        }
        if let Err(e) = std::os::unix::fs::symlink(target, &link) {
            eprintln!("fcw agent: link {}: {e}", link.display());
            return;
        }
    }
}

/// Заменить процесс свежим бинарником с теми же аргументами. Если не вышло — выйти:
/// systemd (`Restart=always`) поднимет службу заново, уже с новым файлом.
fn reexec(exe: &Path) {
    use std::os::unix::process::CommandExt;
    let e = std::process::Command::new(exe).args(std::env::args_os().skip(1)).exec();
    eprintln!("fcw agent: exec {}: {e}", exe.display());
    std::process::exit(1);
}

fn agent(port_path: &str, sock_path: &str) -> i32 {
    let port = open_port(port_path);
    let reader = match port.try_clone() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fcw agent: {e}");
            return 1;
        }
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/usr/local/bin/fcw"));
    let restart_exe = exe.clone();
    install_links(&exe);
    let a = Arc::new(Agent::new(port, exe, Box::new(move || reexec(&restart_exe))));
    let _ = write_redstone_files(&[0; 6]);
    eprintln!("fcw agent {VERSION}: connected to {port_path}");
    serve(a, reader, sock_path)
}

/// Служба на уже открытом порту: приветствие, пульс, локальный сокет, чтение строк хоста.
fn serve(a: Arc<Agent>, reader: File, sock_path: &str) -> ! {
    a.hello();

    // пульс
    {
        let a = a.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(10));
            a.send(&obj(vec![("type", s("heartbeat"))]));
            // подписка на снимки: напоминаем, пока есть кому их показывать
            if !a.watchers.lock().unwrap().is_empty() {
                a.rewatch();
            }
        });
    }
    // служба вычислений: подтверждение раз в секунду, пока стоит флаг
    {
        let a = a.clone();
        std::thread::spawn(move || loop {
            a.apc_beat();
            std::thread::sleep(Duration::from_secs(1));
        });
    }
    // локальный сокет для CLI
    {
        let a = a.clone();
        let sock_path = sock_path.to_string();
        std::thread::spawn(move || {
            if let Some(dir) = Path::new(&sock_path).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::remove_file(&sock_path);
            let l = match UnixListener::bind(&sock_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("fcw agent: bind {sock_path}: {e}");
                    return;
                }
            };
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o666));
            for c in l.incoming().flatten() {
                let a = a.clone();
                std::thread::spawn(move || {
                    let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut line = String::new();
                    let mut r = BufReader::new(&c);
                    if r.read_line(&mut line).is_err() {
                        return;
                    }
                    let resp = match Json::parse(line.trim()) {
                        Some(req) if req.text("cmd") == "watch" => return a.serve_watch(&c, &req),
                        Some(req) => a.on_local(&req),
                        None => obj(vec![("ok", Json::Bool(false)), ("error", s("bad request"))]),
                    };
                    let mut w = &c;
                    let _ = writeln!(w, "{resp}");
                });
            }
        });
    }
    // чтение строк от хоста
    let mut reader = reader;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                // хост не подключён: ждём
                std::thread::sleep(Duration::from_millis(500));
            }
            Ok(k) => {
                buf.extend_from_slice(&chunk[..k]);
                while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    if let Ok(l) = std::str::from_utf8(&line) {
                        a.on_host_line(l);
                    }
                }
                if buf.len() > 1 << 20 {
                    buf.clear();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                eprintln!("fcw agent: read from host: {e}");
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::OwnedFd;
    use std::sync::atomic::AtomicBool;

    /// Служба на поддельном порту: пара сокетов вместо virtio-serial. Возвращает
    /// сторону «хоста», путь локального сокета и флаг «агент перезапустился».
    fn start(dir: &Path, exe: PathBuf, world_timeout: Duration) -> (UnixStream, String, Arc<AtomicBool>) {
        let (guest, host) = UnixStream::pair().unwrap();
        let port = File::from(OwnedFd::from(guest));
        let reader = port.try_clone().unwrap();
        let restarted = Arc::new(AtomicBool::new(false));
        let flag = restarted.clone();
        let mut agent = Agent::new(port, exe, Box::new(move || flag.store(true, Ordering::SeqCst)));
        agent.world_timeout = world_timeout;
        agent.apc_flag = dir.join("apc");
        let sock = dir.join("agent.sock").to_string_lossy().into_owned();
        let sock2 = sock.clone();
        std::thread::spawn(move || serve(Arc::new(agent), reader, &sock2));
        for _ in 0..200 {
            if Path::new(&sock).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        host.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        (host, sock, restarted)
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fcw-agent-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Следующая строка от агента нужного типа (пульс и прочее пропускаем).
    fn next(host: &mut BufReader<UnixStream>, kind: &str) -> Json {
        loop {
            let mut line = String::new();
            assert!(host.read_line(&mut line).unwrap() > 0, "agent closed the port");
            let m = Json::parse(line.trim()).unwrap();
            if m.get("type").and_then(Json::as_str) == Some(kind) {
                return m;
            }
        }
    }

    #[test]
    fn machine_requests_are_routed_by_id() {
        let dir = tmp_dir("route");
        let (host, sock, _) = start(&dir, dir.join("no-such-binary"), Duration::from_secs(5));
        let mut to_guest = host.try_clone().unwrap();
        let mut host = BufReader::new(host);
        let hello = next(&mut host, "hello");
        assert_eq!(hello.get("agent").and_then(Json::as_str), Some(VERSION));
        assert_eq!(hello.get("update").and_then(Json::as_f64), Some(1.0));

        // два клиента одновременно; мир отвечает в обратном порядке
        let clients: Vec<_> = ["eaf-1", "euv-1"]
            .into_iter()
            .map(|name| {
                let sock = sock.clone();
                std::thread::spawn(move || {
                    machine_call(&sock, &machine::request("exec", Some(name), Some("status"), false, "ru_ru"))
                })
            })
            .collect();
        let a = next(&mut host, "machine_req");
        let b = next(&mut host, "machine_req");
        assert_ne!(a.get("id"), b.get("id"));
        for req in [&b, &a] {
            assert_eq!(req.get("op").and_then(Json::as_str), Some("exec"));
            assert_eq!(req.get("lang").and_then(Json::as_str), Some("ru_ru"));
            let resp = obj(vec![
                ("type", s("machine_resp")),
                ("id", req.get("id").unwrap().clone()),
                ("exit", n(0)),
                ("text", s(&format!("ответ для {}", req.get("machine").and_then(Json::as_str).unwrap()))),
                ("json", Json::Null),
            ]);
            writeln!(to_guest, "{resp}").unwrap();
        }
        let texts: Vec<String> = clients.into_iter().map(|c| c.join().unwrap().unwrap().text).collect();
        assert_eq!(texts, ["ответ для eaf-1", "ответ для euv-1"]);
    }

    #[test]
    fn programs_answer_to_their_own_names() {
        assert_eq!(applet("/usr/local/bin/fcw-litho"), Some("litho"));
        assert_eq!(applet("fcw-top"), Some("top"));
        assert_eq!(applet("fcw-apc"), Some("apc"));
        assert_eq!(applet("/usr/local/bin/fcw"), None);
        assert_eq!(applet("fcw-agent"), None);
    }

    #[test]
    fn links_are_made_once_and_never_overwrite() {
        let dir = tmp_dir("links");
        let exe = dir.join("fcw");
        std::fs::write(&exe, b"binary").unwrap();
        std::fs::write(dir.join("fcw-top"), b"someone else's file").unwrap();
        install_links(&exe);
        install_links(&exe);
        for a in APPLETS {
            let link = dir.join(format!("fcw-{a}"));
            if a == "top" {
                assert_eq!(std::fs::read(&link).unwrap(), b"someone else's file");
            } else {
                assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from("fcw"));
                assert_eq!(std::fs::read(&link).unwrap(), b"binary");
            }
        }
    }

    /// Имена машин из сообщения `machine_watch`.
    fn watched(m: &Json) -> Vec<String> {
        m.items("machines").iter().filter_map(Json::as_str).map(str::to_string).collect()
    }

    #[test]
    fn frames_reach_every_program_that_watches() {
        let dir = tmp_dir("watch");
        let (host, sock, _) = start(&dir, dir.join("no-such-binary"), Duration::from_secs(5));
        let mut to_guest = host.try_clone().unwrap();
        let mut host = BufReader::new(host);
        next(&mut host, "hello");

        // первая программа смотрит одну машину
        let mut one = UnixStream::connect(&sock).unwrap();
        writeln!(one, r#"{{"cmd":"watch","machines":["euv-1"],"hz":4}}"#).unwrap();
        let w = next(&mut host, "machine_watch");
        assert_eq!(watched(&w), ["euv-1"]);
        assert_eq!(w.num("hz"), 4.0);
        // вторая — две: у мира подписка одна, в ней объединение
        let mut two = UnixStream::connect(&sock).unwrap();
        writeln!(two, r#"{{"cmd":"watch","machines":["cz-1","euv-1"],"hz":4}}"#).unwrap();
        assert_eq!(watched(&next(&mut host, "machine_watch")), ["euv-1", "cz-1"]);

        let frame = r#"{"type":"machine_frame","id":1,"seq":7,"machines":[{"name":"euv-1"}]}"#;
        writeln!(to_guest, "{frame}").unwrap();
        for st in [&one, &two] {
            st.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut line = String::new();
            BufReader::new(st).read_line(&mut line).unwrap();
            assert_eq!(line.trim(), frame);
        }
        // программа меняет набор машин в том же соединении
        writeln!(one, r#"{{"cmd":"watch","machines":["saw-1"]}}"#).unwrap();
        assert_eq!(watched(&next(&mut host, "machine_watch")), ["saw-1", "cz-1", "euv-1"]);
        // программы закрылись: подписка снимается
        drop(one);
        assert_eq!(watched(&next(&mut host, "machine_watch")), ["cz-1", "euv-1"]);
        drop(two);
        next(&mut host, "machine_unwatch");
        // обычные команды сокета работают как раньше
        let info = ask(&sock, &obj(vec![("cmd", s("info"))]), Duration::from_secs(5)).unwrap();
        assert_eq!(info.get("apc").and_then(Json::as_bool), Some(false));
    }

    #[test]
    fn apc_flag_makes_the_service_beat() {
        let dir = tmp_dir("apc");
        let (host, sock, _) = start(&dir, dir.join("no-such-binary"), Duration::from_secs(5));
        let mut host = BufReader::new(host);
        next(&mut host, "hello");
        let set = |on: bool, machines: Vec<Json>| {
            let req = obj(vec![("cmd", s("apc")), ("on", Json::Bool(on)), ("machines", Json::Arr(machines))]);
            let r = ask(&sock, &req, Duration::from_secs(5)).unwrap();
            assert_eq!(r.get("ok").and_then(Json::as_bool), Some(true), "{r}");
        };
        set(true, vec![]);
        assert!(dir.join("apc").exists());
        // подтверждения идут раз в секунду, пока стоит флаг
        let started = Instant::now();
        for _ in 0..3 {
            assert_eq!(next(&mut host, "apc_beat").get("machines"), None);
        }
        assert!(started.elapsed() >= Duration::from_millis(1500), "{:?}", started.elapsed());
        assert!(started.elapsed() < Duration::from_secs(4), "{:?}", started.elapsed());
        set(true, vec![s("euv-1")]);
        loop {
            let b = next(&mut host, "apc_beat");
            if b.get("machines").is_some() {
                assert_eq!(watched(&b), ["euv-1"]);
                break;
            }
        }
        set(false, vec![]);
        assert!(!dir.join("apc").exists());
        // флаг снят — подтверждений больше нет (одно могло уйти до снятия)
        host.get_ref().set_read_timeout(Some(Duration::from_millis(2500))).unwrap();
        let mut beats = 0;
        let mut line = String::new();
        while host.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
            if line.contains("apc_beat") {
                beats += 1;
            }
            line.clear();
        }
        assert!(beats <= 1, "{beats} beats after off");
    }

    #[test]
    fn silent_world_is_no_answer() {
        let dir = tmp_dir("silent");
        let (host, sock, _) = start(&dir, dir.join("no-such-binary"), Duration::from_millis(200));
        let started = Instant::now();
        let r = machine_call(&sock, &machine::request("list", None, None, false, "en_us"));
        assert_eq!(r, Err(machine::NoAnswer));
        assert!(started.elapsed() < Duration::from_secs(3));
        // службы нет вовсе
        assert_eq!(
            machine_call("/nonexistent/agent.sock", &machine::request("list", None, None, false, "en_us")),
            Err(machine::NoAnswer)
        );
        drop(host);
    }

    fn b64(data: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in data.chunks(3) {
            let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            out.push(A[(v >> 18) as usize & 63] as char);
            out.push(A[(v >> 12) as usize & 63] as char);
            out.push(if c.len() > 1 { A[(v >> 6) as usize & 63] as char } else { '=' });
            out.push(if c.len() > 2 { A[v as usize & 63] as char } else { '=' });
        }
        out
    }

    fn sha(data: &[u8]) -> String {
        let mut h = hash::Sha256::new();
        h.update(data);
        h.finish_hex()
    }

    /// Передать «бинарник» по протоколу обновления; ответ агента на `update_end`.
    fn push_update(host: &mut BufReader<UnixStream>, data: &[u8], sha256: &str) -> Json {
        let mut to_guest = host.get_ref().try_clone().unwrap();
        let begin = obj(vec![("type", s("update_begin")), ("size", n(data.len() as f64)), ("sha256", s(sha256))]);
        writeln!(to_guest, "{begin}").unwrap();
        assert_eq!(next(host, "update_ack").get("seq").and_then(Json::as_f64), Some(0.0));
        for (i, c) in data.chunks(1000).enumerate() {
            let chunk = obj(vec![("type", s("update_chunk")), ("seq", n(i as f64)), ("data", s(&b64(c)))]);
            writeln!(to_guest, "{chunk}").unwrap();
            assert_eq!(next(host, "update_ack").get("seq").and_then(Json::as_f64), Some(i as f64 + 1.0));
        }
        writeln!(to_guest, r#"{{"type":"update_end"}}"#).unwrap();
        next(host, "update_result")
    }

    #[test]
    fn self_update_replaces_the_binary() {
        let dir = tmp_dir("update");
        let exe = dir.join("fcw");
        std::fs::write(&exe, b"old agent").unwrap();
        let (host, _, restarted) = start(&dir, exe.clone(), Duration::from_secs(5));
        let mut host = BufReader::new(host);
        assert_eq!(next(&mut host, "hello").get("sha256").and_then(Json::as_str), Some(sha(b"old agent").as_str()));

        // «новый агент» — скрипт: запускается и отвечает на --version; несколько кусков
        let mut new = b"#!/bin/sh\nexit 0\n".to_vec();
        new.extend(std::iter::repeat_n(b'#', 4321));

        // неверный хэш: файл на месте, агент не перезапущен
        let r = push_update(&mut host, &new, &sha(b"something else"));
        assert_eq!(r.get("ok").and_then(Json::as_bool), Some(false));
        assert_eq!(std::fs::read(&exe).unwrap(), b"old agent");
        assert!(!update::tmp_path(&exe).exists());
        assert!(!restarted.load(Ordering::SeqCst));

        // файл, который не запускается, тоже не ставится
        let broken = b"#!/bin/sh\nexit 3\n";
        let r = push_update(&mut host, broken, &sha(broken));
        assert_eq!(r.get("ok").and_then(Json::as_bool), Some(false));
        assert_eq!(std::fs::read(&exe).unwrap(), b"old agent");

        let r = push_update(&mut host, &new, &sha(&new));
        assert_eq!(r.get("ok").and_then(Json::as_bool), Some(true), "{r}");
        assert_eq!(std::fs::read(&exe).unwrap(), new);
        assert!(!update::tmp_path(&exe).exists());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
        for _ in 0..100 {
            if restarted.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(restarted.load(Ordering::SeqCst));
    }

    #[test]
    fn update_chunks_must_come_in_order() {
        let dir = tmp_dir("order");
        let exe = dir.join("fcw");
        std::fs::write(&exe, b"old agent").unwrap();
        let (host, _, _) = start(&dir, exe.clone(), Duration::from_secs(5));
        let mut to_guest = host.try_clone().unwrap();
        let mut host = BufReader::new(host);
        writeln!(to_guest, r#"{{"type":"update_chunk","seq":0,"data":"AAAA"}}"#).unwrap();
        assert_eq!(next(&mut host, "update_result").get("ok").and_then(Json::as_bool), Some(false));
        let begin = obj(vec![("type", s("update_begin")), ("size", n(6)), ("sha256", s(&sha(b"abcdef")))]);
        writeln!(to_guest, "{begin}").unwrap();
        next(&mut host, "update_ack");
        writeln!(to_guest, r#"{{"type":"update_chunk","seq":1,"data":"AAAA"}}"#).unwrap();
        assert_eq!(next(&mut host, "update_result").get("ok").and_then(Json::as_bool), Some(false));
        assert!(!update::tmp_path(&exe).exists());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old agent");
    }
}
