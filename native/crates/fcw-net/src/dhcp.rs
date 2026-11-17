//! DHCPv4-сервер (RFC 2131/2132), без состояния: адрес машины заранее известен
//! (выдаётся по MAC), так что сервер только отвечает на сообщения клиента.

use std::net::Ipv4Addr;

use crate::wire::Mac;

pub const SERVER_PORT: u16 = 67;
pub const CLIENT_PORT: u16 = 68;

const MAGIC: [u8; 4] = [99, 130, 83, 99];

pub const DISCOVER: u8 = 1;
pub const OFFER: u8 = 2;
pub const REQUEST: u8 = 3;
#[allow(dead_code)]
pub const DECLINE: u8 = 4;
pub const ACK: u8 = 5;
pub const NAK: u8 = 6;
#[allow(dead_code)]
pub const RELEASE: u8 = 7;
pub const INFORM: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub xid: u32,
    pub flags: u16,
    pub ciaddr: Ipv4Addr,
    pub chaddr: Mac,
    pub msg_type: u8,
    pub requested_ip: Option<Ipv4Addr>,
    pub server_id: Option<Ipv4Addr>,
    pub hostname: Option<String>,
}

impl Message {
    pub fn broadcast_flag(&self) -> bool {
        self.flags & 0x8000 != 0
    }
}

/// Разобрать запрос клиента (op = BOOTREQUEST, htype Ethernet).
pub fn parse(p: &[u8]) -> Option<Message> {
    if p.len() < 240 || p[0] != 1 || p[1] != 1 || p[2] != 6 || p[236..240] != MAGIC {
        return None;
    }
    let mut chaddr = [0u8; 6];
    chaddr.copy_from_slice(&p[28..34]);
    let mut m = Message {
        xid: u32::from_be_bytes([p[4], p[5], p[6], p[7]]),
        flags: u16::from_be_bytes([p[10], p[11]]),
        ciaddr: Ipv4Addr::new(p[12], p[13], p[14], p[15]),
        chaddr,
        msg_type: 0,
        requested_ip: None,
        server_id: None,
        hostname: None,
    };
    let mut i = 240;
    while i < p.len() {
        let code = p[i];
        if code == 0 {
            i += 1;
            continue;
        }
        if code == 255 {
            break;
        }
        if i + 1 >= p.len() {
            return None;
        }
        let len = usize::from(p[i + 1]);
        let v = p.get(i + 2..i + 2 + len)?;
        match (code, len) {
            (53, 1) => m.msg_type = v[0],
            (50, 4) => m.requested_ip = Some(Ipv4Addr::new(v[0], v[1], v[2], v[3])),
            (54, 4) => m.server_id = Some(Ipv4Addr::new(v[0], v[1], v[2], v[3])),
            (12, _) => m.hostname = Some(String::from_utf8_lossy(v).into_owned()),
            _ => {}
        }
        i += 2 + len;
    }
    if m.msg_type == 0 {
        return None;
    }
    Some(m)
}

/// Параметры сети, которые сервер сообщает клиенту.
#[derive(Debug, Clone)]
pub struct Lease<'a> {
    pub ip: Ipv4Addr,
    pub server: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub broadcast: Ipv4Addr,
    pub router: Ipv4Addr,
    pub dns: Ipv4Addr,
    pub domain: &'a str,
    pub hostname: &'a str,
    pub lease_secs: u32,
    pub mtu: u16,
}

/// Ответ сервера: данные UDP и IP-адрес назначения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub msg_type: u8,
    pub payload: Vec<u8>,
    pub dst_ip: Ipv4Addr,
}

/// Решить, что ответить на сообщение клиента. `None` — промолчать.
pub fn respond(req: &Message, lease: &Lease<'_>) -> Option<Reply> {
    let reply = |t: u8| build(req, lease, t);
    match req.msg_type {
        DISCOVER => Some(reply(OFFER)),
        REQUEST => {
            if let Some(sid) = req.server_id {
                // SELECTING: клиент выбрал сервер; если не нас, молчим.
                if sid != lease.server {
                    return None;
                }
                return Some(reply(if req.requested_ip == Some(lease.ip) { ACK } else { NAK }));
            }
            if let Some(rip) = req.requested_ip {
                // INIT-REBOOT
                return Some(reply(if rip == lease.ip { ACK } else { NAK }));
            }
            // RENEWING / REBINDING
            Some(reply(if req.ciaddr == lease.ip { ACK } else { NAK }))
        }
        INFORM => Some(reply(ACK)),
        // DECLINE / RELEASE: адрес закреплён за машиной, менять нечего.
        _ => None,
    }
}

fn opt(v: &mut Vec<u8>, code: u8, data: &[u8]) {
    for chunk in data.chunks(255) {
        v.push(code);
        v.push(chunk.len() as u8);
        v.extend_from_slice(chunk);
    }
}

/// Доменное имя в формате DNS (для опции 119, RFC 3397), без сжатия.
fn dns_name(name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    for label in name.split('.').filter(|l| !l.is_empty()) {
        v.push(label.len().min(63) as u8);
        v.extend_from_slice(&label.as_bytes()[..label.len().min(63)]);
    }
    v.push(0);
    v
}

fn build(req: &Message, l: &Lease<'_>, msg_type: u8) -> Reply {
    let mut v = vec![0u8; 236];
    v[0] = 2; // BOOTREPLY
    v[1] = 1;
    v[2] = 6;
    v[4..8].copy_from_slice(&req.xid.to_be_bytes());
    v[10..12].copy_from_slice(&req.flags.to_be_bytes());
    if msg_type != NAK {
        v[12..16].copy_from_slice(&req.ciaddr.octets());
        if req.msg_type != INFORM {
            v[16..20].copy_from_slice(&l.ip.octets()); // yiaddr
        }
        v[20..24].copy_from_slice(&l.server.octets()); // siaddr
    }
    v[28..34].copy_from_slice(&req.chaddr);
    v.extend_from_slice(&MAGIC);
    opt(&mut v, 53, &[msg_type]);
    opt(&mut v, 54, &l.server.octets());
    if msg_type != NAK {
        if req.msg_type != INFORM {
            opt(&mut v, 51, &l.lease_secs.to_be_bytes());
            opt(&mut v, 58, &(l.lease_secs / 2).to_be_bytes());
            opt(&mut v, 59, &(l.lease_secs / 8 * 7).to_be_bytes());
        }
        opt(&mut v, 1, &l.netmask.octets());
        opt(&mut v, 28, &l.broadcast.octets());
        opt(&mut v, 3, &l.router.octets());
        opt(&mut v, 6, &l.dns.octets());
        opt(&mut v, 26, &l.mtu.to_be_bytes());
        if !l.hostname.is_empty() {
            opt(&mut v, 12, l.hostname.as_bytes());
        }
        if !l.domain.is_empty() {
            opt(&mut v, 15, l.domain.as_bytes());
            opt(&mut v, 119, &dns_name(l.domain));
        }
    } else {
        opt(&mut v, 56, b"wrong address");
    }
    v.push(255);
    if v.len() < 300 {
        v.resize(300, 0);
    }
    // Куда слать (RFC 2131 4.1): NAK и ответы без адреса — на broadcast,
    // продление — на ciaddr, иначе на yiaddr либо broadcast по флагу клиента.
    let dst_ip = if msg_type == NAK {
        Ipv4Addr::BROADCAST
    } else if !req.ciaddr.is_unspecified() {
        req.ciaddr
    } else if req.broadcast_flag() {
        Ipv4Addr::BROADCAST
    } else {
        l.ip
    };
    Reply { msg_type, payload: v, dst_ip }
}

/// Собрать запрос клиента (для тестов).
#[cfg(test)]
pub fn build_request(xid: u32, chaddr: Mac, msg_type: u8, requested: Option<Ipv4Addr>, server: Option<Ipv4Addr>) -> Vec<u8> {
    let mut v = vec![0u8; 236];
    v[0] = 1;
    v[1] = 1;
    v[2] = 6;
    v[4..8].copy_from_slice(&xid.to_be_bytes());
    v[28..34].copy_from_slice(&chaddr);
    v.extend_from_slice(&MAGIC);
    opt(&mut v, 53, &[msg_type]);
    if let Some(r) = requested {
        opt(&mut v, 50, &r.octets());
    }
    if let Some(s) = server {
        opt(&mut v, 54, &s.octets());
    }
    opt(&mut v, 55, &[1, 3, 6, 15, 12, 119]);
    v.push(255);
    v
}

/// Разобрать ответ сервера в (тип, yiaddr, опции) — для тестов.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub fn parse_reply(p: &[u8]) -> Option<(u8, Ipv4Addr, Vec<(u8, Vec<u8>)>)> {
    if p.len() < 240 || p[0] != 2 || p[236..240] != MAGIC {
        return None;
    }
    let yiaddr = Ipv4Addr::new(p[16], p[17], p[18], p[19]);
    let mut opts = Vec::new();
    let mut t = 0;
    let mut i = 240;
    while i < p.len() && p[i] != 255 {
        if p[i] == 0 {
            i += 1;
            continue;
        }
        let len = usize::from(*p.get(i + 1)?);
        let v = p.get(i + 2..i + 2 + len)?.to_vec();
        if p[i] == 53 {
            t = v[0];
        }
        opts.push((p[i], v));
        i += 2 + len;
    }
    Some((t, yiaddr, opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: Mac = [0x52, 0x54, 0x00, 0xaa, 0xbb, 0xcc];

    fn lease() -> Lease<'static> {
        Lease {
            ip: Ipv4Addr::new(10, 42, 7, 9),
            server: Ipv4Addr::new(10, 42, 0, 1),
            netmask: Ipv4Addr::new(255, 255, 0, 0),
            broadcast: Ipv4Addr::new(10, 42, 255, 255),
            router: Ipv4Addr::new(10, 42, 0, 1),
            dns: Ipv4Addr::new(10, 42, 0, 1),
            domain: "mc",
            hostname: "alice-pc",
            lease_secs: 43200,
            mtu: 1500,
        }
    }

    fn get(opts: &[(u8, Vec<u8>)], code: u8) -> Option<Vec<u8>> {
        opts.iter().find(|(c, _)| *c == code).map(|(_, v)| v.clone())
    }

    #[test]
    fn discover_request_ack() {
        let l = lease();
        let d = parse(&build_request(0x1234, MAC, DISCOVER, None, None)).unwrap();
        assert_eq!(d.msg_type, DISCOVER);
        assert_eq!(d.chaddr, MAC);
        let offer = respond(&d, &l).unwrap();
        assert_eq!(offer.msg_type, OFFER);
        assert_eq!(offer.dst_ip, l.ip);
        let (t, yiaddr, opts) = parse_reply(&offer.payload).unwrap();
        assert_eq!(t, OFFER);
        assert_eq!(yiaddr, l.ip);
        assert_eq!(get(&opts, 1).unwrap(), vec![255, 255, 0, 0]);
        assert_eq!(get(&opts, 3).unwrap(), vec![10, 42, 0, 1]);
        assert_eq!(get(&opts, 6).unwrap(), vec![10, 42, 0, 1]);
        assert_eq!(get(&opts, 15).unwrap(), b"mc".to_vec());
        assert_eq!(get(&opts, 119).unwrap(), vec![2, b'm', b'c', 0]);
        assert_eq!(get(&opts, 12).unwrap(), b"alice-pc".to_vec());
        assert_eq!(get(&opts, 51).unwrap(), 43200u32.to_be_bytes().to_vec());
        assert_eq!(get(&opts, 54).unwrap(), vec![10, 42, 0, 1]);
        assert!(offer.payload.len() >= 300);

        let r = parse(&build_request(0x1234, MAC, REQUEST, Some(l.ip), Some(l.server))).unwrap();
        let ack = respond(&r, &l).unwrap();
        assert_eq!(ack.msg_type, ACK);
        let (t, yiaddr, _) = parse_reply(&ack.payload).unwrap();
        assert_eq!((t, yiaddr), (ACK, l.ip));
    }

    #[test]
    fn request_wrong_ip_is_nak_other_server_ignored() {
        let l = lease();
        let r = parse(&build_request(1, MAC, REQUEST, Some(Ipv4Addr::new(192, 168, 1, 50)), None)).unwrap();
        let nak = respond(&r, &l).unwrap();
        assert_eq!(nak.msg_type, NAK);
        assert_eq!(nak.dst_ip, Ipv4Addr::BROADCAST);
        let (_, yiaddr, _) = parse_reply(&nak.payload).unwrap();
        assert!(yiaddr.is_unspecified());

        let r = parse(&build_request(1, MAC, REQUEST, Some(l.ip), Some(Ipv4Addr::new(10, 42, 0, 2)))).unwrap();
        assert!(respond(&r, &l).is_none());
        let r = parse(&build_request(1, MAC, RELEASE, None, None)).unwrap();
        assert!(respond(&r, &l).is_none());
    }

    #[test]
    fn renew_goes_unicast_to_ciaddr() {
        let l = lease();
        let mut raw = build_request(9, MAC, REQUEST, None, None);
        raw[12..16].copy_from_slice(&l.ip.octets());
        let r = parse(&raw).unwrap();
        let ack = respond(&r, &l).unwrap();
        assert_eq!((ack.msg_type, ack.dst_ip), (ACK, l.ip));
    }

    #[test]
    fn broadcast_flag_and_garbage() {
        let l = lease();
        let mut raw = build_request(9, MAC, DISCOVER, None, None);
        raw[10] = 0x80;
        assert_eq!(respond(&parse(&raw).unwrap(), &l).unwrap().dst_ip, Ipv4Addr::BROADCAST);
        assert!(parse(&raw[..200]).is_none());
        let mut bad = raw.clone();
        bad[236] = 0;
        assert!(parse(&bad).is_none());
        // Опция, вылезающая за конец пакета.
        let mut trunc = build_request(9, MAC, DISCOVER, None, None);
        trunc.pop();
        trunc.extend_from_slice(&[12, 50, b'x']);
        assert!(parse(&trunc).is_none());
    }
}
