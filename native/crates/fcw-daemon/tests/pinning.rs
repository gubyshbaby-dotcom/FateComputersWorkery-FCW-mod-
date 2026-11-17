//! Закрепление версии образа за машиной (без настоящей ВМ: вместо QEMU
//! запускается /bin/false, нужен только qemu-img). Если qemu-img нет, тест
//! пропускается.

#![cfg(unix)]

use fcw_daemon::accel::Accel;
use fcw_daemon::daemon::Daemon;
use fcw_daemon::localsock::LocalListener;
use fcw_daemon::vm::Env;
use fcw_daemon::{control, pb};
use fcw_proto::control::{envelope, request, response};
use fcw_proto::frame;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::broadcast;

fn find(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
}

fn write_image(dir: &Path, build_id: &str, qemu_img: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(r#"{{"format":1,"id":"alp","name":"Alp","version":"2026.11.17","build_id":"{build_id}","cmdline":"console=hvc0 fcw.test={build_id}"}}"#),
    )
    .unwrap();
    std::fs::write(dir.join("vmlinuz"), format!("kernel {build_id}")).unwrap();
    std::fs::write(dir.join("initrd.img"), format!("initrd {build_id}")).unwrap();
    let st = Command::new(qemu_img)
        .args(["create", "-q", "-f", "qcow2"])
        .arg(dir.join("rootfs.qcow2"))
        .arg("16M")
        .status()
        .unwrap();
    assert!(st.success());
}

/// Путь к базе qcow2-слоя.
fn backing(qemu_img: &Path, disk: &Path) -> String {
    let out = Command::new(qemu_img).args(["info", "-U", "--output=json"]).arg(disk).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["full-backing-filename"].as_str().or(v["backing-filename"].as_str()).unwrap_or("").to_string()
}

struct Ctl {
    s: tokio::net::UnixStream,
    id: u64,
}

impl Ctl {
    async fn call(&mut self, kind: request::Kind) -> pb::Response {
        self.id += 1;
        let env = pb::Envelope { id: self.id, body: Some(envelope::Body::Request(pb::Request { kind: Some(kind) })) };
        frame::write_async(&mut self.s, &env).await.unwrap();
        loop {
            let e: pb::Envelope = frame::read_async(&mut self.s).await.unwrap().unwrap();
            if let Some(envelope::Body::Response(r)) = e.body {
                return r;
            }
        }
    }
    async fn create(&mut self, vm: &str, reinstall: bool) -> pb::Response {
        self.call(request::Kind::CreateVm(pb::CreateVm {
            vm_id: vm.into(),
            image_id: "alp".into(),
            reinstall,
            ..Default::default()
        }))
        .await
    }
    async fn start(&mut self, vm: &str) -> pb::Response {
        self.call(request::Kind::StartVm(pb::StartVm { vm_id: vm.into() })).await
    }
    async fn info(&mut self, vm: &str) -> pb::VmInfo {
        match self.call(request::Kind::GetVm(pb::GetVm { vm_id: vm.into() })).await.kind {
            Some(response::Kind::Vm(v)) => v,
            _ => panic!("GetVm"),
        }
    }
}

/// Подмена `images/alp` целиком (как FetchImage: новый каталог + rename).
fn replace_image(images: &Path, build_id: &str, qemu_img: &Path) {
    let tmp = images.join(".new-alp");
    let _ = std::fs::remove_dir_all(&tmp);
    write_image(&tmp, build_id, qemu_img);
    let old = images.join(".old-alp");
    std::fs::rename(images.join("alp"), &old).unwrap();
    std::fs::rename(&tmp, images.join("alp")).unwrap();
    std::fs::remove_dir_all(&old).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn machines_stay_on_their_image_version() {
    let (Some(qemu_img), Some(fake_qemu)) = (find("qemu-img"), find("false")) else {
        eprintln!("qemu-img or false not found, skipping");
        return;
    };
    let dir = std::env::temp_dir().join(format!("fcw-pin-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let images = dir.join("images");
    write_image(&images.join("alp"), "build-A", &qemu_img);

    let (events, _) = broadcast::channel(256);
    let env = Arc::new(Env {
        images_dir: images.clone(),
        qemu: Some(fake_qemu),
        qemu_img: Some(qemu_img.clone()),
        accel: Accel::Tcg,
        tz: None,
        qemu_version: None,
        net: None,
    });
    let d = Daemon::new(env, dir.join("vms"), 8, 1, "00".into(), events);
    let sock = dir.join("control.sock");
    tokio::spawn(control::serve(d.clone(), LocalListener::bind(&sock).unwrap()));
    let mut c = Ctl { s: tokio::net::UnixStream::connect(&sock).await.unwrap(), id: 0 };

    // старая машина: диск поверх build-A из хранилища
    assert!(c.create("old", false).await.ok);
    assert_eq!(c.info("old").await.image_version, "", "not pinned before the disk exists");
    let r = c.start("old").await;
    assert!(!r.ok && r.error.contains("QEMU exited"), "fake QEMU must fail after disk prep: {}", r.error);
    let old_disk = dir.join("vms/old/disk.qcow2");
    let store_a = images.join(".store/alp/build-A");
    assert!(backing(&qemu_img, &old_disk).starts_with(store_a.to_str().unwrap()), "{}", backing(&qemu_img, &old_disk));
    let cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("vms/old/config.json")).unwrap()).unwrap();
    assert_eq!(cfg["image_version"], "build-A");
    let i = c.info("old").await;
    assert_eq!((i.image_version.as_str(), i.image_update_available), ("build-A", false));

    // образ обновили (тот же version, другой build_id)
    replace_image(&images, "build-B", &qemu_img);
    let i = c.info("old").await;
    assert_eq!((i.image_version.as_str(), i.image_update_available), ("build-A", true));
    // старая машина стартует со своей версией: ядро и cmdline из хранилища A
    let vm = d.vm("old").unwrap();
    let g = vm.op_ticket().acquire().await;
    let (img, _) = vm.prepare_image(&g).await.unwrap();
    drop(g);
    assert_eq!(img.dir, store_a);
    assert_eq!(std::fs::read_to_string(img.kernel()).unwrap(), "kernel build-A");
    assert!(img.manifest.cmdline.contains("fcw.test=build-A"));
    assert!(backing(&qemu_img, &old_disk).starts_with(store_a.to_str().unwrap()));

    // новая машина получает новую версию
    assert!(c.create("new", false).await.ok);
    assert!(!c.start("new").await.ok);
    let i = c.info("new").await;
    assert_eq!((i.image_version.as_str(), i.image_update_available), ("build-B", false));
    assert!(backing(&qemu_img, &dir.join("vms/new/disk.qcow2"))
        .starts_with(images.join(".store/alp/build-B").to_str().unwrap()));

    // ListImages не показывает .store и отдаёт ключ версии
    match c.call(request::Kind::ListImages(pb::ListImages {})).await.kind {
        Some(response::Kind::Images(l)) => {
            assert_eq!(l.images.len(), 1);
            assert_eq!((l.images[0].id.as_str(), l.images[0].image_version.as_str()), ("alp", "build-B"));
        }
        _ => panic!("ListImages"),
    }

    // CreateVm без reinstall сохраняет закрепление
    assert!(c.create("old", false).await.ok);
    assert_eq!(c.info("old").await.image_version, "build-A");

    // закреплённая версия пропала -> понятная ошибка
    std::fs::remove_dir_all(&store_a).unwrap();
    let r = c.start("old").await;
    assert!(!r.ok);
    assert!(r.error.contains("build-A") && r.error.contains("reinstall"), "{}", r.error);

    // переустановка переводит машину на текущую версию
    assert!(c.create("old", true).await.ok);
    assert!(!old_disk.exists());
    assert_eq!(c.info("old").await.image_version, "");
    assert!(!c.start("old").await.ok);
    let i = c.info("old").await;
    assert_eq!((i.image_version.as_str(), i.image_update_available), ("build-B", false));

    // машины без image_version (созданы до закрепления) работают через images/<id>
    let mut cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("vms/new/config.json")).unwrap()).unwrap();
    cfg["image_version"] = "".into();
    std::fs::write(dir.join("vms/new/config.json"), cfg.to_string()).unwrap();
    let d2 = Daemon::new(d.env.clone(), dir.join("vms"), 8, 1, "00".into(), broadcast::channel(8).0);
    let vm = d2.vm("new").unwrap();
    let g = vm.op_ticket().acquire().await;
    let (img, _) = vm.prepare_image(&g).await.unwrap();
    assert_eq!(img.dir, images.join("alp"));
    assert!(!vm.info().image_update_available);

    let _ = std::fs::remove_dir_all(&dir);
}
