//! Самоподписанный сертификат QUIC-сервера, хранится на диске.

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct ServerCert {
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
    pub sha256_hex: String,
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Загружает `<dir>/daemon-cert.der` + `<dir>/daemon-key.der` или создаёт новые.
pub fn load_or_create(dir: &Path) -> Result<ServerCert> {
    let cert_path = dir.join("daemon-cert.der");
    let key_path = dir.join("daemon-key.der");
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        if !cert.is_empty() && !key.is_empty() {
            let sha256_hex = sha256_hex(&cert);
            return Ok(ServerCert {
                cert: CertificateDer::from(cert),
                key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
                sha256_hex,
            });
        }
    }
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let ck = rcgen::generate_simple_self_signed(vec!["fcw-daemon".to_string(), "localhost".to_string()])
        .context("generate certificate")?;
    let cert = ck.cert.der().to_vec();
    let key = ck.key_pair.serialize_der();
    write_private(&key_path, &key)?;
    std::fs::write(&cert_path, &cert).with_context(|| format!("write {}", cert_path.display()))?;
    let sha256_hex = sha256_hex(&cert);
    Ok(ServerCert {
        cert: CertificateDer::from(cert),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        sha256_hex,
    })
}

fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("write {}", path.display()))?;
        f.write_all(data)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, data).with_context(|| format!("write {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn persists() {
        let dir = std::env::temp_dir().join(format!("fcw-cert-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = super::load_or_create(&dir).unwrap();
        let b = super::load_or_create(&dir).unwrap();
        assert_eq!(a.sha256_hex, b.sha256_hex);
        assert_eq!(a.sha256_hex.len(), 64);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
