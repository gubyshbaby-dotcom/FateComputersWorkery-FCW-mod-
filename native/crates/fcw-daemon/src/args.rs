use clap::Parser;
use std::path::PathBuf;

/// Аргументы командной строки (CONTRACT.md §2).
#[derive(Parser, Debug, Clone)]
#[command(name = "fcw-daemon", version, about = "FCW VM daemon")]
pub struct Args {
    /// Каталог гостевых образов (<id>/manifest.json, vmlinuz, initrd.img, rootfs.qcow2)
    #[arg(long)]
    pub images_dir: PathBuf,
    /// Каталог машин мира (<vm_id>/config.json, disk.qcow2, сокеты)
    #[arg(long)]
    pub vms_dir: PathBuf,
    /// Путь к Unix-сокету канала управления
    #[arg(long)]
    pub control: PathBuf,
    /// Адрес UDP для QUIC-сервера
    #[arg(long, default_value = "0.0.0.0:25600")]
    pub quic_bind: String,
    /// Путь к qemu-system-x86_64 (иначе $FCW_QEMU, иначе PATH)
    #[arg(long)]
    pub qemu: Option<PathBuf>,
    /// Предел одновременно работающих машин
    #[arg(long, default_value_t = 8)]
    pub max_vms: u32,
    /// Ускоритель: auto | kvm | whpx | hvf | tcg
    #[arg(long, default_value = "auto")]
    pub accel: String,
    /// Сеть гостей (свитч fcw-net): on | off
    #[arg(long, default_value = "on", value_parser = parse_on_off)]
    pub network: bool,
    /// Выход гостей в интернет через NAT: on | off
    #[arg(long, default_value = "on", value_parser = parse_on_off)]
    pub internet: bool,
    /// Подсеть машин (шлюз, DHCP и DNS — первый адрес)
    #[arg(long, default_value = "10.42.0.0/16")]
    pub net_subnet: String,
    /// Лимит скорости на машину, байт/с в каждую сторону (0 — без лимита)
    #[arg(long)]
    pub net_rate_limit: Option<u64>,
    /// Разрешить адреса/подсети, иначе запрещённые файрволом (повторяемый)
    #[arg(long = "net-allow")]
    pub net_allow: Vec<String>,
    /// Запретить адреса/подсети (повторяемый)
    #[arg(long = "net-deny")]
    pub net_deny: Vec<String>,
    /// HTTP-прокси для исходящих HTTPS гостей (по умолчанию HTTPS_PROXY; none — без прокси)
    #[arg(long)]
    pub upstream_proxy: Option<String>,
    /// Не завершаться при закрытии stdin (для ручного запуска и тестов)
    #[arg(long, hide = true)]
    pub no_stdin_watch: bool,
}

fn parse_on_off(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err(format!("expected on or off, got {s:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_flags() {
        let a = Args::try_parse_from([
            "fcw-daemon", "--images-dir", "i", "--vms-dir", "v", "--control", "c", "--internet", "off",
            "--net-allow", "10.1.0.0/16", "--net-allow", "10.2.0.5", "--net-deny", "1.1.1.1",
        ])
        .unwrap();
        assert!(a.network);
        assert!(!a.internet);
        assert_eq!(a.net_allow, vec!["10.1.0.0/16", "10.2.0.5"]);
        assert_eq!(a.net_deny, vec!["1.1.1.1"]);
        assert_eq!(a.net_subnet, "10.42.0.0/16");
        assert!(Args::try_parse_from(["fcw-daemon", "--images-dir", "i", "--vms-dir", "v", "--control", "c", "--network", "maybe"]).is_err());
    }
}
