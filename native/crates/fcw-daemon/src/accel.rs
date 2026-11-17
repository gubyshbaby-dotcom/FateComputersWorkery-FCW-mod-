//! Выбор ускорителя QEMU.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    Kvm,
    Whpx,
    Hvf,
    Tcg,
}

impl Accel {
    pub fn name(self) -> &'static str {
        match self {
            Accel::Kvm => "kvm",
            Accel::Whpx => "whpx",
            Accel::Hvf => "hvf",
            Accel::Tcg => "tcg",
        }
    }

    pub fn parse(s: &str) -> Option<Option<Accel>> {
        Some(match s.to_ascii_lowercase().as_str() {
            "auto" | "" => None,
            "kvm" => Some(Accel::Kvm),
            "whpx" => Some(Accel::Whpx),
            "hvf" => Some(Accel::Hvf),
            "tcg" => Some(Accel::Tcg),
            _ => return None,
        })
    }

    /// Аргументы `-accel` и `-cpu` для QEMU.
    pub fn qemu_args(self) -> Vec<String> {
        let (accel, cpu) = match self {
            Accel::Kvm => ("kvm", "host"),
            Accel::Hvf => ("hvf", "host"),
            Accel::Whpx => ("whpx,kernel-irqchip=off", "max"),
            Accel::Tcg => ("tcg,thread=multi", "max"),
        };
        vec!["-accel".into(), accel.into(), "-cpu".into(), cpu.into()]
    }
}

/// /dev/kvm доступен на чтение и запись.
pub fn kvm_usable() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::fs::OpenOptions::new().read(true).write(true).open("/dev/kvm").is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Пробный запуск QEMU с ускорителем: если процесс жив через пару секунд,
/// ускоритель работает.
pub async fn probe(qemu: &Path, accel: Accel) -> bool {
    let mut cmd = tokio::process::Command::new(qemu);
    cmd.args(["-machine", "none", "-accel", accel.name(), "-display", "none", "-nodefaults", "-no-user-config", "-S"])
        .args(["-monitor", "none", "-serial", "none"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let Ok(mut child) = cmd.spawn() else { return false };
    match tokio::time::timeout(Duration::from_millis(2500), child.wait()).await {
        Ok(_) => false, // вышел сам — ошибка ускорителя
        Err(_) => {
            let _ = child.kill().await;
            true
        }
    }
}

/// Режим `auto`: kvm -> whpx (Windows) -> hvf (macOS x86_64) -> tcg.
pub async fn detect(qemu: &Path) -> Accel {
    if cfg!(target_os = "linux") && kvm_usable() {
        return Accel::Kvm;
    }
    if cfg!(windows) && probe(qemu, Accel::Whpx).await {
        return Accel::Whpx;
    }
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) && probe(qemu, Accel::Hvf).await {
        return Accel::Hvf;
    }
    Accel::Tcg
}
