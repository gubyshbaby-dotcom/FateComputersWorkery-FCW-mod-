//! Замер кодека видеорежима на сырых кадрах RGBA: время кодирования и
//! декодирования, битрейт, PSNR.
//!
//! ```text
//! ffmpeg -i film.mp4 -frames:v 240 -pix_fmt rgba -f rawvideo film.rgba
//! cargo run --release -p fcw-daemon --example h264_bench -- \
//!     --raw film.rgba --size 1920x1080 --mbps 2,5,10,20 [--threads 2] [--layers 3] \
//!     [--complexity 0] [--no-cabac] [--no-skip] [--fps 30] [--png out-prefix]
//! ```

use fcw_daemon::h264::{H264Config, H264Encoder};
use std::io::Read;
use std::time::Instant;

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn main() {
    let mut raw = String::new();
    let (mut w, mut h) = (1920usize, 1080usize);
    let mut mbps: Vec<f64> = vec![2.0, 5.0, 10.0, 20.0];
    let (mut threads, mut layers, mut complexity, mut cabac, mut skip, mut fps) = (1u16, 3u8, 0u8, true, true, 30.0f32);
    let mut png: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut v = || it.next().expect("value");
        match a.as_str() {
            "--raw" => raw = v(),
            "--size" => {
                let s = v();
                let (a, b) = s.split_once('x').expect("WxH");
                (w, h) = (a.parse().unwrap(), b.parse().unwrap());
            }
            "--mbps" => mbps = v().split(',').map(|x| x.parse().unwrap()).collect(),
            "--threads" => threads = v().parse().unwrap(),
            "--layers" => layers = v().parse().unwrap(),
            "--complexity" => complexity = v().parse().unwrap(),
            "--no-cabac" => cabac = false,
            "--no-skip" => skip = false,
            "--fps" => fps = v().parse().unwrap(),
            "--png" => png = Some(v()),
            other => panic!("unknown argument {other}"),
        }
    }
    let mut data = Vec::new();
    std::fs::File::open(&raw).expect("raw file").read_to_end(&mut data).unwrap();
    let fsz = w * h * 4;
    let n = data.len() / fsz;
    println!(
        "{raw}: {n} кадров {w}x{h}, {fps} к/с, потоков {threads}, слоёв {layers}, сложность {complexity}, \
         cabac {cabac}, пропуск {skip}"
    );
    println!(
        "| цель, Мбит/с | факт | кодирование, мс ср/p95/макс | доля ядра при {fps} к/с | декодирование, мс ср/p95 \
         | PSNR, дБ ср/мин | ключевой, КиБ | пропущено |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for m in mbps {
        let mut cfg = H264Config::new(w as u32, h as u32, (m * 1e6) as u32, fps);
        cfg.threads = threads;
        cfg.temporal_layers = layers;
        cfg.complexity = complexity;
        cfg.cabac = cabac;
        cfg.frame_skip = skip;
        let mut enc = H264Encoder::new(cfg).unwrap();
        let mut dec = openh264::decoder::Decoder::new().unwrap();
        let (mut te, mut td, mut psnr) = (Vec::new(), Vec::new(), Vec::new());
        let (mut bytes, mut skipped, mut key_bytes) = (0usize, 0usize, 0usize);
        let mut out = vec![0u8; fsz];
        for i in 0..n {
            let src = &data[i * fsz..(i + 1) * fsz];
            let t = Instant::now();
            let e = enc.encode_rgba(src, (i as f64 * 1000.0 / fps as f64) as u64).unwrap();
            te.push(t.elapsed().as_secs_f64() * 1000.0);
            let Some(e) = e else {
                skipped += 1;
                continue;
            };
            bytes += e.data.len();
            if e.key {
                key_bytes = key_bytes.max(e.data.len());
            }
            let t = Instant::now();
            let yuv = dec.decode(&e.data).unwrap().expect("picture");
            yuv.write_rgba8(&mut out);
            td.push(t.elapsed().as_secs_f64() * 1000.0);
            let mut err = 0u64;
            for (a, b) in src.as_chunks::<4>().0.iter().zip(out.as_chunks::<4>().0) {
                for k in 0..3 {
                    let d = a[k] as i32 - b[k] as i32;
                    err += (d * d) as u64;
                }
            }
            let mse = err as f64 / (w * h * 3) as f64;
            psnr.push(10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10());
            if let Some(p) = &png {
                if i == n * 2 / 3 {
                    save_png(&format!("{p}-{m}mbps.png"), w, h, &out);
                    save_png(&format!("{p}-src.png"), w, h, src);
                }
            }
        }
        let secs = n as f64 / fps as f64;
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        let (em, dm, pm) = (mean(&te), mean(&td), mean(&psnr));
        let emax = te.iter().cloned().fold(0.0, f64::max);
        let pmin = psnr.iter().cloned().fold(f64::INFINITY, f64::min);
        println!(
            "| {m} | {:.2} | {em:.1}/{:.1}/{emax:.1} | {:.2} | {dm:.1}/{:.1} | {pm:.1}/{pmin:.1} | {} | {skipped} |",
            bytes as f64 * 8.0 / secs / 1e6,
            pct(&mut te, 0.95),
            em * fps as f64 / 1000.0 * threads as f64,
            pct(&mut td, 0.95),
            key_bytes / 1024,
        );
    }
}

fn save_png(path: &str, w: usize, h: usize, rgba: &[u8]) {
    let f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut e = png::Encoder::new(f, w as u32, h as u32);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}
