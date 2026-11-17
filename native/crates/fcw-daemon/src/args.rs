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
    /// Не завершаться при закрытии stdin (для ручного запуска и тестов)
    #[arg(long, hide = true)]
    pub no_stdin_watch: bool,
}
