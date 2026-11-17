//! Кадр = u32 big-endian длина + закодированное protobuf-сообщение.

use prost::Message;
use std::io::{self, Read, Write};

/// Предел длины одного кадра по умолчанию (64 МиБ).
pub const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

/// Кодирует сообщение в кадр (длина + тело).
pub fn encode<M: Message>(msg: &M) -> Vec<u8> {
    let len = msg.encoded_len();
    let mut buf = Vec::with_capacity(4 + len);
    buf.extend_from_slice(&(len as u32).to_be_bytes());
    msg.encode(&mut buf).expect("Vec has unlimited capacity");
    buf
}

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

fn check_len(len: usize, max: usize) -> io::Result<()> {
    if len > max {
        return Err(invalid(format!("frame too large: {len} > {max}")));
    }
    Ok(())
}

/// Пишет один кадр (синхронно).
pub fn write<W: Write, M: Message>(w: &mut W, msg: &M) -> io::Result<()> {
    w.write_all(&encode(msg))
}

/// Читает один кадр. `Ok(None)` — чистый конец потока до начала кадра.
pub fn read<R: Read, M: Message + Default>(r: &mut R) -> io::Result<Option<M>> {
    read_max(r, MAX_FRAME_LEN)
}

/// Как [`read`], но с явным пределом длины.
pub fn read_max<R: Read, M: Message + Default>(r: &mut R, max: usize) -> io::Result<Option<M>> {
    let mut hdr = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut hdr[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes(hdr) as usize;
    check_len(len, max)?;
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    M::decode(body.as_slice()).map(Some).map_err(invalid)
}

/// Разбор кадров из накопительного буфера (для неблокирующего чтения).
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }
    /// Следующее целое сообщение, если оно уже накоплено.
    pub fn next_msg<M: Message + Default>(&mut self) -> io::Result<Option<M>> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        check_len(len, MAX_FRAME_LEN)?;
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let msg = M::decode(&self.buf[4..4 + len]).map_err(invalid)?;
        self.buf.drain(..4 + len);
        Ok(Some(msg))
    }
}

#[cfg(feature = "tokio")]
mod async_io {
    use super::*;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    /// Пишет один кадр (tokio).
    pub async fn write_async<W: AsyncWrite + Unpin, M: Message>(w: &mut W, msg: &M) -> io::Result<()> {
        w.write_all(&encode(msg)).await
    }

    /// Читает один кадр (tokio). `Ok(None)` — чистый конец потока.
    pub async fn read_async<R: AsyncRead + Unpin, M: Message + Default>(r: &mut R) -> io::Result<Option<M>> {
        read_async_max(r, MAX_FRAME_LEN).await
    }

    pub async fn read_async_max<R: AsyncRead + Unpin, M: Message + Default>(
        r: &mut R,
        max: usize,
    ) -> io::Result<Option<M>> {
        let mut hdr = [0u8; 4];
        let mut got = 0;
        while got < 4 {
            let n = r.read(&mut hdr[got..]).await?;
            if n == 0 {
                if got == 0 {
                    return Ok(None);
                }
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            got += n;
        }
        let len = u32::from_be_bytes(hdr) as usize;
        check_len(len, max)?;
        let mut body = vec![0u8; len];
        r.read_exact(&mut body).await?;
        M::decode(body.as_slice()).map(Some).map_err(invalid)
    }
}

#[cfg(feature = "tokio")]
pub use async_io::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{envelope, request, Envelope, Hello, Request};
    use crate::stream::TextRow;

    fn hello() -> Envelope {
        Envelope {
            id: 7,
            body: Some(envelope::Body::Request(Request {
                kind: Some(request::Kind::Hello(Hello {
                    client_name: "test".into(),
                    protocol_version: 1,
                    ..Default::default()
                })),
            })),
        }
    }

    #[test]
    fn sync_roundtrip() {
        let mut buf = Vec::new();
        write(&mut buf, &hello()).unwrap();
        write(&mut buf, &hello()).unwrap();
        let len = u32::from_be_bytes(buf[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, hello().encoded_len());
        let mut cur = io::Cursor::new(buf);
        let a: Envelope = read(&mut cur).unwrap().unwrap();
        let b: Envelope = read(&mut cur).unwrap().unwrap();
        assert_eq!(a, hello());
        assert_eq!(b, hello());
        assert!(read::<_, Envelope>(&mut cur).unwrap().is_none());
    }

    #[test]
    fn truncated_is_error() {
        let buf = encode(&hello());
        let mut cur = io::Cursor::new(&buf[..buf.len() - 1]);
        assert!(read::<_, Envelope>(&mut cur).is_err());
    }

    #[test]
    fn too_large_rejected() {
        let mut cur = io::Cursor::new(vec![0xff, 0xff, 0xff, 0xff]);
        assert!(read::<_, Envelope>(&mut cur).is_err());
    }

    #[test]
    fn decoder_incremental() {
        let row = TextRow { y: 3, chars: vec![65, 66, 0], runs: vec![] };
        let data = encode(&row);
        let mut d = Decoder::new();
        for b in &data[..data.len() - 1] {
            d.push(&[*b]);
            assert!(d.next_msg::<TextRow>().unwrap().is_none());
        }
        d.push(&data[data.len() - 1..]);
        assert_eq!(d.next_msg::<TextRow>().unwrap().unwrap(), row);
    }

    #[tokio::test]
    async fn async_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(64);
        let h = tokio::spawn(async move {
            for _ in 0..10 {
                write_async(&mut a, &hello()).await.unwrap();
            }
        });
        for _ in 0..10 {
            let m: Envelope = read_async(&mut b).await.unwrap().unwrap();
            assert_eq!(m, hello());
        }
        h.await.unwrap();
        assert!(read_async::<_, Envelope>(&mut b).await.unwrap().is_none());
    }
}
