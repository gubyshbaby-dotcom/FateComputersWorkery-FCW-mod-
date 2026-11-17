//! Файрвол исходящих соединений гостя (NAT наружу).

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use ipnet::Ipv4Net;
use parking_lot::Mutex;

/// Правила для соединений гостя наружу (через NAT шлюза).
///
/// Порядок проверки:
/// 1. всегда запрещены: своя подсеть (в т.ч. шлюз — его службы DHCP/DNS/ICMP
///    обслуживаются отдельно), 0.0.0.0/8, мультикаст, 255.255.255.255;
/// 2. `blocked_ports` (TCP и UDP);
/// 3. `deny`;
/// 4. `allow` — разрешено, даже если ниже стоит запрет (явное исключение админа);
/// 5. `block_private`: RFC1918, CGNAT 100.64/10, link-local 169.254/16,
///    127/8, 240/4;
/// 6. `block_host_addresses`: все адреса интерфейсов хоста (и 127/8);
/// 7. иначе разрешено.
#[derive(Clone, Debug)]
pub struct FirewallConfig {
    pub block_private: bool,
    pub block_host_addresses: bool,
    pub allow: Vec<Ipv4Net>,
    pub deny: Vec<Ipv4Net>,
    pub blocked_ports: Vec<u16>,
}

impl Default for FirewallConfig {
    fn default() -> Self {
        FirewallConfig {
            block_private: true,
            block_host_addresses: true,
            allow: Vec::new(),
            deny: Vec::new(),
            blocked_ports: vec![25],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(&'static str),
}

const PRIVATE: [(u32, u8); 8] = [
    (0x0000_0000, 8),  // 0.0.0.0/8
    (0x0a00_0000, 8),  // 10/8
    (0x6440_0000, 10), // 100.64/10
    (0x7f00_0000, 8),  // 127/8
    (0xa9fe_0000, 16), // 169.254/16
    (0xac10_0000, 12), // 172.16/12
    (0xc0a8_0000, 16), // 192.168/16
    (0xf000_0000, 4),  // 240/4 (вкл. 255.255.255.255)
];

fn in_prefix(ip: Ipv4Addr, (net, len): (u32, u8)) -> bool {
    let mask = if len == 0 { 0 } else { u32::MAX << (32 - len) };
    u32::from(ip) & mask == net
}

pub fn is_private(ip: Ipv4Addr) -> bool {
    PRIVATE.iter().any(|&p| in_prefix(ip, p)) || ip.is_multicast()
}

/// Чистая проверка (без обращения к системе).
pub fn check(
    cfg: &FirewallConfig,
    subnet: Ipv4Net,
    host_addrs: &[Ipv4Addr],
    dst: Ipv4Addr,
    dport: u16,
) -> Verdict {
    if subnet.contains(&dst)
        || dst.is_unspecified()
        || in_prefix(dst, (0, 8))
        || dst.is_multicast()
        || dst.is_broadcast()
    {
        return Verdict::Deny("internal");
    }
    if cfg.blocked_ports.contains(&dport) {
        return Verdict::Deny("port");
    }
    if cfg.deny.iter().any(|n| n.contains(&dst)) {
        return Verdict::Deny("deny-list");
    }
    if cfg.allow.iter().any(|n| n.contains(&dst)) {
        return Verdict::Allow;
    }
    if cfg.block_private && is_private(dst) {
        return Verdict::Deny("private");
    }
    if cfg.block_host_addresses && (dst.is_loopback() || host_addrs.contains(&dst)) {
        return Verdict::Deny("host");
    }
    Verdict::Allow
}

/// Файрвол с кешем адресов хоста (обновляется раз в несколько секунд).
pub struct Firewall {
    cfg: FirewallConfig,
    subnet: Ipv4Net,
    host: Mutex<(Option<Instant>, Vec<Ipv4Addr>)>,
}

const HOST_ADDRS_TTL: Duration = Duration::from_secs(5);

pub fn host_ipv4_addrs() -> Vec<Ipv4Addr> {
    match if_addrs::get_if_addrs() {
        Ok(list) => list
            .into_iter()
            .filter_map(|i| match i.ip() {
                IpAddr::V4(a) => Some(a),
                IpAddr::V6(_) => None,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("fcw-net: не удалось получить адреса хоста: {e}");
            Vec::new()
        }
    }
}

impl Firewall {
    pub fn new(cfg: FirewallConfig, subnet: Ipv4Net) -> Self {
        Firewall { cfg, subnet, host: Mutex::new((None, Vec::new())) }
    }

    pub fn check(&self, dst: Ipv4Addr, dport: u16) -> Verdict {
        let addrs = if self.cfg.block_host_addresses {
            let mut h = self.host.lock();
            if h.0.map_or(true, |t| t.elapsed() > HOST_ADDRS_TTL) {
                h.1 = host_ipv4_addrs();
                h.0 = Some(Instant::now());
            }
            h.1.clone()
        } else {
            Vec::new()
        };
        check(&self.cfg, self.subnet, &addrs, dst, dport)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    fn sub() -> Ipv4Net {
        "10.42.0.0/16".parse().unwrap()
    }

    #[test]
    fn defaults_block_private_and_host() {
        let cfg = FirewallConfig::default();
        let host = [ip("203.0.113.7"), ip("192.168.1.20")];
        let c = |d: &str, p: u16| check(&cfg, sub(), &host, ip(d), p);
        assert_eq!(c("93.184.215.14", 443), Verdict::Allow);
        assert_eq!(c("1.1.1.1", 53), Verdict::Allow);
        for d in [
            "10.0.0.1",
            "10.255.255.254",
            "172.16.0.1",
            "172.31.255.1",
            "192.168.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "169.254.169.254",
            "127.0.0.1",
            "127.1.2.3",
            "240.0.0.1",
        ] {
            assert_eq!(c(d, 80), Verdict::Deny("private"), "{d}");
        }
        for d in ["10.42.0.1", "10.42.5.5", "0.0.0.0", "0.1.2.3", "224.0.0.251", "239.1.1.1", "255.255.255.255"] {
            assert_eq!(c(d, 80), Verdict::Deny("internal"), "{d}");
        }
        assert_eq!(c("203.0.113.7", 80), Verdict::Deny("host"));
        assert_eq!(c("8.8.8.8", 25), Verdict::Deny("port"));
        // Соседи диапазонов не задеты.
        assert_eq!(c("172.32.0.1", 80), Verdict::Allow);
        assert_eq!(c("100.128.0.1", 80), Verdict::Allow);
        assert_eq!(c("11.0.0.1", 80), Verdict::Allow);
        assert_eq!(c("192.169.0.1", 80), Verdict::Allow);
    }

    #[test]
    fn allow_and_deny_lists() {
        let cfg = FirewallConfig {
            allow: vec!["192.168.1.0/24".parse().unwrap(), "10.42.0.0/16".parse().unwrap()],
            deny: vec!["8.8.4.0/24".parse().unwrap(), "192.168.1.66/32".parse().unwrap()],
            blocked_ports: vec![25, 6667],
            ..FirewallConfig::default()
        };
        let c = |d: &str, p: u16| check(&cfg, sub(), &[], ip(d), p);
        assert_eq!(c("192.168.1.10", 80), Verdict::Allow);
        assert_eq!(c("192.168.1.66", 80), Verdict::Deny("deny-list"));
        assert_eq!(c("192.168.2.10", 80), Verdict::Deny("private"));
        assert_eq!(c("8.8.4.4", 53), Verdict::Deny("deny-list"));
        assert_eq!(c("8.8.8.8", 53), Verdict::Allow);
        assert_eq!(c("192.168.1.10", 6667), Verdict::Deny("port"));
        // Своя подсеть не открывается даже allow-списком.
        assert_eq!(c("10.42.0.1", 80), Verdict::Deny("internal"));
    }

    #[test]
    fn switches_off() {
        let cfg = FirewallConfig {
            block_private: false,
            block_host_addresses: false,
            blocked_ports: vec![],
            ..FirewallConfig::default()
        };
        let c = |d: &str, p: u16| check(&cfg, sub(), &[ip("203.0.113.7")], ip(d), p);
        assert_eq!(c("192.168.0.1", 25), Verdict::Allow);
        assert_eq!(c("203.0.113.7", 80), Verdict::Allow);
        assert_eq!(c("10.42.0.1", 80), Verdict::Deny("internal"));
        // 127/8 при выключенных обоих флагах открыт только явно (здесь флаги выключены).
        assert_eq!(c("127.0.0.1", 80), Verdict::Allow);

        let cfg = FirewallConfig { block_private: false, ..FirewallConfig::default() };
        assert_eq!(check(&cfg, sub(), &[], ip("127.0.0.1"), 80), Verdict::Deny("host"));
    }

    #[test]
    fn host_addrs_include_loopback() {
        // На любой машине есть хотя бы 127.0.0.1.
        let fw = Firewall::new(FirewallConfig { block_private: false, ..Default::default() }, sub());
        assert_eq!(fw.check(ip("127.0.0.1"), 80), Verdict::Deny("host"));
        for a in host_ipv4_addrs() {
            assert!(matches!(fw.check(a, 80), Verdict::Deny(_)), "{a}");
        }
    }
}
