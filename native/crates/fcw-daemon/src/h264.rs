//! Кодировщик H.264 (OpenH264, собран из исходников крейтом `openh264-sys2`).
//!
//! Обёртка над «сырым» API кодировщика: готовая обёртка крейта `openh264` не
//! даёт задать временные слои, пропуск кадров и предел битрейта. Настройки —
//! для малой задержки: без B-кадров (их в OpenH264 и нет), один опорный кадр,
//! ключевой кадр по запросу ([`H264Encoder::force_idr`]) и изредка сам.
//!
//! Временные слои: при `temporal_layers = 3` кадры идут по схеме
//! `T0 T2 T1 T2 T0 …`; кадры слоя выше можно выбросить, не ломая остальные.
//! Так один кодировщик отдаёт 30, 15 и 7,5 к/с (дальним зрителям — меньше).

use anyhow::{anyhow, bail, Result};
use openh264::formats::{RgbaSliceU8, YUVBuffer, YUVSource};
use openh264_sys2 as sys;
use openh264_sys2::API;
use std::os::raw::{c_int, c_void};
use std::ptr::{addr_of_mut, null_mut};

/// Параметры кодировщика.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct H264Config {
    /// Размер картинки (любой; кодируется с шириной, кратной 8, и чётной высотой).
    pub width: u32,
    pub height: u32,
    pub bitrate_bps: u32,
    pub max_fps: f32,
    /// Период ключевых кадров (кадров); поток надёжный, поэтому редко.
    pub gop: u32,
    /// Временные слои: 1..=3.
    pub temporal_layers: u8,
    /// Потоки кодировщика (кадр делится на столько срезов).
    pub threads: u16,
    /// Кодировщик может пропустить кадр, чтобы уложиться в битрейт.
    pub frame_skip: bool,
    /// 0 быстро, 1 средне, 2 лучше (и медленнее).
    pub complexity: u8,
    /// CABAC вместо CAVLC (профиль High): меньше бит, чуть больше CPU.
    pub cabac: bool,
}

impl H264Config {
    pub fn new(width: u32, height: u32, bitrate_bps: u32, max_fps: f32) -> Self {
        H264Config {
            width,
            height,
            bitrate_bps,
            max_fps,
            // кратно периоду временных слоёв
            gop: ((max_fps * 10.0) as u32).next_multiple_of(8),
            temporal_layers: 3,
            threads: 1,
            frame_skip: true,
            complexity: 0,
            cabac: true,
        }
    }
}

/// Закодированный кадр (access unit, Annex B).
#[derive(Clone, Debug)]
pub struct Encoded {
    pub key: bool,
    /// Временной слой: 0 — базовый.
    pub temporal_id: u8,
    pub data: Vec<u8>,
}

pub struct H264Encoder {
    api: sys::DynamicAPI,
    ptr: *mut sys::ISVCEncoder,
    vt: sys::ISVCEncoderVtbl,
    cfg: H264Config,
    /// Размер, с которым работает кодировщик.
    enc_w: usize,
    enc_h: usize,
    yuv: YUVBuffer,
    /// Картинка, дополненная до размера кодировщика (если нужно).
    padded: Vec<u8>,
    bs: Box<sys::SFrameBSInfo>,
}

// Кодировщик используется из одного потока за раз; указатель — на объект C++
// без привязки к потоку.
unsafe impl Send for H264Encoder {}

fn check(rc: c_int, what: &str) -> Result<()> {
    if rc != 0 {
        bail!("openh264: {what} failed ({rc})");
    }
    Ok(())
}

impl H264Encoder {
    pub fn new(cfg: H264Config) -> Result<Self> {
        if cfg.width < 16 || cfg.height < 16 || cfg.width > 3840 || cfg.height > 2160 {
            bail!("openh264: unsupported size {}x{}", cfg.width, cfg.height);
        }
        // ширина кратна 8: быстрый путь перевода YUV -> RGBA у декодера клиента
        let enc_w = (cfg.width as usize).next_multiple_of(8);
        let enc_h = (cfg.height as usize).next_multiple_of(2);
        let api = sys::DynamicAPI::from_source();
        let mut ptr: *mut sys::ISVCEncoder = null_mut();
        // SAFETY: создаём кодировщик и читаем его таблицу функций; указатель
        // проверен на null, освобождается в Drop.
        let vt = unsafe {
            check(api.WelsCreateSVCEncoder(&raw mut ptr), "create encoder")?;
            if ptr.is_null() || (*ptr).is_null() {
                bail!("openh264: no encoder");
            }
            **ptr
        };
        let mut enc = H264Encoder {
            api,
            ptr,
            vt,
            cfg,
            enc_w,
            enc_h,
            yuv: YUVBuffer::new(enc_w, enc_h),
            padded: Vec::new(),
            bs: Box::default(),
        };
        enc.init()?;
        Ok(enc)
    }

    fn init(&mut self) -> Result<()> {
        let c = self.cfg;
        let missing = || anyhow!("openh264: encoder function missing");
        let get_default = self.vt.GetDefaultParams.ok_or_else(missing)?;
        let init_ext = self.vt.InitializeExt.ok_or_else(missing)?;
        let set_option = self.vt.SetOption.ok_or_else(missing)?;
        let mut p = sys::SEncParamExt::default();
        // SAFETY: p — корректная структура, ptr — живой кодировщик.
        unsafe { check(get_default(self.ptr, &raw mut p), "default params")? };
        let bitrate = c.bitrate_bps.clamp(64_000, 200_000_000) as c_int;
        let threads = c.threads.max(1);
        p.iUsageType = sys::CAMERA_VIDEO_REAL_TIME;
        p.iPicWidth = self.enc_w as c_int;
        p.iPicHeight = self.enc_h as c_int;
        p.iTargetBitrate = bitrate;
        p.iMaxBitrate = bitrate + bitrate / 4;
        p.iRCMode = sys::RC_BITRATE_MODE;
        p.fMaxFrameRate = c.max_fps;
        p.iTemporalLayerNum = c.temporal_layers.clamp(1, 3) as c_int;
        p.iSpatialLayerNum = 1;
        p.iComplexityMode = match c.complexity {
            0 => sys::LOW_COMPLEXITY,
            1 => sys::MEDIUM_COMPLEXITY,
            _ => sys::HIGH_COMPLEXITY,
        };
        p.uiIntraPeriod = c.gop;
        p.iNumRefFrame = 1;
        p.eSpsPpsIdStrategy = sys::CONSTANT_ID;
        p.iEntropyCodingModeFlag = c.cabac as c_int;
        p.bEnableFrameSkip = c.frame_skip;
        p.bEnableLongTermReference = false;
        p.iMultipleThreadIdc = threads;
        p.bEnableDenoise = false;
        p.bEnableBackgroundDetection = true;
        p.bEnableAdaptiveQuant = true;
        p.bEnableSceneChangeDetect = true;
        p.iLoopFilterDisableIdc = 0;
        let l = &mut p.sSpatialLayers[0];
        l.iVideoWidth = self.enc_w as c_int;
        l.iVideoHeight = self.enc_h as c_int;
        l.fFrameRate = c.max_fps;
        l.iSpatialBitrate = bitrate;
        l.iMaxSpatialBitrate = bitrate + bitrate / 4;
        if c.cabac {
            l.uiProfileIdc = sys::PRO_HIGH;
        }
        if threads > 1 {
            l.sSliceArgument.uiSliceMode = sys::SM_FIXEDSLCNUM_SLICE;
            l.sSliceArgument.uiSliceNum = threads as u32;
        } else {
            l.sSliceArgument.uiSliceMode = sys::SM_SINGLE_SLICE;
            l.sSliceArgument.uiSliceNum = 1;
        }
        let mut quiet: c_int = sys::WELS_LOG_QUIET as c_int;
        let mut fmt: c_int = sys::videoFormatI420 as c_int;
        // SAFETY: параметры живут до конца вызовов; опции — c_int, как требует API.
        unsafe {
            // журнал выключаем до инициализации: она пишет в stderr
            set_option(self.ptr, sys::ENCODER_OPTION_TRACE_LEVEL, addr_of_mut!(quiet).cast::<c_void>());
            check(init_ext(self.ptr, &raw const p), "initialize")?;
            check(
                set_option(self.ptr, sys::ENCODER_OPTION_DATAFORMAT, addr_of_mut!(fmt).cast::<c_void>()),
                "data format",
            )?;
        }
        Ok(())
    }

    pub fn config(&self) -> &H264Config {
        &self.cfg
    }

    /// Меняет целевой битрейт на ходу (без ключевого кадра).
    pub fn set_bitrate(&mut self, bitrate_bps: u32) {
        let Some(set_option) = self.vt.SetOption else { return };
        let bitrate = bitrate_bps.clamp(64_000, 200_000_000) as c_int;
        let mut target = sys::SBitrateInfo { iLayer: sys::SPATIAL_LAYER_ALL, iBitrate: bitrate };
        let mut max = sys::SBitrateInfo { iLayer: sys::SPATIAL_LAYER_ALL, iBitrate: bitrate + bitrate / 4 };
        let raise = bitrate_bps > self.cfg.bitrate_bps;
        // SAFETY: живой кодировщик; опции — SBitrateInfo, как требует API.
        // Предел должен оставаться не ниже цели, отсюда порядок.
        unsafe {
            if raise {
                set_option(self.ptr, sys::ENCODER_OPTION_MAX_BITRATE, addr_of_mut!(max).cast::<c_void>());
            }
            set_option(self.ptr, sys::ENCODER_OPTION_BITRATE, addr_of_mut!(target).cast::<c_void>());
            if !raise {
                set_option(self.ptr, sys::ENCODER_OPTION_MAX_BITRATE, addr_of_mut!(max).cast::<c_void>());
            }
        }
        self.cfg.bitrate_bps = bitrate_bps;
    }

    /// Следующий кадр будет ключевым (IDR).
    pub fn force_idr(&mut self) {
        if let Some(f) = self.vt.ForceIntraFrame {
            // SAFETY: живой инициализированный кодировщик.
            unsafe {
                f(self.ptr, true);
            }
        }
    }

    /// Кодирует кадр RGBA (`width * height * 4` байт, строки подряд).
    /// `None` — кодировщик пропустил кадр (управление битрейтом).
    pub fn encode_rgba(&mut self, rgba: &[u8], timestamp_ms: u64) -> Result<Option<Encoded>> {
        let (w, h) = (self.cfg.width as usize, self.cfg.height as usize);
        if rgba.len() != w * h * 4 {
            bail!("openh264: frame size mismatch");
        }
        if (w, h) == (self.enc_w, self.enc_h) {
            self.yuv.read_rgba8(RgbaSliceU8::new(rgba, (w, h)));
        } else {
            // дополнение: повторяем последний столбец и строку
            let (ew, eh) = (self.enc_w, self.enc_h);
            self.padded.resize(ew * eh * 4, 0);
            for y in 0..eh {
                let sy = y.min(h - 1);
                let dst = &mut self.padded[y * ew * 4..(y + 1) * ew * 4];
                dst[..w * 4].copy_from_slice(&rgba[sy * w * 4..(sy + 1) * w * 4]);
                let last: [u8; 4] = rgba[(sy * w + w - 1) * 4..(sy * w + w) * 4].try_into().expect("pixel");
                for px in dst[w * 4..].as_chunks_mut::<4>().0 {
                    *px = last;
                }
            }
            self.yuv.read_rgba8(RgbaSliceU8::new(&self.padded, (ew, eh)));
        }
        let (s0, s1, s2) = self.yuv.strides();
        let pic = sys::SSourcePicture {
            iColorFormat: sys::videoFormatI420 as c_int,
            iStride: [s0 as c_int, s1 as c_int, s2 as c_int, 0],
            pData: [
                self.yuv.y().as_ptr().cast_mut(),
                self.yuv.u().as_ptr().cast_mut(),
                self.yuv.v().as_ptr().cast_mut(),
                null_mut(),
            ],
            iPicWidth: self.enc_w as c_int,
            iPicHeight: self.enc_h as c_int,
            uiTimeStamp: timestamp_ms as i64,
            bPsnrY: false,
            bPsnrU: false,
            bPsnrV: false,
        };
        let encode = self.vt.EncodeFrame.ok_or_else(|| anyhow!("openh264: encoder function missing"))?;
        // SAFETY: плоскости YUV живут в self.yuv до конца вызова, кодировщик их
        // только читает; bs — выходная структура нужного размера.
        unsafe { check(encode(self.ptr, &raw const pic, &raw mut *self.bs), "encode")? };
        let bs = &*self.bs;
        if bs.eFrameType == sys::videoFrameTypeSkip || bs.eFrameType == sys::videoFrameTypeInvalid {
            return Ok(None);
        }
        let mut data = Vec::with_capacity(bs.iFrameSizeInBytes.max(0) as usize);
        let mut temporal_id = 0u8;
        for layer in &bs.sLayerInfo[..(bs.iLayerNum.max(0) as usize).min(bs.sLayerInfo.len())] {
            if layer.uiLayerType == sys::VIDEO_CODING_LAYER as u8 {
                temporal_id = layer.uiTemporalId;
            }
            // SAFETY: кодировщик заполнил iNalCount длин и буфер суммарной длины;
            // они действительны до следующего вызова EncodeFrame.
            unsafe {
                let mut len = 0usize;
                for i in 0..layer.iNalCount.max(0) as usize {
                    len += (*layer.pNalLengthInByte.add(i)).max(0) as usize;
                }
                data.extend_from_slice(std::slice::from_raw_parts(layer.pBsBuf, len));
            }
        }
        if data.is_empty() {
            return Ok(None);
        }
        Ok(Some(Encoded { key: bs.eFrameType == sys::videoFrameTypeIDR, temporal_id, data }))
    }
}

impl Drop for H264Encoder {
    fn drop(&mut self) {
        // SAFETY: указатель получен из WelsCreateSVCEncoder и больше не используется.
        unsafe {
            if let Some(f) = self.vt.Uninitialize {
                f(self.ptr);
            }
            self.api.WelsDestroySVCEncoder(self.ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, t: usize) -> Vec<u8> {
        let mut v = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                v[o] = ((x + t * 3) % 256) as u8;
                v[o + 1] = ((y + t) % 256) as u8;
                v[o + 2] = ((x + y) / 2 % 256) as u8;
            }
        }
        v
    }

    #[test]
    fn encodes_and_decodes_with_temporal_layers() {
        // нечётный размер: проверяем дополнение
        let (w, h) = (161usize, 99usize);
        let mut cfg = H264Config::new(w as u32, h as u32, 1_000_000, 30.0);
        cfg.frame_skip = false;
        let mut enc = H264Encoder::new(cfg).unwrap();
        let mut dec = openh264::decoder::Decoder::new().unwrap();
        let mut tids = Vec::new();
        for t in 0..9 {
            let src = frame(w, h, t);
            if t == 5 {
                enc.force_idr();
            }
            let e = enc.encode_rgba(&src, t as u64 * 33).unwrap().expect("frame");
            assert_eq!(e.key, t == 0 || t == 5, "frame {t}");
            assert_eq!(&e.data[..4], &[0, 0, 0, 1]);
            tids.push(e.temporal_id);
            let yuv = dec.decode(&e.data).unwrap().expect("picture");
            let (dw, dh) = yuv.dimensions();
            assert_eq!((dw, dh), (168, 100));
            let mut rgba = vec![0u8; dw * dh * 4];
            yuv.write_rgba8(&mut rgba);
            // картинка похожа на исходную
            let mut err = 0u64;
            for y in 0..h {
                for x in 0..w {
                    for k in 0..3 {
                        let d = rgba[(y * dw + x) * 4 + k] as i64 - src[(y * w + x) * 4 + k] as i64;
                        err += (d * d) as u64;
                    }
                }
            }
            let mse = err as f64 / (w * h * 3) as f64;
            assert!(mse < 60.0, "frame {t}: mse {mse}");
        }
        assert_eq!(&tids[..5], &[0, 2, 1, 2, 0]);
        // после принудительного IDR схема слоёв начинается заново
        assert_eq!(&tids[5..], &[0, 2, 1, 2]);
    }

    #[test]
    fn base_layer_decodes_alone() {
        let (w, h) = (128usize, 96usize);
        let mut cfg = H264Config::new(w as u32, h as u32, 800_000, 30.0);
        cfg.frame_skip = false;
        let mut enc = H264Encoder::new(cfg).unwrap();
        let mut dec = openh264::decoder::Decoder::new().unwrap();
        let mut shown = 0;
        for t in 0..12 {
            let e = enc.encode_rgba(&frame(w, h, t), t as u64 * 33).unwrap().expect("frame");
            if e.temporal_id > 0 {
                continue; // «дальний зритель»: только базовый слой
            }
            assert!(dec.decode(&e.data).unwrap().is_some(), "frame {t}");
            shown += 1;
        }
        assert_eq!(shown, 3);
    }
}
