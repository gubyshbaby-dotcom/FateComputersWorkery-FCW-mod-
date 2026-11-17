//! Крошечный JSON (разбор и вывод) — чтобы агент и программы обходились без зависимостей.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Число по ключу; нет ключа или не число — 0.
    pub fn num(&self, key: &str) -> f64 {
        self.get(key).and_then(Json::as_f64).unwrap_or(0.0)
    }
    /// Строка по ключу; нет ключа или не строка — пусто.
    pub fn text(&self, key: &str) -> &str {
        self.get(key).and_then(Json::as_str).unwrap_or("")
    }
    pub fn flag(&self, key: &str) -> bool {
        self.get(key).and_then(Json::as_bool).unwrap_or(false)
    }
    /// Элементы массива по ключу; нет ключа или не массив — пусто.
    pub fn items(&self, key: &str) -> &[Json] {
        match self.get(key) {
            Some(Json::Arr(a)) => a,
            _ => &[],
        }
    }
    /// Поля объекта по ключу; нет ключа или не объект — пусто.
    pub fn fields(&self, key: &str) -> &[(String, Json)] {
        match self.get(key) {
            Some(Json::Obj(o)) => o,
            _ => &[],
        }
    }

    pub fn parse(s: &str) -> Option<Json> {
        let mut p = Parser { b: s.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        (p.i == p.b.len()).then_some(v)
    }
}

pub fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub fn s(v: &str) -> Json {
    Json::Str(v.to_string())
}

pub fn n(v: impl Into<f64>) -> Json {
    Json::Num(v.into())
}

fn escape(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_json(&mut out, self);
        f.write_str(&out)
    }
}

fn write_json(out: &mut String, v: &Json) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                let _ = write!(out, "{}", *n as i64);
            } else {
                let _ = write!(out, "{n}");
            }
        }
        Json::Str(s) => escape(out, s),
        Json::Arr(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(out, x);
            }
            out.push(']');
        }
        Json::Obj(o) => {
            out.push('{');
            for (i, (k, x)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                escape(out, k);
                out.push(':');
                write_json(out, x);
            }
            out.push('}');
        }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Option<()> {
        self.ws();
        (self.b.get(self.i) == Some(&c)).then(|| self.i += 1)
    }
    fn lit(&mut self, s: &str, v: Json) -> Option<Json> {
        if self.b[self.i..].starts_with(s.as_bytes()) {
            self.i += s.len();
            Some(v)
        } else {
            None
        }
    }
    fn value(&mut self) -> Option<Json> {
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut o = Vec::new();
                if self.eat(b'}').is_some() {
                    return Some(Json::Obj(o));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.eat(b':')?;
                    let v = self.value()?;
                    o.push((k, v));
                    if self.eat(b',').is_some() {
                        continue;
                    }
                    self.eat(b'}')?;
                    return Some(Json::Obj(o));
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                if self.eat(b']').is_some() {
                    return Some(Json::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    if self.eat(b',').is_some() {
                        continue;
                    }
                    self.eat(b']')?;
                    return Some(Json::Arr(a));
                }
            }
            b'"' => self.string().map(Json::Str),
            b't' => self.lit("true", Json::Bool(true)),
            b'f' => self.lit("false", Json::Bool(false)),
            b'n' => self.lit("null", Json::Null),
            _ => {
                let start = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    self.i += 1;
                }
                std::str::from_utf8(&self.b[start..self.i]).ok()?.parse().ok().map(Json::Num)
            }
        }
    }
    fn string(&mut self) -> Option<String> {
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => {
                            let hex = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let mut cp = u32::from_str_radix(hex, 16).ok()?;
                            if (0xd800..0xdc00).contains(&cp) && self.b.get(self.i..self.i + 2) == Some(b"\\u") {
                                let lo = std::str::from_utf8(self.b.get(self.i + 2..self.i + 6)?).ok()?;
                                let lo = u32::from_str_radix(lo, 16).ok()?;
                                self.i += 6;
                                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                            }
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other as char),
                    }
                }
                _ => {
                    // UTF-8 копируем как есть
                    let start = self.i - 1;
                    let len = match c {
                        0..=0x7f => 1,
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    let end = (start + len).min(self.b.len());
                    out.push_str(std::str::from_utf8(&self.b[start..end]).ok()?);
                    self.i = end;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let src = r#"{"type":"redstone_inputs","levels":{"front":0,"back":15},"ok":true,"s":"Привет \"x\"\n","a":[1,2.5,null]}"#;
        let v = Json::parse(src).unwrap();
        assert_eq!(v.get("type").and_then(Json::as_str), Some("redstone_inputs"));
        assert_eq!(v.get("levels").and_then(|l| l.get("back")).and_then(Json::as_f64), Some(15.0));
        assert_eq!(v.get("s").and_then(Json::as_str), Some("Привет \"x\"\n"));
        let again = Json::parse(&v.to_string()).unwrap();
        assert_eq!(v, again);
        assert!(Json::parse("{bad").is_none());
        assert_eq!(Json::parse(r#""é😀""#), Some(Json::Str("é😀".into())));
    }
}
