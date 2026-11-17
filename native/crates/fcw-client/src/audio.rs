//! Звук машины: пакеты Opus -> PCM 48 кГц стерео s16 (буфер с ограничением).

use std::collections::VecDeque;

/// Максимум буферизованного звука (значений i16): 0.5 с стерео.
const MAX_BUFFERED: usize = 48_000;
const FRAME: usize = 960 * 2;

pub struct AudioSink {
    #[cfg(feature = "audio")]
    dec: Option<opus::Decoder>,
    buf: VecDeque<i16>,
    last_seq: Option<u64>,
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
        }
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
        let n = out.len().min(self.buf.len()) & !1;
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
}
