//! What the tests share: FFmpeg, loaded once, a journal, and pictures
//! and sound as the host engine sends them.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use zyr_codec::{
    Backend, DecodeOutput, DecodedFrame, EncoderConfig, Ffmpeg, Frame, Input, OPUS_FRAME,
    OpusEncoder, VideoDecoder, VideoEncoder,
};
use zyr_media::audio::write_audio;
use zyr_media::codec::VideoCodec;
use zyr_media::video::{OutgoingFrame, Packetizer, Packets};
use zyr_proto::log::Log;

use crate::audio::PACKET;
use crate::present::checksum;

/// Names the folder of a Linux build of FFmpeg for the tests.
const DIR_VARIABLE: &str = "ZYR_FFMPEG_DIR";

/// The size of the tests' pictures.
pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;

/// FFmpeg, from `ZYR_FFMPEG_DIR` or else `vendor/ffmpeg`.
///
/// A test that needs it fails when it cannot be loaded, saying where it
/// looked and how to get it, rather than passing without having run.
pub fn ffmpeg() -> Arc<Ffmpeg> {
    static LOADED: OnceLock<Arc<Ffmpeg>> = OnceLock::new();
    Arc::clone(LOADED.get_or_init(|| {
        let dir = std::env::var_os(DIR_VARIABLE)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(zyr_proto::paths::ffmpeg_dir);
        Ffmpeg::load(&dir).unwrap_or_else(|e| {
            panic!(
                "these tests need FFmpeg and could not load it from {dir}: {e}\n\
                 Build it for this system with `packaging/ffmpeg/build.sh linux <out-dir>` \
                 and run the tests with {DIR_VARIABLE}=<out-dir>/lib.",
                dir = dir.display(),
            )
        })
    }))
}

/// The journal of this run of the tests, under `tag`.
pub fn log(tag: &'static str) -> Log {
    static OPENED: OnceLock<Log> = OnceLock::new();
    OPENED
        .get_or_init(|| {
            let path = std::env::temp_dir()
                .join(format!("zyr-player-tests-{}", std::process::id()))
                .join("player.log");
            Log::open(&path).expect("the tests' journal opens")
        })
        .about(tag)
}

/// A journal of one test's own, for a test that reads back what was
/// written: the run's shared journal holds every test's lines at once.
pub struct OwnLog {
    pub log: Log,
    path: PathBuf,
}

impl OwnLog {
    pub fn new(test: &str) -> Self {
        let path = std::env::temp_dir()
            .join(format!("zyr-player-tests-{}", std::process::id()))
            .join(format!("{test}.log"));
        let log = Log::open(&path).expect("a journal of the test's own opens");
        Self { log, path }
    }

    /// Everything written so far.
    pub fn written(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }
}

impl Drop for OwnLog {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// One encoded frame, and whether it is a key frame.
pub type Encoded = (Vec<u8>, bool);

/// `count` frames of H.264, each a picture of its own, the first and the
/// ones in `keys` key frames; and the fingerprint of each decoded.
pub fn h264(count: usize, keys: &[usize]) -> (Vec<Encoded>, Vec<u64>) {
    let ff = ffmpeg();
    let mut encoder = VideoEncoder::open(
        &ff,
        EncoderConfig {
            codec: VideoCodec::H264,
            width: WIDTH,
            height: HEIGHT,
            fps: 60,
            bitrate_kbps: 2_000,
            backend: Backend::Software,
            input: Input::Cpu,
        },
    )
    .unwrap();
    let mut packets = Vec::new();
    for n in 0..count {
        let mut frame = encoder.frame_for_cpu().unwrap();
        let planes = frame.planes();
        for (row, line) in planes.luma.chunks_mut(planes.luma_stride).enumerate() {
            for (column, pixel) in line.iter_mut().enumerate() {
                *pixel = (16 + (row + column * 2 + n * 7) % 200) as u8;
            }
        }
        planes.chroma.fill(128);
        encoder
            .encode(Frame::Cpu(frame), n == 0 || keys.contains(&n))
            .unwrap();
        let packet = encoder.receive().unwrap().expect("x264 answers at once");
        packets.push((packet.data, packet.key));
    }
    let mut decoder = VideoDecoder::open(&ff, VideoCodec::H264, DecodeOutput::Cpu).unwrap();
    let looks = packets
        .iter()
        .map(|(data, _)| match decoder.decode(data).unwrap() {
            Some(DecodedFrame::Cpu(picture)) => checksum(&picture),
            _ => panic!("every frame gives a picture"),
        })
        .collect();
    (packets, looks)
}

/// The datagrams of one frame, as the host engine sends them.
pub fn datagrams(stream: u16, frame: u32, packet: &Encoded, captured_us: u32) -> Vec<Vec<u8>> {
    let mut packets = Packets::new();
    Packetizer::new(1161, 20)
        .packetize(
            &OutgoingFrame {
                data: &packet.0,
                key: packet.1,
                repeat: false,
                stream,
                frame,
                captured_us,
                host_latency_us: 2_000,
                codec: VideoCodec::H264,
            },
            &mut packets,
        )
        .unwrap();
    packets.iter().map(<[u8]>::to_vec).collect()
}

/// `count` sound datagrams of a tone, numbered from 0.
pub fn sound(count: u16) -> Vec<Vec<u8>> {
    let mut encoder = OpusEncoder::open(&ffmpeg(), 128_000).unwrap();
    (0..count)
        .map(|sequence| {
            let tone: Vec<f32> = (0..PACKET)
                .map(|n| {
                    let t = (usize::from(sequence) * OPUS_FRAME + n / 2) as f32 / 48_000.0;
                    (t * 440.0 * std::f32::consts::TAU).sin() * 0.5
                })
                .collect();
            let mut datagram = Vec::new();
            write_audio(sequence, 0, &encoder.encode(&tone).unwrap(), &mut datagram);
            datagram
        })
        .collect()
}
