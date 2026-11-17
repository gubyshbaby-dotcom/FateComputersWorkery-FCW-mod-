//! Сеть гостей: подключение машин к свитчу fcw-net (CONTRACT.md §3, «Сеть
//! гостей»). QEMU подключается к порту машины через `-netdev stream`
//! (на Unix — сокет в каталоге машины, на Windows — TCP на 127.0.0.1).

use crate::args::Args;
use anyhow::{anyhow, Context, Result};
use fcw_net::{FirewallConfig, NetConfig, NetSwitch};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Свитч демона и его настройки для HelloResult.
pub struct NetEnv {
    pub switch: Arc<NetSwitch>,
    pub internet: bool,
    pub subnet: String,
    pub zone: String,
}

impl std::fmt::Debug for NetEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetEnv")
            .field("internet", &self.internet)
            .field("subnet", &self.subnet)
            .field("zone", &self.zone)
            .finish()
    }
}

fn parse_nets(list: &[String], what: &str) -> Result<Vec<fcw_net::Ipv4Net>> {
    list.iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            // одиночный адрес = /32
            let s = if s.contains('/') { s.to_string() } else { format!("{s}/32") };
            s.parse::<fcw_net::Ipv4Net>().with_context(|| format!("{what} {s:?}: expected IPv4 CIDR"))
        })
        .collect()
}

/// Прокси для исходящих HTTPS гостей: `--upstream-proxy`, иначе HTTPS_PROXY.
/// `none`, `off` или пустая строка отключают прокси.
fn upstream_proxy(arg: &Option<String>) -> Option<String> {
    let v = match arg {
        Some(v) => v.clone(),
        None => std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("https_proxy")).unwrap_or_default(),
    };
    let v = v.trim().to_string();
    (!v.is_empty() && !matches!(v.to_ascii_lowercase().as_str(), "none" | "off")).then_some(v)
}

/// Создаёт свитч по флагам демона. `Ok(None)` — сеть выключена (`--network off`).
pub fn build(args: &Args) -> Result<Option<Arc<NetEnv>>> {
    if !args.network {
        return Ok(None);
    }
    let subnet: fcw_net::Ipv4Net =
        args.net_subnet.parse().with_context(|| format!("--net-subnet {:?}: expected IPv4 CIDR", args.net_subnet))?;
    let mut cfg = NetConfig {
        subnet,
        allow_internet: args.internet,
        upstream_https_proxy: upstream_proxy(&args.upstream_proxy),
        firewall: FirewallConfig {
            allow: parse_nets(&args.net_allow, "--net-allow")?,
            deny: parse_nets(&args.net_deny, "--net-deny")?,
            ..FirewallConfig::default()
        },
        ..NetConfig::default()
    };
    if let Some(r) = args.net_rate_limit {
        cfg.rate_limit_bytes_per_sec = r;
    }
    let subnet_str = cfg.subnet.trunc().to_string();
    let proxy = cfg.upstream_https_proxy.is_some();
    let switch = NetSwitch::new(cfg).map_err(|e| anyhow!("network switch: {e}"))?;
    let zone = switch.dns_zone().to_string();
    tracing::info!(
        "network: subnet {subnet_str}, gateway {}, zone .{zone}, internet {}, upstream proxy {}",
        switch.gateway(),
        if args.internet { "on" } else { "off" },
        if proxy { "on" } else { "off" }
    );
    Ok(Some(Arc::new(NetEnv { switch, internet: args.internet, subnet: subnet_str, zone })))
}

/// Имя машины для DNS и `systemd.hostname=`: `[a-z0-9-]`, до 63 символов.
pub fn sanitize_hostname(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' { c } else { '-' })
        .collect();
    let mut out = String::new();
    for c in mapped.chars() {
        // без повторяющихся дефисов
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    let mut out = out.trim_matches('-').to_string();
    out.truncate(63);
    out.trim_end_matches('-').to_string()
}

/// Имя по умолчанию: `pc-<первые 8 символов vm_id>`.
pub fn default_hostname(vm_id: &str) -> String {
    let short: String = sanitize_hostname(vm_id).chars().filter(|c| *c != '-').take(8).collect();
    if short.is_empty() {
        "pc".into()
    } else {
        format!("pc-{short}")
    }
}

pub fn mac_string(m: [u8; 6]) -> String {
    m.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

/// Версия QEMU из `--version` ("QEMU emulator version 8.2.2 ...").
pub fn qemu_version(qemu: &Path) -> Option<(u32, u32, u32)> {
    let out = std::process::Command::new(qemu).arg("--version").output().ok()?;
    parse_qemu_version(&String::from_utf8_lossy(&out.stdout))
}

pub fn parse_qemu_version(s: &str) -> Option<(u32, u32, u32)> {
    let rest = s.split("version").nth(1)?.trim_start();
    let ver: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut it = ver.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

/// Как QEMU подключается к порту машины.
#[derive(Debug, Clone, PartialEq)]
pub enum NetAttach {
    Unix(PathBuf),
    Tcp(u16),
}

/// Аргумент `-netdev` для подключения к свитчу. С QEMU 9.2 параметр
/// `reconnect` заменён на `reconnect-ms`.
pub fn netdev_arg(attach: &NetAttach, qemu_version: Option<(u32, u32, u32)>) -> String {
    let addr = match attach {
        NetAttach::Unix(p) => format!("addr.type=unix,addr.path={}", p.to_string_lossy().replace(',', ",,")),
        NetAttach::Tcp(port) => format!("addr.type=inet,addr.host=127.0.0.1,addr.port={port}"),
    };
    let reconnect = match qemu_version {
        Some(v) if v >= (9, 2, 0) => "reconnect-ms=1000",
        _ => "reconnect=1",
    };
    format!("stream,id=net0,server=off,{addr},{reconnect}")
}

/// Порт машины на свитче. При удалении перестаёт принимать подключения
/// (сокет удаляется); текущее подключение QEMU разрывается через
/// [`NetSwitch::disconnect`].
pub struct NetPort {
    pub attach: NetAttach,
    #[cfg(unix)]
    _listener: fcw_net::PortListener,
    #[cfg(not(unix))]
    _task: crate::AbortOnDrop<()>,
}

/// Открывает порт машины: на Unix — сокет `sock_path`, на Windows — TCP
/// 127.0.0.1 со случайным портом (принимается только первое подключение).
pub async fn open_port(
    net: &NetEnv,
    sock_path: &Path,
    vm_id: &str,
    hostname: &str,
    mac: [u8; 6],
) -> Result<NetPort> {
    #[cfg(unix)]
    {
        let l = net
            .switch
            .listen_unix(sock_path, vm_id, hostname, mac)
            .with_context(|| format!("network socket {}", sock_path.display()))?;
        Ok(NetPort { attach: NetAttach::Unix(sock_path.to_path_buf()), _listener: l })
    }
    #[cfg(not(unix))]
    {
        let _ = sock_path;
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.context("network port")?;
        let port = l.local_addr()?.port();
        let sw = net.switch.clone();
        let (vm, host) = (vm_id.to_string(), hostname.to_string());
        let task = tokio::spawn(async move {
            let Ok((stream, _)) = l.accept().await else { return };
            drop(l); // больше никого не пускаем
            let _ = stream.set_nodelay(true);
            if let Err(e) = sw.run_port(&vm, &host, mac, stream).await {
                tracing::warn!(vm = %vm, "network port: {e}");
            }
        });
        Ok(NetPort { attach: NetAttach::Tcp(port), _task: crate::AbortOnDrop(task) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames() {
        assert_eq!(sanitize_hostname("Alice's PC #1"), "alice-s-pc-1");
        assert_eq!(sanitize_hostname("--Мой--"), "");
        assert_eq!(sanitize_hostname(&"a".repeat(80)).len(), 63);
        assert_eq!(sanitize_hostname(&format!("{}-b", "a".repeat(62))), "a".repeat(62));
        assert_eq!(default_hostname("550e8400-e29b-41d4-a716-446655440000"), "pc-550e8400");
        assert_eq!(default_hostname("pinA"), "pc-pina");
    }

    #[test]
    fn versions_and_netdev() {
        assert_eq!(parse_qemu_version("QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.18)"), Some((8, 2, 2)));
        assert_eq!(parse_qemu_version("QEMU emulator version 9.2.0\n"), Some((9, 2, 0)));
        assert_eq!(parse_qemu_version("QEMU emulator version 10.1"), Some((10, 1, 0)));
        assert_eq!(parse_qemu_version("garbage"), None);
        let u = NetAttach::Unix(PathBuf::from("/vms/a,b/net.sock"));
        assert_eq!(
            netdev_arg(&u, Some((8, 2, 2))),
            "stream,id=net0,server=off,addr.type=unix,addr.path=/vms/a,,b/net.sock,reconnect=1"
        );
        assert!(netdev_arg(&u, Some((9, 2, 0))).ends_with(",reconnect-ms=1000"));
        assert!(netdev_arg(&u, None).ends_with(",reconnect=1"));
        assert_eq!(
            netdev_arg(&NetAttach::Tcp(4000), Some((10, 0, 0))),
            "stream,id=net0,server=off,addr.type=inet,addr.host=127.0.0.1,addr.port=4000,reconnect-ms=1000"
        );
    }

    #[test]
    fn cidrs() {
        let v = parse_nets(&["10.0.0.0/8,1.2.3.4".into(), "192.168.1.0/24".into()], "x").unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[1].to_string(), "1.2.3.4/32");
        assert!(parse_nets(&["nope".into()], "x").is_err());
    }
}
