//! Канал управления: мод (сервер Minecraft) <-> демон по Unix-сокету.
//! Кадр = u32 BE длина + Envelope. Запросы с id > 0, ответы с тем же id,
//! события с id = 0 (рассылаются всем подключённым).

use crate::daemon::Daemon;
use crate::images;
use crate::localsock::{LocalListener, LocalStream};
use crate::pb::{self, envelope, request, response};
use crate::qemu;
use crate::vm::Vm;
use crate::AbortOnDrop;
use anyhow::{anyhow, Result};
use fcw_proto::frame;
use std::path::Path;
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
            // QEMU ищется заново: его могли скачать после запуска демона
            let (d, tx) = (d.clone(), tx.clone());
            tokio::spawn(async move {
                let hello = hello_result(&d).await;
                respond(&tx, id, Ok(Some(response::Kind::Hello(hello))));
            });
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
        K::FetchQemu(r) => fetch_qemu(d, tx, id, r),
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
        K::StopVm(r) if r.force => {
            // принудительная остановка не ждёт очереди: идущая мягкая остановка
            // (или запуск) завершится сама, когда QEMU умрёт
            let vm = match d.vm_or_err(&r.vm_id) {
                Ok(vm) => vm,
                Err(e) => return respond(tx, id, Err(e)),
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let res = vm.kill_now().await.map(|_| Some(response::Kind::Vm(vm.info())));
                respond(&tx, id, res);
            });
        }
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
        K::DeleteVm(r) => {
            // сначала убить QEMU (без очереди), потом удалить в порядке очереди
            if let Some(vm) = d.vm(&r.vm_id) {
                tokio::spawn(async move {
                    let _ = vm.kill_now().await;
                });
            }
            delete_vm(d, tx, id, &r.vm_id)
        }
    }
}

fn delete_vm(d: &Arc<Daemon>, tx: &Tx, id: u64, vm_id: &str) {
    vm_op(d, tx, id, vm_id, |d, vm, g| async move {
        vm.stop(&g, true, Duration::from_secs(5)).await?;
        d.revoke(&vm.id, "");
        d.vms.lock().remove(&vm.id);
        vm.remove_files()?;
        Ok(None)
    })
}

pub async fn hello_result(d: &Daemon) -> pb::HelloResult {
    let q = d.env.qemu.find().await;
    let accel = match &q {
        Some(q) => q.accel,
        None => d.env.qemu.accel_for(None).await,
    };
    let net = d.env.net.as_ref();
    pb::HelloResult {
        daemon_version: crate::VERSION.to_string(),
        protocol_version: fcw_proto::CONTROL_PROTOCOL_VERSION,
        quic_port: d.quic_port as u32,
        cert_sha256: d.cert_sha256.clone(),
        accelerator: accel.name().to_string(),
        max_vms: d.max_vms,
        host_cpus: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
        host_memory_mb: Daemon::host_memory_mb(),
        network_enabled: net.is_some(),
        internet_allowed: net.map(|n| n.internet).unwrap_or(false),
        network_subnet: net.map(|n| n.subnet.clone()).unwrap_or_default(),
        dns_zone: net.map(|n| n.zone.clone()).unwrap_or_default(),
        qemu_available: q.as_ref().map(|q| q.qemu_img.is_some()).unwrap_or(false),
        qemu_path: q.as_ref().map(|q| q.qemu.to_string_lossy().to_string()).unwrap_or_default(),
        qemu_version: q.as_ref().map(|q| q.version_str.clone()).unwrap_or_default(),
        qemu_bundled: q.as_ref().map(|q| q.bundled).unwrap_or(false),
        accelerator_note: if accel == crate::accel::Accel::Tcg && cfg!(target_os = "linux") {
            crate::accel::kvm_problem().unwrap_or_default()
        } else {
            String::new()
        },
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
    // закреплённые машины работают из хранилища версий; images/<id> напрямую
    // используют только машины, созданные до закрепления версий
    let in_use = d.vms.lock().values().any(|v| {
        let c = v.config();
        v.is_active() && c.image_id == r.image_id && c.image_version.is_empty()
    });
    if in_use {
        return respond(tx, id, Err(anyhow!("image {} is used by a running machine", r.image_id)));
    }
    if !d.fetching.lock().insert(r.image_id.clone()) {
        return respond(tx, id, Err(anyhow!("image {} is already being downloaded", r.image_id)));
    }
    respond(tx, id, Ok(None));
    let images_dir = d.env.images_dir.clone();
    let (image_id, url, sha256) = (r.image_id.clone(), r.url.clone(), r.sha256.clone());
    spawn_fetch(d.clone(), r.image_id.clone(), r.image_id.clone(), format!("image {}", r.image_id), r.url, move |p| {
        images::fetch(images_dir, image_id, url, sha256, p)
    });
}

/// Ключ FetchQemu в `Daemon::fetching` (не пересекается с id образов).
const QEMU_FETCH_KEY: &str = "\0qemu";

/// FetchQemu: как FetchImage, события ImageProgress с image_id = "qemu".
fn fetch_qemu(d: &Arc<Daemon>, tx: &Tx, id: u64, r: pb::FetchQemu) {
    if r.url.is_empty() {
        return respond(tx, id, Err(anyhow!("url is empty")));
    }
    if d.vms.lock().values().any(|v| v.uses_bundled_qemu()) {
        return respond(tx, id, Err(anyhow!("the portable QEMU is used by a running machine; stop it first")));
    }
    if !d.fetching.lock().insert(QEMU_FETCH_KEY.to_string()) {
        return respond(tx, id, Err(anyhow!("QEMU is already being downloaded")));
    }
    respond(tx, id, Ok(None));
    let qemu_dir = d.env.qemu.dir.clone();
    let (url, sha256) = (r.url.clone(), r.sha256.clone());
    let d2 = d.clone();
    spawn_fetch(
        d.clone(),
        QEMU_FETCH_KEY.into(),
        qemu::FETCH_ID.into(),
        "portable QEMU".into(),
        r.url,
        move |p| async move {
            let parent =
                qemu_dir.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
            tokio::fs::create_dir_all(&parent).await?;
            let tmp = parent.join(".fetch-qemu.tar.zst.part");
            images::download(&url, &sha256, &tmp, p).await?;
            // за время скачивания могла запуститься машина на переносимом QEMU
            if d2.vms.lock().values().any(|v| v.uses_bundled_qemu()) {
                let _ = tokio::fs::remove_file(&tmp).await;
                anyhow::bail!("the portable QEMU is used by a running machine; stop it first");
            }
            let tmp2 = tmp.clone();
            let r =
                tokio::task::spawn_blocking(move || qemu::install(&tmp2, &qemu_dir)).await.map_err(|e| anyhow!(e))?;
            let _ = tokio::fs::remove_file(&tmp).await;
            r
        },
    );
}

/// Общая часть скачиваний: прогресс и итог событиями ImageProgress.
fn spawn_fetch<F, Fut>(d: Arc<Daemon>, key: String, event_id: String, what: String, url: String, job: F)
where
    F: FnOnce(Box<dyn Fn(u64, u64) + Send + Sync>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send,
{
    tokio::spawn(async move {
        let events = d.events.clone();
        let last = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
        let last2 = last.clone();
        let eid = event_id.clone();
        let progress = move |done: u64, total: u64| {
            last2.0.store(done, Ordering::Relaxed);
            last2.1.store(total, Ordering::Relaxed);
            let _ = events.send(pb::Event {
                kind: Some(pb::event::Kind::ImageProgress(pb::ImageProgress {
                    image_id: eid.clone(),
                    done_bytes: done,
                    total_bytes: total,
                    finished: false,
                    error: String::new(),
                })),
            });
        };
        tracing::info!("fetching {what} from {url}");
        let res = job(Box::new(progress)).await;
        d.fetching.lock().remove(&key);
        let error = match &res {
            Ok(()) => {
                tracing::info!("{what} installed");
                String::new()
            }
            Err(e) => {
                tracing::error!("fetch {what}: {e:#}");
                format!("{e:#}")
            }
        };
        let (done, total) = (last.0.load(Ordering::Relaxed), last.1.load(Ordering::Relaxed));
        let _ = d.events.send(pb::Event {
            kind: Some(pb::event::Kind::ImageProgress(pb::ImageProgress {
                image_id: event_id,
                done_bytes: done,
                total_bytes: total,
                finished: true,
                error,
            })),
        });
    });
}
