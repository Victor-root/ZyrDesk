//! The pointer's shape, made into what a shader blends over the screen.
//!
//! Windows hands a pointer in one of three forms: a black and white one
//! (two masks: what to keep of the screen, what to flip), a colour one
//! with an alpha, or a colour one whose alpha says, pixel by pixel,
//! whether to paint the colour or to flip the screen's bits with it.
//! All three become the same pair of images: a colour to blend over the
//! screen by its alpha, and a colour to flip the screen's bits with
//! wherever its alpha is set. A pixel is in one of them at most.

use zyr_media::pointer::{PointerKind, PointerPicture};

/// A pointer ready to be drawn: two images of blue, green, red and alpha
/// bytes, `width * 4` bytes a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Blended over the screen by its alpha (not premultiplied).
    pub(crate) paint: Vec<u8>,
    /// Where its alpha is set, the screen's bits are flipped with its
    /// colour.
    pub(crate) flip: Vec<u8>,
}

const CLEAR: [u8; 4] = [0, 0, 0, 0];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The shape of the pointer in `picture`.
pub(crate) fn shape(picture: &PointerPicture) -> Shape {
    let (width, rows) = (usize::from(picture.width), usize::from(picture.height));
    let (pitch, bytes) = (picture.row_bytes(), &picture.bytes);
    let mut paint = Vec::with_capacity(width * rows * 4);
    let mut flip = Vec::with_capacity(width * rows * 4);
    for y in 0..rows {
        for x in 0..width {
            let (painted, flipped) = match picture.kind {
                PointerKind::Monochrome => {
                    let bit = |row: usize| (bytes[row * pitch + x / 8] >> (7 - x % 8)) & 1;
                    match (bit(y), bit(y + rows)) {
                        (0, 0) => (BLACK, CLEAR),
                        (0, _) => (WHITE, CLEAR),
                        (_, 0) => (CLEAR, CLEAR),
                        // Flipping every bit: white.
                        _ => (CLEAR, WHITE),
                    }
                }
                PointerKind::Color => {
                    let at = y * pitch + x * 4;
                    let pixel = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
                    (pixel, CLEAR)
                }
                PointerKind::MaskedColor => {
                    let at = y * pitch + x * 4;
                    let colour = [bytes[at], bytes[at + 1], bytes[at + 2], 255];
                    if bytes[at + 3] == 0 {
                        (colour, CLEAR)
                    } else {
                        (CLEAR, colour)
                    }
                }
            };
            paint.extend_from_slice(&painted);
            flip.extend_from_slice(&flipped);
        }
    }
    Shape {
        width: u32::from(picture.width),
        height: u32::from(picture.height),
        paint,
        flip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [image[at], image[at + 1], image[at + 2], image[at + 3]]
    }

    fn picture(kind: PointerKind, width: u32, height: u32, pitch: u32, bytes: &[u8]) -> Shape {
        shape(&PointerPicture::from_rows(kind, width, height, pitch, (0, 0), 0, bytes).unwrap())
    }

    #[test]
    fn a_black_and_white_pointer_paints_keeps_and_flips() {
        // 8 pixels wide, 2 rows: AND mask rows then XOR mask rows, one
        // byte a row, padded to a pitch of 4.
        let bytes = [
            0b0011_0000,
            0,
            0,
            0, // AND, row 0
            0b0000_1111,
            0,
            0,
            0, // AND, row 1
            0b0101_0000,
            0,
            0,
            0, // XOR, row 0
            0b0000_0101,
            0,
            0,
            0, // XOR, row 1
        ];
        let shape = picture(PointerKind::Monochrome, 8, 4, 4, &bytes);
        assert_eq!((shape.width, shape.height), (8, 2));
        // AND 0, XOR 0: black.
        assert_eq!(pixel(&shape.paint, 8, 0, 0), BLACK);
        // AND 0, XOR 1: white.
        assert_eq!(pixel(&shape.paint, 8, 1, 0), WHITE);
        // AND 1, XOR 0: the screen shows through.
        assert_eq!(pixel(&shape.paint, 8, 2, 0), CLEAR);
        assert_eq!(pixel(&shape.flip, 8, 2, 0), CLEAR);
        // AND 1, XOR 1: the screen is flipped.
        assert_eq!(pixel(&shape.paint, 8, 3, 0), CLEAR);
        assert_eq!(pixel(&shape.flip, 8, 3, 0), WHITE);
        assert_eq!(pixel(&shape.flip, 8, 5, 1), WHITE);
        assert_eq!(pixel(&shape.paint, 8, 4, 1), CLEAR);
    }

    #[test]
    fn a_colour_pointer_is_blended_as_it_is() {
        let bytes = [10, 20, 30, 128, 1, 2, 3, 0];
        let shape = picture(PointerKind::Color, 2, 1, 8, &bytes);
        assert_eq!(shape.paint, bytes);
        assert!(shape.flip.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn a_masked_pointer_paints_or_flips_pixel_by_pixel() {
        let bytes = [10, 20, 30, 0, 40, 50, 60, 255];
        let shape = picture(PointerKind::MaskedColor, 2, 1, 8, &bytes);
        assert_eq!(pixel(&shape.paint, 2, 0, 0), [10, 20, 30, 255]);
        assert_eq!(pixel(&shape.flip, 2, 0, 0), CLEAR);
        assert_eq!(pixel(&shape.paint, 2, 1, 0), CLEAR);
        assert_eq!(pixel(&shape.flip, 2, 1, 0), [40, 50, 60, 255]);
    }
}
