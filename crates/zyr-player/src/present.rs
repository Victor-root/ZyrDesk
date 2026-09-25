//! Putting decoded pictures in front of the person.
//!
//! The picture keeps its own shape whatever the shape of the surface:
//! it is drawn as large as fits, centred, and what is left around it
//! stays black. Where it lands is what the window needs to turn a
//! position under the mouse into a position in the picture.

use std::hash::{DefaultHasher, Hasher};
use std::sync::Arc;

use zyr_codec::{CpuPicture, DecodeOutput, DecodedFrame, Ffmpeg, PictureLayout, VideoDecoder};
use zyr_media::codec::{CodecSet, VideoCodec};

/// Where the picture is drawn in the surface, in its pixels: left, top,
/// width, height.
pub type Rect = (i32, i32, u32, u32);

/// The largest rectangle with the picture's shape that fits in the
/// surface, centred.
///
/// Nothing when either has no area: a minimised window shows nothing.
pub fn letterbox(picture: (u32, u32), surface: (u32, u32)) -> Option<Rect> {
    let (picture_width, picture_height) = (u64::from(picture.0), u64::from(picture.1));
    let (surface_width, surface_height) = (u64::from(surface.0), u64::from(surface.1));
    if picture_width == 0 || picture_height == 0 || surface_width == 0 || surface_height == 0 {
        return None;
    }
    // Compared by cross products, so that no rounding decides which
    // side touches the edges.
    let (width, height) = if picture_width * surface_height >= surface_width * picture_height {
        let height = (surface_width * picture_height / picture_width).max(1);
        (surface_width, height)
    } else {
        let width = (surface_height * picture_width / picture_height).max(1);
        (width, surface_height)
    };
    let left = (surface_width - width) / 2;
    let top = (surface_height - height) / 2;
    // A width or height is at most the surface's, a u32, and a margin at
    // most half of it, which an i32 holds.
    Some((left as i32, top as i32, width as u32, height as u32))
}

/// What went wrong while drawing.
///
/// Only a graphics card fails to draw: off Windows, only the tests'
/// presenters ever say so, which is the one reason for the attribute.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug)]
pub enum Fault {
    /// The graphics card went away (driver update, reset, removal):
    /// everything made on it has to be made again with
    /// [`Presenter::renew`].
    Lost(String),
    /// This picture could not be drawn; the next may be.
    Failed(String),
}

/// What drawing one picture gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// A fingerprint of the picture, when it was drawn on the processor:
    /// how a test or a diagnosis tells pictures apart.
    pub checksum: Option<u64>,
}

/// Something decoded pictures are drawn on.
pub trait Presenter {
    /// Where the decoder is to put its pictures for this presenter;
    /// nothing while there is nowhere, the graphics card being made
    /// again.
    fn output(&self) -> Option<DecodeOutput>;

    /// The codecs this presenter's decoder can decode.
    fn decodable(&self, ff: &Arc<Ffmpeg>) -> CodecSet;

    /// Draws a picture and shows it.
    fn present(&mut self, picture: &DecodedFrame) -> Result<Shown, Fault>;

    /// The surface is now this large, in pixels.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), Fault>;

    /// Where the last picture was drawn, if one was.
    fn picture_rect(&self) -> Option<Rect>;

    /// Makes everything again after [`Fault::Lost`].
    fn renew(&mut self) -> Result<(), String>;
}

/// The width and height of a decoded picture.
pub fn picture_size(picture: &DecodedFrame) -> (u32, u32) {
    match picture {
        #[cfg(windows)]
        DecodedFrame::D3d11(picture) => (picture.width(), picture.height()),
        DecodedFrame::Cpu(picture) => (picture.width(), picture.height()),
    }
}

/// A fingerprint of what a picture shows: every visible byte of every
/// plane, the padding at the end of the rows left out.
pub fn checksum(picture: &CpuPicture) -> u64 {
    let width = picture.width() as usize;
    let mut hasher = DefaultHasher::new();
    for (index, plane) in picture.planes().iter().enumerate() {
        // Luma is as wide as the picture; the chroma planes of both
        // layouts hold as many bytes a row as half the pixels, twice
        // over for the interleaved one: `width` bytes rounded to even
        // for NV12, half of that for each plane of YUV 4:2:0.
        let row = match (index, picture.layout()) {
            (0, _) => width,
            (_, PictureLayout::Nv12) => width.div_ceil(2) * 2,
            (_, PictureLayout::Yuv420p) => width.div_ceil(2),
        };
        for line in plane.data.chunks(plane.stride) {
            hasher.write(&line[..row.min(line.len())]);
        }
    }
    hasher.finish()
}

/// Draws nothing, on the processor: what tests and the diagnostic
/// command line use, with no window at all.
///
/// Each picture it is given leaves its fingerprint, which is what a
/// person would have seen.
#[derive(Debug, Default)]
pub struct Headless {
    /// The size of the surface, once somebody said.
    surface: Option<(u32, u32)>,
    /// The size of the last picture.
    picture: Option<(u32, u32)>,
}

impl Headless {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Presenter for Headless {
    fn output(&self) -> Option<DecodeOutput> {
        Some(DecodeOutput::Cpu)
    }

    /// What the processor decodes here: FFmpeg's own decoders, tried.
    fn decodable(&self, ff: &Arc<Ffmpeg>) -> CodecSet {
        VideoCodec::ALL
            .into_iter()
            .filter(|codec| VideoDecoder::open(ff, *codec, DecodeOutput::Cpu).is_ok())
            .collect()
    }

    fn present(&mut self, picture: &DecodedFrame) -> Result<Shown, Fault> {
        let checksum = match picture {
            DecodedFrame::Cpu(picture) => checksum(picture),
            #[cfg(windows)]
            DecodedFrame::D3d11(_) => {
                return Err(Fault::Failed(
                    "une image décodée par la carte graphique ne se montre pas sans fenêtre"
                        .to_string(),
                ));
            }
        };
        self.picture = Some(picture_size(picture));
        Ok(Shown {
            checksum: Some(checksum),
        })
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), Fault> {
        self.surface = Some((width, height));
        Ok(())
    }

    /// Without a surface of its own size, the picture is drawn at its
    /// own size.
    fn picture_rect(&self) -> Option<Rect> {
        let picture = self.picture?;
        letterbox(picture, self.surface.unwrap_or(picture))
    }

    fn renew(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_of_the_same_shape_fills_the_surface() {
        assert_eq!(
            letterbox((1920, 1080), (1280, 720)),
            Some((0, 0, 1280, 720))
        );
        assert_eq!(letterbox((1280, 720), (1280, 720)), Some((0, 0, 1280, 720)));
    }

    #[test]
    fn a_wider_picture_gets_bands_above_and_below() {
        assert_eq!(
            letterbox((1920, 1080), (1000, 1000)),
            Some((0, 219, 1000, 562))
        );
    }

    #[test]
    fn a_taller_picture_gets_bands_on_the_sides() {
        assert_eq!(
            letterbox((1080, 1920), (1920, 1080)),
            Some((656, 0, 607, 1080))
        );
        assert_eq!(letterbox((4, 3), (1920, 1080)), Some((240, 0, 1440, 1080)));
    }

    #[test]
    fn nothing_is_drawn_without_an_area() {
        assert_eq!(letterbox((0, 1080), (1920, 1080)), None);
        assert_eq!(letterbox((1920, 1080), (1920, 0)), None);
        assert_eq!(letterbox((1920, 1080), (0, 0)), None);
    }

    #[test]
    fn an_extreme_shape_still_gets_a_pixel() {
        assert_eq!(letterbox((65535, 1), (100, 100)), Some((0, 49, 100, 1)));
        assert_eq!(
            letterbox((u32::MAX, u32::MAX), (u32::MAX, 1)),
            Some(((u32::MAX / 2) as i32, 0, 1, 1))
        );
    }

    #[test]
    fn the_headless_surface_letterboxes_like_a_window() {
        let mut headless = Headless::new();
        assert_eq!(headless.picture_rect(), None);
        headless.picture = Some((640, 360));
        assert_eq!(headless.picture_rect(), Some((0, 0, 640, 360)));
        headless.resize(640, 480).unwrap();
        assert_eq!(headless.picture_rect(), Some((0, 60, 640, 360)));
        headless.resize(0, 0).unwrap();
        assert_eq!(headless.picture_rect(), None);
    }
}
