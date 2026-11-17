//! Слой tracing, пересылающий предупреждения и ошибки моду событиями LogLine.

use crate::pb;
use std::fmt::Write as _;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

pub struct ForwardLayer {
    tx: broadcast::Sender<pb::Event>,
}

impl ForwardLayer {
    pub fn new(tx: broadcast::Sender<pb::Event>) -> Self {
        ForwardLayer { tx }
    }
}

struct MsgVisitor(String, String);

impl Visit for MsgVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.1, "{}={:?} ", field.name(), value);
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            let _ = write!(self.1, "{}={} ", field.name(), value);
        }
    }
}

impl<S: Subscriber> Layer<S> for ForwardLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        if level > Level::WARN || self.tx.receiver_count() == 0 {
            return;
        }
        let mut v = MsgVisitor(String::new(), String::new());
        event.record(&mut v);
        let message = format!("{}{}", v.1, v.0);
        let _ = self.tx.send(pb::Event {
            kind: Some(pb::event::Kind::Log(pb::LogLine { level: level.as_str().to_ascii_lowercase(), message })),
        });
    }
}
