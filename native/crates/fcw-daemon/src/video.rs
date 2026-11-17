//! Видеорежим экрана: кодировщики H.264 для видеообласти ([`crate::gfx`]).
//!
//! Кодируем один раз на экран и ступень битрейта, а не на каждого зрителя:
//! у экрана есть [`VideoHub`], зритель просит у него ступень по своему бюджету
//! ([`VideoHub::subscribe`]) и получает кадры из общей рассылки. Каждый
//! кодировщик — отдельный поток ОС; общее число кодировщиков в процессе
//! ограничено (`--video-max-encoders`), лишним зрителям достаётся ступень
//! пониже из уже работающих, а если нет и такой — тайлы, как без видеорежима.
//!
//! Дальним зрителям (меньше к/с) идут только нижние временные слои того же
//! потока, см. [`crate::h264`].

use crate::gfx::{GfxScreen, VideoRegion};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Настройки видеорежима (`--video*`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoConfig {
    pub enabled: bool,
    /// Ступени битрейта, кбит/с, по возрастанию; 0 — ступени нет.
    pub tiers_kbps: [u32; 4],
    /// Ступень для игрока на этом же хосте (`same_host`).
    pub local_kbps: u32,
    /// Частота кадров кодировщика.
    pub fps: u32,
    /// Предел числа кодировщиков во всём демоне.
    pub max_encoders: u32,
    /// Потоков на кодировщик.
    pub threads: u16,
    /// Видео — это когда за кадр меняется такая доля экрана.
    pub min_area: f32,
}

/// Предел кодировщиков по умолчанию: половина ядер, но не меньше двух.
pub fn default_max_encoders() -> u32 {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2) as u32;
    (cores / 2).max(2)
}

impl Default for VideoConfig {
    fn default() -> Self {
        VideoConfig {
            enabled: cfg!(feature = "video"),
            tiers_kbps: [2_000, 5_000, 10_000, 0],
            local_kbps: 20_000,
            fps: 30,
            max_encoders: default_max_encoders(),
            threads: 1,
            min_area: 0.2,
        }
    }
}

impl VideoConfig {
    /// Разбирает список ступеней «2,5,10» (Мбит/с): до четырёх, по возрастанию.
    pub fn parse_tiers(s: &str) -> Result<[u32; 4], String> {
        let mut v: Vec<u32> = Vec::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let m: f64 = part.parse().map_err(|_| format!("bad bitrate {part:?}"))?;
            if !(0.1..=200.0).contains(&m) {
                return Err(format!("bitrate {part} is out of range (0.1 to 200 Mbit/s)"));
            }
            v.push((m * 1000.0) as u32);
        }
        if v.is_empty() || v.len() > 4 {
            return Err("expected 1 to 4 bitrates".into());
        }
        v.sort_unstable();
        v.dedup();
        let mut out = [0u32; 4];
        out[..v.len()].copy_from_slice(&v);
        Ok(out)
    }

    /// Наибольшая ступень не выше `max_kbps`.
    pub fn tier_for(&self, max_kbps: u32) -> Option<u32> {
        self.tiers_kbps.iter().copied().filter(|t| *t != 0 && *t <= max_kbps).max()
    }

    /// Ближайшая ступень ниже `kbps`.
    pub fn tier_below(&self, kbps: u32) -> Option<u32> {
        self.tiers_kbps.iter().copied().filter(|t| *t != 0 && *t < kbps).max()
    }

    /// Старший временной слой для зрителя с частотой `fps`: 2 — все кадры,
    /// 1 — половина, 0 — четверть.
    pub fn max_tid_for(&self, fps: u32) -> u8 {
        let full = self.fps.max(1);
        if fps * 10 >= full * 8 {
            2
        } else if fps * 10 >= full * 4 {
            1
        } else {
            0
        }
    }
}

/// Закодированный кадр видеообласти.
#[derive(Debug)]
pub struct VideoFrame {
    /// Номер потока: новый у каждого кодировщика и каждой области.
    pub stream: u32,
    pub key: bool,
    /// Временной слой (0 — базовый).
    pub tid: u8,
    pub region: VideoRegion,
    /// Access unit, Annex B.
    pub data: Vec<u8>,
    /// Номер изменения экрана и время его фиксации (Unix, мкс).
    pub seq: u64,
    pub capture_unix_us: u64,
}

struct Enc {
    kbps: u32,
    tx: broadcast::Sender<Arc<VideoFrame>>,
    viewers: AtomicU32,
    want_idr: AtomicBool,
    /// Кодировщик не запустился или сломался: подписчикам пора уходить.
    dead: AtomicBool,
}

struct HubInner {
    gfx: Arc<GfxScreen>,
    cfg: VideoConfig,
    encs: Mutex<Vec<Arc<Enc>>>,
}

/// Кодировщики одного экрана.
pub struct VideoHub {
    inner: Arc<HubInner>,
}

/// Подписка зрителя на ступень. При удалении освобождает место.
pub struct VideoSub {
    pub rx: broadcast::Receiver<Arc<VideoFrame>>,
    pub kbps: u32,
    enc: Arc<Enc>,
}

impl VideoSub {
    /// Просит ключевой кадр (новый зритель, сброшенная очередь, RequestKeyframe).
    pub fn request_idr(&self) {
        self.enc.want_idr.store(true, Ordering::Release);
    }

    pub fn is_dead(&self) -> bool {
        self.enc.dead.load(Ordering::Acquire)
    }
}

impl Drop for VideoSub {
    fn drop(&mut self) {
        self.enc.viewers.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Сколько кодировщиков работает во всём процессе.
static ACTIVE: AtomicU32 = AtomicU32::new(0);

/// Счётчики всех кодировщиков процесса (для замеров и журнала).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VideoStats {
    /// Закодировано кадров, из них ключевых.
    pub frames: u64,
    pub keyframes: u64,
    /// Кадры, пропущенные кодировщиком ради битрейта.
    pub skipped: u64,
    pub bytes: u64,
    /// Время кодирования (с переводом в YUV): сумма и наибольшее, мкс.
    pub encode_us: u64,
    pub encode_max_us: u64,
}

static STATS: Mutex<VideoStats> =
    Mutex::new(VideoStats { frames: 0, keyframes: 0, skipped: 0, bytes: 0, encode_us: 0, encode_max_us: 0 });

/// Счётчики кодировщиков с начала работы процесса.
pub fn stats() -> VideoStats {
    *STATS.lock()
}

/// Занимает место под кодировщик, если предел не достигнут.
fn take_slot(max: u32) -> bool {
    let mut n = ACTIVE.load(Ordering::Acquire);
    while n < max {
        match ACTIVE.compare_exchange_weak(n, n + 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(cur) => n = cur,
        }
    }
    false
}

/// Число работающих кодировщиков (для журнала и тестов).
pub fn active_encoders() -> u32 {
    ACTIVE.load(Ordering::Acquire)
}

impl VideoHub {
    pub fn new(gfx: Arc<GfxScreen>, cfg: VideoConfig) -> Self {
        VideoHub { inner: Arc::new(HubInner { gfx, cfg, encs: Mutex::new(Vec::new()) }) }
    }

    pub fn config(&self) -> &VideoConfig {
        &self.inner.cfg
    }

    /// Подписывает зрителя на ступень не выше `max_kbps` (для `local` — на
    /// ступень игрока этого хоста). `None` — видео этому зрителю не достанется
    /// (выключено, бюджет меньше младшей ступени, нет свободных кодировщиков).
    pub fn subscribe(&self, max_kbps: u32, local: bool) -> Option<VideoSub> {
        let cfg = &self.inner.cfg;
        if !cfg.enabled || !cfg!(feature = "video") {
            return None;
        }
        let want = if local { cfg.local_kbps } else { cfg.tier_for(max_kbps)? };
        let mut encs = self.inner.encs.lock();
        encs.retain(|e| !e.dead.load(Ordering::Acquire));
        let join = |e: &Arc<Enc>| {
            e.viewers.fetch_add(1, Ordering::AcqRel);
            e.want_idr.store(true, Ordering::Release);
            VideoSub { rx: e.tx.subscribe(), kbps: e.kbps, enc: e.clone() }
        };
        if let Some(e) = encs.iter().find(|e| e.kbps == want) {
            return Some(join(e));
        }
        if take_slot(cfg.max_encoders) {
            let (tx, rx) = broadcast::channel(16);
            let enc = Arc::new(Enc {
                kbps: want,
                tx,
                viewers: AtomicU32::new(1),
                want_idr: AtomicBool::new(false),
                dead: AtomicBool::new(false),
            });
            if spawn_encoder(self.inner.clone(), enc.clone()) {
                encs.push(enc.clone());
                return Some(VideoSub { rx, kbps: want, enc });
            }
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
        }
        // свободных кодировщиков нет: ступень пониже из работающих
        encs.iter().filter(|e| e.kbps < want).max_by_key(|e| e.kbps).map(join)
    }
}

#[cfg(not(feature = "video"))]
fn spawn_encoder(_hub: Arc<HubInner>, _enc: Arc<Enc>) -> bool {
    false
}

#[cfg(feature = "video")]
fn spawn_encoder(hub: Arc<HubInner>, enc: Arc<Enc>) -> bool {
    let r = std::thread::Builder::new().name("fcw-h264".into()).spawn(move || {
        encoder::run(&hub, &enc);
        enc.dead.store(true, Ordering::Release);
        hub.encs.lock().retain(|e| !Arc::ptr_eq(e, &enc));
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    });
    if let Err(e) = &r {
        tracing::warn!("video encoder thread: {e}");
    }
    r.is_ok()
}

#[cfg(feature = "video")]
mod encoder {
    use super::*;
    use crate::gfx::Capture;
    use crate::h264::{H264Config, H264Encoder};
    use std::time::{Duration, Instant};

    /// Номера потоков (общие на процесс, с 1).
    static STREAMS: AtomicU32 = AtomicU32::new(1);
    /// Кодировщик без зрителей живёт столько (зритель мог пересесть на миг).
    const LINGER: Duration = Duration::from_secs(2);
    /// Ключевые кадры по запросу — не чаще.
    const IDR_SPACING: Duration = Duration::from_millis(300);
    /// Ступени выше этой кодируются CAVLC: CABAC на таком битрейте заметно
    /// нагружает декодер клиента, а выигрыш в качестве уже мал.
    const CABAC_MAX_KBPS: u32 = 12_000;

    struct Active {
        enc: H264Encoder,
        region: VideoRegion,
        stream: u32,
        last_seq: u64,
    }

    pub fn run(hub: &HubInner, shared: &Enc) {
        let fps = hub.cfg.fps.clamp(1, 60);
        let interval = Duration::from_micros(1_000_000 / fps as u64);
        let started = Instant::now();
        let mut active: Option<Active> = None;
        let mut buf: Vec<u8> = Vec::new();
        let mut last_frame = started - interval;
        let mut last_idr = started;
        let mut idle_since: Option<Instant> = None;
        loop {
            // зрители ушли — выходим (под блокировкой: новый зритель не потеряется)
            if shared.viewers.load(Ordering::Acquire) == 0 {
                let since = *idle_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= LINGER {
                    let encs = hub.encs.lock();
                    if shared.viewers.load(Ordering::Acquire) == 0 {
                        shared.dead.store(true, Ordering::Release);
                        drop(encs);
                        return;
                    }
                }
            } else {
                idle_since = None;
            }
            let Some(region) = hub.gfx.video_region() else {
                active = None;
                std::thread::sleep(Duration::from_millis(20));
                continue;
            };
            if active.as_ref().is_none_or(|a| a.region != region) {
                let mut cfg = H264Config::new(region.w, region.h, shared.kbps.saturating_mul(1000), fps as f32);
                cfg.threads = hub.cfg.threads.max(1);
                cfg.cabac = shared.kbps <= CABAC_MAX_KBPS;
                match H264Encoder::new(cfg) {
                    Ok(enc) => {
                        let stream = STREAMS.fetch_add(1, Ordering::Relaxed);
                        tracing::debug!(
                            "video stream {stream}: {}x{} at ({}, {}), {} kbit/s",
                            region.w,
                            region.h,
                            region.x,
                            region.y,
                            shared.kbps
                        );
                        active = Some(Active { enc, region, stream, last_seq: 0 });
                        shared.want_idr.store(false, Ordering::Release);
                        last_idr = Instant::now();
                    }
                    Err(e) => {
                        tracing::warn!("video encoder {}x{}: {e:#}", region.w, region.h);
                        return;
                    }
                }
            }
            let a = active.as_mut().expect("set above");
            let now = Instant::now();
            if now < last_frame + interval {
                std::thread::sleep(last_frame + interval - now);
            }
            let (seq, capture_unix_us) = match hub.gfx.capture_region(region.id, a.last_seq, &mut buf) {
                Capture::Frame { seq, capture_unix_us } => (seq, capture_unix_us),
                Capture::Unchanged => {
                    // картинка стоит; ключевой кадр по запросу всё равно нужен
                    if shared.want_idr.load(Ordering::Acquire) && last_idr.elapsed() >= IDR_SPACING {
                        a.last_seq = 0;
                    } else {
                        std::thread::sleep(Duration::from_millis(3));
                    }
                    continue;
                }
                Capture::Gone => {
                    active = None;
                    continue;
                }
            };
            last_frame = Instant::now();
            a.last_seq = seq;
            if shared.viewers.load(Ordering::Acquire) == 0 {
                continue; // некому слать: не тратим CPU, пока кодировщик доживает
            }
            if last_idr.elapsed() >= IDR_SPACING && shared.want_idr.swap(false, Ordering::AcqRel) {
                a.enc.force_idr();
            }
            let t = Instant::now();
            let encoded = a.enc.encode_rgba(&buf, started.elapsed().as_millis() as u64);
            {
                let us = t.elapsed().as_micros() as u64;
                let mut st = STATS.lock();
                st.encode_us += us;
                st.encode_max_us = st.encode_max_us.max(us);
                match &encoded {
                    Ok(Some(e)) => {
                        st.frames += 1;
                        st.keyframes += e.key as u64;
                        st.bytes += e.data.len() as u64;
                    }
                    Ok(None) => st.skipped += 1,
                    Err(_) => {}
                }
            }
            match encoded {
                Ok(Some(e)) => {
                    if e.key {
                        last_idr = Instant::now();
                    }
                    let _ = shared.tx.send(Arc::new(VideoFrame {
                        stream: a.stream,
                        key: e.key,
                        tid: e.temporal_id,
                        region,
                        data: e.data,
                        seq,
                        capture_unix_us,
                    }));
                }
                Ok(None) => {} // кодировщик пропустил кадр ради битрейта
                Err(e) => {
                    tracing::warn!("video encode: {e:#}");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers() {
        let mut c = VideoConfig::default();
        assert_eq!(c.tier_for(14_000), Some(10_000));
        assert_eq!(c.tier_for(5_000), Some(5_000));
        assert_eq!(c.tier_for(1_999), None);
        assert_eq!(c.tier_below(10_000), Some(5_000));
        assert_eq!(c.tier_below(2_000), None);
        assert_eq!(VideoConfig::parse_tiers("10, 2,5").unwrap(), [2_000, 5_000, 10_000, 0]);
        assert_eq!(VideoConfig::parse_tiers("1.5").unwrap(), [1_500, 0, 0, 0]);
        assert!(VideoConfig::parse_tiers("").is_err());
        assert!(VideoConfig::parse_tiers("1,2,3,4,5").is_err());
        assert!(VideoConfig::parse_tiers("x").is_err());
        c.fps = 30;
        assert_eq!((c.max_tid_for(30), c.max_tid_for(24), c.max_tid_for(15), c.max_tid_for(10)), (2, 2, 1, 0));
    }

    #[test]
    fn disabled_hub_gives_nothing() {
        let hub = VideoHub::new(Arc::new(GfxScreen::new()), VideoConfig { enabled: false, ..Default::default() });
        assert!(hub.subscribe(100_000, false).is_none());
    }
}
