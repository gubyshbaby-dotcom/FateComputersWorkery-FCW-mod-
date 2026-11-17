//! FetchQemu: демон без QEMU, скачивание переносимого QEMU с локального
//! HTTP-сервера, подхват без перезапуска (Hello и StartVm ищут QEMU заново).

#![cfg(unix)]

use fcw_daemon::accel::Accel;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::vm::Env;
use fcw_daemon::{control, pb};
use fcw_proto::control::{envelope, request, response};
use fcw_proto::frame;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

/// tar.zst с каталогом qemu/: скрипты в bin/ без права на исполнение.
fn archive() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let enc = zstd::stream::write::Encoder::new(&mut out, 3).unwrap().auto_finish();
        let mut b = tar::Builder::new(enc);
        let mut add = |path: &str, data: &[u8]| {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, path, data).unwrap();
        };
        add("qemu/bin/qemu-system-x86_64", b"#!/bin/sh\necho 'QEMU emulator version 11.1.3 (fcw)'\n");
        add("qemu/bin/qemu-img", b"#!/bin/sh\necho 'qemu-img version 11.1.3'\n");
        add("qemu/share/qemu/bios-256k.bin", b"bios");
        add("qemu/VERSION", b"11.1.3\n");
        b.finish().unwrap();
    }
    out
}

/// Минимальный HTTP-сервер: на любой запрос отдаёт `body`.
async fn http_server(body: Vec<u8>) -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let body = Arc::new(body);
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                let mut req = Vec::new();
                let mut buf = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/zstd\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes()).await;
                let _ = s.write_all(&body).await;
                let _ = s.shutdown().await;
            });
        }
    });
    port
}

struct Ctl {
    s: tokio::net::UnixStream,
    id: u64,
    events: Vec<pb::Event>,
}

impl Ctl {
    async fn call(&mut self, kind: request::Kind) -> pb::Response {
        self.id += 1;
        let env = pb::Envelope { id: self.id, body: Some(envelope::Body::Request(pb::Request { kind: Some(kind) })) };
        frame::write_async(&mut self.s, &env).await.unwrap();
        loop {
            let e: pb::Envelope = frame::read_async(&mut self.s).await.unwrap().unwrap();
            match e.body {
                Some(envelope::Body::Response(r)) if e.id == self.id => return r,
                Some(envelope::Body::Event(ev)) => self.events.push(ev),
                _ => {}
            }
        }
    }

    async fn hello(&mut self) -> pb::HelloResult {
        match self.call(request::Kind::Hello(pb::Hello::default())).await.kind {
            Some(response::Kind::Hello(h)) => h,
            _ => panic!("hello"),
        }
    }

    /// Ждёт итоговое событие ImageProgress для `id`; возвращает ошибку (пусто — успех).
    async fn wait_fetch(&mut self, id: &str) -> (String, usize) {
        let mut progress = 0;
        loop {
            let ev = if self.events.is_empty() {
                let e: pb::Envelope = tokio::time::timeout(Duration::from_secs(30), frame::read_async(&mut self.s))
                    .await
                    .expect("fetch timeout")
                    .unwrap()
                    .unwrap();
                match e.body {
                    Some(envelope::Body::Event(ev)) => ev,
                    _ => continue,
                }
            } else {
                self.events.remove(0)
            };
            if let Some(pb::event::Kind::ImageProgress(p)) = ev.kind {
                if p.image_id == id {
                    if p.finished {
                        return (p.error, progress);
                    }
                    progress += 1;
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_qemu_installs_and_is_picked_up() {
    let dir = std::env::temp_dir().join(format!("fcw-fetchqemu-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("images/alp")).unwrap();
    std::fs::write(dir.join("images/alp/manifest.json"), r#"{"id":"alp","build_id":"b1"}"#).unwrap();
    for f in ["vmlinuz", "initrd.img", "rootfs.qcow2"] {
        std::fs::write(dir.join("images/alp").join(f), b"x").unwrap();
    }
    let qemu_dir = dir.join("qemu");
    // PATH пустой: системный QEMU не найдётся
    let locator = QemuLocator::new(None, None, qemu_dir.clone(), vec![dir.join("empty")], Some(Accel::Tcg));
    let env = Arc::new(Env {
        images_dir: dir.join("images"),
        qemu: locator,
        tz: None,
        net: None,
        screen: Default::default(),
    });
    let d = Daemon::new(env, dir.join("vms"), 8, 1, "00".into(), broadcast::channel(256).0);
    let sock = dir.join("control.sock");
    tokio::spawn(control::serve(d.clone(), LocalListener::bind(&sock).unwrap()));
    let mut c = Ctl { s: tokio::net::UnixStream::connect(&sock).await.unwrap(), id: 0, events: Vec::new() };

    // без QEMU демон работает, Hello честно говорит, StartVm — понятная ошибка
    let h = c.hello().await;
    assert!(!h.qemu_available && !h.qemu_bundled && h.qemu_path.is_empty());
    assert!(
        c.call(request::Kind::CreateVm(pb::CreateVm {
            vm_id: "m".into(),
            image_id: "alp".into(),
            ..Default::default()
        }))
        .await
        .ok
    );
    let r = c.call(request::Kind::StartVm(pb::StartVm { vm_id: "m".into() })).await;
    assert!(!r.ok && r.error.contains("QEMU not found"), "{}", r.error);

    let body = archive();
    let sha = hex::encode(Sha256::digest(&body));
    let url = format!("http://127.0.0.1:{}/fcw-qemu-linux-x86_64.tar.zst", http_server(body).await);

    // неверная контрольная сумма: ошибка, qemu-dir не создан
    let r = c.call(request::Kind::FetchQemu(pb::FetchQemu { url: url.clone(), sha256: "00".into() })).await;
    assert!(r.ok, "accepted: {}", r.error);
    let (err, _) = c.wait_fetch("qemu").await;
    assert!(err.contains("sha256 mismatch"), "{err}");
    assert!(!qemu_dir.exists());

    // правильная: ставится, права на исполнение, Hello видит переносимый QEMU
    let r = c.call(request::Kind::FetchQemu(pb::FetchQemu { url: url.clone(), sha256: sha.clone() })).await;
    assert!(r.ok, "{}", r.error);
    let (err, progress) = c.wait_fetch("qemu").await;
    assert!(err.is_empty(), "{err}");
    assert!(progress >= 1, "progress events");
    let bin = qemu_dir.join("bin/qemu-system-x86_64");
    assert_eq!(std::fs::metadata(&bin).unwrap().permissions().mode() & 0o111, 0o111);
    assert_eq!(std::fs::metadata(qemu_dir.join("bin/qemu-img")).unwrap().permissions().mode() & 0o111, 0o111);
    let h = c.hello().await;
    assert!(h.qemu_available && h.qemu_bundled, "{h:?}");
    assert_eq!(h.qemu_version, "11.1.3");
    assert_eq!(Path::new(&h.qemu_path), bin);
    // во временных каталогах рядом ничего не осталось
    let left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(left.is_empty(), "{left:?}");

    // повторная установка поверх (обновление QEMU) тоже проходит
    assert!(c.call(request::Kind::FetchQemu(pb::FetchQemu { url, sha256: sha })).await.ok);
    assert!(c.wait_fetch("qemu").await.0.is_empty());
    assert!(c.hello().await.qemu_available);
    let _ = std::fs::remove_dir_all(&dir);
}
