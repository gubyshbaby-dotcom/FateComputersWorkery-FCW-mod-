//! Гостевые образы: manifest.json, список, скачивание tar.zst и хранилище
//! версий `<images-dir>/.store/<id>/<version>/`.
//!
//! Диск машины — qcow2-слой поверх базы. Чтобы обновление образа
//! (FetchImage или ручная замена `images/<id>`) не ломало уже созданные
//! машины, база копируется в хранилище версий, и машина закрепляется за своей
//! версией: слой ссылается на rootfs из хранилища, ядро и initrd при запуске
//! берутся оттуда же.

use crate::pb;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub kernel_version: String,
    pub kernel: String,
    pub initrd: String,
    pub rootfs: String,
    pub cmdline: String,
    pub min_memory_mb: u32,
    pub recommended_memory_mb: u32,
    pub desktop: String,
    pub terminal: String,
    pub user: String,
    pub password: String,
    pub rootfs_used_bytes: u64,
    pub accent: String,
    /// Уникальный номер сборки (дата-время + хэш). Если есть, это и есть версия.
    pub build_id: String,
}

/// Образ на диске: каталог и описание.
#[derive(Clone, Debug)]
pub struct Image {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// Ключ версии: build_id, иначе version, иначе 16 hex SHA-256 manifest.json
    /// (годится как имя каталога).
    pub version: String,
}

impl Image {
    fn file(&self, name: &str, default: &str) -> PathBuf {
        self.dir.join(if name.is_empty() { default } else { name })
    }
    pub fn kernel(&self) -> PathBuf {
        self.file(&self.manifest.kernel, "vmlinuz")
    }
    pub fn initrd(&self) -> PathBuf {
        self.file(&self.manifest.initrd, "initrd.img")
    }
    pub fn rootfs(&self) -> PathBuf {
        self.file(&self.manifest.rootfs, "rootfs.qcow2")
    }
    pub fn installed(&self) -> bool {
        self.kernel().is_file() && self.initrd().is_file() && self.rootfs().is_file()
    }
    pub fn info(&self) -> pb::ImageInfo {
        let m = &self.manifest;
        pb::ImageInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            desktop: m.desktop.clone(),
            terminal: m.terminal.clone(),
            min_memory_mb: m.min_memory_mb,
            recommended_memory_mb: m.recommended_memory_mb,
            installed: self.installed(),
            image_version: self.version.clone(),
        }
    }
}

/// Идентификатор образа или машины: буквы, цифры, `-`, `_`, `.` (не в начале).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Ключ версии образа: `build_id`, иначе `version`, иначе первые 16 hex
/// SHA-256 от manifest.json. Символы вне `[A-Za-z0-9._-]` заменяются на `_`.
pub fn version_key(manifest: &Manifest, manifest_bytes: &[u8]) -> String {
    let raw = if !manifest.build_id.trim().is_empty() {
        manifest.build_id.trim().to_string()
    } else if !manifest.version.trim().is_empty() {
        manifest.version.trim().to_string()
    } else {
        hex::encode(Sha256::digest(manifest_bytes))[..16].to_string()
    };
    let mut key: String = raw
        .chars()
        .take(100)
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    if key.starts_with('.') {
        key.insert(0, 'v');
    }
    key
}

fn load_dir(dir: PathBuf, id: &str) -> Result<Image> {
    let path = dir.join("manifest.json");
    let data = std::fs::read(&path).with_context(|| format!("image {id}: read {}", path.display()))?;
    let mut manifest: Manifest =
        serde_json::from_slice(&data).with_context(|| format!("image {id}: parse manifest.json"))?;
    if manifest.id.is_empty() {
        manifest.id = id.to_string();
    }
    let version = version_key(&manifest, &data);
    Ok(Image { dir, manifest, version })
}

/// Текущий (установленный) образ `images_dir/<id>`.
pub fn load(images_dir: &Path, id: &str) -> Result<Image> {
    if !valid_id(id) {
        bail!("invalid image id {id:?}");
    }
    load_dir(images_dir.join(id), id)
}

// ------------------------------------------------------------ хранилище версий

pub const STORE_DIR: &str = ".store";

/// Копирование в хранилище и замена `images/<id>` при FetchImage не идут
/// одновременно (иначе в хранилище могли бы смешаться файлы двух версий).
static STORE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn store_path(images_dir: &Path, id: &str, version: &str) -> PathBuf {
    images_dir.join(STORE_DIR).join(id).join(version)
}

/// Закреплённая версия образа из хранилища.
pub fn load_stored(images_dir: &Path, id: &str, version: &str) -> Result<Image> {
    if !valid_id(id) || !valid_id(version) {
        bail!("invalid image id/version {id:?}/{version:?}");
    }
    let dir = store_path(images_dir, id, version);
    let img = load_dir(dir, id)?;
    if !img.installed() {
        bail!("stored image {id} {version} is incomplete");
    }
    Ok(Image { version: version.to_string(), ..img })
}

/// Имя файла образа относительно его каталога (без выхода наружу).
fn rel_name(name: &str, default: &str) -> Result<PathBuf> {
    let p = PathBuf::from(if name.is_empty() { default } else { name });
    if p.is_absolute() || p.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        bail!("bad file name in manifest: {name:?}");
    }
    Ok(p)
}

/// Гарантирует, что текущий образ `images_dir/<id>` есть в хранилище, и
/// возвращает его копию оттуда. Файлы копируются (не жёсткие ссылки) во
/// временный каталог, который затем атомарно переименовывается.
pub fn ensure_stored(images_dir: &Path, id: &str) -> Result<Image> {
    let _g = STORE_LOCK.lock();
    let cur = load(images_dir, id)?;
    if !cur.installed() {
        bail!("image {id} is not installed (kernel/initrd/rootfs missing)");
    }
    let dst = store_path(images_dir, id, &cur.version);
    if let Ok(img) = load_stored(images_dir, id, &cur.version) {
        return Ok(img);
    }
    let parent = dst.parent().expect("store dir has parent").to_path_buf();
    std::fs::create_dir_all(&parent).with_context(|| format!("create {}", parent.display()))?;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = parent.join(format!(".tmp-{}-{}-{nonce}", cur.version, std::process::id()));
    let res = (|| -> Result<()> {
        std::fs::create_dir_all(&tmp)?;
        let m = &cur.manifest;
        let files = [
            (PathBuf::from("manifest.json"), cur.dir.join("manifest.json")),
            (rel_name(&m.kernel, "vmlinuz")?, cur.kernel()),
            (rel_name(&m.initrd, "initrd.img")?, cur.initrd()),
            (rel_name(&m.rootfs, "rootfs.qcow2")?, cur.rootfs()),
        ];
        for (name, src) in files {
            let to = tmp.join(&name);
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&src, &to).with_context(|| format!("copy {} -> {}", src.display(), to.display()))?;
        }
        // версия считается по скопированному manifest.json
        let copied = load_dir(tmp.clone(), id)?;
        if copied.version != cur.version {
            bail!("image {id} changed while copying to the store; try again");
        }
        if dst.exists() {
            // неполная копия от прежнего сбоя
            std::fs::remove_dir_all(&dst)?;
        }
        std::fs::rename(&tmp, &dst).with_context(|| format!("rename to {}", dst.display()))?;
        Ok(())
    })();
    if res.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    res?;
    tracing::info!("image {id} version {} saved to {}", cur.version, dst.display());
    load_stored(images_dir, id, &cur.version)
}

pub fn list(images_dir: &Path) -> Vec<Image> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(images_dir) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_id(&name) || !e.path().join("manifest.json").is_file() {
            continue;
        }
        match load(images_dir, &name) {
            Ok(img) => out.push(img),
            Err(err) => tracing::warn!("{err:#}"),
        }
    }
    out.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    out
}

/// Скачивает `url` (tar.zst с каталогом `<id>/` внутри) и распаковывает в
/// `images_dir/<id>`. `progress(done, total)` вызывается по ходу скачивания.
pub async fn fetch(
    images_dir: PathBuf,
    id: String,
    url: String,
    sha256: String,
    progress: impl Fn(u64, u64) + Send,
) -> Result<()> {
    if !valid_id(&id) {
        bail!("invalid image id {id:?}");
    }
    tokio::fs::create_dir_all(&images_dir).await?;
    let tmp = images_dir.join(format!(".fetch-{id}.tar.zst.part"));
    download(&url, &sha256, &tmp, progress).await?;
    let tmp2 = tmp.clone();
    let r = tokio::task::spawn_blocking(move || extract(&tmp2, &images_dir, &id)).await.map_err(|e| anyhow!(e))?;
    let _ = tokio::fs::remove_file(&tmp).await;
    r
}

/// Скачивает `url` в файл `tmp` (HTTPS_PROXY учитывается), проверяет SHA-256,
/// если он задан. `progress(done, total)` — по ходу скачивания.
pub async fn download(url: &str, sha256: &str, tmp: &Path, progress: impl Fn(u64, u64) + Send) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("fcw-daemon/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(url).send().await.context("request")?.error_for_status().context("HTTP")?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(tmp).await.with_context(|| format!("create {}", tmp.display()))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last = std::time::Instant::now();
    let mut stream = resp.bytes_stream();
    progress(0, total);
    let res: Result<()> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("download")?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            if last.elapsed() >= std::time::Duration::from_millis(250) {
                last = std::time::Instant::now();
                progress(done, total);
            }
        }
        file.flush().await?;
        Ok(())
    }
    .await;
    drop(file);
    if let Err(e) = res {
        let _ = tokio::fs::remove_file(tmp).await;
        return Err(e);
    }
    progress(done, total.max(done));
    let digest = hex::encode(hasher.finalize());
    if !sha256.is_empty() && !sha256.trim().eq_ignore_ascii_case(&digest) {
        let _ = tokio::fs::remove_file(tmp).await;
        bail!("sha256 mismatch: expected {sha256}, got {digest}");
    }
    Ok(())
}

/// Распаковывает tar.zst в `staging`, беря только записи внутри `top/`
/// (остальные пропускаются, небезопасные пути — ошибка).
pub fn unpack_tar_zst(archive: &Path, staging: &Path, top: &str) -> Result<()> {
    let f = std::fs::File::open(archive)?;
    let mut dec = zstd::stream::read::Decoder::new(f)?;
    dec.window_log_max(31)?;
    let mut ar = tar::Archive::new(dec);
    ar.set_preserve_permissions(true);
    for entry in ar.entries()? {
        let mut e = entry?;
        let path = e.path()?.into_owned();
        let first = path.components().next().map(|c| c.as_os_str().to_string_lossy().to_string());
        if first.as_deref() != Some(top) {
            tracing::warn!("unpack: skipping entry outside {top}/: {}", path.display());
            continue;
        }
        if !e.unpack_in(staging)? {
            bail!("unsafe path in archive: {}", path.display());
        }
    }
    Ok(())
}

/// Распаковка tar.zst в `images_dir/<id>` через промежуточный каталог.
pub fn extract(archive: &Path, images_dir: &Path, id: &str) -> Result<()> {
    let staging = images_dir.join(format!(".extract-{id}"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let res = (|| -> Result<()> {
        unpack_tar_zst(archive, &staging, id)?;
        let img = load(&staging, id).context("archive has no valid manifest")?;
        if !img.installed() {
            bail!("archive is missing kernel/initrd/rootfs");
        }
        let dst = images_dir.join(id);
        let old = images_dir.join(format!(".old-{id}"));
        let _g = STORE_LOCK.lock();
        let _ = std::fs::remove_dir_all(&old);
        if dst.exists() {
            std::fs::rename(&dst, &old).context("move old image aside")?;
        }
        std::fs::rename(staging.join(id), &dst).context("install image")?;
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(valid_id("fedora"));
        assert!(valid_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(".hidden"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(""));
    }

    fn write_image(dir: &Path, manifest: &str, rootfs: &[u8]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(dir.join("vmlinuz"), b"kernel").unwrap();
        std::fs::write(dir.join("initrd.img"), b"initrd").unwrap();
        std::fs::write(dir.join("rootfs.qcow2"), rootfs).unwrap();
    }

    #[test]
    fn version_keys() {
        let m = |s: &str| -> (Manifest, Vec<u8>) { (serde_json::from_str(s).unwrap(), s.as_bytes().to_vec()) };
        let (a, ab) = m(r#"{"version":"2026.11.17","build_id":"20261117T1022-ab12cd"}"#);
        assert_eq!(version_key(&a, &ab), "20261117T1022-ab12cd");
        let (b, bb) = m(r#"{"version":"2026.11.17"}"#);
        assert_eq!(version_key(&b, &bb), "2026.11.17");
        let (c, cb) = m(r#"{"name":"x"}"#);
        let k = version_key(&c, &cb);
        assert_eq!(k.len(), 16);
        assert!(k.chars().all(|ch| ch.is_ascii_hexdigit()));
        let (d, db) = m(r#"{"build_id":"2026-11-17 10:22:00/ab"}"#);
        assert_eq!(version_key(&d, &db), "2026-11-17_10_22_00_ab");
        let (e, eb) = m(r#"{"version":".hidden"}"#);
        assert!(valid_id(&version_key(&e, &eb)));
    }

    #[test]
    fn store_copies_versions() {
        let base = std::env::temp_dir().join(format!("fcw-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let images = base.join("images");
        write_image(&images.join("alp"), r#"{"id":"alp","version":"1","build_id":"b1"}"#, b"rootfs-v1");

        let s1 = ensure_stored(&images, "alp").unwrap();
        assert_eq!(s1.version, "b1");
        assert_eq!(s1.dir, store_path(&images, "alp", "b1"));
        assert_eq!(std::fs::read(s1.rootfs()).unwrap(), b"rootfs-v1");
        // повторный вызов — та же копия
        assert_eq!(ensure_stored(&images, "alp").unwrap().dir, s1.dir);
        // перезапись файла на месте не задевает хранилище (это копия, не жёсткая ссылка)
        std::fs::write(images.join("alp/rootfs.qcow2"), b"overwritten").unwrap();
        assert_eq!(std::fs::read(s1.rootfs()).unwrap(), b"rootfs-v1");

        // новая сборка с тем же version, но другим build_id — отдельная версия
        std::fs::remove_dir_all(images.join("alp")).unwrap();
        write_image(&images.join("alp"), r#"{"id":"alp","version":"1","build_id":"b2"}"#, b"rootfs-v2");
        let s2 = ensure_stored(&images, "alp").unwrap();
        assert_eq!(s2.version, "b2");
        assert_eq!(std::fs::read(s2.rootfs()).unwrap(), b"rootfs-v2");
        assert_eq!(load_stored(&images, "alp", "b1").unwrap().version, "b1");
        assert!(load_stored(&images, "alp", "b3").is_err());

        // .store не виден как образ, во временных каталогах ничего не осталось
        let ids: Vec<String> = list(&images).iter().map(|i| i.manifest.id.clone()).collect();
        assert_eq!(ids, vec!["alp"]);
        let leftovers: Vec<_> = std::fs::read_dir(images.join(".store/alp"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn extract_tar_zst() {
        let base = std::env::temp_dir().join(format!("fcw-img-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src/alp");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("manifest.json"), br#"{"format":1,"id":"alp","name":"Alpine"}"#).unwrap();
        for f in ["vmlinuz", "initrd.img", "rootfs.qcow2"] {
            std::fs::write(src.join(f), b"x").unwrap();
        }
        let archive = base.join("a.tar.zst");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let enc = zstd::stream::write::Encoder::new(f, 3).unwrap().auto_finish();
            let mut b = tar::Builder::new(enc);
            b.append_dir_all("alp", &src).unwrap();
            b.finish().unwrap();
        }
        let images = base.join("images");
        std::fs::create_dir_all(&images).unwrap();
        extract(&archive, &images, "alp").unwrap();
        let img = load(&images, "alp").unwrap();
        assert!(img.installed());
        assert_eq!(img.manifest.name, "Alpine");
        assert_eq!(list(&images).len(), 1);
        // повторная установка поверх старой
        extract(&archive, &images, "alp").unwrap();
        assert!(load(&images, "alp").unwrap().installed());
        let _ = std::fs::remove_dir_all(&base);
    }
}
