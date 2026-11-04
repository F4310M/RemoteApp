//! Wire protocol shared by host and client.
//!
//! Every message on the control stream is a `u32` little-endian length
//! followed by a bincode-encoded [`ClientMsg`] or [`HostMsg`].
//! Video travels on a separate unidirectional stream opened by the host,
//! one [`VideoFrame`] header + H.264 Annex-B payload per frame.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL_VERSION: u32 = 1;
pub const ALPN: &[u8] = b"remoteapp/1";
pub const DEFAULT_PORT: u16 = 47800;
/// Upper bound for a single control message (keeps a broken peer from
/// making us allocate gigabytes).
const MAX_CONTROL_LEN: u32 = 64 * 1024;
/// Upper bound for one encoded video frame.
pub const MAX_FRAME_LEN: u32 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    /// Absolute position as a fraction of the host screen,
    /// 0 = left/top edge, 65535 = right/bottom edge.
    MouseMove { x: u16, y: u16 },
    MouseButton { button: MouseButton, down: bool },
    /// Wheel steps; positive `dy` scrolls down, positive `dx` scrolls right.
    Scroll { dx: i32, dy: i32 },
    /// Physical key, as a Linux evdev code (see `keymap.rs`).
    Key { code: u16, down: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello { version: u32, password: String },
    Input(InputEvent),
    /// The decoder lost sync (or just started): send an IDR frame.
    RequestKeyframe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostMsg {
    Welcome { screen_width: u32, screen_height: u32, host_name: String },
    Rejected { reason: String },
}

pub async fn write_msg<W: AsyncWrite + Unpin, T: Serialize>(w: &mut W, msg: &T) -> Result<()> {
    let body = bincode::serialize(msg)?;
    w.write_all(&(body.len() as u32).to_le_bytes()).await?;
    w.write_all(&body).await?;
    Ok(())
}

/// Returns `Ok(None)` when the stream ended cleanly.
pub async fn read_msg<R: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(r: &mut R) -> Result<Option<T>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len);
    if len > MAX_CONTROL_LEN {
        bail!("control message too large ({len} bytes)");
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await?;
    Ok(Some(bincode::deserialize(&body).context("malformed control message")?))
}

/// Header written before each encoded frame on the video stream.
#[derive(Debug, Clone, Copy)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub keyframe: bool,
    pub len: u32,
}

impl VideoFrame {
    const SIZE: usize = 13;

    pub async fn write<W: AsyncWrite + Unpin>(&self, w: &mut W, payload: &[u8]) -> Result<()> {
        let mut hdr = [0u8; Self::SIZE];
        hdr[0..4].copy_from_slice(&self.width.to_le_bytes());
        hdr[4..8].copy_from_slice(&self.height.to_le_bytes());
        hdr[8] = self.keyframe as u8;
        hdr[9..13].copy_from_slice(&self.len.to_le_bytes());
        w.write_all(&hdr).await?;
        w.write_all(payload).await?;
        Ok(())
    }

    pub async fn read<R: AsyncRead + Unpin>(r: &mut R, buf: &mut Vec<u8>) -> Result<Option<Self>> {
        let mut hdr = [0u8; Self::SIZE];
        match r.read_exact(&mut hdr).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let f = VideoFrame {
            width: u32::from_le_bytes(hdr[0..4].try_into().unwrap()),
            height: u32::from_le_bytes(hdr[4..8].try_into().unwrap()),
            keyframe: hdr[8] != 0,
            len: u32::from_le_bytes(hdr[9..13].try_into().unwrap()),
        };
        if f.len > MAX_FRAME_LEN {
            bail!("video frame too large ({} bytes)", f.len);
        }
        buf.resize(f.len as usize, 0);
        r.read_exact(buf).await?;
        Ok(Some(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn control_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        write_msg(&mut a, &ClientMsg::Input(InputEvent::Key { code: 30, down: true })).await.unwrap();
        drop(a);
        let m: ClientMsg = read_msg(&mut b).await.unwrap().unwrap();
        assert!(matches!(m, ClientMsg::Input(InputEvent::Key { code: 30, down: true })));
        assert!(read_msg::<_, ClientMsg>(&mut b).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn video_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let payload = vec![0, 0, 0, 1, 0x67, 1, 2, 3];
        let f = VideoFrame { width: 640, height: 480, keyframe: true, len: payload.len() as u32 };
        f.write(&mut a, &payload).await.unwrap();
        let mut buf = Vec::new();
        let g = VideoFrame::read(&mut b, &mut buf).await.unwrap().unwrap();
        assert_eq!((g.width, g.height, g.keyframe), (640, 480, true));
        assert_eq!(buf, payload);
    }
}
