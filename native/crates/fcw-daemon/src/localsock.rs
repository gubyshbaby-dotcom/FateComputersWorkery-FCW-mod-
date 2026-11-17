//! Локальные потоковые сокеты (AF_UNIX): канал управления и сокеты QEMU.
//!
//! На Unix это tokio `UnixStream`. На Windows tokio не умеет AF_UNIX, поэтому
//! сокет `uds_windows` переводится в `DuplexStream` двумя потоками.

use std::io;
use std::path::Path;

/// Максимальная длина пути Unix-сокета с запасом (sun_path = 108 байт в Linux).
pub const MAX_SOCK_PATH: usize = 100;

#[cfg(unix)]
mod imp {
    use super::*;

    pub type LocalStream = tokio::net::UnixStream;

    pub async fn connect(path: &Path) -> io::Result<LocalStream> {
        tokio::net::UnixStream::connect(path).await
    }

    pub struct LocalListener(tokio::net::UnixListener);

    impl LocalListener {
        pub fn bind(path: &Path) -> io::Result<Self> {
            let _ = std::fs::remove_file(path);
            Ok(Self(tokio::net::UnixListener::bind(path)?))
        }
        pub async fn accept(&self) -> io::Result<LocalStream> {
            self.0.accept().await.map(|(s, _)| s)
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::io::{Read, Write};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    pub type LocalStream = tokio::io::DuplexStream;

    fn bridge(s: uds_windows::UnixStream) -> io::Result<LocalStream> {
        let handle = tokio::runtime::Handle::current();
        let (ours, theirs) = tokio::io::duplex(256 * 1024);
        let (mut rd, mut wr) = tokio::io::split(theirs);
        let mut s_read = s.try_clone()?;
        let mut s_write = s;
        let h1 = handle.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = match s_read.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if h1.block_on(wr.write_all(&buf[..n])).is_err() {
                    break;
                }
            }
            let _ = h1.block_on(wr.shutdown());
        });
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = match handle.block_on(rd.read(&mut buf)) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if s_write.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            let _ = s_write.shutdown(std::net::Shutdown::Both);
        });
        Ok(ours)
    }

    pub async fn connect(path: &Path) -> io::Result<LocalStream> {
        let p = path.to_path_buf();
        let s = tokio::task::spawn_blocking(move || uds_windows::UnixStream::connect(p))
            .await
            .map_err(io::Error::other)??;
        bridge(s)
    }

    pub struct LocalListener(std::sync::Arc<uds_windows::UnixListener>);

    impl LocalListener {
        pub fn bind(path: &Path) -> io::Result<Self> {
            let _ = std::fs::remove_file(path);
            Ok(Self(std::sync::Arc::new(uds_windows::UnixListener::bind(path)?)))
        }
        pub async fn accept(&self) -> io::Result<LocalStream> {
            let l = self.0.clone();
            let (s, _) = tokio::task::spawn_blocking(move || l.accept()).await.map_err(io::Error::other)??;
            bridge(s)
        }
    }
}

pub use imp::*;

/// Подключение с повторами, пока сокет не появится (QEMU создаёт его не сразу).
pub async fn connect_retry(path: &Path, timeout: std::time::Duration) -> io::Result<LocalStream> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match connect(path).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(e);
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}
