//! Вид экрана на стороне клиента: буфер RGBA, накопление грязных областей,
//! декодирование FrameUpdate, отрисовка TextUpdate шрифтом и курсор гостя.

use crate::font::{self, CELL_H, CELL_W};
use fcw_proto::stream as spb;
use parking_lot::Mutex;

pub const KIND_GRAPHICAL: i32 = 1;
pub const KIND_TEXT: i32 = 2;

/// Размер ячейки сетки грязных областей.
const TILE: u32 = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectI {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl RectI {
    fn clip(self, w: i32, h: i32) -> Option<RectI> {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = (self.x + self.w).min(w);
        let y1 = (self.y + self.h).min(h);
        (x1 > x0 && y1 > y0).then_some(RectI { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }
}

#[derive(Clone, Debug, Default)]
pub struct CursorImage {
    pub w: u32,
    pub h: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    ch: u32,
    fg: u32,
    bg: u32,
    flags: u32,
}

const BLANK: Cell = Cell { ch: 0, fg: 0xAAAAAA, bg: 0, flags: 0 };

#[derive(Default)]
struct State {
    kind: i32,
    width: u32,
    height: u32,
    base: Vec<u8>,
    seq: u64,
    updates: u64,
    // грязные тайлы
    tiles_x: u32,
    tiles_y: u32,
    dirty: Vec<bool>,
    any_dirty: bool,
    full: bool,
    // текст
    cols: u32,
    rows: u32,
    cells: Vec<Cell>,
    text_cursor: (u32, u32, bool),
    // курсор графического экрана
    cursor: Option<CursorImage>,
    cursor_pos: (i32, i32),
    cursor_drawn: Option<RectI>,
}

impl State {
    fn alloc(&mut self, w: u32, h: u32) {
        self.width = w;
        self.height = h;
        self.base = vec![0; (w * h * 4) as usize];
        for px in self.base.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        self.tiles_x = w.div_ceil(TILE);
        self.tiles_y = h.div_ceil(TILE);
        self.dirty = vec![false; (self.tiles_x * self.tiles_y) as usize];
        self.any_dirty = true;
        self.full = true;
        self.cursor_drawn = None;
    }

    fn mark(&mut self, r: RectI) {
        let Some(r) = r.clip(self.width as i32, self.height as i32) else { return };
        let tx0 = r.x as u32 / TILE;
        let ty0 = r.y as u32 / TILE;
        let tx1 = (r.x + r.w - 1) as u32 / TILE;
        let ty1 = (r.y + r.h - 1) as u32 / TILE;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                self.dirty[(ty * self.tiles_x + tx) as usize] = true;
            }
        }
        self.any_dirty = true;
    }

    fn cursor_rect(&self) -> Option<RectI> {
        if self.kind != KIND_GRAPHICAL {
            return None;
        }
        let c = self.cursor.as_ref()?;
        if c.w == 0 || c.rgba.is_empty() {
            return None;
        }
        RectI {
            x: self.cursor_pos.0 - c.hot_x as i32,
            y: self.cursor_pos.1 - c.hot_y as i32,
            w: c.w as i32,
            h: c.h as i32,
        }
        .clip(self.width as i32, self.height as i32)
    }

    fn rect_is_dirty(&self, r: RectI) -> bool {
        if self.full {
            return true;
        }
        let tx0 = r.x as u32 / TILE;
        let ty0 = r.y as u32 / TILE;
        let tx1 = (r.x + r.w - 1) as u32 / TILE;
        let ty1 = (r.y + r.h - 1) as u32 / TILE;
        (ty0..=ty1).any(|ty| (tx0..=tx1).any(|tx| self.dirty[(ty * self.tiles_x + tx) as usize]))
    }

    /// Грязные тайлы -> прямоугольники (горизонтальные отрезки, склеенные по вертикали).
    fn dirty_rects(&self) -> Vec<RectI> {
        let mut out: Vec<RectI> = Vec::new();
        let mut prev_row: Vec<RectI> = Vec::new();
        for ty in 0..self.tiles_y {
            let mut row: Vec<RectI> = Vec::new();
            let mut tx = 0;
            while tx < self.tiles_x {
                if !self.dirty[(ty * self.tiles_x + tx) as usize] {
                    tx += 1;
                    continue;
                }
                let start = tx;
                while tx < self.tiles_x && self.dirty[(ty * self.tiles_x + tx) as usize] {
                    tx += 1;
                }
                let r = RectI {
                    x: (start * TILE) as i32,
                    y: (ty * TILE) as i32,
                    w: ((tx - start) * TILE) as i32,
                    h: TILE as i32,
                };
                row.push(r.clip(self.width as i32, self.height as i32).unwrap());
            }
            // продлить вниз отрезки предыдущей строки с теми же x/w
            let mut next_prev = Vec::new();
            for r in row {
                if let Some(p) = prev_row.iter().position(|p| p.x == r.x && p.w == r.w) {
                    let mut p = prev_row.remove(p);
                    p.h = r.y + r.h - p.y;
                    next_prev.push(p);
                } else {
                    next_prev.push(r);
                }
            }
            out.append(&mut prev_row);
            prev_row = next_prev;
        }
        out.append(&mut prev_row);
        out
    }

    fn clear_dirty(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = false);
        self.any_dirty = false;
        self.full = false;
    }

    fn copy_rect(&self, dst: &mut [u8], r: RectI) {
        let stride = self.width as usize * 4;
        for y in r.y..r.y + r.h {
            let off = y as usize * stride + r.x as usize * 4;
            let len = r.w as usize * 4;
            dst[off..off + len].copy_from_slice(&self.base[off..off + len]);
        }
    }

    fn blend_cursor(&self, dst: &mut [u8], r: RectI) {
        let Some(c) = &self.cursor else { return };
        let stride = self.width as usize * 4;
        let ox = self.cursor_pos.0 - c.hot_x as i32;
        let oy = self.cursor_pos.1 - c.hot_y as i32;
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                let cx = (x - ox) as usize;
                let cy = (y - oy) as usize;
                let ci = (cy * c.w as usize + cx) * 4;
                let Some(sp) = c.rgba.get(ci..ci + 4) else { continue };
                let a = sp[3] as u32;
                if a == 0 {
                    continue;
                }
                let di = y as usize * stride + x as usize * 4;
                let dp = &mut dst[di..di + 4];
                for k in 0..3 {
                    dp[k] = ((sp[k] as u32 * a + dp[k] as u32 * (255 - a)) / 255) as u8;
                }
                dp[3] = 255;
            }
        }
    }

    fn render_text_row(&mut self, row: u32) {
        let stride = self.width as usize * 4;
        let (cx, cy, cv) = self.text_cursor;
        for col in 0..self.cols {
            let cell = self.cells[(row * self.cols + col) as usize];
            let mut flags = cell.flags;
            if cv && cx == col && cy == row {
                flags ^= font::FLAG_INVERSE;
            }
            font::draw_cell(
                &mut self.base,
                stride,
                col as usize * CELL_W,
                row as usize * CELL_H,
                cell.ch,
                cell.fg,
                cell.bg,
                flags,
            );
        }
        self.mark(RectI { x: 0, y: (row as usize * CELL_H) as i32, w: self.width as i32, h: CELL_H as i32 });
    }
}

/// Распакованный прямоугольник кадра, готовый к записи в буфер.
pub enum Decoded {
    Pixels(spb::Rect, Vec<u8>),
    Solid(spb::Rect, [u8; 4]),
    Copy(spb::Rect),
}

/// Декодер видеорежима (RECT_H264). Состояние живёт, пока жив поток экрана:
/// кадры одного видеопотока зависят друг от друга и идут строго по порядку.
#[derive(Default)]
pub struct VideoDecoder {
    #[cfg(feature = "video")]
    dec: Option<openh264::decoder::Decoder>,
    /// Номер видеопотока, который сейчас декодируется.
    #[cfg(feature = "video")]
    stream: u32,
    /// Декодер получил ключевой кадр и не ошибался.
    #[cfg(feature = "video")]
    synced: bool,
    /// Кадр пришлось отбросить: нужен ключевой.
    need_key: bool,
}

impl VideoDecoder {
    /// Клиент собран с декодером H.264.
    pub const SUPPORTED: bool = cfg!(feature = "video");

    /// Нужно ли просить ключевой кадр (сбрасывает признак).
    pub fn take_need_key(&mut self) -> bool {
        std::mem::take(&mut self.need_key)
    }

    /// Декодирует кадр; `pixels = false` — только обновить состояние декодера
    /// (за этим кадром в том же обновлении идёт следующий). Возвращает RGBA
    /// размера `r.w * r.h`.
    #[cfg(feature = "video")]
    fn decode(&mut self, r: &spb::Rect, pixels: bool) -> Option<Vec<u8>> {
        if self.dec.is_none() || r.video_stream != self.stream {
            if !r.video_key {
                self.need_key = true;
                return None;
            }
            self.dec = openh264::decoder::Decoder::new().ok();
            self.stream = r.video_stream;
            self.synced = false;
        }
        if !self.synced && !r.video_key {
            self.need_key = true;
            return None;
        }
        let yuv = match self.dec.as_mut()?.decode(&r.data) {
            Ok(Some(yuv)) => yuv,
            Ok(None) => return None,
            Err(_) => {
                self.synced = false;
                self.need_key = true;
                return None;
            }
        };
        self.synced = true;
        if !pixels {
            return None;
        }
        use openh264::formats::YUVSource;
        let (dw, dh) = yuv.dimensions();
        let (w, h) = (r.w as usize, r.h as usize);
        if dw < w || dh < h || w == 0 || h == 0 {
            return None;
        }
        let mut full = vec![0u8; dw * dh * 4];
        yuv.write_rgba8(&mut full);
        if (dw, dh) == (w, h) {
            return Some(full);
        }
        let mut out = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            out.extend_from_slice(&full[row * dw * 4..(row * dw + w) * 4]);
        }
        Some(out)
    }

    #[cfg(not(feature = "video"))]
    fn decode(&mut self, _r: &spb::Rect, _pixels: bool) -> Option<Vec<u8>> {
        None
    }
}

/// Распаковка прямоугольников (вне блокировки вида).
pub fn decode_rects(rects: Vec<spb::Rect>, video: &mut VideoDecoder) -> Vec<Decoded> {
    let mut out = Vec::with_capacity(rects.len());
    let h264 = spb::RectEncoding::RectH264 as i32;
    // из нескольких кадров видео в одном обновлении на экран попадёт последний
    let last_video = rects.iter().rposition(|r| r.encoding == h264);
    for (i, r) in rects.into_iter().enumerate() {
        match spb::RectEncoding::try_from(r.encoding).unwrap_or(spb::RectEncoding::RectUnknown) {
            spb::RectEncoding::RectH264 => {
                if let Some(px) = video.decode(&r, Some(i) == last_video) {
                    out.push(Decoded::Pixels(r, px));
                }
            }
            spb::RectEncoding::RectRgbaZstd => {
                let len = (r.w * r.h * 4) as usize;
                if let Ok(px) = zstd::bulk::decompress(&r.data, len) {
                    if px.len() == len {
                        out.push(Decoded::Pixels(r, px));
                    }
                }
            }
            spb::RectEncoding::RectSolid => {
                if r.data.len() >= 4 {
                    let c = [r.data[0], r.data[1], r.data[2], r.data[3]];
                    out.push(Decoded::Solid(r, c));
                }
            }
            spb::RectEncoding::RectCopy => out.push(Decoded::Copy(r)),
            spb::RectEncoding::RectJpeg => {
                if let Some(px) = decode_jpeg(&r.data, r.w, r.h) {
                    out.push(Decoded::Pixels(r, px));
                }
            }
            _ => {}
        }
    }
    out
}

/// JPEG (RGB или серый) -> RGBA размера w*h.
fn decode_jpeg(data: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut d = jpeg_decoder::Decoder::new(data);
    let px = d.decode().ok()?;
    let info = d.info()?;
    if info.width as u32 != w || info.height as u32 != h {
        return None;
    }
    let n = (w * h) as usize;
    let mut out = Vec::with_capacity(n * 4);
    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 if px.len() == n * 3 => {
            for p in px.as_chunks::<3>().0 {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        jpeg_decoder::PixelFormat::L8 if px.len() == n => {
            for &v in &px {
                out.extend_from_slice(&[v, v, v, 255]);
            }
        }
        _ => return None,
    }
    Some(out)
}

pub struct View {
    pub vm_id: String,
    pub screen_id: String,
    pub max_fps: u32,
    st: Mutex<State>,
    stats: Mutex<FrameStats>,
}

/// Счётчики кадров для замеров (fcw-probe, бенчмарки).
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    pub frames: u64,
    /// Размер сообщений FrameUpdate/TextUpdate (байт).
    pub bytes: u64,
    /// Задержка от захвата демоном до готовности кадра у клиента (распакован
    /// и записан в буфер вида), мкс — если демон прислал capture_unix_us.
    pub latency_us: Vec<u64>,
    pub jpeg_rects: u64,
    pub lossless_rects: u64,
    /// Кадры видеорежима (RECT_H264).
    pub video_rects: u64,
}

/// Снимок для `viewInfo`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewInfo {
    pub width: u32,
    pub height: u32,
    pub kind: i32,
    pub seq: u64,
    pub cursor_visible: bool,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub updates: u64,
}

impl View {
    pub fn new(vm_id: &str, screen_id: &str, max_fps: u32, kind: i32) -> View {
        View {
            vm_id: vm_id.to_string(),
            screen_id: screen_id.to_string(),
            max_fps,
            st: Mutex::new(State { kind, ..Default::default() }),
            stats: Mutex::new(FrameStats::default()),
        }
    }

    pub fn info(&self) -> ViewInfo {
        let s = self.st.lock();
        let (cv, cx, cy) = if s.kind == KIND_TEXT {
            let (x, y, v) = s.text_cursor;
            (v, (x as usize * CELL_W) as i32, (y as usize * CELL_H) as i32)
        } else {
            (s.cursor_rect().is_some(), s.cursor_pos.0, s.cursor_pos.1)
        };
        ViewInfo {
            width: s.width,
            height: s.height,
            kind: s.kind,
            seq: s.seq,
            cursor_visible: cv,
            cursor_x: cx,
            cursor_y: cy,
            updates: s.updates,
        }
    }

    /// Применяет графическое обновление (прямоугольники уже распакованы).
    pub fn apply_frame(&self, u: &spb::FrameUpdate, rects: Vec<Decoded>) {
        let mut s = self.st.lock();
        s.kind = KIND_GRAPHICAL;
        if s.width != u.width || s.height != u.height {
            s.alloc(u.width, u.height);
        }
        let (w, h) = (s.width as i32, s.height as i32);
        let stride = s.width as usize * 4;
        for d in rects {
            match d {
                Decoded::Pixels(r, px) => {
                    let Some(c) = (RectI { x: r.x as i32, y: r.y as i32, w: r.w as i32, h: r.h as i32 }).clip(w, h)
                    else {
                        continue;
                    };
                    let src_stride = r.w as usize * 4;
                    for row in 0..c.h as usize {
                        let so = row * src_stride;
                        let doff = (c.y as usize + row) * stride + c.x as usize * 4;
                        let len = c.w as usize * 4;
                        s.base[doff..doff + len].copy_from_slice(&px[so..so + len]);
                    }
                    s.mark(c);
                }
                Decoded::Solid(r, color) => {
                    let Some(c) = (RectI { x: r.x as i32, y: r.y as i32, w: r.w as i32, h: r.h as i32 }).clip(w, h)
                    else {
                        continue;
                    };
                    for row in 0..c.h as usize {
                        let doff = (c.y as usize + row) * stride + c.x as usize * 4;
                        for px in s.base[doff..doff + c.w as usize * 4].as_chunks_mut::<4>().0 {
                            px.copy_from_slice(&color);
                        }
                    }
                    s.mark(c);
                }
                Decoded::Copy(r) => {
                    let Some(c) = (RectI { x: r.x as i32, y: r.y as i32, w: r.w as i32, h: r.h as i32 }).clip(w, h)
                    else {
                        continue;
                    };
                    let src = RectI { x: r.src_x as i32, y: r.src_y as i32, w: c.w, h: c.h };
                    if src.clip(w, h) != Some(src) {
                        continue;
                    }
                    let tmp: Vec<u8> = (0..c.h as usize)
                        .flat_map(|row| {
                            let o = (src.y as usize + row) * stride + src.x as usize * 4;
                            s.base[o..o + c.w as usize * 4].to_vec()
                        })
                        .collect();
                    for row in 0..c.h as usize {
                        let doff = (c.y as usize + row) * stride + c.x as usize * 4;
                        let so = row * c.w as usize * 4;
                        s.base[doff..doff + c.w as usize * 4].copy_from_slice(&tmp[so..so + c.w as usize * 4]);
                    }
                    s.mark(c);
                }
            }
        }
        if u.keyframe {
            s.full = true;
            s.any_dirty = true;
        }
        s.seq = u.seq;
        s.updates += 1;
    }

    /// Применяет обновление текстового экрана.
    pub fn apply_text(&self, u: &spb::TextUpdate) {
        let mut s = self.st.lock();
        s.kind = KIND_TEXT;
        let cols = u.cols.clamp(1, 512);
        let rows = u.rows.clamp(1, 256);
        if s.cols != cols || s.rows != rows || s.cells.len() != (cols * rows) as usize {
            s.cols = cols;
            s.rows = rows;
            s.cells = vec![BLANK; (cols * rows) as usize];
            s.alloc(cols * CELL_W as u32, rows * CELL_H as u32);
            for r in 0..rows {
                s.render_text_row(r);
            }
        }
        let mut touched: Vec<u32> = Vec::new();
        for row in &u.rows_changed {
            if row.y >= rows {
                continue;
            }
            let base = (row.y * cols) as usize;
            let mut run_iter = row.runs.iter();
            let mut run = run_iter.next();
            let mut left = run.map(|r| r.len).unwrap_or(u32::MAX);
            for c in 0..cols as usize {
                let ch = row.chars.get(c).copied().unwrap_or(0);
                while left == 0 {
                    run = run_iter.next();
                    left = run.map(|r| r.len).unwrap_or(u32::MAX);
                }
                let (fg, bg, flags) = run.map(|r| (r.fg, r.bg, r.flags)).unwrap_or((BLANK.fg, BLANK.bg, 0));
                s.cells[base + c] = Cell { ch, fg, bg, flags };
                left = left.saturating_sub(1);
            }
            touched.push(row.y);
        }
        let new_cursor = (u.cursor_x.min(cols - 1), u.cursor_y.min(rows - 1), u.cursor_visible);
        let old_cursor = s.text_cursor;
        s.text_cursor = new_cursor;
        if old_cursor != new_cursor {
            touched.push(old_cursor.1.min(rows - 1));
            touched.push(new_cursor.1);
        }
        touched.sort_unstable();
        touched.dedup();
        for r in touched {
            s.render_text_row(r);
        }
        if u.full {
            s.full = true;
            s.any_dirty = true;
        }
        s.seq = u.seq;
        s.updates += 1;
    }

    pub fn set_cursor_shape(&self, c: Option<CursorImage>) {
        let mut s = self.st.lock();
        s.cursor = c.filter(|c| c.w > 0 && c.h > 0 && c.rgba.len() == (c.w * c.h * 4) as usize);
    }

    pub fn set_cursor_pos(&self, x: i32, y: i32) {
        self.st.lock().cursor_pos = (x, y);
    }

    /// copyDirty: копирует изменения в `dst` (RGBA, width*height*4).
    /// Возвращает число прямоугольников в `rects` (по 4 числа), -1 — обновлён
    /// весь буфер, 0 — изменений нет (или буфер мал: тогда изменения ждут).
    pub fn copy_dirty(&self, dst: &mut [u8], rects: &mut [i32]) -> i32 {
        let mut s = self.st.lock();
        if s.width == 0 || s.height == 0 {
            return 0;
        }
        let need = (s.width * s.height * 4) as usize;
        if dst.len() < need {
            return 0;
        }
        let dst = &mut dst[..need];
        let cur = s.cursor_rect();
        if cur != s.cursor_drawn {
            if let Some(r) = s.cursor_drawn {
                s.mark(r);
            }
            if let Some(r) = cur {
                s.mark(r);
            }
        } else if let Some(r) = cur {
            if s.any_dirty && s.rect_is_dirty(r) {
                s.mark(r);
            }
        }
        if !s.any_dirty {
            return 0;
        }
        let list = if s.full { None } else { Some(s.dirty_rects()) };
        let max = rects.len() / 4;
        let result = match list {
            Some(list) if !list.is_empty() && list.len() <= max => {
                for (i, r) in list.iter().enumerate() {
                    s.copy_rect(dst, *r);
                    rects[i * 4] = r.x;
                    rects[i * 4 + 1] = r.y;
                    rects[i * 4 + 2] = r.w;
                    rects[i * 4 + 3] = r.h;
                }
                list.len() as i32
            }
            _ => {
                dst.copy_from_slice(&s.base);
                -1
            }
        };
        if let Some(r) = cur {
            s.blend_cursor(dst, r);
        }
        s.cursor_drawn = cur;
        s.clear_dirty();
        result
    }

    /// Учёт применённого кадра (размер сообщения и метка времени захвата).
    pub fn record_frame(
        &self,
        bytes: usize,
        capture_unix_us: u64,
        jpeg_rects: u64,
        lossless_rects: u64,
        video_rects: u64,
    ) {
        let mut st = self.stats.lock();
        st.frames += 1;
        st.bytes += bytes as u64;
        st.jpeg_rects += jpeg_rects;
        st.lossless_rects += lossless_rects;
        st.video_rects += video_rects;
        if capture_unix_us != 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_micros() as u64)
                .unwrap_or(0);
            st.latency_us.push(now.saturating_sub(capture_unix_us));
        }
    }

    /// Счётчики с прошлого вызова (обнуляются).
    pub fn take_stats(&self) -> FrameStats {
        std::mem::take(&mut *self.stats.lock())
    }

    /// Полный снимок кадра с курсором (для fcw-probe).
    pub fn snapshot(&self) -> (u32, u32, Vec<u8>) {
        let s = self.st.lock();
        let mut out = s.base.clone();
        if let Some(r) = s.cursor_rect() {
            s.blend_cursor(&mut out, r);
        }
        (s.width, s.height, out)
    }

    /// Текст сетки (для fcw-probe и тестов).
    pub fn text_contents(&self) -> String {
        let s = self.st.lock();
        let mut out = String::new();
        for r in 0..s.rows {
            let line: String = (0..s.cols)
                .map(|c| {
                    let ch = s.cells[(r * s.cols + c) as usize].ch;
                    if ch == 0 {
                        ' '
                    } else {
                        char::from_u32(ch).unwrap_or('?')
                    }
                })
                .collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    /// Пометить весь буфер к копированию (например, после пересоздания текстуры).
    pub fn invalidate(&self) {
        let mut s = self.st.lock();
        s.full = true;
        s.any_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_rects_plain(rects: Vec<spb::Rect>) -> Vec<Decoded> {
        decode_rects(rects, &mut VideoDecoder::default())
    }

    fn solid(x: u32, y: u32, w: u32, h: u32, c: [u8; 4]) -> spb::Rect {
        spb::Rect {
            x,
            y,
            w,
            h,
            encoding: spb::RectEncoding::RectSolid as i32,
            data: c.to_vec(),
            src_x: 0,
            src_y: 0,
            ..Default::default()
        }
    }

    #[test]
    fn frame_and_dirty() {
        let v = View::new("vm", "gfx0", 30, KIND_GRAPHICAL);
        let key = spb::FrameUpdate {
            seq: 1,
            width: 200,
            height: 100,
            keyframe: true,
            rects: vec![],
            capture_unix_us: 0,
            ..Default::default()
        };
        v.apply_frame(&key, decode_rects_plain(vec![solid(0, 0, 200, 100, [1, 2, 3, 255])]));
        let mut buf = vec![0u8; 200 * 100 * 4];
        let mut rects = vec![0i32; 64];
        assert_eq!(v.copy_dirty(&mut buf, &mut rects), -1);
        assert_eq!(&buf[0..4], &[1, 2, 3, 255]);
        assert_eq!(v.copy_dirty(&mut buf, &mut rects), 0);

        let px: Vec<u8> = [9u8, 8, 7, 255].repeat(10 * 10);
        let r = spb::Rect {
            x: 70,
            y: 10,
            w: 10,
            h: 10,
            encoding: spb::RectEncoding::RectRgbaZstd as i32,
            data: zstd::bulk::compress(&px, 1).unwrap(),
            src_x: 0,
            src_y: 0,
            ..Default::default()
        };
        let u = spb::FrameUpdate {
            seq: 2,
            width: 200,
            height: 100,
            keyframe: false,
            rects: vec![r],
            capture_unix_us: 0,
            ..Default::default()
        };
        v.apply_frame(&u, decode_rects_plain(u.rects.clone()));
        assert_eq!(v.copy_dirty(&mut buf, &mut rects), 1);
        assert_eq!(&rects[0..4], &[64, 0, 64, 64]);
        let off = (15 * 200 + 75) * 4;
        assert_eq!(&buf[off..off + 4], &[9, 8, 7, 255]);
        assert_eq!(v.info().seq, 2);

        // мало места для прямоугольников -> весь буфер
        let u2 = spb::FrameUpdate {
            seq: 3,
            width: 200,
            height: 100,
            keyframe: false,
            rects: vec![solid(0, 0, 10, 10, [0; 4]), solid(140, 70, 10, 10, [0; 4])],
            capture_unix_us: 0,
            ..Default::default()
        };
        v.apply_frame(&u2, decode_rects_plain(u2.rects.clone()));
        let mut one = vec![0i32; 4];
        assert_eq!(v.copy_dirty(&mut buf, &mut one), -1);
    }

    #[test]
    fn dirty_rects_merge_vertically() {
        let v = View::new("vm", "gfx0", 30, KIND_GRAPHICAL);
        let key = spb::FrameUpdate {
            seq: 1,
            width: 256,
            height: 256,
            keyframe: true,
            rects: vec![],
            capture_unix_us: 0,
            ..Default::default()
        };
        v.apply_frame(&key, vec![]);
        let mut buf = vec![0u8; 256 * 256 * 4];
        let mut rects = vec![0i32; 64];
        v.copy_dirty(&mut buf, &mut rects);
        let u = spb::FrameUpdate {
            seq: 2,
            width: 256,
            height: 256,
            keyframe: false,
            rects: vec![solid(64, 0, 128, 128, [5; 4])],
            capture_unix_us: 0,
            ..Default::default()
        };
        v.apply_frame(&u, decode_rects_plain(u.rects.clone()));
        assert_eq!(v.copy_dirty(&mut buf, &mut rects), 1);
        assert_eq!(&rects[0..4], &[64, 0, 128, 128]);
    }

    #[test]
    fn cursor_overlay() {
        let v = View::new("vm", "gfx0", 30, KIND_GRAPHICAL);
        let key = spb::FrameUpdate {
            seq: 1,
            width: 128,
            height: 128,
            keyframe: true,
            rects: vec![],
            capture_unix_us: 0,
            ..Default::default()
        };
        v.apply_frame(&key, decode_rects_plain(vec![solid(0, 0, 128, 128, [0, 0, 0, 255])]));
        v.set_cursor_shape(Some(CursorImage { w: 2, h: 2, hot_x: 0, hot_y: 0, rgba: [255u8; 16].to_vec() }));
        v.set_cursor_pos(10, 10);
        let mut buf = vec![0u8; 128 * 128 * 4];
        let mut rects = vec![0i32; 64];
        assert_eq!(v.copy_dirty(&mut buf, &mut rects), -1);
        let at = |b: &[u8], x: usize, y: usize| b[(y * 128 + x) * 4];
        assert_eq!(at(&buf, 10, 10), 255);
        assert_eq!(at(&buf, 12, 10), 0);
        // курсор сдвинулся: старое место восстановлено
        v.set_cursor_pos(100, 100);
        let n = v.copy_dirty(&mut buf, &mut rects);
        assert_eq!(n, 2);
        assert_eq!(at(&buf, 10, 10), 0);
        assert_eq!(at(&buf, 100, 100), 255);
        assert!(v.info().cursor_visible);
    }

    #[test]
    fn text_render() {
        let v = View::new("vm", "tty0", 30, KIND_TEXT);
        let mut chars = vec![0u32; 80];
        for (i, c) in "Hi Ж".chars().enumerate() {
            chars[i] = c as u32;
        }
        let u = spb::TextUpdate {
            seq: 5,
            cols: 80,
            rows: 25,
            cursor_x: 4,
            cursor_y: 0,
            cursor_visible: true,
            full: true,
            rows_changed: vec![spb::TextRow {
                y: 0,
                chars,
                runs: vec![spb::AttrRun { len: 80, fg: 0xffffff, bg: 0x000080, flags: 0 }],
            }],
        };
        v.apply_text(&u);
        let i = v.info();
        assert_eq!((i.width, i.height, i.kind, i.seq), (640, 400, KIND_TEXT, 5));
        assert_eq!((i.cursor_x, i.cursor_y), (32, 0));
        assert!(v.text_contents().starts_with("Hi Ж\n"));
        let mut buf = vec![0u8; 640 * 400 * 4];
        let mut rects = vec![0i32; 64];
        assert_eq!(v.copy_dirty(&mut buf, &mut rects), -1);
        // фон первой ячейки — синий
        assert_eq!(&buf[0..4], &[0, 0, 0x80, 255]);
        // где-то в «H» есть белый пиксель
        let h_white = (0..16).any(|y| (0..8).any(|x| buf[(y * 640 + x) * 4] == 255));
        assert!(h_white);
    }
}
