//! Минимальный клиент QMP (QEMU Machine Protocol): строки JSON в обе стороны.

use crate::localsock::LocalStream;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub struct QmpEvent {
    pub name: String,
    pub data: Value,
}

type Pending = oneshot::Sender<Result<Value>>;

#[derive(Clone)]
pub struct Qmp {
    tx: mpsc::UnboundedSender<(Value, Pending)>,
}

impl Qmp {
    /// Рукопожатие (приветствие + qmp_capabilities) и запуск фоновой задачи.
    /// События QEMU уходят в `events`.
    pub async fn handshake(stream: LocalStream, events: mpsc::UnboundedSender<QmpEvent>) -> Result<Qmp> {
        let (rd, mut wr) = tokio::io::split(stream);
        let mut lines = BufReader::new(rd).lines();
        let greeting = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
            .await
            .context("QMP greeting timeout")??
            .ok_or_else(|| anyhow!("QMP closed"))?;
        let g: Value = serde_json::from_str(&greeting).context("QMP greeting")?;
        if g.get("QMP").is_none() {
            bail!("unexpected QMP greeting: {greeting}");
        }
        wr.write_all(b"{\"execute\":\"qmp_capabilities\"}\n").await?;
        loop {
            let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
                .await
                .context("QMP capabilities timeout")??
                .ok_or_else(|| anyhow!("QMP closed"))?;
            let v: Value = serde_json::from_str(&line)?;
            if v.get("return").is_some() {
                break;
            }
            if let Some(e) = v.get("error") {
                bail!("qmp_capabilities: {e}");
            }
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<(Value, Pending)>();
        tokio::spawn(async move {
            let mut pending: HashMap<u64, Pending> = HashMap::new();
            let mut next_id = 1u64;
            loop {
                tokio::select! {
                    line = lines.next_line() => {
                        let line = match line {
                            Ok(Some(l)) => l,
                            _ => break,
                        };
                        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                        if let Some(name) = v.get("event").and_then(|e| e.as_str()) {
                            let _ = events.send(QmpEvent { name: name.to_string(), data: v.get("data").cloned().unwrap_or(Value::Null) });
                            continue;
                        }
                        let id = v.get("id").and_then(|i| i.as_u64());
                        if let Some(p) = id.and_then(|id| pending.remove(&id)) {
                            let r = if let Some(ret) = v.get("return") {
                                Ok(ret.clone())
                            } else {
                                Err(anyhow!("QMP error: {}", v.get("error").map(|e| e.to_string()).unwrap_or_default()))
                            };
                            let _ = p.send(r);
                        }
                    }
                    cmd = rx.recv() => {
                        let Some((mut cmd, reply)) = cmd else { break };
                        let id = next_id;
                        next_id += 1;
                        cmd["id"] = json!(id);
                        let mut data = cmd.to_string().into_bytes();
                        data.push(b'\n');
                        if let Err(e) = wr.write_all(&data).await {
                            let _ = reply.send(Err(anyhow!("QMP write: {e}")));
                            break;
                        }
                        pending.insert(id, reply);
                    }
                }
            }
            for (_, p) in pending.drain() {
                let _ = p.send(Err(anyhow!("QMP connection closed")));
            }
        });
        Ok(Qmp { tx })
    }

    pub async fn execute(&self, command: &str, args: Option<Value>) -> Result<Value> {
        let mut cmd = json!({ "execute": command });
        if let Some(a) = args {
            cmd["arguments"] = a;
        }
        let (tx, rx) = oneshot::channel();
        self.tx.send((cmd, tx)).map_err(|_| anyhow!("QMP connection closed"))?;
        tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .map_err(|_| anyhow!("QMP {command}: timeout"))?
            .map_err(|_| anyhow!("QMP connection closed"))?
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}
