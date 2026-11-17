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
    /// Путь к Unix-сокету канала управления (в режиме --service необязателен)
    #[arg(long)]
    pub control: Option<PathBuf>,
    /// Канал управления по TCP (ip:port), требует --token-file
    #[arg(long)]
    pub control_tcp: Option<String>,
    /// Файл с секретом для --control-tcp (создаётся, если его нет)
    #[arg(long)]
    pub token_file: Option<PathBuf>,
    /// Режим службы: не выходить при закрытии stdin, Shutdown по TCP запрещён
    #[arg(long)]
    pub service: bool,
    /// Адрес UDP для QUIC-сервера
    #[arg(long, default_value = "0.0.0.0:25600")]
    pub quic_bind: String,
    /// Путь к qemu-system-x86_64 (иначе $FCW_QEMU, иначе PATH)
    #[arg(long)]
    pub qemu: Option<PathBuf>,
    /// Каталог переносимого QEMU (bin/, share/qemu/); по умолчанию <images-dir>/../qemu
    #[arg(long)]
    pub qemu_dir: Option<PathBuf>,
    /// Предел одновременно работающих машин
    #[arg(long, default_value_t = 8)]
    pub max_vms: u32,
    /// Ускоритель: auto | kvm | whpx | hvf | tcg
    #[arg(long, default_value = "auto")]
    pub accel: String,
    /// Сеть гостей (свитч fcw-net): on | off
    #[arg(long, default_value = "on", value_parser = parse_on_off, action = clap::ArgAction::Set)]
    pub network: bool,
    /// Выход гостей в интернет через NAT: on | off
    #[arg(long, default_value = "on", value_parser = parse_on_off, action = clap::ArgAction::Set)]
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
    /// Бюджет потока экрана на подписчика, Мбит/с (0 — без ограничения).
    /// При превышении «пёстрые» тайлы идут JPEG и позже досылаются без потерь.
    #[arg(long, default_value_t = 20.0)]
    pub screen_max_mbps: f64,
    /// Предел частоты кадров экрана на подписчика
    #[arg(long, default_value_t = 30)]
    pub screen_max_fps: u32,
    /// Никогда не слать тайлы с потерями (JPEG)
    #[arg(long)]
    pub screen_lossless: bool,
    /// Видеорежим: область экрана с видео идёт H.264 вместо тайлов: on | off
    #[arg(long, default_value = "on", value_parser = parse_on_off, action = clap::ArgAction::Set)]
    pub video: bool,
    /// Ступени битрейта видеорежима, Мбит/с (до четырёх). Зритель получает
    /// наибольшую ступень, которая укладывается в 70 % его бюджета.
    #[arg(long, default_value = "2,5,10")]
    pub video_tiers: String,
    /// Ступень для игрока на этом же компьютере (same_host), Мбит/с
    #[arg(long, default_value_t = 20.0)]
    pub video_local_mbps: f64,
    /// Частота кадров кодировщика видеорежима
    #[arg(long, default_value_t = 30)]
    pub video_fps: u32,
    /// Предел одновременных кодировщиков на весь демон (по одному на экран и
    /// ступень); по умолчанию половина ядер, но не меньше двух
    #[arg(long)]
    pub video_max_encoders: Option<u32>,
    /// Потоков на один кодировщик
    #[arg(long, default_value_t = 1)]
    pub video_threads: u16,
    /// Видео — это когда за кадр меняется такая доля экрана (0.05–1)
    #[arg(long, default_value_t = 0.2)]
    pub video_min_area: f32,
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
            "fcw-daemon",
            "--images-dir",
            "i",
            "--vms-dir",
            "v",
            "--control",
            "c",
            "--internet",
            "off",
            "--net-allow",
            "10.1.0.0/16",
            "--net-allow",
            "10.2.0.5",
            "--net-deny",
            "1.1.1.1",
        ])
        .unwrap();
        assert!(a.network);
        assert!(!a.internet);
        assert_eq!(a.net_allow, vec!["10.1.0.0/16", "10.2.0.5"]);
        assert_eq!(a.net_deny, vec!["1.1.1.1"]);
        assert_eq!(a.net_subnet, "10.42.0.0/16");
        assert!(Args::try_parse_from([
            "fcw-daemon",
            "--images-dir",
            "i",
            "--vms-dir",
            "v",
            "--control",
            "c",
            "--network",
            "maybe"
        ])
        .is_err());
    }
}

impl Args {
    pub fn video_config(&self) -> Result<crate::video::VideoConfig, String> {
        let tiers_kbps =
            crate::video::VideoConfig::parse_tiers(&self.video_tiers).map_err(|e| format!("--video-tiers: {e}"))?;
        Ok(crate::video::VideoConfig {
            enabled: self.video && cfg!(feature = "video"),
            tiers_kbps,
            local_kbps: (self.video_local_mbps.clamp(0.1, 200.0) * 1000.0) as u32,
            fps: self.video_fps.clamp(5, 60),
            max_encoders: self.video_max_encoders.unwrap_or_else(crate::video::default_max_encoders),
            threads: self.video_threads.clamp(1, 16),
            min_area: self.video_min_area.clamp(0.05, 1.0),
        })
    }

    pub fn screen_config(&self) -> crate::vm::ScreenConfig {
        crate::vm::ScreenConfig {
            video: self.video_config().unwrap_or_default(),
            max_bytes_per_sec: (self.screen_max_mbps.max(0.0) * 125_000.0) as u64,
            max_fps: self.screen_max_fps.clamp(1, 120),
            lossy: !self.screen_lossless,
            frame_acks: true,
            local_unlimited: true,
        }
    }

    /// Проверка сочетаний флагов канала управления.
    pub fn validate(&self) -> Result<(), String> {
        if self.control.is_none() && self.control_tcp.is_none() {
            return Err("either --control or --control-tcp is required".into());
        }
        if self.control_tcp.is_some() && self.token_file.is_none() {
            return Err("--control-tcp requires --token-file".into());
        }
        if let Some(a) = &self.control_tcp {
            a.parse::<std::net::SocketAddr>().map_err(|e| format!("--control-tcp {a:?}: {e}"))?;
        }
        self.video_config()?;
        Ok(())
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;

    fn parse(extra: &[&str]) -> Args {
        let mut v = vec!["fcw-daemon", "--images-dir", "i", "--vms-dir", "v"];
        v.extend_from_slice(extra);
        Args::try_parse_from(v).unwrap()
    }

    #[test]
    fn control_flags() {
        assert!(parse(&[]).validate().is_err(), "no control channel");
        assert!(parse(&["--control", "c"]).validate().is_ok());
        let a = parse(&["--service", "--control-tcp", "172.18.0.1:25700", "--token-file", "/etc/fcw/token"]);
        assert!(a.service && a.control.is_none());
        assert!(a.validate().is_ok());
        assert!(parse(&["--service", "--control-tcp", "127.0.0.1:1"]).validate().is_err(), "token file required");
        assert!(parse(&["--control-tcp", "nonsense", "--token-file", "t"]).validate().is_err());
    }

    #[test]
    fn video_flags() {
        let v = parse(&["--control", "c"]).screen_config().video;
        assert_eq!(v.enabled, cfg!(feature = "video"));
        assert_eq!(v.tiers_kbps, [2_000, 5_000, 10_000, 0]);
        assert_eq!((v.local_kbps, v.fps, v.threads), (20_000, 30, 1));
        assert!(v.max_encoders >= 2);
        let a = parse(&[
            "--control",
            "c",
            "--video",
            "off",
            "--video-tiers",
            "1,3",
            "--video-max-encoders",
            "1",
            "--video-fps",
            "24",
        ]);
        let v = a.screen_config().video;
        assert!(!v.enabled);
        assert_eq!((v.tiers_kbps, v.max_encoders, v.fps), ([1_000, 3_000, 0, 0], 1, 24));
        assert!(parse(&["--control", "c", "--video-tiers", "fast"]).validate().is_err());
    }
}
