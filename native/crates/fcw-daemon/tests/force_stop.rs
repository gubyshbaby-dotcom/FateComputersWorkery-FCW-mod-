//! StopVm(force=true) во время мягкой остановки убивает QEMU сразу, не дожидаясь
//! её таймаута. Вместо QEMU — сценарий на Python, который отвечает по QMP и
//! игнорирует system_powerdown (гость «завис»). Нужны python3 и qemu-img;
//! если их нет, тест пропускается.

#![cfg(unix)]

use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::vm::Env;
use fcw_daemon::{control, pb};
use fcw_proto::control::{envelope, request, response};
use fcw_proto::frame;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const FAKE_QEMU: &str = r#"#!/usr/bin/env python3
# Поддельный QEMU: QMP на сокете из -qmp unix:<path>,...; powerdown игнорирует.
import json, os, socket, sys, time
args = sys.argv[1:]
path = args[args.index('-qmp') + 1].split(':', 1)[1].split(',')[0]
srv = socket.socket(socket.AF_UNIX)
srv.bind(path)
srv.listen(1)
c, _ = srv.accept()
c.sendall(b'{"QMP": {"version": {}, "capabilities": []}}\n')
for line in c.makefile('rb'):
    m = json.loads(line)
    r = {"return": {}}
    if 'id' in m:
        r['id'] = m['id']
    c.sendall((json.dumps(r) + '\n').encode())
    cmd = m.get('execute')
    if cmd == 'cont':
        c.sendall(b'{"event": "RESUME", "data": {}}\n')
    if cmd == 'quit':
        sys.exit(0)
time.sleep(3600)
"#;

fn find(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
}

async fn send(s: &mut tokio::net::UnixStream, id: u64, kind: request::Kind) {
    let env = pb::Envelope { id, body: Some(envelope::Body::Request(pb::Request { kind: Some(kind) })) };
    frame::write_async(s, &env).await.unwrap();
}

/// Ждёт ответы с указанными id; возвращает их и время прихода.
async fn responses(s: &mut tokio::net::UnixStream, ids: &[u64], t0: Instant) -> HashMap<u64, (pb::Response, Duration)> {
    let mut out = HashMap::new();
    while out.len() < ids.len() {
        let e: pb::Envelope = tokio::time::timeout(Duration::from_secs(30), frame::read_async(s))
            .await
            .expect("response timeout")
            .unwrap()
            .unwrap();
        if let Some(envelope::Body::Response(r)) = e.body {
            if ids.contains(&e.id) {
                out.insert(e.id, (r, t0.elapsed()));
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn force_stop_interrupts_soft_stop() {
    let (Some(_py), Some(qemu_img)) = (find("python3"), find("qemu-img")) else {
        eprintln!("python3 or qemu-img not found, skipping");
        return;
    };
    let dir = std::env::temp_dir().join(format!("fcw-force-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let img = dir.join("images/alp");
    std::fs::create_dir_all(&img).unwrap();
    std::fs::write(img.join("manifest.json"), r#"{"id":"alp","build_id":"b1"}"#).unwrap();
    std::fs::write(img.join("vmlinuz"), b"k").unwrap();
    std::fs::write(img.join("initrd.img"), b"i").unwrap();
    assert!(Command::new(&qemu_img)
        .args(["create", "-q", "-f", "qcow2"])
        .arg(img.join("rootfs.qcow2"))
        .arg("16M")
        .status()
        .unwrap()
        .success());
    let fake = dir.join("fake-qemu");
    std::fs::write(&fake, FAKE_QEMU).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let env = Arc::new(Env {
        images_dir: dir.join("images"),
        qemu: QemuLocator::fixed(Some(fake)),
        tz: None,
        net: None,
        screen: Default::default(),
    });
    let d = Daemon::new(env, dir.join("vms"), 8, 1, "00".into(), broadcast::channel(64).0);
    let sock = dir.join("control.sock");
    tokio::spawn(control::serve(d.clone(), LocalListener::bind(&sock).unwrap()));
    let mut s = tokio::net::UnixStream::connect(&sock).await.unwrap();

    let t0 = Instant::now();
    send(
        &mut s,
        1,
        request::Kind::CreateVm(pb::CreateVm { vm_id: "m".into(), image_id: "alp".into(), ..Default::default() }),
    )
    .await;
    send(&mut s, 2, request::Kind::StartVm(pb::StartVm { vm_id: "m".into() })).await;
    let r = responses(&mut s, &[1, 2], t0).await;
    assert!(r[&2].0.ok, "start: {}", r[&2].0.error);
    let vm = d.vm("m").unwrap();
    assert_eq!(vm.state(), pb::VmState::Running);

    // мягкая остановка с длинным таймаутом: гость не реагирует на ACPI
    let t0 = Instant::now();
    send(&mut s, 3, request::Kind::StopVm(pb::StopVm { vm_id: "m".into(), force: false, timeout_ms: 60_000 })).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(vm.state(), pb::VmState::Stopping);
    send(&mut s, 4, request::Kind::StopVm(pb::StopVm { vm_id: "m".into(), force: true, timeout_ms: 0 })).await;
    let r = responses(&mut s, &[3, 4], t0).await;
    let (force, t_force) = &r[&4];
    let (soft, t_soft) = &r[&3];
    assert!(force.ok, "force: {}", force.error);
    assert!(soft.ok, "soft: {}", soft.error);
    assert!(*t_force < Duration::from_secs(5), "force stop took {t_force:?}");
    assert!(*t_soft < Duration::from_secs(5), "soft stop should end with the kill, took {t_soft:?}");
    let Some(response::Kind::Vm(v)) = &force.kind else { panic!("no VmInfo") };
    assert_eq!(v.state, pb::VmState::Stopped as i32);
    assert_eq!(vm.state(), pb::VmState::Stopped);
    assert!(vm.info().last_error.is_empty(), "force stop is not a crash: {}", vm.info().last_error);

    // принудительная остановка остановленной машины — просто ok
    let t0 = Instant::now();
    send(&mut s, 5, request::Kind::StopVm(pb::StopVm { vm_id: "m".into(), force: true, timeout_ms: 0 })).await;
    assert!(responses(&mut s, &[5], t0).await[&5].0.ok);

    // снова запускается после принудительной остановки
    let t0 = Instant::now();
    send(&mut s, 6, request::Kind::StartVm(pb::StartVm { vm_id: "m".into() })).await;
    assert!(responses(&mut s, &[6], t0).await[&6].0.ok);
    assert_eq!(vm.state(), pb::VmState::Running);
    send(&mut s, 7, request::Kind::DeleteVm(pb::DeleteVm { vm_id: "m".into() })).await;
    let r = responses(&mut s, &[7], t0).await;
    assert!(r[&7].0.ok, "delete: {}", r[&7].0.error);
    assert!(d.vm("m").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
