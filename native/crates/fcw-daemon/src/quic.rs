//! QUIC-сервер для клиентов игроков (stream.proto, ALPN `fcw/1`).
//!
//! * первый двунаправленный поток (открывает клиент) — управление;
//! * однонаправленный поток от демона на каждую подписку на экран;
//! * однонаправленные потоки ввода от клиента;
//! * датаграммы: PointerMove (клиент), CursorPos и AudioPacket (демон).

use crate::cert::ServerCert;
use crate::daemon::Daemon;
use crate::gfx;
use crate::text;
use crate::vm::{Controller, Vm, VmNotice, SCREEN_GFX, SCREEN_TTY};
use crate::{pb, spb, AbortOnDrop};
use anyhow::{anyhow, bail, Context, Result};
use fcw_proto::frame;
use fcw_proto::Message;
use parking_lot::Mutex;
use spb::{client_msg, server_msg};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, Notify};
use tokio::time::Instant;

pub fn make_endpoint(bind: SocketAddr, cert: &ServerCert) -> Result<quinn::Endpoint> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.clone()], cert.key.clone_key())
        .context("server certificate")?;
    tls.alpn_protocols = vec![fcw_proto::ALPN.to_vec()];
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)?;
    let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    let mut transport = quinn::TransportConfig::default();
    transport
        .max_concurrent_uni_streams(64u32.into())
        .max_concurrent_bidi_streams(4u32.into())
        .keep_alive_interval(Some(Duration::from_secs(5)))
        .max_idle_timeout(Some(Duration::from_secs(30).try_into()?))
        .datagram_receive_buffer_size(Some(256 * 1024))
        .send_window(16 * 1024 * 1024)
        .stream_receive_window((4u32 * 1024 * 1024).into());
    cfg.transport_config(Arc::new(transport));
    let ep = quinn::Endpoint::server(cfg, bind)?;
    Ok(ep)
}

pub async fn serve(d: Arc<Daemon>, ep: quinn::Endpoint) {
    while let Some(incoming) = ep.accept().await {
        let d = d.clone();
        tokio::spawn(async move {
            let remote = incoming.remote_address();
            match incoming.await {
                Ok(conn) => {
                    let id = d.next_conn_id();
                    tracing::info!("client #{id} connected from {remote}");
                    if let Err(e) = handle_conn(d, conn, id).await {
                        tracing::info!("client #{id}: {e:#}");
                    }
                    tracing::info!("client #{id} disconnected");
                }
                Err(e) => tracing::debug!("handshake from {remote} failed: {e}"),
            }
        });
    }
}

/// Привязка соединения к машине (после Attach).
#[derive(Clone)]
struct Attachment {
    vm: Arc<Vm>,
    perms: u32,
    player_uuid: String,
}

type Attached = Arc<Mutex<HashMap<String, Attachment>>>;

struct Sub {
    keyframe: Arc<Notify>,
    _task: AbortOnDrop<()>,
    _audio: Option<crate::vm::AudioSubGuard>,
}

/// Внутренние сообщения главного цикла соединения.
enum Internal {
    Revoked { vm_id: String, player_uuid: String },
}

struct Conn {
    d: Arc<Daemon>,
    id: u64,
    conn: quinn::Connection,
    out: mpsc::UnboundedSender<spb::ServerMsg>,
    internal: mpsc::UnboundedSender<Internal>,
    attached: Attached,
    notice_tasks: HashMap<String, AbortOnDrop<()>>,
    subs: HashMap<(String, String), Sub>,
    audio_vms: Arc<Mutex<HashMap<String, usize>>>,
}

fn smsg(kind: server_msg::Kind) -> spb::ServerMsg {
    spb::ServerMsg { kind: Some(kind) }
}

async fn handle_conn(d: Arc<Daemon>, conn: quinn::Connection, id: u64) -> Result<()> {
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(15), conn.accept_bi())
        .await
        .map_err(|_| anyhow!("no control stream"))??;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<spb::ServerMsg>();
    let _writer = AbortOnDrop(tokio::spawn(async move {
        while let Some(m) = out_rx.recv().await {
            if frame::write_async(&mut send, &m).await.is_err() {
                break;
            }
        }
    }));
    let (int_tx, mut int_rx) = mpsc::unbounded_channel::<Internal>();
    let attached: Attached = Arc::new(Mutex::new(HashMap::new()));

    // потоки ввода
    let c2 = conn.clone();
    let att2 = attached.clone();
    let _inputs = AbortOnDrop(tokio::spawn(async move {
        while let Ok(stream) = c2.accept_uni().await {
            let att = att2.clone();
            tokio::spawn(async move {
                if let Err(e) = input_stream(stream, att, id).await {
                    tracing::debug!("client #{id} input stream: {e:#}");
                }
            });
        }
    }));
    // датаграммы
    let c3 = conn.clone();
    let att3 = attached.clone();
    let _dgrams = AbortOnDrop(tokio::spawn(async move {
        let mut last_seq: HashMap<String, u64> = HashMap::new();
        while let Ok(b) = c3.read_datagram().await {
            let Ok(dg) = spb::Datagram::decode(b.as_ref()) else { continue };
            if let Some(spb::datagram::Kind::PointerMove(m)) = dg.kind {
                let last = last_seq.entry(m.vm_id.clone()).or_insert(0);
                if m.seq != 0 && m.seq <= *last {
                    continue;
                }
                *last = m.seq;
                let a = att3.lock().get(&m.vm_id).cloned();
                if let Some(a) = a {
                    a.vm.pointer_move(id, a.perms, m.x, m.y);
                }
            }
        }
    }));

    let mut c = Conn {
        d,
        id,
        conn: conn.clone(),
        out: out_tx,
        internal: int_tx,
        attached,
        notice_tasks: HashMap::new(),
        subs: HashMap::new(),
        audio_vms: Arc::new(Mutex::new(HashMap::new())),
    };
    let res: Result<()> = async {
        loop {
            tokio::select! {
                m = frame::read_async::<_, spb::ClientMsg>(&mut recv) => {
                    match m? {
                        Some(m) => c.on_client_msg(m),
                        None => break,
                    }
                }
                Some(i) = int_rx.recv() => c.on_internal(i),
            }
        }
        Ok(())
    }
    .await;
    // уборка: отпустить управление
    let vms: Vec<Arc<Vm>> = c.attached.lock().values().map(|a| a.vm.clone()).collect();
    for vm in vms {
        vm.drop_controller(id);
    }
    c.subs.clear();
    c.notice_tasks.clear();
    conn.close(0u32.into(), b"bye");
    res
}

impl Conn {
    fn send(&self, kind: server_msg::Kind) {
        let _ = self.out.send(smsg(kind));
    }

    fn error(&self, msg: impl Into<String>) {
        self.send(server_msg::Kind::Error(spb::ErrorNotice { message: msg.into() }));
    }

    fn on_client_msg(&mut self, m: spb::ClientMsg) {
        let Some(kind) = m.kind else { return };
        match kind {
            client_msg::Kind::Ping(p) => self.send(server_msg::Kind::Pong(spb::Pong { nonce: p.nonce })),
            client_msg::Kind::Attach(a) => self.attach(&a.token),
            client_msg::Kind::Detach(dt) => self.detach(&dt.vm_id),
            client_msg::Kind::Subscribe(s) => {
                if let Err(e) = self.subscribe(s) {
                    self.error(format!("{e:#}"));
                }
            }
            client_msg::Kind::Unsubscribe(u) => {
                self.unsubscribe(&u.vm_id, &u.screen_id);
            }
            client_msg::Kind::RequestKeyframe(k) => {
                if let Some(s) = self.subs.get(&(k.vm_id.clone(), k.screen_id.clone())) {
                    s.keyframe.notify_one();
                }
            }
        }
    }

    fn on_internal(&mut self, i: Internal) {
        match i {
            Internal::Revoked { vm_id, player_uuid } => {
                let matches = self
                    .attached
                    .lock()
                    .get(&vm_id)
                    .map(|a| player_uuid.is_empty() || a.player_uuid == player_uuid)
                    .unwrap_or(false);
                if matches {
                    self.detach(&vm_id);
                    self.send(server_msg::Kind::AttachResult(spb::AttachResult {
                        ok: false,
                        error: "access revoked".into(),
                        vm_id,
                        permissions: 0,
                        screens: vec![],
                    }));
                }
            }
        }
    }

    fn attach(&mut self, token: &str) {
        let fail = |c: &Conn, e: &str| {
            c.send(server_msg::Kind::AttachResult(spb::AttachResult {
                ok: false,
                error: e.into(),
                vm_id: String::new(),
                permissions: 0,
                screens: vec![],
            }))
        };
        let Some(t) = self.d.tokens.lock().take(token) else {
            return fail(self, "invalid or expired token");
        };
        let Some(vm) = self.d.vm(&t.vm_id) else {
            return fail(self, "unknown machine");
        };
        tracing::info!("client #{} attached to {} as {} (perms {})", self.id, vm.id, t.player_name, t.permissions);
        self.attached.lock().insert(
            vm.id.clone(),
            Attachment { vm: vm.clone(), perms: t.permissions, player_uuid: t.player_uuid.clone() },
        );
        // уведомления машины -> клиенту
        let task = notice_task(
            vm.clone(),
            self.id,
            self.conn.clone(),
            self.out.clone(),
            self.internal.clone(),
            self.audio_vms.clone(),
        );
        self.notice_tasks.insert(vm.id.clone(), AbortOnDrop(tokio::spawn(task)));
        self.send(server_msg::Kind::AttachResult(spb::AttachResult {
            ok: true,
            error: String::new(),
            vm_id: vm.id.clone(),
            permissions: t.permissions,
            screens: vm.screens(),
        }));
        self.send(server_msg::Kind::VmState(spb::VmStateNotice { vm_id: vm.id.clone(), state: vm.state() as u32 }));
        let cursor = vm.gfx.cursor();
        if cursor.width > 0 {
            self.send(server_msg::Kind::CursorShape(cursor_msg(&vm.id, &cursor)));
        }
        let input = pb::Permission::PermKeyboard as u32 | pb::Permission::PermMouse as u32;
        if t.permissions & input != 0 {
            vm.set_controller(Controller {
                conn_id: self.id,
                player_uuid: t.player_uuid.clone(),
                player_name: t.player_name.clone(),
            });
            self.send(server_msg::Kind::InputControl(spb::InputControl {
                vm_id: vm.id.clone(),
                active: true,
                controller_player: t.player_name,
            }));
        }
    }

    fn detach(&mut self, vm_id: &str) {
        let a = self.attached.lock().remove(vm_id);
        if let Some(a) = a {
            a.vm.drop_controller(self.id);
        }
        self.notice_tasks.remove(vm_id);
        self.subs.retain(|(v, _), _| v != vm_id);
        self.audio_vms.lock().remove(vm_id);
    }

    fn subscribe(&mut self, s: spb::Subscribe) -> Result<()> {
        let a = self.attached.lock().get(&s.vm_id).cloned().ok_or_else(|| anyhow!("not attached to {}", s.vm_id))?;
        if a.perms & pb::Permission::PermView as u32 == 0 {
            bail!("no view permission for {}", s.vm_id);
        }
        if s.screen_id != SCREEN_GFX && s.screen_id != SCREEN_TTY {
            bail!("unknown screen {}", s.screen_id);
        }
        let fps = if s.max_fps == 0 { 30 } else { s.max_fps.clamp(1, 60) };
        let key = (s.vm_id.clone(), s.screen_id.clone());
        self.unsubscribe(&key.0, &key.1);
        let keyframe = Arc::new(Notify::new());
        let conn = self.conn.clone();
        let vm = a.vm.clone();
        let screen = s.screen_id.clone();
        let kf = keyframe.clone();
        let cid = self.id;
        let task = tokio::spawn(async move {
            if let Err(e) = screen_stream(conn, vm, screen, fps, kf).await {
                tracing::debug!("client #{cid} screen stream: {e:#}");
            }
        });
        let audio = if s.audio {
            *self.audio_vms.lock().entry(s.vm_id.clone()).or_insert(0) += 1;
            Some(a.vm.audio_subscribe())
        } else {
            None
        };
        self.subs.insert(key, Sub { keyframe, _task: AbortOnDrop(task), _audio: audio });
        Ok(())
    }

    fn unsubscribe(&mut self, vm_id: &str, screen_id: &str) {
        if let Some(sub) = self.subs.remove(&(vm_id.to_string(), screen_id.to_string())) {
            if sub._audio.is_some() {
                let mut av = self.audio_vms.lock();
                if let Some(n) = av.get_mut(vm_id) {
                    *n -= 1;
                    if *n == 0 {
                        av.remove(vm_id);
                    }
                }
            }
        }
    }
}

pub fn cursor_msg(vm_id: &str, c: &gfx::Cursor) -> spb::CursorShape {
    spb::CursorShape {
        vm_id: vm_id.to_string(),
        screen_id: SCREEN_GFX.into(),
        width: c.width,
        height: c.height,
        hotspot_x: c.hot_x,
        hotspot_y: c.hot_y,
        rgba: if c.visible() { c.rgba.clone() } else { Vec::new() },
    }
}

fn send_datagram(conn: &quinn::Connection, kind: spb::datagram::Kind) {
    let dg = spb::Datagram { kind: Some(kind) };
    let _ = conn.send_datagram(dg.encode_to_vec().into());
}

async fn notice_task(
    vm: Arc<Vm>,
    conn_id: u64,
    conn: quinn::Connection,
    out: mpsc::UnboundedSender<spb::ServerMsg>,
    internal: mpsc::UnboundedSender<Internal>,
    audio_vms: Arc<Mutex<HashMap<String, usize>>>,
) {
    let mut rx = vm.notices.subscribe();
    let mut was_controller = vm.controller().map(|c| c.conn_id == conn_id).unwrap_or(false);
    loop {
        let n = match rx.recv().await {
            Ok(n) => n,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let send = |k| {
            let _ = out.send(smsg(k));
        };
        match n {
            VmNotice::State(s) => {
                send(server_msg::Kind::VmState(spb::VmStateNotice { vm_id: vm.id.clone(), state: s as u32 }))
            }
            VmNotice::ScreenChanged { screen, removed } => send(server_msg::Kind::ScreenChanged(spb::ScreenChanged {
                vm_id: vm.id.clone(),
                screen: Some(screen),
                removed,
            })),
            VmNotice::CursorShape(c) => send(server_msg::Kind::CursorShape(cursor_msg(&vm.id, &c))),
            VmNotice::CursorPos { x, y, from_conn } => {
                if from_conn != conn_id {
                    let visible = vm.gfx.cursor().visible();
                    send_datagram(
                        &conn,
                        spb::datagram::Kind::CursorPos(spb::CursorPos {
                            vm_id: vm.id.clone(),
                            screen_id: SCREEN_GFX.into(),
                            x,
                            y,
                            visible,
                        }),
                    );
                }
            }
            VmNotice::Controller { conn_id: c, player_name } => {
                let active = c == conn_id;
                if active != was_controller || active {
                    send(server_msg::Kind::InputControl(spb::InputControl {
                        vm_id: vm.id.clone(),
                        active,
                        controller_player: player_name,
                    }));
                }
                was_controller = active;
            }
            VmNotice::Revoked { player_uuid } => {
                let _ = internal.send(Internal::Revoked { vm_id: vm.id.clone(), player_uuid });
            }
            VmNotice::Audio(p) => {
                if audio_vms.lock().contains_key(&vm.id) {
                    send_datagram(
                        &conn,
                        spb::datagram::Kind::Audio(spb::AudioPacket {
                            vm_id: vm.id.clone(),
                            seq: p.seq,
                            pts_us: p.pts_us,
                            opus: p.data.clone(),
                        }),
                    );
                }
            }
        }
    }
}

/// Поток экрана: заголовок, затем FrameUpdate/TextUpdate не чаще max_fps.
async fn screen_stream(
    conn: quinn::Connection,
    vm: Arc<Vm>,
    screen: String,
    fps: u32,
    keyframe: Arc<Notify>,
) -> Result<()> {
    let mut send = conn.open_uni().await?;
    let hdr = spb::StreamHeader {
        kind: spb::StreamKind::StreamScreen as i32,
        vm_id: vm.id.clone(),
        screen_id: screen.clone(),
    };
    frame::write_async(&mut send, &hdr).await?;
    let interval = Duration::from_micros(1_000_000 / fps as u64);
    let mut last = Instant::now() - interval;
    let mut want_key = true;
    if screen == SCREEN_GFX {
        let mut rx = vm.gfx.subscribe();
        let mut cur = gfx::SubCursor::default();
        loop {
            rx.borrow_and_update();
            if let Some(u) = vm.gfx.collect(&mut cur, want_key) {
                want_key = false;
                frame::write_async(&mut send, &u).await?;
                last = Instant::now();
            }
            tokio::select! {
                r = rx.changed() => r?,
                _ = keyframe.notified() => want_key = true,
            }
            let next = last + interval;
            if Instant::now() < next {
                tokio::time::sleep_until(next).await;
            }
        }
    } else {
        let mut rx = vm.tty.subscribe();
        let mut cur = text::SubCursor::default();
        loop {
            rx.borrow_and_update();
            if let Some(u) = vm.tty.collect(&mut cur, want_key) {
                want_key = false;
                frame::write_async(&mut send, &u).await?;
                last = Instant::now();
            }
            tokio::select! {
                r = rx.changed() => r?,
                _ = keyframe.notified() => want_key = true,
            }
            let next = last + interval;
            if Instant::now() < next {
                tokio::time::sleep_until(next).await;
            }
        }
    }
}

/// Поток ввода от клиента: заголовок (STREAM_INPUT), затем InputEvent.
async fn input_stream(mut recv: quinn::RecvStream, attached: Attached, conn_id: u64) -> Result<()> {
    let hdr: spb::StreamHeader = frame::read_async(&mut recv).await?.ok_or_else(|| anyhow!("empty stream"))?;
    if hdr.kind != spb::StreamKind::StreamInput as i32 {
        bail!("unexpected stream kind {}", hdr.kind);
    }
    while let Some(ev) = frame::read_async::<_, spb::InputEvent>(&mut recv).await? {
        let Some(kind) = ev.kind else { continue };
        let a = attached.lock().get(&hdr.vm_id).cloned();
        match a {
            Some(a) => a.vm.input(conn_id, a.perms, &hdr.screen_id, kind),
            None => continue,
        }
    }
    Ok(())
}
