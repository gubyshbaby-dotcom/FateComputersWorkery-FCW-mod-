//! Замер потока экрана: синтетический «гость» меняет кадровый буфер gfx0,
//! демон отдаёт его по QUIC настоящему клиенту (fcw_client::Session), при
//! желании через имитатор медленного канала (fcw_client::shaper).
//!
//! ```text
//! cargo run --release -p fcw-daemon --example screen_bench -- \
//!     --scene video|scroll|text --seconds 10 --rate-mbps 8 --delay-ms 40 --fps 60
//! ```
//!
//! Печатает строку таблицы: fps клиента, Мбит/с, задержка (p50/p95/max) от
//! захвата демоном до прихода к клиенту и время до совпадения картинки клиента
//! с демоном без потерь после остановки сцены.

#![allow(clippy::too_many_arguments)]

use fcw_client::shaper::{self, ShaperConfig};
use fcw_client::Session;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::vm::{Env, ScreenConfig};
use fcw_daemon::{cert, pb, quic};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

const W: usize = 1024;
const H: usize = 768;

struct Opts {
    scene: String,
    seconds: u64,
    rate_mbps: f64,
    delay_ms: u64,
    fps: u32,
    /// Бюджет демона на подписчика (`--screen-max-mbps`), None — по умолчанию.
    max_mbps: Option<f64>,
    max_fps: Option<u32>,
    /// Прежнее поведение: окно 16 МиБ, Cubic, без потерь, без подтверждений,
    /// без бюджета, до 60 fps.
    legacy: bool,
}

fn parse() -> Opts {
    let mut o = Opts {
        scene: "video".into(),
        seconds: 10,
        rate_mbps: 0.0,
        delay_ms: 40,
        fps: 60,
        max_mbps: None,
        max_fps: None,
        legacy: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut v = || it.next().expect("value");
        match a.as_str() {
            "--scene" => o.scene = v(),
            "--seconds" => o.seconds = v().parse().unwrap(),
            "--rate-mbps" => o.rate_mbps = v().parse().unwrap(),
            "--delay-ms" => o.delay_ms = v().parse().unwrap(),
            "--fps" => o.fps = v().parse().unwrap(),
            "--max-mbps" => o.max_mbps = Some(v().parse().unwrap()),
            "--max-fps" => o.max_fps = Some(v().parse().unwrap()),
            "--legacy" => o.legacy = true,
            other => panic!("unknown argument {other}"),
        }
    }
    o
}

// ------------------------------------------------------------ сцены

struct Sin([f32; 1024]);

impl Sin {
    fn new() -> Self {
        Sin(std::array::from_fn(|i| (i as f32 / 1024.0 * std::f32::consts::TAU).sin()))
    }
    fn at(&self, x: f32) -> f32 {
        let i = ((x / std::f32::consts::TAU) * 1024.0) as i64;
        self.0[i.rem_euclid(1024) as usize]
    }
}

/// «Видео»: плавно меняющаяся картинка (много цветов).
fn plasma(sin: &Sin, buf: &mut [u8], stride: usize, x0: usize, y0: usize, w: usize, h: usize, t: f32) {
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let r = 128.0 + 127.0 * sin.at(fx / 23.0 + t);
            let g = 128.0 + 127.0 * sin.at(fy / 17.0 + t * 1.3);
            let b = 128.0 + 127.0 * sin.at((fx + fy) / 31.0 + t * 0.7 + sin.at(fy / 40.0 + t));
            let o = (y0 + y) * stride + (x0 + x) * 4;
            buf[o] = r as u8;
            buf[o + 1] = g as u8;
            buf[o + 2] = b as u8;
        }
    }
}

/// Текст шрифтом клиента + лёгкое размытие (как сглаженный шрифт браузера).
fn text_block(
    buf: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    cols: usize,
    rows: usize,
    seed: &mut u64,
    fg: u32,
    bg: u32,
) {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/   ";
    for r in 0..rows {
        for c in 0..cols {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            let ch = ALPHA[(*seed % ALPHA.len() as u64) as usize] as u32;
            fcw_client::font::draw_cell(buf, stride, x0 + c * 8, y0 + r * 16, ch, fg, bg, 0);
        }
    }
    // сглаживание: 1-2-1 по горизонтали
    for y in y0..y0 + rows * 16 {
        let row = &mut buf[y * stride + x0 * 4..y * stride + (x0 + cols * 8) * 4];
        let orig = row.to_vec();
        for x in 1..cols * 8 - 1 {
            for k in 0..3 {
                let v = (orig[(x - 1) * 4 + k] as u32 + 2 * orig[x * 4 + k] as u32 + orig[(x + 1) * 4 + k] as u32) / 4;
                row[x * 4 + k] = v as u8;
            }
        }
    }
}

/// Вертикальное сглаживание 1-2-1 (вместе с горизонтальным — как
/// сглаженный шрифт браузера, 10-20 оттенков в тайле).
fn blur_v(buf: &mut [u8], stride: usize, x0: usize, y0: usize, w: usize, h: usize) {
    let orig: Vec<u8> = buf[y0 * stride..(y0 + h) * stride].to_vec();
    for y in 1..h - 1 {
        for x in x0 * 4..(x0 + w) * 4 {
            let v = (orig[(y - 1) * stride + x] as u32
                + 2 * orig[y * stride + x] as u32
                + orig[(y + 1) * stride + x] as u32)
                / 4;
            buf[(y0 + y) * stride + x] = v as u8;
        }
    }
}

/// «Веб-страница» высотой `ph`: абзацы текста и картинки.
fn web_page(sin: &Sin, ph: usize) -> Vec<u8> {
    let mut p = vec![255u8; W * ph * 4];
    let mut seed = 0x9e3779b97f4a7c15u64;
    let mut y = 16;
    let mut n = 0;
    while y + 320 < ph {
        if n % 3 == 2 {
            plasma(sin, &mut p, W * 4, 112, y, 640, 300, n as f32);
            y += 316;
        } else {
            text_block(&mut p, W * 4, 64, y, 110, 10, &mut seed, 0x202020, 0xffffff);
            blur_v(&mut p, W * 4, 64, y, 110 * 8, 160);
            y += 176;
        }
        n += 1;
    }
    p
}

fn render(scene: &str, sin: &Sin, page: &[u8], frame: u64, seed: &mut u64, buf: &mut [u8]) {
    match scene {
        "video" => {
            // рамка «плеера» на статичном интерфейсе
            plasma(sin, buf, W * 4, 256, 192, 512, 384, frame as f32 * 0.08);
        }
        "scroll" => {
            let ph = page.len() / (W * 4);
            let off = ((frame * 8) as usize) % (ph - H);
            buf.copy_from_slice(&page[off * W * 4..(off + H) * W * 4]);
        }
        "text" => text_block(buf, W * 4, 0, 0, W / 8, H / 16, seed, 0xd0d0d0, 0x101010),
        other => panic!("unknown scene {other}"),
    }
}

fn scene_fps(scene: &str) -> u64 {
    match scene {
        "video" => 30,
        "scroll" => 60,
        _ => 20,
    }
}

fn pct(v: &mut [u64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_unstable();
    let i = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[i] as f64 / 1000.0
}

/// Гистограмма числа цветов в тайлах 64x64 PNG-снимка (для порогов).
fn count_png(path: &str) {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let bpp = info.line_size / w;
    let mut hist = [0usize; 6];
    let (mut raw4, mut z4, mut z3) = (0usize, 0usize, 0usize);
    for ty in (0..h).step_by(64) {
        for tx in (0..w).step_by(64) {
            let (tw, th) = (64.min(w - tx), 64.min(h - ty));
            let mut t = Vec::with_capacity(tw * th * 4);
            let mut t3 = Vec::with_capacity(tw * th * 3);
            for y in ty..ty + th {
                for x in tx..tx + tw {
                    let o = y * info.line_size + x * bpp;
                    t.extend_from_slice(&[buf[o], buf[o + 1], buf[o + 2], 255]);
                    t3.extend_from_slice(&[buf[o], buf[o + 1], buf[o + 2]]);
                }
            }
            let c = fcw_daemon::gfx::count_colors(&t, 1000);
            let b = match c {
                1 => 0,
                2..=4 => 1,
                5..=16 => 2,
                17..=64 => 3,
                65..=200 => 4,
                _ => 5,
            };
            hist[b] += 1;
            if c > 1 {
                raw4 += t.len();
                z4 += zstd::bulk::compress(&t, 1).unwrap().len();
                z3 += zstd::bulk::compress(&t3, 1).unwrap().len();
            }
        }
    }
    println!(
        "{path}: colors per tile 1:{} 2-4:{} 5-16:{} 17-64:{} 65-200:{} >200:{}",
        hist[0], hist[1], hist[2], hist[3], hist[4], hist[5]
    );
    println!(
        "  non-solid tiles: raw {} KiB, zstd RGBA {} KiB, zstd RGB {} KiB ({:+.1}%)",
        raw4 / 1024,
        z4 / 1024,
        z3 / 1024,
        (z3 as f64 / z4 as f64 - 1.0) * 100.0
    );
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    if let Some(p) = std::env::args().nth(1).filter(|a| a.ends_with(".png")) {
        for p in std::env::args().skip(1) {
            count_png(&p);
        }
        let _ = p;
        return;
    }
    let o = parse();
    let dir = std::env::temp_dir().join(format!("fcw-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = cert::load_or_create(&dir).unwrap();
    let tuning =
        if o.legacy { quic::Tuning { send_window: 16 * 1024 * 1024, bbr: false } } else { quic::Tuning::default() };
    let ep = quic::make_endpoint_tuned("127.0.0.1:0".parse().unwrap(), &c, tuning).unwrap();
    let port = ep.local_addr().unwrap().port();
    let mut screen = if o.legacy {
        ScreenConfig { max_bytes_per_sec: 0, max_fps: 60, lossy: false, frame_acks: false, local_unlimited: false }
    } else {
        // клиент идёт через 127.0.0.1, но изображает удалённого игрока
        ScreenConfig { local_unlimited: false, ..ScreenConfig::default() }
    };
    if let Some(m) = o.max_mbps {
        screen.max_bytes_per_sec = (m * 125_000.0) as u64;
    }
    if let Some(f) = o.max_fps {
        screen.max_fps = f;
    }
    let env =
        Arc::new(Env { images_dir: dir.join("images"), qemu: QemuLocator::fixed(None), tz: None, net: None, screen });
    let d = Daemon::new(env, dir.join("vms"), 4, port, c.sha256_hex.clone(), tokio::sync::broadcast::channel(16).0);
    let (vm, _) = d
        .vm_for_create(&pb::CreateVm { vm_id: "b".into(), image_id: "x".into(), gpu: true, ..Default::default() })
        .unwrap();
    vm.gfx.resize(W as u32, H as u32);
    tokio::spawn(quic::serve(d.clone(), ep.clone()));

    // канал
    let (addr, shaper_stats) = if o.rate_mbps > 0.0 {
        let rate = (o.rate_mbps * 125_000.0) as u64;
        let cfg = ShaperConfig {
            rate_down: rate,
            rate_up: rate,
            delay: Duration::from_millis(o.delay_ms),
            max_queue: Duration::from_secs(10),
        };
        let (a, s) = shaper::start(format!("127.0.0.1:{port}").parse().unwrap(), cfg).await.unwrap();
        (a, Some(s))
    } else {
        (format!("127.0.0.1:{port}").parse().unwrap(), None)
    };
    let (token, _) = d.tokens.lock().issue("b", "p", "bench", 7, 60);
    let s = Session::connect("127.0.0.1", addr.port(), &c.sha256_hex);
    while s.state() == fcw_client::session::STATE_CONNECTING {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    s.attach(&token);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let view = s.open_view("b", "gfx0", o.fps);

    // начальная картинка: светлый «интерфейс»
    let sin = Sin::new();
    let page = web_page(&sin, 4000);
    let mut buf = vec![0xe8u8; W * H * 4];
    let mut seed = 1u64;
    vm.gfx.apply_rgbx(0, 0, W as u32, H as u32, &buf);
    vm.gfx.commit();
    let t0 = Instant::now();
    while view.info().width == 0 && t0.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _ = view.take_stats();
    let down0 = shaper_stats.as_ref().map(|s| s.down_bytes.load(Ordering::Relaxed)).unwrap_or(0);

    // сцена
    let fps = scene_fps(&o.scene);
    let start = Instant::now();
    let mut frame = 0u64;
    let mut tick = tokio::time::interval(Duration::from_micros(1_000_000 / fps));
    let mut produced = 0u64;
    while start.elapsed() < Duration::from_secs(o.seconds) {
        tick.tick().await;
        render(&o.scene, &sin, &page, frame, &mut seed, &mut buf);
        vm.gfx.apply_rgbx(0, 0, W as u32, H as u32, &buf);
        vm.gfx.commit();
        frame += 1;
        produced += 1;
    }
    let elapsed = start.elapsed().as_secs_f64();
    let mut st = view.take_stats();
    let down = shaper_stats.as_ref().map(|s| s.down_bytes.load(Ordering::Relaxed)).unwrap_or(0) - down0;

    // сходимость: картинка клиента = картинка демона (без потерь)
    let stop = Instant::now();
    let converged = loop {
        let (_, _, srv) = vm.gfx.snapshot();
        let (_, _, cli) = view.snapshot();
        if srv == cli {
            break Some(stop.elapsed().as_secs_f64());
        }
        if stop.elapsed() > Duration::from_secs(30) {
            break None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let tail = view.take_stats();
    let max_q = shaper_stats.as_ref().map(|s| s.max_queue_us.load(Ordering::Relaxed) as f64 / 1000.0);

    let mbps = st.bytes as f64 * 8.0 / elapsed / 1e6;
    let wire = if down > 0 { format!("{:.1}", down as f64 * 8.0 / elapsed / 1e6) } else { "-".into() };
    let lat = &mut st.latency_us;
    let (p50, p95) = (pct(lat, 0.5), pct(lat, 0.95));
    let max = lat.iter().copied().max().unwrap_or(0) as f64 / 1000.0;
    println!(
        "| {} | {} | {:.0}/{:.0} | {:.1} | {} | {:.0} | {:.0} | {:.0} | {} | {} | {}/{} |",
        if o.legacy { format!("{} (было)", o.scene) } else { o.scene.clone() },
        if o.rate_mbps > 0.0 {
            format!("{} Мбит/с, {} мс", o.rate_mbps, o.delay_ms)
        } else {
            "без огр.".into()
        },
        st.frames as f64 / elapsed,
        produced as f64 / elapsed,
        mbps,
        wire,
        p50,
        p95,
        max,
        converged.map(|c| format!("{c:.1} с")).unwrap_or_else(|| "нет за 30 с".into()),
        max_q.map(|q| format!("{q:.0} мс")).unwrap_or_else(|| "-".into()),
        st.jpeg_rects + tail.jpeg_rects,
        st.lossless_rects + tail.lossless_rects,
    );
    let _ = std::fs::remove_dir_all(&dir);
    std::process::exit(0);
}
