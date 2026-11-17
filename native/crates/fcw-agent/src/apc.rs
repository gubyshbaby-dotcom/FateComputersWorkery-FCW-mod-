//! `fcw apc on|off|status` — служба вычислений для машин (docs/design/DEFECTS.md, раздел 6).
//!
//! Отдельного процесса нет: «служба включена» — это файл-флаг `/var/lib/fcw/apc`. Пока он есть,
//! служба агента (она уже работает под systemd) раз в секунду шлёт миру `apc_beat`, и машины
//! получают полную вычислительную мощность процессора компьютера; без подтверждений — 75 %.
//! В файле можно перечислить машины, которым отдавать CU (по имени на строку); пусто — всем поровну.

use fcw_tui::json::{obj, s, Json};
use std::path::{Path, PathBuf};

pub const FLAG: &str = "/var/lib/fcw/apc";

pub const USAGE: &str = "usage:\n  \
    fcw apc on [machine...]   start confirming to the world that this computer runs the plant (100 % CU)\n  \
    fcw apc off               stop: machines get 75 % of the CPU's compute units\n  \
    fcw apc status            is it on, and what each machine receives";

/// Где лежит флаг; `FCW_APC_FLAG` — для проверок вне гостя.
pub fn flag_path() -> PathBuf {
    std::env::var_os("FCW_APC_FLAG").filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(FLAG))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    On(Vec<String>),
    Off,
    Status,
}

pub fn parse_args(args: &[&str]) -> Result<Cmd, String> {
    match args {
        ["on", machines @ ..] => {
            if let Some(bad) = machines.iter().find(|m| m.starts_with('-')) {
                return Err(format!("unknown option: {bad}"));
            }
            Ok(Cmd::On(machines.iter().map(|m| m.to_string()).collect()))
        }
        ["off"] => Ok(Cmd::Off),
        [] | ["status"] => Ok(Cmd::Status),
        _ => Err("expected on, off or status".into()),
    }
}

/// Машины из файла-флага: по имени на строку. `None` — флага нет, служба выключена.
pub fn read_flag(path: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect())
}

pub fn write_flag(path: &Path, machines: &[String]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = machines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    std::fs::write(path, text)
}

pub fn clear_flag(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Подтверждение для мира: `{"type":"apc_beat","machines":[…]}`; без списка — всем машинам сети.
pub fn beat(machines: &[String]) -> Json {
    let mut v = vec![("type", s("apc_beat"))];
    if !machines.is_empty() {
        v.push(("machines", Json::Arr(machines.iter().map(|m| s(m)).collect())));
    }
    obj(v)
}

/// Строка состояния машины для `fcw apc status`: имя, получено / нужно, доля от максимума.
/// `None` — машине APC не нужен.
pub fn machine_line(name: &str, status: &Json, ru: bool) -> Option<String> {
    let apc = status.get("apc")?;
    let need = apc.num("need");
    if need <= 0.0 {
        return None;
    }
    let got = apc.num("cu").min(need);
    let hosts = apc.items("hosts");
    let service = hosts.iter().any(|h| h.flag("service"));
    let note = if hosts.is_empty() {
        if ru {
            "  нет компьютера"
        } else {
            "  no computer"
        }
    } else if !service {
        if ru {
            "  без службы"
        } else {
            "  no service"
        }
    } else {
        ""
    };
    let cu = |v: f64| if (v - v.round()).abs() < 0.05 { format!("{:.0}", v.round()) } else { format!("{v:.1}") };
    Some(format!("{name:<10} {:>4} / {:<4} CU  {:>3.0} %{note}", cu(got), cu(need), got / need * 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(parse_args(&["on"]), Ok(Cmd::On(vec![])));
        assert_eq!(parse_args(&["on", "euv-1", "cz-1"]), Ok(Cmd::On(vec!["euv-1".into(), "cz-1".into()])));
        assert_eq!(parse_args(&["off"]), Ok(Cmd::Off));
        assert_eq!(parse_args(&[]), Ok(Cmd::Status));
        assert_eq!(parse_args(&["status"]), Ok(Cmd::Status));
        assert!(parse_args(&["restart"]).is_err());
        assert!(parse_args(&["on", "--all"]).is_err());
    }

    #[test]
    fn flag_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("fcw-apc-test-{}", std::process::id()));
        let path = dir.join("state").join("apc");
        assert_eq!(read_flag(&path), None);
        write_flag(&path, &[]).unwrap();
        assert_eq!(read_flag(&path), Some(vec![]));
        write_flag(&path, &["euv-1".into(), "cz-1".into()]).unwrap();
        assert_eq!(read_flag(&path), Some(vec!["euv-1".to_string(), "cz-1".to_string()]));
        clear_flag(&path).unwrap();
        clear_flag(&path).unwrap();
        assert_eq!(read_flag(&path), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn beat_lists_machines_only_when_asked() {
        assert_eq!(beat(&[]).to_string(), r#"{"type":"apc_beat"}"#);
        assert_eq!(beat(&["euv-1".into()]).to_string(), r#"{"type":"apc_beat","machines":["euv-1"]}"#);
    }

    #[test]
    fn status_lines() {
        let full = Json::parse(r#"{"apc":{"cu":8,"max":8,"need":8,"hosts":[{"host":"fedora-1.mc","service":true}]}}"#)
            .unwrap();
        assert_eq!(machine_line("euv-1", &full, true).unwrap(), "euv-1         8 / 8    CU  100 %");
        let part = Json::parse(r#"{"apc":{"cu":6,"max":8,"need":8,"hosts":[{"host":"fedora-1.mc","service":false}]}}"#)
            .unwrap();
        assert_eq!(machine_line("euv-1", &part, true).unwrap(), "euv-1         6 / 8    CU   75 %  без службы");
        let none = Json::parse(r#"{"apc":{"cu":0,"max":0,"need":0,"hosts":[]}}"#).unwrap();
        assert_eq!(machine_line("eaf-1", &none, true), None);
    }
}
