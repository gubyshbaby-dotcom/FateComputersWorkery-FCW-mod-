//! Разбор и сборка кадров: Ethernet, ARP, IPv4, UDP, TCP (только заголовки), ICMP echo.
//!
//! Всё на срезах байтов, без аллокаций при разборе. Сборка возвращает `Vec<u8>`.

use std::net::Ipv4Addr;

pub type Mac = [u8; 6];

pub const ETH_HDR_LEN: usize = 14;
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const BROADCAST_MAC: Mac = [0xff; 6];

pub const PROTO_ICMP: u8 = 1;
pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
pub const TCP_ACK: u8 = 0x10;

#[inline]
pub fn is_multicast_mac(mac: &Mac) -> bool {
    mac[0] & 1 != 0
}

pub fn mac_to_string(mac: &Mac) -> String {
    format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5])
}

#[inline]
fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

#[inline]
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

#[inline]
fn ip4(b: &[u8]) -> Ipv4Addr {
    Ipv4Addr::new(b[0], b[1], b[2], b[3])
}

// ---------------------------------------------------------------- Ethernet

#[derive(Debug, Clone, Copy)]
pub struct Eth<'a> {
    pub dst: Mac,
    pub src: Mac,
    pub ethertype: u16,
    pub payload: &'a [u8],
}

pub fn parse_eth(frame: &[u8]) -> Option<Eth<'_>> {
    if frame.len() < ETH_HDR_LEN {
        return None;
    }
    let mut dst = [0u8; 6];
    let mut src = [0u8; 6];
    dst.copy_from_slice(&frame[0..6]);
    src.copy_from_slice(&frame[6..12]);
    Some(Eth { dst, src, ethertype: be16(&frame[12..14]), payload: &frame[14..] })
}

pub fn build_eth(dst: Mac, src: Mac, ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(ETH_HDR_LEN + payload.len());
    v.extend_from_slice(&dst);
    v.extend_from_slice(&src);
    v.extend_from_slice(&ethertype.to_be_bytes());
    v.extend_from_slice(payload);
    v
}

// ---------------------------------------------------------------- ARP

pub const ARP_REQUEST: u16 = 1;
pub const ARP_REPLY: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arp {
    pub op: u16,
    pub sha: Mac,
    pub spa: Ipv4Addr,
    pub tha: Mac,
    pub tpa: Ipv4Addr,
}

/// ARP для Ethernet/IPv4 (htype 1, ptype 0x0800, hlen 6, plen 4).
pub fn parse_arp(p: &[u8]) -> Option<Arp> {
    if p.len() < 28 || be16(&p[0..2]) != 1 || be16(&p[2..4]) != ETHERTYPE_IPV4 || p[4] != 6 || p[5] != 4 {
        return None;
    }
    let mut sha = [0u8; 6];
    let mut tha = [0u8; 6];
    sha.copy_from_slice(&p[8..14]);
    tha.copy_from_slice(&p[18..24]);
    Some(Arp { op: be16(&p[6..8]), sha, spa: ip4(&p[14..18]), tha, tpa: ip4(&p[24..28]) })
}

pub fn build_arp(a: &Arp) -> Vec<u8> {
    let mut v = Vec::with_capacity(28);
    v.extend_from_slice(&1u16.to_be_bytes());
    v.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    v.push(6);
    v.push(4);
    v.extend_from_slice(&a.op.to_be_bytes());
    v.extend_from_slice(&a.sha);
    v.extend_from_slice(&a.spa.octets());
    v.extend_from_slice(&a.tha);
    v.extend_from_slice(&a.tpa.octets());
    v
}

// ---------------------------------------------------------------- checksum

/// Сумма 16-битных слов (без свёртки и инверсии).
pub fn csum_partial(mut sum: u32, data: &[u8]) -> u32 {
    let (chunks, rest) = data.as_chunks::<2>();
    for c in chunks {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes(*c)));
    }
    if let [last] = rest {
        sum = sum.wrapping_add(u32::from(*last) << 8);
    }
    sum
}

pub fn csum_finish(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn checksum(data: &[u8]) -> u16 {
    csum_finish(csum_partial(0, data))
}

fn pseudo_header_sum(src: Ipv4Addr, dst: Ipv4Addr, proto: u8, len: usize) -> u32 {
    let mut s = csum_partial(0, &src.octets());
    s = csum_partial(s, &dst.octets());
    s = s.wrapping_add(u32::from(proto));
    s.wrapping_add(len as u32)
}

// ---------------------------------------------------------------- IPv4

#[derive(Debug, Clone, Copy)]
pub struct Ipv4<'a> {
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub proto: u8,
    #[allow(dead_code)]
    pub id: u16,
    /// Флаги + смещение фрагмента (как в заголовке).
    pub frag: u16,
    /// Весь пакет (заголовок + данные), обрезанный по total length.
    pub packet: &'a [u8],
    pub payload: &'a [u8],
}

impl Ipv4<'_> {
    /// Фрагмент (MF или ненулевое смещение).
    pub fn is_fragment(&self) -> bool {
        self.frag & 0x3fff != 0
    }
}

pub fn parse_ipv4(p: &[u8]) -> Option<Ipv4<'_>> {
    if p.len() < 20 || p[0] >> 4 != 4 {
        return None;
    }
    let ihl = usize::from(p[0] & 0x0f) * 4;
    let total = usize::from(be16(&p[2..4]));
    if ihl < 20 || total < ihl || total > p.len() {
        return None;
    }
    if checksum(&p[..ihl]) != 0 {
        return None;
    }
    let packet = &p[..total];
    Some(Ipv4 {
        src: ip4(&p[12..16]),
        dst: ip4(&p[16..20]),
        proto: p[9],
        id: be16(&p[4..6]),
        frag: be16(&p[6..8]),
        packet,
        payload: &packet[ihl..],
    })
}

/// IPv4-пакет с заголовком без опций, флаг DF.
pub fn build_ipv4(src: Ipv4Addr, dst: Ipv4Addr, proto: u8, id: u16, payload: &[u8]) -> Vec<u8> {
    build_ipv4_frag(src, dst, proto, id, 0x4000, payload)
}

fn build_ipv4_frag(src: Ipv4Addr, dst: Ipv4Addr, proto: u8, id: u16, frag: u16, payload: &[u8]) -> Vec<u8> {
    let total = 20 + payload.len();
    let mut v = Vec::with_capacity(total);
    v.push(0x45);
    v.push(0);
    v.extend_from_slice(&(total as u16).to_be_bytes());
    v.extend_from_slice(&id.to_be_bytes());
    v.extend_from_slice(&frag.to_be_bytes());
    v.push(64);
    v.push(proto);
    v.extend_from_slice(&[0, 0]);
    v.extend_from_slice(&src.octets());
    v.extend_from_slice(&dst.octets());
    let c = checksum(&v[..20]);
    v[10..12].copy_from_slice(&c.to_be_bytes());
    v.extend_from_slice(payload);
    v
}

/// Разбить IP-данные на фрагменты под MTU (для больших UDP-ответов гостю).
pub fn build_ipv4_fragments(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    proto: u8,
    id: u16,
    payload: &[u8],
    mtu: usize,
) -> Vec<Vec<u8>> {
    if 20 + payload.len() <= mtu {
        return vec![build_ipv4(src, dst, proto, id, payload)];
    }
    let chunk = ((mtu - 20) / 8) * 8;
    let mut out = Vec::new();
    let mut off = 0;
    while off < payload.len() {
        let end = (off + chunk).min(payload.len());
        let more = if end < payload.len() { 0x2000 } else { 0 };
        out.push(build_ipv4_frag(src, dst, proto, id, more | (off / 8) as u16, &payload[off..end]));
        off = end;
    }
    out
}

// ---------------------------------------------------------------- UDP

#[derive(Debug, Clone, Copy)]
pub struct Udp<'a> {
    pub sport: u16,
    pub dport: u16,
    pub payload: &'a [u8],
}

pub fn parse_udp<'a>(ip: &Ipv4<'a>) -> Option<Udp<'a>> {
    let p = ip.payload;
    if ip.proto != PROTO_UDP || ip.is_fragment() || p.len() < 8 {
        return None;
    }
    let len = usize::from(be16(&p[4..6]));
    if len < 8 || len > p.len() {
        return None;
    }
    let ck = be16(&p[6..8]);
    if ck != 0 {
        let s = pseudo_header_sum(ip.src, ip.dst, PROTO_UDP, len);
        if csum_finish(csum_partial(s, &p[..len])) != 0 {
            return None;
        }
    }
    Some(Udp { sport: be16(&p[0..2]), dport: be16(&p[2..4]), payload: &p[8..len] })
}

/// UDP-датаграмма (заголовок + данные) с контрольной суммой.
pub fn build_udp(src: Ipv4Addr, sport: u16, dst: Ipv4Addr, dport: u16, payload: &[u8]) -> Vec<u8> {
    let len = 8 + payload.len();
    let mut v = Vec::with_capacity(len);
    v.extend_from_slice(&sport.to_be_bytes());
    v.extend_from_slice(&dport.to_be_bytes());
    v.extend_from_slice(&(len as u16).to_be_bytes());
    v.extend_from_slice(&[0, 0]);
    v.extend_from_slice(payload);
    let s = pseudo_header_sum(src, dst, PROTO_UDP, len);
    let mut c = csum_finish(csum_partial(s, &v));
    if c == 0 {
        c = 0xffff;
    }
    v[6..8].copy_from_slice(&c.to_be_bytes());
    v
}

// ---------------------------------------------------------------- TCP

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tcp {
    pub sport: u16,
    pub dport: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    /// Длина данных сегмента.
    pub data_len: usize,
}

impl Tcp {
    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
    /// Первый SYN соединения (без ACK).
    pub fn is_syn_only(&self) -> bool {
        self.has(TCP_SYN) && !self.has(TCP_ACK) && !self.has(TCP_RST)
    }
}

/// Заголовок TCP (контрольная сумма не проверяется: это сделает smoltcp).
pub fn parse_tcp(ip: &Ipv4<'_>) -> Option<Tcp> {
    let p = ip.payload;
    if ip.proto != PROTO_TCP || ip.is_fragment() || p.len() < 20 {
        return None;
    }
    let off = usize::from(p[12] >> 4) * 4;
    if off < 20 || off > p.len() {
        return None;
    }
    Some(Tcp {
        sport: be16(&p[0..2]),
        dport: be16(&p[2..4]),
        seq: be32(&p[4..8]),
        ack: be32(&p[8..12]),
        flags: p[13],
        data_len: p.len() - off,
    })
}

/// Ответ RST на сегмент `t` из пакета `ip` (RFC 793, «reset generation»).
/// `None`, если сам сегмент — RST.
pub fn build_tcp_rst_reply(ip: &Ipv4<'_>, t: &Tcp, ip_id: u16) -> Option<Vec<u8>> {
    if t.has(TCP_RST) {
        return None;
    }
    let (seq, ack, flags) = if t.has(TCP_ACK) {
        (t.ack, 0, TCP_RST)
    } else {
        let mut seg_len = t.data_len as u32;
        if t.has(TCP_SYN) {
            seg_len += 1;
        }
        if t.has(TCP_FIN) {
            seg_len += 1;
        }
        (0, t.seq.wrapping_add(seg_len), TCP_RST | TCP_ACK)
    };
    let seg = build_tcp(ip.dst, t.dport, ip.src, t.sport, seq, ack, flags, 0, &[]);
    Some(build_ipv4(ip.dst, ip.src, PROTO_TCP, ip_id, &seg))
}

/// TCP-сегмент без опций (заголовок + данные) с контрольной суммой.
#[allow(clippy::too_many_arguments)]
pub fn build_tcp(
    src: Ipv4Addr,
    sport: u16,
    dst: Ipv4Addr,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut h = vec![0u8; 20];
    h[0..2].copy_from_slice(&sport.to_be_bytes());
    h[2..4].copy_from_slice(&dport.to_be_bytes());
    h[4..8].copy_from_slice(&seq.to_be_bytes());
    h[8..12].copy_from_slice(&ack.to_be_bytes());
    h[12] = 5 << 4;
    h[13] = flags;
    h[14..16].copy_from_slice(&window.to_be_bytes());
    h.extend_from_slice(payload);
    let s = pseudo_header_sum(src, dst, PROTO_TCP, h.len());
    let c = csum_finish(csum_partial(s, &h));
    h[16..18].copy_from_slice(&c.to_be_bytes());
    h
}

/// Проверить контрольную сумму TCP-сегмента.
#[cfg(test)]
pub fn tcp_checksum_ok(ip: &Ipv4<'_>) -> bool {
    let s = pseudo_header_sum(ip.src, ip.dst, PROTO_TCP, ip.payload.len());
    csum_finish(csum_partial(s, ip.payload)) == 0
}

// ---------------------------------------------------------------- ICMP

/// Ответ на ICMP echo request (тот же id/seq/данные). `None`, если это не echo request.
pub fn build_icmp_echo_reply(ip: &Ipv4<'_>, ip_id: u16) -> Option<Vec<u8>> {
    let p = ip.payload;
    if ip.proto != PROTO_ICMP || ip.is_fragment() || p.len() < 8 || p[0] != 8 || p[1] != 0 {
        return None;
    }
    if checksum(p) != 0 {
        return None;
    }
    let mut r = p.to_vec();
    r[0] = 0;
    r[2] = 0;
    r[3] = 0;
    let c = checksum(&r);
    r[2..4].copy_from_slice(&c.to_be_bytes());
    Some(build_ipv4(ip.dst, ip.src, PROTO_ICMP, ip_id, &r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_rfc1071_example() {
        // Пример из RFC 1071: 00 01 f2 03 f4 f5 f6 f7 -> сумма ddf2, контрольная 220d.
        let d = [0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
        assert_eq!(checksum(&d), 0x220d);
        // Нечётная длина.
        assert_eq!(checksum(&[0xff]), !0xff00u16);
    }

    #[test]
    fn eth_roundtrip() {
        let f = build_eth([1, 2, 3, 4, 5, 6], [7, 8, 9, 10, 11, 12], ETHERTYPE_ARP, &[0xaa; 28]);
        let e = parse_eth(&f).unwrap();
        assert_eq!(e.dst, [1, 2, 3, 4, 5, 6]);
        assert_eq!(e.src, [7, 8, 9, 10, 11, 12]);
        assert_eq!(e.ethertype, ETHERTYPE_ARP);
        assert_eq!(e.payload.len(), 28);
        assert!(parse_eth(&f[..13]).is_none());
        assert!(is_multicast_mac(&BROADCAST_MAC));
        assert!(is_multicast_mac(&[0x01, 0x00, 0x5e, 0, 0, 0xfb]));
        assert!(!is_multicast_mac(&[0x52, 0x54, 0, 1, 2, 3]));
    }

    #[test]
    fn arp_roundtrip() {
        let a = Arp {
            op: ARP_REQUEST,
            sha: [0x52, 0x54, 0, 1, 2, 3],
            spa: Ipv4Addr::new(10, 42, 1, 5),
            tha: [0; 6],
            tpa: Ipv4Addr::new(10, 42, 0, 1),
        };
        let b = build_arp(&a);
        assert_eq!(b.len(), 28);
        assert_eq!(parse_arp(&b), Some(a));
        let mut bad = b.clone();
        bad[4] = 8; // hlen
        assert!(parse_arp(&bad).is_none());
    }

    #[test]
    fn ipv4_udp_roundtrip_and_validation() {
        let src = Ipv4Addr::new(10, 42, 1, 5);
        let dst = Ipv4Addr::new(10, 42, 0, 1);
        let udp = build_udp(src, 5353, dst, 53, b"hello");
        let pkt = build_ipv4(src, dst, PROTO_UDP, 7, &udp);
        let ip = parse_ipv4(&pkt).unwrap();
        assert_eq!((ip.src, ip.dst, ip.proto, ip.id), (src, dst, PROTO_UDP, 7));
        assert!(!ip.is_fragment());
        let u = parse_udp(&ip).unwrap();
        assert_eq!((u.sport, u.dport, u.payload), (5353, 53, &b"hello"[..]));

        // Испорченная контрольная сумма UDP.
        let mut bad = pkt.clone();
        let n = bad.len();
        bad[n - 1] ^= 0xff;
        assert!(parse_udp(&parse_ipv4(&bad).unwrap()).is_none());
        // Испорченный заголовок IP.
        let mut bad = pkt.clone();
        bad[8] = 1;
        assert!(parse_ipv4(&bad).is_none());
        // Обрезанный пакет.
        assert!(parse_ipv4(&pkt[..pkt.len() - 1]).is_none());
        // Хвостовые байты (паддинг Ethernet) отрезаются по total length.
        let mut padded = pkt.clone();
        padded.extend_from_slice(&[0; 10]);
        assert_eq!(parse_ipv4(&padded).unwrap().packet.len(), pkt.len());
    }

    #[test]
    fn fragments() {
        let src = Ipv4Addr::new(1, 1, 1, 1);
        let dst = Ipv4Addr::new(10, 42, 1, 5);
        let data = vec![0x5a; 3000];
        let frags = build_ipv4_fragments(src, dst, PROTO_UDP, 9, &data, 1500);
        assert_eq!(frags.len(), 3);
        let mut total = 0;
        for (i, f) in frags.iter().enumerate() {
            let ip = parse_ipv4(f).unwrap();
            assert!(f.len() <= 1500);
            assert!(ip.is_fragment());
            assert_eq!(usize::from(ip.frag & 0x1fff) * 8, total);
            assert_eq!(ip.frag & 0x2000 != 0, i + 1 < frags.len());
            total += ip.payload.len();
        }
        assert_eq!(total, 3000);
    }

    #[test]
    fn tcp_rst_reply_for_syn() {
        let src = Ipv4Addr::new(10, 42, 1, 5);
        let dst = Ipv4Addr::new(10, 0, 0, 1);
        let h = build_tcp(src, 40000, dst, 80, 1000, 0, TCP_SYN, 64240, &[]);
        let pkt = build_ipv4(src, dst, PROTO_TCP, 1, &h);
        let ip = parse_ipv4(&pkt).unwrap();
        let t = parse_tcp(&ip).unwrap();
        assert!(t.is_syn_only());
        let rst = build_tcp_rst_reply(&ip, &t, 2).unwrap();
        let rip = parse_ipv4(&rst).unwrap();
        assert_eq!((rip.src, rip.dst), (dst, src));
        let rt = parse_tcp(&rip).unwrap();
        assert_eq!((rt.sport, rt.dport), (80, 40000));
        assert_eq!(rt.flags, TCP_RST | TCP_ACK);
        assert_eq!(rt.ack, 1001);
        assert!(tcp_checksum_ok(&rip));
        // На RST не отвечаем.
        let rip2 = parse_ipv4(&rst).unwrap();
        assert!(build_tcp_rst_reply(&rip2, &parse_tcp(&rip2).unwrap(), 3).is_none());
    }

    #[test]
    fn icmp_echo() {
        let src = Ipv4Addr::new(10, 42, 1, 5);
        let gw = Ipv4Addr::new(10, 42, 0, 1);
        let mut icmp = vec![8, 0, 0, 0, 0x12, 0x34, 0, 1, b'p', b'i', b'n', b'g'];
        let c = checksum(&icmp);
        icmp[2..4].copy_from_slice(&c.to_be_bytes());
        let pkt = build_ipv4(src, gw, PROTO_ICMP, 1, &icmp);
        let ip = parse_ipv4(&pkt).unwrap();
        let r = build_icmp_echo_reply(&ip, 5).unwrap();
        let rip = parse_ipv4(&r).unwrap();
        assert_eq!((rip.src, rip.dst), (gw, src));
        assert_eq!(rip.payload[0], 0);
        assert_eq!(&rip.payload[4..], &icmp[4..]);
        assert_eq!(checksum(rip.payload), 0);
    }
}
