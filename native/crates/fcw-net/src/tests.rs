//! Сквозные тесты свитча: «машины» — тестовый код по ту сторону duplex-потока,
//! кадры собираются руками.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};

use crate::switch::GATEWAY_MAC;
use crate::wire::{self, Mac};
use crate::{dhcp, dns, FirewallConfig, NetConfig, NetSwitch};

const GW: Ipv4Addr = Ipv4Addr::new(10, 42, 0, 1);

struct Vm {
    mac: Mac,
    ip: Ipv4Addr,
    r: ReadHalf<DuplexStream>,
    w: WriteHalf<DuplexStream>,
}

impl Vm {
    async fn connect(sw: &Arc<NetSwitch>, vm_id: &str, host: &str) -> Vm {
        let mac = NetSwitch::mac_for_vm(vm_id);
        let (a, b) = tokio::io::duplex(1 << 20);
        let (sw2, id, h) = (sw.clone(), vm_id.to_string(), host.to_string());
        tokio::spawn(async move { sw2.run_port(&id, &h, mac, b).await });
        // Дождаться регистрации порта.
        for _ in 0..100 {
            if sw.stats(vm_id).is_some_and(|s| s.connected) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let ip = sw.ip_of(vm_id).unwrap();
        let (r, w) = tokio::io::split(a);
        Vm { mac, ip, r, w }
    }

    async fn send(&mut self, frame: &[u8]) {
        self.w.write_all(&(frame.len() as u32).to_be_bytes()).await.unwrap();
        self.w.write_all(frame).await.unwrap();
    }

    async fn send_ip(&mut self, dst_mac: Mac, pkt: &[u8]) {
        let f = wire::build_eth(dst_mac, self.mac, wire::ETHERTYPE_IPV4, pkt);
        self.send(&f).await;
    }

    async fn recv_timeout(&mut self, t: Duration) -> Option<Vec<u8>> {
        tokio::time::timeout(t, async {
            let mut l = [0u8; 4];
            self.r.read_exact(&mut l).await.unwrap();
            let mut f = vec![0u8; u32::from_be_bytes(l) as usize];
            self.r.read_exact(&mut f).await.unwrap();
            f
        })
        .await
        .ok()
    }

    async fn recv(&mut self) -> Vec<u8> {
        self.recv_timeout(Duration::from_secs(5)).await.expect("нет кадра")
    }

    /// Ждать кадр, подходящий под условие (остальные пропускаются).
    async fn recv_match(&mut self, mut f: impl FnMut(&[u8]) -> bool) -> Vec<u8> {
        loop {
            let fr = self.recv().await;
            if f(&fr) {
                return fr;
            }
        }
    }

    async fn nothing(&mut self, ms: u64) {
        if let Some(f) = self.recv_timeout(Duration::from_millis(ms)).await {
            panic!("неожиданный кадр: {:02x?}", &f[..f.len().min(64)]);
        }
    }

    fn udp_to(&self, dst: Ipv4Addr, sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
        let u = wire::build_udp(self.ip, sport, dst, dport, payload);
        wire::build_ipv4(self.ip, dst, wire::PROTO_UDP, 1, &u)
    }

    async fn dns(&mut self, name: &str, qtype: u16) -> (u8, bool, Vec<dns::RData>) {
        let q = dns::build_query(0x4242, name, qtype, true);
        let pkt = self.udp_to(GW, 5353, 53, &q);
        self.send_ip(GATEWAY_MAC, &pkt).await;
        let f = self.recv_match(|f| udp_of(f).is_some_and(|(s, _, _)| s == 53)).await;
        let (_, _, payload) = udp_of(&f).unwrap();
        let (id, rcode, aa, ans) = dns::parse_response(&payload).unwrap();
        assert_eq!(id, 0x4242);
        (rcode, aa, ans)
    }
}

/// (sport, dport, payload) UDP-кадра.
fn udp_of(f: &[u8]) -> Option<(u16, u16, Vec<u8>)> {
    let e = wire::parse_eth(f)?;
    let ip = wire::parse_ipv4(e.payload)?;
    let u = wire::parse_udp(&ip)?;
    Some((u.sport, u.dport, u.payload.to_vec()))
}

fn tcp_of(f: &[u8]) -> Option<(Ipv4Addr, Ipv4Addr, wire::Tcp, Vec<u8>)> {
    let e = wire::parse_eth(f)?;
    let ip = wire::parse_ipv4(e.payload)?;
    let t = wire::parse_tcp(&ip)?;
    assert!(wire::tcp_checksum_ok(&ip));
    let data = ip.payload[ip.payload.len() - t.data_len..].to_vec();
    Some((ip.src, ip.dst, t, data))
}

fn test_cfg() -> NetConfig {
    NetConfig {
        rate_limit_bytes_per_sec: 0,
        firewall: FirewallConfig {
            // Для тестов NAT открываем 127.0.0.1 (серверы тестов слушают там).
            allow: vec!["127.0.0.1/32".parse().unwrap()],
            ..FirewallConfig::default()
        },
        ..NetConfig::default()
    }
}

#[tokio::test]
async fn dhcp_arp_ping_dns() {
    let sw = NetSwitch::new(test_cfg()).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "Alice PC").await;
    assert!(!sw.stats("vm-a").unwrap().ip_configured);
    assert!(sw.ip_of("vm-a").unwrap().octets()[..2] == [10, 42]);
    assert_eq!(sw.stats("vm-a").unwrap().hostname, "alice-pc");

    // DHCP DISCOVER -> OFFER -> REQUEST -> ACK
    for (t, req) in [(dhcp::DISCOVER, None), (dhcp::REQUEST, Some(a.ip))] {
        let d = dhcp::build_request(0x77, a.mac, t, req, req.map(|_| GW));
        let u = wire::build_udp(Ipv4Addr::UNSPECIFIED, 68, Ipv4Addr::BROADCAST, 67, &d);
        let pkt = wire::build_ipv4(Ipv4Addr::UNSPECIFIED, Ipv4Addr::BROADCAST, wire::PROTO_UDP, 1, &u);
        a.send_ip(wire::BROADCAST_MAC, &pkt).await;
        let f = a.recv_match(|f| udp_of(f).is_some_and(|(s, d, _)| (s, d) == (67, 68))).await;
        assert_eq!(wire::parse_eth(&f).unwrap().src, GATEWAY_MAC);
        let (_, _, payload) = udp_of(&f).unwrap();
        let (mt, yiaddr, opts) = dhcp::parse_reply(&payload).unwrap();
        assert_eq!(mt, if t == dhcp::DISCOVER { dhcp::OFFER } else { dhcp::ACK });
        assert_eq!(yiaddr, a.ip);
        let get = |c: u8| opts.iter().find(|(k, _)| *k == c).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get(3), GW.octets().to_vec());
        assert_eq!(get(6), GW.octets().to_vec());
        assert_eq!(get(15), b"mc".to_vec());
        assert_eq!(get(12), b"alice-pc".to_vec());
    }
    assert!(sw.stats("vm-a").unwrap().ip_configured);

    // ARP шлюза.
    let arp = wire::Arp { op: wire::ARP_REQUEST, sha: a.mac, spa: a.ip, tha: [0; 6], tpa: GW };
    a.send(&wire::build_eth(wire::BROADCAST_MAC, a.mac, wire::ETHERTYPE_ARP, &wire::build_arp(&arp))).await;
    let f = a.recv().await;
    let e = wire::parse_eth(&f).unwrap();
    assert_eq!((e.dst, e.src, e.ethertype), (a.mac, GATEWAY_MAC, wire::ETHERTYPE_ARP));
    let r = wire::parse_arp(e.payload).unwrap();
    assert_eq!((r.op, r.sha, r.spa, r.tha, r.tpa), (wire::ARP_REPLY, GATEWAY_MAC, GW, a.mac, a.ip));

    // ARP на чужой адрес шлюз не отвечает.
    let arp = wire::Arp { tpa: Ipv4Addr::new(10, 42, 9, 9), ..arp };
    a.send(&wire::build_eth(wire::BROADCAST_MAC, a.mac, wire::ETHERTYPE_ARP, &wire::build_arp(&arp))).await;
    a.nothing(200).await;

    // ping шлюза
    let mut icmp = vec![8, 0, 0, 0, 0, 1, 0, 1, 1, 2, 3, 4];
    let c = wire::checksum(&icmp);
    icmp[2..4].copy_from_slice(&c.to_be_bytes());
    a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, GW, wire::PROTO_ICMP, 3, &icmp)).await;
    let f = a.recv().await;
    let ip = wire::parse_ipv4(wire::parse_eth(&f).unwrap().payload).unwrap().packet.to_vec();
    let ip = wire::parse_ipv4(&ip).unwrap();
    assert_eq!((ip.src, ip.dst, ip.payload[0]), (GW, a.ip, 0));

    // DNS
    let mut b = Vm::connect(&sw, "vm-b", "bob").await;
    let (rc, aa, ans) = a.dns("bob.mc", dns::TYPE_A).await;
    assert_eq!((rc, aa, ans), (dns::NOERROR, true, vec![dns::RData::A(b.ip)]));
    let (rc, _, ans) = b.dns("ALICE-PC.mc", dns::TYPE_A).await;
    assert_eq!((rc, ans), (dns::NOERROR, vec![dns::RData::A(a.ip)]));
    assert_eq!(a.dns("gateway.mc", dns::TYPE_A).await.2, vec![dns::RData::A(GW)]);
    assert_eq!(a.dns("bob.mc", dns::TYPE_AAAA).await, (dns::NOERROR, true, vec![]));
    assert_eq!(a.dns("nobody.mc", dns::TYPE_A).await.0, dns::NXDOMAIN);
    assert_eq!(a.dns("nas", dns::TYPE_A).await.0, dns::NXDOMAIN);
    assert_eq!(a.dns("example.com", dns::TYPE_AAAA).await, (dns::NOERROR, false, vec![]));
    let rev = {
        let o = b.ip.octets();
        format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])
    };
    assert_eq!(a.dns(&rev, dns::TYPE_PTR).await.2, vec![dns::RData::Ptr("bob.mc".into())]);

    // Машина отключилась — её имени больше нет.
    drop(b);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(a.dns("bob.mc", dns::TYPE_A).await.0, dns::NXDOMAIN);
    assert!(!sw.stats("vm-b").unwrap().connected);
    assert!(sw.ip_of("vm-b").is_some());
    assert!(sw.stats("vm-a").unwrap().dns_queries >= 8);
}

#[tokio::test]
async fn l2_switching_and_port_security() {
    let sw = NetSwitch::new(test_cfg()).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let mut b = Vm::connect(&sw, "vm-b", "b").await;
    let mut c = Vm::connect(&sw, "vm-c", "c").await;

    // Широковещательный ARP от a видят b и c.
    let arp = wire::Arp { op: wire::ARP_REQUEST, sha: a.mac, spa: a.ip, tha: [0; 6], tpa: b.ip };
    let req = wire::build_eth(wire::BROADCAST_MAC, a.mac, wire::ETHERTYPE_ARP, &wire::build_arp(&arp));
    a.send(&req).await;
    assert_eq!(b.recv().await, req);
    assert_eq!(c.recv().await, req);
    a.nothing(100).await;

    // Ответ b — только a.
    let rep = wire::Arp { op: wire::ARP_REPLY, sha: b.mac, spa: b.ip, tha: a.mac, tpa: a.ip };
    let rep = wire::build_eth(a.mac, b.mac, wire::ETHERTYPE_ARP, &wire::build_arp(&rep));
    b.send(&rep).await;
    assert_eq!(a.recv().await, rep);
    c.nothing(100).await;

    // Unicast IPv4 a -> b.
    let pkt = a.udp_to(b.ip, 1111, 2222, b"hi bob");
    a.send_ip(b.mac, &pkt).await;
    assert_eq!(udp_of(&b.recv().await).unwrap(), (1111, 2222, b"hi bob".to_vec()));
    c.nothing(100).await;

    // Подделки от a не проходят: чужой MAC, чужой IP, ARP от имени шлюза, IPv6,
    // ответ DHCP-сервера.
    let mut spoof_mac = wire::build_eth(b.mac, [0x52, 0x54, 0, 9, 9, 9], wire::ETHERTYPE_IPV4, &pkt);
    a.send(&spoof_mac).await;
    spoof_mac[6..12].copy_from_slice(&c.mac);
    a.send(&spoof_mac).await;
    let fake_ip = {
        let u = wire::build_udp(c.ip, 1, b.ip, 2, b"x");
        wire::build_ipv4(c.ip, b.ip, wire::PROTO_UDP, 1, &u)
    };
    a.send_ip(b.mac, &fake_ip).await;
    let poison = wire::Arp { op: wire::ARP_REPLY, sha: a.mac, spa: GW, tha: b.mac, tpa: b.ip };
    a.send(&wire::build_eth(b.mac, a.mac, wire::ETHERTYPE_ARP, &wire::build_arp(&poison))).await;
    a.send(&wire::build_eth(wire::BROADCAST_MAC, a.mac, 0x86dd, &[0x60; 40])).await;
    let rogue = {
        let u = wire::build_udp(a.ip, 67, Ipv4Addr::BROADCAST, 68, &[0; 300]);
        wire::build_ipv4(a.ip, Ipv4Addr::BROADCAST, wire::PROTO_UDP, 1, &u)
    };
    a.send_ip(wire::BROADCAST_MAC, &rogue).await;
    // Несуществующий MAC назначения.
    a.send_ip([0x52, 0x54, 0, 1, 1, 1], &pkt).await;
    b.nothing(200).await;
    c.nothing(50).await;
    let st = sw.stats("vm-a").unwrap();
    assert_eq!((st.spoofed_frames, st.unsupported_frames), (5, 1));

    // Второй порт с тем же vm_id вытесняет первый.
    let mut a2 = Vm::connect(&sw, "vm-a", "a").await;
    assert_eq!(a2.ip, a.ip);
    let mut buf = [0u8; 4];
    let eof = tokio::time::timeout(Duration::from_secs(2), a.r.read(&mut buf)).await.unwrap().unwrap();
    assert_eq!(eof, 0, "старый порт должен закрыться");
    let pkt = b.udp_to(a2.ip, 5, 6, b"again");
    b.send_ip(a2.mac, &pkt).await;
    assert_eq!(udp_of(&a2.recv().await).unwrap().2, b"again".to_vec());
}

#[tokio::test]
async fn firewall_rejects() {
    let sw = NetSwitch::new(NetConfig { rate_limit_bytes_per_sec: 0, ..NetConfig::default() }).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let host_addr = crate::firewall::host_ipv4_addrs().into_iter().find(|i| !i.is_loopback());
    let mut targets = vec![
        (Ipv4Addr::new(10, 0, 0, 1), 80),
        (Ipv4Addr::new(192, 168, 1, 1), 80),
        (Ipv4Addr::new(172, 16, 0, 1), 443),
        (Ipv4Addr::new(100, 64, 0, 1), 80),
        (Ipv4Addr::new(169, 254, 169, 254), 80),
        (Ipv4Addr::new(127, 0, 0, 1), 25565),
        (GW, 22),
        (Ipv4Addr::new(93, 184, 215, 14), 25),
    ];
    if let Some(h) = host_addr {
        targets.push((h, 25575));
    }
    for (i, (dst, port)) in targets.iter().enumerate() {
        let seg = wire::build_tcp(a.ip, 40000 + i as u16, *dst, *port, 5000, 0, wire::TCP_SYN, 64240, &[]);
        a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, *dst, wire::PROTO_TCP, 1, &seg)).await;
        let (src, d, t, _) = tcp_of(&a.recv().await).unwrap();
        assert_eq!((src, d), (*dst, a.ip), "{dst}");
        assert_eq!(t.flags, wire::TCP_RST | wire::TCP_ACK, "{dst}");
        assert_eq!(t.ack, 5001);
    }
    // UDP в запрещённые места — молча.
    for dst in [Ipv4Addr::new(192, 168, 0, 1), Ipv4Addr::new(127, 0, 0, 1)] {
        let pkt = a.udp_to(dst, 3333, 53, b"q");
        a.send_ip(GATEWAY_MAC, &pkt).await;
    }
    a.nothing(200).await;
    assert_eq!(sw.stats("vm-a").unwrap().firewall_blocked, targets.len() as u64 + 2);
}

#[tokio::test]
async fn internet_disabled() {
    let sw = NetSwitch::new(NetConfig { allow_internet: false, ..test_cfg() }).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let seg = wire::build_tcp(a.ip, 40000, Ipv4Addr::new(1, 1, 1, 1), 443, 1, 0, wire::TCP_SYN, 1000, &[]);
    a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, Ipv4Addr::new(1, 1, 1, 1), wire::PROTO_TCP, 1, &seg)).await;
    assert_eq!(tcp_of(&a.recv().await).unwrap().2.flags, wire::TCP_RST | wire::TCP_ACK);
    // А зона DNS работает.
    assert_eq!(a.dns("a.mc", dns::TYPE_A).await.2, vec![dns::RData::A(a.ip)]);
}

#[tokio::test]
async fn tcp_nat_end_to_end() {
    let srv = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = srv.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut s, _) = srv.accept().await.unwrap();
        s.write_all(b"HELLO\n").await.unwrap();
        let mut got = Vec::new();
        s.read_to_end(&mut got).await.unwrap();
        // Большой ответ после полузакрытия гостем.
        s.write_all(&vec![b'z'; 300_000]).await.unwrap();
        got
    });

    let sw = NetSwitch::new(test_cfg()).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let lo = Ipv4Addr::LOCALHOST;
    let tcp = |a: &Vm, seq: u32, ack: u32, flags: u8, data: &[u8]| {
        let seg = wire::build_tcp(a.ip, 41000, lo, srv_port, seq, ack, flags, 65535, data);
        wire::build_ipv4(a.ip, lo, wire::PROTO_TCP, 1, &seg)
    };

    // Рукопожатие.
    let isn = 100_000u32;
    a.send_ip(GATEWAY_MAC, &tcp(&a, isn, 0, wire::TCP_SYN, &[])).await;
    let (src, dst, t, _) = tcp_of(&a.recv().await).unwrap();
    assert_eq!((src, dst), (lo, a.ip));
    assert_eq!((t.sport, t.dport), (srv_port, 41000));
    assert_eq!(t.flags & (wire::TCP_SYN | wire::TCP_ACK), wire::TCP_SYN | wire::TCP_ACK);
    assert_eq!(t.ack, isn + 1);
    let mut rcv = t.seq.wrapping_add(1);
    let mut snd = isn + 1;
    a.send_ip(GATEWAY_MAC, &tcp(&a, snd, rcv, wire::TCP_ACK, &[])).await;
    assert_eq!(sw.stats("vm-a").unwrap().tcp_connections, 1);

    // Приветствие сервера.
    let f = a.recv_match(|f| tcp_of(f).is_some_and(|(_, _, _, d)| !d.is_empty())).await;
    let (_, _, t, d) = tcp_of(&f).unwrap();
    assert_eq!(d, b"HELLO\n");
    assert_eq!(t.seq, rcv);
    rcv = rcv.wrapping_add(d.len() as u32);

    // Данные гостя + FIN.
    a.send_ip(GATEWAY_MAC, &tcp(&a, snd, rcv, wire::TCP_ACK | 0x08, b"from guest")).await;
    snd += 10;
    a.send_ip(GATEWAY_MAC, &tcp(&a, snd, rcv, wire::TCP_ACK | wire::TCP_FIN, &[])).await;
    snd += 1;

    // Принимаем 300 КБ, подтверждая всё полученное по порядку.
    let mut total = 0usize;
    let mut fin = false;
    while !fin {
        let f = a.recv().await;
        let Some((_, _, t, d)) = tcp_of(&f) else { continue };
        if t.seq == rcv {
            total += d.len();
            rcv = rcv.wrapping_add(d.len() as u32);
            if t.has(wire::TCP_FIN) {
                rcv = rcv.wrapping_add(1);
                fin = true;
            }
        }
        a.send_ip(GATEWAY_MAC, &tcp(&a, snd, rcv, wire::TCP_ACK, &[])).await;
    }
    assert_eq!(total, 300_000);
    assert_eq!(server.await.unwrap(), b"from guest");
    // После TIME-WAIT (smoltcp держит 10 с) соединение уходит из статистики.
    let st = sw.stats("vm-a").unwrap();
    assert_eq!(st.tcp_connections_total, 1);

    // Закрытый порт -> RST на SYN.
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let seg = wire::build_tcp(a.ip, 41001, lo, closed, 7, 0, wire::TCP_SYN, 65535, &[]);
    a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, lo, wire::PROTO_TCP, 1, &seg)).await;
    let f = a.recv_match(|f| tcp_of(f).is_some_and(|(_, _, t, _)| t.dport == 41001)).await;
    let (_, _, t, _) = tcp_of(&f).unwrap();
    assert_eq!((t.flags, t.ack), (wire::TCP_RST | wire::TCP_ACK, 8));

    // Сегмент несуществующего соединения -> RST.
    let seg = wire::build_tcp(a.ip, 41002, lo, srv_port, 9, 1234, wire::TCP_ACK, 65535, b"x");
    a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, lo, wire::PROTO_TCP, 1, &seg)).await;
    let f = a.recv_match(|f| tcp_of(f).is_some_and(|(_, _, t, _)| t.dport == 41002)).await;
    let (_, _, t, _) = tcp_of(&f).unwrap();
    assert_eq!((t.flags, t.seq), (wire::TCP_RST, 1234));
}

#[tokio::test]
async fn connection_limit() {
    let srv = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = srv.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut keep = Vec::new();
        while let Ok((s, _)) = srv.accept().await {
            keep.push(s);
        }
    });
    let sw = NetSwitch::new(NetConfig { max_connections_per_vm: 2, ..test_cfg() }).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let lo = Ipv4Addr::LOCALHOST;
    for sport in [50000u16, 50001, 50002] {
        let seg = wire::build_tcp(a.ip, sport, lo, port, 1, 0, wire::TCP_SYN, 65535, &[]);
        a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, lo, wire::PROTO_TCP, 1, &seg)).await;
    }
    let mut synack = 0;
    let mut rst = 0;
    while synack + rst < 3 {
        let (_, _, t, _) = tcp_of(&a.recv().await).unwrap();
        if t.has(wire::TCP_SYN) {
            synack += 1;
        } else if t.has(wire::TCP_RST) {
            assert_eq!(t.dport, 50002);
            rst += 1;
        }
    }
    assert_eq!((synack, rst), (2, 1));
    let st = sw.stats("vm-a").unwrap();
    assert_eq!((st.tcp_connections, st.connection_limit_hits), (2, 1));

    // Отключение порта освобождает соединения.
    sw.disconnect("vm-a");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(sw.stats("vm-a").unwrap().tcp_connections, 0);
}

#[tokio::test]
async fn udp_nat_echo() {
    let srv = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let srv_port = srv.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            let (n, from) = srv.recv_from(&mut buf).await.unwrap();
            let mut r = b"echo:".to_vec();
            r.extend_from_slice(&buf[..n]);
            srv.send_to(&r, from).await.unwrap();
            // И большой ответ, который придётся фрагментировать.
            srv.send_to(&vec![7u8; 3000], from).await.unwrap();
        }
    });
    let sw = NetSwitch::new(test_cfg()).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let pkt = a.udp_to(Ipv4Addr::LOCALHOST, 6000, srv_port, b"ping");
    a.send_ip(GATEWAY_MAC, &pkt).await;
    let f = a.recv().await;
    assert_eq!(udp_of(&f).unwrap(), (srv_port, 6000, b"echo:ping".to_vec()));
    let ip = wire::parse_ipv4(wire::parse_eth(&f).unwrap().payload).unwrap();
    assert_eq!((ip.src, ip.dst), (Ipv4Addr::LOCALHOST, a.ip));
    // Три фрагмента большого ответа.
    let mut got = 0;
    for _ in 0..3 {
        let f = a.recv().await;
        let ip = wire::parse_ipv4(wire::parse_eth(&f).unwrap().payload).unwrap();
        assert!(ip.is_fragment() && f.len() <= 1514);
        got += ip.payload.len();
    }
    assert_eq!(got, 3008);
    let st = sw.stats("vm-a").unwrap();
    assert_eq!((st.udp_flows, st.udp_flows_total), (1, 1));
}

#[tokio::test]
async fn rate_limit_shapes_traffic() {
    let sw = NetSwitch::new(NetConfig { rate_limit_bytes_per_sec: 200_000, ..test_cfg() }).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    let mut b = Vm::connect(&sw, "vm-b", "b").await;
    let t0 = std::time::Instant::now();
    let pkt = a.udp_to(b.ip, 1, 2, &[0u8; 1400]);
    let frame = wire::build_eth(b.mac, a.mac, wire::ETHERTYPE_IPV4, &pkt);
    let n = 200; // ~290 КБ при 200 КБ/с: не меньше ~1 с
    let sender = tokio::spawn(async move {
        for _ in 0..n {
            a.send(&frame).await;
        }
        a
    });
    for _ in 0..n {
        b.recv().await;
    }
    let el = t0.elapsed().as_secs_f64();
    assert!(el > 1.0 && el < 3.0, "{el}");
    let _ = sender.await;
}

/// Рукопожатие гостя (без опций TCP). Возвращает (snd, rcv).
async fn guest_connect(a: &mut Vm, dst: std::net::SocketAddrV4, sport: u16) -> (u32, u32) {
    let isn = 777_000u32;
    let seg = wire::build_tcp(a.ip, sport, *dst.ip(), dst.port(), isn, 0, wire::TCP_SYN, 65535, &[]);
    a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, *dst.ip(), wire::PROTO_TCP, 1, &seg)).await;
    let f = a.recv_match(|f| tcp_of(f).is_some_and(|(_, _, t, _)| t.dport == sport)).await;
    let (_, _, t, _) = tcp_of(&f).unwrap();
    assert_eq!(t.flags & (wire::TCP_SYN | wire::TCP_ACK), wire::TCP_SYN | wire::TCP_ACK, "{t:?}");
    let (snd, rcv) = (isn + 1, t.seq.wrapping_add(1));
    let seg = wire::build_tcp(a.ip, sport, *dst.ip(), dst.port(), snd, rcv, wire::TCP_ACK, 65535, &[]);
    a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, *dst.ip(), wire::PROTO_TCP, 1, &seg)).await;
    (snd, rcv)
}

#[tokio::test]
async fn proxy_connect_uses_sni_checked_against_dns() {
    // Поддельный HTTP-прокси: запоминает строку CONNECT, отвечает 200 и эхом.
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pport = l.local_addr().unwrap().port();
    let (seen_tx, mut seen) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            let seen_tx = seen_tx.clone();
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut b = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if s.read(&mut b).await.unwrap() == 0 {
                        return;
                    }
                    head.push(b[0]);
                }
                let line = String::from_utf8_lossy(&head).lines().next().unwrap().to_string();
                seen_tx.send(line).unwrap();
                s.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.unwrap();
                let (mut r, mut w) = s.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    let ip1 = Ipv4Addr::new(93, 184, 215, 14);
    let ip2 = Ipv4Addr::new(93, 184, 215, 15);
    // Имя, которое резолвер хоста точно знает: localhost -> 127.0.0.1 (в test_cfg
    // открыт allow-списком, так что перепроверку проходит).
    let cfg = NetConfig { upstream_https_proxy: Some(format!("http://127.0.0.1:{pport}")), ..test_cfg() };
    let sw = NetSwitch::new(cfg).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    sw.remember_for_vm("vm-a", "localhost", ip1);
    sw.remember_for_vm("vm-a", "b.example", ip1);

    let cases = [
        // SNI подтверждён DNS шлюза и перепроверкой.
        (ip1, Some("localhost"), "CONNECT localhost:443 HTTP/1.1"),
        // SNI не подтверждён: последнее имя из DNS (b.example), но оно не
        // разрешается на хосте -> по IP.
        (ip1, Some("evil.internal"), "CONNECT 93.184.215.14:443 HTTP/1.1"),
        // Имён для адреса нет: по IP.
        (ip2, Some("x.example"), "CONNECT 93.184.215.15:443 HTTP/1.1"),
    ];
    for (i, (ip, sni, want)) in cases.into_iter().enumerate() {
        let sport = 42000 + i as u16;
        proxy_case(&mut a, &mut seen, ip, sport, Some(crate::nat_tcp::client_hello(sni)), want).await;
    }
    // Сервер, который говорит первым (ClientHello нет): через 2 с — по последнему имени.
    sw.remember_for_vm("vm-a", "localhost", ip2);
    proxy_case(&mut a, &mut seen, ip2, 42100, None, "CONNECT localhost:443 HTTP/1.1").await;

    // Без allow-списка localhost -> 127.0.0.1 запрещён: имя не используется
    // (защита от DNS rebinding через прокси).
    let cfg = NetConfig { upstream_https_proxy: Some(format!("http://127.0.0.1:{pport}")), ..NetConfig::default() };
    let sw = NetSwitch::new(cfg).unwrap();
    let mut a = Vm::connect(&sw, "vm-a", "a").await;
    sw.remember_for_vm("vm-a", "localhost", ip1);
    let hello = crate::nat_tcp::client_hello(Some("localhost"));
    proxy_case(&mut a, &mut seen, ip1, 42200, Some(hello), "CONNECT 93.184.215.14:443 HTTP/1.1").await;
}

async fn proxy_case(
    a: &mut Vm,
    seen: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    ip: Ipv4Addr,
    sport: u16,
    hello: Option<Vec<u8>>,
    want: &str,
) {
    let dst = std::net::SocketAddrV4::new(ip, 443);
    let (snd, mut rcv) = guest_connect(a, dst, sport).await;
    if let Some(hello) = &hello {
        let seg = wire::build_tcp(a.ip, sport, ip, 443, snd, rcv, wire::TCP_ACK | 0x08, 65535, hello);
        a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, ip, wire::PROTO_TCP, 1, &seg)).await;
    }
    let line = tokio::time::timeout(Duration::from_secs(8), seen.recv()).await.unwrap().unwrap();
    assert_eq!(line, want);
    let Some(hello) = hello else { return };
    // Эхо от «сервера» через прокси доходит до гостя.
    let mut got = Vec::new();
    while got.len() < hello.len() {
        let f = a.recv_match(|f| tcp_of(f).is_some_and(|(_, _, t, d)| t.dport == sport && !d.is_empty())).await;
        let (_, _, t, d) = tcp_of(&f).unwrap();
        if t.seq == rcv {
            rcv = rcv.wrapping_add(d.len() as u32);
            got.extend_from_slice(&d);
        }
        let seg = wire::build_tcp(a.ip, sport, ip, 443, snd + hello.len() as u32, rcv, wire::TCP_ACK, 65535, &[]);
        a.send_ip(GATEWAY_MAC, &wire::build_ipv4(a.ip, ip, wire::PROTO_TCP, 1, &seg)).await;
    }
    assert_eq!(got, hello);
}
