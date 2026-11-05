//! H.264 encoding with OpenH264 (software, bundled, no system libraries).

use anyhow::Result;
use openh264::OpenH264API;
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode,
    UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};

use super::capture::Frame;

/// OpenH264 handles at most 3840x2160.
const MAX_W: usize = 3840;
const MAX_H: usize = 2160;

pub struct Encoded {
    pub width: u32,
    pub height: u32,
    pub keyframe: bool,
    pub data: Vec<u8>,
}

pub struct VideoEncoder {
    enc: Encoder,
    yuv: Option<YUVBuffer>,
    resized: Vec<u8>,
}

impl VideoEncoder {
    pub fn new(bitrate_kbps: u32, fps: u32) -> Result<Self> {
        let cfg = EncoderConfig::new()
            .usage_type(UsageType::ScreenContentRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .bitrate(BitRate::from_bps(bitrate_kbps.saturating_mul(1000)))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .skip_frames(false)
            // A fresh keyframe every 10 s caps how long a glitch can last.
            .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 10));
        let enc = Encoder::with_api_config(OpenH264API::from_source(), cfg)?;
        Ok(VideoEncoder {
            enc,
            yuv: None,
            resized: Vec::new(),
        })
    }

    pub fn force_keyframe(&mut self) {
        self.enc.force_intra_frame();
    }

    pub fn encode(&mut self, frame: &Frame) -> Result<Option<Encoded>> {
        // The encoder wants even dimensions within its limits: shrink by an
        // integer factor if the screen is too big, then drop an odd edge.
        let factor = (frame.width.div_ceil(MAX_W))
            .max(frame.height.div_ceil(MAX_H))
            .max(1);
        let w = (frame.width / factor) & !1;
        let h = (frame.height / factor) & !1;
        if w == 0 || h == 0 {
            return Ok(None);
        }
        let pixels: &[u8] = if factor == 1 && w == frame.width && h == frame.height {
            &frame.bgra
        } else {
            self.resized.resize(w * h * 4, 0);
            for y in 0..h {
                let src_row = &frame.bgra[(y * factor) * frame.width * 4..];
                let dst_row = &mut self.resized[y * w * 4..(y + 1) * w * 4];
                if factor == 1 {
                    dst_row.copy_from_slice(&src_row[..w * 4]);
                } else {
                    for x in 0..w {
                        dst_row[x * 4..x * 4 + 4]
                            .copy_from_slice(&src_row[x * factor * 4..x * factor * 4 + 4]);
                    }
                }
            }
            &self.resized
        };

        let yuv = match &mut self.yuv {
            Some(b) if (b_dims(b)) == (w, h) => b,
            slot => slot.insert(YUVBuffer::new(w, h)),
        };
        yuv.read_bgra8(BgraSliceU8::new(pixels, (w, h)));
        let bs = self.enc.encode(yuv)?;
        let keyframe = matches!(bs.frame_type(), FrameType::IDR | FrameType::I);
        if matches!(bs.frame_type(), FrameType::Skip | FrameType::Invalid) {
            return Ok(None);
        }
        let data = bs.to_vec();
        if data.is_empty() {
            return Ok(None);
        }
        Ok(Some(Encoded {
            width: w as u32,
            height: h as u32,
            keyframe,
            data,
        }))
    }
}

fn b_dims(b: &YUVBuffer) -> (usize, usize) {
    use openh264::formats::YUVSource;
    b.dimensions()
}
