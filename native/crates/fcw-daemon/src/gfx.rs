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

use crate::spb;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::watch;

pub const TILE: u32 = 64;
const ZSTD_LEVEL: i32 = 1;
/// Тайл «пёстрый», если в нём больше стольких разных цветов (текст и
/// интерфейс обычно укладываются, фото и видео — нет).
pub const BUSY_COLORS: usize = 64;
/// Тайл «горячий», если он менялся 3 раза за это время (видео, прокрутка).
pub const HOT_WINDOW_US: u64 = 1_000_000;
/// Тайл «успокоился», если не менялся столько времени: можно досылать без потерь.
pub const SETTLE_US: u64 = 400_000;
pub const JPEG_QUALITY: u8 = 70;

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
    /// «Пёстрый» ли тайл этой версии.
    busy: Vec<Option<(u64, bool)>>,
    /// Номер последнего зафиксированного изменения.
    seq: u64,
    /// Меняется при смене размера/сбросе: подписчикам нужен ключевой кадр.
    epoch: u64,
    /// Есть ли незафиксированные изменения тайлов (помечены как seq + 1).
    pending: bool,
    cursor: Cursor,
    /// Время фиксации изменения `seq` (Unix, мкс).
    seq_time_us: u64,
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
        self.busy = vec![None; n];
        self.pending = false;
        self.seq_time_us = crate::unix_us();
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
    /// Решить по числу цветов: JPEG для «пёстрого», иначе без потерь.
    Decide,
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
                busy: Vec::new(),
                seq: 0,
                epoch: 0,
                pending: false,
                cursor: Cursor::default(),
                seq_time_us: 0,
            }),
            notify,
        }
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
            for t in std::mem::take(&mut g.pending_tiles) {
                let c = &mut g.changes[t];
                *c = [now, c[0], c[1]];
            }
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
            let mut push = |g: &Inner, parts: &mut Vec<_>, jobs: &mut Vec<TileJob>, i: usize, want_lossy: bool, refine: bool| {
                let ver = g.tile_ver[i];
                let busy = g.busy[i].filter(|(v, _)| *v == ver).map(|(_, b)| b);
                let job = match (want_lossy, busy) {
                    (true, Some(true)) => Job::Jpeg,
                    (true, None) => Job::Decide,
                    _ => Job::Lossless,
                };
                let cached = match job {
                    Job::Jpeg => g.jpeg[i].as_ref().filter(|(v, _)| *v == ver).map(|(_, r)| (r.clone(), true)),
                    Job::Lossless => g.cache[i].as_ref().filter(|(v, _)| *v == ver).map(|(_, r)| (r.clone(), false)),
                    Job::Decide => None,
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
                for i in 0..g.tile_ver.len() {
                    let ver = g.tile_ver[i];
                    if !key && (ver <= cur.seq || ver > g.seq) {
                        continue;
                    }
                    taken[i] = true;
                    let want_lossy = policy.lossy_ok && (policy.congested || g.hot(i, now));
                    push(&g, &mut parts, &mut jobs, i, want_lossy, false);
                }
            }
            if !key && cur.epoch == g.epoch {
                for &(i, ver) in refine {
                    if i < g.tile_ver.len() && !taken[i] && g.tile_ver[i] == ver && ver <= g.seq && g.settled(i, now) {
                        taken[i] = true;
                        push(&g, &mut parts, &mut jobs, i, false, true);
                    }
                }
            }
        }
        if parts.is_empty() {
            if delta {
                // изменились только незафиксированные тайлы
                cur.seq = seq;
            }
            return None;
        }
        let encoded: Vec<(Arc<spb::Rect>, bool, Option<bool>)> = jobs
            .iter()
            .map(|j| {
                let (x, y, w, h) = j.rect;
                match j.job {
                    Job::Lossless => (Arc::new(encode_tile(x, y, w, h, &j.px)), false, None),
                    Job::Jpeg => (Arc::new(encode_jpeg(x, y, w, h, &j.px)), true, Some(true)),
                    Job::Decide => {
                        let busy = count_colors(&j.px, BUSY_COLORS + 1) > BUSY_COLORS;
                        if busy {
                            (Arc::new(encode_jpeg(x, y, w, h, &j.px)), true, Some(true))
                        } else {
                            (Arc::new(encode_tile(x, y, w, h, &j.px)), false, Some(false))
                        }
                    }
                }
            })
            .collect();
        if !jobs.is_empty() {
            let mut g = self.inner.lock();
            if g.epoch == epoch {
                for (j, (r, lossy, busy)) in jobs.iter().zip(&encoded) {
                    // незафиксированные тайлы (ver > seq) ещё могут меняться — не кешируем
                    if j.ver > seq || g.tile_ver.get(j.idx) != Some(&j.ver) {
                        continue;
                    }
                    if let Some(b) = busy {
                        g.busy[j.idx] = Some((j.ver, *b));
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
            update: spb::FrameUpdate { seq: fseq, width, height, keyframe: key, rects, capture_unix_us: capture },
            tiles,
        })
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
        Ok(()) => spb::Rect { x, y, w, h, encoding: spb::RectEncoding::RectJpeg as i32, data: out, src_x: 0, src_y: 0 },
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
        };
    }
    let data = zstd::bulk::compress(rgba, ZSTD_LEVEL).unwrap_or_default();
    spb::Rect { x, y, w, h, encoding: spb::RectEncoding::RectRgbaZstd as i32, data, src_x: 0, src_y: 0 }
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
}
