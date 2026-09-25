//! Which encoders work on this machine, found by trying them.
//!
//! An encoder being in FFmpeg says nothing of the card, its driver or
//! what it supports: each candidate is opened for real, given a picture
//! and asked for a key frame, and only those that give one are kept.

use std::sync::Arc;

use zyr_media::codec::VideoCodec;

use crate::encoder::{Backend, EncoderConfig, Input, VideoEncoder};
use crate::error::CodecError;
use crate::library::Ffmpeg;

/// Who made the graphics card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl GpuVendor {
    /// From the card's PCI vendor id, as DXGI reports it.
    pub fn from_pci(vendor_id: u32) -> Self {
        match vendor_id {
            0x10de => GpuVendor::Nvidia,
            0x1002 => GpuVendor::Amd,
            0x8086 => GpuVendor::Intel,
            _ => GpuVendor::Other,
        }
    }

    /// The card maker's own encoder, which only runs on its cards.
    fn backend(self) -> Option<Backend> {
        match self {
            GpuVendor::Nvidia => Some(Backend::Nvenc),
            GpuVendor::Amd => Some(Backend::Amf),
            GpuVendor::Intel => Some(Backend::Qsv),
            GpuVendor::Other => None,
        }
    }
}

/// Size of the picture each candidate is tried with: small, so the
/// whole probe stays well under a second.
const TRIED_WIDTH: u32 = 640;
const TRIED_HEIGHT: u32 = 360;

/// Every encoder that works here, best first: the card maker's own,
/// then what Windows offers, then x264 (H.264 only).
///
/// x264 is always tried on pictures in memory, whatever `input` says,
/// since that is the only way it reads them. Each encoder left out is
/// named in the log, with why (see [`Ffmpeg::log_into`]).
pub fn probe(ff: &Arc<Ffmpeg>, input: &Input, vendor: GpuVendor) -> Vec<(VideoCodec, Backend)> {
    let backends = vendor
        .backend()
        .into_iter()
        .chain([Backend::MediaFoundation, Backend::Software]);
    backends
        .flat_map(|backend| VideoCodec::ALL.map(|codec| (codec, backend)))
        .filter_map(|(codec, backend)| Some((codec, backend, backend.encoder_name(codec)?)))
        .filter(
            |(codec, backend, name)| match works(ff, input, *codec, *backend) {
                Ok(()) => true,
                Err(refused) => {
                    crate::log::note(&format!("{name} left out by the probe: {refused}"));
                    false
                }
            },
        )
        .map(|(codec, backend, _)| (codec, backend))
        .collect()
}

/// Whether the encoder opens and turns a picture into a key frame, and
/// why not.
fn works(
    ff: &Arc<Ffmpeg>,
    input: &Input,
    codec: VideoCodec,
    backend: Backend,
) -> Result<(), CodecError> {
    let input = if backend == Backend::Software {
        Input::Cpu
    } else {
        input.clone()
    };
    let mut encoder = VideoEncoder::open(
        ff,
        EncoderConfig {
            codec,
            width: TRIED_WIDTH,
            height: TRIED_HEIGHT,
            fps: 60,
            bitrate_kbps: 2_000,
            backend,
            input,
        },
    )?;
    let frame = encoder.blank_frame()?;
    encoder.encode(frame, true)?;
    let packet = match encoder.receive()? {
        Some(packet) => Some(packet),
        None => {
            encoder.finish()?;
            encoder.receive()?
        }
    };
    if !packet.is_some_and(|packet| packet.key) {
        return Err(CodecError::Invalid(
            "l'image d'essai n'a pas donné d'image clé".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn card_makers_are_known_by_their_pci_ids() {
        assert_eq!(GpuVendor::from_pci(0x10de), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci(0x1002), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci(0x8086), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_pci(0x5143), GpuVendor::Other);
    }

    #[test]
    fn pictures_in_memory_find_x264_for_h264() {
        let ff = testing::ffmpeg();
        // The Linux build has no hardware encoder: whatever the card, x264
        // is what is left, and only for H.264.
        for vendor in [GpuVendor::Nvidia, GpuVendor::Other] {
            assert_eq!(
                probe(&ff, &Input::Cpu, vendor),
                vec![(VideoCodec::H264, Backend::Software)],
                "{vendor:?}"
            );
        }
    }
}
