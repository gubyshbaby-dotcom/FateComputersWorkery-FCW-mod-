//! Самообновление агента (CONTRACT.md §7): мод присылает свежий бинарник
//! кусками base64, агент собирает его рядом со своим, сверяет SHA-256,
//! пробует запустить, атомарно подменяет себя и перезапускается.

use crate::hash::{base64_decode, Sha256};
use fcw_tui::json::Json;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Больше этого агент не бывает; защита от заполнения диска.
pub const MAX_SIZE: u64 = 64 << 20;

/// Идущая передача нового бинарника.
pub struct Update {
    target: PathBuf,
    tmp: PathBuf,
    file: File,
    sha256: String,
    size: u64,
    received: u64,
    next: u64,
    hasher: Sha256,
}

/// Куда складывается новый файл: рядом с целью, чтобы `rename` был атомарным.
pub fn tmp_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".new");
    target.with_file_name(name)
}

impl Update {
    /// `{"type":"update_begin","size":N,"sha256":"<hex>"}`
    pub fn begin(target: &Path, m: &Json) -> Result<Update, String> {
        let size = m.get("size").and_then(Json::as_f64).filter(|v| *v >= 1.0).ok_or("no size")? as u64;
        if size > MAX_SIZE {
            return Err(format!("too large: {size} bytes"));
        }
        let sha256 = m.get("sha256").and_then(Json::as_str).unwrap_or("").to_ascii_lowercase();
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("bad sha256".into());
        }
        let tmp = tmp_path(target);
        let _ = std::fs::remove_file(&tmp);
        let file = File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        Ok(Update {
            target: target.to_path_buf(),
            tmp,
            file,
            sha256,
            size,
            received: 0,
            next: 0,
            hasher: Sha256::new(),
        })
    }

    /// `{"type":"update_chunk","seq":N,"data":"<base64>"}`; возвращает номер следующего куска.
    pub fn chunk(&mut self, m: &Json) -> Result<u64, String> {
        let seq = m.get("seq").and_then(Json::as_f64).ok_or("no seq")? as u64;
        if seq != self.next {
            return Err(format!("chunk {seq} out of order, expected {}", self.next));
        }
        let data = m.get("data").and_then(Json::as_str).and_then(base64_decode).ok_or("bad chunk data")?;
        if self.received + data.len() as u64 > self.size {
            return Err("more data than announced".into());
        }
        self.file.write_all(&data).map_err(|e| format!("{}: {e}", self.tmp.display()))?;
        self.hasher.update(&data);
        self.received += data.len() as u64;
        self.next += 1;
        Ok(self.next)
    }

    /// `{"type":"update_end"}`: проверить и подменить бинарник.
    pub fn finish(self) -> Result<(), String> {
        let Update { target, tmp, file, sha256, size, received, hasher, .. } = self;
        let check = || -> Result<(), String> {
            if received != size {
                return Err(format!("got {received} of {size} bytes"));
            }
            let got = hasher.finish_hex();
            if got != sha256 {
                return Err(format!("sha256 mismatch: {got}"));
            }
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            // Бинарник, который не запускается, на место не ставим: иначе гость останется без агента.
            let ran = std::process::Command::new(&tmp)
                .arg("--version")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            match ran {
                Ok(st) if st.success() => {}
                Ok(st) => return Err(format!("new binary failed its self-test: {st}")),
                Err(e) => return Err(format!("new binary does not start: {e}")),
            }
            std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))?;
            if let Some(dir) = target.parent().and_then(|d| File::open(d).ok()) {
                let _ = dir.sync_all();
            }
            Ok(())
        };
        let r = check();
        if r.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        r
    }

    /// Передача оборвана: убрать недописанный файл.
    pub fn abort(self) {
        let _ = std::fs::remove_file(&self.tmp);
    }
}
