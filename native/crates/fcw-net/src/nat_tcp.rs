//! TCP NAT: соединения гостя терминируются стеком smoltcp (any_ip, одна
//! «IP-сетевая карта» на все машины) и проксируются в сокеты хоста.
//!
//! Прямые соединения: SYN гостя придерживается, пока соединение с хоста не
//! установлено; отказ снаружи гость видит как RST на свой SYN («connection
//! refused»), а не как обрыв после установления.
//!
//! Через HTTP-прокси: рукопожатие с гостем завершается сразу, по первому
//! сегменту (TLS ClientHello) берётся SNI, и прокси получает `CONNECT имя:порт`
//! — многие прокси не пускают CONNECT по IP. Имя принимается, только если
//! DNS шлюза недавно отдал этой машине этот адрес для этого имени; иначе
//! берётся последнее такое имя из DNS, иначе IP.

use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, Hasher};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use bytes::Bytes;
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{self, Device, DeviceCapabilities, Medium};
use smoltcp::socket::tcp;
use smoltcp::wire::{HardwareAddress, IpCidr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;

use crate::proxy::{self, HttpProxy};
use crate::switch::{next_ip_id, Counters, NetSwitch, Port, PortId, MTU};
use crate::wire;

const SOCK_RX: usize = 64 * 1024;
const SOCK_TX: usize = 64 * 1024;
/// Кусок данных между задачами хоста и стеком.
const CHUNK: usize = 16 * 1024;
/// Сколько данных хоста может ждать в очереди к стеку на одно соединение.
const HOST_WINDOW: usize = 128 * 1024;
/// Очередь гость -> хост (в кусках).
const TO_HOST_CHUNKS: usize = 8;
/// Сколько ждать первый сегмент гостя, чтобы прочитать SNI.
const SNIFF_TIMEOUT: Duration = Duration::from_secs(2);

/// Пакет от машины (через ограниченную очередь).
pub(crate) enum Msg {
    Packet { port: Arc<Port>, pkt: Bytes },
}

/// События задач хоста и свитча.
pub(crate) enum Ev {
    Connected {
        id: u64,
        stream: TcpStream,
    },
    ConnectFailed {
        id: u64,
        err: std::io::Error,
    },
    Data {
        id: u64,
        data: Bytes,
    },
    Eof {
        id: u64,
    },
    HostError {
        id: u64,
    },
    /// Задача записи отправила кусок хосту: можно читать из сокета дальше.
    Drained,
    PortClosed {
        port: PortId,
    },
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    guest: SocketAddrV4,
    remote: SocketAddrV4,
}

/// Прямое соединение: SYN гостя ждёт, пока хост не соединится.
struct Pending {
    key: Key,
    port: Arc<Port>,
    syn: Bytes,
    task: JoinHandle<()>,
}

enum Phase {
    /// Через прокси: ждём ClientHello гостя, чтобы узнать имя.
    Sniffing {
        deadline: Instant,
    },
    /// Через прокси: соединяемся; данные гостя копятся в сокете.
    Connecting {
        task: JoinHandle<()>,
    },
    Open {
        reader: JoinHandle<()>,
        writer: JoinHandle<()>,
    },
}

struct Conn {
    handle: SocketHandle,
    key: Key,
    port: Arc<Port>,
    phase: Phase,
    to_guest: VecDeque<Bytes>,
    credits: Arc<Semaphore>,
    to_host: Option<mpsc::Sender<Bytes>>,
    host_eof: bool,
    fin_sent: bool,
}

/// Сетевая карта smoltcp: очереди IP-пакетов в памяти.
#[derive(Default)]
struct VDev {
    rx: VecDeque<Vec<u8>>,
    tx: Vec<Vec<u8>>,
}

struct RxTok(Vec<u8>);
struct TxTok<'a>(&'a mut Vec<Vec<u8>>);

impl phy::RxToken for RxTok {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}

impl phy::TxToken for TxTok<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut v = vec![0u8; len];
        let r = f(&mut v);
        self.0.push(v);
        r
    }
}

impl Device for VDev {
    type RxToken<'a> = RxTok;
    type TxToken<'a> = TxTok<'a>;

    fn receive(&mut self, _t: smoltcp::time::Instant) -> Option<(RxTok, TxTok<'_>)> {
        let p = self.rx.pop_front()?;
        Some((RxTok(p), TxTok(&mut self.tx)))
    }

    fn transmit(&mut self, _t: smoltcp::time::Instant) -> Option<TxTok<'_>> {
        Some(TxTok(&mut self.tx))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ip;
        c.max_transmission_unit = MTU;
        c
    }
}

pub(crate) struct Nat {
    sw: Weak<NetSwitch>,
    proxy: Option<Arc<HttpProxy>>,
    proxy_ports: Vec<u16>,
    max_conns: usize,
    iface: Interface,
    dev: VDev,
    sockets: SocketSet<'static>,
    conns: HashMap<u64, Conn>,
    pending: HashMap<u64, Pending>,
    by_key: HashMap<Key, u64>,
    ev_tx: mpsc::UnboundedSender<Ev>,
    next_id: u64,
    epoch: Instant,
}

fn random_u64() -> u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.finish()
}

fn new_socket() -> tcp::Socket<'static> {
    let mut s = tcp::Socket::new(tcp::SocketBuffer::new(vec![0; SOCK_RX]), tcp::SocketBuffer::new(vec![0; SOCK_TX]));
    s.set_nagle_enabled(false);
    s.set_keep_alive(Some(smoltcp::time::Duration::from_secs(60)));
    s
}

fn spawn_connect(
    id: u64,
    dst: SocketAddrV4,
    proxy: Option<Arc<HttpProxy>>,
    host: Option<String>,
    sw: Weak<NetSwitch>,
    ev: mpsc::UnboundedSender<Ev>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Прокси разрешит имя сам. Чтобы гость не подсунул имя, которое к этому
        // моменту указывает на хост или его сеть (DNS rebinding), проверяем
        // свежий ответ резолвера хоста тем же файрволом; не прошло — CONNECT по IP.
        let host = match host {
            Some(h) => {
                let ok = match tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::net::lookup_host((h.as_str(), dst.port())),
                )
                .await
                {
                    Ok(Ok(addrs)) => {
                        let v4: Vec<Ipv4Addr> = addrs
                            .filter_map(|a| match a {
                                std::net::SocketAddr::V4(a) => Some(*a.ip()),
                                std::net::SocketAddr::V6(_) => None,
                            })
                            .collect();
                        sw.upgrade().is_some_and(|sw| {
                            !v4.is_empty()
                                && v4.iter().all(|ip| sw.fw.check(*ip, dst.port()) == crate::firewall::Verdict::Allow)
                        })
                    }
                    _ => false,
                };
                if !ok {
                    tracing::debug!("fcw-net: имя {h} для {dst} не прошло перепроверку, CONNECT по IP");
                }
                ok.then_some(h)
            }
            None => None,
        };
        let e = match proxy::connect(dst, proxy.as_deref(), host.as_deref()).await {
            Ok(stream) => Ev::Connected { id, stream },
            Err(err) => Ev::ConnectFailed { id, err },
        };
        let _ = ev.send(e);
    })
}

impl Nat {
    pub fn new(
        sw: Weak<NetSwitch>,
        gw: Ipv4Addr,
        prefix: u8,
        proxy: Option<Arc<HttpProxy>>,
        proxy_ports: Vec<u16>,
        max_conns: usize,
        ev_tx: mpsc::UnboundedSender<Ev>,
    ) -> Self {
        let mut dev = VDev::default();
        let mut cfg = Config::new(HardwareAddress::Ip);
        cfg.random_seed = random_u64();
        let mut iface = Interface::new(cfg, &mut dev, smoltcp::time::Instant::ZERO);
        iface.update_ip_addrs(|a| {
            let _ = a.push(IpCidr::new(gw.into(), prefix));
        });
        iface.set_any_ip(true);
        let _ = iface.routes_mut().add_default_ipv4_route(gw);
        Nat {
            sw,
            proxy,
            proxy_ports,
            max_conns,
            iface,
            dev,
            sockets: SocketSet::new(Vec::new()),
            conns: HashMap::new(),
            pending: HashMap::new(),
            by_key: HashMap::new(),
            ev_tx,
            next_id: 1,
            epoch: Instant::now(),
        }
    }

    fn now(&self) -> smoltcp::time::Instant {
        smoltcp::time::Instant::from_micros(self.epoch.elapsed().as_micros() as i64)
    }

    fn poll(&mut self) {
        let now = self.now();
        self.iface.poll(now, &mut self.dev, &mut self.sockets);
    }

    fn rst_to_guest(port: &Port, pkt: &[u8]) {
        if let Some(ip) = wire::parse_ipv4(pkt) {
            if let Some(t) = wire::parse_tcp(&ip) {
                if let Some(r) = wire::build_tcp_rst_reply(&ip, &t, next_ip_id()) {
                    port.send_ip(&r);
                }
            }
        }
    }

    fn proxy_for(&self, port: u16) -> Option<Arc<HttpProxy>> {
        self.proxy.clone().filter(|_| self.proxy_ports.is_empty() || self.proxy_ports.contains(&port))
    }

    /// Создать слушающий сокет и скормить ему SYN. Пока других слушающих
    /// сокетов нет (каждый живёт только внутри этого вызова), SYN попадёт ровно в него.
    fn accept_syn(&mut self, key: Key, syn: &[u8]) -> Option<SocketHandle> {
        let mut s = new_socket();
        let ok = s.listen(key.remote).is_ok();
        let handle = self.sockets.add(s);
        if ok {
            self.dev.rx.push_back(syn.to_vec());
            self.poll();
        }
        let state = self.sockets.get::<tcp::Socket>(handle).state();
        if matches!(state, tcp::State::SynReceived | tcp::State::Established) {
            Some(handle)
        } else {
            tracing::debug!("fcw-net: SYN {} -> {} не принят стеком ({state})", key.guest, key.remote);
            self.sockets.remove(handle);
            None
        }
    }

    fn on_packet(&mut self, port: Arc<Port>, pkt: Bytes) {
        let Some(ip) = wire::parse_ipv4(&pkt) else { return };
        let Some(t) = wire::parse_tcp(&ip) else { return };
        let key = Key { guest: SocketAddrV4::new(ip.src, t.sport), remote: SocketAddrV4::new(ip.dst, t.dport) };
        if let Some(&id) = self.by_key.get(&key) {
            if let Some(p) = self.pending.get_mut(&id) {
                if t.has(wire::TCP_RST) {
                    self.cancel_pending(id);
                } else if t.is_syn_only() {
                    p.syn = pkt; // повтор SYN: запомним самый свежий
                }
                return;
            }
            self.dev.rx.push_back(pkt.to_vec());
            return;
        }
        if !t.is_syn_only() {
            // Соединение не наше (например, пережило перезапуск демона).
            Self::rst_to_guest(&port, &pkt);
            return;
        }
        if port.c.active() >= self.max_conns as u64 {
            Counters::inc(&port.c.limit_hits);
            tracing::debug!(vm = %port.vm_id, "fcw-net: лимит соединений, {} отклонён", key.remote);
            Self::rst_to_guest(&port, &pkt);
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        if self.proxy_for(key.remote.port()).is_some() {
            let Some(handle) = self.accept_syn(key, &pkt) else {
                Self::rst_to_guest(&port, &pkt);
                return;
            };
            Counters::inc(&port.c.tcp_active);
            Counters::inc(&port.c.tcp_total);
            self.by_key.insert(key, id);
            self.conns.insert(
                id,
                Conn {
                    handle,
                    key,
                    port,
                    phase: Phase::Sniffing { deadline: Instant::now() + SNIFF_TIMEOUT },
                    to_guest: VecDeque::new(),
                    credits: Arc::new(Semaphore::new(HOST_WINDOW)),
                    to_host: None,
                    host_eof: false,
                    fin_sent: false,
                },
            );
            return;
        }
        let task = spawn_connect(id, key.remote, None, None, Weak::new(), self.ev_tx.clone());
        Counters::inc(&port.c.tcp_active);
        Counters::inc(&port.c.tcp_total);
        self.by_key.insert(key, id);
        self.pending.insert(id, Pending { key, port, syn: pkt, task });
    }

    fn cancel_pending(&mut self, id: u64) {
        if let Some(p) = self.pending.remove(&id) {
            p.task.abort();
            self.by_key.remove(&p.key);
            Counters::dec(&p.port.c.tcp_active);
        }
    }

    fn open(&mut self, id: u64, stream: TcpStream) {
        let Some(c) = self.conns.get_mut(&id) else { return };
        let (r, w) = stream.into_split();
        let (to_host, rx) = mpsc::channel(TO_HOST_CHUNKS);
        let reader = tokio::spawn(host_reader(id, r, c.credits.clone(), self.ev_tx.clone()));
        let writer = tokio::spawn(host_writer(id, w, rx, self.ev_tx.clone()));
        c.to_host = Some(to_host);
        c.phase = Phase::Open { reader, writer };
        tracing::debug!(vm = %c.port.vm_id, "fcw-net: TCP {} -> {}", c.key.guest, c.key.remote);
    }

    fn on_connected(&mut self, id: u64, stream: TcpStream) {
        if let Some(p) = self.pending.remove(&id) {
            // Прямое соединение: теперь можно ответить гостю на SYN.
            let Some(handle) = self.accept_syn(p.key, &p.syn) else {
                self.by_key.remove(&p.key);
                Counters::dec(&p.port.c.tcp_active);
                Self::rst_to_guest(&p.port, &p.syn);
                return;
            };
            self.conns.insert(
                id,
                Conn {
                    handle,
                    key: p.key,
                    port: p.port,
                    phase: Phase::Connecting { task: p.task },
                    to_guest: VecDeque::new(),
                    credits: Arc::new(Semaphore::new(HOST_WINDOW)),
                    to_host: None,
                    host_eof: false,
                    fin_sent: false,
                },
            );
            self.open(id, stream);
        } else if self.conns.get(&id).is_some_and(|c| matches!(c.phase, Phase::Connecting { .. })) {
            self.open(id, stream);
        }
    }

    fn on_ev(&mut self, e: Ev) {
        match e {
            Ev::Connected { id, stream } => self.on_connected(id, stream),
            Ev::ConnectFailed { id, err } => {
                if let Some(p) = self.pending.remove(&id) {
                    tracing::debug!(vm = %p.port.vm_id, "fcw-net: TCP {} недоступен: {err}", p.key.remote);
                    self.by_key.remove(&p.key);
                    Counters::dec(&p.port.c.tcp_active);
                    // И на отказ, и на тайм-аут — RST: гостю незачем ждать дальше.
                    Self::rst_to_guest(&p.port, &p.syn);
                } else if let Some(c) = self.conns.get(&id) {
                    tracing::debug!(vm = %c.port.vm_id, "fcw-net: TCP {} через прокси: {err}", c.key.remote);
                    self.sockets.get_mut::<tcp::Socket>(c.handle).abort();
                }
            }
            Ev::Data { id, data } => {
                if let Some(c) = self.conns.get_mut(&id) {
                    c.to_guest.push_back(data);
                }
            }
            Ev::Eof { id } => {
                if let Some(c) = self.conns.get_mut(&id) {
                    c.host_eof = true;
                }
            }
            Ev::HostError { id } => {
                if let Some(c) = self.conns.get_mut(&id) {
                    self.sockets.get_mut::<tcp::Socket>(c.handle).abort();
                    c.to_guest.clear();
                    c.host_eof = true;
                }
            }
            Ev::Drained => {}
            Ev::PortClosed { port } => {
                let ids: Vec<u64> = self.pending.iter().filter(|(_, p)| p.port.id == port).map(|(i, _)| *i).collect();
                for id in ids {
                    self.cancel_pending(id);
                }
                let ids: Vec<u64> = self.conns.iter().filter(|(_, c)| c.port.id == port).map(|(i, _)| *i).collect();
                for id in ids {
                    self.remove_conn(id, false);
                }
            }
        }
    }

    fn remove_conn(&mut self, id: u64, graceful: bool) {
        let Some(c) = self.conns.remove(&id) else { return };
        self.sockets.remove(c.handle);
        self.by_key.remove(&c.key);
        match c.phase {
            Phase::Sniffing { .. } => {}
            Phase::Connecting { task } => task.abort(),
            Phase::Open { reader, writer } => {
                reader.abort();
                // При мягком закрытии писатель сам допишет хвост и закроет запись.
                if !graceful {
                    writer.abort();
                }
            }
        }
        drop(c.to_host);
        Counters::dec(&c.port.c.tcp_active);
    }

    /// Соединения через прокси, у которых пора выбрать имя и соединиться.
    fn sniff(&mut self) {
        let now = Instant::now();
        let mut ready = Vec::new();
        for (&id, c) in self.conns.iter() {
            let Phase::Sniffing { deadline } = c.phase else { continue };
            let s = self.sockets.get_mut::<tcp::Socket>(c.handle);
            let queued = s.recv_queue();
            let mut name = None;
            let mut done = now >= deadline || !s.may_recv() && queued == 0 && s.state() != tcp::State::SynReceived;
            if queued > 0 {
                let mut buf = vec![0u8; queued.min(MAX_HELLO)];
                let n = s.peek_slice(&mut buf).unwrap_or(0);
                match parse_sni(&buf[..n]) {
                    Sni::NeedMore if n < MAX_HELLO && !done => {}
                    Sni::NeedMore => done = true,
                    Sni::Done(sni) => {
                        name = sni;
                        done = true;
                    }
                }
            }
            if done {
                ready.push((id, name));
            }
        }
        if ready.is_empty() {
            return;
        }
        let sw = self.sw.upgrade();
        for (id, sni) in ready {
            let Some(port) = self.conns.get(&id).map(|c| c.key.remote.port()) else { continue };
            let proxy = self.proxy_for(port);
            let Some(c) = self.conns.get_mut(&id) else { continue };
            let ip = *c.key.remote.ip();
            let host = sw.as_ref().and_then(|sw| {
                sni.filter(|n| sw.name_resolved_to(c.port.id, n, ip))
                    .or_else(|| sw.name_for(c.port.id, ip).map(|n| n.to_string()))
            });
            tracing::debug!(vm = %c.port.vm_id, "fcw-net: TCP {} через прокси как {:?}", c.key.remote, host);
            let task = spawn_connect(id, c.key.remote, proxy, host, self.sw.clone(), self.ev_tx.clone());
            c.phase = Phase::Connecting { task };
        }
    }

    /// Переложить данные между сокетами smoltcp и задачами хоста.
    fn service(&mut self) -> bool {
        let mut progressed = false;
        let mut dead = Vec::new();
        for (&id, c) in self.conns.iter_mut() {
            let s = self.sockets.get_mut::<tcp::Socket>(c.handle);
            // хост -> гость
            while let Some(front) = c.to_guest.front_mut() {
                if !s.can_send() {
                    break;
                }
                match s.send_slice(front) {
                    Ok(n) if n > 0 => {
                        c.credits.add_permits(n);
                        let _ = front.split_to(n);
                        if front.is_empty() {
                            c.to_guest.pop_front();
                        }
                        progressed = true;
                    }
                    _ => break,
                }
            }
            if c.host_eof && c.to_guest.is_empty() && !c.fin_sent && s.may_send() {
                s.close();
                c.fin_sent = true;
                progressed = true;
            }
            // гость -> хост
            if let Some(tx) = &c.to_host {
                while s.can_recv() {
                    match tx.try_reserve() {
                        Ok(permit) => match s.recv(|b| {
                            let n = b.len().min(CHUNK);
                            (n, Bytes::copy_from_slice(&b[..n]))
                        }) {
                            Ok(d) => {
                                permit.send(d);
                                progressed = true;
                            }
                            Err(_) => break,
                        },
                        Err(TrySendError::Full(())) => break,
                        Err(TrySendError::Closed(())) => {
                            s.abort();
                            break;
                        }
                    }
                }
                // Гость закрыл свою сторону и всё прочитано: закрыть запись к хосту.
                let guest_fin = matches!(
                    s.state(),
                    tcp::State::CloseWait | tcp::State::LastAck | tcp::State::Closing | tcp::State::TimeWait
                );
                if guest_fin && s.recv_queue() == 0 {
                    c.to_host = None;
                    progressed = true;
                }
            }
            match s.state() {
                // Listen: гость сбросил соединение во время рукопожатия.
                tcp::State::Closed | tcp::State::Listen => {
                    let graceful = c.to_host.is_none() && !matches!(c.phase, Phase::Connecting { .. });
                    dead.push((id, graceful));
                }
                _ => {}
            }
        }
        for (id, graceful) in dead {
            self.remove_conn(id, graceful);
            progressed = true;
        }
        progressed
    }

    /// Отправить пакеты стека машинам.
    fn flush_tx(&mut self) {
        if self.dev.tx.is_empty() {
            return;
        }
        let sw = self.sw.upgrade();
        for pkt in std::mem::take(&mut self.dev.tx) {
            let Some(ip) = wire::parse_ipv4(&pkt) else { continue };
            let port = wire::parse_tcp(&ip)
                .and_then(|t| {
                    let key =
                        Key { guest: SocketAddrV4::new(ip.dst, t.dport), remote: SocketAddrV4::new(ip.src, t.sport) };
                    self.by_key.get(&key).and_then(|id| self.conns.get(id)).map(|c| c.port.clone())
                })
                .or_else(|| sw.as_ref().and_then(|sw| sw.port_by_ip(ip.dst)));
            if let Some(port) = port {
                port.send_ip(&pkt);
            }
        }
    }

    fn pump(&mut self) {
        for _ in 0..8 {
            self.poll();
            self.sniff();
            let progressed = self.service();
            self.flush_tx();
            if !progressed {
                break;
            }
        }
        self.flush_tx();
    }

    fn poll_delay(&mut self) -> Duration {
        let now = self.now();
        let mut d = self
            .iface
            .poll_delay(now, &self.sockets)
            .map_or(Duration::from_secs(1), |d| Duration::from_micros(d.total_micros()))
            .min(Duration::from_secs(1));
        if self.conns.values().any(|c| matches!(c.phase, Phase::Sniffing { .. })) {
            d = d.min(Duration::from_millis(100));
        }
        d
    }
}

pub(crate) async fn run(mut nat: Nat, mut pkt_rx: mpsc::Receiver<Msg>, mut ev_rx: mpsc::UnboundedReceiver<Ev>) {
    loop {
        let delay = nat.poll_delay();
        tokio::select! {
            m = pkt_rx.recv() => match m {
                Some(Msg::Packet { port, pkt }) => nat.on_packet(port, pkt),
                None => break, // свитч уничтожен
            },
            Some(e) = ev_rx.recv() => nat.on_ev(e),
            _ = tokio::time::sleep(delay) => {}
        }
        for _ in 0..256 {
            match pkt_rx.try_recv() {
                Ok(Msg::Packet { port, pkt }) => nat.on_packet(port, pkt),
                Err(_) => break,
            }
        }
        for _ in 0..256 {
            match ev_rx.try_recv() {
                Ok(e) => nat.on_ev(e),
                Err(_) => break,
            }
        }
        nat.pump();
    }
    let ids: Vec<u64> = nat.pending.keys().copied().collect();
    for id in ids {
        nat.cancel_pending(id);
    }
    let ids: Vec<u64> = nat.conns.keys().copied().collect();
    for id in ids {
        nat.remove_conn(id, false);
    }
}

async fn host_reader(id: u64, mut r: OwnedReadHalf, credits: Arc<Semaphore>, ev: mpsc::UnboundedSender<Ev>) {
    loop {
        let Ok(permit) = credits.acquire_many(CHUNK as u32).await else { return };
        permit.forget();
        let mut buf = vec![0u8; CHUNK];
        match r.read(&mut buf).await {
            Ok(0) => {
                let _ = ev.send(Ev::Eof { id });
                return;
            }
            Ok(n) => {
                credits.add_permits(CHUNK - n);
                buf.truncate(n);
                if ev.send(Ev::Data { id, data: Bytes::from(buf) }).is_err() {
                    return;
                }
            }
            Err(_) => {
                let _ = ev.send(Ev::HostError { id });
                return;
            }
        }
    }
}

async fn host_writer(id: u64, mut w: OwnedWriteHalf, mut rx: mpsc::Receiver<Bytes>, ev: mpsc::UnboundedSender<Ev>) {
    while let Some(b) = rx.recv().await {
        if w.write_all(&b).await.is_err() {
            let _ = ev.send(Ev::HostError { id });
            return;
        }
        let _ = ev.send(Ev::Drained);
    }
    let _ = w.shutdown().await;
}

// ------------------------------------------------------------ SNI

/// Больше ClientHello не ждём (одна TLS-запись).
const MAX_HELLO: usize = 5 + 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum Sni {
    NeedMore,
    /// Разобрано: имя из server_name или его нет (не TLS, нет расширения).
    Done(Option<String>),
}

/// Имя сервера из TLS ClientHello (первая запись).
fn parse_sni(b: &[u8]) -> Sni {
    if b.is_empty() {
        return Sni::NeedMore;
    }
    if b[0] != 0x16 {
        return Sni::Done(None);
    }
    if b.len() < 5 {
        return Sni::NeedMore;
    }
    let rec_len = usize::from(u16::from_be_bytes([b[3], b[4]]));
    if b.len() < 5 + rec_len {
        return Sni::NeedMore;
    }
    Sni::Done(sni_from_handshake(&b[5..5 + rec_len]))
}

fn sni_from_handshake(h: &[u8]) -> Option<String> {
    let mut r = Reader(h);
    if r.u8()? != 1 {
        return None; // не ClientHello
    }
    let len = r.u24()?;
    let mut r = Reader(r.take(len)?);
    r.take(2 + 32)?; // версия, random
    let sid = usize::from(r.u8()?);
    r.take(sid)?;
    let cs = usize::from(r.u16()?);
    r.take(cs)?;
    let cm = usize::from(r.u8()?);
    r.take(cm)?;
    let ext_len = usize::from(r.u16()?);
    let mut ext = Reader(r.take(ext_len)?);
    while !ext.0.is_empty() {
        let t = ext.u16()?;
        let l = usize::from(ext.u16()?);
        let data = ext.take(l)?;
        if t != 0 {
            continue;
        }
        let mut sn = Reader(data);
        let list = usize::from(sn.u16()?);
        let mut list = Reader(sn.take(list)?);
        while !list.0.is_empty() {
            let kind = list.u8()?;
            let nl = usize::from(list.u16()?);
            let name = list.take(nl)?;
            if kind == 0 {
                let s = std::str::from_utf8(name).ok()?.trim_end_matches('.').to_ascii_lowercase();
                let ok = !s.is_empty()
                    && s.len() <= 253
                    && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.' || c == b'_');
                return ok.then_some(s);
            }
        }
        return None;
    }
    None
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }
    fn u24(&mut self) -> Option<usize> {
        self.take(3).map(|b| (usize::from(b[0]) << 16) | (usize::from(b[1]) << 8) | usize::from(b[2]))
    }
}

#[cfg(test)]
pub(crate) fn client_hello(sni: Option<&str>) -> Vec<u8> {
    let mut ext = Vec::new();
    // Постороннее расширение до server_name.
    ext.extend_from_slice(&[0x00, 0x0b, 0x00, 0x02, 0x01, 0x00]);
    if let Some(n) = sni {
        let mut list = vec![0u8];
        list.extend_from_slice(&(n.len() as u16).to_be_bytes());
        list.extend_from_slice(n.as_bytes());
        ext.extend_from_slice(&[0x00, 0x00]);
        ext.extend_from_slice(&((list.len() + 2) as u16).to_be_bytes());
        ext.extend_from_slice(&(list.len() as u16).to_be_bytes());
        ext.extend_from_slice(&list);
    }
    let mut body = vec![0x03, 0x03];
    body.extend_from_slice(&[0x11; 32]);
    body.push(32);
    body.extend_from_slice(&[0x22; 32]);
    body.extend_from_slice(&[0x00, 0x04, 0x13, 0x01, 0x13, 0x02]);
    body.extend_from_slice(&[0x01, 0x00]);
    body.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    body.extend_from_slice(&ext);
    let mut hs = vec![0x01];
    hs.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    hs.extend_from_slice(&body);
    let mut rec = vec![0x16, 0x03, 0x01];
    rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    rec.extend_from_slice(&hs);
    rec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sni_parsing() {
        let h = client_hello(Some("Example.COM"));
        assert_eq!(parse_sni(&h), Sni::Done(Some("example.com".into())));
        for cut in [0, 3, 5, 40, h.len() - 1] {
            assert_eq!(parse_sni(&h[..cut]), Sni::NeedMore, "{cut}");
        }
        assert_eq!(parse_sni(&client_hello(None)), Sni::Done(None));
        assert_eq!(parse_sni(b"GET / HTTP/1.1\r\n"), Sni::Done(None));
        assert_eq!(parse_sni(&client_hello(Some("bad name/x"))), Sni::Done(None));
        // Испорченная длина внутри записи.
        let mut bad = client_hello(Some("a.b"));
        bad[6] = 0xff;
        assert_eq!(parse_sni(&bad), Sni::Done(None));
    }
}
