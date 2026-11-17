//! Свитч: порты машин, таблица MAC, защита портов, шлюз (ARP, ICMP, DHCP, DNS)
//! и передача трафика наружу в NAT.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::{Mutex, RwLock};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::{mpsc, Notify, Semaphore};

use crate::dns::{self, Decision};
use crate::firewall::{Firewall, Verdict};
use crate::proxy::HttpProxy;
use crate::ratelimit::TokenBucket;
use crate::wire::{self, Mac};
use crate::{dhcp, nat_tcp, nat_udp, NetConfig};

pub(crate) type PortId = u64;

/// MAC шлюза (локально администрируемый, не пересекается с 52:54:00).
pub(crate) const GATEWAY_MAC: Mac = [0x02, 0xfc, 0x77, 0x00, 0x00, 0x01];
/// Самый большой кадр, который принимаем от QEMU (MTU 1500 + запас под VLAN/джамбо).
const MAX_FRAME: usize = 16 * 1024;
/// Больше этого в поле длины — поток испорчен.
const MAX_STREAM_LEN: usize = 256 * 1024;
const PORT_QUEUE: usize = 512;
const DNS_INFLIGHT_PER_PORT: usize = 32;
const LEASE_SECS: u32 = 12 * 3600;
const LEARNED_MACS_PER_PORT: usize = 16;
const MAC_AGE: Duration = Duration::from_secs(300);
pub(crate) const MTU: usize = 1500;

static IP_ID: AtomicU16 = AtomicU16::new(1);

pub(crate) fn next_ip_id() -> u16 {
    IP_ID.fetch_add(1, Ordering::Relaxed)
}

/// FNV-1a 64: стабильный хеш (не зависит от версии Rust).
fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Default, Debug)]
pub(crate) struct Counters {
    pub from_vm_frames: AtomicU64,
    pub from_vm_bytes: AtomicU64,
    pub to_vm_frames: AtomicU64,
    pub to_vm_bytes: AtomicU64,
    pub dropped: AtomicU64,
    pub spoofed: AtomicU64,
    pub unsupported: AtomicU64,
    pub fw_blocked: AtomicU64,
    pub tcp_active: AtomicU64,
    pub udp_active: AtomicU64,
    pub tcp_total: AtomicU64,
    pub udp_total: AtomicU64,
    pub limit_hits: AtomicU64,
    pub dns_queries: AtomicU64,
    pub ip_configured: AtomicBool,
}

impl Counters {
    pub fn active(&self) -> u64 {
        self.tcp_active.load(Ordering::Relaxed) + self.udp_active.load(Ordering::Relaxed)
    }
    pub fn inc(c: &AtomicU64) {
        c.fetch_add(1, Ordering::Relaxed);
    }
    pub fn dec(c: &AtomicU64) {
        // не уходим ниже нуля; compare_exchange вместо fetch_update/try_update,
        // чтобы собиралось и на старых, и на новых версиях Rust
        let mut cur = c.load(Ordering::Relaxed);
        while let Some(next) = cur.checked_sub(1) {
            match c.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }
}

/// Порт свитча (одна машина).
pub(crate) struct Port {
    pub id: PortId,
    pub vm_id: String,
    pub hostname: String,
    pub mac: Mac,
    pub ip: Ipv4Addr,
    pub c: Arc<Counters>,
    tx: mpsc::Sender<Bytes>,
    closed: Notify,
    dns_inflight: Arc<Semaphore>,
}

impl Port {
    /// Поставить кадр в очередь к машине; при переполнении кадр теряется.
    pub fn send(&self, frame: Bytes) -> bool {
        match self.tx.try_send(frame) {
            Ok(()) => true,
            Err(_) => {
                Counters::inc(&self.c.dropped);
                false
            }
        }
    }

    /// Отправить IPv4-пакет машине от имени шлюза.
    pub fn send_ip(&self, packet: &[u8]) -> bool {
        self.send(Bytes::from(wire::build_eth(self.mac, GATEWAY_MAC, wire::ETHERTYPE_IPV4, packet)))
    }
}

/// Статистика порта. «from_vm» — от машины в свитч, «to_vm» — из свитча в машину.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortStats {
    pub connected: bool,
    pub mac: [u8; 6],
    pub ip: Option<Ipv4Addr>,
    pub hostname: String,
    /// У машины есть адрес: выдан по DHCP или с него уже идёт трафик
    /// (после перезапуска демона гость живёт со старой арендой и DHCP не зовёт).
    pub ip_configured: bool,
    pub from_vm_frames: u64,
    pub from_vm_bytes: u64,
    pub to_vm_frames: u64,
    pub to_vm_bytes: u64,
    /// Кадры к машине, потерянные из-за переполнения очереди, и битые кадры от неё.
    pub dropped_frames: u64,
    /// Кадры от машины, отброшенные защитой порта (чужой MAC/IP, ARP от чужого
    /// имени, ответы DHCP-сервера).
    pub spoofed_frames: u64,
    /// Кадры неподдерживаемых протоколов от машины (IPv6, VLAN и т.п.), отброшены.
    pub unsupported_frames: u64,
    /// Соединения/датаграммы наружу, отклонённые файрволом.
    pub firewall_blocked: u64,
    /// Отказы из-за `max_connections_per_vm`.
    pub connection_limit_hits: u64,
    pub tcp_connections: u64,
    pub udp_flows: u64,
    pub tcp_connections_total: u64,
    pub udp_flows_total: u64,
    pub dns_queries: u64,
}

struct KnownVm {
    mac: Mac,
    hostname: String,
    c: Arc<Counters>,
}

#[derive(Default)]
struct Tables {
    ports: HashMap<PortId, Arc<Port>>,
    by_vm: HashMap<String, PortId>,
    /// Таблица MAC: адрес -> (порт, когда видели).
    by_mac: HashMap<Mac, (PortId, Instant)>,
    by_ip: HashMap<Ipv4Addr, PortId>,
    by_name: HashMap<String, PortId>,
    /// Закреплённые адреса: MAC -> IP (живут, пока жив свитч).
    alloc: HashMap<Mac, Ipv4Addr>,
    alloc_rev: HashMap<Ipv4Addr, Mac>,
    known: HashMap<String, KnownVm>,
}

type DnsCache = HashMap<String, (Instant, Result<Vec<Ipv4Addr>, u8>)>;
type NameMap = HashMap<(PortId, Ipv4Addr), Vec<(Arc<str>, Instant)>>;

/// Свитч с шлюзом. Создаётся внутри рантайма tokio.
pub struct NetSwitch {
    pub(crate) cfg: NetConfig,
    zone: String,
    pub(crate) gw_ip: Ipv4Addr,
    netmask: Ipv4Addr,
    bcast: Ipv4Addr,
    pub(crate) fw: Firewall,
    t: RwLock<Tables>,
    next_port: AtomicU64,
    tcp_tx: mpsc::Sender<nat_tcp::Msg>,
    pub(crate) tcp_ev: mpsc::UnboundedSender<nat_tcp::Ev>,
    pub(crate) udp: nat_udp::UdpNat,
    dns_cache: Mutex<DnsCache>,
    /// Какое имя машина последним разрешила в этот адрес (для CONNECT по имени).
    /// Какие имена DNS шлюза недавно отдал машине для адреса (последнее — в конце).
    names: Mutex<NameMap>,
    weak: Weak<NetSwitch>,
}

/// Нормализовать имя для DNS: [a-z0-9-], до 63 символов.
fn sanitize_label(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    out = out.trim_matches('-').to_string();
    out.truncate(63);
    out.trim_end_matches('-').to_string()
}

impl NetSwitch {
    /// Создать свитч и запустить фоновую задачу NAT. Нужен рантайм tokio.
    pub fn new(cfg: NetConfig) -> io::Result<Arc<Self>> {
        let bad = |m: String| io::Error::new(io::ErrorKind::InvalidInput, m);
        let subnet = cfg.subnet.trunc();
        if !(8..=29).contains(&subnet.prefix_len()) {
            return Err(bad(format!("subnet {subnet}: нужна длина префикса 8..29")));
        }
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| io::Error::other("NetSwitch::new: нужен рантайм tokio"))?;
        let proxy =
            cfg.upstream_https_proxy.as_deref().filter(|s| !s.trim().is_empty()).map(HttpProxy::parse).transpose()?;
        let zone = cfg
            .dns_zone
            .trim_matches('.')
            .split('.')
            .map(sanitize_label)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(".");
        let gw_ip = Ipv4Addr::from(u32::from(subnet.network()) + 1);
        let mut cfg = cfg;
        cfg.subnet = subnet;
        let (tcp_tx, tcp_rx) = mpsc::channel(4096);
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let sw = Arc::new_cyclic(|weak| NetSwitch {
            fw: Firewall::new(cfg.firewall.clone(), subnet),
            zone,
            gw_ip,
            netmask: subnet.netmask(),
            bcast: subnet.broadcast(),
            t: RwLock::new(Tables::default()),
            next_port: AtomicU64::new(1),
            tcp_tx,
            tcp_ev: ev_tx.clone(),
            udp: nat_udp::UdpNat::default(),
            dns_cache: Mutex::new(HashMap::new()),
            names: Mutex::new(HashMap::new()),
            weak: weak.clone(),
            cfg,
        });
        let nat = nat_tcp::Nat::new(
            Arc::downgrade(&sw),
            gw_ip,
            subnet.prefix_len(),
            proxy.map(Arc::new),
            sw.cfg.upstream_proxy_ports.clone(),
            sw.cfg.max_connections_per_vm,
            ev_tx,
        );
        handle.spawn(nat_tcp::run(nat, tcp_rx, ev_rx));
        Ok(sw)
    }

    pub(crate) fn weak_self(&self) -> Weak<NetSwitch> {
        self.weak.clone()
    }

    /// Адрес шлюза (он же DNS).
    pub fn gateway(&self) -> Ipv4Addr {
        self.gw_ip
    }

    /// Зона DNS после нормализации.
    pub fn dns_zone(&self) -> &str {
        &self.zone
    }

    /// Детерминированный MAC машины: 52:54:00:xx:xx:xx из хеша vm_id.
    pub fn mac_for_vm(vm_id: &str) -> [u8; 6] {
        let h = fnv1a(vm_id.as_bytes()).to_be_bytes();
        [0x52, 0x54, 0x00, h[0], h[1], h[2]]
    }

    /// Адрес машины (закрепляется при первом подключении порта и живёт,
    /// пока жив свитч).
    pub fn ip_of(&self, vm_id: &str) -> Option<Ipv4Addr> {
        let t = self.t.read();
        let k = t.known.get(vm_id)?;
        t.alloc.get(&k.mac).copied()
    }

    /// Статистика машины (и отключённой тоже, если порт когда-то был).
    pub fn stats(&self, vm_id: &str) -> Option<PortStats> {
        let t = self.t.read();
        let k = t.known.get(vm_id)?;
        let c = &k.c;
        let l = |a: &AtomicU64| a.load(Ordering::Relaxed);
        Some(PortStats {
            connected: t.by_vm.contains_key(vm_id),
            mac: k.mac,
            ip: t.alloc.get(&k.mac).copied(),
            hostname: k.hostname.clone(),
            ip_configured: c.ip_configured.load(Ordering::Relaxed),
            from_vm_frames: l(&c.from_vm_frames),
            from_vm_bytes: l(&c.from_vm_bytes),
            to_vm_frames: l(&c.to_vm_frames),
            to_vm_bytes: l(&c.to_vm_bytes),
            dropped_frames: l(&c.dropped),
            spoofed_frames: l(&c.spoofed),
            unsupported_frames: l(&c.unsupported),
            firewall_blocked: l(&c.fw_blocked),
            connection_limit_hits: l(&c.limit_hits),
            tcp_connections: l(&c.tcp_active),
            udp_flows: l(&c.udp_active),
            tcp_connections_total: l(&c.tcp_total),
            udp_flows_total: l(&c.udp_total),
            dns_queries: l(&c.dns_queries),
        })
    }

    /// Разорвать порт машины (run_port вернёт Ok). QEMU с `reconnect=` переподключится.
    pub fn disconnect(&self, vm_id: &str) {
        let t = self.t.read();
        if let Some(p) = t.by_vm.get(vm_id).and_then(|id| t.ports.get(id)) {
            p.closed.notify_one();
        }
    }

    /// Подключить порт машины и обслуживать его, пока поток не закроется.
    ///
    /// `stream` — соединение от QEMU `-netdev stream,...,server=off` (кадры:
    /// u32 BE длина + Ethernet-кадр). `mac` должен совпадать с MAC устройства
    /// virtio-net в QEMU: с защитой портов кадры с другим MAC отбрасываются.
    /// Новое подключение того же `vm_id` вытесняет старое.
    pub async fn run_port<S>(&self, vm_id: &str, hostname: &str, mac: [u8; 6], stream: S) -> io::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (port, rx) = self.register(vm_id, hostname, mac)?;
        let _guard = PortGuard { sw: self, port: port.clone() };
        tracing::info!(vm = vm_id, ip = %port.ip, mac = %wire::mac_to_string(&mac), host = %port.hostname, "fcw-net: порт подключён");
        let (r, w) = tokio::io::split(stream);
        let rate = self.cfg.rate_limit_bytes_per_sec;
        let res = tokio::select! {
            r = self.port_reader(&port, r) => r,
            r = port_writer(rate, rx, w, &port.c) => r,
            _ = port.closed.notified() => Ok(()),
        };
        tracing::info!(vm = vm_id, "fcw-net: порт отключён: {:?}", res.as_ref().err());
        res
    }

    fn register(&self, vm_id: &str, hostname: &str, mac: Mac) -> io::Result<(Arc<Port>, mpsc::Receiver<Bytes>)> {
        if wire::is_multicast_mac(&mac) || mac == [0; 6] {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "плохой MAC"));
        }
        let mut name = sanitize_label(hostname);
        if name.is_empty() || name == "gateway" {
            name = format!("vm-{:06x}", fnv1a(vm_id.as_bytes()) & 0xff_ffff);
        }
        let mut t = self.t.write();
        // Старое подключение той же машины вытесняется.
        if let Some(old) = t.by_vm.get(vm_id).and_then(|id| t.ports.get(id)).cloned() {
            old.closed.notify_one();
            Self::unregister_locked(&mut t, &old);
            self.port_gone(old.id);
        }
        if let Some((pid, _)) = t.by_mac.get(&mac) {
            if t.ports.get(pid).is_some_and(|p| p.mac == mac) {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, "MAC уже занят другой машиной"));
            }
        }
        let ip = self.alloc_ip(&mut t, mac)?;
        let c = match t.known.get_mut(vm_id) {
            Some(k) => {
                k.mac = mac;
                k.hostname = name.clone();
                k.c.ip_configured.store(false, Ordering::Relaxed);
                k.c.clone()
            }
            None => {
                let c = Arc::new(Counters::default());
                t.known.insert(vm_id.to_string(), KnownVm { mac, hostname: name.clone(), c: c.clone() });
                c
            }
        };
        let (tx, rx) = mpsc::channel(PORT_QUEUE);
        let id = self.next_port.fetch_add(1, Ordering::Relaxed);
        let port = Arc::new(Port {
            id,
            vm_id: vm_id.to_string(),
            hostname: name.clone(),
            mac,
            ip,
            c,
            tx,
            closed: Notify::new(),
            dns_inflight: Arc::new(Semaphore::new(DNS_INFLIGHT_PER_PORT)),
        });
        t.ports.insert(id, port.clone());
        t.by_vm.insert(vm_id.to_string(), id);
        t.by_mac.insert(mac, (id, Instant::now()));
        t.by_ip.insert(ip, id);
        if let Some(prev) = t.by_name.insert(name.clone(), id) {
            if prev != id {
                tracing::warn!(vm = vm_id, "fcw-net: имя {name} было у другой машины, теперь у этой");
            }
        }
        Ok((port, rx))
    }

    fn unregister_locked(t: &mut Tables, port: &Port) {
        if t.ports.get(&port.id).is_some_and(|p| std::ptr::eq(p.as_ref(), port)) {
            t.ports.remove(&port.id);
        }
        if t.by_vm.get(&port.vm_id) == Some(&port.id) {
            t.by_vm.remove(&port.vm_id);
        }
        t.by_mac.retain(|_, (id, _)| *id != port.id);
        if t.by_ip.get(&port.ip) == Some(&port.id) {
            t.by_ip.remove(&port.ip);
        }
        if t.by_name.get(&port.hostname) == Some(&port.id) {
            t.by_name.remove(&port.hostname);
        }
    }

    /// Убрать NAT-состояние порта.
    fn port_gone(&self, id: PortId) {
        self.names.lock().retain(|(p, _), _| *p != id);
        let _ = self.tcp_ev.send(nat_tcp::Ev::PortClosed { port: id });
        self.udp.remove_port(id);
    }

    fn alloc_ip(&self, t: &mut Tables, mac: Mac) -> io::Result<Ipv4Addr> {
        if let Some(ip) = t.alloc.get(&mac) {
            return Ok(*ip);
        }
        let subnet = self.cfg.subnet;
        let net = u32::from(subnet.network());
        let last = u32::from(subnet.broadcast()) - 1;
        // Первая /24 большой подсети — под шлюз и служебные адреса.
        let first = if subnet.prefix_len() <= 23 { net + 256 } else { net + 2 };
        let count = last - first + 1;
        let start = (fnv1a(&mac) % u64::from(count)) as u32;
        for i in 0..count {
            let ip = Ipv4Addr::from(first + (start + i) % count);
            let o = ip.octets()[3];
            if (count > 1024 && (o == 0 || o == 255)) || ip == self.gw_ip || t.alloc_rev.contains_key(&ip) {
                continue;
            }
            t.alloc.insert(mac, ip);
            t.alloc_rev.insert(ip, mac);
            return Ok(ip);
        }
        Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "в подсети кончились адреса"))
    }

    pub(crate) fn port_by_ip(&self, ip: Ipv4Addr) -> Option<Arc<Port>> {
        let t = self.t.read();
        t.by_ip.get(&ip).and_then(|id| t.ports.get(id)).cloned()
    }

    // ------------------------------------------------------------ порт: чтение

    async fn port_reader<R: AsyncRead + Unpin>(&self, port: &Arc<Port>, r: R) -> io::Result<()> {
        let mut bucket = TokenBucket::new(self.cfg.rate_limit_bytes_per_sec);
        let mut r = BufReader::with_capacity(64 * 1024, r);
        let mut lenb = [0u8; 4];
        loop {
            match r.read_exact(&mut lenb).await {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(e),
            }
            let len = u32::from_be_bytes(lenb) as usize;
            if len > MAX_STREAM_LEN {
                return Err(io::Error::new(io::ErrorKind::InvalidData, format!("кадр {len} байт")));
            }
            let mut buf = vec![0u8; len];
            r.read_exact(&mut buf).await?;
            if let Some(b) = bucket.as_mut() {
                b.wait(len).await;
            }
            self.ingress(port, Bytes::from(buf));
        }
    }

    /// Кадр от машины.
    pub(crate) fn ingress(&self, port: &Arc<Port>, frame: Bytes) {
        Counters::inc(&port.c.from_vm_frames);
        port.c.from_vm_bytes.fetch_add(frame.len() as u64, Ordering::Relaxed);
        let Some(eth) = wire::parse_eth(&frame).filter(|_| frame.len() <= MAX_FRAME) else {
            Counters::inc(&port.c.dropped);
            return;
        };
        if self.cfg.port_security {
            if eth.ethertype != wire::ETHERTYPE_ARP && eth.ethertype != wire::ETHERTYPE_IPV4 {
                // IPv6, VLAN, LLDP...: в этой сети не нужны (и закрывают, например,
                // поддельные RA с чужим DNS). Обычное дело, не подделка.
                Counters::inc(&port.c.unsupported);
                return;
            }
            if !self.port_security_ok(port, &eth) {
                if port.c.spoofed.fetch_add(1, Ordering::Relaxed) == 0 {
                    tracing::warn!(
                        vm = %port.vm_id,
                        "fcw-net: кадр отброшен защитой порта (src {}, type {:04x}); дальше молча",
                        wire::mac_to_string(&eth.src),
                        eth.ethertype
                    );
                }
                return;
            }
        } else if wire::is_multicast_mac(&eth.src) || !self.learn(port, eth.src) {
            Counters::inc(&port.c.spoofed);
            return;
        }

        if eth.dst == GATEWAY_MAC {
            self.gateway_rx(port, &eth);
            return;
        }
        if wire::is_multicast_mac(&eth.dst) {
            self.flood(port.id, &frame);
            if eth.dst == wire::BROADCAST_MAC {
                self.gateway_rx(port, &eth);
            }
            return;
        }
        let target = {
            let t = self.t.read();
            t.by_mac.get(&eth.dst).and_then(|(id, _)| t.ports.get(id)).cloned()
        };
        match target {
            Some(p) if p.id != port.id => {
                p.send(frame);
            }
            Some(_) => {}
            // Неизвестный адрес: с защитой портов все MAC известны — значит, такого нет.
            None if !self.cfg.port_security => self.flood(port.id, &frame),
            None => {}
        }
    }

    fn learn(&self, port: &Port, mac: Mac) -> bool {
        let now = Instant::now();
        {
            let t = self.t.read();
            if let Some((id, seen)) = t.by_mac.get(&mac) {
                if *id == port.id && now.duration_since(*seen) < Duration::from_secs(10) {
                    return true;
                }
            }
        }
        let mut g = self.t.write();
        let t = &mut *g;
        let ports = &t.ports;
        // Устаревшие выученные адреса забываем; собственные MAC портов — никогда.
        t.by_mac
            .retain(|m, (id, seen)| ports.get(id).is_some_and(|p| p.mac == *m || now.duration_since(*seen) < MAC_AGE));
        let mine = t.by_mac.values().filter(|(id, _)| *id == port.id).count();
        match t.by_mac.get(&mac) {
            Some((id, _)) if *id == port.id => {}
            // Адрес живого порта (его собственный MAC) другой порт присвоить не может.
            Some((id, _)) if t.ports.get(id).is_some_and(|p| p.mac == mac) => return false,
            _ if mine >= LEARNED_MACS_PER_PORT => return false,
            _ => {}
        }
        t.by_mac.insert(mac, (port.id, now));
        true
    }

    fn port_security_ok(&self, port: &Port, eth: &wire::Eth<'_>) -> bool {
        if eth.src != port.mac {
            return false;
        }
        match eth.ethertype {
            wire::ETHERTYPE_ARP => match wire::parse_arp(eth.payload) {
                Some(a) => a.sha == port.mac && (a.spa == port.ip || a.spa.is_unspecified()),
                None => false,
            },
            wire::ETHERTYPE_IPV4 => {
                let Some(ip) = wire::parse_ipv4(eth.payload) else { return false };
                let udp_ports = || {
                    if ip.proto == wire::PROTO_UDP && !ip.is_fragment() && ip.payload.len() >= 4 {
                        let p = ip.payload;
                        Some((u16::from_be_bytes([p[0], p[1]]), u16::from_be_bytes([p[2], p[3]])))
                    } else {
                        None
                    }
                };
                if ip.src == port.ip {
                    if !port.c.ip_configured.load(Ordering::Relaxed) {
                        port.c.ip_configured.store(true, Ordering::Relaxed);
                    }
                    // Гость не может быть DHCP-сервером для других.
                    !matches!(udp_ports(), Some((dhcp::SERVER_PORT, _)))
                } else {
                    ip.src.is_unspecified() && matches!(udp_ports(), Some((dhcp::CLIENT_PORT, dhcp::SERVER_PORT)))
                }
            }
            _ => false,
        }
    }

    fn flood(&self, from: PortId, frame: &Bytes) {
        let t = self.t.read();
        for p in t.ports.values() {
            if p.id != from {
                p.send(frame.clone());
            }
        }
    }

    // ------------------------------------------------------------ шлюз

    fn gateway_rx(&self, port: &Arc<Port>, eth: &wire::Eth<'_>) {
        match eth.ethertype {
            wire::ETHERTYPE_ARP => {
                let Some(a) = wire::parse_arp(eth.payload) else { return };
                if a.op == wire::ARP_REQUEST && a.tpa == self.gw_ip {
                    let reply =
                        wire::Arp { op: wire::ARP_REPLY, sha: GATEWAY_MAC, spa: self.gw_ip, tha: a.sha, tpa: a.spa };
                    port.send(Bytes::from(wire::build_eth(
                        eth.src,
                        GATEWAY_MAC,
                        wire::ETHERTYPE_ARP,
                        &wire::build_arp(&reply),
                    )));
                }
            }
            wire::ETHERTYPE_IPV4 => {
                let Some(ip) = wire::parse_ipv4(eth.payload) else { return };
                if ip.dst == self.gw_ip || ip.dst.is_broadcast() || ip.dst == self.bcast {
                    self.gateway_local(port, &ip);
                } else if eth.dst == GATEWAY_MAC {
                    self.route_out(port, &ip);
                }
            }
            _ => {}
        }
    }

    /// Пакет на адрес шлюза (или широковещательный).
    fn gateway_local(&self, port: &Arc<Port>, ip: &wire::Ipv4<'_>) {
        let to_gw = ip.dst == self.gw_ip;
        match ip.proto {
            wire::PROTO_ICMP if to_gw => {
                if let Some(r) = wire::build_icmp_echo_reply(ip, next_ip_id()) {
                    port.send_ip(&r);
                }
            }
            wire::PROTO_UDP => {
                let Some(u) = wire::parse_udp(ip) else { return };
                match u.dport {
                    dhcp::SERVER_PORT if u.sport == dhcp::CLIENT_PORT => self.dhcp(port, u.payload),
                    dns::PORT if to_gw => self.dns(port, ip.src, u.sport, u.payload),
                    _ => {}
                }
            }
            wire::PROTO_TCP if to_gw => {
                // Других служб на шлюзе нет (DNS по TCP тоже не поддерживаем).
                if let Some(t) = wire::parse_tcp(ip) {
                    if t.is_syn_only() {
                        Counters::inc(&port.c.fw_blocked);
                    }
                    if let Some(r) = wire::build_tcp_rst_reply(ip, &t, next_ip_id()) {
                        port.send_ip(&r);
                    }
                }
            }
            _ => {}
        }
    }

    fn dhcp(&self, port: &Arc<Port>, payload: &[u8]) {
        let Some(req) = dhcp::parse(payload) else { return };
        if req.chaddr != port.mac {
            return;
        }
        let lease = dhcp::Lease {
            ip: port.ip,
            server: self.gw_ip,
            netmask: self.netmask,
            broadcast: self.bcast,
            router: self.gw_ip,
            dns: self.gw_ip,
            domain: &self.zone,
            hostname: &port.hostname,
            lease_secs: LEASE_SECS,
            mtu: MTU as u16,
        };
        let Some(reply) = dhcp::respond(&req, &lease) else { return };
        tracing::debug!(vm = %port.vm_id, "fcw-net: DHCP {} -> {}", req.msg_type, reply.msg_type);
        if reply.msg_type == dhcp::ACK
            && req.msg_type != dhcp::INFORM
            && !port.c.ip_configured.swap(true, Ordering::Relaxed)
        {
            tracing::info!(vm = %port.vm_id, ip = %port.ip, "fcw-net: адрес выдан по DHCP");
        }
        let udp = wire::build_udp(self.gw_ip, dhcp::SERVER_PORT, reply.dst_ip, dhcp::CLIENT_PORT, &reply.payload);
        port.send_ip(&wire::build_ipv4(self.gw_ip, reply.dst_ip, wire::PROTO_UDP, next_ip_id(), &udp));
    }

    fn machine_ip(&self, label: &str) -> Option<Ipv4Addr> {
        let t = self.t.read();
        t.by_name.get(label).and_then(|id| t.ports.get(id)).map(|p| p.ip)
    }

    fn machine_name(&self, ip: Ipv4Addr) -> Option<String> {
        let t = self.t.read();
        t.by_ip.get(&ip).and_then(|id| t.ports.get(id)).map(|p| p.hostname.clone())
    }

    fn reply_udp(&self, port: &Port, sport: u16, dst: Ipv4Addr, dport: u16, payload: &[u8]) {
        let udp = wire::build_udp(self.gw_ip, sport, dst, dport, payload);
        for f in wire::build_ipv4_fragments(self.gw_ip, dst, wire::PROTO_UDP, next_ip_id(), &udp, MTU) {
            port.send_ip(&f);
        }
    }

    fn dns(&self, port: &Arc<Port>, src: Ipv4Addr, sport: u16, payload: &[u8]) {
        Counters::inc(&port.c.dns_queries);
        let q = match dns::parse_query(payload) {
            Ok(q) => q,
            Err(dns::ParseError::Ignore) => return,
            Err(dns::ParseError::Error { id, opcode, rcode }) => {
                self.reply_udp(port, dns::PORT, src, sport, &dns::build_error(id, opcode, rcode));
                return;
            }
        };
        let decision = dns::decide(
            &q,
            &self.zone,
            self.cfg.subnet,
            self.gw_ip,
            |l| self.machine_ip(l),
            |ip| self.machine_name(ip),
        );
        match decision {
            Decision::Answer { rcode, aa, answers } => {
                self.reply_udp(port, dns::PORT, src, sport, &dns::build_response(&q, rcode, aa, &answers));
            }
            Decision::Forward(name) => {
                if let Some(res) = self.dns_cache_get(&name) {
                    self.remember_names(port.id, &name, &res);
                    self.reply_udp(port, dns::PORT, src, sport, &forward_response(&q, &res));
                    return;
                }
                let Ok(permit) = port.dns_inflight.clone().try_acquire_owned() else {
                    return; // гость повторит запрос
                };
                let raw = payload.to_vec();
                let weak = self.weak.clone();
                let port = port.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let res = resolve_a(&name).await;
                    let Some(sw) = weak.upgrade() else { return };
                    sw.remember_names(port.id, &name, &res);
                    sw.dns_cache_put(name, res.clone());
                    if let Ok(q) = dns::parse_query(&raw) {
                        sw.reply_udp(&port, dns::PORT, src, sport, &forward_response(&q, &res));
                    }
                });
            }
        }
    }

    fn remember_names(&self, port: PortId, name: &str, res: &Result<Vec<Ipv4Addr>, u8>) {
        let Ok(ips) = res else { return };
        if ips.is_empty() {
            return;
        }
        let name: Arc<str> = Arc::from(name);
        let now = Instant::now();
        let mut m = self.names.lock();
        if m.len() > 16 * 1024 {
            m.retain(|_, v| v.last().is_some_and(|(_, t)| now.duration_since(*t) < NAME_TTL));
        }
        for ip in ips {
            let v = m.entry((port, *ip)).or_default();
            v.retain(|(n, _)| **n != *name);
            if v.len() >= NAMES_PER_IP {
                v.remove(0);
            }
            v.push((name.clone(), now));
        }
    }

    #[cfg(test)]
    pub(crate) fn remember_for_vm(&self, vm_id: &str, name: &str, ip: Ipv4Addr) {
        let id = *self.t.read().by_vm.get(vm_id).unwrap();
        self.remember_names(id, name, &Ok(vec![ip]));
    }

    /// Последнее имя, которым машина недавно получила адрес `ip`.
    pub(crate) fn name_for(&self, port: PortId, ip: Ipv4Addr) -> Option<Arc<str>> {
        let m = self.names.lock();
        m.get(&(port, ip))?.last().filter(|(_, t)| t.elapsed() < NAME_TTL).map(|(n, _)| n.clone())
    }

    /// Отдавал ли DNS шлюза машине адрес `ip` для имени `name` (недавно).
    pub(crate) fn name_resolved_to(&self, port: PortId, name: &str, ip: Ipv4Addr) -> bool {
        let m = self.names.lock();
        m.get(&(port, ip)).is_some_and(|v| v.iter().any(|(n, t)| **n == *name && t.elapsed() < NAME_TTL))
    }

    fn dns_cache_get(&self, name: &str) -> Option<Result<Vec<Ipv4Addr>, u8>> {
        let c = self.dns_cache.lock();
        c.get(name).filter(|(exp, _)| *exp > Instant::now()).map(|(_, r)| r.clone())
    }

    fn dns_cache_put(&self, name: String, res: Result<Vec<Ipv4Addr>, u8>) {
        let ttl = match &res {
            Ok(v) if !v.is_empty() => Duration::from_secs(u64::from(DNS_FORWARD_TTL)),
            Err(dns::SERVFAIL) => return,
            _ => Duration::from_secs(10),
        };
        let mut c = self.dns_cache.lock();
        if c.len() > 4096 {
            let now = Instant::now();
            c.retain(|_, (exp, _)| *exp > now);
            if c.len() > 4096 {
                c.clear();
            }
        }
        c.insert(name, (Instant::now() + ttl, res));
    }

    // ------------------------------------------------------------ наружу

    /// Пакет, который машина отправила через шлюз (не на его адрес).
    fn route_out(&self, port: &Arc<Port>, ip: &wire::Ipv4<'_>) {
        if ip.src != port.ip || ip.is_fragment() {
            return;
        }
        match ip.proto {
            wire::PROTO_TCP => {
                let Some(t) = wire::parse_tcp(ip) else { return };
                if t.is_syn_only() || !self.cfg.allow_internet {
                    let v = if self.cfg.allow_internet {
                        self.fw.check(ip.dst, t.dport)
                    } else {
                        Verdict::Deny("no-internet")
                    };
                    if let Verdict::Deny(why) = v {
                        if t.is_syn_only() {
                            Counters::inc(&port.c.fw_blocked);
                            tracing::debug!(vm = %port.vm_id, "fcw-net: TCP {}:{} запрещён ({why})", ip.dst, t.dport);
                        }
                        if let Some(r) = wire::build_tcp_rst_reply(ip, &t, next_ip_id()) {
                            port.send_ip(&r);
                        }
                        return;
                    }
                }
                let msg = nat_tcp::Msg::Packet { port: port.clone(), pkt: Bytes::copy_from_slice(ip.packet) };
                if self.tcp_tx.try_send(msg).is_err() {
                    Counters::inc(&port.c.dropped);
                }
            }
            wire::PROTO_UDP => {
                if !self.cfg.allow_internet {
                    return;
                }
                let Some(u) = wire::parse_udp(ip) else { return };
                self.udp.packet(self, port, u.sport, SocketAddrV4::new(ip.dst, u.dport), u.payload);
            }
            // ICMP и прочее наружу не ходит.
            _ => {}
        }
    }
}

const DNS_FORWARD_TTL: u32 = 60;
const NAME_TTL: Duration = Duration::from_secs(3600);
const NAMES_PER_IP: usize = 16;

fn forward_response(q: &dns::Query<'_>, res: &Result<Vec<Ipv4Addr>, u8>) -> Vec<u8> {
    match res {
        Ok(ips) => {
            let answers: Vec<dns::Answer> =
                ips.iter().map(|ip| dns::Answer { ttl: DNS_FORWARD_TTL, data: dns::RData::A(*ip) }).collect();
            dns::build_response(q, dns::NOERROR, false, &answers)
        }
        Err(rcode) => dns::build_response(q, *rcode, false, &[]),
    }
}

/// A-записи имени через резолвер хоста (getaddrinfo).
async fn resolve_a(name: &str) -> Result<Vec<Ipv4Addr>, u8> {
    match tokio::time::timeout(Duration::from_secs(8), tokio::net::lookup_host((name, 0))).await {
        Err(_) => Err(dns::SERVFAIL),
        Ok(Err(e)) => {
            let m = e.to_string().to_ascii_lowercase();
            if m.contains("temporary") || m.contains("try again") {
                Err(dns::SERVFAIL)
            } else {
                Err(dns::NXDOMAIN)
            }
        }
        Ok(Ok(it)) => {
            let mut v: Vec<Ipv4Addr> = Vec::new();
            for a in it {
                if let std::net::SocketAddr::V4(a) = a {
                    if !v.contains(a.ip()) {
                        v.push(*a.ip());
                    }
                }
            }
            Ok(v)
        }
    }
}

struct PortGuard<'a> {
    sw: &'a NetSwitch,
    port: Arc<Port>,
}

impl Drop for PortGuard<'_> {
    fn drop(&mut self) {
        {
            let mut t = self.sw.t.write();
            NetSwitch::unregister_locked(&mut t, &self.port);
        }
        self.sw.port_gone(self.port.id);
    }
}

async fn port_writer<W: AsyncWrite + Unpin>(
    rate: u64,
    mut rx: mpsc::Receiver<Bytes>,
    w: W,
    c: &Counters,
) -> io::Result<()> {
    let mut bucket = TokenBucket::new(rate);
    let mut w = BufWriter::with_capacity(64 * 1024, w);
    while let Some(f) = rx.recv().await {
        if let Some(b) = bucket.as_mut() {
            let d = b.take(f.len(), tokio::time::Instant::now());
            if !d.is_zero() {
                w.flush().await?;
                tokio::time::sleep(d).await;
            }
        }
        w.write_all(&(f.len() as u32).to_be_bytes()).await?;
        w.write_all(&f).await?;
        Counters::inc(&c.to_vm_frames);
        c.to_vm_bytes.fetch_add(f.len() as u64, Ordering::Relaxed);
        if rx.is_empty() {
            w.flush().await?;
        }
    }
    w.flush().await?;
    Ok(())
}

// ------------------------------------------------------------ Unix-сокет

/// Слушатель Unix-сокета порта: принимает подключения QEMU и обслуживает их.
/// При drop перестаёт принимать и удаляет файл сокета (текущее подключение
/// живёт, пока QEMU его не закроет; см. [`NetSwitch::disconnect`]).
#[cfg(unix)]
pub struct PortListener {
    task: tokio::task::JoinHandle<()>,
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl PortListener {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for PortListener {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
impl NetSwitch {
    /// Сразу создать Unix-сокет `path` (старый файл удаляется) и принимать на
    /// нём подключения QEMU для машины `vm_id`. Удобно звать до запуска QEMU.
    pub fn listen_unix(
        self: &Arc<Self>,
        path: impl AsRef<std::path::Path>,
        vm_id: &str,
        hostname: &str,
        mac: [u8; 6],
    ) -> io::Result<PortListener> {
        let path = path.as_ref().to_path_buf();
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path)?;
        let sw = self.clone();
        let (vm, host) = (vm_id.to_string(), hostname.to_string());
        let task = tokio::spawn(async move {
            loop {
                let stream = match listener.accept().await {
                    Ok((s, _)) => s,
                    Err(e) => {
                        tracing::warn!(vm = %vm, "fcw-net: accept: {e}");
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                };
                let (sw, vm, host) = (sw.clone(), vm.clone(), host.clone());
                tokio::spawn(async move {
                    if let Err(e) = sw.run_port(&vm, &host, mac, stream).await {
                        tracing::warn!(vm = %vm, "fcw-net: порт: {e}");
                    }
                });
            }
        });
        Ok(PortListener { task, path })
    }
}
