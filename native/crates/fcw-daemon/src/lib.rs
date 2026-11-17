//! Демон ВМ FCW: запускает гостевые машины под QEMU, снимает их экраны
//! (VNC для gfx0, virtio-console для tty0) и раздаёт их клиентам по QUIC.
//! Сервер Minecraft управляет демоном через Unix-сокет (control.proto).

pub mod accel;
pub mod agent;
pub mod args;
pub mod audio;
pub mod auth;
pub mod cert;
pub mod control;
pub mod daemon;
pub mod gfx;
pub mod images;
pub mod keymap;
pub mod localsock;
pub mod logfwd;
pub mod net;
pub mod qemu;
pub mod qmp;
pub mod quic;
pub mod rfb;
pub mod text;
pub mod tokens;
pub mod vm;

pub use fcw_proto::control as pb;
pub use fcw_proto::stream as spb;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Задача tokio, которая отменяется при удалении хэндла.
pub struct AbortOnDrop<T>(pub tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Текущее время Unix в миллисекундах.
pub fn unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Текущее время Unix в микросекундах.
pub fn unix_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
