//! Клиентская нативная библиотека FCW.
//!
//! * [`session`] — QUIC-сессия с демоном (общая для JNI и fcw-probe);
//! * [`view`] — буферы видов RGBA, грязные области, курсор, текст;
//! * [`jni_api`] — экспорт `com.fatecomputers.fcw.client.nativelib.FcwNative`.

pub mod audio;
pub mod font;
pub mod glfw;
pub mod jni_api;
pub mod session;
pub mod shaper;
pub mod tls;
pub mod view;

pub use session::Session;
pub use view::View;
