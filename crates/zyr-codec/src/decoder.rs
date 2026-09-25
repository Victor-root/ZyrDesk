//! Packets back into pictures, each one out as soon as its packet is in.
//!
//! On Windows the graphics card decodes, into textures of the player's
//! own Direct3D 11 device, and never falls back to the processor behind
//! anyone's back. The processor decodes for the tests and for headless
//! players.

use std::ffi::c_int;
use std::sync::Arc;

use zyr_media::codec::VideoCodec;

#[cfg(windows)]
use crate::d3d11::{D3d11Picture, Pictures};
use crate::error::CodecError;
use crate::library::Ffmpeg;
use crate::owned::{CodecContext, LentPacket, OwnedFrame};
use crate::sys;

/// Where decoded pictures go.
#[derive(Clone)]
pub enum DecodeOutput {
    /// Textures of this Direct3D 11 device, which must have been created
    /// with video support.
    #[cfg(windows)]
    D3d11 {
        device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    },
    /// Memory.
    Cpu,
}

impl DecodeOutput {
    fn on_card(&self) -> bool {
        match self {
            #[cfg(windows)]
            DecodeOutput::D3d11 { .. } => true,
            DecodeOutput::Cpu => false,
        }
    }
}

/// A decoded picture.
pub enum DecodedFrame {
    #[cfg(windows)]
    D3d11(D3d11Picture),
    Cpu(CpuPicture),
}

/// A picture decoded into memory, as FFmpeg's software decoders lay it
/// out.
pub struct CpuPicture {
    frame: OwnedFrame,
    layout: PictureLayout,
}

/// How the planes of a picture in memory are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureLayout {
    /// Three planes: luma, U, V, the last two at half size each way.
    Yuv420p,
    /// Two planes: luma, then U and V interleaved at half the height.
    Nv12,
}

/// One plane of a picture: rows of bytes, each starting `stride` bytes
/// after the previous one.
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

impl CpuPicture {
    fn new(frame: OwnedFrame) -> Result<Self, CodecError> {
        let fields = frame.get();
        let layout = match sys::AVPixelFormat(fields.format) {
            sys::AVPixelFormat::AV_PIX_FMT_YUV420P => PictureLayout::Yuv420p,
            sys::AVPixelFormat::AV_PIX_FMT_NV12 => PictureLayout::Nv12,
            other => {
                return Err(CodecError::Invalid(format!(
                    "le décodeur a rendu une image dans un format inattendu ({})",
                    other.0
                )));
            }
        };
        let planes = match layout {
            PictureLayout::Yuv420p => 3,
            PictureLayout::Nv12 => 2,
        };
        if fields.linesize[..planes].iter().any(|stride| *stride <= 0) {
            return Err(CodecError::Invalid(
                "le décodeur a rendu une image retournée".to_string(),
            ));
        }
        Ok(Self { frame, layout })
    }

    pub fn width(&self) -> u32 {
        self.frame.get().width.unsigned_abs()
    }

    pub fn height(&self) -> u32 {
        self.frame.get().height.unsigned_abs()
    }

    pub fn layout(&self) -> PictureLayout {
        self.layout
    }

    /// The planes, luma first.
    pub fn planes(&self) -> Vec<Plane<'_>> {
        let frame = self.frame.get();
        let height = self.height() as usize;
        let rows = match self.layout {
            PictureLayout::Yuv420p => vec![height, height.div_ceil(2), height.div_ceil(2)],
            PictureLayout::Nv12 => vec![height, height.div_ceil(2)],
        };
        rows.into_iter()
            .enumerate()
            .map(|(index, rows)| {
                let stride = frame.linesize[index].unsigned_abs() as usize;
                // SAFETY: a decoded plane holds `stride` bytes for each of
                // its rows (strides were checked positive), alive as long
                // as the frame, which this borrow keeps.
                let data = unsafe { std::slice::from_raw_parts(frame.data[index], stride * rows) };
                Plane { data, stride }
            })
            .collect()
    }
}

/// An open video decoder.
pub struct VideoDecoder {
    ff: Arc<Ffmpeg>,
    context: CodecContext,
    packet: LentPacket,
    /// A frame to receive into, kept from one call to the next.
    spare: Option<OwnedFrame>,
    replaced: u64,
    #[cfg(windows)]
    pictures: Option<Pictures>,
}

impl VideoDecoder {
    pub fn open(
        ff: &Arc<Ffmpeg>,
        codec: VideoCodec,
        output: DecodeOutput,
    ) -> Result<Self, CodecError> {
        if codec == VideoCodec::Av1 && !output.on_card() {
            return Err(CodecError::Invalid(
                "FFmpeg ne décode l'AV1 que par la carte graphique".to_string(),
            ));
        }
        let mut context = CodecContext::decoder(ff, decoder_name(codec))?;
        let fields = context.fields();
        // Every picture out as soon as it is decoded, never held back to
        // be reordered, and no threads, each of which would hold a
        // frame of its own.
        fields.flags |= sys::AV_CODEC_FLAG_LOW_DELAY as c_int;
        fields.thread_count = 1;
        // A damaged picture is an error, not a picture: the player asks
        // for a key frame rather than showing what came out.
        fields.err_recognition |= sys::AV_EF_EXPLODE as c_int;
        #[cfg(windows)]
        let pictures = match output {
            DecodeOutput::D3d11 { device } => Some(Pictures::attach(ff, &mut context, &device)?),
            DecodeOutput::Cpu => None,
        };
        context.open(&[])?;
        Ok(Self {
            ff: Arc::clone(ff),
            context,
            packet: LentPacket::new(ff)?,
            spare: None,
            replaced: 0,
            #[cfg(windows)]
            pictures,
        })
    }

    /// Decodes one packet, and hands over the picture it gave, if any.
    ///
    /// Should a packet ever give more than one, the newest is handed over
    /// and the others counted in [`VideoDecoder::replaced`].
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<DecodedFrame>, CodecError> {
        self.context.send_bytes(&mut self.packet, data)?;
        let mut newest: Option<OwnedFrame> = None;
        let mut frame = match self.spare.take() {
            Some(frame) => frame,
            None => OwnedFrame::new(&self.ff)?,
        };
        while self.context.receive_frame(&mut frame)? {
            frame = match newest.replace(frame) {
                // Emptied by the next receive, which lets its picture go.
                Some(older) => {
                    self.replaced += 1;
                    older
                }
                None => OwnedFrame::new(&self.ff)?,
            };
        }
        self.spare = Some(frame);
        newest.map(|frame| self.picture(frame)).transpose()
    }

    /// Pictures decoded but never handed over, because a newer one came
    /// out of the same packet.
    pub fn replaced(&self) -> u64 {
        self.replaced
    }

    /// How the player gets at the decoded textures, when the graphics
    /// card decodes.
    #[cfg(windows)]
    pub fn sampling(&self) -> Option<crate::d3d11::Sampling> {
        self.pictures.as_ref().map(Pictures::sampling)
    }

    /// The codecs this device decodes in hardware.
    #[cfg(windows)]
    pub fn supported(
        ff: &Arc<Ffmpeg>,
        device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
    ) -> zyr_media::codec::CodecSet {
        crate::d3d11::supported(device, |codec| {
            CodecContext::decoder(ff, decoder_name(codec)).is_ok()
        })
    }

    fn picture(&mut self, frame: OwnedFrame) -> Result<DecodedFrame, CodecError> {
        #[cfg(windows)]
        if let Some(pictures) = &mut self.pictures {
            return pictures.picture(frame).map(DecodedFrame::D3d11);
        }
        CpuPicture::new(frame).map(DecodedFrame::Cpu)
    }
}

/// FFmpeg's own decoder of a codec, the one that can use the graphics
/// card.
fn decoder_name(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "h264",
        VideoCodec::Hevc => "hevc",
        VideoCodec::Av1 => "av1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn av1_is_never_decoded_on_the_processor() {
        let refused = VideoDecoder::open(&testing::ffmpeg(), VideoCodec::Av1, DecodeOutput::Cpu);
        assert!(matches!(refused, Err(CodecError::Invalid(_))));
    }

    #[test]
    fn a_damaged_packet_is_an_error_and_not_a_picture() {
        let mut decoder =
            VideoDecoder::open(&testing::ffmpeg(), VideoCodec::H264, DecodeOutput::Cpu).unwrap();
        // A slice of a picture whose parameter sets never came.
        let orphan = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x33, 0xff, 0x12, 0x34];
        assert!(!matches!(decoder.decode(&orphan), Ok(Some(_))));
    }

    #[test]
    fn an_empty_packet_is_refused_and_does_not_end_the_stream() {
        let ff = testing::ffmpeg();
        let mut decoder = VideoDecoder::open(&ff, VideoCodec::H264, DecodeOutput::Cpu).unwrap();
        assert!(matches!(decoder.decode(&[]), Err(CodecError::Invalid(_))));

        let mut encoder = crate::VideoEncoder::open(
            &ff,
            crate::EncoderConfig {
                codec: VideoCodec::H264,
                width: 64,
                height: 64,
                fps: 30,
                bitrate_kbps: 500,
                backend: crate::Backend::Software,
                input: crate::Input::Cpu,
            },
        )
        .unwrap();
        let frame = encoder.blank_frame().unwrap();
        encoder.encode(frame, true).unwrap();
        let packet = encoder.receive().unwrap().unwrap();
        assert!(matches!(
            decoder.decode(&packet.data),
            Ok(Some(DecodedFrame::Cpu(_)))
        ));
    }
}
