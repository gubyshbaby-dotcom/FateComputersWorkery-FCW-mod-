//! Канал управления: мод (сервер Minecraft) <-> демон по Unix-сокету.
//! Кадр = u32 BE длина + Envelope. Запросы с id > 0, ответы с тем же id,
//! события с id = 0 (рассылаются всем подключённым).

use crate::daemon::Daemon;
use crate::images;
use crate::localsock::{LocalListener, LocalStream};
use crate::pb::{self, envelope, request, response};
use crate::vm::Vm;
use crate::AbortOnDrop;
use anyhow::{anyhow, Result};
use fcw_proto::frame;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

pub async fn serve(d: Arc<Daemon>, listener: LocalListener) {
    loop {
        match listener.accept().await {
            Ok(s) => {
                let d = d.clone();
                tokio::spawn(async move { handle_conn(d, s).await });
            }
            Err(e) => {
                tracing::warn!("control accept: {e}");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

type Tx = mpsc::UnboundedSender<pb::Envelope>;

fn respond(tx: &Tx, id: u64, r: Result<Option<response::Kind>>) {
    let resp = match r {
        Ok(kind) => pb::Response { ok: true, error: String::new(), kind },
        Err(e) => pb::Response { ok: false, error: format!("{e:#}"), kind: None },
    };
    let _ = tx.send(pb::Envelope { id, body: Some(envelope::Body::Response(resp)) });
}

async fn handle_conn(d: Arc<Daemon>, s: LocalStream) {
    tracing::debug!("control client connected");
    let (mut rd, mut wr) = tokio::io::split(s);
    let (tx, mut rx) = mpsc::unbounded_channel::<pb::Envelope>();
    let _writer = AbortOnDrop(tokio::spawn(async move {
        while let Some(env) = rx.recv().await {
            if frame::write_async(&mut wr, &env).await.is_err() {
                break;
            }
        }
    }));
    let mut ev_rx = d.events.subscribe();
    let ev_tx = tx.clone();
    let _events = AbortOnDrop(tokio::spawn(async move {
        loop {
            match ev_rx.recv().await {
                Ok(ev) => {
                    if ev_tx.send(pb::Envelope { id: 0, body: Some(envelope::Body::Event(ev)) }).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => tracing::warn!("control client lagged {n} events"),
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }));
    loop {
        let env: pb::Envelope = match frame::read_async(&mut rd).await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("control read: {e}");
                break;
            }
        };
        let Some(envelope::Body::Request(req)) = env.body else { continue };
        let Some(kind) = req.kind else {
            respond(&tx, env.id, Err(anyhow!("empty request")));
            continue;
        };
        dispatch(&d, &tx, env.id, kind);
    }
    tracing::debug!("control client disconnected");
}

/// Быстрые запросы выполняются сразу; операции над машинами — в задачах,
/// но место в очереди машины занимается здесь, в порядке поступления.
fn dispatch(d: &Arc<Daemon>, tx: &Tx, id: u64, kind: request::Kind) {
    use request::Kind as K;
    match kind {
        K::Hello(h) => {
            tracing::info!("control hello from {:?} (protocol {})", h.client_name, h.protocol_version);
            respond(
                tx,
                id,
                Ok(Some(response::Kind::Hello(pb::HelloResult {
                    daemon_version: crate::VERSION.to_string(),
                    protocol_version: fcw_proto::CONTROL_PROTOCOL_VERSION,
                    quic_port: d.quic_port as u32,
                    cert_sha256: d.cert_sha256.clone(),
                    accelerator: d.env.accel.name().to_string(),
                    max_vms: d.max_vms,
                    host_cpus: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
                    host_memory_mb: Daemon::host_memory_mb(),
                }))),
            );
        }
        K::ListImages(_) => {
            let images = images::list(&d.env.images_dir).iter().map(|i| i.info()).collect();
            respond(tx, id, Ok(Some(response::Kind::Images(pb::ImageList { images }))));
        }
        K::GetVm(r) => respond(tx, id, d.vm_or_err(&r.vm_id).map(|vm| Some(response::Kind::Vm(vm.info())))),
        K::ListVms(_) => respond(tx, id, Ok(Some(response::Kind::Vms(pb::VmList { vms: d.list_vms() })))),
        K::IssueToken(r) => {
            let res = d.vm_or_err(&r.vm_id).map(|_| {
                let (token, exp) =
                    d.tokens.lock().issue(&r.vm_id, &r.player_uuid, &r.player_name, r.permissions, r.ttl_seconds);
                Some(response::Kind::Token(pb::TokenResult { token, expires_unix_ms: exp }))
            });
            respond(tx, id, res);
        }
        K::RevokeTokens(r) => {
            d.revoke(&r.vm_id, &r.player_uuid);
            respond(tx, id, Ok(None));
        }
        K::AgentSend(r) => {
            let res = d.vm_or_err(&r.vm_id).and_then(|vm| {
                if serde_json::from_str::<serde_json::Value>(&r.json).is_err() {
                    return Err(anyhow!("json is not valid JSON"));
                }
                if vm.agent_send(r.json.clone()) {
                    Ok(None)
                } else {
                    Err(anyhow!("machine is not running or agent queue is full"))
                }
            });
            respond(tx, id, res);
        }
        K::SetRedstoneInputs(r) => {
            let res = d.vm_or_err(&r.vm_id).map(|vm| {
                vm.set_redstone_inputs(&r.levels);
                None
            });
            respond(tx, id, res);
        }
        K::FetchImage(r) => fetch_image(d, tx, id, r),
        K::Shutdown(_) => {
            respond(tx, id, Ok(None));
            d.request_shutdown();
        }
        K::CreateVm(r) => match d.vm_for_create(&r) {
            Err(e) => respond(tx, id, Err(e)),
            Ok((vm, _new)) => {
                let ticket = vm.op_ticket();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let g = ticket.acquire().await;
                    let cfg = crate::vm::VmConfig::from_request(&r);
                    let res = vm.update_config(&g, cfg, r.reinstall).map(|_| Some(response::Kind::Vm(vm.info())));
                    respond(&tx, id, res);
                });
            }
        },
        K::StartVm(r) => vm_op(d, tx, id, &r.vm_id, |d, vm, g| async move {
            if !vm.is_active() && d.max_vms > 0 && d.running_count() >= d.max_vms as usize {
                return Err(anyhow!("too many running machines (max {})", d.max_vms));
            }
            vm.start(&g).await?;
            Ok(Some(response::Kind::Vm(vm.info())))
        }),
        K::StopVm(r) => {
            let timeout = Duration::from_millis(if r.timeout_ms == 0 { 30_000 } else { r.timeout_ms as u64 });
            let force = r.force;
            vm_op(d, tx, id, &r.vm_id, move |_d, vm, g| async move {
                vm.stop(&g, force, timeout).await?;
                Ok(Some(response::Kind::Vm(vm.info())))
            })
        }
        K::PauseVm(r) => vm_op(d, tx, id, &r.vm_id, |_d, vm, g| async move {
            vm.pause(&g).await?;
            Ok(Some(response::Kind::Vm(vm.info())))
        }),
        K::ResumeVm(r) => vm_op(d, tx, id, &r.vm_id, |_d, vm, g| async move {
            vm.resume(&g).await?;
            Ok(Some(response::Kind::Vm(vm.info())))
        }),
        K::DeleteVm(r) => vm_op(d, tx, id, &r.vm_id, |d, vm, g| async move {
            vm.stop(&g, true, Duration::from_secs(5)).await?;
            d.revoke(&vm.id, "");
            d.vms.lock().remove(&vm.id);
            vm.remove_files()?;
            Ok(None)
        }),
    }
}

fn vm_op<F, Fut>(d: &Arc<Daemon>, tx: &Tx, id: u64, vm_id: &str, f: F)
where
    F: FnOnce(Arc<Daemon>, Arc<Vm>, tokio::sync::OwnedMutexGuard<()>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Option<response::Kind>>> + Send,
{
    let vm = match d.vm_or_err(vm_id) {
        Ok(vm) => vm,
        Err(e) => return respond(tx, id, Err(e)),
    };
    let ticket = vm.op_ticket();
    let tx = tx.clone();
    let d = d.clone();
    tokio::spawn(async move {
        let g = ticket.acquire().await;
        let res = f(d, vm, g).await;
        respond(&tx, id, res);
    });
}

/// FetchImage: ответ сразу (принято), ход и итог — событиями ImageProgress.
fn fetch_image(d: &Arc<Daemon>, tx: &Tx, id: u64, r: pb::FetchImage) {
    if !images::valid_id(&r.image_id) {
        return respond(tx, id, Err(anyhow!("invalid image id {:?}", r.image_id)));
    }
    if r.url.is_empty() {
        return respond(tx, id, Err(anyhow!("url is empty")));
    }
    let in_use = d.vms.lock().values().any(|v| v.is_active() && v.config().image_id == r.image_id);
    if in_use {
        return respond(tx, id, Err(anyhow!("image {} is used by a running machine", r.image_id)));
    }
    if !d.fetching.lock().insert(r.image_id.clone()) {
        return respond(tx, id, Err(anyhow!("image {} is already being downloaded", r.image_id)));
    }
    respond(tx, id, Ok(None));
    let d = d.clone();
    tokio::spawn(async move {
        let events = d.events.clone();
        let image_id = r.image_id.clone();
        let last = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
        let last2 = last.clone();
        let progress = move |done: u64, total: u64| {
            last2.0.store(done, Ordering::Relaxed);
            last2.1.store(total, Ordering::Relaxed);
            let _ = events.send(pb::Event {
                kind: Some(pb::event::Kind::ImageProgress(pb::ImageProgress {
                    image_id: image_id.clone(),
                    done_bytes: done,
                    total_bytes: total,
                    finished: false,
                    error: String::new(),
                })),
            });
        };
        tracing::info!("fetching image {} from {}", r.image_id, r.url);
        let res = images::fetch(d.env.images_dir.clone(), r.image_id.clone(), r.url.clone(), r.sha256.clone(), progress)
            .await;
        d.fetching.lock().remove(&r.image_id);
        let error = match &res {
            Ok(()) => {
                tracing::info!("image {} installed", r.image_id);
                String::new()
            }
            Err(e) => {
                tracing::error!("fetch image {}: {e:#}", r.image_id);
                format!("{e:#}")
            }
        };
        let (done, total) = (last.0.load(Ordering::Relaxed), last.1.load(Ordering::Relaxed));
        let _ = d.events.send(pb::Event {
            kind: Some(pb::event::Kind::ImageProgress(pb::ImageProgress {
                image_id: r.image_id.clone(),
                done_bytes: done,
                total_bytes: total,
                finished: true,
                error,
            })),
        });
    });
}
