//! Sound from one rate and number of channels to another, through
//! swresample.
//!
//! The host brings whatever the sound card plays to Opus's 48 kHz
//! stereo; the player brings Opus's sound to whatever the sound card
//! plays. Both sides are interleaved 32-bit floats, what Windows' mixer
//! works in.

use std::ffi::c_int;
use std::ptr::{self, NonNull};
use std::sync::Arc;

use crate::error::CodecError;
use crate::library::Ffmpeg;
use crate::sys;

/// A rate and a number of channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    /// Samples per second, per channel.
    pub rate: u32,
    pub channels: u16,
}

impl AudioFormat {
    /// What Opus carries: 48 kHz stereo.
    pub const OPUS: AudioFormat = AudioFormat {
        rate: 48_000,
        channels: 2,
    };
}

/// A converter between two formats, keeping between two calls what the
/// filter still needs of the previous one.
pub struct Resampler {
    ff: Arc<Ffmpeg>,
    raw: NonNull<sys::SwrContext>,
    from: AudioFormat,
    to: AudioFormat,
}

impl Resampler {
    /// Channels are taken in their usual order for their number (stereo
    /// for two, 5.1 for six), the way sound cards give them.
    pub fn new(ff: &Arc<Ffmpeg>, from: AudioFormat, to: AudioFormat) -> Result<Self, CodecError> {
        for format in [from, to] {
            if format.rate == 0 || format.channels == 0 {
                return Err(CodecError::Invalid(format!(
                    "format de son impossible : {} Hz, {} canaux",
                    format.rate, format.channels
                )));
            }
        }
        let rate = |format: AudioFormat| {
            c_int::try_from(format.rate).map_err(|_| {
                CodecError::Invalid(format!("fréquence de son impossible : {} Hz", format.rate))
            })
        };
        let (from_rate, to_rate) = (rate(from)?, rate(to)?);
        let from_layout = ChannelLayout::usual(ff, from.channels);
        let to_layout = ChannelLayout::usual(ff, to.channels);
        let mut raw = ptr::null_mut();
        // SAFETY: both layouts are initialised and outlive the call,
        // which copies them; it allocates the context into `raw`.
        let allocated = unsafe {
            ff.swresample.swr_alloc_set_opts2(
                &mut raw,
                &to_layout.raw,
                sys::AVSampleFormat::AV_SAMPLE_FMT_FLT,
                to_rate,
                &from_layout.raw,
                sys::AVSampleFormat::AV_SAMPLE_FMT_FLT,
                from_rate,
                0,
                ptr::null_mut(),
            )
        };
        ff.check(allocated, "préparation de la conversion du son")?;
        let raw = NonNull::new(raw).ok_or(CodecError::OutOfMemory {
            what: "la conversion du son",
        })?;
        let resampler = Self {
            ff: Arc::clone(ff),
            raw,
            from,
            to,
        };
        // SAFETY: a context set up just above.
        let initialised = unsafe { ff.swresample.swr_init(resampler.raw.as_ptr()) };
        ff.check(initialised, "préparation de la conversion du son")?;
        Ok(resampler)
    }

    /// Converts interleaved samples, a whole number for each channel.
    ///
    /// What comes out may be a little shorter or longer than the exact
    /// ratio: the filter holds a few samples back from one call to the
    /// next, and they come out with the next.
    pub fn convert(&mut self, input: &[f32]) -> Result<Vec<f32>, CodecError> {
        let from_channels = usize::from(self.from.channels);
        if !input.len().is_multiple_of(from_channels) {
            return Err(CodecError::Invalid(format!(
                "{} échantillons ne se répartissent pas sur {from_channels} canaux",
                input.len()
            )));
        }
        let in_count = c_int::try_from(input.len() / from_channels).map_err(|_| {
            CodecError::Invalid(format!("{} échantillons d'un coup, trop", input.len()))
        })?;
        // SAFETY: a live context; returns an upper bound for what the
        // next call can give back.
        let room = unsafe {
            self.ff
                .swresample
                .swr_get_out_samples(self.raw.as_ptr(), in_count)
        };
        let room = self.ff.check(room, "conversion du son")?;
        let to_channels = usize::from(self.to.channels);
        let mut output = vec![0.0f32; usize::try_from(room).unwrap_or(0) * to_channels];
        let output_planes = [output.as_mut_ptr().cast::<u8>()];
        let input_planes = [input.as_ptr().cast::<u8>()];
        // SAFETY: interleaved sound has a single plane on each side; the
        // output has room for `room` samples per channel and the input
        // holds `in_count`.
        let converted = unsafe {
            self.ff.swresample.swr_convert(
                self.raw.as_ptr(),
                output_planes.as_ptr(),
                room,
                input_planes.as_ptr(),
                in_count,
            )
        };
        let converted = self.ff.check(converted, "conversion du son")?;
        output.truncate(usize::try_from(converted).unwrap_or(0) * to_channels);
        Ok(output)
    }
}

impl Drop for Resampler {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: allocated by swr_alloc_set_opts2, freed once here.
        unsafe { self.ff.swresample.swr_free(&mut raw) };
    }
}

/// An `AVChannelLayout` of our own, uninitialised when it goes.
struct ChannelLayout<'a> {
    ff: &'a Ffmpeg,
    raw: sys::AVChannelLayout,
}

impl<'a> ChannelLayout<'a> {
    /// The usual layout for that many channels.
    fn usual(ff: &'a Ffmpeg, channels: u16) -> Self {
        // SAFETY: all zeros is a valid, empty layout, which
        // av_channel_layout_default then fills.
        let mut raw: sys::AVChannelLayout = unsafe { std::mem::zeroed() };
        // SAFETY: a layout of ours, filled in place.
        unsafe {
            ff.avutil
                .av_channel_layout_default(&mut raw, c_int::from(channels))
        };
        Self { ff, raw }
    }
}

impl Drop for ChannelLayout<'_> {
    fn drop(&mut self) {
        // SAFETY: initialised by av_channel_layout_default, once.
        unsafe { self.ff.avutil.av_channel_layout_uninit(&mut self.raw) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// A 1 kHz tone at `rate`, the same on every channel.
    fn tone(rate: u32, channels: u16, seconds: f32) -> Vec<f32> {
        let samples = (rate as f32 * seconds) as usize;
        (0..samples)
            .flat_map(|index| {
                let value =
                    0.5 * (std::f32::consts::TAU * 1000.0 * index as f32 / rate as f32).sin();
                std::iter::repeat_n(value, usize::from(channels))
            })
            .collect()
    }

    /// Rising zero crossings of the first channel, per second.
    fn frequency(pcm: &[f32], format: AudioFormat) -> f32 {
        let first: Vec<f32> = pcm
            .iter()
            .step_by(usize::from(format.channels))
            .copied()
            .collect();
        let crossings = first
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        crossings as f32 * format.rate as f32 / first.len() as f32
    }

    /// Converts one second in 10 ms slices, the way the engine does.
    fn one_second(from: AudioFormat, to: AudioFormat) -> Vec<f32> {
        let ff = testing::ffmpeg();
        let mut resampler = Resampler::new(&ff, from, to).unwrap();
        let input = tone(from.rate, from.channels, 1.0);
        let slice = from.rate as usize / 100 * usize::from(from.channels);
        input
            .chunks(slice)
            .flat_map(|chunk| resampler.convert(chunk).unwrap())
            .collect()
    }

    #[test]
    fn cd_sound_becomes_opus_sound_at_the_same_pitch() {
        let from = AudioFormat {
            rate: 44_100,
            channels: 2,
        };
        let output = one_second(from, AudioFormat::OPUS);
        let frames = output.len() / 2;
        // All of it but what the filter still holds.
        assert!((47_900..=48_000).contains(&frames), "{frames}");
        let pitch = frequency(&output, AudioFormat::OPUS);
        assert!((pitch - 1000.0).abs() < 5.0, "{pitch} Hz");
    }

    #[test]
    fn opus_sound_goes_back_to_a_sound_card_rate() {
        let to = AudioFormat {
            rate: 44_100,
            channels: 2,
        };
        let output = one_second(AudioFormat::OPUS, to);
        let frames = output.len() / 2;
        assert!((44_000..=44_100).contains(&frames), "{frames}");
        let pitch = frequency(&output, to);
        assert!((pitch - 1000.0).abs() < 5.0, "{pitch} Hz");
    }

    #[test]
    fn surround_and_mono_become_stereo() {
        for channels in [1, 6] {
            let from = AudioFormat {
                rate: 48_000,
                channels,
            };
            let output = one_second(from, AudioFormat::OPUS);
            assert_eq!(output.len() % 2, 0);
            let frames = output.len() / 2;
            assert!((47_900..=48_000).contains(&frames), "{channels}: {frames}");
            assert!(
                output.iter().any(|sample| sample.abs() > 0.1),
                "{channels}: silence"
            );
        }
    }

    #[test]
    fn samples_that_do_not_fill_every_channel_are_refused() {
        let ff = testing::ffmpeg();
        let mut resampler = Resampler::new(&ff, AudioFormat::OPUS, AudioFormat::OPUS).unwrap();
        assert!(matches!(
            resampler.convert(&[0.0; 3]),
            Err(CodecError::Invalid(_))
        ));
        assert!(
            Resampler::new(
                &ff,
                AudioFormat {
                    rate: 0,
                    channels: 2
                },
                AudioFormat::OPUS
            )
            .is_err()
        );
    }
}
