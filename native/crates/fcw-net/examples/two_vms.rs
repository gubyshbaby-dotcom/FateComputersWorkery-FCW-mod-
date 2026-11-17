//! Свитч на Unix-сокетах для ручной проверки с живыми ВМ.
//!
//! ```text
//! cargo run --release --example two_vms -- <каталог> [имя ...]
//! ```
//!
//! Для каждого имени (по умолчанию `vm1 vm2`) создаёт `<каталог>/<имя>.sock`
//! и печатает строку `PORT <имя> <сокет> <mac> <ip>` плюс аргументы QEMU.
//! Прокси берётся из `HTTPS_PROXY` (отключить: `FCW_NET_NO_PROXY=1`; все
//! TCP-порты через прокси: `FCW_NET_PROXY_ALL=1`). Лимит скорости — `FCW_NET_RATE`
//! (байт/с). Раз в 20 с печатает статистику портов в stderr.

use std::time::Duration;

use fcw_net::{NetConfig, NetSwitch};

fn mac_str(m: [u8; 6]) -> String {
    m.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "fcw_net=info".into()),
        )
        .init();
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "/tmp/fcw-net".into()));
    let mut names: Vec<String> = args.collect();
    if names.is_empty() {
        names = vec!["vm1".into(), "vm2".into()];
    }
    std::fs::create_dir_all(&dir)?;

    let mut cfg = NetConfig::default();
    if std::env::var_os("FCW_NET_NO_PROXY").is_none() {
        cfg.upstream_https_proxy = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("https_proxy")).ok();
    }
    if std::env::var_os("FCW_NET_PROXY_ALL").is_some() {
        cfg.upstream_proxy_ports.clear();
    }
    if let Ok(r) = std::env::var("FCW_NET_RATE") {
        cfg.rate_limit_bytes_per_sec = r.parse().expect("FCW_NET_RATE");
    }
    eprintln!("config: {cfg:?}");
    let sw = NetSwitch::new(cfg)?;

    let mut listeners = Vec::new();
    for n in &names {
        let mac = NetSwitch::mac_for_vm(n);
        let sock = dir.join(format!("{n}.sock"));
        listeners.push(sw.listen_unix(&sock, n, n, mac)?);
        println!("PORT {n} {} {}", sock.display(), mac_str(mac));
        println!(
            "  qemu: -netdev stream,id=net0,server=off,addr.type=unix,addr.path={},reconnect=1 \
             -device virtio-net-pci,netdev=net0,mac={},romfile=",
            sock.display(),
            mac_str(mac)
        );
    }
    println!("READY gateway={} zone={}", sw.gateway(), sw.dns_zone());

    let mut tick = tokio::time::interval(Duration::from_secs(20));
    tick.tick().await;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tick.tick() => {
                for n in &names {
                    if let Some(s) = sw.stats(n) {
                        eprintln!("stats {n}: {s:?}");
                    }
                }
            }
        }
    }
    drop(listeners);
    Ok(())
}
