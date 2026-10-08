//! The far pointer's picture, made into a pointer of this computer.
//!
//! When a program over there draws its pointer for itself, the engine
//! there carries its picture (`zyr_media::pointer`). It is drawn here as a
//! pointer of this computer, where the hand is, at the scale the far
//! screen is shown at: a finger, a brush or a sight is drawn for what is
//! under it, and keeps its size against it.
//!
//! What Windows makes a pointer of: a colour image and a mask of one bit a
//! pixel. Where the colour has an alpha, it is blended by it. Where it has
//! none, the screen is kept where the mask is set and the colour's bits
//! are flipped into it, which is how the system itself draws a black and
//! white pointer, and one that flips the screen.

use std::sync::Arc;

use zyr_media::pointer::{PointerKind, PointerPicture};

/// The smallest and the largest scale a far pointer is drawn at: past
/// those, it would be lost, or hide what it points at.
const SMALLEST: f64 = 0.25;
const LARGEST: f64 = 4.0;

/// A pointer ready to be made one of this computer's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerImage {
    pub width: u32,
    pub height: u32,
    /// The pixel that points, from the top left corner.
    pub hot_x: u32,
    pub hot_y: u32,
    /// Blue, green, red and alpha, `width * 4` bytes a row, the top row
    /// first. An alpha of 0 everywhere leaves it to the mask.
    pub colour: Vec<u8>,
    /// One bit a pixel, the leftmost the highest, each row padded to two
    /// bytes as Windows wants a mask.
    pub mask: Vec<u8>,
}

impl PointerImage {
    /// Bytes in a row of the mask.
    pub fn mask_row_bytes(&self) -> usize {
        (self.width as usize).div_ceil(16) * 2
    }
}

/// The far pointer as last said, and the image last made of it.
#[derive(Debug, Default)]
pub(crate) struct FarPointer {
    picture: Option<PointerPicture>,
    /// For a picture of the far screen that many pixels wide here.
    made: Option<(u32, Arc<PointerImage>)>,
}

impl FarPointer {
    /// The far pointer is now this picture, or one of the system's own
    /// shapes.
    pub(crate) fn set(&mut self, picture: Option<PointerPicture>) {
        self.picture = picture;
        self.made = None;
    }

    /// The image to draw, the far screen being shown `across` pixels wide
    /// here (0 while that is not known). Made again only when either
    /// changed: it is asked every time the pointer moves.
    pub(crate) fn image(&mut self, across: u32) -> Option<Arc<PointerImage>> {
        let picture = self.picture.as_ref()?;
        if let Some((made_for, image)) = &self.made
            && *made_for == across
        {
            return Some(Arc::clone(image));
        }
        let scale = if across == 0 || picture.screen_width == 0 {
            1.0
        } else {
            f64::from(across) / f64::from(picture.screen_width)
        };
        let image = Arc::new(image(picture, scale));
        self.made = Some((across, Arc::clone(&image)));
        Some(image)
    }
}

/// One pixel of a pointer: its colour, and whether the screen is kept
/// under it.
type Pixel = ([u8; 4], bool);

/// `picture` as a pointer of this computer, `scale` times its size.
pub(crate) fn image(picture: &PointerPicture, scale: f64) -> PointerImage {
    let scale = scale.clamp(SMALLEST, LARGEST);
    let (from_width, from_height) = (u32::from(picture.width), u32::from(picture.height));
    let sized = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let (width, height) = (sized(from_width), sized(from_height));
    let pixels = pixels(picture);
    // A colour with an alpha blends between its pixels; the others flip
    // the screen, and a pixel between two that flip means nothing.
    let blends = picture.kind == PointerKind::Color;
    let taps = |from: u32, to: u32| {
        if blends {
            smooth_taps(from, to)
        } else {
            nearest_taps(from, to)
        }
    };
    let (across, down) = (taps(from_width, width), taps(from_height, height));
    let from = from_width as usize;
    let mut colour = Vec::with_capacity(width as usize * height as usize * 4);
    let mask_row = (width as usize).div_ceil(16) * 2;
    let mut mask = vec![0; mask_row * height as usize];
    for (y, rows) in down.iter().enumerate() {
        for (x, columns) in across.iter().enumerate() {
            let (painted, kept) = if blends {
                blended(&pixels, from, rows, columns)
            } else {
                pixels[rows[0].0 * from + columns[0].0]
            };
            colour.extend_from_slice(&painted);
            if kept {
                mask[y * mask_row + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    let hot = |at: u16, side: u32| ((f64::from(at) * scale).round() as u32).min(side - 1);
    PointerImage {
        width,
        height,
        hot_x: hot(picture.hot_x, width),
        hot_y: hot(picture.hot_y, height),
        colour,
        mask,
    }
}

/// The picture's pixels at its own size, a row after the other.
fn pixels(picture: &PointerPicture) -> Vec<Pixel> {
    let (width, rows) = (usize::from(picture.width), usize::from(picture.height));
    let (pitch, bytes) = (picture.row_bytes(), &picture.bytes);
    let mut pixels = Vec::with_capacity(width * rows);
    for y in 0..rows {
        for x in 0..width {
            pixels.push(match picture.kind {
                PointerKind::Monochrome => {
                    let bit = |row: usize| (bytes[row * pitch + x / 8] >> (7 - x % 8)) & 1 == 1;
                    let flipped = if bit(y + rows) { 255 } else { 0 };
                    ([flipped, flipped, flipped, 0], bit(y))
                }
                PointerKind::Color => {
                    let at = y * pitch + x * 4;
                    let alpha = bytes[at + 3];
                    if alpha == 0 {
                        ([0; 4], true)
                    } else {
                        ([bytes[at], bytes[at + 1], bytes[at + 2], alpha], false)
                    }
                }
                PointerKind::MaskedColor => {
                    let at = y * pitch + x * 4;
                    (
                        [bytes[at], bytes[at + 1], bytes[at + 2], 0],
                        bytes[at + 3] != 0,
                    )
                }
            });
        }
    }
    pixels
}

/// The pixels a row or column of `to` takes from one of `from`, and how
/// much of each: the nearest.
fn nearest_taps(from: u32, to: u32) -> Vec<Vec<(usize, f64)>> {
    let ratio = f64::from(from) / f64::from(to);
    (0..to)
        .map(|at| {
            let nearest = ((f64::from(at) + 0.5) * ratio) as usize;
            vec![(nearest.min(from as usize - 1), 1.0)]
        })
        .collect()
}

/// The same, smoothly: two neighbours weighed by how close they are when
/// it grows, everything it covers weighed by how much when it shrinks.
fn smooth_taps(from: u32, to: u32) -> Vec<Vec<(usize, f64)>> {
    let ratio = f64::from(from) / f64::from(to);
    let last = from as usize - 1;
    (0..to)
        .map(|at| {
            let at = f64::from(at);
            if ratio <= 1.0 {
                let centre = ((at + 0.5) * ratio - 0.5).max(0.0);
                let left = (centre.floor() as usize).min(last);
                let right = (left + 1).min(last);
                let towards = (centre - left as f64).clamp(0.0, 1.0);
                vec![(left, 1.0 - towards), (right, towards)]
            } else {
                let (start, end) = (at * ratio, (at + 1.0) * ratio);
                (start.floor() as usize..(end.ceil() as usize).min(from as usize))
                    .map(|each| {
                        let covered = end.min(each as f64 + 1.0) - start.max(each as f64);
                        (each, covered / ratio)
                    })
                    .collect()
            }
        })
        .collect()
}

/// One pixel of a colour pointer, weighed from those it covers: blended
/// with their alpha, so that a clear pixel lends nothing of its colour.
fn blended(
    pixels: &[Pixel],
    width: usize,
    rows: &[(usize, f64)],
    columns: &[(usize, f64)],
) -> Pixel {
    let (mut sums, mut alpha) = ([0.0; 3], 0.0);
    for &(y, down) in rows {
        for &(x, across) in columns {
            let (colour, _) = pixels[y * width + x];
            let weight = down * across * f64::from(colour[3]);
            for (sum, channel) in sums.iter_mut().zip(colour) {
                *sum += weight * f64::from(channel);
            }
            alpha += weight;
        }
    }
    let opacity = alpha.round().min(255.0) as u8;
    if opacity == 0 {
        return ([0; 4], true);
    }
    let channel = |sum: f64| (sum / alpha).round().min(255.0) as u8;
    (
        [
            channel(sums[0]),
            channel(sums[1]),
            channel(sums[2]),
            opacity,
        ],
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEPT: [u8; 4] = [0; 4];

    fn picture(kind: PointerKind, width: u16, height: u16, bytes: Vec<u8>) -> PointerPicture {
        PointerPicture {
            kind,
            width,
            height,
            hot_x: 1,
            hot_y: 1,
            screen_width: 1000,
            bytes,
        }
    }

    fn colour_at(image: &PointerImage, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        image.colour[at..at + 4].try_into().unwrap()
    }

    fn kept_at(image: &PointerImage, x: u32, y: u32) -> bool {
        let at = y as usize * image.mask_row_bytes() + x as usize / 8;
        image.mask[at] & (0x80 >> (x % 8)) != 0
    }

    #[test]
    fn a_colour_pointer_keeps_its_pixels_and_shows_the_screen_where_it_is_clear() {
        let bytes = vec![10, 20, 30, 255, 1, 2, 3, 0, 40, 50, 60, 128, 7, 7, 7, 0];
        let image = image(&picture(PointerKind::Color, 2, 2, bytes), 1.0);
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(colour_at(&image, 0, 0), [10, 20, 30, 255]);
        assert!(!kept_at(&image, 0, 0));
        // A clear pixel lends nothing: no colour, the screen kept.
        assert_eq!(colour_at(&image, 1, 0), KEPT);
        assert!(kept_at(&image, 1, 0));
        assert_eq!(colour_at(&image, 0, 1), [40, 50, 60, 128]);
    }

    #[test]
    fn a_black_and_white_pointer_becomes_a_mask_and_the_bits_to_flip() {
        // 8 pixels wide, one row: AND mask, then XOR mask.
        let bytes = vec![0b0011_0000, 0b0101_0000];
        let image = image(&picture(PointerKind::Monochrome, 8, 1, bytes), 1.0);
        // AND 0, XOR 0: black. AND 0, XOR 1: white.
        assert_eq!(
            (colour_at(&image, 0, 0), kept_at(&image, 0, 0)),
            (KEPT, false)
        );
        assert_eq!(
            (colour_at(&image, 1, 0), kept_at(&image, 1, 0)),
            ([255, 255, 255, 0], false)
        );
        // AND 1, XOR 0: the screen. AND 1, XOR 1: the screen flipped.
        assert_eq!(
            (colour_at(&image, 2, 0), kept_at(&image, 2, 0)),
            (KEPT, true)
        );
        assert_eq!(
            (colour_at(&image, 3, 0), kept_at(&image, 3, 0)),
            ([255, 255, 255, 0], true)
        );
    }

    #[test]
    fn a_masked_pointer_paints_or_flips_with_no_alpha() {
        let bytes = vec![10, 20, 30, 0, 40, 50, 60, 255];
        let image = image(&picture(PointerKind::MaskedColor, 2, 1, bytes), 1.0);
        assert_eq!(
            (colour_at(&image, 0, 0), kept_at(&image, 0, 0)),
            ([10, 20, 30, 0], false)
        );
        assert_eq!(
            (colour_at(&image, 1, 0), kept_at(&image, 1, 0)),
            ([40, 50, 60, 0], true)
        );
    }

    #[test]
    fn a_pointer_grows_with_its_point() {
        let bytes = vec![0b1000_0000, 0b0100_0000, 0, 0];
        let image = image(&picture(PointerKind::Monochrome, 8, 2, bytes), 2.0);
        assert_eq!((image.width, image.height), (16, 4));
        assert_eq!((image.hot_x, image.hot_y), (2, 2));
        // Each pixel becomes four, and none is blended with its neighbour.
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert!(kept_at(&image, x, y), "{x},{y}");
        }
        assert!(!kept_at(&image, 2, 0));
        assert!(kept_at(&image, 2, 2) && kept_at(&image, 3, 3));
        assert!(!kept_at(&image, 0, 2));
    }

    #[test]
    fn a_colour_pointer_shrinks_by_what_its_pixels_cover() {
        // Two opaque white pixels over two clear ones, halved.
        let bytes = vec![
            255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let image = image(&picture(PointerKind::Color, 2, 2, bytes), 0.5);
        assert_eq!((image.width, image.height), (1, 1));
        // Half covered: half as opaque, and still white.
        assert_eq!(colour_at(&image, 0, 0), [255, 255, 255, 128]);
        assert!(!kept_at(&image, 0, 0));
        assert_eq!((image.hot_x, image.hot_y), (0, 0));
    }

    #[test]
    fn a_colour_pointer_grows_smoothly_between_its_pixels() {
        // Opaque black then opaque white, made four pixels wide.
        let bytes = vec![0, 0, 0, 255, 255, 255, 255, 255];
        let image = image(&picture(PointerKind::Color, 2, 1, bytes), 2.0);
        let greys: Vec<u8> = (0..4).map(|x| colour_at(&image, x, 0)[0]).collect();
        assert_eq!(greys.first(), Some(&0));
        assert_eq!(greys.last(), Some(&255));
        assert!(greys.windows(2).all(|pair| pair[0] <= pair[1]), "{greys:?}");
        assert!(greys[1] > 0 && greys[2] < 255, "{greys:?}");
    }

    #[test]
    fn a_scale_far_from_the_picture_is_held_to_what_a_pointer_can_be() {
        let bytes = vec![0; 2];
        let large = image(&picture(PointerKind::Monochrome, 8, 1, bytes.clone()), 50.0);
        assert_eq!((large.width, large.height), (32, 4));
        let small = image(&picture(PointerKind::Monochrome, 8, 1, bytes), 0.01);
        assert_eq!((small.width, small.height), (2, 1));
    }

    #[test]
    fn mask_rows_are_padded_to_two_bytes() {
        let image = image(&picture(PointerKind::Monochrome, 3, 2, vec![0xff; 4]), 1.0);
        assert_eq!(image.mask_row_bytes(), 2);
        assert_eq!(image.mask, vec![0b1110_0000, 0, 0b1110_0000, 0]);
    }

    #[test]
    fn the_far_pointer_is_drawn_at_the_scale_its_screen_is_shown() {
        let mut far = FarPointer::default();
        assert_eq!(far.image(500), None);
        far.set(Some(picture(PointerKind::Monochrome, 8, 2, vec![0; 4])));
        // A far screen 1000 pixels wide shown 500 wide: half its size.
        let half = far.image(500).unwrap();
        assert_eq!((half.width, half.height), (4, 1));
        // Asked again for the same: the very same image.
        assert!(Arc::ptr_eq(&half, &far.image(500).unwrap()));
        // Before anything is shown, as it is over there.
        let whole = far.image(0).unwrap();
        assert_eq!((whole.width, whole.height), (8, 2));
        far.set(None);
        assert_eq!(far.image(500), None);
    }
}
