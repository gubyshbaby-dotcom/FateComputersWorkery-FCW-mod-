//! Имитатор медленного канала для замеров: UDP-ретранслятор между клиентом и
//! демоном с ограничением скорости, задержкой и очередью (как UDP-прокси frp,
//! который возит датаграммы по своему TCP-туннелю: потерь нет, очередь большая).

use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct ShaperConfig {
    /// Скорость демон -> клиент, байт/с (0 — без ограничения).
    pub rate_down: u64,
    /// Скорость клиент -> демон, байт/с (0 — без ограничения).
    pub rate_up: u64,
    /// Задержка в одну сторону.
    pub delay: Duration,
    /// Предел очереди: пакет, который ждал бы дольше, отбрасывается.
    pub max_queue: Duration,
}

#[derive(Default, Debug)]
pub struct ShaperStats {
    pub down_bytes: AtomicU64,
    pub up_bytes: AtomicU64,
    pub dropped: AtomicU64,
    /// Наибольшее время в очереди (мкс), демон -> клиент.
    pub max_queue_us: AtomicU64,
}

/// Направление: расписание отправки по скорости и задержке.
struct Lane {
    rate: u64,
    delay: Duration,
    max_queue: Duration,
    next_free: Instant,
}

impl Lane {
    /// Момент доставки пакета или `None`, если очередь переполнена.
    fn schedule(&mut self, len: usize, now: Instant, stats: &ShaperStats, down: bool) -> Option<Instant> {
        if self.rate == 0 {
            return Some(now + self.delay);
        }
        let start = self.next_free.max(now);
        let wait = start - now;
        if wait > self.max_queue {
            stats.dropped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        if down {
            stats.max_queue_us.fetch_max(wait.as_micros() as u64, Ordering::Relaxed);
        }
        self.next_free = start + Duration::from_secs_f64(len as f64 / self.rate as f64);
        Some(self.next_free + self.delay)
    }
}

/// Запускает ретранслятор: клиент шлёт на возвращённый адрес, пакеты уходят
/// на `upstream`. Работает, пока жив рантайм.
pub async fn start(upstream: SocketAddr, cfg: ShaperConfig) -> std::io::Result<(SocketAddr, Arc<ShaperStats>)> {
    let front = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let back = Arc::new(UdpSocket::bind(if upstream.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" }).await?);
    back.connect(upstream).await?;
    let local = front.local_addr()?;
    let stats = Arc::new(ShaperStats::default());
    let client: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));

    // клиент -> демон
    let (up_tx, mut up_rx) = mpsc::unbounded_channel::<(Instant, Vec<u8>)>();
    {
        let (front, client, stats) = (front.clone(), client.clone(), stats.clone());
        tokio::spawn(async move {
            let mut lane = Lane { rate: cfg.rate_up, delay: cfg.delay, max_queue: cfg.max_queue, next_free: Instant::now() };
            let mut buf = vec![0u8; 65536];
            while let Ok((n, from)) = front.recv_from(&mut buf).await {
                *client.lock() = Some(from);
                stats.up_bytes.fetch_add(n as u64, Ordering::Relaxed);
                if let Some(at) = lane.schedule(n, Instant::now(), &stats, false) {
                    let _ = up_tx.send((at, buf[..n].to_vec()));
                }
            }
        });
        let back = back.clone();
        tokio::spawn(async move {
            while let Some((at, p)) = up_rx.recv().await {
                tokio::time::sleep_until(at).await;
                let _ = back.send(&p).await;
            }
        });
    }
    // демон -> клиент
    let (down_tx, mut down_rx) = mpsc::unbounded_channel::<(Instant, Vec<u8>)>();
    {
        let (back, stats) = (back.clone(), stats.clone());
        tokio::spawn(async move {
            let mut lane =
                Lane { rate: cfg.rate_down, delay: cfg.delay, max_queue: cfg.max_queue, next_free: Instant::now() };
            let mut buf = vec![0u8; 65536];
            while let Ok(n) = back.recv(&mut buf).await {
                stats.down_bytes.fetch_add(n as u64, Ordering::Relaxed);
                if let Some(at) = lane.schedule(n, Instant::now(), &stats, true) {
                    let _ = down_tx.send((at, buf[..n].to_vec()));
                }
            }
        });
        let (front, client) = (front.clone(), client.clone());
        tokio::spawn(async move {
            while let Some((at, p)) = down_rx.recv().await {
                tokio::time::sleep_until(at).await;
                let to = *client.lock();
                if let Some(to) = to {
                    let _ = front.send_to(&p, to).await;
                }
            }
        });
    }
    Ok((local, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rate_and_delay() {
        let echo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();
        tokio::spawn(async move {
            let mut b = [0u8; 2048];
            while let Ok((n, from)) = echo.recv_from(&mut b).await {
                let _ = echo.send_to(&b[..n], from).await;
            }
        });
        let cfg = ShaperConfig {
            rate_down: 100_000,
            rate_up: 0,
            delay: Duration::from_millis(20),
            max_queue: Duration::from_secs(5),
        };
        let (addr, stats) = start(echo_addr, cfg).await.unwrap();
        let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        c.connect(addr).await.unwrap();
        let t = Instant::now();
        // 20 пакетов по 1000 байт обратно при 100 КБ/с: около 0.2 с + 2 задержки
        for _ in 0..20 {
            c.send(&[7u8; 1000]).await.unwrap();
        }
        let mut b = [0u8; 2048];
        for _ in 0..20 {
            tokio::time::timeout(Duration::from_secs(5), c.recv(&mut b)).await.unwrap().unwrap();
        }
        let el = t.elapsed();
        assert!(el >= Duration::from_millis(220), "{el:?}");
        assert!(el < Duration::from_millis(600), "{el:?}");
        assert_eq!(stats.down_bytes.load(Ordering::Relaxed), 20_000);
    }
}
