//! Pictures into H.264, HEVC or AV1, one frame in and one packet out.
//!
//! The host renders each picture straight into a texture the encoder
//! handed it (Windows, hardware encoders), or writes it into memory
//! (x264, and the tests). Nothing waits: no B-frames, no look-ahead, a
//! key frame only when asked for, and a rate whose buffer holds one
//! frame. The settings of each backend are in `tuning`.

use std::ffi::c_int;
use std::sync::Arc;

use zyr_media::codec::VideoCodec;

#[cfg(windows)]
use crate::d3d11::{GpuFrame, Surfaces};
use crate::error::CodecError;
use crate::library::Ffmpeg;
use crate::owned::{CodecContext, OwnedFrame, OwnedPacket};
use crate::sys;
use crate::tuning;

/// Which encoder does the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// NVIDIA's.
    Nvenc,
    /// AMD's.
    Amf,
    /// Intel's Quick Sync.
    Qsv,
    /// Whatever Windows offers itself, Qualcomm's among others.
    MediaFoundation,
    /// x264, on the processor: H.264 only.
    Software,
}

impl Backend {
    /// FFmpeg's name for this backend's encoder of `codec`, if it has
    /// one.
    pub fn encoder_name(self, codec: VideoCodec) -> Option<&'static str> {
        Some(match (self, codec) {
            (Backend::Nvenc, VideoCodec::H264) => "h264_nvenc",
            (Backend::Nvenc, VideoCodec::Hevc) => "hevc_nvenc",
            (Backend::Nvenc, VideoCodec::Av1) => "av1_nvenc",
            (Backend::Amf, VideoCodec::H264) => "h264_amf",
            (Backend::Amf, VideoCodec::Hevc) => "hevc_amf",
            (Backend::Amf, VideoCodec::Av1) => "av1_amf",
            (Backend::Qsv, VideoCodec::H264) => "h264_qsv",
            (Backend::Qsv, VideoCodec::Hevc) => "hevc_qsv",
            (Backend::Qsv, VideoCodec::Av1) => "av1_qsv",
            (Backend::MediaFoundation, VideoCodec::H264) => "h264_mf",
            (Backend::MediaFoundation, VideoCodec::Hevc) => "hevc_mf",
            (Backend::MediaFoundation, VideoCodec::Av1) => "av1_mf",
            (Backend::Software, VideoCodec::H264) => "libx264",
            (Backend::Software, VideoCodec::Hevc | VideoCodec::Av1) => return None,
        })
    }

    /// The name people know it by.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Nvenc => "NVENC",
            Backend::Amf => "AMF",
            Backend::Qsv => "Quick Sync",
            Backend::MediaFoundation => "Media Foundation",
            Backend::Software => "x264",
        }
    }

    /// Whether a new rate reaches this encoder between two frames
    /// (FFmpeg forwards it for NVENC, Quick Sync and x264).
    fn changes_rate_in_place(self) -> bool {
        matches!(self, Backend::Nvenc | Backend::Qsv | Backend::Software)
    }
}

/// What the encoder is given its pictures in.
#[derive(Clone)]
pub enum Input {
    /// NV12 textures of this Direct3D 11 device, rendered into by the
    /// caller: the picture never leaves the graphics card. FFmpeg wants
    /// the device created with video support.
    #[cfg(windows)]
    D3d11 {
        device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    },
    /// NV12 pictures in memory.
    Cpu,
}

/// What an encoder is opened for.
#[derive(Clone)]
pub struct EncoderConfig {
    pub codec: VideoCodec,
    /// Even, like everything 4:2:0.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub backend: Backend,
    pub input: Input,
}

/// One encoded frame, in Annex B (H.264, HEVC) or as a temporal unit
/// (AV1), copied out of FFmpeg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    /// A key frame: a player can start from it.
    pub key: bool,
}

/// What became of a new rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Taken from the next frame on.
    InPlace,
    /// This encoder has to be opened again to take it.
    NeedsRebuild,
}

/// A picture for an encoder that reads memory: NV12, written by the
/// caller through [`CpuFrame::planes`].
pub struct CpuFrame {
    frame: OwnedFrame,
}

/// The two planes of an NV12 picture: luma, then chroma with U and V
/// interleaved, at half the height. Rows start every `stride` bytes.
pub struct Nv12Planes<'a> {
    pub luma: &'a mut [u8],
    pub luma_stride: usize,
    pub chroma: &'a mut [u8],
    pub chroma_stride: usize,
}

impl CpuFrame {
    pub fn width(&self) -> u32 {
        self.frame.get().width.unsigned_abs()
    }

    pub fn height(&self) -> u32 {
        self.frame.get().height.unsigned_abs()
    }

    pub fn planes(&mut self) -> Nv12Planes<'_> {
        let height = self.height() as usize;
        let frame = self.frame.get();
        let luma_stride = frame.linesize[0].unsigned_abs() as usize;
        let chroma_stride = frame.linesize[1].unsigned_abs() as usize;
        let (luma, chroma) = (frame.data[0], frame.data[1]);
        // SAFETY: av_frame_get_buffer gave each plane at least `stride`
        // bytes for each of its rows, the two planes do not overlap, and
        // the frame is ours alone until it is handed to the encoder,
        // which takes it by value.
        unsafe {
            Nv12Planes {
                luma: std::slice::from_raw_parts_mut(luma, luma_stride * height),
                luma_stride,
                chroma: std::slice::from_raw_parts_mut(chroma, chroma_stride * height.div_ceil(2)),
                chroma_stride,
            }
        }
    }
}

/// A picture on its way to an encoder.
pub enum Frame {
    Cpu(CpuFrame),
    #[cfg(windows)]
    Gpu(GpuFrame),
}

impl From<CpuFrame> for Frame {
    fn from(frame: CpuFrame) -> Self {
        Frame::Cpu(frame)
    }
}

#[cfg(windows)]
impl From<GpuFrame> for Frame {
    fn from(frame: GpuFrame) -> Self {
        Frame::Gpu(frame)
    }
}

/// An open video encoder.
pub struct VideoEncoder {
    ff: Arc<Ffmpeg>,
    context: CodecContext,
    packet: OwnedPacket,
    codec: VideoCodec,
    backend: Backend,
    width: u32,
    height: u32,
    fps: u32,
    slices: c_int,
    next_pts: i64,
    /// The textures it is fed from, when it reads a graphics card.
    #[cfg(windows)]
    surfaces: Option<Surfaces>,
}

impl VideoEncoder {
    pub fn open(ff: &Arc<Ffmpeg>, config: EncoderConfig) -> Result<Self, CodecError> {
        let name = checked(&config)?;
        #[cfg(windows)]
        let surfaces = match &config.input {
            Input::D3d11 { device } => Some(Surfaces::new(
                ff,
                device,
                config.width,
                config.height,
                config.backend == Backend::Qsv,
            )?),
            Input::Cpu => None,
        };
        let slices = if config.backend == Backend::Software {
            tuning::software_threads()
        } else {
            1
        };

        let options = tuning::options(config.backend, config.codec);
        let attempt = |options: &[(&str, &str)]| {
            let mut context = CodecContext::encoder(ff, name)?;
            configure(context.fields(), &config, slices);
            #[cfg(windows)]
            if let Some(surfaces) = &surfaces {
                let fields = context.fields();
                fields.pix_fmt = surfaces.pixel_format();
                fields.hw_frames_ctx = surfaces.frames_for_encoder()?;
            }
            context.open(options)?;
            Ok::<_, CodecError>(context)
        };
        let context = match (attempt(&options), tuning::fallback(config.backend)) {
            (Ok(context), _) => context,
            (Err(_), Some(fallback)) => attempt(&tuning::changed(&options, fallback))?,
            (Err(refused), None) => return Err(refused),
        };

        Ok(Self {
            ff: Arc::clone(ff),
            context,
            packet: OwnedPacket::new(ff)?,
            codec: config.codec,
            backend: config.backend,
            width: config.width,
            height: config.height,
            fps: config.fps,
            slices,
            next_pts: 0,
            #[cfg(windows)]
            surfaces,
        })
    }

    pub fn codec(&self) -> VideoCodec {
        self.codec
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// A texture from the encoder's own pool to render the next picture
    /// into: NV12, the encoder's size, bound as a render target.
    #[cfg(windows)]
    pub fn frame_for_gpu(&self) -> Result<GpuFrame, CodecError> {
        self.surfaces()?.frame(&self.ff)
    }

    /// A picture in memory to write the next frame into.
    pub fn frame_for_cpu(&self) -> Result<CpuFrame, CodecError> {
        self.reads_memory()?;
        let mut frame = OwnedFrame::new(&self.ff)?;
        let fields = frame.fields();
        fields.format = sys::AVPixelFormat::AV_PIX_FMT_NV12.0;
        fields.width = self.width as c_int;
        fields.height = self.height as c_int;
        frame.allocate()?;
        Ok(CpuFrame { frame })
    }

    /// Hands a picture to the encoder, as a key frame if `force_key`.
    ///
    /// Its packet comes out of [`VideoEncoder::receive`], which is to be
    /// called until it says there is nothing more.
    ///
    /// The picture must be one this encoder handed out (in memory, one of
    /// its size will do): the encoder reads as many rows as it was opened
    /// for, and would read past a smaller picture.
    pub fn encode(&mut self, frame: Frame, force_key: bool) -> Result<(), CodecError> {
        let mut frame = match frame {
            Frame::Cpu(frame) => {
                self.reads_memory()?;
                if (frame.width(), frame.height()) != (self.width, self.height) {
                    return Err(CodecError::Invalid(format!(
                        "une image de {} sur {} ne va pas à un encodeur de {} sur {}",
                        frame.width(),
                        frame.height(),
                        self.width,
                        self.height
                    )));
                }
                frame.frame
            }
            #[cfg(windows)]
            Frame::Gpu(frame) => self.surfaces()?.for_encoder(&self.ff, frame)?,
        };
        let fields = frame.fields();
        fields.pts = self.next_pts;
        if force_key {
            fields.pict_type = sys::AVPictureType::AV_PICTURE_TYPE_I;
            fields.flags |= sys::AV_FRAME_FLAG_KEY as c_int;
        } else {
            fields.pict_type = sys::AVPictureType::AV_PICTURE_TYPE_NONE;
        }
        self.next_pts += 1;
        self.context.send_frame(&frame)
    }

    /// The next packet, if one is ready.
    pub fn receive(&mut self) -> Result<Option<EncodedPacket>, CodecError> {
        if !self.context.receive_packet(&mut self.packet)? {
            return Ok(None);
        }
        Ok(Some(EncodedPacket {
            data: self.packet.to_vec(),
            key: self.packet.get().flags & sys::AV_PKT_FLAG_KEY as c_int != 0,
        }))
    }

    /// Asks for another rate, from the next frame on when the encoder
    /// allows it.
    ///
    /// NVENC starts its stream again at the new rate: its next packet is
    /// a key frame, and flagged so.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<Applied, CodecError> {
        if kbps == 0 {
            return Err(CodecError::Invalid("un débit nul".to_string()));
        }
        if !self.backend.changes_rate_in_place() {
            return Ok(Applied::NeedsRebuild);
        }
        let rates = tuning::rates(self.backend, kbps, self.fps, self.slices);
        let fields = self.context.fields();
        fields.bit_rate = rates.bit_rate;
        fields.rc_max_rate = rates.max_rate;
        fields.rc_buffer_size = rates.buffer;
        Ok(Applied::InPlace)
    }

    /// A picture to try the encoder with: black in memory, or whatever a
    /// fresh texture holds.
    pub(crate) fn blank_frame(&self) -> Result<Frame, CodecError> {
        #[cfg(windows)]
        if self.surfaces.is_some() {
            return self.frame_for_gpu().map(Frame::Gpu);
        }
        let mut frame = self.frame_for_cpu()?;
        let planes = frame.planes();
        planes.luma.fill(16);
        planes.chroma.fill(128);
        Ok(Frame::Cpu(frame))
    }

    /// Tells the encoder no frame will follow, so it hands over what it
    /// still holds.
    pub(crate) fn finish(&mut self) -> Result<(), CodecError> {
        self.context.send_end()
    }

    #[cfg(windows)]
    fn surfaces(&self) -> Result<&Surfaces, CodecError> {
        self.surfaces.as_ref().ok_or_else(|| {
            CodecError::Invalid("cet encodeur lit la mémoire, pas des textures".to_string())
        })
    }

    fn reads_memory(&self) -> Result<(), CodecError> {
        #[cfg(windows)]
        if self.surfaces.is_some() {
            return Err(CodecError::Invalid(
                "cet encodeur lit des textures, pas la mémoire".to_string(),
            ));
        }
        Ok(())
    }
}

/// FFmpeg's name for the encoder, once the configuration is known to
/// make sense.
fn checked(config: &EncoderConfig) -> Result<&'static str, CodecError> {
    let EncoderConfig {
        codec,
        width,
        height,
        fps,
        bitrate_kbps,
        backend,
        ..
    } = *config;
    let name = backend.encoder_name(codec).ok_or_else(|| {
        CodecError::Invalid(format!(
            "{} ne sait pas encoder en {}",
            backend.name(),
            codec.name()
        ))
    })?;
    let fits = |value: u32| value > 0 && value.is_multiple_of(2) && c_int::try_from(value).is_ok();
    if !fits(width) || !fits(height) {
        return Err(CodecError::Invalid(format!(
            "une image de {width} sur {height} ne s'encode pas : il faut des dimensions paires"
        )));
    }
    if fps == 0 || c_int::try_from(fps).is_err() || bitrate_kbps == 0 {
        return Err(CodecError::Invalid(format!(
            "{fps} images par seconde à {bitrate_kbps} kb/s ne s'encodent pas"
        )));
    }
    #[cfg(windows)]
    if backend == Backend::Software && matches!(config.input, Input::D3d11 { .. }) {
        return Err(CodecError::Invalid(
            "x264 lit la mémoire, pas des textures".to_string(),
        ));
    }
    Ok(name)
}

/// What every backend shares, set on the codec context.
fn configure(fields: &mut sys::AVCodecContext, config: &EncoderConfig, slices: c_int) {
    let rates = tuning::rates(config.backend, config.bitrate_kbps, config.fps, slices);
    let fps = config.fps as c_int;
    fields.width = config.width as c_int;
    fields.height = config.height as c_int;
    fields.time_base = sys::AVRational { num: 1, den: fps };
    fields.framerate = sys::AVRational { num: fps, den: 1 };
    fields.pix_fmt = sys::AVPixelFormat::AV_PIX_FMT_NV12;
    fields.sw_pix_fmt = sys::AVPixelFormat::AV_PIX_FMT_NV12;
    fields.bit_rate = rates.bit_rate;
    fields.rc_max_rate = rates.max_rate;
    fields.rc_buffer_size = rates.buffer;
    fields.gop_size = tuning::gop(config.backend);
    fields.keyint_min = fields.gop_size;
    if let Some(compliance) = tuning::compliance(config.backend) {
        fields.strict_std_compliance = compliance;
    }
    fields.max_b_frames = 0;
    fields.flags |= (sys::AV_CODEC_FLAG_LOW_DELAY | sys::AV_CODEC_FLAG_CLOSED_GOP) as c_int;
    fields.color_range = sys::AVColorRange::AVCOL_RANGE_MPEG;
    fields.color_primaries = sys::AVColorPrimaries::AVCOL_PRI_BT709;
    fields.color_trc = sys::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
    fields.colorspace = sys::AVColorSpace::AVCOL_SPC_BT709;
    if config.backend == Backend::Software {
        fields.thread_type = sys::FF_THREAD_SLICE as c_int;
        fields.thread_count = slices;
    } else {
        fields.slices = slices;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use crate::{DecodeOutput, DecodedFrame, VideoDecoder};

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 240;

    fn x264_sized(width: u32, height: u32, kbps: u32) -> VideoEncoder {
        VideoEncoder::open(
            &testing::ffmpeg(),
            EncoderConfig {
                codec: VideoCodec::H264,
                width,
                height,
                fps: 30,
                bitrate_kbps: kbps,
                backend: Backend::Software,
                input: Input::Cpu,
            },
        )
        .unwrap()
    }

    fn x264(kbps: u32) -> VideoEncoder {
        x264_sized(WIDTH, HEIGHT, kbps)
    }

    /// Luma of the synthetic picture number `index`: a diagonal gradient
    /// that slides one pixel a frame, and in the top left corner a block
    /// whose brightness says which frame it is.
    fn luma(index: usize, x: usize, y: usize) -> u8 {
        if x < 32 && y < 32 {
            return marker(index);
        }
        (16 + (x + y + index) % 220) as u8
    }

    fn marker(index: usize) -> u8 {
        (20 + index * 7) as u8
    }

    fn picture(encoder: &VideoEncoder, index: usize) -> Frame {
        let mut frame = encoder.frame_for_cpu().unwrap();
        let planes = frame.planes();
        for y in 0..HEIGHT as usize {
            for x in 0..WIDTH as usize {
                planes.luma[y * planes.luma_stride + x] = luma(index, x, y);
            }
        }
        for y in 0..HEIGHT as usize / 2 {
            for x in 0..WIDTH as usize {
                planes.chroma[y * planes.chroma_stride + x] = 128;
            }
        }
        Frame::Cpu(frame)
    }

    /// Picture after picture of noise, which no encoder can shrink much.
    fn noise(encoder: &VideoEncoder, seed: u32) -> Frame {
        let mut frame = encoder.frame_for_cpu().unwrap();
        let planes = frame.planes();
        let mut state = seed.wrapping_mul(2_654_435_761).max(1);
        for byte in planes.luma.iter_mut().chain(planes.chroma.iter_mut()) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = 16 + (state % 220) as u8;
        }
        Frame::Cpu(frame)
    }

    fn encode_one(encoder: &mut VideoEncoder, frame: Frame, key: bool) -> EncodedPacket {
        encoder.encode(frame, key).unwrap();
        let packet = encoder
            .receive()
            .unwrap()
            .expect("a packet for every frame");
        assert!(encoder.receive().unwrap().is_none(), "one packet a frame");
        packet
    }

    fn psnr(decoded: &crate::CpuPicture, index: usize) -> f64 {
        let luma = &decoded.planes()[0];
        let mut error = 0.0;
        for y in 0..HEIGHT as usize {
            for x in 0..WIDTH as usize {
                let difference =
                    f64::from(luma.data[y * luma.stride + x]) - f64::from(self::luma(index, x, y));
                error += difference * difference;
            }
        }
        let mean = error / f64::from(WIDTH * HEIGHT);
        10.0 * (255.0 * 255.0 / mean.max(1e-9)).log10()
    }

    #[test]
    fn thirty_frames_come_back_from_the_decoder_in_order_and_intact() {
        let ff = testing::ffmpeg();
        let mut encoder = x264(2_000);
        let mut decoder = VideoDecoder::open(&ff, VideoCodec::H264, DecodeOutput::Cpu).unwrap();

        for index in 0..30 {
            let frame = picture(&encoder, index);
            let packet = encode_one(&mut encoder, frame, index == 0);
            assert_eq!(packet.key, index == 0, "frame {index}");
            let Some(DecodedFrame::Cpu(decoded)) = decoder.decode(&packet.data).unwrap() else {
                panic!("frame {index} did not come out of the decoder at once");
            };
            assert_eq!((decoded.width(), decoded.height()), (WIDTH, HEIGHT));
            let corner = decoded.planes()[0].data[8 * decoded.planes()[0].stride + 8];
            assert!(
                corner.abs_diff(marker(index)) <= 2,
                "frame {index} came out with the marker of another: {corner}"
            );
            let quality = psnr(&decoded, index);
            assert!(quality > 35.0, "frame {index}: {quality:.1} dB");
        }
        assert_eq!(decoder.replaced(), 0);
    }

    #[test]
    fn a_key_frame_comes_when_asked_and_only_then() {
        let mut encoder = x264(1_000);
        for index in 0..12 {
            let key = index == 0 || index == 7;
            let frame = picture(&encoder, index);
            let packet = encode_one(&mut encoder, frame, key);
            assert_eq!(packet.key, key, "frame {index}");
        }
    }

    #[test]
    fn a_new_rate_reaches_x264_between_two_frames() {
        let mut encoder = x264(300);
        let sizes = |encoder: &mut VideoEncoder, seed: u32| -> usize {
            (0..20)
                .map(|index| {
                    let frame = noise(encoder, seed + index);
                    encode_one(encoder, frame, index == 0 && seed == 0)
                        .data
                        .len()
                })
                .skip(5)
                .sum()
        };
        let low = sizes(&mut encoder, 0);
        assert_eq!(encoder.set_bitrate(3_000).unwrap(), Applied::InPlace);
        let high = sizes(&mut encoder, 100);
        assert!(high > low * 4, "{low} bytes at 300 kb/s, {high} at 3000");
    }

    #[test]
    fn a_rate_of_nothing_is_refused() {
        let mut encoder = x264(1_000);
        assert!(matches!(
            encoder.set_bitrate(0),
            Err(CodecError::Invalid(_))
        ));
    }

    #[test]
    fn a_picture_of_another_size_is_refused_before_the_encoder_reads_it() {
        let mut encoder = x264(1_000);
        let larger = x264_sized(WIDTH * 2, HEIGHT * 2, 1_000);
        let smaller = x264_sized(WIDTH / 2, HEIGHT / 2, 1_000);
        for other in [larger, smaller] {
            let stranger = other.frame_for_cpu().unwrap();
            let refused = encoder.encode(stranger.into(), false);
            assert!(
                matches!(refused, Err(CodecError::Invalid(_))),
                "{:?}",
                refused.err()
            );
        }
        // Nothing reached the encoder: its own first picture is still
        // the first frame of its stream.
        let frame = picture(&encoder, 0);
        assert!(encode_one(&mut encoder, frame, true).key);
    }

    #[test]
    fn some_encoders_have_to_be_opened_again_for_a_new_rate() {
        assert!(Backend::Nvenc.changes_rate_in_place());
        assert!(Backend::Qsv.changes_rate_in_place());
        assert!(!Backend::Amf.changes_rate_in_place());
        assert!(!Backend::MediaFoundation.changes_rate_in_place());
    }

    #[test]
    fn impossible_configurations_are_refused_before_ffmpeg_sees_them() {
        let ff = testing::ffmpeg();
        let config = EncoderConfig {
            codec: VideoCodec::H264,
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            bitrate_kbps: 1_000,
            backend: Backend::Software,
            input: Input::Cpu,
        };
        let refused = |config: EncoderConfig| match VideoEncoder::open(&ff, config) {
            Err(CodecError::Invalid(reason)) => reason,
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("opened"),
        };
        refused(EncoderConfig {
            width: 321,
            ..config.clone()
        });
        refused(EncoderConfig {
            fps: 0,
            ..config.clone()
        });
        let hevc = refused(EncoderConfig {
            codec: VideoCodec::Hevc,
            ..config.clone()
        });
        assert!(hevc.contains("HEVC"), "{hevc}");
    }

    #[test]
    fn an_encoder_this_ffmpeg_lacks_is_named() {
        let refused = VideoEncoder::open(
            &testing::ffmpeg(),
            EncoderConfig {
                codec: VideoCodec::Hevc,
                width: WIDTH,
                height: HEIGHT,
                fps: 30,
                bitrate_kbps: 1_000,
                backend: Backend::Nvenc,
                input: Input::Cpu,
            },
        );
        // The Linux build carries no hardware encoder.
        assert!(
            matches!(
                refused,
                Err(CodecError::Missing {
                    codec: "hevc_nvenc"
                })
            ),
            "{:?}",
            refused.err()
        );
    }
}
