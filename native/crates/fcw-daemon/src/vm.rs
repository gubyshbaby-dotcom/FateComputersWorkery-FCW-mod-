//! Машина: конфигурация, диск, процесс QEMU, QMP, экраны, ввод, агент.

use crate::agent::{self, GuestMsg};
use crate::audio::{AudioEncoder, OpusPacket};
use crate::gfx::{Cursor, GfxScreen};
use crate::images::{self, Image};
use crate::localsock::{self, MAX_SOCK_PATH};
use crate::net::{self, NetAttach, NetEnv, NetPort};
use crate::qemu::{QemuInfo, QemuLocator};
use crate::qmp::{Qmp, QmpEvent};
use crate::rfb::{self, RfbCmd, RfbEvent};
use crate::text::{self, Mods, TextScreen};
use crate::{pb, spb, AbortOnDrop};
use anyhow::{anyhow, bail, Context, Result};
use fcw_proto::keys;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, watch, OwnedMutexGuard};

pub const SCREEN_GFX: &str = "gfx0";
pub const SCREEN_TTY: &str = "tty0";

/// Параметры окружения, общие для всех машин.
#[derive(Debug)]
pub struct Env {
    pub images_dir: PathBuf,
    /// Поиск QEMU (заново при каждом запуске машины).
    pub qemu: QemuLocator,
    /// Часовой пояс хоста (IANA), уходит гостю как `fcw.tz=`.
    pub tz: Option<String>,
    /// Свитч сети гостей; `None` — сеть выключена (`--network off`).
    pub net: Option<Arc<NetEnv>>,
}

/// Часовой пояс хоста в формате IANA ("Europe/Moscow") или `None`.
pub fn host_timezone() -> Option<String> {
    let tz = iana_time_zone::get_timezone().ok()?;
    let ok = !tz.is_empty()
        && tz.len() <= 64
        && tz.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'));
    ok.then_some(tz)
}

/// config.json машины — последний CreateVm.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct VmConfig {
    pub vm_id: String,
    pub image_id: String,
    pub cpus: u32,
    pub memory_mb: u32,
    pub gpu: bool,
    pub disk_gb: u32,
    pub lang: String,
    pub kbd_layout: String,
    pub sound: bool,
    pub network: bool,
    pub display_width: u32,
    pub display_height: u32,
    /// Версия образа, за которой закреплён диск (ключ из images::version_key).
    /// Пусто — диск ещё не создан или машина создана до закрепления версий.
    pub image_version: String,
    /// Имя в сети (уже очищенное); пусто — `pc-<8 символов vm_id>`.
    pub hostname: String,
}

impl VmConfig {
    pub fn from_request(r: &pb::CreateVm) -> Self {
        VmConfig {
            vm_id: r.vm_id.clone(),
            image_id: r.image_id.clone(),
            cpus: r.cpus,
            memory_mb: r.memory_mb,
            gpu: r.gpu,
            disk_gb: r.disk_gb,
            lang: r.lang.clone(),
            kbd_layout: r.kbd_layout.clone(),
            sound: r.sound,
            network: r.network,
            display_width: r.display_width,
            display_height: r.display_height,
            image_version: String::new(),
            hostname: net::sanitize_hostname(&r.hostname),
        }
    }
    /// Итоговое имя машины: из CreateVm.hostname или по умолчанию.
    pub fn hostname(&self) -> String {
        let h = net::sanitize_hostname(&self.hostname);
        if h.is_empty() {
            net::default_hostname(&self.vm_id)
        } else {
            h
        }
    }
    pub fn width(&self) -> u32 {
        if self.display_width == 0 {
            1024
        } else {
            self.display_width.clamp(320, 4096)
        }
    }
    pub fn height(&self) -> u32 {
        if self.display_height == 0 {
            768
        } else {
            self.display_height.clamp(200, 4096)
        }
    }
}

/// Уведомления для QUIC-соединений, привязанных к машине.
#[derive(Debug, Clone)]
pub enum VmNotice {
    State(pb::VmState),
    ScreenChanged { screen: spb::Screen, removed: bool },
    CursorShape(Cursor),
    CursorPos { x: u32, y: u32, from_conn: u64 },
    Controller { conn_id: u64, player_name: String },
    Revoked { player_uuid: String },
    Audio(Arc<OpusPacket>),
}

#[derive(Debug, Clone)]
pub struct Controller {
    pub conn_id: u64,
    pub player_uuid: String,
    pub player_name: String,
}

#[derive(Default)]
struct InputState {
    pressed: HashSet<u32>,
    buttons: u8,
    mods: Mods,
    x: u16,
    y: u16,
}

/// QMP, канал kill, ожидание выхода, канал агента.
type RunHandles = (Option<Qmp>, mpsc::UnboundedSender<()>, watch::Receiver<bool>, mpsc::Sender<String>);

struct Run {
    gen: u64,
    qmp: Option<Qmp>,
    rfb_tx: Option<mpsc::UnboundedSender<RfbCmd>>,
    tty_tx: mpsc::UnboundedSender<Vec<u8>>,
    agent_tx: mpsc::Sender<String>,
    kill_tx: mpsc::UnboundedSender<()>,
    exit_rx: watch::Receiver<bool>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    _tasks: Vec<AbortOnDrop<()>>,
    /// Порт машины на свитче сети (сокет удаляется вместе с Run).
    _net: Option<NetPort>,
    /// Машина запущена переносимым QEMU из --qemu-dir.
    bundled_qemu: bool,
    stopping: bool,
}

struct Inner {
    config: VmConfig,
    state: pb::VmState,
    started: Option<Instant>,
    last_error: String,
    agent_connected: bool,
    agent_last_seen: Option<Instant>,
    controller: Option<Controller>,
    redstone_in: [u32; 6],
    run: Option<Run>,
    gen: u64,
    /// Остановку запросили во время запуска (StopVm/DeleteVm).
    stop_requested: bool,
}

pub struct Vm {
    pub id: String,
    pub dir: PathBuf,
    pub sock_dir: PathBuf,
    pub gfx: Arc<GfxScreen>,
    pub tty: Arc<TextScreen>,
    pub notices: broadcast::Sender<VmNotice>,
    env: Arc<Env>,
    events: broadcast::Sender<pb::Event>,
    inner: Mutex<Inner>,
    input: Mutex<InputState>,
    op: Arc<tokio::sync::Mutex<()>>,
    audio_subs: AtomicUsize,
}

/// Очередь операций над машиной (start/stop/…): место в очереди занимается
/// сразу при получении запроса, поэтому операции выполняются по порядку.
pub enum OpTicket {
    Ready(OwnedMutexGuard<()>),
    Waiting(Pin<Box<dyn Future<Output = OwnedMutexGuard<()>> + Send>>),
}

impl OpTicket {
    pub async fn acquire(self) -> OwnedMutexGuard<()> {
        match self {
            OpTicket::Ready(g) => g,
            OpTicket::Waiting(f) => f.await,
        }
    }
}

/// Счётчик подписчиков на звук; уменьшается при удалении.
pub struct AudioSubGuard(Arc<Vm>);

impl Drop for AudioSubGuard {
    fn drop(&mut self) {
        self.0.audio_subs.fetch_sub(1, Ordering::Relaxed);
    }
}

/// MAC машины на свитче (детерминированный по vm_id).
struct NetSwitchMac;

impl NetSwitchMac {
    fn of(vm_id: &str) -> [u8; 6] {
        fcw_net::NetSwitch::mac_for_vm(vm_id)
    }
}

fn qemu_escape(p: &Path) -> String {
    p.to_string_lossy().replace(',', ",,")
}

fn abs_path(p: &Path) -> PathBuf {
    let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    p
}

/// Каталог сокетов: каталог машины, если путь короткий, иначе временный.
fn socket_dir(vm_dir: &Path) -> PathBuf {
    if vm_dir.join("agent.sock").to_string_lossy().len() <= MAX_SOCK_PATH {
        return vm_dir.to_path_buf();
    }
    let h = crate::cert::sha256_hex(vm_dir.to_string_lossy().as_bytes());
    std::env::temp_dir().join(format!("fcw-{}", &h[..12]))
}

impl Vm {
    pub fn new(
        id: &str,
        vms_dir: &Path,
        config: VmConfig,
        env: Arc<Env>,
        events: broadcast::Sender<pb::Event>,
    ) -> Arc<Vm> {
        let dir = vms_dir.join(id);
        let sock_dir = socket_dir(&dir);
        let (notices, _) = broadcast::channel(512);
        Arc::new(Vm {
            id: id.to_string(),
            dir,
            sock_dir,
            gfx: Arc::new(GfxScreen::new()),
            tty: Arc::new(TextScreen::new()),
            notices,
            env,
            events,
            inner: Mutex::new(Inner {
                config,
                state: pb::VmState::Stopped,
                started: None,
                last_error: String::new(),
                agent_connected: false,
                agent_last_seen: None,
                controller: None,
                redstone_in: [0; 6],
                run: None,
                gen: 0,
                stop_requested: false,
            }),
            input: Mutex::new(InputState::default()),
            op: Arc::new(tokio::sync::Mutex::new(())),
            audio_subs: AtomicUsize::new(0),
        })
    }

    /// Встать в очередь операций (вызывать в порядке получения запросов).
    pub fn op_ticket(&self) -> OpTicket {
        use futures_util::FutureExt;
        let mut fut = Box::pin(self.op.clone().lock_owned());
        match (&mut fut).now_or_never() {
            Some(g) => OpTicket::Ready(g),
            None => OpTicket::Waiting(fut),
        }
    }

    pub fn config(&self) -> VmConfig {
        self.inner.lock().config.clone()
    }

    pub fn state(&self) -> pb::VmState {
        self.inner.lock().state
    }

    /// Запущена переносимым QEMU из --qemu-dir (его нельзя заменять).
    pub fn uses_bundled_qemu(&self) -> bool {
        self.inner.lock().run.as_ref().map(|r| r.bundled_qemu).unwrap_or(false)
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().run.is_some()
    }

    pub fn save_config(&self, cfg: &VmConfig) -> Result<()> {
        std::fs::create_dir_all(&self.dir).with_context(|| format!("create {}", self.dir.display()))?;
        let data = serde_json::to_vec_pretty(cfg)?;
        let tmp = self.dir.join("config.json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, self.dir.join("config.json"))?;
        Ok(())
    }

    /// CreateVm для существующей машины: новые параметры железа (вступят в
    /// силу при следующем запуске), `reinstall` удаляет диск.
    pub fn update_config(&self, _op: &OwnedMutexGuard<()>, mut cfg: VmConfig, reinstall: bool) -> Result<()> {
        let disk = self.dir.join("disk.qcow2");
        if reinstall {
            if self.is_active() {
                bail!("machine is running; stop it before reinstall");
            }
            if disk.exists() {
                std::fs::remove_file(&disk).with_context(|| format!("remove {}", disk.display()))?;
            }
            self.tty.reset();
            // новый диск закрепится за текущей версией образа при запуске
            cfg.image_version.clear();
        } else if disk.exists() {
            // диск уже создан поверх определённого образа и версии: сменить их
            // можно только переустановкой
            let old = self.config();
            if old.image_id != cfg.image_id {
                tracing::warn!(
                    vm = %self.id,
                    "image change {} -> {} ignored without reinstall", old.image_id, cfg.image_id
                );
                cfg.image_id = old.image_id.clone();
            }
            cfg.image_version = old.image_version;
        } else {
            cfg.image_version.clear();
        }
        self.save_config(&cfg)?;
        self.inner.lock().config = cfg;
        Ok(())
    }

    pub fn screens(&self) -> Vec<spb::Screen> {
        let cfg = self.config();
        let mut out = Vec::new();
        if cfg.gpu {
            let (w, h) = self.gfx.size();
            let (w, h) = if w == 0 || h == 0 { (cfg.width(), cfg.height()) } else { (w, h) };
            out.push(spb::Screen {
                screen_id: SCREEN_GFX.into(),
                kind: spb::ScreenKind::Graphical as i32,
                width: w,
                height: h,
                cols: 0,
                rows: 0,
            });
        }
        out.push(spb::Screen {
            screen_id: SCREEN_TTY.into(),
            kind: spb::ScreenKind::Text as i32,
            width: text::COLS as u32 * 8,
            height: text::ROWS as u32 * 16,
            cols: text::COLS as u32,
            rows: text::ROWS as u32,
        });
        out
    }

    pub fn info(&self) -> pb::VmInfo {
        let screens = self
            .screens()
            .into_iter()
            .map(|s| pb::ScreenInfo {
                screen_id: s.screen_id,
                kind: s.kind,
                width: s.width,
                height: s.height,
                cols: s.cols,
                rows: s.rows,
            })
            .collect();
        let cfg = self.config();
        // в images/<id> установлена другая версия, чем закреплённая за машиной
        let update_available = !cfg.image_version.is_empty()
            && images::load(&self.env.images_dir, &cfg.image_id)
                .map(|i| i.version != cfg.image_version)
                .unwrap_or(false);
        // адрес — только пока порт подключён и гость получил его по DHCP
        let ip = self
            .env
            .net
            .as_ref()
            .and_then(|n| n.switch.stats(&self.id))
            .filter(|st| st.connected && st.ip_configured)
            .and_then(|st| st.ip)
            .map(|ip| ip.to_string())
            .unwrap_or_default();
        let g = self.inner.lock();
        pb::VmInfo {
            vm_id: self.id.clone(),
            state: g.state as i32,
            image_id: g.config.image_id.clone(),
            cpus: g.config.cpus,
            memory_mb: g.config.memory_mb,
            gpu: g.config.gpu,
            screens,
            uptime_ms: g.started.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0),
            last_error: g.last_error.clone(),
            agent_connected: g.agent_connected,
            controller_player: g.controller.as_ref().map(|c| c.player_name.clone()).unwrap_or_default(),
            image_version: g.config.image_version.clone(),
            image_update_available: update_available,
            hostname: g.config.hostname(),
            ip_address: ip,
        }
    }

    fn emit(&self, kind: pb::event::Kind) {
        let _ = self.events.send(pb::Event { kind: Some(kind) });
    }

    fn set_state(&self, state: pb::VmState, reason: &str) {
        {
            let mut g = self.inner.lock();
            if g.state == state {
                return;
            }
            g.state = state;
        }
        tracing::info!(vm = %self.id, "state -> {:?} {}", state, reason);
        self.emit(pb::event::Kind::VmState(pb::VmStateChanged {
            vm_id: self.id.clone(),
            state: state as i32,
            reason: reason.to_string(),
        }));
        let _ = self.notices.send(VmNotice::State(state));
    }

    fn run_alive(&self, gen: u64) -> bool {
        self.inner.lock().run.as_ref().map(|r| r.gen == gen).unwrap_or(false)
    }

    fn sock(&self, name: &str) -> PathBuf {
        self.sock_dir.join(name)
    }

    // ------------------------------------------------------------ запуск

    pub async fn start(self: &Arc<Self>, op: &OwnedMutexGuard<()>) -> Result<()> {
        if self.is_active() {
            return Ok(());
        }
        self.inner.lock().stop_requested = false;
        match self.start_inner(op).await {
            Ok(()) => Ok(()),
            Err(e) => {
                if self.inner.lock().stop_requested {
                    // запуск прерван принудительной остановкой — это не сбой
                    let run = self.inner.lock().run.as_ref().map(|r| (r.kill_tx.clone(), r.exit_rx.clone()));
                    if let Some((kill, mut exit)) = run {
                        let _ = kill.send(());
                        let _ = tokio::time::timeout(Duration::from_secs(5), exit.wait_for(|e| *e)).await;
                    }
                    self.inner.lock().run = None;
                    self.set_state(pb::VmState::Stopped, "stopped during start");
                    return Err(anyhow!("machine was stopped during start"));
                }
                let msg = format!("{e:#}");
                tracing::error!(vm = %self.id, "start failed: {msg}");
                let run = {
                    let mut g = self.inner.lock();
                    g.last_error = msg.clone();
                    g.run.as_ref().map(|r| (r.kill_tx.clone(), r.exit_rx.clone()))
                };
                if let Some((kill, mut exit)) = run {
                    let _ = kill.send(());
                    let _ = tokio::time::timeout(Duration::from_secs(5), exit.wait_for(|e| *e)).await;
                }
                {
                    let mut g = self.inner.lock();
                    g.last_error = msg.clone();
                    g.run = None;
                }
                self.set_state(pb::VmState::Crashed, &msg);
                Err(e)
            }
        }
    }

    /// Образ, с которого грузится машина, и её диск. Если диска нет, текущая
    /// версия образа копируется в хранилище, диск создаётся поверх копии и
    /// машина закрепляется за этой версией. Иначе берётся закреплённая версия
    /// (или, для машин без закрепления, текущий `images/<id>` — как раньше).
    pub async fn prepare_image(&self, _op: &OwnedMutexGuard<()>) -> Result<(Image, PathBuf)> {
        let cfg = self.config();
        std::fs::create_dir_all(&self.dir)?;
        let disk = self.dir.join("disk.qcow2");
        if !disk.exists() {
            let images_dir = self.env.images_dir.clone();
            let id = cfg.image_id.clone();
            let img = tokio::task::spawn_blocking(move || images::ensure_stored(&images_dir, &id))
                .await
                .map_err(|e| anyhow!("store task: {e}"))??;
            let qemu_img = self.env.qemu.find().await.and_then(|q| q.qemu_img);
            self.create_disk(qemu_img.as_deref(), &img, &disk, cfg.disk_gb).await?;
            let mut pinned = cfg.clone();
            pinned.image_version = img.version.clone();
            if let Err(e) = self.save_config(&pinned) {
                let _ = std::fs::remove_file(&disk);
                return Err(e);
            }
            self.inner.lock().config.image_version = img.version.clone();
            tracing::info!(vm = %self.id, "pinned to image {} version {}", cfg.image_id, img.version);
            return Ok((img, disk));
        }
        if !cfg.image_version.is_empty() {
            let img = images::load_stored(&self.env.images_dir, &cfg.image_id, &cfg.image_version).map_err(|e| {
                tracing::warn!(vm = %self.id, "{e:#}");
                anyhow!(
                    "base image {} version {} is missing from the image store; the machine needs a reinstall",
                    cfg.image_id,
                    cfg.image_version
                )
            })?;
            return Ok((img, disk));
        }
        // машина создана до закрепления версий
        let img = images::load(&self.env.images_dir, &cfg.image_id)?;
        if !img.installed() {
            bail!("image {} is not installed (kernel/initrd/rootfs missing)", cfg.image_id);
        }
        Ok((img, disk))
    }

    async fn start_inner(self: &Arc<Self>, op: &OwnedMutexGuard<()>) -> Result<()> {
        let q = self.env.qemu.find().await.ok_or_else(|| {
            anyhow!(
                "QEMU not found: install qemu-system-x86_64, download the portable QEMU (FetchQemu) \
                 into {}, or use --qemu / FCW_QEMU",
                self.env.qemu.dir.display()
            )
        })?;
        let qemu = q.qemu.clone();
        let (img, disk) = self.prepare_image(op).await?;
        let cfg = self.config();
        std::fs::create_dir_all(&self.sock_dir)?;
        for s in ["qmp.sock", "vnc.sock", "tty.sock", "agent.sock", "net.sock"] {
            let _ = std::fs::remove_file(self.sock(s));
        }
        // порт на свитче сети: слушаем до запуска QEMU, он подключится сам
        let net_port = match (&self.env.net, cfg.network) {
            (Some(n), true) => Some(
                net::open_port(n, &self.sock("net.sock"), &self.id, &cfg.hostname(), NetSwitchMac::of(&self.id))
                    .await?,
            ),
            (None, true) => {
                tracing::warn!(vm = %self.id, "network requested but the daemon network is off (--network off)");
                None
            }
            _ => None,
        };
        if self.inner.lock().stop_requested {
            bail!("machine was stopped during start");
        }
        let args = self.qemu_args(&cfg, &img, &disk, net_port.as_ref().map(|p| &p.attach), &q);
        tracing::info!(vm = %self.id, "starting: {} {}", qemu.display(), args.join(" "));

        {
            let mut g = self.inner.lock();
            g.last_error.clear();
        }
        self.set_state(pb::VmState::Starting, "");
        if cfg.gpu {
            self.gfx.resize(cfg.width(), cfg.height());
        } else {
            self.gfx.resize(0, 0);
        }
        self.tty.reset();
        self.input.lock().pressed.clear();

        let mut cmd = tokio::process::Command::new(&qemu);
        cmd.args(&args)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            cmd.process_group(0);
        }
        #[cfg(target_os = "linux")]
        unsafe {
            cmd.pre_exec(|| {
                // QEMU умирает вместе с демоном, даже если тот убит SIGKILL
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        let mut child = cmd.spawn().with_context(|| format!("spawn {}", qemu.display()))?;

        let (kill_tx, mut kill_rx) = mpsc::unbounded_channel::<()>();
        let (exit_tx, exit_rx) = watch::channel(false);
        let (tty_tx, tty_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(64);
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let gen = {
            let mut g = self.inner.lock();
            g.gen += 1;
            let gen = g.gen;
            g.run = Some(Run {
                gen,
                qmp: None,
                rfb_tx: None,
                tty_tx,
                agent_tx,
                kill_tx,
                exit_rx: exit_rx.clone(),
                stderr_tail: stderr_tail.clone(),
                _tasks: Vec::new(),
                _net: net_port,
                bundled_qemu: q.bundled,
                stopping: false,
            });
            g.agent_connected = false;
            g.agent_last_seen = None;
            gen
        };

        // stderr QEMU -> журнал
        if let Some(stderr) = child.stderr.take() {
            let id = self.id.clone();
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    // безобидный шум QEMU не отправляем моду как предупреждения
                    if l.contains("virtio_input_hid_handle_status") {
                        tracing::debug!(vm = %id, "qemu: {l}");
                        continue;
                    }
                    tracing::warn!(vm = %id, "qemu: {l}");
                    let mut t = tail.lock();
                    if t.len() >= 20 {
                        t.pop_front();
                    }
                    t.push_back(l);
                }
            });
        }
        // ожидание выхода процесса
        let me = self.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                s = child.wait() => s,
                _ = kill_rx.recv() => {
                    let _ = child.start_kill();
                    child.wait().await
                }
            };
            // дочитать stderr, чтобы сообщение об ошибке было полным
            tokio::time::sleep(Duration::from_millis(150)).await;
            me.on_exit(gen, status).await;
            let _ = exit_tx.send(true);
        });

        // QMP
        let qmp_path = self.sock("qmp.sock");
        let stream = {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                if *exit_rx.borrow() {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    let tail = stderr_tail.lock().iter().cloned().collect::<Vec<_>>().join(" | ");
                    bail!("QEMU exited: {tail}");
                }
                match localsock::connect(&qmp_path).await {
                    Ok(s) => break s,
                    Err(e) if Instant::now() > deadline => bail!("QMP connect: {e}"),
                    Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        };
        let (qev_tx, mut qev_rx) = mpsc::unbounded_channel::<QmpEvent>();
        let qmp = Qmp::handshake(stream, qev_tx).await?;

        let mut tasks = Vec::new();
        let me = self.clone();
        tasks.push(AbortOnDrop(tokio::spawn(async move {
            while let Some(ev) = qev_rx.recv().await {
                me.on_qmp_event(ev);
            }
        })));
        let me = self.clone();
        tasks.push(AbortOnDrop(tokio::spawn(async move { me.tty_task(gen, tty_rx).await })));
        let me = self.clone();
        tasks.push(AbortOnDrop(tokio::spawn(async move { me.agent_task(gen, agent_rx).await })));
        let mut rfb_tx = None;
        if cfg.gpu || cfg.sound {
            let (tx, rx) = mpsc::unbounded_channel();
            rfb_tx = Some(tx);
            let me = self.clone();
            let want_audio = cfg.sound;
            tasks.push(AbortOnDrop(tokio::spawn(async move { me.rfb_task(gen, rx, want_audio).await })));
        }
        {
            let mut g = self.inner.lock();
            match g.run.as_mut() {
                Some(r) if r.gen == gen => {
                    r.qmp = Some(qmp.clone());
                    r.rfb_tx = rfb_tx;
                    r._tasks = tasks;
                }
                _ => bail!("QEMU exited during start"),
            }
            g.started = Some(Instant::now());
        }
        // даём консоли подключиться до старта гостя
        let tty_path = self.sock("tty.sock");
        for _ in 0..100 {
            if tty_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        qmp.execute("cont", None).await.context("cont")?;
        self.set_state(pb::VmState::Running, "");
        Ok(())
    }

    async fn create_disk(&self, qemu_img: Option<&Path>, img: &Image, disk: &Path, disk_gb: u32) -> Result<()> {
        let qemu_img = qemu_img.map(Path::to_path_buf).ok_or_else(|| anyhow!("qemu-img not found"))?;
        let base = abs_path(&img.rootfs());
        let mut cmd = tokio::process::Command::new(&qemu_img);
        cmd.arg("create").args(["-f", "qcow2", "-F", "qcow2", "-b"]).arg(&base).arg(disk);
        if disk_gb > 0 {
            let want = disk_gb as u64 * 1024 * 1024 * 1024;
            match image_virtual_size(&qemu_img, &base).await {
                Ok(base_size) if want > base_size => {
                    cmd.arg(format!("{disk_gb}G"));
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(vm = %self.id, "qemu-img info: {e:#}"),
            }
        }
        let out = cmd.output().await.with_context(|| format!("run {}", qemu_img.display()))?;
        if !out.status.success() {
            let _ = std::fs::remove_file(disk);
            bail!("qemu-img create failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        tracing::info!(vm = %self.id, "created disk {} over {}", disk.display(), base.display());
        Ok(())
    }

    pub fn qemu_args(
        &self,
        cfg: &VmConfig,
        img: &Image,
        disk: &Path,
        net: Option<&NetAttach>,
        q: &QemuInfo,
    ) -> Vec<String> {
        let host_cpus = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
        let cpus = if cfg.cpus == 0 { 2.min(host_cpus) } else { cfg.cpus.min(host_cpus.max(1) * 2) };
        let mem = if cfg.memory_mb != 0 {
            cfg.memory_mb
        } else if img.manifest.recommended_memory_mb != 0 {
            img.manifest.recommended_memory_mb
        } else {
            1024
        }
        .max(128);
        let mut cmdline = img.manifest.cmdline.trim().to_string();
        if cmdline.is_empty() {
            cmdline = "root=/dev/vda rw console=tty1 console=hvc0".into();
        }
        if !cfg.lang.is_empty() {
            cmdline.push_str(&format!(" fcw.lang={}", cfg.lang));
        }
        if !cfg.kbd_layout.is_empty() {
            cmdline.push_str(&format!(" fcw.kbd={}", cfg.kbd_layout));
        }
        if let Some(tz) = &self.env.tz {
            cmdline.push_str(&format!(" fcw.tz={tz}"));
        }
        cmdline.push_str(&format!(" systemd.hostname={}", cfg.hostname()));
        let mut a: Vec<String> = vec!["-name".into(), format!("fcw-{}", self.id), "-machine".into(), "q35".into()];
        a.extend(q.accel.qemu_args());
        if let Some(d) = &q.data_dir {
            // переносимый QEMU: BIOS и option ROM из его share/qemu
            a.push("-L".into());
            a.push(d.to_string_lossy().into());
        }
        a.extend(
            [
                "-smp",
                &cpus.to_string(),
                "-m",
                &mem.to_string(),
                "-nodefaults",
                "-no-user-config",
                "-display",
                "none",
                "-S",
            ]
            .map(String::from),
        );
        a.push("-kernel".into());
        a.push(abs_path(&img.kernel()).to_string_lossy().into());
        a.push("-initrd".into());
        a.push(abs_path(&img.initrd()).to_string_lossy().into());
        a.push("-append".into());
        a.push(cmdline);
        a.push("-drive".into());
        a.push(format!("if=none,id=root,file={},format=qcow2,discard=unmap", qemu_escape(disk)));
        a.extend(["-device", "virtio-blk-pci,drive=root", "-device", "virtio-serial-pci"].map(String::from));
        a.push("-chardev".into());
        a.push(format!("socket,id=con0,path={},server=on,wait=off", qemu_escape(&self.sock("tty.sock"))));
        a.extend(["-device", "virtconsole,chardev=con0"].map(String::from));
        a.push("-chardev".into());
        a.push(format!("socket,id=ag0,path={},server=on,wait=off", qemu_escape(&self.sock("agent.sock"))));
        a.extend(["-device", "virtserialport,chardev=ag0,name=fcw.agent"].map(String::from));
        if cfg!(windows) {
            a.extend(["-object", "rng-builtin,id=rng0"].map(String::from));
        } else {
            a.extend(["-object", "rng-random,id=rng0,filename=/dev/urandom"].map(String::from));
        }
        a.extend(["-device", "virtio-rng-pci,rng=rng0"].map(String::from));
        a.push("-qmp".into());
        a.push(format!("unix:{},server=on,wait=off", qemu_escape(&self.sock("qmp.sock"))));
        if cfg.gpu {
            a.extend(["-vga", "none", "-device"].map(String::from));
            a.push(format!("virtio-gpu-pci,xres={},yres={},edid=on", cfg.width(), cfg.height()));
            a.extend(["-device", "virtio-keyboard-pci", "-device", "virtio-tablet-pci"].map(String::from));
        }
        // VNC — экран и звук. Без видеокарты VNC нужен только ради звука.
        if cfg.gpu || cfg.sound {
            a.push("-vnc".into());
            let mut vnc = format!("unix:{}", qemu_escape(&self.sock("vnc.sock")));
            if cfg.sound {
                vnc.push_str(",audiodev=snd0");
            }
            a.push(vnc);
        }
        if cfg.sound {
            a.extend(["-audiodev", "none,id=snd0", "-device", "virtio-sound-pci,audiodev=snd0"].map(String::from));
        }
        if let Some(attach) = net {
            a.push("-netdev".into());
            a.push(net::netdev_arg(attach, q.version));
            a.push("-device".into());
            a.push(format!("virtio-net-pci,netdev=net0,mac={},romfile=", net::mac_string(NetSwitchMac::of(&self.id))));
        }
        a
    }

    async fn on_exit(&self, gen: u64, status: std::io::Result<std::process::ExitStatus>) {
        let (stopping, tail) = {
            let mut g = self.inner.lock();
            match g.run.as_ref() {
                Some(r) if r.gen == gen => {}
                _ => return,
            }
            let run = g.run.take().unwrap();
            g.started = None;
            g.agent_connected = false;
            let tail = run.stderr_tail.lock().iter().cloned().collect::<Vec<_>>().join(" | ");
            (run.stopping, tail)
            // run (и его задачи) удаляется здесь
        };
        self.input.lock().pressed.clear();
        for s in ["qmp.sock", "vnc.sock", "tty.sock", "agent.sock"] {
            let _ = std::fs::remove_file(self.sock(s));
        }
        if let Some(n) = &self.env.net {
            n.switch.disconnect(&self.id);
        }
        let ok = matches!(&status, Ok(s) if s.success());
        if stopping || ok {
            self.set_state(pb::VmState::Stopped, if stopping { "stopped" } else { "guest powered off" });
        } else {
            let msg = match &status {
                Ok(s) => {
                    format!("QEMU exited ({s}){}", if tail.is_empty() { String::new() } else { format!(": {tail}") })
                }
                Err(e) => format!("QEMU wait failed: {e}"),
            };
            {
                let mut g = self.inner.lock();
                if g.last_error.is_empty() {
                    g.last_error = msg.clone();
                }
            }
            self.set_state(pb::VmState::Crashed, &msg);
        }
    }

    fn on_qmp_event(&self, ev: QmpEvent) {
        tracing::debug!(vm = %self.id, "QMP event {} {}", ev.name, ev.data);
        match ev.name.as_str() {
            "STOP" => {
                if self.state() == pb::VmState::Running {
                    self.set_state(pb::VmState::Paused, "paused");
                }
            }
            "RESUME" => {
                if matches!(self.state(), pb::VmState::Paused | pb::VmState::Starting) {
                    self.set_state(pb::VmState::Running, "resumed");
                }
            }
            "SHUTDOWN" => {
                if let Some(r) = self.inner.lock().run.as_mut() {
                    r.stopping = true;
                }
                let reason = ev.data.get("reason").and_then(|r| r.as_str()).unwrap_or("shutdown").to_string();
                self.set_state(pb::VmState::Stopping, &reason);
            }
            "GUEST_PANICKED" => {
                self.inner.lock().last_error = "guest kernel panic".into();
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ остановка

    fn run_handles(&self) -> Option<RunHandles> {
        let mut g = self.inner.lock();
        g.stop_requested = true;
        let r = g.run.as_mut()?;
        r.stopping = true;
        Some((r.qmp.clone(), r.kill_tx.clone(), r.exit_rx.clone(), r.agent_tx.clone()))
    }

    pub async fn stop(&self, _op: &OwnedMutexGuard<()>, force: bool, timeout: Duration) -> Result<()> {
        let was_paused = self.state() == pb::VmState::Paused;
        let Some((qmp, kill, mut exit, agent_tx)) = self.run_handles() else {
            return Ok(());
        };
        self.set_state(pb::VmState::Stopping, if force { "force stop" } else { "stop" });
        let wait = |exit: &mut watch::Receiver<bool>, d: Duration| {
            let mut exit = exit.clone();
            async move { tokio::time::timeout(d, exit.wait_for(|e| *e)).await.is_ok() }
        };
        if !force {
            if let Some(q) = &qmp {
                if was_paused {
                    let _ = q.execute("cont", None).await;
                }
                if let Err(e) = q.execute("system_powerdown", None).await {
                    tracing::warn!(vm = %self.id, "system_powerdown: {e:#}");
                }
            }
            if wait(&mut exit, timeout / 2).await {
                return Ok(());
            }
            // ACPI не помог — просим агента гостя
            let _ = agent_tx.try_send(agent::SHUTDOWN_MSG.to_string());
            if wait(&mut exit, timeout / 2).await {
                return Ok(());
            }
            tracing::warn!(vm = %self.id, "guest did not power off in {:?}, quitting QEMU", timeout);
        }
        if let Some(q) = &qmp {
            let _ = tokio::time::timeout(Duration::from_secs(2), q.execute("quit", None)).await;
            if wait(&mut exit, Duration::from_secs(3)).await {
                return Ok(());
            }
        }
        let _ = kill.send(());
        if wait(&mut exit, Duration::from_secs(5)).await {
            Ok(())
        } else {
            bail!("QEMU did not exit")
        }
    }

    /// Принудительная остановка без очереди операций: QEMU убивается сразу,
    /// даже если идёт мягкая остановка или запуск (они завершатся сами).
    pub async fn kill_now(&self) -> Result<()> {
        let Some((_, kill, exit, _)) = self.run_handles() else {
            return Ok(());
        };
        self.set_state(pb::VmState::Stopping, "force stop");
        let _ = kill.send(());
        let mut exit = exit;
        if tokio::time::timeout(Duration::from_secs(10), exit.wait_for(|e| *e)).await.is_ok() {
            Ok(())
        } else {
            bail!("QEMU did not exit")
        }
    }

    pub async fn pause(&self, _op: &OwnedMutexGuard<()>) -> Result<()> {
        let qmp = self.qmp().ok_or_else(|| anyhow!("machine is not running"))?;
        if self.state() == pb::VmState::Paused {
            return Ok(());
        }
        qmp.execute("stop", None).await?;
        self.set_state(pb::VmState::Paused, "paused");
        Ok(())
    }

    pub async fn resume(&self, _op: &OwnedMutexGuard<()>) -> Result<()> {
        let qmp = self.qmp().ok_or_else(|| anyhow!("machine is not running"))?;
        qmp.execute("cont", None).await?;
        self.set_state(pb::VmState::Running, "resumed");
        // часы гостя отстали на время паузы
        if self.inner.lock().agent_connected {
            self.agent_send(agent::time_msg());
        }
        Ok(())
    }

    fn qmp(&self) -> Option<Qmp> {
        self.inner.lock().run.as_ref().and_then(|r| r.qmp.clone())
    }

    // ------------------------------------------------------------ консоль

    async fn tty_task(self: Arc<Self>, gen: u64, mut rx: mpsc::UnboundedReceiver<Vec<u8>>) {
        let path = self.sock("tty.sock");
        let mut buf = vec![0u8; 16 * 1024];
        while self.run_alive(gen) {
            let s = match localsock::connect_retry(&path, Duration::from_secs(30)).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(vm = %self.id, "tty connect: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let (mut rd, mut wr) = tokio::io::split(s);
            loop {
                tokio::select! {
                    n = rd.read(&mut buf) => {
                        let n = match n { Ok(0) | Err(_) => break, Ok(n) => n };
                        let reply = self.tty.feed(&buf[..n]);
                        if !reply.is_empty() && wr.write_all(&reply).await.is_err() {
                            break;
                        }
                    }
                    data = rx.recv() => {
                        let Some(data) = data else { return };
                        if wr.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // ------------------------------------------------------------ VNC

    async fn rfb_task(self: Arc<Self>, gen: u64, mut rx: mpsc::UnboundedReceiver<RfbCmd>, want_audio: bool) {
        let path = self.sock("vnc.sock");
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<RfbEvent>();
        let me = self.clone();
        let _events = AbortOnDrop(tokio::spawn(async move {
            let mut enc: Option<AudioEncoder> = None;
            while let Some(ev) = ev_rx.recv().await {
                me.on_rfb_event(ev, &mut enc);
            }
        }));
        while self.run_alive(gen) {
            match localsock::connect_retry(&path, Duration::from_secs(30)).await {
                Ok(s) => {
                    if let Err(e) = rfb::run(s, self.gfx.clone(), &mut rx, ev_tx.clone(), want_audio).await {
                        if self.run_alive(gen) {
                            tracing::warn!(vm = %self.id, "VNC: {e:#}");
                        }
                    }
                }
                Err(e) => tracing::warn!(vm = %self.id, "VNC connect: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    fn on_rfb_event(&self, ev: RfbEvent, enc: &mut Option<AudioEncoder>) {
        let gpu = self.config().gpu;
        match ev {
            // без видеокарты VNC работает только ради звука: экрана gfx0 нет
            RfbEvent::Resized(..) | RfbEvent::Cursor(_) if !gpu => {}
            RfbEvent::Resized(w, h) => {
                let _ = self.notices.send(VmNotice::ScreenChanged {
                    screen: spb::Screen {
                        screen_id: SCREEN_GFX.into(),
                        kind: spb::ScreenKind::Graphical as i32,
                        width: w,
                        height: h,
                        cols: 0,
                        rows: 0,
                    },
                    removed: false,
                });
            }
            RfbEvent::Cursor(c) => {
                self.gfx.set_cursor(c.clone());
                let _ = self.notices.send(VmNotice::CursorShape(c));
            }
            RfbEvent::Audio(pcm) => {
                if self.audio_subs.load(Ordering::Relaxed) == 0 {
                    return;
                }
                if enc.is_none() {
                    match AudioEncoder::new() {
                        Ok(e) => *enc = Some(e),
                        Err(e) => {
                            tracing::warn!(vm = %self.id, "audio encoder: {e:#}");
                            return;
                        }
                    }
                }
                if let Some(e) = enc.as_mut() {
                    for p in e.push(&pcm) {
                        let _ = self.notices.send(VmNotice::Audio(Arc::new(p)));
                    }
                }
            }
        }
    }

    pub fn audio_subscribe(self: &Arc<Self>) -> AudioSubGuard {
        self.audio_subs.fetch_add(1, Ordering::Relaxed);
        AudioSubGuard(self.clone())
    }

    /// Полный кадр от VNC (например, после потери данных).
    pub fn request_refresh(&self) {
        if let Some(tx) = self.inner.lock().run.as_ref().and_then(|r| r.rfb_tx.clone()) {
            let _ = tx.send(RfbCmd::Refresh);
        }
    }

    // ------------------------------------------------------------ агент

    async fn agent_task(self: Arc<Self>, gen: u64, mut rx: mpsc::Receiver<String>) {
        let path = self.sock("agent.sock");
        while self.run_alive(gen) {
            let s = match localsock::connect_retry(&path, Duration::from_secs(30)).await {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let (rd, mut wr) = tokio::io::split(s);
            let mut lines = BufReader::new(rd).lines();
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    line = lines.next_line() => {
                        match line {
                            Ok(Some(l)) => self.on_agent_line(&l),
                            _ => break,
                        }
                    }
                    msg = rx.recv() => {
                        let Some(mut msg) = msg else { return };
                        msg.push('\n');
                        if wr.write_all(msg.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    _ = tick.tick() => {
                        let mut g = self.inner.lock();
                        if g.agent_connected
                            && g.agent_last_seen.map(|t| t.elapsed() > Duration::from_secs(35)).unwrap_or(true)
                        {
                            g.agent_connected = false;
                            tracing::info!(vm = %self.id, "guest agent timed out");
                        }
                    }
                }
            }
            self.inner.lock().agent_connected = false;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    fn on_agent_line(&self, line: &str) {
        let Some(msg) = agent::parse(line) else {
            tracing::debug!(vm = %self.id, "bad agent line: {line}");
            return;
        };
        {
            let mut g = self.inner.lock();
            g.agent_last_seen = Some(Instant::now());
            g.agent_connected = true;
        }
        match msg {
            GuestMsg::Hello(v) => {
                tracing::info!(vm = %self.id, "guest agent: {v}");
                let levels = self.inner.lock().redstone_in;
                self.agent_send(agent::time_msg());
                self.agent_send(agent::redstone_inputs_msg(&levels));
                self.emit(pb::event::Kind::AgentMessage(pb::AgentMessage {
                    vm_id: self.id.clone(),
                    json: v.to_string(),
                }));
            }
            GuestMsg::Heartbeat => {}
            GuestMsg::RedstoneSet { side, level } => {
                self.emit(pb::event::Kind::RedstoneOutput(pb::RedstoneOutput {
                    vm_id: self.id.clone(),
                    side: side as u32,
                    level,
                }));
            }
            GuestMsg::Other(v) => {
                self.emit(pb::event::Kind::AgentMessage(pb::AgentMessage {
                    vm_id: self.id.clone(),
                    json: v.to_string(),
                }));
            }
        }
    }

    /// Строка JSON агенту гостя. `false`, если машина не запущена или очередь полна.
    pub fn agent_send(&self, json: String) -> bool {
        let tx = self.inner.lock().run.as_ref().map(|r| r.agent_tx.clone());
        match tx {
            Some(tx) => tx.try_send(json.replace('\n', " ")).is_ok(),
            None => false,
        }
    }

    pub fn set_redstone_inputs(&self, levels: &[u32]) {
        let mut l = [0u32; 6];
        for (i, v) in levels.iter().take(6).enumerate() {
            l[i] = (*v).min(15);
        }
        let connected = {
            let mut g = self.inner.lock();
            g.redstone_in = l;
            g.agent_connected
        };
        if connected {
            self.agent_send(agent::redstone_inputs_msg(&l));
        }
    }

    pub fn agent_connected(&self) -> bool {
        self.inner.lock().agent_connected
    }

    // ------------------------------------------------------------ ввод

    pub fn controller(&self) -> Option<Controller> {
        self.inner.lock().controller.clone()
    }

    /// Новый управляющий игрок; прежнему отпускаются все клавиши.
    pub fn set_controller(&self, c: Controller) {
        let changed = {
            let mut g = self.inner.lock();
            let changed = g.controller.as_ref().map(|o| o.conn_id) != Some(c.conn_id);
            g.controller = Some(c.clone());
            changed
        };
        if changed {
            self.release_all();
        }
        // уведомление всегда: задачи соединений сами отсеивают повторы
        let _ = self.notices.send(VmNotice::Controller { conn_id: c.conn_id, player_name: c.player_name });
    }

    pub fn drop_controller(&self, conn_id: u64) {
        let dropped = {
            let mut g = self.inner.lock();
            if g.controller.as_ref().map(|c| c.conn_id) == Some(conn_id) {
                g.controller = None;
                true
            } else {
                false
            }
        };
        if dropped {
            self.release_all();
            let _ = self.notices.send(VmNotice::Controller { conn_id: 0, player_name: String::new() });
        }
    }

    fn is_controller(&self, conn_id: u64) -> bool {
        self.inner.lock().controller.as_ref().map(|c| c.conn_id) == Some(conn_id)
    }

    fn senders(&self) -> (Option<mpsc::UnboundedSender<RfbCmd>>, Option<mpsc::UnboundedSender<Vec<u8>>>) {
        let g = self.inner.lock();
        match g.run.as_ref() {
            Some(r) => (r.rfb_tx.clone(), Some(r.tty_tx.clone())),
            None => (None, None),
        }
    }

    /// Экран, на который идёт ввод: явно указанный или основной.
    fn input_target(&self, screen: &str) -> &'static str {
        match screen {
            SCREEN_TTY => SCREEN_TTY,
            SCREEN_GFX => SCREEN_GFX,
            _ => {
                if self.config().gpu {
                    SCREEN_GFX
                } else {
                    SCREEN_TTY
                }
            }
        }
    }

    pub fn release_all(&self) {
        let (rfb, _) = self.senders();
        let mut st = self.input.lock();
        if let Some(rfb) = rfb {
            for k in st.pressed.drain() {
                let _ = rfb.send(RfbCmd::Key { evdev: k, down: false });
            }
            if st.buttons != 0 {
                st.buttons = 0;
                let _ = rfb.send(RfbCmd::Pointer { mask: 0, x: st.x, y: st.y });
            }
        }
        st.pressed.clear();
        st.buttons = 0;
        st.mods = Mods::default();
    }

    fn clamp_pos(&self, x: u32, y: u32) -> (u16, u16) {
        let (w, h) = self.gfx.size();
        let x = if w > 0 { x.min(w - 1) } else { x };
        let y = if h > 0 { y.min(h - 1) } else { y };
        (x.min(65535) as u16, y.min(65535) as u16)
    }

    fn send_pointer(&self, conn_id: u64, rfb: &mpsc::UnboundedSender<RfbCmd>, st: &mut InputState, x: u32, y: u32) {
        let (x, y) = self.clamp_pos(x, y);
        let moved = (x, y) != (st.x, st.y);
        st.x = x;
        st.y = y;
        let _ = rfb.send(RfbCmd::Pointer { mask: st.buttons, x, y });
        if moved {
            let _ = self.notices.send(VmNotice::CursorPos { x: x as u32, y: y as u32, from_conn: conn_id });
        }
    }

    /// Движение мыши (датаграмма PointerMove).
    pub fn pointer_move(&self, conn_id: u64, perms: u32, x: u32, y: u32) {
        if perms & pb::Permission::PermMouse as u32 == 0 || !self.is_controller(conn_id) {
            return;
        }
        let (Some(rfb), _) = self.senders() else { return };
        let mut st = self.input.lock();
        self.send_pointer(conn_id, &rfb, &mut st, x, y);
    }

    pub fn input(&self, conn_id: u64, perms: u32, screen: &str, ev: spb::input_event::Kind) {
        use spb::input_event::Kind;
        if !self.is_controller(conn_id) {
            return;
        }
        let kb = perms & pb::Permission::PermKeyboard as u32 != 0;
        let mouse = perms & pb::Permission::PermMouse as u32 != 0;
        let target = self.input_target(screen);
        let (rfb, tty) = self.senders();
        match ev {
            Kind::ReleaseAll(_) => self.release_all(),
            Kind::Key(k) if kb => {
                if target == SCREEN_GFX {
                    let Some(rfb) = rfb else { return };
                    let mut st = self.input.lock();
                    if k.down {
                        st.pressed.insert(k.evdev);
                    } else if !st.pressed.remove(&k.evdev) {
                        return; // отпускание без нажатия
                    }
                    let _ = rfb.send(RfbCmd::Key { evdev: k.evdev, down: k.down });
                } else {
                    let Some(tty) = tty else { return };
                    let seq = {
                        let mut st = self.input.lock();
                        if st.mods.update(k.evdev, k.down) || !k.down || keys::is_modifier(k.evdev) {
                            None
                        } else {
                            text::key_sequence(k.evdev, st.mods, self.tty.application_cursor())
                        }
                    };
                    if let Some(seq) = seq {
                        let _ = tty.send(seq);
                    }
                }
            }
            Kind::Text(t) if kb => {
                if target == SCREEN_TTY && !t.utf8.is_empty() {
                    if let Some(tty) = tty {
                        let _ = tty.send(t.utf8.into_bytes());
                    }
                }
            }
            Kind::Buttons(b) if mouse && target == SCREEN_GFX => {
                let Some(rfb) = rfb else { return };
                let mut st = self.input.lock();
                // протокол: 1 левая, 2 правая, 4 средняя -> VNC: 1 левая, 2 средняя, 4 правая
                let mut m = 0u8;
                if b.mask & 1 != 0 {
                    m |= 1;
                }
                if b.mask & 2 != 0 {
                    m |= 4;
                }
                if b.mask & 4 != 0 {
                    m |= 2;
                }
                st.buttons = m;
                self.send_pointer(conn_id, &rfb, &mut st, b.x, b.y);
            }
            Kind::Wheel(w) if mouse && target == SCREEN_GFX => {
                let Some(rfb) = rfb else { return };
                let mut st = self.input.lock();
                let (x, y) = self.clamp_pos(w.x, w.y);
                st.x = x;
                st.y = y;
                let clicks = |n: i32, pos: u8, neg: u8| {
                    let bit = if n > 0 { pos } else { neg };
                    for _ in 0..n.unsigned_abs().min(20) {
                        let _ = rfb.send(RfbCmd::Pointer { mask: st.buttons | bit, x, y });
                        let _ = rfb.send(RfbCmd::Pointer { mask: st.buttons, x, y });
                    }
                };
                clicks(w.dy, 0x08, 0x10);
                clicks(w.dx, 0x40, 0x20);
            }
            Kind::Pointer(p) if mouse && target == SCREEN_GFX => {
                let Some(rfb) = rfb else { return };
                let mut st = self.input.lock();
                self.send_pointer(conn_id, &rfb, &mut st, p.x, p.y);
            }
            _ => {}
        }
    }

    pub fn cursor_pos(&self) -> (u32, u32) {
        let st = self.input.lock();
        (st.x as u32, st.y as u32)
    }

    /// Удаление файлов машины (после остановки).
    pub fn remove_files(&self) -> Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir).with_context(|| format!("remove {}", self.dir.display()))?;
        }
        if self.sock_dir != self.dir {
            let _ = std::fs::remove_dir_all(&self.sock_dir);
        }
        Ok(())
    }
}

async fn image_virtual_size(qemu_img: &Path, image: &Path) -> Result<u64> {
    let out = tokio::process::Command::new(qemu_img).args(["info", "--output=json", "-U"]).arg(image).output().await?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    v.get("virtual-size").and_then(|s| s.as_u64()).ok_or_else(|| anyhow!("no virtual-size"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accel::Accel;

    fn test_vm(gpu: bool, sound: bool) -> (Arc<Vm>, Image) {
        let env = Arc::new(Env {
            images_dir: PathBuf::from("/images"),
            qemu: QemuLocator::fixed(None),
            tz: Some("Europe/Moscow".into()),
            net: None,
        });
        let (ev, _) = broadcast::channel(8);
        let cfg = VmConfig {
            vm_id: "vm1".into(),
            image_id: "img".into(),
            cpus: 1,
            memory_mb: 512,
            gpu,
            sound,
            network: true,
            lang: "ru_RU.UTF-8".into(),
            kbd_layout: "us,ru".into(),
            ..Default::default()
        };
        let vm = Vm::new("vm1", Path::new("/vms"), cfg, env, ev);
        let img = Image {
            dir: PathBuf::from("/images/img"),
            manifest: images::Manifest { cmdline: "root=/dev/vda console=hvc0".into(), ..Default::default() },
            version: "test".into(),
        };
        (vm, img)
    }

    fn test_qemu(bundled: bool) -> QemuInfo {
        QemuInfo {
            qemu: PathBuf::from("/q/bin/qemu-system-x86_64"),
            qemu_img: None,
            bundled,
            data_dir: bundled.then(|| PathBuf::from("/q/share/qemu")),
            version: Some((8, 2, 2)),
            version_str: "8.2.2".into(),
            accel: Accel::Tcg,
        }
    }

    #[test]
    fn args_contract() {
        let (vm, img) = test_vm(true, true);
        let a = vm
            .qemu_args(
                &vm.config(),
                &img,
                Path::new("/vms/vm1/disk.qcow2"),
                Some(&NetAttach::Unix("/vms/vm1/net.sock".into())),
                &test_qemu(false),
            )
            .join(" ");
        assert!(a.contains("-machine q35"));
        assert!(!a.contains(" -L "), "system QEMU has no -L");
        assert!(a.contains("-accel tcg,thread=multi -cpu max"));
        assert!(a.contains("-m 512"));
        assert!(
            a.contains("-append root=/dev/vda console=hvc0 fcw.lang=ru_RU.UTF-8 fcw.kbd=us,ru fcw.tz=Europe/Moscow")
        );
        assert!(a.contains("file=/vms/vm1/disk.qcow2,format=qcow2,discard=unmap"));
        assert!(a.contains("socket,id=con0,path=/vms/vm1/tty.sock,server=on,wait=off"));
        assert!(a.contains("virtserialport,chardev=ag0,name=fcw.agent"));
        assert!(a.contains("-qmp unix:/vms/vm1/qmp.sock,server=on,wait=off"));
        assert!(a.contains("virtio-gpu-pci,xres=1024,yres=768,edid=on"));
        assert!(a.contains("-vnc unix:/vms/vm1/vnc.sock,audiodev=snd0"));
        assert!(a.contains("-audiodev none,id=snd0 -device virtio-sound-pci,audiodev=snd0"));
        assert!(a.contains("fcw.tz=Europe/Moscow systemd.hostname=pc-vm1"));
        assert!(a.contains("-netdev stream,id=net0,server=off,addr.type=unix,addr.path=/vms/vm1/net.sock,reconnect=1"));
        let mac = net::mac_string(fcw_net::NetSwitch::mac_for_vm("vm1"));
        assert!(a.contains(&format!("-device virtio-net-pci,netdev=net0,mac={mac},romfile=")));
        let (vm, img) = test_vm(false, false);
        let a = vm.qemu_args(&vm.config(), &img, Path::new("/d.qcow2"), None, &test_qemu(true)).join(" ");
        assert!(!a.contains("-vnc"));
        assert!(a.contains("-accel tcg,thread=multi -cpu max -L /q/share/qemu"), "portable QEMU gets -L");
        assert!(!a.contains("virtio-gpu"));
        // звук без видеокарты: VNC только ради звука
        let (vm, img) = test_vm(false, true);
        let a = vm.qemu_args(&vm.config(), &img, Path::new("/d.qcow2"), None, &test_qemu(true)).join(" ");
        assert!(a.contains("-vnc unix:/vms/vm1/vnc.sock,audiodev=snd0"));
        assert!(!a.contains("virtio-gpu"));
    }

    #[test]
    fn escape_commas() {
        assert_eq!(qemu_escape(Path::new("/a,b/c")), "/a,,b/c");
    }

    #[test]
    fn long_paths_use_short_socket_dir() {
        let long = PathBuf::from(format!("/{}/vm", "x".repeat(120)));
        let d = socket_dir(&long);
        assert!(d.join("agent.sock").to_string_lossy().len() < 108);
        assert_eq!(socket_dir(Path::new("/vms/a")), PathBuf::from("/vms/a"));
    }
}
