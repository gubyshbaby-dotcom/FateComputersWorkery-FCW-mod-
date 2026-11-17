//! Канал управления по TCP (демон-служба): токен в первом Hello, таймаут,
//! рассылка событий нескольким клиентам, запрет Shutdown, повторный bind.

use fcw_daemon::control::{self, ConnPolicy};
use fcw_daemon::daemon::Daemon;
use fcw_daemon::pb;
use fcw_daemon::qemu::QemuLocator;
use fcw_daemon::vm::Env;
use fcw_proto::control::{envelope, event, request, response};
use fcw_proto::frame;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

const TOKEN: &str = "s3cr3t-token-for-tests";

fn daemon(name: &str) -> Arc<Daemon> {
    let dir = std::env::temp_dir().join(format!("fcw-tcp-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = Arc::new(Env { images_dir: dir.join("images"), qemu: QemuLocator::fixed(None), tz: None, net: None });
    Daemon::new(env, dir.join("vms"), 4, 1, "00".into(), broadcast::channel(64).0)
}

async fn server(d: Arc<Daemon>, service: bool, auth_timeout: Duration) -> std::net::SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let mut p = ConnPolicy::tcp(Arc::new(TOKEN.into()), service);
    p.auth_timeout = auth_timeout;
    tokio::spawn(control::serve_tcp_on(d, l, p));
    addr
}

async fn send(s: &mut TcpStream, id: u64, kind: request::Kind) {
    let env = pb::Envelope { id, body: Some(envelope::Body::Request(pb::Request { kind: Some(kind) })) };
    frame::write_async(s, &env).await.unwrap();
}

fn hello(token: &str) -> request::Kind {
    request::Kind::Hello(pb::Hello { client_name: "test".into(), protocol_version: 1, auth_token: token.into() })
}

/// Следующий ответ (события пропускаются); `None` — соединение закрыто.
async fn response(s: &mut TcpStream) -> Option<(u64, pb::Response)> {
    loop {
        let e: pb::Envelope = tokio::time::timeout(Duration::from_secs(5), frame::read_async(s)).await.ok()?.ok()??;
        if let Some(envelope::Body::Response(r)) = e.body {
            return Some((e.id, r));
        }
    }
}

async fn closed(s: &mut TcpStream) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(5), frame::read_async::<_, pb::Envelope>(s)).await,
        Ok(Ok(None)) | Ok(Err(_))
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_required_and_events_to_all() {
    let d = daemon("auth");
    let addr = server(d.clone(), true, Duration::from_millis(500)).await;

    // неверный токен: unauthorized и разрыв
    let mut bad = TcpStream::connect(addr).await.unwrap();
    send(&mut bad, 1, hello("wrong")).await;
    let (id, r) = response(&mut bad).await.unwrap();
    assert_eq!((id, r.ok, r.error.as_str()), (1, false, "unauthorized"));
    assert!(closed(&mut bad).await);

    // первым пришёл не Hello: тоже unauthorized, запрос не выполняется
    let mut early = TcpStream::connect(addr).await.unwrap();
    send(&mut early, 7, request::Kind::Shutdown(pb::Shutdown {})).await;
    let (_, r) = response(&mut early).await.unwrap();
    assert_eq!(r.error, "unauthorized");
    assert!(closed(&mut early).await);
    assert!(!*d.shutdown_signal().borrow());

    // молчащий клиент отключается по таймауту Hello
    let mut silent = TcpStream::connect(addr).await.unwrap();
    let t = Instant::now();
    assert!(closed(&mut silent).await);
    assert!(t.elapsed() >= Duration::from_millis(400), "{:?}", t.elapsed());

    // два авторизованных клиента
    let mut a = TcpStream::connect(addr).await.unwrap();
    let mut b = TcpStream::connect(addr).await.unwrap();
    for s in [&mut a, &mut b] {
        send(s, 1, hello(TOKEN)).await;
        let (_, r) = response(s).await.unwrap();
        assert!(r.ok, "{}", r.error);
        assert!(matches!(r.kind, Some(response::Kind::Hello(_))));
    }
    // клиент, ещё не приславший Hello, событий не получает
    let mut pending = TcpStream::connect(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    d.log_event("warn", "broadcast-check");
    for s in [&mut a, &mut b] {
        let e: pb::Envelope =
            tokio::time::timeout(Duration::from_secs(5), frame::read_async(s)).await.unwrap().unwrap().unwrap();
        match e.body {
            Some(envelope::Body::Event(pb::Event { kind: Some(event::Kind::Log(l)) })) => {
                assert_eq!((e.id, l.message.as_str()), (0, "broadcast-check"))
            }
            other => panic!("expected event, got {other:?}"),
        }
    }
    send(&mut pending, 3, hello(TOKEN)).await;
    let (id, r) = response(&mut pending).await.unwrap();
    assert!(r.ok && id == 3);

    // обычные запросы после авторизации
    send(&mut a, 2, request::Kind::ListVms(pb::ListVms {})).await;
    let (id, r) = response(&mut a).await.unwrap();
    assert!(r.ok && id == 2);

    // Shutdown по TCP в режиме службы отклоняется, демон работает дальше
    send(&mut a, 3, request::Kind::Shutdown(pb::Shutdown {})).await;
    let (_, r) = response(&mut a).await.unwrap();
    assert!(!r.ok && r.error.contains("not allowed"), "{}", r.error);
    assert!(!*d.shutdown_signal().borrow());
    // соединение живо
    send(&mut a, 4, request::Kind::ListImages(pb::ListImages {})).await;
    assert!(response(&mut a).await.unwrap().1.ok);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_allowed_when_not_service() {
    let d = daemon("shutdown");
    let addr = server(d.clone(), false, Duration::from_secs(5)).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    send(&mut s, 1, hello(TOKEN)).await;
    assert!(response(&mut s).await.unwrap().1.ok);
    send(&mut s, 2, request::Kind::Shutdown(pb::Shutdown {})).await;
    assert!(response(&mut s).await.unwrap().1.ok);
    assert!(*d.shutdown_signal().borrow());
}

#[tokio::test]
async fn bind_retries_until_address_appears() {
    let attempts = Arc::new(AtomicU32::new(0));
    let a2 = attempts.clone();
    let t = Instant::now();
    let l = control::bind_retry(
        move || {
            let n = a2.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err(std::io::Error::from(std::io::ErrorKind::AddrNotAvailable))
                } else {
                    TcpListener::bind("127.0.0.1:0").await
                }
            }
        },
        Duration::from_millis(50),
    )
    .await
    .unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert!(t.elapsed() >= Duration::from_millis(100));
    drop(l);

    // адрес занят — ошибка сразу, без повторов
    let busy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = busy.local_addr().unwrap();
    let err = control::bind_retry(|| TcpListener::bind(addr), Duration::from_millis(50)).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);

    // настоящий EADDRNOTAVAIL: адреса нет ни на одном интерфейсе (TEST-NET-1)
    let r = tokio::time::timeout(
        Duration::from_millis(300),
        control::bind_retry(|| TcpListener::bind("192.0.2.123:25700"), Duration::from_millis(50)),
    )
    .await;
    assert!(r.is_err(), "keeps retrying while the address is missing");
}
