//! `fcw` — агент гостя FCW (CONTRACT.md §7).
//!
//! * `fcw agent` — служба: порт virtio-serial `fcw.agent` (строки JSON с
//!   хостом) и локальный сокет `/run/fcw/agent.sock` для команд;
//! * `fcw redstone get [side]`, `fcw redstone set <side> <0-15>`;
//! * `fcw info`, `fcw send '<json>'` (произвольное сообщение хосту).

mod json;

use json::{n, obj, s, Json};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
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
         fcw info                                   agent status\n  \
         fcw send '<json>'                          send a JSON message to the world\n\n\
         sides: front back left right top bottom"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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

fn cli(req: Json, show: impl FnOnce(&Json)) -> i32 {
    let mut st = match UnixStream::connect(SOCK) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fcw: cannot connect to agent ({SOCK}): {e}. Is `fcw agent` running?");
            return 1;
        }
    };
    let _ = st.set_read_timeout(Some(Duration::from_secs(5)));
    if writeln!(st, "{req}").is_err() {
        eprintln!("fcw: agent connection failed");
        return 1;
    }
    let mut line = String::new();
    if BufReader::new(&st).read_line(&mut line).is_err() || line.is_empty() {
        eprintln!("fcw: no answer from agent");
        return 1;
    }
    let Some(resp) = Json::parse(line.trim()) else {
        eprintln!("fcw: bad answer from agent");
        return 1;
    };
    if resp.get("ok").and_then(Json::as_bool) != Some(true) {
        eprintln!("fcw: {}", resp.get("error").and_then(Json::as_str).unwrap_or("error"));
        return 1;
    }
    show(&resp);
    0
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
}

impl Agent {
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
        ]));
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
            "ping" => self.hello(),
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
                    ("os", s(&os_name())),
                    ("hostname", s(&hostname())),
                    ("port", Json::Bool(self.port.lock().unwrap().is_some())),
                    ("host_seen", Json::Bool(connected)),
                    ("uptime_s", n(st.started.elapsed().as_secs() as f64)),
                    ("inputs", Self::levels_json(&st.inputs)),
                    ("outputs", Self::levels_json(&st.outputs)),
                ])
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

fn hostname() -> String {
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

fn agent(port_path: &str, sock_path: &str) -> i32 {
    let port = open_port(port_path);
    let reader = match port.try_clone() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fcw agent: {e}");
            return 1;
        }
    };
    let a = Arc::new(Agent {
        port: Mutex::new(Some(port)),
        st: Mutex::new(State { inputs: [0; 6], outputs: [0; 6], last_host: None, started: Instant::now() }),
    });
    let _ = write_redstone_files(&[0; 6]);
    a.hello();
    eprintln!("fcw agent {VERSION}: connected to {port_path}");

    // пульс
    {
        let a = a.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(10));
            a.send(&obj(vec![("type", s("heartbeat"))]));
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
