//! Секрет канала управления по TCP (`--token-file`).

use anyhow::{bail, Context, Result};
use base64::Engine;
use rand::RngCore;
use std::path::Path;

/// Читает токен из файла; если файла нет — создаёт (32 случайных байта,
/// base64url, права 0600; каталог создаётся). Возвращает токен и признак
/// «создан сейчас».
pub fn load_or_create_token(path: &Path) -> Result<(String, bool)> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let t = s.trim().to_string();
            if t.is_empty() {
                bail!("token file {} is empty", path.display());
            }
            Ok((t, false))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
            }
            let mut raw = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut raw);
            let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
            write_secret(path, format!("{token}\n").as_bytes())?;
            Ok((token, true))
        }
        Err(e) => Err(e).with_context(|| format!("read token file {}", path.display())),
    }
}

fn write_secret(path: &Path, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("create token file {}", path.display()))?;
        f.write_all(data)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, data).with_context(|| format!("create token file {}", path.display()))
    }
}

/// Сравнение за время, не зависящее от совпадающего префикса.
pub fn ct_eq(given: &[u8], expected: &[u8]) -> bool {
    let mut diff = (given.len() ^ expected.len()) as u64;
    for (i, e) in expected.iter().enumerate() {
        let g = given.get(i).copied().unwrap_or(0);
        diff |= (g ^ e) as u64;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_load() {
        let dir = std::env::temp_dir().join(format!("fcw-token-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("etc/fcw/token");
        let (t, created) = load_or_create_token(&path).unwrap();
        assert!(created);
        assert_eq!(t.len(), 43);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let (t2, created2) = load_or_create_token(&path).unwrap();
        assert_eq!((t2, created2), (t, false));
        std::fs::write(&path, "  \n").unwrap();
        assert!(load_or_create_token(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn constant_time_eq() {
        assert!(ct_eq(b"secret", b"secret"));
        assert!(!ct_eq(b"secreT", b"secret"));
        assert!(!ct_eq(b"secret2", b"secret"));
        assert!(!ct_eq(b"secre", b"secret"));
        assert!(!ct_eq(b"", b"secret"));
    }
}
