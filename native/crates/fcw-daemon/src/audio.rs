//! Звук: PCM S16LE 48 кГц стерео из QEMU Audio (VNC) -> пакеты Opus по 20 мс.

pub const SAMPLE_RATE: u32 = 48000;
pub const CHANNELS: usize = 2;
/// Отсчётов на канал в одном кадре Opus (20 мс).
pub const FRAME_SAMPLES: usize = 960;

/// Готовый пакет Opus.
#[derive(Debug, Clone)]
pub struct OpusPacket {
    pub seq: u64,
    pub pts_us: u64,
    pub data: Vec<u8>,
}

#[cfg(feature = "audio")]
pub struct AudioEncoder {
    enc: opus::Encoder,
    pcm: Vec<i16>,
    odd: Option<u8>,
    seq: u64,
    samples: u64,
}

#[cfg(feature = "audio")]
impl AudioEncoder {
    pub fn new() -> anyhow::Result<Self> {
        let mut enc = opus::Encoder::new(SAMPLE_RATE, opus::Channels::Stereo, opus::Application::Audio)?;
        enc.set_bitrate(opus::Bitrate::Bits(96_000))?;
        Ok(AudioEncoder { enc, pcm: Vec::with_capacity(FRAME_SAMPLES * CHANNELS * 2), odd: None, seq: 0, samples: 0 })
    }

    /// Добавляет сырые байты PCM; возвращает готовые пакеты.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<OpusPacket> {
        let mut data = bytes;
        if let Some(b0) = self.odd.take() {
            if let Some((&b1, rest)) = data.split_first() {
                self.pcm.push(i16::from_le_bytes([b0, b1]));
                data = rest;
            } else {
                self.odd = Some(b0);
            }
        }
        let (chunks, rest) = data.as_chunks::<2>();
        for c in chunks {
            self.pcm.push(i16::from_le_bytes(*c));
        }
        if let [b] = rest {
            self.odd = Some(*b);
        }
        let mut out = Vec::new();
        let frame = FRAME_SAMPLES * CHANNELS;
        let mut buf = [0u8; 1500];
        while self.pcm.len() >= frame {
            let pts_us = self.samples * 1_000_000 / SAMPLE_RATE as u64;
            if let Ok(n) = self.enc.encode(&self.pcm[..frame], &mut buf) {
                out.push(OpusPacket { seq: self.seq, pts_us, data: buf[..n].to_vec() });
                self.seq += 1;
            }
            self.samples += FRAME_SAMPLES as u64;
            self.pcm.drain(..frame);
        }
        out
    }
}

/// Заглушка без libopus: звук не передаётся.
#[cfg(not(feature = "audio"))]
pub struct AudioEncoder;

#[cfg(not(feature = "audio"))]
impl AudioEncoder {
    pub fn new() -> anyhow::Result<Self> {
        anyhow::bail!("built without audio support")
    }
    pub fn push(&mut self, _bytes: &[u8]) -> Vec<OpusPacket> {
        Vec::new()
    }
}

#[cfg(all(test, feature = "audio"))]
mod tests {
    use super::*;

    #[test]
    fn encodes_20ms_frames() {
        let mut e = AudioEncoder::new().unwrap();
        // 30 мс тишины-синуса, порезанные на нечётные куски
        let mut bytes = Vec::new();
        for i in 0..(48 * 30) {
            let v = ((i as f32 * 0.05).sin() * 8000.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut pkts = Vec::new();
        for c in bytes.chunks(333) {
            pkts.extend(e.push(c));
        }
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].seq, 0);
        assert!(!pkts[0].data.is_empty() && pkts[0].data.len() < 1200);
        let more = e.push(&vec![0u8; 960 * 4]);
        assert_eq!(more.len(), 1);
        assert_eq!(more[0].pts_us, 20_000);
    }
}
