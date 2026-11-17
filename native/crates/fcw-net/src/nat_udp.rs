//! UDP NAT: поток на (порт машины, порт источника, назначение), у каждого свой
//! подключённый сокет хоста; поток закрывается после простоя.

use std::collections::HashMap;
use std::net::{SocketAddrV4, UdpSocket as StdUdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

use crate::firewall::Verdict;
use crate::switch::{next_ip_id, Counters, NetSwitch, Port, PortId, MTU};
use crate::wire;

/// Простой UDP-потока до закрытия; для DNS (порт 53) короче.
const IDLE: Duration = Duration::from_secs(60);
const IDLE_DNS: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    port: PortId,
    sport: u16,
    dst: SocketAddrV4,
}

struct Flow {
    /// Отправка — через std-сокет: у свежего tokio-сокета try_send вернёт
    /// WouldBlock, пока реактор не отметит готовность.
    send: StdUdpSocket,
    last: Arc<AtomicU64>,
    gen: u64,
    c: Arc<Counters>,
    task: JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct UdpNat {
    flows: Mutex<HashMap<Key, Flow>>,
    next_gen: AtomicU64,
}

fn now_ms(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

impl UdpNat {
    pub fn packet(&self, sw: &NetSwitch, port: &Arc<Port>, sport: u16, dst: SocketAddrV4, payload: &[u8]) {
        let key = Key { port: port.id, sport, dst };
        let mut flows = self.flows.lock();
        if let Some(f) = flows.get(&key) {
            f.last.store(now_ms(epoch()), Ordering::Relaxed);
            let _ = f.send.send(payload);
            return;
        }
        if let Verdict::Deny(why) = sw.fw.check(*dst.ip(), dst.port()) {
            Counters::inc(&port.c.fw_blocked);
            tracing::debug!(vm = %port.vm_id, "fcw-net: UDP {dst} запрещён ({why})");
            return;
        }
        if port.c.active() >= sw.cfg.max_connections_per_vm as u64 {
            Counters::inc(&port.c.limit_hits);
            return;
        }
        let (send, sock) = match open_socket(dst) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(vm = %port.vm_id, "fcw-net: UDP {dst}: {e}");
                return;
            }
        };
        let _ = send.send(payload);
        let gen = self.next_gen.fetch_add(1, Ordering::Relaxed);
        let last = Arc::new(AtomicU64::new(now_ms(epoch())));
        let idle = if dst.port() == 53 { IDLE_DNS } else { IDLE };
        let task = tokio::spawn(flow_task(
            sw.weak_self(),
            port.clone(),
            key,
            gen,
            sock,
            last.clone(),
            idle,
        ));
        Counters::inc(&port.c.udp_active);
        Counters::inc(&port.c.udp_total);
        flows.insert(key, Flow { send, last, gen, c: port.c.clone(), task });
    }

    /// Порт отключился: закрыть его потоки.
    pub fn remove_port(&self, port: PortId) {
        self.flows.lock().retain(|k, f| {
            if k.port == port {
                f.task.abort();
                Counters::dec(&f.c.udp_active);
                false
            } else {
                true
            }
        });
    }

    fn finish(&self, key: Key, gen: u64) {
        let mut flows = self.flows.lock();
        if flows.get(&key).is_some_and(|f| f.gen == gen) {
            if let Some(f) = flows.remove(&key) {
                Counters::dec(&f.c.udp_active);
            }
        }
    }
}

fn open_socket(dst: SocketAddrV4) -> std::io::Result<(StdUdpSocket, UdpSocket)> {
    let s = StdUdpSocket::bind("0.0.0.0:0")?;
    s.connect(dst)?;
    s.set_nonblocking(true)?;
    Ok((s.try_clone()?, UdpSocket::from_std(s)?))
}

async fn flow_task(
    sw: Weak<NetSwitch>,
    port: Arc<Port>,
    key: Key,
    gen: u64,
    sock: UdpSocket,
    last: Arc<AtomicU64>,
    idle: Duration,
) {
    let mut buf = vec![0u8; 65536];
    let mut errors = 0u32;
    loop {
        let since = Duration::from_millis(now_ms(epoch()).saturating_sub(last.load(Ordering::Relaxed)));
        if since >= idle {
            break;
        }
        match tokio::time::timeout(idle - since, sock.recv(&mut buf)).await {
            Err(_) => continue,
            Ok(Err(_)) => {
                // ECONNREFUSED от ICMP port unreachable и т.п.: не фатально, но без бесконечного цикла.
                errors += 1;
                if errors > 64 {
                    break;
                }
            }
            Ok(Ok(n)) => {
                last.store(now_ms(epoch()), Ordering::Relaxed);
                let udp = wire::build_udp(*key.dst.ip(), key.dst.port(), port.ip, key.sport, &buf[..n]);
                for f in wire::build_ipv4_fragments(*key.dst.ip(), port.ip, wire::PROTO_UDP, next_ip_id(), &udp, MTU) {
                    port.send_ip(&f);
                }
            }
        }
    }
    if let Some(sw) = sw.upgrade() {
        sw.udp.finish(key, gen);
    }
}
