//! `fcw machine …` — управление производственными машинами из гостя
//! (docs/MACHINE_CONTROL.md, docs/design/MACHINE_UI.md).
//!
//! CLI собирает запрос `machine_req`, служба агента пересылает его в мир и
//! возвращает `machine_resp`. `wait` и `log --follow` — опрос раз в секунду.

use fcw_tui::json::{obj, s, Json};
use std::io::Write;
use std::time::Duration;

/// Сколько ждать ответа мира.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// Период опроса для `wait` и `log --follow`.
pub const POLL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// `fcw machine list [--json]`
    List { json: bool },
    /// `fcw machine <имя> <команда…> [--json]`
    Exec { machine: String, line: String, json: bool },
    /// `fcw machine <имя> wait <состояние> [--timeout N]`
    Wait { machine: String, state: String, timeout: Option<u64> },
    /// `fcw machine <имя> log [n] --follow`
    Follow { machine: String, line: String },
}

pub const USAGE: &str = "usage:\n  \
    fcw machine list [--json]\n  \
    fcw machine <name> <command...> [--json]      status, start, stop [now], recipe [<name>|auto], log [n], help\n  \
    fcw machine <name> log [n] --follow\n  \
    fcw machine <name> wait <off|idle|run|fault> [--timeout <seconds>]";

/// Разбор аргументов после слова `machine`.
pub fn parse_args(args: &[&str]) -> Result<Cmd, String> {
    let mut json = false;
    let mut follow = false;
    let mut timeout = None;
    let mut words: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--json" => json = true,
            "--follow" | "-f" => follow = true,
            "--timeout" => {
                let v = it.next().ok_or("--timeout needs a number of seconds")?;
                timeout = Some(v.parse::<u64>().map_err(|_| format!("bad --timeout value: {v}"))?);
            }
            x if x.starts_with("--timeout=") => {
                let v = &x["--timeout=".len()..];
                timeout = Some(v.parse::<u64>().map_err(|_| format!("bad --timeout value: {v}"))?);
            }
            x => words.push(x),
        }
    }
    let Some((&first, rest)) = words.split_first() else {
        return Err("no machine name".into());
    };
    if first == "list" && rest.is_empty() {
        if follow || timeout.is_some() {
            return Err("list takes only --json".into());
        }
        return Ok(Cmd::List { json });
    }
    let machine = first.to_string();
    if machine.starts_with('-') {
        return Err(format!("unknown option: {machine}"));
    }
    // без команды — status: так удобнее смотреть
    let rest: Vec<&str> = if rest.is_empty() { vec!["status"] } else { rest.to_vec() };
    if rest[0] == "wait" {
        if json || follow {
            return Err("wait takes only --timeout".into());
        }
        return match rest.as_slice() {
            [_, state] => Ok(Cmd::Wait { machine, state: state.to_ascii_lowercase(), timeout }),
            _ => Err("wait needs exactly one state: off, idle, run or fault".into()),
        };
    }
    if timeout.is_some() {
        return Err("--timeout is only for wait".into());
    }
    let line = rest.join(" ");
    if follow {
        if rest[0] != "log" {
            return Err("--follow is only for log".into());
        }
        if json {
            return Err("--follow and --json cannot be combined".into());
        }
        return Ok(Cmd::Follow { machine, line });
    }
    Ok(Cmd::Exec { machine, line, json })
}

/// Код языка Minecraft из окружения гостя: `ru_RU.UTF-8` → `ru_ru`; пусто, `C`, `POSIX` → `en_us`.
pub fn lang_from_env(get: impl Fn(&str) -> Option<String>) -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Some(v) = get(var).filter(|v| !v.is_empty()) else { continue };
        return lang_code(&v);
    }
    "en_us".into()
}

fn lang_code(locale: &str) -> String {
    let base = locale.split(['.', '@']).next().unwrap_or("");
    let mut parts = base.split(['_', '-']);
    let lang = parts.next().unwrap_or("").to_ascii_lowercase();
    let region = parts.next().unwrap_or("").to_ascii_lowercase();
    let ok = |p: &str| (2..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_lowercase());
    if !ok(&lang) {
        return "en_us".into(); // C, POSIX и прочее
    }
    if ok(&region) {
        return format!("{lang}_{region}");
    }
    match lang.as_str() {
        "ru" => "ru_ru".into(),
        "en" => "en_us".into(),
        other => format!("{other}_{other}"),
    }
}

/// Запрос без `id`: его проставляет служба агента.
pub fn request(op: &str, machine: Option<&str>, line: Option<&str>, json: bool, lang: &str) -> Json {
    let mut v = vec![("type", s("machine_req")), ("op", s(op))];
    if let Some(m) = machine {
        v.push(("machine", s(m)));
    }
    if let Some(l) = line {
        v.push(("line", s(l)));
    }
    v.push(("json", Json::Bool(json)));
    v.push(("lang", s(lang)));
    obj(v)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resp {
    pub exit: i32,
    pub text: String,
    pub json: Option<Json>,
}

/// Разбор `machine_resp`.
pub fn parse_resp(m: &Json) -> Option<Resp> {
    if m.get("type").and_then(Json::as_str) != Some("machine_resp") {
        return None;
    }
    let exit = m.get("exit").and_then(Json::as_f64).unwrap_or(2.0) as i32;
    let text = m.get("text").and_then(Json::as_str).unwrap_or("").to_string();
    let json = m.get("json").filter(|j| **j != Json::Null).cloned();
    Some(Resp { exit, text, json })
}

/// Мир не ответил за [`TIMEOUT`] или служба агента недоступна.
#[derive(Debug, Clone, PartialEq)]
pub struct NoAnswer;

fn ru(lang: &str) -> bool {
    lang.starts_with("ru")
}

pub fn no_answer_text(lang: &str) -> &'static str {
    if ru(lang) {
        "агент не отвечает"
    } else {
        "agent is not responding"
    }
}

/// Наибольший `seq` где угодно в ответе `log --json`.
fn max_seq(j: &Json) -> Option<u64> {
    match j {
        Json::Arr(a) => a.iter().filter_map(max_seq).max(),
        Json::Obj(o) => o
            .iter()
            .filter_map(|(k, v)| match (k.as_str(), v) {
                ("seq", Json::Num(n)) if *n >= 0.0 => Some(*n as u64),
                (_, v) => max_seq(v),
            })
            .max(),
        _ => None,
    }
}

fn state_matches(wanted: &str, got: &str) -> bool {
    let norm = |x: &str| match x {
        "error" => "fault".to_string(),
        other => other.to_ascii_lowercase(),
    };
    norm(wanted) == norm(got)
}

/// Куда пишет CLI и чем ждёт: вынесено, чтобы проверять тестами.
pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    /// Запрос в мир и ответ на него.
    pub call: &'a mut dyn FnMut(&Json) -> Result<Resp, NoAnswer>,
    /// Пауза между опросами; `false` — прервать (только в тестах).
    pub sleep: &'a mut dyn FnMut(Duration) -> bool,
}

fn emit(io: &mut Io, r: &Resp, json: bool) {
    if json {
        if let Some(j) = &r.json {
            let _ = writeln!(io.out, "{j}");
            return;
        }
    }
    if r.text.is_empty() {
        return;
    }
    let text = r.text.trim_end_matches('\n');
    if r.exit == 0 {
        let _ = writeln!(io.out, "{text}");
    } else {
        let _ = writeln!(io.err, "{text}");
    }
}

fn fail_no_answer(io: &mut Io, lang: &str) -> i32 {
    let _ = writeln!(io.err, "fcw: {}", no_answer_text(lang));
    2
}

/// Выполнить команду; возвращает код выхода процесса.
pub fn run(cmd: &Cmd, lang: &str, io: &mut Io) -> i32 {
    match cmd {
        Cmd::List { json } => match (io.call)(&request("list", None, None, *json, lang)) {
            Ok(r) => {
                emit(io, &r, *json);
                r.exit
            }
            Err(NoAnswer) => fail_no_answer(io, lang),
        },
        Cmd::Exec { machine, line, json } => {
            match (io.call)(&request("exec", Some(machine), Some(line), *json, lang)) {
                Ok(r) => {
                    emit(io, &r, *json);
                    r.exit
                }
                Err(NoAnswer) => fail_no_answer(io, lang),
            }
        }
        Cmd::Wait { machine, state, timeout } => {
            let mut waited = 0u64;
            loop {
                let r = match (io.call)(&request("exec", Some(machine), Some("status"), true, lang)) {
                    Ok(r) => r,
                    Err(NoAnswer) => return fail_no_answer(io, lang),
                };
                if r.exit == 2 {
                    emit(io, &r, false);
                    return 2;
                }
                let got = r.json.as_ref().and_then(|j| j.get("state")).and_then(Json::as_str).unwrap_or("");
                if state_matches(state, got) {
                    return 0;
                }
                if timeout.is_some_and(|t| waited >= t) {
                    let _ = if ru(lang) {
                        writeln!(io.err, "fcw: {machine}: состояние {state} не наступило за {waited} с")
                    } else {
                        writeln!(io.err, "fcw: {machine}: state {state} not reached in {waited} s")
                    };
                    return 1;
                }
                if !(io.sleep)(POLL) {
                    return 1;
                }
                waited += POLL.as_secs();
            }
        }
        Cmd::Follow { machine, line } => {
            let first = match (io.call)(&request("exec", Some(machine), Some(line), true, lang)) {
                Ok(r) => r,
                Err(NoAnswer) => return fail_no_answer(io, lang),
            };
            emit(io, &first, false);
            if first.exit != 0 {
                return first.exit;
            }
            let mut last = first.json.as_ref().and_then(max_seq).unwrap_or(0);
            loop {
                let _ = io.out.flush();
                if !(io.sleep)(POLL) {
                    return 0;
                }
                let since = format!("log --since {last}");
                let r = match (io.call)(&request("exec", Some(machine), Some(&since), true, lang)) {
                    Ok(r) => r,
                    Err(NoAnswer) => return fail_no_answer(io, lang),
                };
                emit(io, &r, false);
                if r.exit != 0 {
                    return r.exit;
                }
                if let Some(seq) = r.json.as_ref().and_then(max_seq) {
                    last = last.max(seq);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fcw_tui::json::n;

    #[test]
    fn args() {
        assert_eq!(parse_args(&["list"]), Ok(Cmd::List { json: false }));
        assert_eq!(parse_args(&["list", "--json"]), Ok(Cmd::List { json: true }));
        assert_eq!(parse_args(&["--json", "list"]), Ok(Cmd::List { json: true }));
        assert_eq!(
            parse_args(&["euv-1", "status"]),
            Ok(Cmd::Exec { machine: "euv-1".into(), line: "status".into(), json: false })
        );
        assert_eq!(
            parse_args(&["euv-1"]),
            Ok(Cmd::Exec { machine: "euv-1".into(), line: "status".into(), json: false })
        );
        assert_eq!(
            parse_args(&["euv-1", "recipe", "wafer_memory", "--json"]),
            Ok(Cmd::Exec { machine: "euv-1".into(), line: "recipe wafer_memory".into(), json: true })
        );
        assert_eq!(
            parse_args(&["eaf-1", "stop", "now"]),
            Ok(Cmd::Exec { machine: "eaf-1".into(), line: "stop now".into(), json: false })
        );
        // машину с именем list всё равно можно спросить
        assert_eq!(
            parse_args(&["list", "status"]),
            Ok(Cmd::Exec { machine: "list".into(), line: "status".into(), json: false })
        );
        assert_eq!(
            parse_args(&["euv-1", "wait", "IDLE", "--timeout", "600"]),
            Ok(Cmd::Wait { machine: "euv-1".into(), state: "idle".into(), timeout: Some(600) })
        );
        assert_eq!(
            parse_args(&["euv-1", "wait", "run", "--timeout=5"]),
            Ok(Cmd::Wait { machine: "euv-1".into(), state: "run".into(), timeout: Some(5) })
        );
        assert_eq!(
            parse_args(&["euv-1", "wait", "fault"]),
            Ok(Cmd::Wait { machine: "euv-1".into(), state: "fault".into(), timeout: None })
        );
        assert_eq!(
            parse_args(&["euv-1", "log", "5", "--follow"]),
            Ok(Cmd::Follow { machine: "euv-1".into(), line: "log 5".into() })
        );
        assert_eq!(
            parse_args(&["euv-1", "log", "-f"]),
            Ok(Cmd::Follow { machine: "euv-1".into(), line: "log".into() })
        );
        for bad in [
            &[][..],
            &["--json"],
            &["euv-1", "wait"],
            &["euv-1", "wait", "idle", "run"],
            &["euv-1", "wait", "idle", "--timeout"],
            &["euv-1", "wait", "idle", "--timeout", "soon"],
            &["euv-1", "wait", "idle", "--json"],
            &["euv-1", "status", "--follow"],
            &["euv-1", "status", "--timeout", "3"],
            &["euv-1", "log", "--follow", "--json"],
            &["list", "--follow"],
            &["--bogus", "status"],
        ] {
            assert!(parse_args(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn lang() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(lang_from_env(env(&[("LANG", "ru_RU.UTF-8")])), "ru_ru");
        assert_eq!(lang_from_env(env(&[("LANG", "ru_RU.UTF-8"), ("LC_ALL", "en_GB.UTF-8")])), "en_gb");
        assert_eq!(lang_from_env(env(&[("LANG", "ru_RU.UTF-8"), ("LC_MESSAGES", "de_DE@euro")])), "de_de");
        assert_eq!(lang_from_env(env(&[("LC_ALL", ""), ("LANG", "uk_UA.UTF-8")])), "uk_ua");
        assert_eq!(lang_from_env(env(&[("LANG", "C")])), "en_us");
        assert_eq!(lang_from_env(env(&[("LANG", "C.UTF-8")])), "en_us");
        assert_eq!(lang_from_env(env(&[("LANG", "POSIX")])), "en_us");
        assert_eq!(lang_from_env(env(&[("LANG", "ru")])), "ru_ru");
        assert_eq!(lang_from_env(env(&[("LANG", "en")])), "en_us");
        assert_eq!(lang_from_env(env(&[])), "en_us");
    }

    #[test]
    fn protocol() {
        let r = request("exec", Some("euv-1"), Some("status"), false, "ru_ru");
        assert_eq!(
            r.to_string(),
            r#"{"type":"machine_req","op":"exec","machine":"euv-1","line":"status","json":false,"lang":"ru_ru"}"#
        );
        assert_eq!(
            request("list", None, None, true, "en_us").to_string(),
            r#"{"type":"machine_req","op":"list","json":true,"lang":"en_us"}"#
        );
        let resp =
            Json::parse(r#"{"type":"machine_resp","id":18,"exit":0,"text":"состояние  работа","json":null}"#).unwrap();
        assert_eq!(parse_resp(&resp), Some(Resp { exit: 0, text: "состояние  работа".into(), json: None }));
        let resp = Json::parse(r#"{"type":"machine_resp","id":1,"exit":1,"text":"x","json":{"state":"run"}}"#).unwrap();
        assert_eq!(parse_resp(&resp).unwrap().json.unwrap().get("state").and_then(Json::as_str), Some("run"));
        assert_eq!(parse_resp(&Json::parse(r#"{"type":"time"}"#).unwrap()), None);
    }

    /// Прогон команды против заготовленных ответов: (stdout, stderr, код, запросы).
    fn drive(cmd: &Cmd, lang: &str, answers: Vec<Result<Resp, NoAnswer>>) -> (String, String, i32, Vec<String>) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut reqs = Vec::new();
        let mut answers = answers.into_iter();
        let mut left = 50;
        let code = run(
            cmd,
            lang,
            &mut Io {
                out: &mut out,
                err: &mut err,
                call: &mut |r| {
                    reqs.push(r.to_string());
                    answers.next().unwrap_or(Err(NoAnswer))
                },
                sleep: &mut |_| {
                    left -= 1;
                    left > 0
                },
            },
        );
        (String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap(), code, reqs)
    }

    fn ok(text: &str, json: Option<Json>) -> Result<Resp, NoAnswer> {
        Ok(Resp { exit: 0, text: text.into(), json })
    }

    fn state(st: &str) -> Option<Json> {
        Some(obj(vec![("state", s(st))]))
    }

    #[test]
    fn exec_output_and_exit() {
        let cmd = parse_args(&["eaf-1", "status"]).unwrap();
        let (out, err, code, reqs) = drive(&cmd, "ru_ru", vec![ok("состояние  ожидание\n", None)]);
        assert_eq!((out.as_str(), err.as_str(), code), ("состояние  ожидание\n", "", 0));
        assert!(reqs[0].contains(r#""machine":"eaf-1""#) && reqs[0].contains(r#""line":"status""#));

        // отказ машины: текст в stderr, код 1
        let refuse = Ok(Resp { exit: 1, text: "отказ: нет маски".into(), json: None });
        let (out, err, code, _) = drive(&parse_args(&["euv-1", "start"]).unwrap(), "ru_ru", vec![refuse]);
        assert_eq!((out.as_str(), err.as_str(), code), ("", "отказ: нет маски\n", 1));

        // --json печатает json, а не текст
        let cmd = parse_args(&["list", "--json"]).unwrap();
        let (out, _, code, _) = drive(&cmd, "en_us", vec![ok("eaf-1  arc furnace  idle", state("idle"))]);
        assert_eq!((out.as_str(), code), ("{\"state\":\"idle\"}\n", 0));

        // мир молчит
        let (out, err, code, _) = drive(&parse_args(&["list"]).unwrap(), "ru_ru", vec![]);
        assert_eq!((out.as_str(), err.as_str(), code), ("", "fcw: агент не отвечает\n", 2));
        let (_, err, _, _) = drive(&parse_args(&["list"]).unwrap(), "en_us", vec![]);
        assert_eq!(err, "fcw: agent is not responding\n");
    }

    #[test]
    fn wait_polls_until_state() {
        let cmd = parse_args(&["euv-1", "wait", "idle", "--timeout", "10"]).unwrap();
        let (out, err, code, reqs) =
            drive(&cmd, "ru_ru", vec![ok("", state("run")), ok("", state("run")), ok("", state("idle"))]);
        assert_eq!((out.as_str(), err.as_str(), code, reqs.len()), ("", "", 0, 3));
        assert!(reqs.iter().all(|r| r.contains(r#""line":"status""#) && r.contains(r#""json":true"#)));

        // таймаут: 0, 1, 2 с — три опроса
        let cmd = parse_args(&["euv-1", "wait", "idle", "--timeout", "2"]).unwrap();
        let (_, err, code, reqs) = drive(&cmd, "en_us", (0..9).map(|_| ok("", state("run"))).collect());
        assert_eq!((code, reqs.len()), (1, 3));
        assert_eq!(err, "fcw: euv-1: state idle not reached in 2 s\n");

        // «error» в ответе считается fault
        let cmd = parse_args(&["euv-1", "wait", "fault"]).unwrap();
        assert_eq!(drive(&cmd, "en_us", vec![ok("", state("error"))]).2, 0);

        // нет такой машины
        let cmd = parse_args(&["euv-9", "wait", "idle"]).unwrap();
        let none = Ok(Resp {
            exit: 2, text: "нет машины euv-9 в сети этого компьютера".into(), json: None
        });
        let (_, err, code, _) = drive(&cmd, "ru_ru", vec![none]);
        assert_eq!((err.as_str(), code), ("нет машины euv-9 в сети этого компьютера\n", 2));
    }

    #[test]
    fn follow_asks_only_new_records() {
        let entries = |seqs: &[u32]| {
            Some(obj(vec![("log", Json::Arr(seqs.iter().map(|q| obj(vec![("seq", n(*q))])).collect()))]))
        };
        let cmd = parse_args(&["euv-1", "log", "2", "--follow"]).unwrap();
        let (out, _, code, reqs) = drive(
            &cmd,
            "ru_ru",
            vec![
                ok("14:18:41  ожидание\n14:19:52  пуск", entries(&[6, 7])),
                ok("", entries(&[])),
                ok("14:20:31  останов", entries(&[8])),
                Ok(Resp { exit: 2, text: "нет машины".into(), json: None }),
            ],
        );
        assert_eq!(out, "14:18:41  ожидание\n14:19:52  пуск\n14:20:31  останов\n");
        assert_eq!(code, 2);
        assert!(reqs[0].contains(r#""line":"log 2""#));
        assert!(reqs[1].contains(r#""line":"log --since 7""#));
        assert!(reqs[2].contains(r#""line":"log --since 7""#));
        assert!(reqs[3].contains(r#""line":"log --since 8""#));
    }
}
