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
use crate::video;
use crate::vm::{Controller, ScreenConfig, Vm, VmNotice, SCREEN_GFX, SCREEN_TTY};
use crate::{pb, spb, AbortOnDrop};
use anyhow::{anyhow, bail, Context, Result};
use fcw_proto::frame;
use fcw_proto::Message;
use parking_lot::Mutex;
use spb::{client_msg, server_msg};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, Notify};
use tokio::time::Instant;

/// Окно отправки соединения (неподтверждённые + неотправленные данные).
pub const SEND_WINDOW: u64 = 1024 * 1024;

/// Настройки транспорта QUIC (для замеров можно вернуть прежние).
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    pub send_window: u64,
    pub bbr: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning { send_window: SEND_WINDOW, bbr: true }
    }
}

pub fn make_endpoint(bind: SocketAddr, cert: &ServerCert) -> Result<quinn::Endpoint> {
    make_endpoint_tuned(bind, cert, Tuning::default())
}

pub fn make_endpoint_tuned(bind: SocketAddr, cert: &ServerCert, tuning: Tuning) -> Result<quinn::Endpoint> {
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
        // Небольшое окно отправки: кадры не копятся в буфере QUIC на секунды
        // вперёд (основное ограничение — подтверждения кадров, см. GfxSender).
        .send_window(tuning.send_window)
        .stream_receive_window((4u32 * 1024 * 1024).into());
    if tuning.bbr {
        // BBR не раздувает очередь на канале без потерь (UDP через TCP-туннель frp)
        transport.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
    }
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
    /// Подтверждения кадров (FrameAck.frames) для задачи потока экрана.
    acks: mpsc::UnboundedSender<u64>,
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
            client_msg::Kind::FrameAck(a) => {
                if let Some(s) = self.subs.get(&(a.vm_id, a.screen_id)) {
                    let _ = s.acks.send(a.frames);
                }
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
        // уведомления машины -> клиенту (подписка до set_controller, чтобы не потерять)
        let task = notice_task(
            vm.notices.subscribe(),
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
            let (x, y) = vm.cursor_pos();
            send_datagram(
                &self.conn,
                spb::datagram::Kind::CursorPos(spb::CursorPos {
                    vm_id: vm.id.clone(),
                    screen_id: SCREEN_GFX.into(),
                    x,
                    y,
                    visible: cursor.visible(),
                }),
            );
        }
        let input = pb::Permission::PermKeyboard as u32 | pb::Permission::PermMouse as u32;
        if t.permissions & input != 0 {
            // InputControl придёт этому и прежнему клиенту через уведомления
            vm.set_controller(Controller {
                conn_id: self.id,
                player_uuid: t.player_uuid.clone(),
                player_name: t.player_name.clone(),
            });
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
        let mut screen_cfg = self.d.env.screen;
        let mut local = false;
        // Игрок на этом же хосте: канал не ограничен, картинка без потерь.
        // Нужны оба признака: клиент сам подключался к loopback, и пакеты
        // пришли с loopback. Через frp, NAT-петлю или прокси удалённые игроки
        // тоже приходят с 127.0.0.1, но подключались они к внешнему адресу.
        if screen_cfg.local_unlimited && s.same_host && self.conn.remote_address().ip().is_loopback() {
            screen_cfg.max_bytes_per_sec = 0;
            screen_cfg.lossy = false;
            screen_cfg.max_fps = screen_cfg.max_fps.max(60);
            local = true;
        }
        let fps = if s.max_fps == 0 { 30 } else { s.max_fps }.clamp(1, screen_cfg.max_fps.max(1));
        let key = (s.vm_id.clone(), s.screen_id.clone());
        self.unsubscribe(&key.0, &key.1);
        let keyframe = Arc::new(Notify::new());
        let conn = self.conn.clone();
        let vm = a.vm.clone();
        let screen = s.screen_id.clone();
        let kf = keyframe.clone();
        let cid = self.id;
        let (ack_tx, ack_rx) = mpsc::unbounded_channel();
        let opts = StreamOpts {
            fps,
            accept_lossy: s.accept_lossy && screen_cfg.lossy,
            frame_acks: s.frame_acks && screen_cfg.frame_acks,
            video: s.accept_h264 && screen_cfg.video.enabled,
            local,
            cfg: screen_cfg,
        };
        let task = tokio::spawn(async move {
            if let Err(e) = screen_stream(conn, vm, screen, opts, kf, ack_rx).await {
                tracing::debug!("client #{cid} screen stream: {e:#}");
            }
        });
        let audio = if s.audio {
            *self.audio_vms.lock().entry(s.vm_id.clone()).or_insert(0) += 1;
            Some(a.vm.audio_subscribe())
        } else {
            None
        };
        self.subs.insert(key, Sub { keyframe, acks: ack_tx, _task: AbortOnDrop(task), _audio: audio });
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
    mut rx: broadcast::Receiver<VmNotice>,
    vm: Arc<Vm>,
    conn_id: u64,
    conn: quinn::Connection,
    out: mpsc::UnboundedSender<spb::ServerMsg>,
    internal: mpsc::UnboundedSender<Internal>,
    audio_vms: Arc<Mutex<HashMap<String, usize>>>,
) {
    let mut was_controller = false;
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
                if active != was_controller {
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

/// Параметры подписки на экран.
#[derive(Clone, Copy, Debug)]
struct StreamOpts {
    fps: u32,
    accept_lossy: bool,
    frame_acks: bool,
    /// Клиент принимает RECT_H264, и видеорежим включён.
    video: bool,
    /// Игрок на этом же хосте: ступень `--video-local-mbps`.
    local: bool,
    cfg: ScreenConfig,
}

/// Поток экрана: заголовок, затем FrameUpdate/TextUpdate не чаще max_fps.
async fn screen_stream(
    conn: quinn::Connection,
    vm: Arc<Vm>,
    screen: String,
    opts: StreamOpts,
    keyframe: Arc<Notify>,
    acks: mpsc::UnboundedReceiver<u64>,
) -> Result<()> {
    let mut send = conn.open_uni().await?;
    let hdr = spb::StreamHeader {
        kind: spb::StreamKind::StreamScreen as i32,
        vm_id: vm.id.clone(),
        screen_id: screen.clone(),
    };
    frame::write_async(&mut send, &hdr).await?;
    let interval = Duration::from_micros(1_000_000 / opts.fps as u64);
    if screen == SCREEN_GFX {
        return GfxSender::new(conn, opts).run(&mut send, &vm, keyframe, acks).await;
    }
    let mut last = Instant::now() - interval;
    let mut want_key = true;
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

/// Сколько данных экрана держать в пути, если скорость канала неизвестна.
const MIN_CREDIT: usize = 128 * 1024;
/// Допустимая очередь сверх RTT: кадры в пути не старше этого.
const TARGET_QUEUE: Duration = Duration::from_millis(80);
/// Запас бюджета битрейта (кадр можно отправить «в долг» на это время).
const RATE_BURST: Duration = Duration::from_millis(100);
/// Сколько держать режим «канал не успевает» после последней задержки.
const CONGESTED_HOLD: Duration = Duration::from_millis(1500);
/// Кадр, меняющий такую долю экрана, шлётся с JPEG для «пёстрых» тайлов сразу
/// (если у зрителя есть бюджет битрейта), см. [`gfx::EncodePolicy::bulk_fraction`].
const BULK_LOSSY_FRACTION: f32 = 0.2;
/// Доля бюджета зрителя под видео (остальное — тайлы вне области, звук, запас).
const VIDEO_BUDGET_SHARE: f64 = 0.7;
/// Кадры видео, прождавшие отправки дольше, сбрасываются: зритель ждёт
/// новый ключевой кадр.
const VIDEO_MAX_WAIT: Duration = Duration::from_millis(300);
/// Столько сбросов за [`VIDEO_STRIKE_WINDOW`] — зритель уходит на ступень ниже.
const VIDEO_STRIKES: usize = 2;
const VIDEO_STRIKE_WINDOW: Duration = Duration::from_secs(10);
/// Через столько после понижения пробуем вернуть ступень; с каждым
/// понижением срок удваивается (до [`VIDEO_BACKOFF_MAX`]).
const VIDEO_BACKOFF: Duration = Duration::from_secs(20);
const VIDEO_BACKOFF_MAX: Duration = Duration::from_secs(300);
/// Столько сбросов частоты (см. [`VideoState::shed`]) за то же окно — тоже
/// повод уйти на ступень ниже: лучше все кадры похуже, чем четверть получше.
const VIDEO_SHEDS: usize = 8;
/// Повтор подписки на видео, если кодировщика не досталось.
const VIDEO_RETRY: Duration = Duration::from_secs(2);

/// Видеорежим подписчика: подписка на ступень и кадры, ждущие отправки.
struct VideoState {
    sub: video::VideoSub,
    /// Область, тайлы которой этому подписчику не шлются.
    region: gfx::VideoRegion,
    queue: VecDeque<(Instant, Arc<video::VideoFrame>)>,
    /// Ждём ключевой кадр: остальные отбрасываем.
    need_key: bool,
    /// Очередь росла, и кадры верхних временных слоёв пришлось выбросить:
    /// до следующего базового кадра идут только базовые.
    shed: bool,
}

/// Отправка графического экрана одному подписчику.
///
/// Задержку ограничивают три вещи: частота кадров, бюджет битрейта и (если
/// клиент шлёт FrameAck) объём неподтверждённых кадров — не больше, чем канал
/// доставит за RTT + 80 мс. Пока отправить нельзя, изменения склеиваются в
/// буфере экрана, а не встают в очередь. Если канал не успевает или тайл
/// меняется очень часто, «пёстрые» тайлы идут JPEG и позже досылаются без
/// потерь.
///
/// Видеорежим: пока на экране есть видеообласть ([`gfx::GfxScreen::video_region`])
/// и клиент умеет H.264, тайлы области не шлются, вместо них идут кадры общей
/// для зрителей ступени ([`video::VideoHub`]). Кадры видео в очереди склеить
/// нельзя, поэтому при заторе сначала выбрасываются кадры верхних временных
/// слоёв (падает частота), потом очередь сбрасывается до ключевого кадра, а
/// если это повторяется, зритель переходит на ступень ниже. Когда видео кончилось, тайлы области досылаются
/// без потерь тем же путём, что и после JPEG.
struct GfxSender {
    conn: quinn::Connection,
    opts: StreamOpts,
    interval: Duration,
    last_send: Instant,
    /// Бюджет битрейта: момент, когда «долг» будет выплачен.
    next_free: Instant,
    inflight: VecDeque<(u64, usize, Instant)>,
    outstanding: usize,
    frames_sent: u64,
    /// Оценки скорости доставки по подтверждённым кадрам: (момент, байт/с).
    rate_samples: VecDeque<(Instant, f64)>,
    /// Размер последнего отправленного кадра.
    last_frame: usize,
    congested_until: Instant,
    /// Тайлы, отправленные с потерями: индекс -> версия.
    lossy: HashMap<usize, u64>,
    video: Option<VideoState>,
    video_retry: Instant,
    /// Потолок ступени видео сейчас, кбит/с (бюджет, пониженный после заторов).
    video_cap: u32,
    /// Зрителю положена ступень игрока этого хоста.
    video_local: bool,
    video_restore_at: Option<Instant>,
    video_backoff: Duration,
    /// Недавние сбросы очереди видео и сбросы частоты.
    strikes: VecDeque<Instant>,
    sheds: VecDeque<Instant>,
}

/// Следующий кадр видео подписчика (без подписки — ждёт вечно).
async fn next_video(v: &mut Option<VideoState>) -> Result<Arc<video::VideoFrame>, broadcast::error::RecvError> {
    match v {
        Some(v) => v.sub.rx.recv().await,
        None => std::future::pending().await,
    }
}

impl GfxSender {
    fn new(conn: quinn::Connection, opts: StreamOpts) -> Self {
        let now = Instant::now();
        let interval = Duration::from_micros(1_000_000 / opts.fps as u64);
        let mut s = GfxSender {
            conn,
            opts,
            interval,
            last_send: now - interval,
            next_free: now,
            inflight: VecDeque::new(),
            outstanding: 0,
            frames_sent: 0,
            rate_samples: VecDeque::new(),
            last_frame: 0,
            congested_until: now,
            lossy: HashMap::new(),
            video: None,
            video_retry: now,
            video_cap: 0,
            video_local: opts.local,
            video_restore_at: None,
            video_backoff: VIDEO_BACKOFF,
            strikes: VecDeque::new(),
            sheds: VecDeque::new(),
        };
        s.video_cap = s.video_budget_kbps();
        s
    }

    fn rate(&self) -> Option<f64> {
        (self.opts.cfg.max_bytes_per_sec > 0).then_some(self.opts.cfg.max_bytes_per_sec as f64)
    }

    /// Битрейт, который бюджет зрителя оставляет под видео, кбит/с.
    fn video_budget_kbps(&self) -> u32 {
        match self.rate() {
            Some(r) => (r * 8.0 / 1000.0 * VIDEO_BUDGET_SHARE) as u32,
            None => u32::MAX,
        }
    }

    /// Сколько байт кадров можно держать неподтверждёнными: то, что канал
    /// доставит за RTT + TARGET_QUEUE, плюс ещё один кадр (чтобы канал не
    /// простаивал, пока идёт подтверждение).
    fn credit(&self) -> usize {
        let rtt = self.conn.stats().path.min_rtt;
        let est = self.rate_samples.iter().map(|(_, r)| *r).fold(0.0, f64::max);
        let c = est * (rtt + TARGET_QUEUE).as_secs_f64() + self.last_frame as f64;
        (c as usize).max(MIN_CREDIT)
    }

    fn on_ack(&mut self, frames: u64) {
        let now = Instant::now();
        let rtt = self.conn.stats().path.min_rtt;
        while let Some(&(n, bytes, sent)) = self.inflight.front() {
            if n >= frames {
                break;
            }
            self.inflight.pop_front();
            self.outstanding = self.outstanding.saturating_sub(bytes);
            // время передачи кадра ≈ от отправки до подтверждения минус RTT
            let busy = (now - sent).saturating_sub(rtt);
            if busy > Duration::from_millis(5) && bytes > 16 * 1024 {
                self.rate_samples.push_back((now, bytes as f64 / busy.as_secs_f64()));
            }
        }
        // максимум за последние 2 с (как фильтр максимума в BBR)
        while self.rate_samples.front().is_some_and(|(t, _)| now - *t > Duration::from_secs(2)) {
            self.rate_samples.pop_front();
        }
    }

    fn can_send(&self, now: Instant) -> (bool, bool) {
        let rate_ok = self.rate().is_none() || now + RATE_BURST >= self.next_free;
        let credit_ok = !self.opts.frame_acks || self.inflight.is_empty() || self.outstanding < self.credit();
        (rate_ok, credit_ok)
    }

    // ------------------------------------------------------------ видео

    /// Тайлы области нужно дослать тайлами (видео для них кончилось).
    fn mark_region(&mut self, gfx: &gfx::GfxScreen, r: &gfx::VideoRegion) {
        for (i, ver) in gfx.region_tiles(r) {
            self.lossy.insert(i, ver);
        }
    }

    /// Тайлы области теперь идут видео: досылать их не нужно.
    fn unmark_region(&mut self, gfx: &gfx::GfxScreen, r: &gfx::VideoRegion) {
        for (i, _) in gfx.region_tiles(r) {
            self.lossy.remove(&i);
        }
    }

    fn drop_video(&mut self, gfx: &gfx::GfxScreen) {
        if let Some(v) = self.video.take() {
            self.mark_region(gfx, &v.region);
        }
    }

    /// Приводит подписку на видео в соответствие с видеообластью экрана.
    fn sync_video(&mut self, vm: &Vm, now: Instant) {
        if !self.opts.video {
            return;
        }
        let gfx = &vm.gfx;
        let region = gfx.video_region();
        if self.video.as_ref().is_some_and(|v| v.sub.is_dead()) {
            self.drop_video(gfx);
            self.video_retry = now + VIDEO_RETRY;
        }
        let Some(region) = region else {
            self.drop_video(gfx);
            return;
        };
        // после понижения ступени: пора попробовать вернуть прежнюю
        if self.video_restore_at.is_some_and(|t| now >= t) && now >= self.congested_until {
            self.video_restore_at = None;
            self.video_cap = self.video_budget_kbps();
            self.video_local = self.opts.local;
            let cfg = self.opts.cfg.video;
            let want = if self.video_local { Some(cfg.local_kbps) } else { cfg.tier_for(self.video_cap) };
            if self.video.as_ref().is_some_and(|v| Some(v.sub.kbps) != want) {
                self.drop_video(gfx);
            }
        }
        match self.video.as_ref().map(|v| v.region) {
            Some(old) if old != region => {
                // область сменилась: кодировщик начнёт новый поток с ключевого кадра
                self.mark_region(gfx, &old);
                self.unmark_region(gfx, &region);
                if let Some(v) = self.video.as_mut() {
                    v.region = region;
                }
            }
            Some(_) => {}
            None if now >= self.video_retry => match vm.video.subscribe(self.video_cap, self.video_local) {
                Some(sub) => {
                    self.unmark_region(gfx, &region);
                    self.video = Some(VideoState { sub, region, queue: VecDeque::new(), need_key: true, shed: false });
                }
                None => self.video_retry = now + VIDEO_RETRY,
            },
            None => {}
        }
    }

    /// Пришёл кадр от кодировщика (или рассылка отстала).
    fn on_video(&mut self, r: Result<Arc<video::VideoFrame>, broadcast::error::RecvError>, gfx: &gfx::GfxScreen) {
        let now = Instant::now();
        // дальнему зрителю — только нижние временные слои
        let lod = self.opts.cfg.video.max_tid_for(self.opts.fps);
        let Some(v) = self.video.as_mut() else { return };
        match r {
            Ok(f) => {
                if f.tid > lod {
                    return;
                }
                if f.key {
                    v.need_key = false;
                    v.shed = false;
                } else if v.need_key {
                    return;
                }
                if v.shed {
                    // верхние слои ссылаются на выброшенные кадры; с базового
                    // кадра цепочка снова целая
                    if f.tid != 0 {
                        return;
                    }
                    v.shed = false;
                }
                v.queue.push_back((now, f));
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                v.queue.clear();
                v.need_key = true;
                v.sub.request_idr();
            }
            Err(broadcast::error::RecvError::Closed) => {
                self.drop_video(gfx);
                self.video_retry = now + VIDEO_RETRY;
            }
        }
    }

    /// Клиент попросил ключевой кадр.
    fn video_resync(&mut self) {
        if let Some(v) = self.video.as_mut() {
            v.queue.clear();
            v.need_key = true;
            v.sub.request_idr();
        }
    }

    /// Кадры видео ждут, а канал (или бюджет) не пускает.
    fn video_backlog(&mut self, gfx: &gfx::GfxScreen, now: Instant) {
        let Some(v) = self.video.as_mut() else { return };
        let recent = |q: &mut VecDeque<Instant>| {
            q.push_back(now);
            while q.front().is_some_and(|t| now - *t > VIDEO_STRIKE_WINDOW) {
                q.pop_front();
            }
            q.len()
        };
        let mut lower = false;
        if v.queue.len() >= 3 && v.queue.iter().any(|(_, f)| f.tid != 0) {
            // сначала жертвуем частотой: базовые кадры от остальных не зависят
            v.queue.retain(|(_, f)| f.tid == 0);
            v.shed = true;
            lower = recent(&mut self.sheds) >= VIDEO_SHEDS;
        }
        if v.queue.front().is_some_and(|(t, _)| now - *t > VIDEO_MAX_WAIT) {
            // и базовые не уходят: сброс до ключевого кадра
            v.queue.clear();
            v.need_key = true;
            v.sub.request_idr();
            lower |= recent(&mut self.strikes) >= VIDEO_STRIKES;
        }
        if !lower {
            return;
        }
        // канал не тянет эту ступень: на ступень ниже, а с младшей — на тайлы
        let kbps = v.sub.kbps;
        self.strikes.clear();
        self.sheds.clear();
        self.video_local = false;
        match self.opts.cfg.video.tier_below(kbps) {
            Some(lower) => self.video_cap = lower,
            None => self.video_retry = now + self.video_backoff,
        }
        self.drop_video(gfx);
        self.video_restore_at = Some(now + self.video_backoff);
        self.video_backoff = (self.video_backoff * 2).min(VIDEO_BACKOFF_MAX);
    }

    async fn run(
        mut self,
        send: &mut quinn::SendStream,
        vm: &Arc<Vm>,
        keyframe: Arc<Notify>,
        mut acks: mpsc::UnboundedReceiver<u64>,
    ) -> Result<()> {
        let gfx = &vm.gfx;
        let mut rx = gfx.subscribe();
        let mut cur = gfx::SubCursor::default();
        let mut want_key = true;
        loop {
            rx.borrow_and_update();
            let now = Instant::now();
            self.sync_video(vm, now);
            let changes = want_key || gfx.has_changes(&cur);
            let fps_ok = now >= self.last_send + self.interval;
            let (rate_ok, credit_ok) = self.can_send(now);
            let mut have_video = self.video.as_ref().is_some_and(|v| !v.queue.is_empty());
            if (changes && fps_ok && !(rate_ok && credit_ok)) || (have_video && !credit_ok) {
                // есть что слать, но канал (или бюджет) не пускает: склеиваем
                self.congested_until = now + CONGESTED_HOLD;
            }
            if have_video && !credit_ok {
                self.video_backlog(gfx, now);
                have_video = false;
            }
            // Видео бюджета не ждёт: ступень выбрана так, что в бюджет укладывается,
            // а «долг» за большой кадр тайлов задержит только тайлы.
            let tiles_due = rate_ok && fps_ok && (changes || !self.lossy.is_empty());
            let mut sent = false;
            if credit_ok && (tiles_due || have_video) {
                let congested = now < self.congested_until;
                let mut update: Option<spb::FrameUpdate> = None;
                if tiles_due {
                    let policy = gfx::EncodePolicy {
                        lossy_ok: self.opts.accept_lossy,
                        congested,
                        skip: self.video.as_ref().map(|v| v.region),
                        bulk_fraction: if self.rate().is_some() { BULK_LOSSY_FRACTION } else { 0.0 },
                    };
                    // уточнение без потерь — только когда канал свободен
                    let refine: Vec<(usize, u64)> = if congested {
                        Vec::new()
                    } else {
                        self.lossy.iter().map(|(i, v)| (*i, *v)).take(512).collect()
                    };
                    let budget = match self.rate() {
                        Some(r) => ((r * self.interval.as_secs_f64()) as usize).max(32 * 1024),
                        None => 512 * 1024,
                    };
                    if let Some(c) = gfx.collect(&mut cur, want_key, policy, &refine, budget) {
                        if c.update.keyframe {
                            // тайлы видеообласти в ключевой кадр не входят и остаются за видео
                            self.lossy.clear();
                        }
                        want_key = false;
                        for (i, ver, lossy) in &c.tiles {
                            if *lossy {
                                self.lossy.insert(*i, *ver);
                            } else {
                                self.lossy.remove(i);
                            }
                        }
                        update = Some(c.update);
                    }
                }
                if let Some(v) = self.video.as_mut().filter(|v| !v.queue.is_empty()) {
                    let u = update.get_or_insert_with(|| {
                        let (width, height) = gfx.size();
                        spb::FrameUpdate { seq: cur.seq, width, height, ..Default::default() }
                    });
                    for (_, f) in v.queue.drain(..) {
                        u.seq = u.seq.max(f.seq);
                        u.capture_unix_us = f.capture_unix_us;
                        u.encode_delay_us = crate::unix_us().saturating_sub(f.capture_unix_us).max(1);
                        u.rects.push(spb::Rect {
                            x: f.region.x,
                            y: f.region.y,
                            w: f.region.w,
                            h: f.region.h,
                            encoding: spb::RectEncoding::RectH264 as i32,
                            data: f.data.clone(),
                            video_stream: f.stream,
                            video_key: f.key,
                            ..Default::default()
                        });
                    }
                }
                if let Some(u) = update {
                    let bytes = u.encoded_len() + 4;
                    frame::write_async(send, &u).await?;
                    let now = Instant::now();
                    self.last_send = now;
                    if let Some(r) = self.rate() {
                        let start = self.next_free.max(now - RATE_BURST);
                        self.next_free = start + Duration::from_secs_f64(bytes as f64 / r);
                    }
                    if self.opts.frame_acks {
                        self.inflight.push_back((self.frames_sent, bytes, now));
                        self.outstanding += bytes;
                    }
                    self.last_frame = bytes;
                    self.frames_sent += 1;
                    sent = true;
                }
            }
            if sent {
                continue;
            }
            // когда проснуться без внешнего события
            let now = Instant::now();
            let mut wake: Option<Instant> = None;
            let mut at = |t: Instant| wake = Some(wake.map_or(t, |w| w.min(t)));
            if changes {
                if !fps_ok {
                    at(self.last_send + self.interval);
                }
                if !rate_ok {
                    at(self.next_free - RATE_BURST);
                }
                // !credit_ok: ждём подтверждения
            }
            if !self.lossy.is_empty() {
                // уточнения: тайлы ещё не успокоились или канал занят — проверить позже
                at((now + Duration::from_millis(100)).max(self.last_send + self.interval));
            }
            if self.opts.frame_acks && !self.inflight.is_empty() {
                // подстраховка от потерянного подтверждения
                at(now + Duration::from_secs(1));
            }
            if let Some(v) = &self.video {
                // видео могло стихнуть без единого изменения экрана
                at(now + Duration::from_millis(250));
                if let Some((t, _)) = v.queue.front() {
                    at(*t + VIDEO_MAX_WAIT);
                }
            }
            let sleep = tokio::time::sleep_until(wake.unwrap_or(now + Duration::from_secs(3600)).max(now));
            tokio::select! {
                r = rx.changed() => r?,
                _ = keyframe.notified() => {
                    want_key = true;
                    self.video_resync();
                }
                Some(n) = acks.recv() => self.on_ack(n),
                f = next_video(&mut self.video) => self.on_video(f, gfx),
                _ = sleep, if wake.is_some() => {
                    // подтверждения не приходят больше секунды: не стоим вечно
                    if let Some(&(_, _, t)) = self.inflight.front() {
                        if Instant::now() - t > Duration::from_secs(3) {
                            self.inflight.clear();
                            self.outstanding = 0;
                        }
                    }
                }
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
