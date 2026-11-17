//! Графический экран (gfx0): кадровый буфер RGBA, разбитый на тайлы 64x64.
//!
//! VNC-клиент пишет прямоугольники в буфер ([`GfxScreen::apply_rgbx`]) и в
//! конце каждого FramebufferUpdate фиксирует изменения ([`GfxScreen::commit`]).
//! Каждый тайл помнит номер изменения (seq), а подписчики забирают тайлы,
//! изменившиеся после их последней отправки. Сжатые тайлы кешируются, чтобы
//! несколько зрителей не сжимали одно и то же.
//!
//! Тайлы кодируются без потерь (сплошной цвет или RGBA + zstd), а «пёстрые»
//! (много цветов: фото, видео) — JPEG, если клиент это принимает и канал не
//! успевает или тайл меняется очень часто. Такие тайлы подписчик запоминает и
//! позже, когда тайл успокоился, досылает без потерь ([`GfxScreen::collect`]
//! с `refine`).
//!
//! Видеорежим: [`GfxScreen::commit`] следит за тем, какая часть экрана меняется
//! кадр за кадром. Если большая область меняется несколько кадров подряд, она
//! объявляется видеообластью ([`GfxScreen::video_region`]); её кадры забирает
//! кодировщик ([`GfxScreen::capture_region`], модуль `video`), а подписчики с
//! видеокодеком пропускают её тайлы (`EncodePolicy::skip`). Когда движение
//! стихает, область снимается, и подписчики досылают её тайлы без потерь.

use crate::spb;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::watch;

pub const TILE: u32 = 64;
const ZSTD_LEVEL: i32 = 1;
/// Тайл «пёстрый», если в нём больше стольких разных цветов (текст и
/// интерфейс обычно укладываются, фото и видео — нет).
pub const BUSY_COLORS: usize = 64;
/// Порог для тайлов, которые меняются очень часто, когда канал не успевает
/// (прокрутка сглаженного текста): их тоже можно временно слать JPEG.
/// Консольный текст и плоский интерфейс (несколько цветов) — всегда без потерь.
pub const HOT_COLORS: usize = 16;
/// Тайл «горячий», если он менялся 3 раза за это время (видео, прокрутка).
pub const HOT_WINDOW_US: u64 = 1_000_000;
/// Тайл «успокоился», если не менялся столько времени: можно досылать без потерь.
pub const SETTLE_US: u64 = 400_000;
pub const JPEG_QUALITY: u8 = 70;

/// Сколько изменившихся тайлов кадра проверять на «пёстрость» (равномерно).
const VIDEO_SAMPLE_TILES: usize = 32;
/// «Большое» изменение считается видео, если такая доля проверенных тайлов —
/// «пёстрые» (больше [`BUSY_COLORS`] цветов). Поток текста в терминале или
/// прокрутка страницы меняют весь экран, но это не видео: их тайлы
/// малоцветные и должны идти без потерь.
const VIDEO_BUSY_FRACTION: f32 = 0.34;
/// Видео начинается, когда столько «больших» изменений пришло за
/// [`VIDEO_ON_WINDOW_US`] (при 30 к/с — около 0,2 с движения).
pub const VIDEO_ON_FRAMES: usize = 6;
pub const VIDEO_ON_WINDOW_US: u64 = 500_000;
/// Видео кончилось, если столько времени в области ничего заметного не менялось.
pub const VIDEO_IDLE_US: u64 = 1_500_000;
/// «Заметное» изменение: такая доля тайлов видеообласти (тихая сцена фильма
/// меняет меньше 20 % экрана, но режим не должен из-за неё выключаться).
pub const VIDEO_KEEP_FRACTION: f32 = 0.05;
/// Область, занявшая такую долю экрана, расширяется до всего экрана.
pub const VIDEO_FULL_FRACTION: f32 = 0.75;
/// Столько больших изменений подряд вышло за область — область растёт.
pub const VIDEO_GROW_FRAMES: u32 = 3;
/// Раз в такое время область сжимается, если движение заняло не больше
/// [`VIDEO_SHRINK_FRACTION`] её площади (плеер вышел из полноэкранного режима).
pub const VIDEO_SHRINK_WINDOW_US: u64 = 3_000_000;
pub const VIDEO_SHRINK_FRACTION: f32 = 0.6;

/// Настройки распознавания видео на экране.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoDetect {
    pub enabled: bool,
    /// «Большое» изменение: такая доля тайлов экрана за один кадр (и эти
    /// тайлы «пёстрые», как у фото и видео).
    pub min_area: f32,
}

impl Default for VideoDetect {
    fn default() -> Self {
        VideoDetect { enabled: false, min_area: 0.2 }
    }
}

/// Видеообласть экрана в пикселях: границы по тайлам (или по краю экрана).
/// `id` меняется при каждой смене области.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoRegion {
    pub id: u64,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Прямоугольник в тайлах: [x0, x1) x [y0, y1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TileBox {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl TileBox {
    fn of_tile(tx: u32, ty: u32) -> Self {
        TileBox { x0: tx, y0: ty, x1: tx + 1, y1: ty + 1 }
    }

    fn of_region(r: &VideoRegion) -> Self {
        TileBox { x0: r.x / TILE, y0: r.y / TILE, x1: (r.x + r.w).div_ceil(TILE), y1: (r.y + r.h).div_ceil(TILE) }
    }

    fn area(&self) -> u32 {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }

    fn union(self, o: TileBox) -> TileBox {
        TileBox { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }

    fn contains(&self, o: &TileBox) -> bool {
        self.x0 <= o.x0 && self.y0 <= o.y0 && self.x1 >= o.x1 && self.y1 >= o.y1
    }

    fn has_tile(&self, tx: u32, ty: u32) -> bool {
        tx >= self.x0 && tx < self.x1 && ty >= self.y0 && ty < self.y1
    }
}

fn join(a: Option<TileBox>, b: TileBox) -> TileBox {
    a.map_or(b, |a| a.union(b))
}

/// Состояние распознавания видео (внутри [`Inner`]).
#[derive(Default)]
struct VideoState {
    cfg: VideoDetect,
    /// Недавние большие изменения: (время, охват).
    big: std::collections::VecDeque<(u64, TileBox)>,
    region: Option<(VideoRegion, TileBox)>,
    ids: u64,
    /// Время последнего заметного изменения в области.
    active_us: u64,
    /// Номер последнего изменения (seq), затронувшего область.
    dirty_seq: u64,
    /// Большие изменения подряд, вышедшие за область, и их охват.
    grow: u32,
    grow_box: Option<TileBox>,
    /// Охват заметных изменений за текущее окно сжатия.
    win_start_us: u64,
    win_box: Option<TileBox>,
}

/// Результат [`GfxScreen::capture_region`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Области больше нет (видео кончилось, область или размер экрана сменились).
    Gone,
    /// С прошлого захвата область не менялась.
    Unchanged,
    /// Кадр скопирован: номер изменения и время его фиксации (Unix, мкс).
    Frame { seq: u64, capture_unix_us: u64 },
}

/// Форма аппаратного курсора гостя (RGBA).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cursor {
    pub width: u32,
    pub height: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub rgba: Vec<u8>,
}

impl Cursor {
    pub fn visible(&self) -> bool {
        self.width > 0 && self.height > 0 && self.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 0)
    }
}

struct Inner {
    width: u32,
    height: u32,
    fb: Vec<u8>,
    tiles_x: u32,
    tiles_y: u32,
    tile_ver: Vec<u64>,
    /// Время трёх последних изменений тайла (Unix, мкс), новое первым.
    changes: Vec<[u64; 3]>,
    /// Тайлы, изменённые в незафиксированном обновлении.
    pending_tiles: Vec<usize>,
    /// Сжатые без потерь тайлы по версии.
    cache: Vec<Option<(u64, Arc<spb::Rect>)>>,
    /// JPEG по версии.
    jpeg: Vec<Option<(u64, Arc<spb::Rect>)>>,
    /// Число цветов тайла этой версии (не больше BUSY_COLORS + 1).
    colors: Vec<Option<(u64, u16)>>,
    /// Номер последнего зафиксированного изменения.
    seq: u64,
    /// Меняется при смене размера/сбросе: подписчикам нужен ключевой кадр.
    epoch: u64,
    /// Есть ли незафиксированные изменения тайлов (помечены как seq + 1).
    pending: bool,
    cursor: Cursor,
    /// Время фиксации изменения `seq` (Unix, мкс).
    seq_time_us: u64,
    video: VideoState,
}

impl Inner {
    fn alloc(&mut self, w: u32, h: u32) {
        self.width = w;
        self.height = h;
        self.fb = vec![0; (w as usize) * (h as usize) * 4];
        for px in self.fb.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        self.tiles_x = w.div_ceil(TILE);
        self.tiles_y = h.div_ceil(TILE);
        let n = (self.tiles_x * self.tiles_y) as usize;
        self.seq += 1;
        self.epoch += 1;
        self.tile_ver = vec![self.seq; n];
        self.changes = vec![[0; 3]; n];
        self.pending_tiles.clear();
        self.cache = vec![None; n];
        self.jpeg = vec![None; n];
        self.colors = vec![None; n];
        self.pending = false;
        self.seq_time_us = crate::unix_us();
        let cfg = self.video.cfg;
        let ids = self.video.ids;
        self.video = VideoState { cfg, ids, ..Default::default() };
    }

    fn set_video_region(&mut self, b: TileBox, now: u64) {
        let v = &mut self.video;
        v.ids += 1;
        let x = b.x0 * TILE;
        let y = b.y0 * TILE;
        let r = VideoRegion {
            id: v.ids,
            x,
            y,
            w: (b.x1 * TILE).min(self.width) - x,
            h: (b.y1 * TILE).min(self.height) - y,
        };
        v.region = Some((r, b));
        v.active_us = now;
        v.dirty_seq = self.seq;
        v.big.clear();
        v.grow = 0;
        v.grow_box = None;
        v.win_start_us = now;
        v.win_box = None;
    }

    /// Похоже ли изменение на фото или видео: выборка изменившихся тайлов
    /// в основном «пёстрая». Число цветов запоминается для [`GfxScreen::collect`].
    fn mostly_busy(&mut self, tiles: &[usize]) -> bool {
        let step = tiles.len().div_ceil(VIDEO_SAMPLE_TILES).max(1);
        let (mut busy, mut n) = (0usize, 0usize);
        for &i in tiles.iter().step_by(step) {
            let ver = self.tile_ver[i];
            let colors = match self.colors[i] {
                Some((v, c)) if v == ver => c as usize,
                _ => {
                    let c = count_colors(&self.copy_tile(i).4, BUSY_COLORS + 1);
                    self.colors[i] = Some((ver, c as u16));
                    c
                }
            };
            busy += (colors > BUSY_COLORS) as usize;
            n += 1;
        }
        busy as f32 >= VIDEO_BUSY_FRACTION * n as f32
    }

    /// Учитывает зафиксированное изменение тайлов `tiles` (см. шапку модуля).
    fn detect_video(&mut self, tiles: &[usize], now: u64) {
        if !self.video.cfg.enabled || tiles.is_empty() {
            return;
        }
        let total = self.tiles_x * self.tiles_y;
        let min_area = self.video.cfg.min_area;
        let full = TileBox { x0: 0, y0: 0, x1: self.tiles_x, y1: self.tiles_y };
        let region = self.video.region.map(|(_, b)| b);
        let mut bbox: Option<TileBox> = None;
        let mut inside_box: Option<TileBox> = None;
        let mut inside = 0u32;
        for &t in tiles {
            let (tx, ty) = (t as u32 % self.tiles_x, t as u32 / self.tiles_x);
            let b = TileBox::of_tile(tx, ty);
            bbox = Some(join(bbox, b));
            if region.is_some_and(|r| r.has_tile(tx, ty)) {
                inside += 1;
                inside_box = Some(join(inside_box, b));
            }
        }
        let bbox = bbox.expect("tiles is not empty");
        let big = tiles.len() as f32 >= min_area * total as f32 && self.mostly_busy(tiles);
        let seq = self.seq;
        let Some(rb) = region else {
            let v = &mut self.video;
            if big {
                v.big.push_back((now, bbox));
            }
            while v.big.front().is_some_and(|(t, _)| now.saturating_sub(*t) > VIDEO_ON_WINDOW_US) {
                v.big.pop_front();
            }
            if v.big.len() >= VIDEO_ON_FRAMES {
                // охват — по последним кадрам: раньше могло случиться постороннее
                // большое изменение (открылось окно плеера)
                let mut b = v.big.iter().rev().take(3).map(|(_, b)| *b).reduce(TileBox::union).expect("not empty");
                if b.area() as f32 >= VIDEO_FULL_FRACTION * total as f32 {
                    b = full;
                }
                self.set_video_region(b, now);
            }
            return;
        };
        let v = &mut self.video;
        if inside > 0 {
            v.dirty_seq = seq;
        }
        if inside as f32 >= (VIDEO_KEEP_FRACTION * rb.area() as f32).max(2.0) {
            v.active_us = now;
            if let Some(b) = inside_box {
                v.win_box = Some(join(v.win_box, b));
            }
        }
        if big {
            if rb.contains(&bbox) {
                v.grow = 0;
                v.grow_box = None;
            } else {
                v.grow += 1;
                v.grow_box = Some(join(v.grow_box, bbox));
                if v.grow >= VIDEO_GROW_FRAMES {
                    let mut b = rb.union(v.grow_box.expect("set above"));
                    if b.area() as f32 >= VIDEO_FULL_FRACTION * total as f32 {
                        b = full;
                    }
                    self.set_video_region(b, now);
                    return;
                }
            }
        }
        if now.saturating_sub(v.win_start_us) >= VIDEO_SHRINK_WINDOW_US {
            let shrink = v.win_box.filter(|wb| {
                wb.area() as f32 <= VIDEO_SHRINK_FRACTION * rb.area() as f32
                    && wb.area() as f32 >= min_area * total as f32
            });
            v.win_start_us = now;
            v.win_box = None;
            if let Some(wb) = shrink {
                self.set_video_region(wb, now);
            }
        }
    }

    fn tile_rect(&self, idx: usize) -> (u32, u32, u32, u32) {
        let tx = idx as u32 % self.tiles_x;
        let ty = idx as u32 / self.tiles_x;
        let x = tx * TILE;
        let y = ty * TILE;
        (x, y, TILE.min(self.width - x), TILE.min(self.height - y))
    }

    fn hot(&self, idx: usize, now: u64) -> bool {
        let c = self.changes[idx][2];
        c != 0 && now.saturating_sub(c) < HOT_WINDOW_US
    }

    fn settled(&self, idx: usize, now: u64) -> bool {
        now.saturating_sub(self.changes[idx][0]) >= SETTLE_US
    }

    fn copy_tile(&self, idx: usize) -> (u32, u32, u32, u32, Vec<u8>) {
        let (x, y, w, h) = self.tile_rect(idx);
        let stride = self.width as usize * 4;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let off = row as usize * stride + x as usize * 4;
            out.extend_from_slice(&self.fb[off..off + w as usize * 4]);
        }
        (x, y, w, h, out)
    }
}

/// Как кодировать тайлы для подписчика.
#[derive(Clone, Copy, Debug, Default)]
pub struct EncodePolicy {
    /// Клиент принимает RECT_JPEG.
    pub lossy_ok: bool,
    /// Канал не успевает: все «пёстрые» тайлы — JPEG, не только «горячие».
    pub congested: bool,
    /// Видеообласть, которую подписчик получает видеокодеком: её тайлы в
    /// кадр не попадают.
    pub skip: Option<VideoRegion>,
    /// Если в кадр попадает не меньше такой доли тайлов экрана (смена сцены,
    /// ключевой кадр), «пёстрые» тайлы идут JPEG, даже если меняются редко:
    /// без потерь такой кадр — мегабайты, и канал с бюджетом встал бы на
    /// секунды. Текст и интерфейс это не затрагивает. 0 — выключено.
    pub bulk_fraction: f32,
}

/// Результат [`GfxScreen::collect`].
pub struct Collected {
    pub update: spb::FrameUpdate,
    /// Тайлы кадра: (индекс, версия, с потерями).
    pub tiles: Vec<(usize, u64, bool)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Job {
    Lossless,
    Jpeg,
    /// Решить по числу цветов: JPEG, если цветов больше порога.
    Decide(usize),
}

struct TileJob {
    idx: usize,
    ver: u64,
    rect: (u32, u32, u32, u32),
    px: Vec<u8>,
    job: Job,
}

enum Part {
    Ready(Arc<spb::Rect>, bool),
    Job(usize),
}

/// Состояние подписчика: что он уже получил.
#[derive(Clone, Copy, Debug, Default)]
pub struct SubCursor {
    pub seq: u64,
    pub epoch: u64,
    pub started: bool,
}

pub struct GfxScreen {
    inner: Mutex<Inner>,
    notify: watch::Sender<u64>,
}

impl Default for GfxScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl GfxScreen {
    pub fn new() -> Self {
        let (notify, _) = watch::channel(0);
        GfxScreen {
            inner: Mutex::new(Inner {
                width: 0,
                height: 0,
                fb: Vec::new(),
                tiles_x: 0,
                tiles_y: 0,
                tile_ver: Vec::new(),
                changes: Vec::new(),
                pending_tiles: Vec::new(),
                cache: Vec::new(),
                jpeg: Vec::new(),
                colors: Vec::new(),
                seq: 0,
                epoch: 0,
                pending: false,
                cursor: Cursor::default(),
                seq_time_us: 0,
                video: VideoState::default(),
            }),
            notify,
        }
    }

    /// Включает или выключает распознавание видео.
    pub fn set_video_detect(&self, cfg: VideoDetect) {
        let mut g = self.inner.lock();
        g.video.cfg = cfg;
        if !cfg.enabled {
            g.video.region = None;
        }
    }

    /// Текущая видеообласть. Заодно снимает её, если движение стихло.
    pub fn video_region(&self) -> Option<VideoRegion> {
        let mut g = self.inner.lock();
        let (r, _) = g.video.region?;
        if crate::unix_us().saturating_sub(g.video.active_us) > VIDEO_IDLE_US {
            g.video.region = None;
            g.video.big.clear();
            return None;
        }
        Some(r)
    }

    /// Копирует пиксели видеообласти `id` в `out` (RGBA, `w * h * 4`), если
    /// она менялась после изменения `last_seq`.
    pub fn capture_region(&self, id: u64, last_seq: u64, out: &mut Vec<u8>) -> Capture {
        let g = self.inner.lock();
        let Some((r, _)) = g.video.region.filter(|(r, _)| r.id == id) else { return Capture::Gone };
        if g.video.dirty_seq <= last_seq {
            return Capture::Unchanged;
        }
        let stride = g.width as usize * 4;
        let row_len = r.w as usize * 4;
        out.clear();
        out.reserve(row_len * r.h as usize);
        for row in r.y..r.y + r.h {
            let off = row as usize * stride + r.x as usize * 4;
            out.extend_from_slice(&g.fb[off..off + row_len]);
        }
        Capture::Frame { seq: g.seq, capture_unix_us: g.seq_time_us }
    }

    /// Тайлы области и их текущие версии: (индекс, версия).
    pub fn region_tiles(&self, r: &VideoRegion) -> Vec<(usize, u64)> {
        let g = self.inner.lock();
        let b = TileBox::of_region(r);
        let mut out = Vec::with_capacity(b.area() as usize);
        for ty in b.y0..b.y1.min(g.tiles_y) {
            for tx in b.x0..b.x1.min(g.tiles_x) {
                let i = (ty * g.tiles_x + tx) as usize;
                out.push((i, g.tile_ver[i]));
            }
        }
        out
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notify.subscribe()
    }

    pub fn size(&self) -> (u32, u32) {
        let g = self.inner.lock();
        (g.width, g.height)
    }

    /// Новый размер (DesktopSize) или сброс при запуске машины: чёрный кадр.
    pub fn resize(&self, w: u32, h: u32) {
        let seq = {
            let mut g = self.inner.lock();
            g.alloc(w, h);
            g.seq
        };
        self.notify.send_replace(seq);
    }

    /// Пишет прямоугольник пикселей формата R,G,B,X (32 бит) в буфер.
    pub fn apply_rgbx(&self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        let mut g = self.inner.lock();
        if x >= g.width || y >= g.height || w == 0 || h == 0 {
            return;
        }
        let cw = w.min(g.width - x);
        let ch = h.min(g.height - y);
        let stride = g.width as usize * 4;
        let src_stride = w as usize * 4;
        let next = g.seq + 1;
        let tiles_x = g.tiles_x;
        let mut changed_tiles: Vec<usize> = Vec::new();
        let mut row_buf = vec![0u8; cw as usize * 4];
        for row in 0..ch {
            let src = &data[row as usize * src_stride..row as usize * src_stride + cw as usize * 4];
            for (d, s) in row_buf.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
                d[0] = s[0];
                d[1] = s[1];
                d[2] = s[2];
                d[3] = 255;
            }
            let off = (y + row) as usize * stride + x as usize * 4;
            let dst = &mut g.fb[off..off + cw as usize * 4];
            if dst == row_buf.as_slice() {
                continue;
            }
            // какие тайлы в этой строке изменились
            let ty = (y + row) / TILE;
            let mut tx = x / TILE;
            while tx * TILE < x + cw {
                let sx = (tx * TILE).max(x);
                let ex = ((tx + 1) * TILE).min(x + cw);
                let a = ((sx - x) * 4) as usize;
                let b = ((ex - x) * 4) as usize;
                if dst[a..b] != row_buf[a..b] {
                    changed_tiles.push((ty * tiles_x + tx) as usize);
                }
                tx += 1;
            }
            dst.copy_from_slice(&row_buf);
        }
        if !changed_tiles.is_empty() {
            for t in changed_tiles {
                if g.tile_ver[t] != next {
                    g.tile_ver[t] = next;
                    g.pending_tiles.push(t);
                }
            }
            g.pending = true;
        }
    }

    /// Конец FramebufferUpdate: изменения становятся видны подписчикам.
    pub fn commit(&self) {
        let seq = {
            let mut g = self.inner.lock();
            if !g.pending {
                return;
            }
            g.pending = false;
            g.seq += 1;
            let now = crate::unix_us();
            g.seq_time_us = now;
            let tiles = std::mem::take(&mut g.pending_tiles);
            for &t in &tiles {
                let c = &mut g.changes[t];
                *c = [now, c[0], c[1]];
            }
            g.detect_video(&tiles, now);
            g.seq
        };
        self.notify.send_replace(seq);
    }

    pub fn set_cursor(&self, c: Cursor) {
        self.inner.lock().cursor = c;
    }

    pub fn cursor(&self) -> Cursor {
        self.inner.lock().cursor.clone()
    }

    /// Снимок всего кадра RGBA (для тестов и отладки).
    pub fn snapshot(&self) -> (u32, u32, Vec<u8>) {
        let g = self.inner.lock();
        (g.width, g.height, g.fb.clone())
    }

    /// Собирает FrameUpdate для подписчика: тайлы, изменённые после `cur.seq`,
    /// или весь кадр, если `keyframe` или сменился размер. Без изменений —
    /// только уточнения: тайлы из `refine` (отправленные раньше с потерями,
    /// с тех пор не менявшиеся и успокоившиеся) без потерь, примерно до
    /// `refine_budget` байт. `None` — нечего слать.
    pub fn collect(
        &self,
        cur: &mut SubCursor,
        keyframe: bool,
        policy: EncodePolicy,
        refine: &[(usize, u64)],
        refine_budget: usize,
    ) -> Option<Collected> {
        let now = crate::unix_us();
        let mut parts: Vec<(usize, u64, Part, bool)> = Vec::new(); // idx, ver, part, refine?
        let mut jobs: Vec<TileJob> = Vec::new();
        let (seq, epoch, width, height, key, capture_unix_us, delta);
        {
            let g = self.inner.lock();
            key = keyframe || !cur.started || cur.epoch != g.epoch;
            delta = key || cur.seq < g.seq;
            seq = g.seq;
            capture_unix_us = g.seq_time_us;
            epoch = g.epoch;
            width = g.width;
            height = g.height;
            let mut taken = vec![false; g.tile_ver.len()];
            // тайлы видеообласти подписчик получает видеокодеком
            if let Some(b) = policy.skip.as_ref().map(TileBox::of_region) {
                for ty in b.y0..b.y1.min(g.tiles_y) {
                    for tx in b.x0..b.x1.min(g.tiles_x) {
                        taken[(ty * g.tiles_x + tx) as usize] = true;
                    }
                }
            }
            // lossy_above: JPEG, если цветов больше; None — только без потерь
            let push = |g: &Inner,
                        parts: &mut Vec<_>,
                        jobs: &mut Vec<TileJob>,
                        i: usize,
                        lossy_above: Option<usize>,
                        refine: bool| {
                let ver = g.tile_ver[i];
                let colors = g.colors[i].filter(|(v, _)| *v == ver).map(|(_, c)| c as usize);
                let job = match (lossy_above, colors) {
                    (Some(t), Some(c)) if c > t => Job::Jpeg,
                    (Some(t), None) => Job::Decide(t),
                    _ => Job::Lossless,
                };
                let cached = match job {
                    Job::Jpeg => g.jpeg[i].as_ref().filter(|(v, _)| *v == ver).map(|(_, r)| (r.clone(), true)),
                    Job::Lossless => g.cache[i].as_ref().filter(|(v, _)| *v == ver).map(|(_, r)| (r.clone(), false)),
                    Job::Decide(_) => None,
                };
                match cached {
                    Some((r, lossy)) => parts.push((i, ver, Part::Ready(r, lossy), refine)),
                    None => {
                        let (x, y, w, h, px) = g.copy_tile(i);
                        parts.push((i, ver, Part::Job(jobs.len()), refine));
                        jobs.push(TileJob { idx: i, ver, rect: (x, y, w, h), px, job });
                    }
                }
            };
            if delta {
                let wanted = |taken: &[bool], i: usize| {
                    let ver = g.tile_ver[i];
                    !taken[i] && (key || (ver > cur.seq && ver <= g.seq))
                };
                let bulk = policy.bulk_fraction > 0.0 && {
                    let n = (0..g.tile_ver.len()).filter(|i| wanted(&taken, *i)).count();
                    n > 1 && n as f32 >= policy.bulk_fraction * g.tile_ver.len() as f32
                };
                for i in 0..g.tile_ver.len() {
                    if !wanted(&taken, i) {
                        continue;
                    }
                    taken[i] = true;
                    let hot = g.hot(i, now);
                    let lossy_above = match (policy.lossy_ok, hot, policy.congested) {
                        (false, _, _) => None,
                        (true, true, true) => Some(HOT_COLORS),
                        (true, true, false) | (true, false, true) => Some(BUSY_COLORS),
                        (true, false, false) => bulk.then_some(BUSY_COLORS),
                    };
                    push(&g, &mut parts, &mut jobs, i, lossy_above, false);
                }
            }
            if !key && cur.epoch == g.epoch {
                for &(i, ver) in refine {
                    if i < g.tile_ver.len() && !taken[i] && g.tile_ver[i] == ver && ver <= g.seq && g.settled(i, now) {
                        taken[i] = true;
                        push(&g, &mut parts, &mut jobs, i, None, true);
                    }
                }
            }
        }
        if parts.is_empty() {
            if !delta {
                return None;
            }
            // изменились только незафиксированные тайлы — или экран пустой (0x0)
            cur.seq = seq;
            cur.epoch = epoch;
            cur.started = true;
            if !key {
                return None;
            }
            let update = spb::FrameUpdate {
                seq,
                width,
                height,
                keyframe: true,
                rects: vec![],
                capture_unix_us,
                ..Default::default()
            };
            return Some(Collected { update, tiles: vec![] });
        }
        let encoded: Vec<(Arc<spb::Rect>, bool, Option<u16>)> = jobs
            .iter()
            .map(|j| {
                let (x, y, w, h) = j.rect;
                match j.job {
                    Job::Lossless => (Arc::new(encode_tile(x, y, w, h, &j.px)), false, None),
                    Job::Jpeg => (Arc::new(encode_jpeg(x, y, w, h, &j.px)), true, None),
                    Job::Decide(t) => {
                        let colors = count_colors(&j.px, BUSY_COLORS + 1);
                        if colors > t {
                            (Arc::new(encode_jpeg(x, y, w, h, &j.px)), true, Some(colors as u16))
                        } else {
                            (Arc::new(encode_tile(x, y, w, h, &j.px)), false, Some(colors as u16))
                        }
                    }
                }
            })
            .collect();
        if !jobs.is_empty() {
            let mut g = self.inner.lock();
            if g.epoch == epoch {
                for (j, (r, lossy, colors)) in jobs.iter().zip(&encoded) {
                    // незафиксированные тайлы (ver > seq) ещё могут меняться — не кешируем
                    if j.ver > seq || g.tile_ver.get(j.idx) != Some(&j.ver) {
                        continue;
                    }
                    if let Some(c) = colors {
                        g.colors[j.idx] = Some((j.ver, *c));
                    }
                    if *lossy {
                        g.jpeg[j.idx] = Some((j.ver, r.clone()));
                    } else {
                        g.cache[j.idx] = Some((j.ver, r.clone()));
                    }
                }
            }
        }
        let mut rects = Vec::with_capacity(parts.len());
        let mut tiles = Vec::with_capacity(parts.len());
        let mut refined = 0usize;
        for (i, ver, part, is_refine) in parts {
            let (r, lossy) = match part {
                Part::Ready(r, l) => (r, l),
                Part::Job(k) => (encoded[k].0.clone(), encoded[k].1),
            };
            if is_refine {
                if refined > 0 && refined + r.data.len() > refine_budget {
                    continue;
                }
                refined += r.data.len();
            }
            rects.push((*r).clone());
            tiles.push((i, ver, lossy));
        }
        let (fseq, capture) = if delta { (seq, capture_unix_us) } else { (cur.seq, 0) };
        if delta {
            cur.seq = seq;
            cur.epoch = epoch;
            cur.started = true;
        }
        Some(Collected {
            update: spb::FrameUpdate {
                seq: fseq,
                width,
                height,
                keyframe: key,
                rects,
                capture_unix_us: capture,
                ..Default::default()
            },
            tiles,
        })
    }

    /// Есть ли для подписчика зафиксированные изменения (или нужен ключевой кадр).
    pub fn has_changes(&self, cur: &SubCursor) -> bool {
        let g = self.inner.lock();
        !cur.started || cur.epoch != g.epoch || cur.seq < g.seq
    }

    /// Без потерь, как раньше (для тестов и клиентов без JPEG).
    pub fn collect_lossless(&self, cur: &mut SubCursor, keyframe: bool) -> Option<spb::FrameUpdate> {
        self.collect(cur, keyframe, EncodePolicy::default(), &[], 0).map(|c| c.update)
    }
}

/// Число разных цветов в тайле, но не больше `limit`.
pub fn count_colors(rgba: &[u8], limit: usize) -> usize {
    // открытая адресация на 256 ячеек; 0 — пусто (цвет хранится с битом 1<<24)
    let mut table = [0u32; 256];
    let mut n = 0;
    for p in rgba.as_chunks::<4>().0 {
        let v = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16) | (1 << 24);
        let mut h = (v.wrapping_mul(0x9E37_79B1) >> 24) as usize;
        loop {
            if table[h] == v {
                break;
            }
            if table[h] == 0 {
                table[h] = v;
                n += 1;
                if n >= limit || n >= 255 {
                    return n;
                }
                break;
            }
            h = (h + 1) & 255;
        }
    }
    n
}

/// JPEG (RGB, качество [`JPEG_QUALITY`]); при ошибке — без потерь.
pub fn encode_jpeg(x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) -> spb::Rect {
    let mut out = Vec::with_capacity(4096);
    let enc = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
    match enc.encode(rgba, w as u16, h as u16, jpeg_encoder::ColorType::Rgba) {
        Ok(()) => spb::Rect {
            x,
            y,
            w,
            h,
            encoding: spb::RectEncoding::RectJpeg as i32,
            data: out,
            src_x: 0,
            src_y: 0,
            ..Default::default()
        },
        Err(_) => encode_tile(x, y, w, h, rgba),
    }
}

/// Сжимает тайл: сплошной цвет -> RECT_SOLID, иначе RGBA + zstd.
pub fn encode_tile(x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) -> spb::Rect {
    let first: [u8; 4] = rgba[0..4].try_into().expect("tile has at least one pixel");
    let solid = rgba.as_chunks::<4>().0.iter().all(|p| *p == first);
    if solid {
        return spb::Rect {
            x,
            y,
            w,
            h,
            encoding: spb::RectEncoding::RectSolid as i32,
            data: first.to_vec(),
            src_x: 0,
            src_y: 0,
            ..Default::default()
        };
    }
    let data = zstd::bulk::compress(rgba, ZSTD_LEVEL).unwrap_or_default();
    spb::Rect {
        x,
        y,
        w,
        h,
        encoding: spb::RectEncoding::RectRgbaZstd as i32,
        data,
        src_x: 0,
        src_y: 0,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_then_delta() {
        let s = GfxScreen::new();
        s.resize(130, 70); // 3x2 тайла, крайние неполные
        let mut cur = SubCursor::default();
        let k = s.collect_lossless(&mut cur, false).unwrap();
        assert!(k.keyframe);
        assert_eq!(k.rects.len(), 6);
        assert!(k.rects.iter().all(|r| r.encoding == spb::RectEncoding::RectSolid as i32));
        assert_eq!(k.rects[2].w, 2);
        assert_eq!(k.rects[5].h, 6);
        assert!(s.collect_lossless(&mut cur, false).is_none());

        // красная точка в тайле (1,1)
        s.apply_rgbx(70, 65, 1, 1, &[255, 0, 0, 0]);
        assert!(s.collect_lossless(&mut cur, false).is_none(), "not visible before commit");
        s.commit();
        let d = s.collect_lossless(&mut cur, false).unwrap();
        assert!(!d.keyframe);
        assert_eq!(d.rects.len(), 1);
        let r = &d.rects[0];
        assert_eq!((r.x, r.y, r.w, r.h), (64, 64, 64, 6));
        assert_eq!(r.encoding, spb::RectEncoding::RectRgbaZstd as i32);
        let px = zstd::bulk::decompress(&r.data, 64 * 6 * 4).unwrap();
        let off = (64 + 6) * 4;
        assert_eq!(&px[off..off + 4], &[255, 0, 0, 255]);

        // те же пиксели повторно — изменений нет
        s.apply_rgbx(70, 65, 1, 1, &[255, 0, 0, 7]);
        s.commit();
        assert!(s.collect_lossless(&mut cur, false).is_none());

        // ключевой кадр по запросу берёт закешированные тайлы
        let k2 = s.collect_lossless(&mut cur, true).unwrap();
        assert!(k2.keyframe);
        assert_eq!(k2.rects.len(), 6);
    }

    fn plasma(t: f32) -> Vec<u8> {
        let mut v = Vec::with_capacity(64 * 64 * 4);
        for y in 0..64 {
            for x in 0..64 {
                let (fx, fy) = (x as f32, y as f32);
                v.extend_from_slice(&[
                    (128.0 + 120.0 * (fx / 7.0 + t).sin()) as u8,
                    (128.0 + 120.0 * (fy / 5.0 + t).sin()) as u8,
                    (128.0 + 120.0 * ((fx + fy) / 9.0 + t).sin()) as u8,
                    0,
                ]);
            }
        }
        v
    }

    fn stripes(t: u8) -> Vec<u8> {
        (0..64 * 64).flat_map(|i| if (i / 4) % 2 == 0 { [t, 0, 0, 0] } else { [255, 255, 255, 0] }).collect()
    }

    #[test]
    fn colors() {
        assert_eq!(count_colors(&[1, 2, 3, 255].repeat(100), 100), 1);
        assert_eq!(count_colors(&stripes(9), 100), 2);
        assert!(count_colors(&plasma(0.0), BUSY_COLORS + 1) > BUSY_COLORS);
        assert_eq!(count_colors(&plasma(0.0), 10), 10, "stops at the limit");
    }

    #[test]
    fn hot_busy_tiles_go_lossy_then_refined() {
        let s = GfxScreen::new();
        s.resize(128, 64);
        let mut cur = SubCursor::default();
        s.collect_lossless(&mut cur, false).unwrap();
        let policy = EncodePolicy { lossy_ok: true, ..Default::default() };
        // оба тайла меняются трижды подряд: «горячие»
        for t in 0..3 {
            s.apply_rgbx(0, 0, 64, 64, &plasma(t as f32));
            s.apply_rgbx(64, 0, 64, 64, &stripes(t));
            s.commit();
        }
        let c = s.collect(&mut cur, false, policy, &[], 0).unwrap();
        assert!(c.update.capture_unix_us > 0);
        assert_eq!(c.tiles.len(), 2);
        let (t0, t1) = (c.tiles[0], c.tiles[1]);
        assert!(t0.2, "busy hot tile is lossy");
        assert_eq!(c.update.rects[0].encoding, spb::RectEncoding::RectJpeg as i32);
        assert!(!t1.2, "two-colour tile stays lossless");
        assert_eq!(c.update.rects[1].encoding, spb::RectEncoding::RectRgbaZstd as i32);
        // клиент без JPEG получает тот же тайл без потерь
        let mut other = SubCursor::default();
        let k = s.collect(&mut other, false, EncodePolicy::default(), &[], 0).unwrap();
        assert!(k.update.rects.iter().all(|r| r.encoding != spb::RectEncoding::RectJpeg as i32));

        // уточнение: пока тайл не успокоился — нечего слать
        let refine = [(t0.0, t0.1)];
        assert!(s.collect(&mut cur, false, policy, &refine, 1 << 20).is_none());
        std::thread::sleep(std::time::Duration::from_micros(SETTLE_US + 50_000));
        let r = s.collect(&mut cur, false, policy, &refine, 1 << 20).unwrap();
        assert!(!r.update.keyframe);
        assert_eq!(r.update.capture_unix_us, 0, "refinement is not a new capture");
        assert_eq!(r.tiles, vec![(t0.0, t0.1, false)]);
        assert_eq!(r.update.rects[0].encoding, spb::RectEncoding::RectRgbaZstd as i32);
        // тайл изменился после отправки с потерями — уточнять старую версию нельзя
        s.apply_rgbx(0, 0, 64, 64, &plasma(9.0));
        s.commit();
        assert!(s.collect(&mut cur, false, policy, &[], 0).is_some());
        assert!(s.collect(&mut cur, false, policy, &refine, 1 << 20).is_none());
    }

    #[test]
    fn congestion_makes_busy_tiles_lossy() {
        let s = GfxScreen::new();
        s.resize(128, 64);
        let mut cur = SubCursor::default();
        s.collect_lossless(&mut cur, false).unwrap();
        s.apply_rgbx(0, 0, 64, 64, &plasma(1.0));
        s.apply_rgbx(64, 0, 64, 64, &stripes(1));
        s.commit();
        // одно изменение (не «горячее»): без перегрузки — без потерь
        let mut c2 = cur;
        let calm = s.collect(&mut c2, false, EncodePolicy { lossy_ok: true, ..Default::default() }, &[], 0).unwrap();
        assert!(calm.tiles.iter().all(|t| !t.2));
        let busy = s
            .collect(&mut cur, false, EncodePolicy { lossy_ok: true, congested: true, ..Default::default() }, &[], 0)
            .unwrap();
        assert_eq!(busy.tiles.iter().map(|t| t.2).collect::<Vec<_>>(), vec![true, false]);
        // JPEG заметно меньше zstd для такой картинки
        assert!(busy.update.rects[0].data.len() < calm.update.rects[0].data.len());
    }

    #[test]
    fn resize_forces_keyframe() {
        let s = GfxScreen::new();
        s.resize(64, 64);
        let mut cur = SubCursor::default();
        s.collect_lossless(&mut cur, false).unwrap();
        s.resize(128, 64);
        let k = s.collect_lossless(&mut cur, false).unwrap();
        assert!(k.keyframe);
        assert_eq!((k.width, k.height, k.rects.len()), (128, 64, 2));
    }

    /// Меняет тайлы [tx0, tx1) x [ty0, ty1) и фиксирует изменение: в каждом
    /// пиксель (3, 3) цвета `v` и рядом «пёстрый» квадрат 10 x 10, как у видео.
    fn touch(s: &GfxScreen, tx0: u32, ty0: u32, tx1: u32, ty1: u32, v: u8) {
        let noise: Vec<u8> = (0..100u8).flat_map(|i| [i, v, i ^ v, 0]).collect();
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                s.apply_rgbx(tx * TILE + 3, ty * TILE + 3, 1, 1, &[v, v, v, 0]);
                s.apply_rgbx(tx * TILE + 20, ty * TILE + 20, 10, 10, &noise);
            }
        }
        s.commit();
    }

    #[test]
    fn text_flood_is_not_video() {
        let s = video_screen();
        // весь экран меняется кадр за кадром, но в тайлах по два цвета (текст)
        for v in 1..=20u8 {
            let px: Vec<u8> = (0..640 * 480)
                .flat_map(|i| if (i / 3 + v as usize).is_multiple_of(2) { [v; 4] } else { [0; 4] })
                .collect();
            s.apply_rgbx(0, 0, 640, 480, &px);
            s.commit();
        }
        assert_eq!(s.video_region(), None);
    }

    fn video_screen() -> GfxScreen {
        let s = GfxScreen::new();
        s.set_video_detect(VideoDetect { enabled: true, min_area: 0.2 });
        s.resize(640, 480); // 10 x 8 тайлов (нижний ряд неполный)
        s
    }

    #[test]
    fn video_region_appears_grows_and_ends() {
        let s = video_screen();
        // мелкие изменения — не видео
        for v in 0..10 {
            touch(&s, 0, 0, 2, 2, v);
        }
        assert_eq!(s.video_region(), None);
        // окно 4 x 4 тайла (20 % экрана) меняется шесть кадров подряд
        for v in 0..VIDEO_ON_FRAMES as u8 {
            assert_eq!(s.video_region(), None);
            touch(&s, 2, 1, 6, 5, 100 + v);
        }
        let r = s.video_region().expect("video region");
        assert_eq!((r.x, r.y, r.w, r.h), (128, 64, 256, 256));
        // кадр области: только после нового изменения
        let mut buf = Vec::new();
        let Capture::Frame { seq, .. } = s.capture_region(r.id, 0, &mut buf) else { panic!("no frame") };
        assert_eq!(buf.len(), 256 * 256 * 4);
        assert_eq!(&buf[(3 * 256 + 3) * 4..(3 * 256 + 3) * 4 + 3], &[105, 105, 105]);
        assert_eq!(s.capture_region(r.id, seq, &mut buf), Capture::Unchanged);
        touch(&s, 0, 0, 1, 1, 9); // вне области
        assert_eq!(s.capture_region(r.id, seq, &mut buf), Capture::Unchanged);
        touch(&s, 3, 3, 4, 4, 9);
        assert!(matches!(s.capture_region(r.id, seq, &mut buf), Capture::Frame { .. }));
        assert_eq!(s.capture_region(r.id + 1, 0, &mut buf), Capture::Gone);

        // подписчик с видео не получает тайлы области, остальные получает
        let mut cur = SubCursor::default();
        let skip = EncodePolicy { skip: Some(r), ..Default::default() };
        let k = s.collect(&mut cur, true, skip, &[], 0).unwrap();
        assert_eq!(k.tiles.len(), 80 - 16);
        touch(&s, 1, 1, 3, 3, 77); // два тайла вне области, два внутри
        let d = s.collect(&mut cur, false, skip, &[], 0).unwrap();
        assert_eq!(d.tiles.iter().map(|t| t.0).collect::<Vec<_>>(), vec![11, 21]);
        assert_eq!(s.region_tiles(&r).len(), 16);

        // видео развернули на весь экран: область растёт (новый id)
        for v in 0..VIDEO_GROW_FRAMES as u8 {
            touch(&s, 0, 0, 10, 8, 150 + v);
        }
        let full = s.video_region().expect("video region");
        assert_ne!(full.id, r.id);
        assert_eq!((full.x, full.y, full.w, full.h), (0, 0, 640, 480));
        assert_eq!(s.capture_region(r.id, 0, &mut buf), Capture::Gone);

        // движение стихло: мелочь режим не держит
        std::thread::sleep(std::time::Duration::from_micros(VIDEO_IDLE_US / 2));
        touch(&s, 0, 0, 1, 1, 1);
        assert!(s.video_region().is_some());
        std::thread::sleep(std::time::Duration::from_micros(VIDEO_IDLE_US / 2 + 50_000));
        touch(&s, 0, 0, 1, 1, 2);
        assert_eq!(s.video_region(), None);
        assert_eq!(s.capture_region(full.id, 0, &mut buf), Capture::Gone);
    }

    #[test]
    fn video_region_shrinks_and_resets_on_resize() {
        let s = video_screen();
        for v in 0..VIDEO_ON_FRAMES as u8 {
            touch(&s, 0, 0, 10, 8, v + 1);
        }
        let full = s.video_region().unwrap();
        assert_eq!((full.w, full.h), (640, 480));
        // плеер вышел из полного экрана: дальше меняется только окно 5 x 4;
        // по итогам окна наблюдения область сжимается до него
        for v in 0..3u8 {
            assert_eq!(s.video_region().map(|r| r.id), Some(full.id));
            std::thread::sleep(std::time::Duration::from_micros(VIDEO_SHRINK_WINDOW_US / 3 + 50_000));
            touch(&s, 3, 2, 8, 6, 100 + v);
        }
        let r = s.video_region().unwrap();
        assert_ne!(r.id, full.id);
        assert_eq!((r.x, r.y, r.w, r.h), (192, 128, 320, 256));
        // смена размера экрана снимает область
        s.resize(320, 240);
        assert_eq!(s.video_region(), None);
        // выключенное распознавание
        let off = GfxScreen::new();
        off.resize(640, 480);
        for v in 0..20 {
            touch(&off, 0, 0, 10, 8, v);
        }
        assert_eq!(off.video_region(), None);
    }
}
