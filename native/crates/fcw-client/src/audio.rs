//! Звук машины: пакеты Opus -> PCM 48 кГц стерео s16 (буфер с ограничением).
//!
//! Пока экран машины идёт видеорежимом, картинка отстаёт от звука на время
//! кодирования и декодирования кадра. Сессия сообщает это отставание
//! ([`AudioSink::set_video_lag`]), и звук задерживается на столько же: в
//! буфере удерживается соответствующий запас.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Максимум буферизованного звука (значений i16): 0.5 с стерео.
const MAX_BUFFERED: usize = 48_000;
const FRAME: usize = 960 * 2;
/// Наибольшая задержка звука под видео.
const MAX_VIDEO_LAG_US: u64 = 250_000;
/// Отставание меньше этого не выравниваем.
const MIN_VIDEO_LAG_US: u64 = 15_000;
/// Задержку меняем, только если она разошлась с нужной больше чем на столько
/// значений (20 мс): каждая смена — короткая пауза или скачок звука.
const LAG_STEP: usize = 960 * 2;
/// Без новых замеров отставания (видео кончилось) задержка снимается.
const LAG_TTL: Duration = Duration::from_millis(1500);

pub struct AudioSink {
    #[cfg(feature = "audio")]
    dec: Option<opus::Decoder>,
    buf: VecDeque<i16>,
    last_seq: Option<u64>,
    /// Сглаженное отставание видео, мкс, и время последнего замера.
    lag_us: u64,
    lag_at: Option<Instant>,
    /// Сколько значений сейчас удерживается в буфере (задержка звука).
    hold: usize,
}

impl Default for AudioSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioSink {
    pub fn new() -> Self {
        AudioSink {
            #[cfg(feature = "audio")]
            dec: opus::Decoder::new(48_000, opus::Channels::Stereo).ok(),
            buf: VecDeque::new(),
            last_seq: None,
            lag_us: 0,
            lag_at: None,
            hold: 0,
        }
    }

    /// Видео этой машины сейчас отстаёт от звука на `lag_us` микросекунд.
    pub fn set_video_lag(&mut self, lag_us: u64) {
        let lag = lag_us.min(MAX_VIDEO_LAG_US);
        let fresh = self.lag_at.is_some_and(|t| t.elapsed() < LAG_TTL);
        self.lag_us = if fresh { (self.lag_us * 7 + lag) / 8 } else { lag };
        self.lag_at = Some(Instant::now());
    }

    /// Нужная задержка звука в значениях i16 (стерео, чётное число).
    fn wanted_hold(&self) -> usize {
        if !self.lag_at.is_some_and(|t| t.elapsed() < LAG_TTL) || self.lag_us < MIN_VIDEO_LAG_US {
            return 0;
        }
        (self.lag_us as usize * 48 / 1000) * 2
    }

    /// Сглаженное отставание видео от звука, мс (0 — видео сейчас не идёт).
    pub fn video_lag_ms(&self) -> u32 {
        if self.lag_at.is_some_and(|t| t.elapsed() < LAG_TTL) {
            (self.lag_us / 1000) as u32
        } else {
            0
        }
    }

    /// Текущая задержка звука под видео, мс.
    pub fn video_delay_ms(&self) -> u32 {
        (self.hold / 2 / 48) as u32
    }

    #[cfg(feature = "audio")]
    pub fn push(&mut self, seq: u64, data: &[u8]) {
        let Some(dec) = self.dec.as_mut() else { return };
        if let Some(last) = self.last_seq {
            if seq <= last {
                return; // запоздавший или повторный пакет
            }
            // потерянные пакеты: маскировка потерь
            let lost = (seq - last - 1).min(5);
            for _ in 0..lost {
                let mut out = [0i16; FRAME];
                if let Ok(n) = dec.decode(&[], &mut out, false) {
                    self.buf.extend(&out[..n * 2]);
                }
            }
        }
        self.last_seq = Some(seq);
        let mut out = [0i16; FRAME * 3];
        if let Ok(n) = dec.decode(data, &mut out, false) {
            self.buf.extend(&out[..n * 2]);
        }
        while self.buf.len() > MAX_BUFFERED {
            self.buf.pop_front();
        }
    }

    #[cfg(not(feature = "audio"))]
    pub fn push(&mut self, seq: u64, _data: &[u8]) {
        self.last_seq = Some(seq);
    }

    /// Забирает до `out.len()` значений (стерео чередуется); возвращает число.
    pub fn read(&mut self, out: &mut [i16]) -> usize {
        let want = self.wanted_hold();
        if want.abs_diff(self.hold) >= LAG_STEP || (want == 0 && self.hold != 0) {
            if want < self.hold {
                // задержка уменьшилась: лишний запас выбрасываем, иначе он
                // так и останется задержкой
                let drop = (self.hold - want).min(self.buf.len()) & !1;
                self.buf.drain(..drop);
            }
            self.hold = want;
        }
        let n = out.len().min(self.buf.len().saturating_sub(self.hold)) & !1;
        for (i, v) in self.buf.drain(..n).enumerate() {
            out[i] = v;
        }
        n
    }

    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(all(test, feature = "audio"))]
mod tests {
    use super::*;

    #[test]
    fn decode_roundtrip() {
        let mut enc = opus::Encoder::new(48_000, opus::Channels::Stereo, opus::Application::Audio).unwrap();
        let pcm: Vec<i16> = (0..FRAME).map(|i| ((i as f32 * 0.03).sin() * 6000.0) as i16).collect();
        let mut pkt = [0u8; 1500];
        let n = enc.encode(&pcm, &mut pkt).unwrap();
        let mut sink = AudioSink::new();
        sink.push(0, &pkt[..n]);
        assert_eq!(sink.buffered(), FRAME);
        sink.push(2, &pkt[..n]); // один потерян -> PLC
        assert_eq!(sink.buffered(), FRAME * 3);
        sink.push(1, &pkt[..n]); // запоздал -> отброшен
        assert_eq!(sink.buffered(), FRAME * 3);
        let mut out = vec![0i16; 1001];
        assert_eq!(sink.read(&mut out), 1000);
    }

    #[test]
    fn video_lag_delays_audio() {
        let mut sink = AudioSink::new();
        sink.buf.extend(std::iter::repeat_n(0i16, FRAME * 10)); // 200 мс
        let mut out = vec![0i16; FRAME * 20];
        // видео отстаёт на 60 мс: в буфере остаётся 60 мс запаса
        sink.set_video_lag(60_000);
        assert_eq!(sink.read(&mut out), FRAME * 7);
        assert_eq!(sink.video_delay_ms(), 60);
        assert_eq!(sink.buffered(), FRAME * 3);
        // малые колебания задержку не трогают
        sink.set_video_lag(70_000);
        sink.buf.extend(std::iter::repeat_n(0i16, FRAME));
        assert_eq!(sink.read(&mut out), FRAME);
        assert_eq!(sink.video_delay_ms(), 60);
        // видео кончилось: запас выбрасывается, задержки больше нет
        sink.lag_at = Some(Instant::now() - LAG_TTL * 2);
        assert_eq!(sink.read(&mut out), 0);
        assert_eq!((sink.video_delay_ms(), sink.buffered()), (0, 0));
        // совсем малое отставание не выравнивается
        sink.set_video_lag(5_000);
        sink.buf.extend(std::iter::repeat_n(0i16, FRAME));
        assert_eq!(sink.read(&mut out), FRAME);
    }
}
