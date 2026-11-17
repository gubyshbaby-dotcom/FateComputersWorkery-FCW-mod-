//! Небольшой DNS-сервер шлюза: разбор запроса, решение «ответить самому или
//! спросить систему хоста», сборка ответа. Только то, что нужно гостям:
//! A для имён зоны и внешних имён, PTR для своей подсети, на AAAA и прочие
//! типы — пустой ответ (NODATA).

use std::net::Ipv4Addr;

use ipnet::Ipv4Net;

pub const PORT: u16 = 53;

pub const TYPE_A: u16 = 1;
pub const TYPE_PTR: u16 = 12;
#[allow(dead_code)]
pub const TYPE_AAAA: u16 = 28;
pub const TYPE_OPT: u16 = 41;
pub const TYPE_ANY: u16 = 255;
pub const CLASS_IN: u16 = 1;
pub const CLASS_ANY: u16 = 255;

pub const NOERROR: u8 = 0;
pub const FORMERR: u8 = 1;
pub const SERVFAIL: u8 = 2;
pub const NXDOMAIN: u8 = 3;
pub const NOTIMP: u8 = 4;
pub const REFUSED: u8 = 5;

/// TTL ответов о машинах своей зоны (адреса стабильны, но машины выключаются).
pub const ZONE_TTL: u32 = 10;

#[derive(Debug, Clone)]
pub struct Query<'a> {
    pub id: u16,
    pub opcode: u8,
    pub rd: bool,
    /// Имя в нижнем регистре, без точки в конце; "" — корень.
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
    /// Секция вопроса как есть (имя + тип + класс), копируется в ответ.
    pub question: &'a [u8],
    /// Размер UDP из EDNS(0), если запрос с OPT.
    pub edns_payload: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Не запрос или мусор: не отвечать.
    Ignore,
    /// Ответить заголовком с кодом ошибки.
    Error { id: u16, opcode: u8, rcode: u8 },
}

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

/// Прочитать имя без сжатия. Возвращает (имя, смещение после имени).
fn read_name(msg: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut total = 0;
    loop {
        let len = usize::from(*msg.get(i)?);
        if len == 0 {
            return Some((name, i + 1));
        }
        if len & 0xc0 != 0 {
            return None; // сжатие в вопросе не поддерживаем
        }
        let label = msg.get(i + 1..i + 1 + len)?;
        total += len + 1;
        if total > 255 {
            return None;
        }
        if !name.is_empty() {
            name.push('.');
        }
        for &b in label {
            name.push(char::from(b.to_ascii_lowercase()));
        }
        i += 1 + len;
    }
}

pub fn parse_query(msg: &[u8]) -> Result<Query<'_>, ParseError> {
    if msg.len() < 12 {
        return Err(ParseError::Ignore);
    }
    let id = be16(&msg[0..2]);
    let flags = be16(&msg[2..4]);
    if flags & 0x8000 != 0 {
        return Err(ParseError::Ignore); // это ответ
    }
    let opcode = ((flags >> 11) & 0x0f) as u8;
    let err = |rcode| ParseError::Error { id, opcode, rcode };
    if opcode != 0 {
        return Err(err(NOTIMP));
    }
    let qd = be16(&msg[4..6]);
    let an = be16(&msg[6..8]);
    let ns = be16(&msg[8..10]);
    let ar = be16(&msg[10..12]);
    if qd != 1 || an != 0 || ns != 0 {
        return Err(err(FORMERR));
    }
    let (name, end) = read_name(msg, 12).ok_or(err(FORMERR))?;
    if end + 4 > msg.len() {
        return Err(err(FORMERR));
    }
    let qtype = be16(&msg[end..end + 2]);
    let qclass = be16(&msg[end + 2..end + 4]);
    let question = &msg[12..end + 4];
    let mut edns_payload = None;
    if ar >= 1 {
        // Первая запись дополнительной секции: ищем OPT (имя — корень).
        let i = end + 4;
        if msg.get(i) == Some(&0) && i + 11 <= msg.len() && be16(&msg[i + 1..i + 3]) == TYPE_OPT {
            edns_payload = Some(be16(&msg[i + 3..i + 5]).max(512));
        }
    }
    Ok(Query { id, opcode, rd: flags & 0x0100 != 0, name, qtype, qclass, question, edns_payload })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RData {
    A(Ipv4Addr),
    Ptr(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub ttl: u32,
    pub data: RData,
}

fn encode_name(v: &mut Vec<u8>, name: &str) {
    for label in name.split('.').filter(|l| !l.is_empty()) {
        let b = &label.as_bytes()[..label.len().min(63)];
        v.push(b.len() as u8);
        v.extend_from_slice(b);
    }
    v.push(0);
}

#[allow(clippy::too_many_arguments)]
fn header(id: u16, opcode: u8, rd: bool, aa: bool, rcode: u8, qd: u16, an: u16, ar: u16) -> Vec<u8> {
    let mut flags: u16 = 0x8000 | (u16::from(opcode) << 11) | 0x0080 | u16::from(rcode & 0x0f);
    if aa {
        flags |= 0x0400;
    }
    if rd {
        flags |= 0x0100;
    }
    let mut v = Vec::with_capacity(512);
    v.extend_from_slice(&id.to_be_bytes());
    v.extend_from_slice(&flags.to_be_bytes());
    v.extend_from_slice(&qd.to_be_bytes());
    v.extend_from_slice(&an.to_be_bytes());
    v.extend_from_slice(&0u16.to_be_bytes());
    v.extend_from_slice(&ar.to_be_bytes());
    v
}

/// Ответ-ошибка без секции вопроса (для неразобранных запросов).
pub fn build_error(id: u16, opcode: u8, rcode: u8) -> Vec<u8> {
    header(id, opcode, false, false, rcode, 0, 0, 0)
}

/// Собрать ответ на запрос. Записи, не влезающие в размер UDP, отбрасываются.
pub fn build_response(q: &Query<'_>, rcode: u8, aa: bool, answers: &[Answer]) -> Vec<u8> {
    let limit = usize::from(q.edns_payload.map_or(512, |p| p.min(1232)));
    let opt_len = if q.edns_payload.is_some() { 11 } else { 0 };
    let mut body = Vec::new();
    let mut count = 0u16;
    for a in answers {
        let mut rr = vec![0xc0, 0x0c]; // указатель на имя в вопросе
        let (t, rdata) = match &a.data {
            RData::A(ip) => (TYPE_A, ip.octets().to_vec()),
            RData::Ptr(n) => {
                let mut d = Vec::new();
                encode_name(&mut d, n);
                (TYPE_PTR, d)
            }
        };
        rr.extend_from_slice(&t.to_be_bytes());
        rr.extend_from_slice(&CLASS_IN.to_be_bytes());
        rr.extend_from_slice(&a.ttl.to_be_bytes());
        rr.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        rr.extend_from_slice(&rdata);
        if 12 + q.question.len() + body.len() + rr.len() + opt_len > limit {
            break;
        }
        body.extend_from_slice(&rr);
        count += 1;
    }
    let mut v = header(q.id, q.opcode, q.rd, aa, rcode, 1, count, u16::from(q.edns_payload.is_some()));
    v.extend_from_slice(q.question);
    v.extend_from_slice(&body);
    if q.edns_payload.is_some() {
        // OPT: имя корень, тип 41, класс = наш размер UDP, TTL 0 (без DO), без данных.
        v.push(0);
        v.extend_from_slice(&TYPE_OPT.to_be_bytes());
        v.extend_from_slice(&1232u16.to_be_bytes());
        v.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    }
    v
}

/// Что делать с запросом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Answer { rcode: u8, aa: bool, answers: Vec<Answer> },
    /// Спросить у системы хоста A-записи этого имени.
    Forward(String),
}

fn nodata(aa: bool) -> Decision {
    Decision::Answer { rcode: NOERROR, aa, answers: vec![] }
}

fn nxdomain(aa: bool) -> Decision {
    Decision::Answer { rcode: NXDOMAIN, aa, answers: vec![] }
}

/// Имена, которые нельзя отдавать резолверу хоста: однословные (поиск по
/// доменам хоста раскрыл бы его локальную сеть), mDNS и служебные зоны.
fn is_local_only(name: &str) -> bool {
    !name.contains('.')
        || name == "localhost"
        || [".localhost", ".local", ".internal", ".home.arpa", ".arpa"].iter().any(|s| name.ends_with(s))
}

fn parse_reverse_v4(name: &str) -> Option<Ipv4Addr> {
    let rest = name.strip_suffix(".in-addr.arpa")?;
    let parts: Vec<&str> = rest.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut o = [0u8; 4];
    for (i, p) in parts.iter().rev().enumerate() {
        o[i] = p.parse().ok()?;
    }
    Some(Ipv4Addr::from(o))
}

/// Решить, как ответить на запрос.
///
/// `machine_ip(label)` — адрес машины с таким именем в зоне,
/// `machine_name(ip)` — имя машины по адресу (без зоны).
pub fn decide(
    q: &Query<'_>,
    zone: &str,
    subnet: Ipv4Net,
    gateway: Ipv4Addr,
    machine_ip: impl Fn(&str) -> Option<Ipv4Addr>,
    machine_name: impl Fn(Ipv4Addr) -> Option<String>,
) -> Decision {
    if q.qclass != CLASS_IN && q.qclass != CLASS_ANY {
        return Decision::Answer { rcode: REFUSED, aa: false, answers: vec![] };
    }
    let want_a = q.qtype == TYPE_A || q.qtype == TYPE_ANY;
    let name = q.name.as_str();

    // Своя зона: <имя>.<zone>, gateway.<zone>.
    if !zone.is_empty() {
        if name == zone {
            return nodata(true);
        }
        if let Some(label) = name.strip_suffix(zone).and_then(|n| n.strip_suffix('.')) {
            let ip = if label == "gateway" { Some(gateway) } else { machine_ip(label) };
            return match ip {
                Some(ip) if want_a => Decision::Answer {
                    rcode: NOERROR,
                    aa: true,
                    answers: vec![Answer { ttl: ZONE_TTL, data: RData::A(ip) }],
                },
                Some(_) => nodata(true),
                None => nxdomain(true),
            };
        }
    }

    // Обратная зона своей подсети.
    if let Some(ip) = parse_reverse_v4(name) {
        if subnet.contains(&ip) {
            let host = if ip == gateway { Some("gateway".to_string()) } else { machine_name(ip) };
            return match host {
                Some(h) if q.qtype == TYPE_PTR || q.qtype == TYPE_ANY => {
                    let fqdn = if zone.is_empty() { h } else { format!("{h}.{zone}") };
                    Decision::Answer {
                        rcode: NOERROR,
                        aa: true,
                        answers: vec![Answer { ttl: ZONE_TTL, data: RData::Ptr(fqdn) }],
                    }
                }
                Some(_) => nodata(true),
                None => nxdomain(true),
            };
        }
        return nxdomain(false);
    }

    if name.is_empty() {
        return nodata(false);
    }
    if is_local_only(name) {
        return nxdomain(false);
    }
    if want_a {
        Decision::Forward(name.to_string())
    } else {
        // AAAA и прочее: IPv6 гостям не нужен, остальные типы через системный
        // резолвер не получить.
        nodata(false)
    }
}

// ---------------------------------------------------------------- для тестов

/// Собрать запрос (для тестов): `edns` — добавить OPT.
#[cfg(test)]
pub fn build_query(id: u16, name: &str, qtype: u16, edns: bool) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&id.to_be_bytes());
    v.extend_from_slice(&0x0100u16.to_be_bytes());
    v.extend_from_slice(&1u16.to_be_bytes());
    v.extend_from_slice(&0u16.to_be_bytes());
    v.extend_from_slice(&0u16.to_be_bytes());
    v.extend_from_slice(&u16::from(edns).to_be_bytes());
    encode_name(&mut v, name);
    v.extend_from_slice(&qtype.to_be_bytes());
    v.extend_from_slice(&CLASS_IN.to_be_bytes());
    if edns {
        v.push(0);
        v.extend_from_slice(&TYPE_OPT.to_be_bytes());
        v.extend_from_slice(&4096u16.to_be_bytes());
        v.extend_from_slice(&[0, 0, 0x80, 0, 0, 0]); // DO
    }
    v
}

/// Разобрать ответ (для тестов): (id, rcode, aa, ответы A/PTR).
#[cfg(test)]
pub fn parse_response(msg: &[u8]) -> Option<(u16, u8, bool, Vec<RData>)> {
    if msg.len() < 12 || msg[2] & 0x80 == 0 {
        return None;
    }
    let id = be16(&msg[0..2]);
    let rcode = msg[3] & 0x0f;
    let aa = msg[2] & 0x04 != 0;
    let qd = be16(&msg[4..6]);
    let an = be16(&msg[6..8]);
    let mut i = 12;
    for _ in 0..qd {
        let (_, e) = read_name(msg, i)?;
        i = e + 4;
    }
    let mut out = Vec::new();
    for _ in 0..an {
        // имя: указатель или обычное
        if msg.get(i)? & 0xc0 == 0xc0 {
            i += 2;
        } else {
            let (_, e) = read_name(msg, i)?;
            i = e;
        }
        let t = be16(msg.get(i..i + 2)?);
        let len = usize::from(be16(msg.get(i + 8..i + 10)?));
        let d = msg.get(i + 10..i + 10 + len)?;
        match t {
            TYPE_A if len == 4 => out.push(RData::A(Ipv4Addr::new(d[0], d[1], d[2], d[3]))),
            TYPE_PTR => out.push(RData::Ptr(read_name(msg, i + 10)?.0)),
            _ => {}
        }
        i += 10 + len;
    }
    Some((id, rcode, aa, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subnet() -> Ipv4Net {
        "10.42.0.0/16".parse().unwrap()
    }
    const GW: Ipv4Addr = Ipv4Addr::new(10, 42, 0, 1);

    fn run(name: &str, qtype: u16) -> Decision {
        let raw = build_query(7, name, qtype, false);
        let q = parse_query(&raw).unwrap();
        decide(
            &q,
            "mc",
            subnet(),
            GW,
            |l| (l == "alice").then(|| Ipv4Addr::new(10, 42, 3, 4)),
            |ip| (ip == Ipv4Addr::new(10, 42, 3, 4)).then(|| "alice".to_string()),
        )
    }

    #[test]
    fn parse_basic_and_case() {
        let raw = build_query(0xbeef, "WWW.Example.COM", TYPE_A, true);
        let q = parse_query(&raw).unwrap();
        assert_eq!(q.id, 0xbeef);
        assert_eq!(q.name, "www.example.com");
        assert_eq!(q.qtype, TYPE_A);
        assert!(q.rd);
        assert_eq!(q.edns_payload, Some(4096));
        assert_eq!(q.question.len(), raw.len() - 12 - 11);
    }

    #[test]
    fn parse_errors() {
        assert_eq!(parse_query(&[0; 5]).unwrap_err(), ParseError::Ignore);
        let mut resp = build_query(1, "a.b", TYPE_A, false);
        resp[2] |= 0x80;
        assert_eq!(parse_query(&resp).unwrap_err(), ParseError::Ignore);
        let mut two = build_query(1, "a.b", TYPE_A, false);
        two[5] = 2;
        assert!(matches!(parse_query(&two).unwrap_err(), ParseError::Error { rcode: FORMERR, .. }));
        let mut update = build_query(1, "a.b", TYPE_A, false);
        update[2] |= 5 << 3;
        assert!(matches!(parse_query(&update).unwrap_err(), ParseError::Error { rcode: NOTIMP, .. }));
        let trunc = build_query(1, "abc.def", TYPE_A, false);
        assert!(matches!(parse_query(&trunc[..trunc.len() - 3]).unwrap_err(), ParseError::Error { rcode: FORMERR, .. }));
        let mut ptr = build_query(1, "abc.def", TYPE_A, false);
        ptr[12] = 0xc0;
        assert!(parse_query(&ptr).is_err());
    }

    #[test]
    fn zone_answers() {
        let a = Answer { ttl: ZONE_TTL, data: RData::A(Ipv4Addr::new(10, 42, 3, 4)) };
        assert_eq!(run("alice.mc", TYPE_A), Decision::Answer { rcode: NOERROR, aa: true, answers: vec![a] });
        assert_eq!(
            run("gateway.mc", TYPE_A),
            Decision::Answer { rcode: NOERROR, aa: true, answers: vec![Answer { ttl: ZONE_TTL, data: RData::A(GW) }] }
        );
        assert_eq!(run("alice.mc", TYPE_AAAA), Decision::Answer { rcode: NOERROR, aa: true, answers: vec![] });
        assert_eq!(run("bob.mc", TYPE_A), Decision::Answer { rcode: NXDOMAIN, aa: true, answers: vec![] });
        assert_eq!(run("mc", TYPE_A), Decision::Answer { rcode: NOERROR, aa: true, answers: vec![] });
    }

    #[test]
    fn reverse_answers() {
        assert_eq!(
            run("4.3.42.10.in-addr.arpa", TYPE_PTR),
            Decision::Answer {
                rcode: NOERROR,
                aa: true,
                answers: vec![Answer { ttl: ZONE_TTL, data: RData::Ptr("alice.mc".into()) }]
            }
        );
        assert_eq!(
            run("1.0.42.10.in-addr.arpa", TYPE_PTR),
            Decision::Answer {
                rcode: NOERROR,
                aa: true,
                answers: vec![Answer { ttl: ZONE_TTL, data: RData::Ptr("gateway.mc".into()) }]
            }
        );
        assert!(matches!(run("5.3.42.10.in-addr.arpa", TYPE_PTR), Decision::Answer { rcode: NXDOMAIN, .. }));
        assert!(matches!(run("1.1.168.192.in-addr.arpa", TYPE_PTR), Decision::Answer { rcode: NXDOMAIN, .. }));
    }

    #[test]
    fn forwarding_rules() {
        assert_eq!(run("example.com", TYPE_A), Decision::Forward("example.com".into()));
        assert_eq!(run("example.com", TYPE_AAAA), Decision::Answer { rcode: NOERROR, aa: false, answers: vec![] });
        assert_eq!(run("example.com", 15), Decision::Answer { rcode: NOERROR, aa: false, answers: vec![] });
        for n in ["nas", "localhost", "printer.local", "db.internal", "router.home.arpa", "x.ip6.arpa"] {
            assert!(matches!(run(n, TYPE_A), Decision::Answer { rcode: NXDOMAIN, .. }), "{n}");
        }
    }

    #[test]
    fn response_roundtrip_and_truncation() {
        let raw = build_query(0x55, "many.example", TYPE_A, false);
        let q = parse_query(&raw).unwrap();
        let answers: Vec<Answer> =
            (0..100).map(|i| Answer { ttl: 30, data: RData::A(Ipv4Addr::new(1, 2, 3, i)) }).collect();
        let r = build_response(&q, NOERROR, false, &answers);
        assert!(r.len() <= 512);
        let (id, rcode, aa, rd) = parse_response(&r).unwrap();
        assert_eq!((id, rcode, aa), (0x55, NOERROR, false));
        assert!(rd.len() > 10 && rd.len() < 100);
        assert_eq!(rd[0], RData::A(Ipv4Addr::new(1, 2, 3, 0)));

        // С EDNS в ответе есть OPT и больше места.
        let raw = build_query(0x56, "many.example", TYPE_A, true);
        let q = parse_query(&raw).unwrap();
        let r = build_response(&q, NOERROR, false, &answers);
        assert!(r.len() > 512 && r.len() <= 1232);
        assert_eq!(u16::from_be_bytes([r[10], r[11]]), 1);
        assert_eq!(&r[r.len() - 11..r.len() - 8], &[0, 0, 41]);

        let e = build_error(9, 0, SERVFAIL);
        assert_eq!(parse_response(&e).unwrap(), (9, SERVFAIL, false, vec![]));
    }
}
