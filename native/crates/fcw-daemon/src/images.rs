//! Гостевые образы: manifest.json, список, скачивание tar.zst.

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
}

/// Образ на диске: каталог и описание.
#[derive(Clone, Debug)]
pub struct Image {
    pub dir: PathBuf,
    pub manifest: Manifest,
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

pub fn load(images_dir: &Path, id: &str) -> Result<Image> {
    if !valid_id(id) {
        bail!("invalid image id {id:?}");
    }
    let dir = images_dir.join(id);
    let path = dir.join("manifest.json");
    let data = std::fs::read(&path).with_context(|| format!("image {id}: read {}", path.display()))?;
    let mut manifest: Manifest =
        serde_json::from_slice(&data).with_context(|| format!("image {id}: parse manifest.json"))?;
    if manifest.id.is_empty() {
        manifest.id = id.to_string();
    }
    Ok(Image { dir, manifest })
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
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    if !valid_id(&id) {
        bail!("invalid image id {id:?}");
    }
    tokio::fs::create_dir_all(&images_dir).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("fcw-daemon/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(&url).send().await.context("request")?.error_for_status().context("HTTP")?;
    let total = resp.content_length().unwrap_or(0);
    let tmp = images_dir.join(format!(".fetch-{id}.tar.zst.part"));
    let mut file = tokio::fs::File::create(&tmp).await.with_context(|| format!("create {}", tmp.display()))?;
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
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    progress(done, total.max(done));
    let digest = hex::encode(hasher.finalize());
    if !sha256.is_empty() && !sha256.eq_ignore_ascii_case(&digest) {
        let _ = tokio::fs::remove_file(&tmp).await;
        bail!("sha256 mismatch: expected {sha256}, got {digest}");
    }
    let tmp2 = tmp.clone();
    let r = tokio::task::spawn_blocking(move || extract(&tmp2, &images_dir, &id)).await.map_err(|e| anyhow!(e))?;
    let _ = tokio::fs::remove_file(&tmp).await;
    r
}

/// Распаковка tar.zst в `images_dir/<id>` через промежуточный каталог.
pub fn extract(archive: &Path, images_dir: &Path, id: &str) -> Result<()> {
    let staging = images_dir.join(format!(".extract-{id}"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let res = (|| -> Result<()> {
        let f = std::fs::File::open(archive)?;
        let mut dec = zstd::stream::read::Decoder::new(f)?;
        dec.window_log_max(31)?;
        let mut ar = tar::Archive::new(dec);
        ar.set_preserve_permissions(true);
        for entry in ar.entries()? {
            let mut e = entry?;
            let path = e.path()?.into_owned();
            let first = path.components().next().map(|c| c.as_os_str().to_string_lossy().to_string());
            if first.as_deref() != Some(id) {
                tracing::warn!("fetch {id}: skipping entry outside {id}/: {}", path.display());
                continue;
            }
            if !e.unpack_in(&staging)? {
                bail!("unsafe path in archive: {}", path.display());
            }
        }
        let img = load(&staging, id).context("archive has no valid manifest")?;
        if !img.installed() {
            bail!("archive is missing kernel/initrd/rootfs");
        }
        let dst = images_dir.join(id);
        let old = images_dir.join(format!(".old-{id}"));
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
