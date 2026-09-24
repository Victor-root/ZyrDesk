//! How each encoder is told to keep latency down.
//!
//! Every value handed to an encoder, as data, in one place, so the whole
//! set can be read, compared with what Sunshine settled on, and tested
//! without the encoder at hand. Names and values are FFmpeg 9.0.2's, from
//! the option tables of libavcodec (nvenc_h264.c and its siblings,
//! amfenc_*.c, qsvenc*.c, mfenc.c, libx264.c). An option an encoder does
//! not know stops its opening rather than being ignored.
//!
//! What every backend shares is set on the codec context instead (see
//! the encoder): no B-frames, a key frame only when asked, a constant
//! rate whose buffer holds one frame, low delay, BT.709 limited range.

use std::ffi::c_int;

use zyr_media::codec::VideoCodec;

use crate::encoder::Backend;

/// The private options of one encoder.
pub(crate) fn options(backend: Backend, codec: VideoCodec) -> Vec<(&'static str, &'static str)> {
    match backend {
        Backend::Nvenc => vec![
            // The fastest preset, Sunshine's default: the rate, not the
            // preset, is what buys the picture back.
            ("preset", "p1"),
            ("tune", "ull"),
            ("rc", "cbr"),
            ("zerolatency", "1"),
            // A frame's packet is fetched as soon as it is encoded.
            ("delay", "0"),
            // One frame in flight at a time.
            ("surfaces", "1"),
            ("forced-idr", "1"),
        ],
        Backend::Amf => {
            let mut options = vec![
                ("usage", "ultralowlatency"),
                ("rc", "cbr"),
                // One frame in flight at a time; FFmpeg's default is 16.
                ("async_depth", "1"),
                ("forced_idr", "1"),
                // Padding a frame up to the rate is bytes for nothing.
                ("filler_data", "0"),
                // A constant rate that must also keep its buffer model
                // exact drops frames to do it.
                ("enforce_hrd", "0"),
            ];
            match codec {
                VideoCodec::H264 => options.extend([
                    ("latency", "1"),
                    ("frame_skipping", "0"),
                    ("profile", "high"),
                ]),
                VideoCodec::Hevc => options.extend([
                    ("latency", "1"),
                    ("skip_frame", "0"),
                    // The parameter sets with every IDR, which is where a
                    // player that lost its way starts again.
                    ("header_insertion_mode", "idr"),
                    ("gops_per_idr", "1"),
                ]),
                VideoCodec::Av1 => {
                    options.extend([("latency", "lowest_latency"), ("skip_frame", "0")])
                }
            }
            options
        }
        Backend::Qsv => {
            let mut options = vec![
                ("low_power", "1"),
                ("async_depth", "1"),
                ("forced_idr", "1"),
                // Every frame kept to its share of the rate.
                ("low_delay_brc", "1"),
            ];
            match codec {
                VideoCodec::H264 => options.extend([
                    ("look_ahead", "0"),
                    // The video conferencing rate control, made for this.
                    ("vcm", "1"),
                    ("profile", "high"),
                    // Tells the decoder it never has to hold a frame back.
                    ("max_dec_frame_buffering", "1"),
                    ("recovery_point_sei", "0"),
                    ("pic_timing_sei", "0"),
                ]),
                VideoCodec::Hevc => {
                    options.extend([("recovery_point_sei", "0"), ("pic_timing_sei", "0")])
                }
                VideoCodec::Av1 => {}
            }
            options
        }
        Backend::MediaFoundation => vec![
            ("hw_encoding", "1"),
            ("rate_control", "cbr"),
            ("scenario", "display_remoting"),
        ],
        Backend::Software => vec![
            ("preset", "superfast"),
            ("tune", "zerolatency"),
            ("forced-idr", "1"),
        ],
    }
}

/// The one option to change when an encoder refuses the first set, the
/// way Sunshine tries it again.
pub(crate) fn fallback(backend: Backend) -> Option<(&'static str, &'static str)> {
    match backend {
        // Low power encoding is missing on some Intel generations; the
        // full encoder is tried then.
        Backend::Qsv => Some(("low_power", "0")),
        // Some AMD drivers refuse the ultra low latency usage (Sunshine's
        // workaround for AMF issue 410).
        Backend::Amf => Some(("usage", "lowlatency")),
        Backend::Nvenc | Backend::MediaFoundation | Backend::Software => None,
    }
}

/// `options` with one of them changed.
pub(crate) fn changed(
    options: &[(&'static str, &'static str)],
    (key, value): (&'static str, &'static str),
) -> Vec<(&'static str, &'static str)> {
    options
        .iter()
        .map(|&(name, old)| (name, if name == key { value } else { old }))
        .collect()
}

/// The rates set on the codec context.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rates {
    pub(crate) bit_rate: i64,
    pub(crate) max_rate: i64,
    /// The rate-control buffer in bits, 0 to leave it to the encoder.
    pub(crate) buffer: c_int,
}

/// What the encoder is asked for at `kbps` and `fps`.
///
/// The buffer holds one frame's worth of bits, so no frame can be much
/// larger than its share and the network never has to absorb a burst.
/// Intel is driven as Sunshine drives it: a variable rate one bit under
/// its cap, which picks the capped modes (the video conferencing one
/// for H.264), with the buffer left to the driver. x264 cut in slices
/// gets a frame and a half, below which it loses picture badly.
pub(crate) fn rates(backend: Backend, kbps: u32, fps: u32, slices: c_int) -> Rates {
    let bits = i64::from(kbps) * 1000;
    let frame = bits / i64::from(fps.max(1));
    let buffer = |bits: i64| c_int::try_from(bits).unwrap_or(c_int::MAX);
    match backend {
        Backend::Qsv => Rates {
            bit_rate: bits - 1,
            max_rate: bits,
            buffer: 0,
        },
        Backend::Software if slices > 1 => Rates {
            bit_rate: bits,
            max_rate: bits,
            buffer: buffer(frame * 3 / 2),
        },
        _ => Rates {
            bit_rate: bits,
            max_rate: bits,
            buffer: buffer(frame),
        },
    }
}

/// The distance between key frames: as far as the encoder allows, since
/// a key frame only comes when asked for. Intel keeps it in 16 bits.
pub(crate) fn gop(backend: Backend) -> c_int {
    match backend {
        Backend::Qsv => c_int::from(u16::MAX),
        _ => c_int::MAX,
    }
}

/// Threads for x264, one slice each: a frame then takes a fraction of
/// the time one core would, which is latency. Four at most, the most a
/// software decoder splits a frame across either.
pub(crate) fn software_threads() -> c_int {
    let cores = std::thread::available_parallelism().map_or(1, |cores| cores.get());
    c_int::try_from(cores.min(4)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKENDS: [Backend; 5] = [
        Backend::Nvenc,
        Backend::Amf,
        Backend::Qsv,
        Backend::MediaFoundation,
        Backend::Software,
    ];

    fn each_encoder() -> impl Iterator<Item = (Backend, VideoCodec)> {
        BACKENDS.into_iter().flat_map(|backend| {
            VideoCodec::ALL
                .into_iter()
                .filter(move |codec| backend.encoder_name(*codec).is_some())
                .map(move |codec| (backend, codec))
        })
    }

    fn has(backend: Backend, codec: VideoCodec, option: (&str, &str)) -> bool {
        options(backend, codec).contains(&option)
    }

    #[test]
    fn every_encoder_turns_a_forced_key_frame_into_an_idr() {
        for (backend, codec) in each_encoder() {
            let forced = options(backend, codec)
                .iter()
                .any(|(name, value)| name.starts_with("forced") && *value == "1");
            // Media Foundation has no such option: FFmpeg asks it for a
            // key frame directly.
            assert_eq!(
                forced,
                backend != Backend::MediaFoundation,
                "{backend:?} {codec:?}"
            );
        }
    }

    #[test]
    fn every_encoder_is_set_for_the_lowest_latency_it_has() {
        for codec in VideoCodec::ALL {
            for option in [
                ("tune", "ull"),
                ("zerolatency", "1"),
                ("delay", "0"),
                ("rc", "cbr"),
            ] {
                assert!(
                    has(Backend::Nvenc, codec, option),
                    "nvenc {codec:?} {option:?}"
                );
            }
            for option in [
                ("usage", "ultralowlatency"),
                ("async_depth", "1"),
                ("rc", "cbr"),
            ] {
                assert!(has(Backend::Amf, codec, option), "amf {codec:?} {option:?}");
            }
            for option in [
                ("low_power", "1"),
                ("async_depth", "1"),
                ("low_delay_brc", "1"),
            ] {
                assert!(has(Backend::Qsv, codec, option), "qsv {codec:?} {option:?}");
            }
        }
        assert!(has(Backend::Qsv, VideoCodec::H264, ("look_ahead", "0")));
        assert!(has(
            Backend::Software,
            VideoCodec::H264,
            ("tune", "zerolatency")
        ));
        assert!(has(
            Backend::Software,
            VideoCodec::H264,
            ("preset", "superfast")
        ));
        assert!(has(
            Backend::MediaFoundation,
            VideoCodec::Hevc,
            ("scenario", "display_remoting")
        ));
    }

    #[test]
    fn no_option_is_given_twice() {
        for (backend, codec) in each_encoder() {
            let options = options(backend, codec);
            for (index, (name, _)) in options.iter().enumerate() {
                assert!(
                    options[index + 1..].iter().all(|(other, _)| other != name),
                    "{backend:?} {codec:?} {name}"
                );
            }
        }
    }

    #[test]
    fn a_fallback_changes_an_option_the_first_set_has() {
        for (backend, codec) in each_encoder() {
            let Some(fallback) = fallback(backend) else {
                continue;
            };
            let first = options(backend, codec);
            let second = changed(&first, fallback);
            assert!(second.contains(&fallback), "{backend:?} {codec:?}");
            let differences = first.iter().zip(&second).filter(|(a, b)| a != b).count();
            assert_eq!(differences, 1, "{backend:?} {codec:?}");
        }
    }

    #[test]
    fn the_buffer_holds_one_frame_of_the_rate() {
        // 20 Mb/s at 60 fps: 333 333 bits a frame.
        let rates = rates(Backend::Nvenc, 20_000, 60, 1);
        assert_eq!(
            rates,
            Rates {
                bit_rate: 20_000_000,
                max_rate: 20_000_000,
                buffer: 333_333,
            }
        );
        assert_eq!(
            super::rates(Backend::Software, 20_000, 60, 1).buffer,
            333_333
        );
        assert_eq!(
            super::rates(Backend::Software, 20_000, 60, 4).buffer,
            499_999
        );
    }

    #[test]
    fn intel_gets_a_variable_rate_just_under_its_cap() {
        let rates = rates(Backend::Qsv, 20_000, 60, 1);
        assert_eq!(rates.max_rate, 20_000_000);
        assert_eq!(rates.bit_rate, rates.max_rate - 1);
        assert_eq!(rates.buffer, 0);
    }

    #[test]
    fn huge_rates_do_not_overflow_the_buffer() {
        assert_eq!(rates(Backend::Nvenc, u32::MAX, 1, 1).buffer, c_int::MAX);
    }

    #[test]
    fn key_frames_are_as_far_apart_as_each_encoder_can_count() {
        assert_eq!(gop(Backend::Nvenc), c_int::MAX);
        assert_eq!(gop(Backend::Qsv), 65_535);
        assert!((1..=4).contains(&software_threads()));
    }
}
