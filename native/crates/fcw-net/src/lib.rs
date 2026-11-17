//! fcw-net: сеть для гостевых ВМ FCW целиком в пространстве пользователя.
//!
//! - L2-свитч между портами-машинами (кадры от QEMU `-netdev stream`:
//!   u32 BE длина + Ethernet-кадр);
//! - виртуальный шлюз на свитче (по умолчанию 10.42.0.1): ARP, ping, DHCP,
//!   DNS (`<hostname>.<zone>`, остальное — через резолвер хоста);
//! - NAT наружу: TCP терминируется стеком smoltcp и проксируется в сокеты
//!   хоста (или через HTTP CONNECT-прокси), UDP — потоками;
//! - файрвол исходящих соединений и лимиты на машину.
//!
//! ```no_run
//! # async fn demo() -> std::io::Result<()> {
//! use fcw_net::{NetConfig, NetSwitch};
//! let sw = NetSwitch::new(NetConfig::default())?;
//! let mac = NetSwitch::mac_for_vm("vm-1");
//! // QEMU: -netdev stream,id=net0,server=off,addr.type=unix,addr.path=/run/vm-1/net.sock,reconnect=1
//! //       -device virtio-net-pci,netdev=net0,mac=52:54:00:..,romfile=
//! # #[cfg(unix)]
//! let _listener = sw.listen_unix("/run/vm-1/net.sock", "vm-1", "alice-pc", mac)?;
//! # Ok(()) }
//! ```

mod dhcp;
mod dns;
mod firewall;
mod nat_tcp;
mod nat_udp;
mod proxy;
mod ratelimit;
mod switch;
mod wire;

#[cfg(test)]
mod tests;

pub use firewall::FirewallConfig;
pub use ipnet::Ipv4Net;
#[cfg(unix)]
pub use switch::PortListener;
pub use switch::{NetSwitch, PortStats};

/// Настройки сети.
#[derive(Clone, Debug)]
pub struct NetConfig {
    /// Подсеть машин. Шлюз (и DNS) — первый адрес (`.0.1`), машинам выдаются
    /// адреса за пределами первой /24 (если подсеть больше /24).
    pub subnet: Ipv4Net,
    /// Зона DNS: машины видны как `<hostname>.<zone>`, шлюз — `gateway.<zone>`.
    pub dns_zone: String,
    pub firewall: FirewallConfig,
    /// Лимит на машину в каждую сторону, байт/с (весь трафик порта); 0 — без лимита.
    pub rate_limit_bytes_per_sec: u64,
    /// Одновременных NAT-соединений на машину (TCP + UDP-потоки).
    pub max_connections_per_vm: usize,
    /// Пускать ли машины наружу (NAT). Без него работают свитч, DHCP и DNS зоны.
    pub allow_internet: bool,
    /// HTTP-прокси для исходящих TCP (`http://[user:pass@]host:port`),
    /// например из `HTTPS_PROXY`. Через него идут соединения на `upstream_proxy_ports`.
    pub upstream_https_proxy: Option<String>,
    /// Порты назначения, которые идут через прокси; пусто — все TCP. По умолчанию 443.
    pub upstream_proxy_ports: Vec<u16>,
    /// Защита портов: кадры только с MAC порта, ARP/IPv4 только с адреса,
    /// выданного по DHCP, без IPv6/VLAN и чужих DHCP-серверов. Выключать стоит
    /// только для отладки: тогда свитч учит любые MAC (до 16 на порт).
    pub port_security: bool,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            subnet: "10.42.0.0/16".parse().unwrap(),
            dns_zone: "mc".into(),
            firewall: FirewallConfig::default(),
            rate_limit_bytes_per_sec: 8 * 1024 * 1024,
            max_connections_per_vm: 256,
            allow_internet: true,
            upstream_https_proxy: None,
            upstream_proxy_ports: vec![443],
            port_security: true,
        }
    }
}
