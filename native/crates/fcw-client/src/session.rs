//! QUIC-сессия клиента с демоном. Общий код для JNI-библиотеки и fcw-probe.
//!
//! Все публичные методы неблокирующие: команды уходят в фоновую задачу на
//! собственном рантайме tokio, данные экранов складываются в [`View`].

use crate::audio::AudioSink;
use crate::tls;
use crate::view::{decode_rects, CursorImage, VideoDecoder, View, KIND_GRAPHICAL, KIND_TEXT};
use anyhow::{anyhow, bail, Context, Result};
use fcw_proto::frame;
use fcw_proto::stream as spb;
use fcw_proto::Message;
use parking_lot::Mutex;
use serde_json::json;
use spb::{client_msg, input_event, server_msg};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

pub const STATE_CONNECTING: i32 = 0;
pub const STATE_READY: i32 = 1;
pub const STATE_ERROR: i32 = 2;
pub const STATE_CLOSED: i32 = 3;

const MAX_EVENTS: usize = 4096;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Собственный рантайм библиотеки (2 рабочих потока).
pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("fcw-client")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

enum Cmd {
    Msg(spb::ClientMsg),
    Input { vm: String, screen: String, ev: input_event::Kind },
    Close,
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct Session {
    state: AtomicI32,
    error: Mutex<String>,
    events: Mutex<VecDeque<String>>,
    cmd: mpsc::UnboundedSender<Cmd>,
    views: Mutex<HashMap<(String, String), Arc<View>>>,
    screens: Mutex<HashMap<String, Vec<spb::Screen>>>,
    controlling: Mutex<HashSet<String>>,
    cursor_shapes: Mutex<HashMap<String, Option<CursorImage>>>,
    cursor_pos: Mutex<HashMap<String, (i32, i32)>>,
    audio: Mutex<HashMap<String, AudioSink>>,
    input_screen: Mutex<HashMap<String, String>>,
    conn: Mutex<Option<quinn::Connection>>,
    pointer_seq: AtomicU64,
    done: AtomicBool,
    close_pong: tokio::sync::Notify,
    /// Адрес демона — loopback: игрок на этом же компьютере.
    same_host: bool,
    /// Просить видеорежим (RECT_H264) в новых подписках.
    accept_video: AtomicBool,
}

/// `127.0.0.1`, `::1`, `localhost`: демон на этом же компьютере.
fn host_is_loopback(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost") || h.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}

/// Nonce пинга, которым закрытие сессии ждёт обработки ввода демоном.
const CLOSE_PING: u64 = u64::MAX;

fn msg(kind: client_msg::Kind) -> spb::ClientMsg {
    spb::ClientMsg { kind: Some(kind) }
}

fn screen_json(s: &spb::Screen) -> serde_json::Value {
    json!({
        "id": s.screen_id, "kind": s.kind, "width": s.width, "height": s.height,
        "cols": s.cols, "rows": s.rows,
    })
}

impl Session {
    /// Начинает соединение в фоне и сразу возвращает сессию.
    pub fn connect(host: &str, port: u16, cert_sha256_hex: &str) -> Arc<Session> {
        let (tx, rx) = mpsc::unbounded_channel();
        let s = Arc::new(Session {
            state: AtomicI32::new(STATE_CONNECTING),
            error: Mutex::new(String::new()),
            events: Mutex::new(VecDeque::new()),
            cmd: tx,
            views: Mutex::new(HashMap::new()),
            screens: Mutex::new(HashMap::new()),
            controlling: Mutex::new(HashSet::new()),
            cursor_shapes: Mutex::new(HashMap::new()),
            cursor_pos: Mutex::new(HashMap::new()),
            audio: Mutex::new(HashMap::new()),
            input_screen: Mutex::new(HashMap::new()),
            conn: Mutex::new(None),
            pointer_seq: AtomicU64::new(0),
            done: AtomicBool::new(false),
            close_pong: tokio::sync::Notify::new(),
            same_host: host_is_loopback(host),
            accept_video: AtomicBool::new(true),
        });
        let s2 = s.clone();
        let host = host.to_string();
        let pin = cert_sha256_hex.to_string();
        runtime().spawn(async move { s2.run(host, port, pin, rx).await });
        s
    }

    /// Выключает видеорежим для следующих подписок (замеры «как было»).
    pub fn set_accept_video(&self, on: bool) {
        self.accept_video.store(on, Ordering::Relaxed);
    }

    pub fn state(&self) -> i32 {
        self.state.load(Ordering::Acquire)
    }

    pub fn error(&self) -> String {
        self.error.lock().clone()
    }

    /// Закрывает сессию: фоновая задача дописывает ввод (до ~1 с) и закрывает
    /// соединение. Не блокирует.
    pub fn close(&self) {
        self.state.store(STATE_CLOSED, Ordering::Release);
        let _ = self.cmd.send(Cmd::Close);
        self.views.lock().clear();
        if let Some(c) = self.conn.lock().clone() {
            runtime().spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                c.close(0u32.into(), b"closed");
            });
        }
    }

    /// Фоновая задача завершилась (соединение закрыто).
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    fn push_event(&self, v: serde_json::Value) {
        let mut q = self.events.lock();
        if q.len() >= MAX_EVENTS {
            q.pop_front();
        }
        q.push_back(v.to_string());
    }

    /// Следующее событие (JSON) или `None`.
    pub fn poll_event(&self) -> Option<String> {
        self.events.lock().pop_front()
    }

    fn send_msg(&self, kind: client_msg::Kind) {
        let _ = self.cmd.send(Cmd::Msg(msg(kind)));
    }

    pub fn attach(&self, token: &str) {
        self.send_msg(client_msg::Kind::Attach(spb::Attach { token: token.to_string() }));
    }

    pub fn detach(&self, vm_id: &str) {
        self.send_msg(client_msg::Kind::Detach(spb::Detach { vm_id: vm_id.to_string() }));
    }

    pub fn ping(&self, nonce: u64) {
        self.send_msg(client_msg::Kind::Ping(spb::Ping { nonce }));
    }

    /// Известные экраны машины (из AttachResult / ScreenChanged).
    pub fn screens(&self, vm_id: &str) -> Vec<spb::Screen> {
        self.screens.lock().get(vm_id).cloned().unwrap_or_default()
    }

    fn screen_kind(&self, vm_id: &str, screen_id: &str) -> i32 {
        let known =
            self.screens.lock().get(vm_id).and_then(|v| v.iter().find(|s| s.screen_id == screen_id).map(|s| s.kind));
        match known {
            Some(k) if k == KIND_GRAPHICAL || k == KIND_TEXT => k,
            _ if screen_id.starts_with("tty") => KIND_TEXT,
            _ => KIND_GRAPHICAL,
        }
    }

    /// Подписка на экран. Вид живёт, пока не вызван [`Session::close_view`].
    pub fn open_view(&self, vm_id: &str, screen_id: &str, max_fps: u32) -> Arc<View> {
        let kind = self.screen_kind(vm_id, screen_id);
        let view = Arc::new(View::new(vm_id, screen_id, max_fps, kind));
        if kind == KIND_GRAPHICAL {
            if let Some(c) = self.cursor_shapes.lock().get(vm_id) {
                view.set_cursor_shape(c.clone());
            }
            if let Some((x, y)) = self.cursor_pos.lock().get(vm_id) {
                view.set_cursor_pos(*x, *y);
            }
        }
        self.views.lock().insert((vm_id.to_string(), screen_id.to_string()), view.clone());
        self.input_screen.lock().insert(vm_id.to_string(), screen_id.to_string());
        self.send_msg(client_msg::Kind::Subscribe(spb::Subscribe {
            vm_id: vm_id.to_string(),
            screen_id: screen_id.to_string(),
            max_fps,
            audio: true,
            accept_lossy: true,
            frame_acks: true,
            same_host: self.same_host,
            accept_h264: VideoDecoder::SUPPORTED && self.accept_video.load(Ordering::Relaxed),
        }));
        view
    }

    pub fn close_view(&self, view: &Arc<View>) {
        let key = (view.vm_id.clone(), view.screen_id.clone());
        let removed = {
            let mut views = self.views.lock();
            match views.get(&key) {
                Some(v) if Arc::ptr_eq(v, view) => {
                    views.remove(&key);
                    true
                }
                _ => false,
            }
        };
        if removed {
            self.send_msg(client_msg::Kind::Unsubscribe(spb::Unsubscribe { vm_id: key.0, screen_id: key.1 }));
        }
    }

    pub fn request_keyframe(&self, vm_id: &str, screen_id: &str) {
        self.send_msg(client_msg::Kind::RequestKeyframe(spb::RequestKeyframe {
            vm_id: vm_id.to_string(),
            screen_id: screen_id.to_string(),
        }));
    }

    pub fn is_controlling(&self, vm_id: &str) -> bool {
        self.controlling.lock().contains(vm_id)
    }

    // ------------------------------------------------------------ ввод

    /// Событие ввода на явно указанный экран ("" — основной экран машины).
    pub fn send_input(&self, vm_id: &str, screen_id: &str, ev: input_event::Kind) {
        let _ = self.cmd.send(Cmd::Input { vm: vm_id.to_string(), screen: screen_id.to_string(), ev });
    }

    fn input(&self, vm_id: &str, ev: input_event::Kind) {
        let screen = self.input_screen.lock().get(vm_id).cloned().unwrap_or_default();
        self.send_input(vm_id, &screen, ev);
    }

    pub fn send_key(&self, vm_id: &str, evdev: u32, down: bool) {
        self.input(vm_id, input_event::Kind::Key(spb::Key { evdev, down }));
    }

    pub fn send_text(&self, vm_id: &str, text: &str) {
        if !text.is_empty() {
            self.input(vm_id, input_event::Kind::Text(spb::Text { utf8: text.to_string() }));
        }
    }

    fn local_cursor(&self, vm_id: &str, x: i32, y: i32) {
        self.cursor_pos.lock().insert(vm_id.to_string(), (x, y));
        let views: Vec<Arc<View>> =
            self.views.lock().iter().filter(|((v, _), _)| v == vm_id).map(|(_, v)| v.clone()).collect();
        for v in views {
            v.set_cursor_pos(x, y);
        }
    }

    /// Движение мыши: датаграмма (ненадёжная, старые отбрасываются по seq).
    pub fn send_pointer(&self, vm_id: &str, x: i32, y: i32) {
        let (x, y) = (x.max(0), y.max(0));
        self.local_cursor(vm_id, x, y);
        let seq = self.pointer_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let dg = spb::Datagram {
            kind: Some(spb::datagram::Kind::PointerMove(spb::PointerMove {
                vm_id: vm_id.to_string(),
                seq,
                x: x as u32,
                y: y as u32,
            })),
        };
        if let Some(c) = self.conn.lock().as_ref() {
            let _ = c.send_datagram(dg.encode_to_vec().into());
        }
    }

    pub fn send_buttons(&self, vm_id: &str, mask: u32, x: i32, y: i32) {
        let (x, y) = (x.max(0), y.max(0));
        self.local_cursor(vm_id, x, y);
        self.input(vm_id, input_event::Kind::Buttons(spb::PointerButtons { mask, x: x as u32, y: y as u32 }));
    }

    pub fn send_wheel(&self, vm_id: &str, dx: i32, dy: i32, x: i32, y: i32) {
        let (x, y) = (x.max(0), y.max(0));
        self.input(vm_id, input_event::Kind::Wheel(spb::Wheel { dx, dy, x: x as u32, y: y as u32 }));
    }

    pub fn release_all(&self, vm_id: &str) {
        self.input(vm_id, input_event::Kind::ReleaseAll(spb::ReleaseAll {}));
    }

    /// На сколько видеорежим экрана машины сейчас отстаёт от её звука, мс
    /// (кодирование у демона и декодирование здесь); на это время
    /// задерживается звук. 0 — видео не идёт.
    pub fn video_lag_ms(&self, vm_id: &str) -> u32 {
        self.audio.lock().get(vm_id).map(|a| a.video_lag_ms()).unwrap_or(0)
    }

    /// PCM 48 кГц стерео s16 (чередование L,R); возвращает число значений.
    pub fn read_audio(&self, vm_id: &str, out: &mut [i16]) -> usize {
        self.audio.lock().get_mut(vm_id).map(|a| a.read(out)).unwrap_or(0)
    }

    // ------------------------------------------------------------ фон

    async fn run(self: Arc<Self>, host: String, port: u16, pin: String, mut rx: mpsc::UnboundedReceiver<Cmd>) {
        let res = self.run_inner(&host, port, &pin, &mut rx).await;
        *self.conn.lock() = None;
        self.done.store(true, Ordering::Release);
        if self.state() == STATE_CLOSED {
            return;
        }
        let err = match res {
            Ok(()) => "connection closed".to_string(),
            Err(e) => format!("{e:#}"),
        };
        *self.error.lock() = err.clone();
        self.state.store(STATE_ERROR, Ordering::Release);
        self.push_event(json!({ "type": "error", "message": err, "fatal": true }));
    }

    async fn run_inner(
        self: &Arc<Self>,
        host: &str,
        port: u16,
        pin: &str,
        rx: &mut mpsc::UnboundedReceiver<Cmd>,
    ) -> Result<()> {
        let pin = tls::parse_pin(pin).map_err(|e| anyhow!(e))?;
        let addr = tokio::net::lookup_host((host, port))
            .await
            .with_context(|| format!("resolve {host}"))?
            .next()
            .ok_or_else(|| anyhow!("cannot resolve {host}"))?;
        let bind: std::net::SocketAddr = if addr.is_ipv6() { "[::]:0".parse()? } else { "0.0.0.0:0".parse()? };
        let mut ep = quinn::Endpoint::client(bind)?;
        ep.set_default_client_config(tls::client_config(pin).map_err(|e| anyhow!(e))?);
        let conn = tokio::time::timeout(Duration::from_secs(15), ep.connect(addr, "fcw-daemon")?)
            .await
            .map_err(|_| anyhow!("connect to {addr}: timeout"))?
            .with_context(|| format!("connect to {addr}"))?;
        if self.state() == STATE_CLOSED {
            conn.close(0u32.into(), b"closed");
            return Ok(());
        }
        *self.conn.lock() = Some(conn.clone());
        let (mut send, mut recv) = conn.open_bi().await?;
        // первый кадр, чтобы демон увидел поток управления
        frame::write_async(&mut send, &msg(client_msg::Kind::Ping(spb::Ping { nonce: 0 }))).await?;
        self.state.store(STATE_READY, Ordering::Release);

        let me = self.clone();
        let _ctl = AbortOnDrop(tokio::spawn(async move {
            while let Ok(Some(m)) = frame::read_async::<_, spb::ServerMsg>(&mut recv).await {
                me.on_server_msg(m);
            }
        }));
        let me = self.clone();
        let c2 = conn.clone();
        let _uni = AbortOnDrop(tokio::spawn(async move {
            while let Ok(stream) = c2.accept_uni().await {
                let me = me.clone();
                tokio::spawn(async move {
                    let _ = me.screen_stream(stream).await;
                });
            }
        }));
        let me = self.clone();
        let c3 = conn.clone();
        let _dg = AbortOnDrop(tokio::spawn(async move {
            while let Ok(b) = c3.read_datagram().await {
                if let Ok(d) = spb::Datagram::decode(b.as_ref()) {
                    me.on_datagram(d);
                }
            }
        }));

        let mut inputs: HashMap<(String, String), quinn::SendStream> = HashMap::new();
        loop {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    None | Some(Cmd::Close) => {
                        // Дописать ввод, потом закрыть. Подтверждение QUIC значит
                        // «получено», но не «обработано»: демон читает потоки ввода
                        // в своих задачах, и закрытие соединения выбросило бы
                        // непрочитанное. Поэтому ещё ждём Pong (круг через демон)
                        // и даём ему немного времени.
                        let flush = async {
                            let had_input = !inputs.is_empty();
                            for (_, mut s) in inputs.drain() {
                                let _ = s.finish();
                                let _ = s.stopped().await;
                            }
                            if had_input {
                                let ping = msg(client_msg::Kind::Ping(spb::Ping { nonce: CLOSE_PING }));
                                if frame::write_async(&mut send, &ping).await.is_ok() {
                                    self.close_pong.notified().await;
                                }
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                            let _ = send.finish();
                        };
                        let _ = tokio::time::timeout(Duration::from_secs(2), flush).await;
                        conn.close(0u32.into(), b"closed");
                        let _ = tokio::time::timeout(Duration::from_millis(500), ep.wait_idle()).await;
                        return Ok(());
                    }
                    Some(Cmd::Msg(m)) => frame::write_async(&mut send, &m).await?,
                    Some(Cmd::Input { vm, screen, ev }) => {
                        let key = (vm.clone(), screen.clone());
                        if !inputs.contains_key(&key) {
                            let mut s = conn.open_uni().await?;
                            let hdr = spb::StreamHeader {
                                kind: spb::StreamKind::StreamInput as i32,
                                vm_id: vm.clone(),
                                screen_id: screen.clone(),
                            };
                            frame::write_async(&mut s, &hdr).await?;
                            inputs.insert(key.clone(), s);
                        }
                        let s = inputs.get_mut(&key).unwrap();
                        let ev = spb::InputEvent { kind: Some(ev) };
                        if frame::write_async(s, &ev).await.is_err() {
                            inputs.remove(&key);
                        }
                    }
                },
                e = conn.closed() => bail!("connection lost: {e}"),
            }
        }
    }

    fn on_server_msg(&self, m: spb::ServerMsg) {
        let Some(kind) = m.kind else { return };
        match kind {
            server_msg::Kind::AttachResult(a) => {
                if a.ok {
                    self.screens.lock().insert(a.vm_id.clone(), a.screens.clone());
                    // уже открытые виды узнают настоящий тип экрана
                } else if !a.vm_id.is_empty() {
                    self.controlling.lock().remove(&a.vm_id);
                }
                self.push_event(json!({
                    "type": "attach", "ok": a.ok, "error": a.error, "vm_id": a.vm_id,
                    "permissions": a.permissions,
                    "screens": a.screens.iter().map(screen_json).collect::<Vec<_>>(),
                }));
            }
            server_msg::Kind::ScreenChanged(sc) => {
                if let Some(s) = &sc.screen {
                    let mut screens = self.screens.lock();
                    let list = screens.entry(sc.vm_id.clone()).or_default();
                    list.retain(|x| x.screen_id != s.screen_id);
                    if !sc.removed {
                        list.push(s.clone());
                    }
                }
                self.push_event(json!({
                    "type": "screen_changed", "vm_id": sc.vm_id, "removed": sc.removed,
                    "screen": sc.screen.as_ref().map(screen_json),
                }));
            }
            server_msg::Kind::CursorShape(c) => {
                let img = (!c.rgba.is_empty()).then_some(CursorImage {
                    w: c.width,
                    h: c.height,
                    hot_x: c.hotspot_x,
                    hot_y: c.hotspot_y,
                    rgba: c.rgba,
                });
                self.cursor_shapes.lock().insert(c.vm_id.clone(), img.clone());
                if let Some(v) = self.views.lock().get(&(c.vm_id.clone(), c.screen_id.clone())) {
                    v.set_cursor_shape(img);
                }
            }
            server_msg::Kind::VmState(s) => {
                self.push_event(json!({ "type": "vm_state", "vm_id": s.vm_id, "state": s.state }));
            }
            server_msg::Kind::Pong(p) => {
                if p.nonce == CLOSE_PING {
                    self.close_pong.notify_one();
                }
            }
            server_msg::Kind::Error(e) => {
                self.push_event(json!({ "type": "error", "message": e.message, "fatal": false }));
            }
            server_msg::Kind::InputControl(ic) => {
                if ic.active {
                    self.controlling.lock().insert(ic.vm_id.clone());
                } else {
                    self.controlling.lock().remove(&ic.vm_id);
                }
                self.push_event(json!({
                    "type": "input_control", "vm_id": ic.vm_id, "active": ic.active,
                    "controller_player": ic.controller_player,
                }));
            }
        }
    }

    fn on_datagram(&self, d: spb::Datagram) {
        match d.kind {
            Some(spb::datagram::Kind::CursorPos(c)) => {
                if self.is_controlling(&c.vm_id) {
                    return; // у управляющего клиента позиция локальная
                }
                self.cursor_pos.lock().insert(c.vm_id.clone(), (c.x as i32, c.y as i32));
                if let Some(v) = self.views.lock().get(&(c.vm_id.clone(), c.screen_id.clone())) {
                    v.set_cursor_pos(c.x as i32, c.y as i32);
                }
            }
            Some(spb::datagram::Kind::Audio(a)) => {
                self.audio.lock().entry(a.vm_id.clone()).or_default().push(a.seq, &a.opus);
            }
            _ => {}
        }
    }

    async fn screen_stream(self: Arc<Self>, mut recv: quinn::RecvStream) -> Result<()> {
        let hdr: spb::StreamHeader = frame::read_async(&mut recv).await?.ok_or_else(|| anyhow!("empty stream"))?;
        if hdr.kind != spb::StreamKind::StreamScreen as i32 {
            bail!("unexpected stream kind");
        }
        let key = (hdr.vm_id.clone(), hdr.screen_id.clone());
        let text = self.screen_kind(&hdr.vm_id, &hdr.screen_id) == KIND_TEXT;
        let mut frames = 0u64;
        let mut video = VideoDecoder::default();
        let mut key_asked: Option<std::time::Instant> = None;
        loop {
            let view = self.views.lock().get(&key).cloned();
            let Some(view) = view else {
                let _ = recv.stop(0u32.into());
                return Ok(());
            };
            if text {
                let Some(u) = frame::read_async::<_, spb::TextUpdate>(&mut recv).await? else { return Ok(()) };
                view.apply_text(&u);
            } else {
                let Some(mut u) = frame::read_async::<_, spb::FrameUpdate>(&mut recv).await? else { return Ok(()) };
                let count = |e: spb::RectEncoding| u.rects.iter().filter(|r| r.encoding == e as i32).count() as u64;
                let (jpeg, h264) = (count(spb::RectEncoding::RectJpeg), count(spb::RectEncoding::RectH264));
                let (bytes, other) = (u.encoded_len(), u.rects.len() as u64 - jpeg - h264);
                let rects = std::mem::take(&mut u.rects);
                let started = std::time::Instant::now();
                let decoded = decode_rects(rects, &mut video);
                view.apply_frame(&u, decoded);
                view.record_frame(bytes, u.capture_unix_us, jpeg, other, h264);
                if h264 > 0 && u.encode_delay_us > 0 {
                    // видео отстаёт от звука на кодирование и декодирование:
                    // звук этой машины задерживаем на столько же
                    let lag = u.encode_delay_us + started.elapsed().as_micros() as u64;
                    self.audio.lock().entry(hdr.vm_id.clone()).or_default().set_video_lag(lag);
                }
                if video.take_need_key() && key_asked.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                    // кадр видео не декодировался (нет ключевого): просим новый
                    key_asked = Some(std::time::Instant::now());
                    self.request_keyframe(&hdr.vm_id, &hdr.screen_id);
                }
                // подтверждение: демон не держит в пути больше, чем доставляет канал
                frames += 1;
                self.send_msg(client_msg::Kind::FrameAck(spb::FrameAck {
                    vm_id: hdr.vm_id.clone(),
                    screen_id: hdr.screen_id.clone(),
                    frames,
                }));
            }
        }
    }
}

#[cfg(test)]
mod same_host_tests {
    use super::host_is_loopback;

    #[test]
    fn loopback_hosts() {
        for h in ["127.0.0.1", "127.1.2.3", "::1", "[::1]", "localhost", "LocalHost"] {
            assert!(host_is_loopback(h), "{h}");
        }
        for h in ["193.233.248.245", "10.0.0.1", "example.com", "", "fe80::1"] {
            assert!(!host_is_loopback(h), "{h}");
        }
    }
}
