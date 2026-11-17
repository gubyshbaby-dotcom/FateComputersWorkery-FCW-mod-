//! Консольные программы FCW (`fcw top`, `fcw litho`, …): отрисовка в сетку знакомест по снимкам
//! машин и обработка клавиш. Терминального ввода-вывода здесь нет: сетку выводит тот, кто её
//! заказал — агент гостя ANSI-последовательностями, пульт машины текстурой консоли.
//! Дизайн: docs/design/CONSOLE_APPS.md.

pub mod app;
pub mod demo;
pub mod grid;
pub mod json;
pub mod model;
mod screens;
pub mod text;

pub use app::{Action, App, Key, Modal, Program};
pub use grid::{Cell, Grid};
pub use model::World;
pub use text::Lang;
