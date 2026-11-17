//! Сквозные проверки без QEMU: канал управления и QUIC-сервер демона вместе с
//! клиентской сессией fcw-client (тот же код, что в JNI-библиотеке).

#![cfg(unix)]

use fcw_client::Session;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::video::VideoConfig;
use fcw_daemon::vm::{Env, ScreenConfig};
use fcw_daemon::{cert, control, pb, quic};
use fcw_proto::control::{envelope, request, response};
use fcw_proto::frame;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fcw-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn daemon(dir: &Path, port: u16, sha: &str) -> Arc<Daemon> {
    daemon_with(dir, port, sha, ScreenConfig::default())
}

fn daemon_with(dir: &Path, port: u16, sha: &str, screen: ScreenConfig) -> Arc<Daemon> {
    let (events, _) = broadcast::channel(256);
    let env =
        Arc::new(Env { images_dir: dir.join("images"), qemu: QemuLocator::fixed(None), tz: None, net: None, screen });
    Daemon::new(env, dir.join("vms"), 4, port, sha.to_string(), events)
}

async fn wait_for(what: &str, timeout_ms: u64, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        if t.elapsed() > Duration::from_millis(timeout_ms) {
            panic!("timeout waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Ждёт событие клиента с заданным типом и возвращает его.
async fn wait_event(s: &Session, ty: &str) -> serde_json::Value {
    let t = Instant::now();
    loop {
        while let Some(e) = s.poll_event() {
            let v: serde_json::Value = serde_json::from_str(&e).unwrap();
            if v["type"] == ty {
                return v;
            }
        }
        if t.elapsed() > Duration::from_secs(10) {
            panic!("no {ty} event");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quic_screens_input_control_and_revoke() {
    let dir = tmpdir("quic");
    let c = cert::load_or_create(&dir).unwrap();
    let ep = quic::make_endpoint("127.0.0.1:0".parse().unwrap(), &c).unwrap();
    let port = ep.local_addr().unwrap().port();
    let d = daemon(&dir, port, &c.sha256_hex);
    let (vm, _) = d
        .vm_for_create(&pb::CreateVm { vm_id: "vm1".into(), image_id: "img".into(), gpu: true, ..Default::default() })
        .unwrap();
    // экраны наполняем напрямую, как это делали бы консоль и VNC
    vm.tty.feed("Hello \x1b[1;31mFCW\x1b[0m Привет\r\n".as_bytes());
    vm.gfx.resize(130, 70);
    let red: Vec<u8> = [200u8, 10, 20, 0].repeat(10 * 10);
    vm.gfx.apply_rgbx(5, 5, 10, 10, &red);
    vm.gfx.commit();
    tokio::spawn(quic::serve(d.clone(), ep.clone()));

    // неверный отпечаток сертификата -> ошибка
    let bad = Session::connect("127.0.0.1", port, &"ab".repeat(32));
    wait_for("bad pin error", 10_000, || bad.state() == fcw_client::session::STATE_ERROR).await;
    assert!(bad.error().contains("fingerprint"), "{}", bad.error());

    // игрок 1: смотрит и управляет
    let (tok1, _) = d.tokens.lock().issue("vm1", "uuid-1", "Alex", 7, 60);
    let s1 = Session::connect("127.0.0.1", port, &c.sha256_hex);
    wait_for("ready", 10_000, || s1.state() == fcw_client::session::STATE_READY).await;
    s1.attach("not-a-token");
    let a = wait_event(&s1, "attach").await;
    assert_eq!(a["ok"], false);
    s1.attach(&tok1);
    let a = wait_event(&s1, "attach").await;
    assert_eq!(a["ok"], true, "{a}");
    assert_eq!(a["permissions"], 7);
    let ids: Vec<String> = a["screens"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().into()).collect();
    assert_eq!(ids, vec!["gfx0", "tty0"]);
    let ic = wait_event(&s1, "input_control").await;
    assert_eq!(ic["active"], true);
    assert_eq!(vm.controller().unwrap().player_name, "Alex");
    // токен одноразовый
    s1.attach(&tok1);
    assert_eq!(wait_event(&s1, "attach").await["ok"], false);

    let tv = s1.open_view("vm1", "tty0", 60);
    wait_for("tty text", 10_000, || tv.text_contents().starts_with("Hello FCW Привет\n")).await;
    let i = tv.info();
    assert_eq!((i.width, i.height, i.kind), (640, 400, 2));
    let gv = s1.open_view("vm1", "gfx0", 60);
    wait_for("gfx frame", 10_000, || gv.info().width == 130).await;
    let px = |v: &fcw_client::View, x: usize, y: usize| {
        let (w, _, rgba) = v.snapshot();
        let i = (y * w as usize + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    assert_eq!(px(&gv, 7, 7), [200, 10, 20, 255]);
    assert_eq!(px(&gv, 100, 60), [0, 0, 0, 255]);
    // copyDirty: первый раз весь буфер
    let mut buf = vec![0u8; 130 * 70 * 4];
    let mut rects = vec![0i32; 64];
    assert_eq!(gv.copy_dirty(&mut buf, &mut rects), -1);

    // инкрементальные обновления
    vm.gfx.apply_rgbx(100, 60, 1, 1, &[0, 255, 0, 0]);
    vm.gfx.commit();
    wait_for("gfx delta", 10_000, || px(&gv, 100, 60) == [0, 255, 0, 255]).await;
    let n = gv.copy_dirty(&mut buf, &mut rects);
    assert_eq!(n, 1, "one dirty tile");
    assert_eq!(&rects[..4], &[64, 0, 64, 64]);
    vm.tty.feed(b"second line");
    wait_for("tty delta", 10_000, || tv.text_contents().contains("\nsecond line\n")).await;

    // игрок 2 только смотрит: управление не забирает
    let (tok2, _) = d.tokens.lock().issue("vm1", "uuid-2", "Bea", 1, 60);
    let s2 = Session::connect("127.0.0.1", port, &c.sha256_hex);
    s2.attach(&tok2);
    assert_eq!(wait_event(&s2, "attach").await["ok"], true);
    assert_eq!(vm.controller().unwrap().player_name, "Alex");
    // игрок 3 с клавиатурой забирает управление у первого
    let (tok3, _) = d.tokens.lock().issue("vm1", "uuid-3", "Cid", 3, 60);
    let s3 = Session::connect("127.0.0.1", port, &c.sha256_hex);
    s3.attach(&tok3);
    assert_eq!(wait_event(&s3, "attach").await["ok"], true);
    assert_eq!(wait_event(&s3, "input_control").await["active"], true);
    let lost = wait_event(&s1, "input_control").await;
    assert_eq!(lost["active"], false);
    assert_eq!(lost["controller_player"], "Cid");
    assert_eq!(vm.controller().unwrap().player_name, "Cid");

    // отзыв доступа игрока 3: привязка снимается, управление освобождается
    d.revoke("vm1", "uuid-3");
    let r = wait_event(&s3, "attach").await;
    assert_eq!(r["ok"], false);
    assert_eq!(r["vm_id"], "vm1");
    wait_for("controller released", 10_000, || vm.controller().is_none()).await;

    // закрытие сессии
    s1.close();
    wait_for("session finished", 10_000, || s1.finished()).await;
    assert_eq!(s1.state(), fcw_client::session::STATE_CLOSED);
    s2.close();
    s3.close();
    let _ = std::fs::remove_dir_all(&dir);
}

async fn call(s: &mut tokio::net::UnixStream, id: u64, kind: request::Kind) -> pb::Response {
    let env = pb::Envelope { id, body: Some(envelope::Body::Request(pb::Request { kind: Some(kind) })) };
    frame::write_async(s, &env).await.unwrap();
    loop {
        let e: pb::Envelope = frame::read_async(s).await.unwrap().unwrap();
        if let Some(envelope::Body::Response(r)) = e.body {
            assert_eq!(e.id, id);
            return r;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn control_channel() {
    let dir = tmpdir("control");
    let d = daemon(&dir, 25600, "00");
    // образ без файлов: виден в списке, но не установлен
    std::fs::create_dir_all(dir.join("images/alp")).unwrap();
    std::fs::write(dir.join("images/alp/manifest.json"), br#"{"format":1,"id":"alp","name":"Alpine"}"#).unwrap();
    let sock = dir.join("control.sock");
    let l = LocalListener::bind(&sock).unwrap();
    tokio::spawn(control::serve(d.clone(), l));
    let mut s = tokio::net::UnixStream::connect(&sock).await.unwrap();

    let r = call(
        &mut s,
        1,
        request::Kind::Hello(pb::Hello { client_name: "test".into(), protocol_version: 1, ..Default::default() }),
    )
    .await;
    assert!(r.ok);
    let Some(response::Kind::Hello(h)) = r.kind else { panic!() };
    assert_eq!((h.quic_port, h.accelerator.as_str(), h.max_vms), (25600, "tcg", 4));

    let r = call(&mut s, 2, request::Kind::ListImages(pb::ListImages {})).await;
    let Some(response::Kind::Images(l)) = r.kind else { panic!() };
    assert_eq!(l.images.len(), 1);
    assert_eq!(l.images[0].name, "Alpine");
    assert!(!l.images[0].installed);

    let create =
        pb::CreateVm { vm_id: "m1".into(), image_id: "alp".into(), cpus: 2, memory_mb: 512, ..Default::default() };
    let r = call(&mut s, 3, request::Kind::CreateVm(create.clone())).await;
    assert!(r.ok, "{}", r.error);
    assert!(dir.join("vms/m1/config.json").is_file());
    let r = call(&mut s, 4, request::Kind::GetVm(pb::GetVm { vm_id: "m1".into() })).await;
    let Some(response::Kind::Vm(v)) = r.kind else { panic!() };
    assert_eq!((v.state, v.cpus, v.memory_mb), (pb::VmState::Stopped as i32, 2, 512));
    assert_eq!(v.screens.len(), 1, "no gpu -> only tty0");

    let r =
        call(&mut s, 5, request::Kind::IssueToken(pb::IssueToken { vm_id: "m1".into(), ..Default::default() })).await;
    let Some(response::Kind::Token(t)) = r.kind else { panic!() };
    assert_eq!(t.token.len(), 43);
    let r =
        call(&mut s, 6, request::Kind::IssueToken(pb::IssueToken { vm_id: "nope".into(), ..Default::default() })).await;
    assert!(!r.ok);

    // запуск без QEMU: ошибка и состояние CRASHED с причиной
    let r = call(&mut s, 7, request::Kind::StartVm(pb::StartVm { vm_id: "m1".into() })).await;
    assert!(!r.ok);
    assert!(r.error.contains("QEMU"), "{}", r.error);
    let r = call(&mut s, 8, request::Kind::GetVm(pb::GetVm { vm_id: "m1".into() })).await;
    let Some(response::Kind::Vm(v)) = r.kind else { panic!() };
    assert_eq!(v.state, pb::VmState::Crashed as i32);
    assert!(!v.last_error.is_empty());

    let r = call(&mut s, 9, request::Kind::AgentSend(pb::AgentSend { vm_id: "m1".into(), json: "{}".into() })).await;
    assert!(!r.ok, "not running");
    let r = call(
        &mut s,
        10,
        request::Kind::SetRedstoneInputs(pb::SetRedstoneInputs { vm_id: "m1".into(), levels: vec![1, 2, 3, 4, 5, 6] }),
    )
    .await;
    assert!(r.ok);

    let r = call(&mut s, 11, request::Kind::DeleteVm(pb::DeleteVm { vm_id: "m1".into() })).await;
    assert!(r.ok, "{}", r.error);
    assert!(!dir.join("vms/m1").exists());
    let r = call(&mut s, 12, request::Kind::ListVms(pb::ListVms {})).await;
    let Some(response::Kind::Vms(l)) = r.kind else { panic!() };
    assert!(l.vms.is_empty());

    let r = call(&mut s, 13, request::Kind::Shutdown(pb::Shutdown {})).await;
    assert!(r.ok);
    assert!(*d.shutdown_signal().borrow());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Тайл «видео»: много цветов.
fn plasma(t: f32) -> Vec<u8> {
    let mut v = Vec::with_capacity(128 * 64 * 4);
    for y in 0..64 {
        for x in 0..128 {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lossy_tiles_are_refined_to_exact_picture() {
    let dir = tmpdir("lossy");
    let c = cert::load_or_create(&dir).unwrap();
    let ep = quic::make_endpoint("127.0.0.1:0".parse().unwrap(), &c).unwrap();
    let port = ep.local_addr().unwrap().port();
    // клиент на 127.0.0.1, но ведём себя как с удалённым игроком; видеорежим
    // выключен: здесь проверяется путь JPEG
    let video = VideoConfig { enabled: false, ..Default::default() };
    let screen = ScreenConfig { local_unlimited: false, video, ..ScreenConfig::default() };
    let d = daemon_with(&dir, port, &c.sha256_hex, screen);
    let (vm, _) = d
        .vm_for_create(&pb::CreateVm { vm_id: "v".into(), image_id: "i".into(), gpu: true, ..Default::default() })
        .unwrap();
    vm.gfx.resize(128, 64);
    tokio::spawn(quic::serve(d.clone(), ep.clone()));
    let (tok, _) = d.tokens.lock().issue("v", "u", "P", 1, 60);
    let s = Session::connect("127.0.0.1", port, &c.sha256_hex);
    s.attach(&tok);
    assert_eq!(wait_event(&s, "attach").await["ok"], true);
    let v = s.open_view("v", "gfx0", 30);
    wait_for("first frame", 10_000, || v.info().width == 128).await;
    // «видео»: часто меняющиеся пёстрые тайлы идут JPEG
    for t in 0..12 {
        vm.gfx.apply_rgbx(0, 0, 128, 64, &plasma(t as f32 * 0.3));
        vm.gfx.commit();
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    wait_for("jpeg tiles", 10_000, || v.info().seq >= 2).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let st = v.take_stats();
    assert!(st.jpeg_rects > 0, "hot busy tiles should be sent lossy: {st:?}");
    assert!(st.latency_us.iter().all(|l| *l < 2_000_000));
    // видео остановилось: тайлы успокоились и досланы без потерь — картинка точная
    wait_for("exact picture", 10_000, || vm.gfx.snapshot().2 == v.snapshot().2).await;
    s.close();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Плавная «плазма» w x h (RGBX).
#[cfg(feature = "video")]
fn plasma_wh(w: usize, h: usize, t: f32) -> Vec<u8> {
    let mut v = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            v.extend_from_slice(&[
                (128.0 + 120.0 * (fx / 37.0 + t).sin()) as u8,
                (128.0 + 120.0 * (fy / 29.0 + t * 1.3).sin()) as u8,
                (128.0 + 120.0 * ((fx + fy) / 51.0 + t * 0.7).sin()) as u8,
                0,
            ]);
        }
    }
    v
}

/// PSNR (дБ) по RGB между двумя кадрами RGBA.
#[cfg(feature = "video")]
fn psnr(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mut err = 0u64;
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        for k in 0..3 {
            let d = p[k] as i64 - q[k] as i64;
            err += (d * d) as u64;
        }
    }
    let mse = err as f64 / (a.len() / 4 * 3) as f64;
    10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10()
}

/// Видеорежим: область с «фильмом» идёт H.264 (одним кодировщиком на всех
/// зрителей ступени), дальний зритель получает меньше кадров, старый клиент —
/// тайлы; после остановки видео картинка у всех становится точной.
#[cfg(feature = "video")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn video_region_goes_h264_then_exact_picture() {
    let dir = tmpdir("video");
    let c = cert::load_or_create(&dir).unwrap();
    let ep = quic::make_endpoint("127.0.0.1:0".parse().unwrap(), &c).unwrap();
    let port = ep.local_addr().unwrap().port();
    // кодировщиков хватает и на младшую ступень для дальнего зрителя
    let video = VideoConfig { max_encoders: 4, ..Default::default() };
    let screen = ScreenConfig { local_unlimited: false, video, ..ScreenConfig::default() };
    assert!(screen.video.enabled);
    let d = daemon_with(&dir, port, &c.sha256_hex, screen);
    let (vm, _) = d
        .vm_for_create(&pb::CreateVm { vm_id: "v".into(), image_id: "i".into(), gpu: true, ..Default::default() })
        .unwrap();
    // 8 x 6 тайлов: серый «рабочий стол», плеер займёт 6 x 4 тайла
    const W: u32 = 512;
    const H: u32 = 384;
    vm.gfx.resize(W, H);
    vm.gfx.apply_rgbx(0, 0, W, H, &[0x60u8, 0x70, 0x80, 0].repeat((W * H) as usize));
    vm.gfx.commit();
    tokio::spawn(quic::serve(d.clone(), ep.clone()));

    let connect = |video: bool, fps: u32| {
        let (tok, _) = d.tokens.lock().issue("v", "u", "P", 1, 60);
        let s = Session::connect("127.0.0.1", port, &c.sha256_hex);
        s.set_accept_video(video);
        s.attach(&tok);
        (s, fps)
    };
    let mut viewers = Vec::new();
    for (video, fps) in [(true, 30), (true, 30), (true, 8), (false, 30)] {
        let (s, fps) = connect(video, fps);
        assert_eq!(wait_event(&s, "attach").await["ok"], true);
        let v = s.open_view("v", "gfx0", fps);
        viewers.push((s, v));
    }
    for (_, v) in &viewers {
        wait_for("first frame", 10_000, || v.info().width == W).await;
        v.take_stats();
    }

    // «фильм»: 2 с по 30 кадров в секунду; часы в углу тикают вне видеообласти
    let mut region = None;
    for t in 0..60u32 {
        vm.gfx.apply_rgbx(64, 64, 384, 256, &plasma_wh(384, 256, t as f32 * 0.07));
        if t % 10 == 0 {
            vm.gfx.apply_rgbx(480, 4, 8, 8, &[t as u8, 255, 0, 0].repeat(64));
        }
        vm.gfx.commit();
        region = region.or(vm.gfx.video_region());
        tokio::time::sleep(Duration::from_millis(33)).await;
    }
    let region = region.expect("video detected");
    assert_eq!((region.x, region.y, region.w, region.h), (64, 64, 384, 256));
    // кодировщиков два, а не по одному на зрителя: общая ступень двух ближних
    // и младшая ступень дальнего
    assert_eq!(fcw_daemon::video::active_encoders(), 2);

    // сразу после остановки: у зрителей с видео — последний кадр H.264, близкий к оригиналу
    tokio::time::sleep(Duration::from_millis(250)).await;
    let server = vm.gfx.snapshot().2;
    let stats: Vec<_> = viewers.iter().map(|(_, v)| v.take_stats()).collect();
    for (i, st) in stats.iter().enumerate() {
        // дальний зритель мог остановиться на кадре постарше: его не сравниваем
        let q = psnr(&server, &viewers[i].1.snapshot().2);
        assert!(i == 2 || q > 30.0, "viewer {i}: psnr {q:.1}");
        assert!(st.latency_us.iter().all(|l| *l < 2_000_000), "viewer {i}: {st:?}");
    }
    assert!(stats[0].video_rects >= 30, "{:?}", stats[0]);
    assert!(stats[1].video_rects >= 30, "{:?}", stats[1]);
    // дальний зритель (8 к/с): только базовый временной слой — около четверти кадров
    assert!(stats[2].video_rects >= 5 && stats[2].video_rects * 2 < stats[0].video_rects, "{:?}", stats[2]);
    // старый клиент: без видео, тайлы JPEG
    assert_eq!(stats[3].video_rects, 0);
    assert!(stats[3].jpeg_rects > 0, "{:?}", stats[3]);
    // тайлы видеообласти зрителям с видео не шли (только часы и первые кадры до распознавания)
    assert!(stats[0].jpeg_rects + stats[0].lossless_rects < stats[3].jpeg_rects / 2, "{:?}", stats[0]);

    // видео кончилось: область снята, тайлы досланы без потерь — картинка точная у всех
    for (i, (_, v)) in viewers.iter().enumerate() {
        wait_for(&format!("exact picture for viewer {i}"), 15_000, || vm.gfx.snapshot().2 == v.snapshot().2).await;
    }
    assert_eq!(vm.gfx.video_region(), None);
    for (s, _) in &viewers {
        s.close();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
