//! fcw-probe: консольный клиент для проверок демона без игры.
//!
//! Канал управления — тот же Unix-сокет, что у мода; экраны и ввод — через
//! QUIC тем же кодом сессии, что и в клиентской библиотеке (fcw_client::Session).

#[cfg(unix)]
mod probe;

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    probe::main()
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("fcw-probe: only Unix is supported");
    std::process::ExitCode::FAILURE
}
