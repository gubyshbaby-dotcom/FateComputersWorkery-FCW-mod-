//! Общее состояние демона: машины, токены, события.

use crate::images;
use crate::pb;
use crate::tokens::TokenStore;
use crate::vm::{Env, Vm, VmConfig, VmNotice};
use anyhow::{bail, Result};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};

pub struct Daemon {
    pub env: Arc<Env>,
    pub vms_dir: PathBuf,
    pub max_vms: u32,
    pub quic_port: u16,
    pub cert_sha256: String,
    pub vms: Mutex<HashMap<String, Arc<Vm>>>,
    pub tokens: Mutex<TokenStore>,
    pub events: broadcast::Sender<pb::Event>,
    pub fetching: Mutex<HashSet<String>>,
    shutdown_tx: watch::Sender<bool>,
    next_conn: AtomicU64,
}

impl Daemon {
    pub fn new(
        env: Arc<Env>,
        vms_dir: PathBuf,
        max_vms: u32,
        quic_port: u16,
        cert_sha256: String,
        events: broadcast::Sender<pb::Event>,
    ) -> Arc<Daemon> {
        let (shutdown_tx, _) = watch::channel(false);
        let d = Arc::new(Daemon {
            env,
            vms_dir,
            max_vms,
            quic_port,
            cert_sha256,
            vms: Mutex::new(HashMap::new()),
            tokens: Mutex::new(TokenStore::default()),
            events,
            fetching: Mutex::new(HashSet::new()),
            shutdown_tx,
            next_conn: AtomicU64::new(1),
        });
        d.load_vms();
        d
    }

    /// Машины, созданные в прошлых запусках (vms-dir/*/config.json).
    fn load_vms(&self) {
        let Ok(rd) = std::fs::read_dir(&self.vms_dir) else { return };
        let mut vms = self.vms.lock();
        for e in rd.flatten() {
            let id = e.file_name().to_string_lossy().to_string();
            if !images::valid_id(&id) {
                continue;
            }
            let path = e.path().join("config.json");
            let Ok(data) = std::fs::read(&path) else { continue };
            match serde_json::from_slice::<VmConfig>(&data) {
                Ok(mut cfg) => {
                    cfg.vm_id = id.clone();
                    vms.insert(id.clone(), Vm::new(&id, &self.vms_dir, cfg, self.env.clone(), self.events.clone()));
                }
                Err(err) => tracing::warn!("{}: {err}", path.display()),
            }
        }
        if !vms.is_empty() {
            tracing::info!("loaded {} machine(s) from {}", vms.len(), self.vms_dir.display());
        }
    }

    pub fn next_conn_id(&self) -> u64 {
        self.next_conn.fetch_add(1, Ordering::Relaxed)
    }

    pub fn vm(&self, id: &str) -> Option<Arc<Vm>> {
        self.vms.lock().get(id).cloned()
    }

    pub fn vm_or_err(&self, id: &str) -> Result<Arc<Vm>> {
        match self.vm(id) {
            Some(v) => Ok(v),
            None => bail!("unknown vm {id:?}"),
        }
    }

    /// Машина для CreateVm: существующая или новая (ещё без файлов).
    pub fn vm_for_create(&self, req: &pb::CreateVm) -> Result<(Arc<Vm>, bool)> {
        if !images::valid_id(&req.vm_id) {
            bail!("invalid vm id {:?}", req.vm_id);
        }
        if !images::valid_id(&req.image_id) {
            bail!("invalid image id {:?}", req.image_id);
        }
        let mut vms = self.vms.lock();
        if let Some(vm) = vms.get(&req.vm_id) {
            return Ok((vm.clone(), false));
        }
        let cfg = VmConfig::from_request(req);
        let vm = Vm::new(&req.vm_id, &self.vms_dir, cfg, self.env.clone(), self.events.clone());
        vms.insert(req.vm_id.clone(), vm.clone());
        Ok((vm, true))
    }

    pub fn running_count(&self) -> usize {
        self.vms.lock().values().filter(|v| v.is_active()).count()
    }

    pub fn list_vms(&self) -> Vec<pb::VmInfo> {
        let mut v: Vec<_> = self.vms.lock().values().map(|vm| vm.info()).collect();
        v.sort_by(|a, b| a.vm_id.cmp(&b.vm_id));
        v
    }

    /// Отзыв токенов и активных привязок игроков.
    pub fn revoke(&self, vm_id: &str, player_uuid: &str) -> usize {
        let n = self.tokens.lock().revoke(vm_id, player_uuid);
        let vms: Vec<Arc<Vm>> = self
            .vms
            .lock()
            .values()
            .filter(|v| vm_id.is_empty() || v.id == vm_id)
            .cloned()
            .collect();
        for vm in vms {
            let _ = vm.notices.send(VmNotice::Revoked { player_uuid: player_uuid.to_string() });
        }
        n
    }

    pub fn log_event(&self, level: &str, message: impl Into<String>) {
        let _ = self.events.send(pb::Event {
            kind: Some(pb::event::Kind::Log(pb::LogLine { level: level.into(), message: message.into() })),
        });
    }

    pub fn request_shutdown(&self) {
        self.shutdown_tx.send_replace(true);
    }

    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    /// Гасит все машины параллельно: ACPI, потом quit/kill.
    pub async fn stop_all(&self, timeout: Duration) {
        let vms: Vec<Arc<Vm>> = self.vms.lock().values().filter(|v| v.is_active()).cloned().collect();
        let mut tasks = Vec::new();
        for vm in vms {
            let ticket = vm.op_ticket();
            tasks.push(tokio::spawn(async move {
                // не ждём бесконечно, если идёт долгая операция
                let guard = match tokio::time::timeout(timeout, ticket.acquire()).await {
                    Ok(g) => Some(g),
                    Err(_) => None,
                };
                match guard {
                    Some(g) => {
                        if let Err(e) = vm.stop(&g, false, timeout).await {
                            tracing::warn!(vm = %vm.id, "stop: {e:#}");
                        }
                    }
                    None => tracing::warn!(vm = %vm.id, "busy, leaving QEMU to die with the daemon"),
                }
            }));
        }
        for t in tasks {
            let _ = t.await;
        }
    }

    pub fn host_memory_mb() -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
                for l in s.lines() {
                    if let Some(rest) = l.strip_prefix("MemTotal:") {
                        let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
                        return kb / 1024;
                    }
                }
            }
        }
        0
    }
}
