//! Типы протоколов FCW (prost) и кадрирование «u32 BE длина + сообщение».
//!
//! * [`control`] — канал управления мод <-> демон (`fcw.control.v1`).
//! * [`stream`] — клиент игрока <-> демон по QUIC (`fcw.stream.v1`).
//! * [`frame`] — синхронное и асинхронное (tokio) кадрирование.
//! * [`keys`] — коды клавиш Linux evdev, общие для клиента и демона.

pub mod control {
    include!(concat!(env!("OUT_DIR"), "/fcw.control.v1.rs"));
}

pub mod stream {
    include!(concat!(env!("OUT_DIR"), "/fcw.stream.v1.rs"));
}

pub mod frame;
pub mod keys;

pub use prost::Message;

/// ALPN QUIC-соединения клиент <-> демон.
pub const ALPN: &[u8] = b"fcw/1";
/// Версия протокола канала управления (Hello.protocol_version).
pub const CONTROL_PROTOCOL_VERSION: u32 = 1;
