//! Поиск QEMU (CONTRACT.md §2) и переносимый QEMU в `--qemu-dir`.
//!
//! QEMU ищется заново при каждом запуске машины и при каждом Hello, чтобы
//! скачанный через FetchQemu QEMU подхватывался без перезапуска демона.
//! Порядок: `--qemu`, `$FCW_QEMU`, `<qemu-dir>/bin/qemu-system-x86_64`,
//! `qemu-system-x86_64` в PATH. Версия (`--version`) и ускоритель кешируются
//! по пути, времени изменения и размеру файла.

use crate::accel::{self, Accel};
use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const QEMU_BIN: &str = "qemu-system-x86_64";
pub const QEMU_IMG_BIN: &str = "qemu-img";
/// image_id в событиях ImageProgress для FetchQemu.
pub const FETCH_ID: &str = "qemu";

pub fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Найденный QEMU.
#[derive(Clone, Debug)]
pub struct QemuInfo {
    pub qemu: PathBuf,
    pub qemu_img: Option<PathBuf>,
    /// Переносимый QEMU из `--qemu-dir`.
    pub bundled: bool,
    /// Каталог данных для `-L` (BIOS, option ROM, keymaps) у переносимого QEMU.
    pub data_dir: Option<PathBuf>,
    pub version: Option<(u32, u32, u32)>,
    pub version_str: String,
    pub accel: Accel,
}

type FileKey = (PathBuf, Option<SystemTime>, u64);

/// Поиск QEMU с кешем версий и ускорителей.
pub struct QemuLocator {
    /// `--qemu`
    pub arg: Option<PathBuf>,
    /// `$FCW_QEMU` (снимается при запуске демона)
    pub env_qemu: Option<PathBuf>,
    /// `--qemu-dir`
    pub dir: PathBuf,
    /// Каталоги PATH (и стандартные места установки QEMU)
    pub path_dirs: Vec<PathBuf>,
    /// `$FCW_QEMU_IMG`
    pub env_qemu_img: Option<PathBuf>,
    /// `--accel`; `None` — auto
    pub accel_req: Option<Accel>,
    /// Искать qemu-system-x86_64 в PATH (qemu-img ищется там всегда).
    pub search_path: bool,
    versions: Mutex<HashMap<FileKey, Option<(u32, u32, u32)>>>,
    accels: Mutex<HashMap<FileKey, Accel>>,
}

impl std::fmt::Debug for QemuLocator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QemuLocator")
            .field("arg", &self.arg)
            .field("env_qemu", &self.env_qemu)
            .field("dir", &self.dir)
            .field("accel_req", &self.accel_req)
            .finish()
    }
}

fn file_key(p: &Path) -> Option<FileKey> {
    let m = std::fs::metadata(p).ok()?;
    m.is_file().then(|| (p.to_path_buf(), m.modified().ok(), m.len()))
}

/// Каталоги PATH плюс стандартные места установки QEMU.
pub fn system_path_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if cfg!(windows) {
        dirs.push(PathBuf::from(r"C:\Program Files\qemu"));
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let rest = s.split("version").nth(1)?.trim_start();
    let ver: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut it = ver.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

impl QemuLocator {
    pub fn new(
        arg: Option<PathBuf>,
        env_qemu: Option<PathBuf>,
        dir: PathBuf,
        path_dirs: Vec<PathBuf>,
        accel_req: Option<Accel>,
    ) -> Self {
        QemuLocator {
            arg,
            env_qemu,
            dir,
            path_dirs,
            env_qemu_img: None,
            accel_req,
            search_path: true,
            versions: Mutex::new(HashMap::new()),
            accels: Mutex::new(HashMap::new()),
        }
    }

    /// Из настроек процесса: `$FCW_QEMU`, `$FCW_QEMU_IMG`, PATH.
    pub fn from_env(arg: Option<PathBuf>, dir: PathBuf, accel_req: Option<Accel>) -> Self {
        let var = |n: &str| std::env::var_os(n).filter(|v| !v.is_empty()).map(PathBuf::from);
        let mut l = Self::new(arg, var("FCW_QEMU"), dir, system_path_dirs(), accel_req);
        l.env_qemu_img = var("FCW_QEMU_IMG");
        l
    }

    /// Только явно указанный QEMU (для тестов); qemu-img — рядом или из PATH.
    pub fn fixed(qemu: Option<PathBuf>) -> Self {
        let mut l = Self::new(qemu, None, PathBuf::from("/nonexistent/fcw-qemu"), system_path_dirs(), Some(Accel::Tcg));
        l.search_path = false;
        l
    }

    pub fn bundled_bin(&self) -> PathBuf {
        self.dir.join("bin").join(exe(QEMU_BIN))
    }

    /// Путь к QEMU по порядку поиска (без запуска).
    pub fn locate(&self) -> Option<(PathBuf, bool)> {
        let bundled = self.bundled_bin();
        let candidates =
            [self.arg.clone().map(|p| (p, false)), self.env_qemu.clone().map(|p| (p, false)), Some((bundled, true))];
        for (p, is_bundled) in candidates.into_iter().flatten() {
            if p.is_file() {
                return Some((p, is_bundled));
            }
            tracing::debug!("qemu candidate {} not found", p.display());
        }
        if !self.search_path {
            return None;
        }
        self.path_dirs.iter().map(|d| d.join(exe(QEMU_BIN))).find(|p| p.is_file()).map(|p| (p, false))
    }

    fn locate_img(&self, qemu: &Path) -> Option<PathBuf> {
        if let Some(dir) = qemu.parent() {
            let p = dir.join(exe(QEMU_IMG_BIN));
            if p.is_file() {
                return Some(p);
            }
        }
        if let Some(p) = self.env_qemu_img.clone().filter(|p| p.is_file()) {
            return Some(p);
        }
        self.path_dirs.iter().map(|d| d.join(exe(QEMU_IMG_BIN))).find(|p| p.is_file())
    }

    async fn version_of(&self, qemu: &Path) -> Option<(u32, u32, u32)> {
        let key = file_key(qemu)?;
        if let Some(v) = self.versions.lock().get(&key) {
            return *v;
        }
        let out = tokio::process::Command::new(qemu).arg("--version").kill_on_drop(true).output();
        let v = match tokio::time::timeout(std::time::Duration::from_secs(10), out).await {
            Ok(Ok(o)) => parse_version(&String::from_utf8_lossy(&o.stdout)),
            _ => None,
        };
        if v.is_none() {
            tracing::warn!("cannot get version of {}", qemu.display());
        }
        self.versions.lock().insert(key, v);
        v
    }

    /// Ускоритель для этого QEMU: явный `--accel` или результат пробы (кеш).
    pub async fn accel_for(&self, qemu: Option<&Path>) -> Accel {
        if let Some(a) = self.accel_req {
            return a;
        }
        let Some(key) = qemu.and_then(file_key) else {
            return if accel::kvm_usable() { Accel::Kvm } else { Accel::Tcg };
        };
        if let Some(a) = self.accels.lock().get(&key) {
            return *a;
        }
        let a = accel::detect(&key.0).await;
        self.accels.lock().insert(key, a);
        a
    }

    /// Найти QEMU, qemu-img, версию и ускоритель. `None` — QEMU нет.
    pub async fn find(&self) -> Option<QemuInfo> {
        let (qemu, bundled) = self.locate()?;
        let qemu_img = self.locate_img(&qemu);
        let version = self.version_of(&qemu).await;
        let version_str = version.map(|(a, b, c)| format!("{a}.{b}.{c}")).unwrap_or_default();
        let data_dir = if bundled {
            let d = self.dir.join("share").join("qemu");
            d.is_dir().then_some(d)
        } else {
            None
        };
        let accel = self.accel_for(Some(&qemu)).await;
        Some(QemuInfo { qemu, qemu_img, bundled, data_dir, version, version_str, accel })
    }
}

/// Распаковывает архив переносимого QEMU (каталог `qemu/` внутри) и атомарно
/// заменяет им `qemu_dir`. Файлам в `bin/` выставляются права на исполнение.
pub fn install(archive: &Path, qemu_dir: &Path) -> Result<()> {
    let parent = qemu_dir.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = qemu_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "qemu".into());
    let staging = parent.join(format!(".extract-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let res = (|| -> Result<()> {
        crate::images::unpack_tar_zst(archive, &staging, "qemu")?;
        let new = staging.join("qemu");
        let bin = new.join("bin");
        if !bin.join(exe(QEMU_BIN)).is_file() {
            bail!("archive has no qemu/bin/{}", exe(QEMU_BIN));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for e in std::fs::read_dir(&bin)?.flatten() {
                if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(0o755))?;
                }
            }
        }
        let old = parent.join(format!(".old-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&old);
        if qemu_dir.exists() {
            std::fs::rename(qemu_dir, &old).with_context(|| format!("move {} aside", qemu_dir.display()))?;
        }
        if let Err(e) = std::fs::rename(&new, qemu_dir) {
            // вернуть старый QEMU на место
            let _ = std::fs::rename(&old, qemu_dir);
            return Err(e).with_context(|| format!("install {}", qemu_dir.display()));
        }
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    res
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(path: &Path, version: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\necho 'QEMU emulator version {version} (test)'\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fcw-qemu-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn search_order_and_version_cache() {
        let d = tmp("order");
        let arg = d.join("arg/qemu-system-x86_64");
        let envq = d.join("env/qemu-system-x86_64");
        let qdir = d.join("qemu");
        let pathdir = d.join("path");
        script(&pathdir.join("qemu-system-x86_64"), "1.0.0");
        script(&pathdir.join("qemu-img"), "1.0.0");
        let l = |a: Option<PathBuf>, e: Option<PathBuf>| {
            QemuLocator::new(a, e, qdir.clone(), vec![pathdir.clone()], Some(Accel::Tcg))
        };

        // только PATH
        let q = l(None, None).find().await.unwrap();
        assert_eq!(q.qemu, pathdir.join("qemu-system-x86_64"));
        assert!(!q.bundled && q.data_dir.is_none());
        assert_eq!(q.qemu_img.as_deref(), Some(pathdir.join("qemu-img").as_path()));
        assert_eq!(q.version_str, "1.0.0");

        // переносимый QEMU важнее PATH; qemu-img — рядом с ним, -L на share/qemu
        script(&qdir.join("bin/qemu-system-x86_64"), "11.1.3");
        script(&qdir.join("bin/qemu-img"), "11.1.3");
        std::fs::create_dir_all(qdir.join("share/qemu")).unwrap();
        let q = l(None, None).find().await.unwrap();
        assert!(q.bundled);
        assert_eq!(q.qemu, qdir.join("bin/qemu-system-x86_64"));
        assert_eq!(q.qemu_img.as_deref(), Some(qdir.join("bin/qemu-img").as_path()));
        assert_eq!(q.data_dir.as_deref(), Some(qdir.join("share/qemu").as_path()));
        assert_eq!(q.version, Some((11, 1, 3)));

        // $FCW_QEMU важнее переносимого, --qemu важнее всех; несуществующие пропускаются
        script(&envq, "9.0.0");
        assert_eq!(l(None, Some(envq.clone())).find().await.unwrap().qemu, envq);
        assert_eq!(l(Some(d.join("missing")), Some(envq.clone())).find().await.unwrap().qemu, envq);
        script(&arg, "10.0.0");
        let q = l(Some(arg.clone()), Some(envq.clone())).find().await.unwrap();
        assert_eq!((q.qemu.clone(), q.bundled), (arg.clone(), false));
        // qemu-img не рядом с --qemu -> из PATH
        assert_eq!(q.qemu_img.as_deref(), Some(pathdir.join("qemu-img").as_path()));

        // кеш версии по пути и mtime/размеру: замена файла даёт новую версию
        let loc = l(Some(arg.clone()), None);
        assert_eq!(loc.find().await.unwrap().version_str, "10.0.0");
        std::thread::sleep(std::time::Duration::from_millis(20));
        script(&arg, "10.2.1-longer");
        assert_eq!(loc.find().await.unwrap().version_str, "10.2.1");

        // ничего нет
        let none = QemuLocator::new(None, None, d.join("nope"), vec![d.join("empty")], None);
        assert!(none.find().await.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_archive() {
        let d = tmp("install");
        // архив: qemu/bin/* без права на исполнение
        let src = d.join("src/qemu");
        std::fs::create_dir_all(src.join("bin")).unwrap();
        std::fs::create_dir_all(src.join("share/qemu")).unwrap();
        for f in ["bin/qemu-system-x86_64", "bin/qemu-img"] {
            std::fs::write(src.join(f), b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(src.join(f), std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        std::fs::write(src.join("share/qemu/bios-256k.bin"), b"bios").unwrap();
        std::fs::write(src.join("VERSION"), b"11.1.3\n").unwrap();
        let archive = d.join("fcw-qemu.tar.zst");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let enc = zstd::stream::write::Encoder::new(f, 3).unwrap().auto_finish();
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("qemu", &src).unwrap();
            b.finish().unwrap();
        }
        let qdir = d.join("fcw/qemu");
        std::fs::create_dir_all(&qdir).unwrap();
        std::fs::write(qdir.join("old-file"), b"old").unwrap();
        install(&archive, &qdir).unwrap();
        assert!(!qdir.join("old-file").exists(), "old qemu-dir replaced");
        let mode = std::fs::metadata(qdir.join("bin/qemu-system-x86_64")).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "bin/ is executable");
        assert_eq!(std::fs::read(qdir.join("share/qemu/bios-256k.bin")).unwrap(), b"bios");
        // во временных каталогах ничего не осталось
        let left: Vec<_> = std::fs::read_dir(d.join("fcw"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with('.'))
            .collect();
        assert!(left.is_empty(), "{left:?}");

        // архив без qemu/bin/qemu-system-x86_64 отклоняется, старый QEMU цел
        let bad_src = d.join("bad/qemu");
        std::fs::create_dir_all(&bad_src).unwrap();
        std::fs::write(bad_src.join("VERSION"), b"x").unwrap();
        let bad = d.join("bad.tar.zst");
        {
            let f = std::fs::File::create(&bad).unwrap();
            let enc = zstd::stream::write::Encoder::new(f, 3).unwrap().auto_finish();
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("qemu", &bad_src).unwrap();
            b.finish().unwrap();
        }
        assert!(install(&bad, &qdir).is_err());
        assert!(qdir.join("bin/qemu-system-x86_64").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }
}
