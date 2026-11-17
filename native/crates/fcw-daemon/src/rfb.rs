//! Минимальный клиент RFB 3.8 к VNC-серверу QEMU (Unix-сокет).
//!
//! Кодировки: Raw, DesktopSize (-223), Cursor (-239), QEMU Extended Key
//! Event (-258), QEMU Audio (-259), QEMU LED State (-261),
//! QEMU Pointer Motion Change (-257).
//! Формат пикселей: 32 бит, little-endian, R<<0 | G<<8 | B<<16, то есть в
//! памяти R,G,B,X — почти готовый RGBA.

use crate::gfx::{Cursor, GfxScreen};
use crate::keymap;
use crate::localsock::LocalStream;
use anyhow::{anyhow, bail, Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::Instant;

const ENC_RAW: i32 = 0;
const ENC_DESKTOP_SIZE: i32 = -223;
const ENC_CURSOR: i32 = -239;
const ENC_POINTER_TYPE_CHANGE: i32 = -257;
const ENC_QEMU_EXT_KEY: i32 = -258;
const ENC_QEMU_AUDIO: i32 = -259;
/// QEMU LED state: заодно выключает в QEMU «синхронизацию» Caps/Num Lock по
/// keysym, которая иначе ломает Shift+буква в расширенных событиях клавиш.
const ENC_LED_STATE: i32 = -261;
const ENC_EXT_DESKTOP_SIZE: i32 = -308;

/// Минимальный интервал между запросами обновлений экрана.
const UPDATE_INTERVAL: Duration = Duration::from_millis(20);

/// Команды от демона к VNC-соединению.
#[derive(Debug, Clone)]
pub enum RfbCmd {
    /// Клавиша evdev.
    Key { evdev: u32, down: bool },
    /// Кнопки VNC (бит 0 левая, 1 средняя, 2 правая, 3/4 колесо вверх/вниз,
    /// 5/6 колесо влево/вправо) и абсолютная позиция.
    Pointer { mask: u8, x: u16, y: u16 },
    /// Полный (неинкрементальный) запрос кадра.
    Refresh,
}

/// События VNC-соединения для машины.
#[derive(Debug, Clone)]
pub enum RfbEvent {
    Resized(u32, u32),
    Cursor(Cursor),
    /// PCM S16LE, 2 канала, 48 кГц.
    Audio(Vec<u8>),
}

enum ReaderMsg {
    UpdateDone,
    ExtKey,
    AudioSupported,
}

/// Подключается, выполняет рукопожатие и работает, пока соединение живо.
pub async fn run(
    stream: LocalStream,
    gfx: Arc<GfxScreen>,
    cmds: &mut mpsc::UnboundedReceiver<RfbCmd>,
    events: mpsc::UnboundedSender<RfbEvent>,
    want_audio: bool,
) -> Result<()> {
    let (mut rd, mut wr) = tokio::io::split(stream);

    // --- рукопожатие
    let mut ver = [0u8; 12];
    rd.read_exact(&mut ver).await.context("RFB version")?;
    if &ver[..4] != b"RFB " {
        bail!("not an RFB server");
    }
    wr.write_all(b"RFB 003.008\n").await?;
    let n = rd.read_u8().await?;
    if n == 0 {
        let len = rd.read_u32().await? as usize;
        let mut reason = vec![0u8; len.min(4096)];
        rd.read_exact(&mut reason).await?;
        bail!("VNC refused: {}", String::from_utf8_lossy(&reason));
    }
    let mut types = vec![0u8; n as usize];
    rd.read_exact(&mut types).await?;
    if !types.contains(&1) {
        bail!("VNC requires authentication ({types:?})");
    }
    wr.write_all(&[1]).await?;
    let result = rd.read_u32().await?;
    if result != 0 {
        bail!("VNC security handshake failed");
    }
    wr.write_all(&[1]).await?; // ClientInit: shared
    let width = rd.read_u16().await? as u32;
    let height = rd.read_u16().await? as u32;
    let mut pf = [0u8; 16];
    rd.read_exact(&mut pf).await?;
    let name_len = rd.read_u32().await? as usize;
    let mut name = vec![0u8; name_len.min(4096)];
    rd.read_exact(&mut name).await?;
    tracing::debug!("VNC connected: {}x{} {:?}", width, height, String::from_utf8_lossy(&name));

    gfx.resize(width, height);
    let _ = events.send(RfbEvent::Resized(width, height));

    // SetPixelFormat
    let mut msg = vec![0u8, 0, 0, 0];
    msg.extend_from_slice(&[32, 24, 0, 1]);
    msg.extend_from_slice(&255u16.to_be_bytes());
    msg.extend_from_slice(&255u16.to_be_bytes());
    msg.extend_from_slice(&255u16.to_be_bytes());
    msg.extend_from_slice(&[0, 8, 16, 0, 0, 0]);
    wr.write_all(&msg).await?;
    // SetEncodings
    let encs = [ENC_RAW, ENC_DESKTOP_SIZE, ENC_CURSOR, ENC_QEMU_EXT_KEY, ENC_QEMU_AUDIO, ENC_LED_STATE];
    let mut msg = vec![2u8, 0];
    msg.extend_from_slice(&(encs.len() as u16).to_be_bytes());
    for e in encs {
        msg.extend_from_slice(&e.to_be_bytes());
    }
    wr.write_all(&msg).await?;
    write_update_request(&mut wr, false, width, height).await?;

    // --- чтение в отдельной задаче
    let (rtx, mut rrx) = mpsc::unbounded_channel::<ReaderMsg>();
    let gfx_r = gfx.clone();
    let ev_r = events.clone();
    let mut reader = AbortOnDrop(tokio::spawn(async move { read_loop(&mut rd, gfx_r, ev_r, rtx).await }));

    let mut ext_key = false;
    let mut audio_on = false;
    let (mut w, mut h) = (width, height);
    let mut last_request = Instant::now();
    let mut next_request: Option<Instant> = None;
    let mut full_refresh = false;
    loop {
        let sleep_until = next_request.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            res = &mut reader.0 => {
                return match res {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(anyhow!("VNC reader: {e}")),
                };
            }
            msg = rrx.recv() => match msg {
                Some(ReaderMsg::UpdateDone) => {
                    let (nw, nh) = gfx.size();
                    w = nw; h = nh;
                    let at = last_request + UPDATE_INTERVAL;
                    next_request = Some(at.max(Instant::now()));
                }
                Some(ReaderMsg::ExtKey) => ext_key = true,
                Some(ReaderMsg::AudioSupported) if want_audio && !audio_on => {
                    audio_on = true;
                    // формат S16, 2 канала, 48000 Гц, затем включить
                    let mut m = vec![255u8, 1];
                    m.extend_from_slice(&2u16.to_be_bytes());
                    m.extend_from_slice(&[3, 2]);
                    m.extend_from_slice(&48000u32.to_be_bytes());
                    m.extend_from_slice(&[255u8, 1]);
                    m.extend_from_slice(&0u16.to_be_bytes());
                    wr.write_all(&m).await?;
                }
                Some(ReaderMsg::AudioSupported) | None => {}
            },
            _ = tokio::time::sleep_until(sleep_until), if next_request.is_some() => {
                next_request = None;
                last_request = Instant::now();
                write_update_request(&mut wr, !full_refresh, w, h).await?;
                full_refresh = false;
            }
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { return Ok(()) };
                match cmd {
                    RfbCmd::Key { evdev, down } => {
                        let keysym = keymap::evdev_to_keysym(evdev);
                        if ext_key {
                            if let Some(q) = keymap::evdev_to_qnum(evdev) {
                                let mut m = vec![255u8, 0];
                                m.extend_from_slice(&(down as u16).to_be_bytes());
                                m.extend_from_slice(&keysym.to_be_bytes());
                                m.extend_from_slice(&q.to_be_bytes());
                                wr.write_all(&m).await?;
                            }
                        } else if keysym != 0 {
                            let mut m = vec![4u8, down as u8, 0, 0];
                            m.extend_from_slice(&keysym.to_be_bytes());
                            wr.write_all(&m).await?;
                        }
                    }
                    RfbCmd::Pointer { mask, x, y } => {
                        let mut m = vec![5u8, mask];
                        m.extend_from_slice(&x.to_be_bytes());
                        m.extend_from_slice(&y.to_be_bytes());
                        wr.write_all(&m).await?;
                    }
                    RfbCmd::Refresh => {
                        full_refresh = true;
                        if next_request.is_none() {
                            last_request = Instant::now();
                            write_update_request(&mut wr, false, w, h).await?;
                            full_refresh = false;
                        }
                    }
                }
            }
        }
    }
}

struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn write_update_request<W: tokio::io::AsyncWrite + Unpin>(
    wr: &mut W,
    incremental: bool,
    w: u32,
    h: u32,
) -> Result<()> {
    let mut m = vec![3u8, incremental as u8, 0, 0, 0, 0];
    m.extend_from_slice(&(w.min(65535) as u16).to_be_bytes());
    m.extend_from_slice(&(h.min(65535) as u16).to_be_bytes());
    wr.write_all(&m).await?;
    Ok(())
}

async fn read_loop<R: AsyncRead + Unpin>(
    rd: &mut R,
    gfx: Arc<GfxScreen>,
    events: mpsc::UnboundedSender<RfbEvent>,
    tx: mpsc::UnboundedSender<ReaderMsg>,
) -> Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let t = match rd.read_u8().await {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        match t {
            0 => {
                let _pad = rd.read_u8().await?;
                let n = rd.read_u16().await?;
                for _ in 0..n {
                    let x = rd.read_u16().await? as u32;
                    let y = rd.read_u16().await? as u32;
                    let w = rd.read_u16().await? as u32;
                    let h = rd.read_u16().await? as u32;
                    let enc = rd.read_i32().await?;
                    match enc {
                        ENC_RAW => {
                            let len = (w * h * 4) as usize;
                            buf.resize(len, 0);
                            rd.read_exact(&mut buf[..len]).await?;
                            gfx.apply_rgbx(x, y, w, h, &buf[..len]);
                        }
                        ENC_DESKTOP_SIZE => {
                            gfx.resize(w, h);
                            let _ = events.send(RfbEvent::Resized(w, h));
                        }
                        ENC_EXT_DESKTOP_SIZE => {
                            let screens = rd.read_u8().await?;
                            let mut skip = vec![0u8; 3 + screens as usize * 16];
                            rd.read_exact(&mut skip).await?;
                            gfx.resize(w, h);
                            let _ = events.send(RfbEvent::Resized(w, h));
                        }
                        ENC_CURSOR => {
                            let plen = (w * h * 4) as usize;
                            let mlen = (w.div_ceil(8) * h) as usize;
                            let mut px = vec![0u8; plen];
                            let mut mask = vec![0u8; mlen];
                            rd.read_exact(&mut px).await?;
                            rd.read_exact(&mut mask).await?;
                            let _ = events.send(RfbEvent::Cursor(cursor_rgba(x, y, w, h, &px, &mask)));
                        }
                        ENC_QEMU_EXT_KEY => {
                            let _ = tx.send(ReaderMsg::ExtKey);
                        }
                        ENC_QEMU_AUDIO => {
                            let _ = tx.send(ReaderMsg::AudioSupported);
                        }
                        ENC_POINTER_TYPE_CHANGE => {}
                        ENC_LED_STATE => {
                            let _leds = rd.read_u8().await?;
                        }
                        other => bail!("unsupported VNC encoding {other}"),
                    }
                }
                gfx.commit();
                let _ = tx.send(ReaderMsg::UpdateDone);
            }
            1 => {
                let _pad = rd.read_u8().await?;
                let _first = rd.read_u16().await?;
                let n = rd.read_u16().await? as usize;
                let mut skip = vec![0u8; n * 6];
                rd.read_exact(&mut skip).await?;
            }
            2 => {} // Bell
            3 => {
                let mut pad = [0u8; 3];
                rd.read_exact(&mut pad).await?;
                let len = rd.read_u32().await? as usize;
                let mut skip = vec![0u8; len];
                rd.read_exact(&mut skip).await?;
            }
            255 => {
                let sub = rd.read_u8().await?;
                if sub != 1 {
                    bail!("unknown QEMU server message {sub}");
                }
                let op = rd.read_u16().await?;
                if op == 2 {
                    let len = rd.read_u32().await? as usize;
                    if len > 16 * 1024 * 1024 {
                        bail!("audio chunk too large");
                    }
                    let mut data = vec![0u8; len];
                    rd.read_exact(&mut data).await?;
                    let _ = events.send(RfbEvent::Audio(data));
                }
            }
            other => bail!("unknown VNC server message {other}"),
        }
    }
}

/// Пиксели курсора (R,G,B,X) + битовая маска -> RGBA.
pub fn cursor_rgba(hx: u32, hy: u32, w: u32, h: u32, px: &[u8], mask: &[u8]) -> Cursor {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let row_bytes = w.div_ceil(8) as usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            let bit = mask[y * row_bytes + x / 8] & (0x80 >> (x % 8)) != 0;
            rgba[i] = px[i];
            rgba[i + 1] = px[i + 1];
            rgba[i + 2] = px[i + 2];
            rgba[i + 3] = if bit { 255 } else { 0 };
        }
    }
    Cursor { width: w, height: h, hot_x: hx, hot_y: hy, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_mask() {
        // 2x1: первый пиксель видим, второй прозрачен
        let c = cursor_rgba(1, 0, 2, 1, &[10, 20, 30, 0, 40, 50, 60, 0], &[0b1000_0000]);
        assert_eq!(c.rgba, vec![10, 20, 30, 255, 40, 50, 60, 0]);
        assert!(c.visible());
        assert_eq!(c.hot_x, 1);
    }
}
