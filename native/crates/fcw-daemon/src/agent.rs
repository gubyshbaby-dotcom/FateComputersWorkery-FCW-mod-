//! Канал агента гостя (virtio-serial `fcw.agent`): строки JSON.

use serde_json::{json, Value};

/// Стороны блока в порядке control.proto (front, back, left, right, top, bottom).
pub const SIDES: [&str; 6] = ["front", "back", "left", "right", "top", "bottom"];

pub fn side_index(v: &Value) -> Option<usize> {
    if let Some(n) = v.as_u64() {
        return (n < 6).then_some(n as usize);
    }
    let s = v.as_str()?.to_ascii_lowercase();
    let s = match s.as_str() {
        "up" => "top",
        "down" => "bottom",
        other => other,
    };
    SIDES.iter().position(|x| *x == s)
}

/// `{"type":"redstone_inputs","levels":{...}}`
pub fn redstone_inputs_msg(levels: &[u32; 6]) -> String {
    let mut m = serde_json::Map::new();
    for (i, s) in SIDES.iter().enumerate() {
        m.insert(s.to_string(), json!(levels[i].min(15)));
    }
    json!({ "type": "redstone_inputs", "levels": m }).to_string()
}

pub fn time_msg() -> String {
    json!({ "type": "time", "unix_ms": crate::unix_ms() }).to_string()
}

pub const SHUTDOWN_MSG: &str = r#"{"type":"shutdown"}"#;
pub const PING_MSG: &str = r#"{"type":"ping"}"#;

/// Разобранное сообщение от гостя.
#[derive(Debug, PartialEq)]
pub enum GuestMsg {
    Hello(Value),
    Heartbeat,
    RedstoneSet { side: usize, level: u32 },
    Other(Value),
}

pub fn parse(line: &str) -> Option<GuestMsg> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let t = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    Some(match t {
        "hello" => GuestMsg::Hello(v),
        "heartbeat" | "pong" => GuestMsg::Heartbeat,
        "redstone_set" => {
            let side = side_index(v.get("side")?)?;
            let level = v.get("level").and_then(|l| l.as_u64()).unwrap_or(0).min(15) as u32;
            GuestMsg::RedstoneSet { side, level }
        }
        _ => GuestMsg::Other(v),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_messages() {
        assert_eq!(
            parse(r#"{"type":"redstone_set","side":"left","level":20}"#),
            Some(GuestMsg::RedstoneSet { side: 2, level: 15 })
        );
        assert_eq!(parse(r#"{"type":"heartbeat"}"#), Some(GuestMsg::Heartbeat));
        assert!(matches!(parse(r#"{"type":"hello","agent":"0.1.0"}"#), Some(GuestMsg::Hello(_))));
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"type":"redstone_set","side":"nowhere","level":1}"#).is_none());
    }

    #[test]
    fn inputs_json() {
        let s = redstone_inputs_msg(&[0, 15, 3, 0, 0, 99]);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["type"], "redstone_inputs");
        assert_eq!(v["levels"]["back"], 15);
        assert_eq!(v["levels"]["bottom"], 15);
        assert_eq!(v["levels"]["left"], 3);
    }
}
