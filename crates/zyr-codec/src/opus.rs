//! Sound in Opus: the host encodes what the computer plays, ten
//! milliseconds at a time, and the player decodes it.
//!
//! Always 48 kHz stereo, Opus's own rate, whatever the sound card has:
//! the resampler brings everything to it first.

use std::ffi::c_int;
use std::sync::Arc;

use crate::error::CodecError;
use crate::library::Ffmpeg;
use crate::owned::{CodecContext, LentPacket, OwnedFrame, OwnedPacket};
use crate::resample::AudioFormat;
use crate::sys;

/// Samples per channel in one packet: 10 ms at 48 kHz.
pub const OPUS_FRAME: usize = 480;

/// What a packet carries, interleaved: left, right, left, right...
const INTERLEAVED: usize = OPUS_FRAME * AudioFormat::OPUS.channels as usize;

/// The encoder of the host's sound, libopus.
pub struct OpusEncoder {
    ff: Arc<Ffmpeg>,
    context: CodecContext,
    packet: OwnedPacket,
    next_pts: i64,
}

impl OpusEncoder {
    /// Opens libopus for 10 ms packets at `bitrate_bps` (the engine uses
    /// 128 000).
    ///
    /// The low-delay application, which gives up the speech-only modes
    /// and their longer look-ahead, and a constant rate, so every packet
    /// weighs the same on the network, as Sunshine sends it.
    pub fn open(ff: &Arc<Ffmpeg>, bitrate_bps: u32) -> Result<Self, CodecError> {
        let mut context = CodecContext::encoder(ff, "libopus")?;
        let fields = context.fields();
        fields.sample_fmt = sys::AVSampleFormat::AV_SAMPLE_FMT_FLT;
        fields.sample_rate = AudioFormat::OPUS.rate as c_int;
        fields.bit_rate = i64::from(bitrate_bps);
        fields.time_base = sys::AVRational {
            num: 1,
            den: AudioFormat::OPUS.rate as c_int,
        };
        stereo(ff, &mut fields.ch_layout);
        context.open(&[
            ("application", "lowdelay"),
            ("frame_duration", "10"),
            ("vbr", "off"),
        ])?;
        let frame_size = context.get().frame_size;
        if usize::try_from(frame_size) != Ok(OPUS_FRAME) {
            return Err(CodecError::Invalid(format!(
                "libopus a choisi des paquets de {frame_size} échantillons au lieu de {OPUS_FRAME}"
            )));
        }
        Ok(Self {
            ff: Arc::clone(ff),
            context,
            packet: OwnedPacket::new(ff)?,
            next_pts: 0,
        })
    }

    /// Encodes 10 ms of interleaved stereo, `OPUS_FRAME` samples per
    /// channel, into one packet.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>, CodecError> {
        if pcm.len() != INTERLEAVED {
            return Err(CodecError::Invalid(format!(
                "Opus prend {INTERLEAVED} échantillons entrelacés à la fois, pas {}",
                pcm.len()
            )));
        }
        let mut frame = OwnedFrame::new(&self.ff)?;
        let fields = frame.fields();
        fields.format = sys::AVSampleFormat::AV_SAMPLE_FMT_FLT.0;
        fields.nb_samples = OPUS_FRAME as c_int;
        fields.sample_rate = AudioFormat::OPUS.rate as c_int;
        fields.pts = self.next_pts;
        stereo(&self.ff, &mut fields.ch_layout);
        frame.allocate()?;
        // SAFETY: interleaved float sound lives in one plane, which
        // av_frame_get_buffer made large (and aligned) enough for every
        // sample of the frame; nothing else references it yet.
        unsafe {
            std::ptr::copy_nonoverlapping(
                pcm.as_ptr(),
                frame.get().data[0].cast::<f32>(),
                INTERLEAVED,
            );
        }
        self.next_pts += OPUS_FRAME as i64;

        self.context.send_frame(&frame)?;
        if !self.context.receive_packet(&mut self.packet)? {
            return Err(CodecError::Invalid(
                "libopus n'a rendu aucun paquet pour 10 ms de son".to_string(),
            ));
        }
        Ok(self.packet.to_vec())
    }
}

/// The player's decoder, FFmpeg's own.
pub struct OpusDecoder {
    context: CodecContext,
    packet: LentPacket,
    frame: OwnedFrame,
}

impl OpusDecoder {
    pub fn open(ff: &Arc<Ffmpeg>) -> Result<Self, CodecError> {
        let mut context = CodecContext::decoder(ff, "opus")?;
        let fields = context.fields();
        fields.sample_rate = AudioFormat::OPUS.rate as c_int;
        stereo(ff, &mut fields.ch_layout);
        context.open(&[])?;
        Ok(Self {
            context,
            packet: LentPacket::new(ff)?,
            frame: OwnedFrame::new(ff)?,
        })
    }

    /// Decodes one packet into interleaved stereo.
    pub fn decode(&mut self, packet: &[u8]) -> Result<Vec<f32>, CodecError> {
        self.context.send_bytes(&mut self.packet, packet)?;
        let mut pcm = Vec::with_capacity(INTERLEAVED);
        while self.context.receive_frame(&mut self.frame)? {
            interleave(self.frame.get(), &mut pcm)?;
        }
        Ok(pcm)
    }
}

/// Appends a decoded frame to `pcm`, one sample of each channel after
/// the other.
fn interleave(frame: &sys::AVFrame, pcm: &mut Vec<f32>) -> Result<(), CodecError> {
    let samples = usize::try_from(frame.nb_samples).unwrap_or(0);
    let channels = usize::try_from(frame.ch_layout.nb_channels).unwrap_or(0);
    if channels == 0 || channels > frame.data.len() {
        return Err(CodecError::Invalid(format!(
            "le décodeur Opus a rendu {channels} canaux"
        )));
    }
    match sys::AVSampleFormat(frame.format) {
        sys::AVSampleFormat::AV_SAMPLE_FMT_FLTP => {
            let planes: Vec<&[f32]> = frame.data[..channels]
                .iter()
                // SAFETY: a planar float frame has one plane per channel,
                // each holding `nb_samples` samples, alive as long as the
                // frame is not emptied, which cannot happen during this
                // borrow of it.
                .map(|plane| unsafe { std::slice::from_raw_parts(plane.cast::<f32>(), samples) })
                .collect();
            for index in 0..samples {
                pcm.extend(planes.iter().map(|plane| plane[index]));
            }
        }
        sys::AVSampleFormat::AV_SAMPLE_FMT_FLT => {
            // SAFETY: an interleaved float frame holds every sample in its
            // first plane.
            let all = unsafe {
                std::slice::from_raw_parts(frame.data[0].cast::<f32>(), samples * channels)
            };
            pcm.extend_from_slice(all);
        }
        other => {
            return Err(CodecError::Invalid(format!(
                "le décodeur Opus a rendu un format de son inattendu ({})",
                other.0
            )));
        }
    }
    Ok(())
}

/// Sets `layout` to plain stereo.
fn stereo(ff: &Ffmpeg, layout: &mut sys::AVChannelLayout) {
    // SAFETY: the layout is FFmpeg's, owned by the context or frame it
    // sits in, which uninitialise it when they go; a native layout
    // allocates nothing.
    unsafe {
        ff.avutil
            .av_channel_layout_default(layout, c_int::from(AudioFormat::OPUS.channels))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// A 440 Hz tone at half scale, both channels alike.
    fn tone(from: usize, samples: usize) -> Vec<f32> {
        (from..from + samples)
            .flat_map(|index| {
                let value = 0.5 * (std::f32::consts::TAU * 440.0 * index as f32 / 48_000.0).sin();
                [value, value]
            })
            .collect()
    }

    fn loudness(pcm: &[f32]) -> f32 {
        (pcm.iter().map(|sample| sample * sample).sum::<f32>() / pcm.len() as f32).sqrt()
    }

    #[test]
    fn ten_millisecond_packets_come_back_as_ten_milliseconds_of_the_same_sound() {
        let ff = testing::ffmpeg();
        let mut encoder = OpusEncoder::open(&ff, 128_000).unwrap();
        let mut decoder = OpusDecoder::open(&ff).unwrap();

        let mut heard = Vec::new();
        for packet in 0..50 {
            let pcm = tone(packet * OPUS_FRAME, OPUS_FRAME);
            let encoded = encoder.encode(&pcm).unwrap();
            // A constant rate of 128 kb/s is 160 bytes every 10 ms.
            assert_eq!(encoded.len(), 160, "packet {packet}");
            let decoded = decoder.decode(&encoded).unwrap();
            assert_eq!(decoded.len(), INTERLEAVED, "packet {packet}");
            heard.extend(decoded);
        }

        // Past the encoder's look-ahead, the tone is there at its
        // loudness: 0.5 / sqrt(2).
        let settled = &heard[heard.len() / 2..];
        let expected = loudness(&tone(0, OPUS_FRAME * 10));
        let got = loudness(settled);
        assert!(
            (got - expected).abs() < 0.05 * expected,
            "{got} against {expected}"
        );
    }

    #[test]
    fn the_encoder_takes_ten_milliseconds_and_nothing_else() {
        let ff = testing::ffmpeg();
        let mut encoder = OpusEncoder::open(&ff, 128_000).unwrap();
        let refused = encoder.encode(&tone(0, OPUS_FRAME / 2)).unwrap_err();
        assert!(matches!(refused, CodecError::Invalid(_)), "{refused:?}");
        assert!(encoder.encode(&tone(0, OPUS_FRAME)).is_ok());
    }

    #[test]
    fn a_damaged_packet_is_an_error_and_the_decoder_goes_on() {
        let ff = testing::ffmpeg();
        let mut encoder = OpusEncoder::open(&ff, 128_000).unwrap();
        let mut decoder = OpusDecoder::open(&ff).unwrap();
        // Code 3 announces a count of frames in a second byte that is
        // not there.
        assert!(decoder.decode(&[0xff]).is_err());
        // Nothing at all is refused too, and is not the end of the sound.
        assert!(matches!(decoder.decode(&[]), Err(CodecError::Invalid(_))));
        let encoded = encoder.encode(&tone(0, OPUS_FRAME)).unwrap();
        assert_eq!(decoder.decode(&encoded).unwrap().len(), INTERLEAVED);
    }
}
