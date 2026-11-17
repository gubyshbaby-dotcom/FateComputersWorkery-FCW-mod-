//! Сквозные проверки без QEMU: канал управления и QUIC-сервер демона вместе с
//! клиентской сессией fcw-client (тот же код, что в JNI-библиотеке).

#![cfg(unix)]

use fcw_client::Session;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::vm::Env;
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
    let (events, _) = broadcast::channel(256);
    let env = Arc::new(Env { images_dir: dir.join("images"), qemu: QemuLocator::fixed(None), tz: None, net: None });
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
