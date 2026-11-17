//! Token bucket с «долгом»: кадр пропускается сразу, если токены были, а
//! недостачу отрабатывает ожиданием. Так кадры больше ёмкости ведра не
//! застревают, а средняя скорость держится на заданной.

use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug)]
pub struct TokenBucket {
    rate: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    /// `rate` байт/с; `None` при rate = 0 (без ограничения).
    pub fn new(rate: u64) -> Option<Self> {
        if rate == 0 {
            return None;
        }
        let rate = rate as f64;
        // Ёмкость: 50 мс трафика, но не меньше пары полноразмерных кадров.
        let burst = (rate / 20.0).max(4096.0);
        Some(TokenBucket { rate, burst, tokens: burst, last: Instant::now() })
    }

    /// Списать `n` байт; вернуть, сколько надо подождать.
    pub fn take(&mut self, n: usize, now: Instant) -> Duration {
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.rate).min(self.burst);
        self.tokens -= n as f64;
        if self.tokens >= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(-self.tokens / self.rate)
        }
    }

    pub async fn wait(&mut self, n: usize) {
        let d = self.take(n, Instant::now());
        if !d.is_zero() {
            tokio::time::sleep(d).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited() {
        assert!(TokenBucket::new(0).is_none());
    }

    #[test]
    fn average_rate() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(100_000).unwrap();
        b.last = t0;
        // Ведро полно (5000 байт): первый кадр без ожидания.
        assert_eq!(b.take(1500, t0), Duration::ZERO);
        // Отправляем 1 МБ «мгновенно»: суммарное ожидание ~10 с.
        let mut now = t0;
        let mut sent = 1500;
        while sent < 1_000_000 {
            let d = b.take(1500, now);
            now += d;
            sent += 1500;
        }
        let secs = (now - t0).as_secs_f64();
        assert!((9.8..10.1).contains(&secs), "{secs}");
        // После простоя ведро наполняется не больше ёмкости.
        now += Duration::from_secs(100);
        assert_eq!(b.take(5000, now), Duration::ZERO);
        assert!(b.take(1500, now) > Duration::ZERO);
    }
}
