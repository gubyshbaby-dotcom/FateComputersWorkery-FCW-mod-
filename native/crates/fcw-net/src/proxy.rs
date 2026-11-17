//! Исходящие TCP-соединения хоста: напрямую или через HTTP CONNECT-прокси.

use std::io;
use std::net::SocketAddrV4;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// HTTP-прокси для CONNECT (`http://[user:pass@]host:port`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpProxy {
    pub host: String,
    pub port: u16,
    /// Готовое значение заголовка Proxy-Authorization.
    pub auth: Option<String>,
}

impl HttpProxy {
    pub fn parse(url: &str) -> io::Result<Self> {
        let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidInput, format!("upstream proxy {url:?}: {m}"));
        let url = url.trim();
        let rest = if let Some(r) = url.strip_prefix("http://") {
            r
        } else if url.contains("://") {
            return Err(bad("поддерживается только http://"));
        } else {
            url
        };
        let rest = rest.split('/').next().unwrap_or("");
        let (auth, hostport) = match rest.rsplit_once('@') {
            Some((a, h)) => (Some(format!("Basic {}", base64(&percent_decode(a)))), h),
            None => (None, rest),
        };
        let (host, port) = if let Some(h) = hostport.strip_prefix('[') {
            // [v6]:port
            let (h, p) = h.split_once(']').ok_or_else(|| bad("нет ']'"))?;
            (h.to_string(), p.strip_prefix(':').unwrap_or("80"))
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), p),
                None => (hostport.to_string(), "80"),
            }
        };
        if host.is_empty() {
            return Err(bad("пустой хост"));
        }
        let port = port.parse().map_err(|_| bad("плохой порт"))?;
        Ok(HttpProxy { host, port, auth })
    }
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        s.push(char::from(T[(n >> 18) as usize & 63]));
        s.push(char::from(T[(n >> 12) as usize & 63]));
        s.push(if c.len() > 1 { char::from(T[(n >> 6) as usize & 63]) } else { '=' });
        s.push(if c.len() > 2 { char::from(T[n as usize & 63]) } else { '=' });
    }
    s
}

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Соединиться с `dst`: напрямую или через CONNECT-прокси. Для прокси
/// `host` — имя, по которому гость получил этот адрес (многие прокси
/// пропускают CONNECT только по имени); без него — CONNECT по IP.
pub async fn connect(dst: SocketAddrV4, proxy: Option<&HttpProxy>, host: Option<&str>) -> io::Result<TcpStream> {
    let fut = async {
        match proxy {
            None => TcpStream::connect(dst).await,
            Some(p) => {
                let target = match host {
                    Some(h) => format!("{h}:{}", dst.port()),
                    None => dst.to_string(),
                };
                connect_via(p, &target).await
            }
        }
    };
    let s = tokio::time::timeout(CONNECT_TIMEOUT, fut)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connect timeout"))??;
    let _ = s.set_nodelay(true);
    Ok(s)
}

async fn connect_via(p: &HttpProxy, dst: &str) -> io::Result<TcpStream> {
    let mut s = TcpStream::connect((p.host.as_str(), p.port)).await?;
    let mut req = format!("CONNECT {dst} HTTP/1.1\r\nHost: {dst}\r\n");
    if let Some(a) = &p.auth {
        req.push_str(&format!("Proxy-Authorization: {a}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await?;
    // Читаем заголовок ответа побайтно, чтобы не съесть данные туннеля
    // (сервер может заговорить первым: SSH, SMTP).
    let mut head = Vec::with_capacity(256);
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if s.read(&mut b).await? == 0 {
            return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "proxy closed connection"));
        }
        head.push(b[0]);
        if head.len() > 16 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "proxy reply too long"));
        }
    }
    let line = String::from_utf8_lossy(&head);
    let status = line.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok()).unwrap_or(0);
    if !(200..300).contains(&status) {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("proxy CONNECT {dst}: {}", line.lines().next().unwrap_or("")),
        ));
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proxy_urls() {
        assert_eq!(
            HttpProxy::parse("http://127.0.0.1:46589").unwrap(),
            HttpProxy { host: "127.0.0.1".into(), port: 46589, auth: None }
        );
        assert_eq!(
            HttpProxy::parse("proxy.lan:3128/").unwrap(),
            HttpProxy { host: "proxy.lan".into(), port: 3128, auth: None }
        );
        let p = HttpProxy::parse("http://user:p%40ss@[::1]:8080").unwrap();
        assert_eq!(p.host, "::1");
        assert_eq!(p.port, 8080);
        assert_eq!(p.auth.as_deref(), Some("Basic dXNlcjpwQHNz")); // user:p@ss
        assert!(HttpProxy::parse("https://x:1").is_err());
        assert!(HttpProxy::parse("http://:1").is_err());
        assert!(HttpProxy::parse("http://h:notaport").is_err());
    }

    #[test]
    fn base64_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    #[tokio::test]
    async fn connect_through_fake_proxy() {
        use tokio::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let srv = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 1024];
            let mut got = Vec::new();
            while !got.ends_with(b"\r\n\r\n") {
                let n = s.read(&mut buf).await.unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            // Сразу за ответом — баннер «сервера», он не должен потеряться.
            s.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\nSSH-2.0-test\r\n").await.unwrap();
            String::from_utf8(got).unwrap()
        });
        let p = HttpProxy { host: "127.0.0.1".into(), port: addr.port(), auth: Some("Basic eA==".into()) };
        let mut s = connect("93.184.215.14:443".parse().unwrap(), Some(&p), None).await.unwrap();
        let mut banner = [0u8; 14];
        s.read_exact(&mut banner).await.unwrap();
        assert_eq!(&banner, b"SSH-2.0-test\r\n");
        let req = srv.await.unwrap();
        assert!(req.starts_with("CONNECT 93.184.215.14:443 HTTP/1.1\r\n"), "{req}");
        assert!(req.contains("Proxy-Authorization: Basic eA==\r\n"));

        // Отказ прокси -> ConnectionRefused.
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n").await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        let p = HttpProxy { host: "127.0.0.1".into(), port, auth: None };
        let e = connect("93.184.215.14:443".parse().unwrap(), Some(&p), Some("example.com")).await.unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused);
        assert!(srv.await.unwrap().starts_with("CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n"));
    }
}
