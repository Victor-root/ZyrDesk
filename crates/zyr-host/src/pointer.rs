//! The pointer's shape, made into what a shader blends over the screen.
//!
//! Windows hands a pointer in one of three forms: a black and white one
//! (two masks: what to keep of the screen, what to flip), a colour one
//! with an alpha, or a colour one whose alpha says, pixel by pixel,
//! whether to paint the colour or to flip the screen's bits with it.
//! All three become the same pair of images: a colour to blend over the
//! screen by its alpha, and a colour to flip the screen's bits with
//! wherever its alpha is set. A pixel is in one of them at most.

/// The three forms Windows hands a pointer in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One bit a pixel: an AND mask, then an XOR mask, each `height / 2`
    /// rows.
    Monochrome,
    /// Blue, green, red and alpha.
    Color,
    /// Blue, green, red, and an alpha that is either 0 (paint the colour)
    /// or 255 (flip the screen's bits with it).
    MaskedColor,
}

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

/// The shape of a pointer of `kind`, `width` by `height` as Windows says
/// them, its rows `pitch` bytes apart in `bytes`; nothing if the bytes
/// cannot hold it.
pub(crate) fn shape(
    kind: Kind,
    width: u32,
    height: u32,
    pitch: u32,
    bytes: &[u8],
) -> Option<Shape> {
    let rows = match kind {
        Kind::Monochrome => height / 2,
        Kind::Color | Kind::MaskedColor => height,
    };
    let (width_px, rows_px, pitch) = (width as usize, rows as usize, pitch as usize);
    let needed_rows = if kind == Kind::Monochrome {
        rows_px * 2
    } else {
        rows_px
    };
    let row_bytes = match kind {
        Kind::Monochrome => width_px.div_ceil(8),
        Kind::Color | Kind::MaskedColor => width_px * 4,
    };
    if width_px == 0 || rows_px == 0 || pitch < row_bytes || bytes.len() < pitch * needed_rows {
        return None;
    }
    let mut paint = Vec::with_capacity(width_px * rows_px * 4);
    let mut flip = Vec::with_capacity(width_px * rows_px * 4);
    for y in 0..rows_px {
        for x in 0..width_px {
            let (painted, flipped) = match kind {
                Kind::Monochrome => {
                    let bit = |row: usize| (bytes[row * pitch + x / 8] >> (7 - x % 8)) & 1;
                    match (bit(y), bit(y + rows_px)) {
                        (0, 0) => (BLACK, CLEAR),
                        (0, _) => (WHITE, CLEAR),
                        (_, 0) => (CLEAR, CLEAR),
                        // Flipping every bit: white.
                        _ => (CLEAR, WHITE),
                    }
                }
                Kind::Color => {
                    let at = y * pitch + x * 4;
                    let pixel = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
                    (pixel, CLEAR)
                }
                Kind::MaskedColor => {
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
    Some(Shape {
        width,
        height: rows,
        paint,
        flip,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [image[at], image[at + 1], image[at + 2], image[at + 3]]
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
        let shape = shape(Kind::Monochrome, 8, 4, 4, &bytes).unwrap();
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
        let shape = shape(Kind::Color, 2, 1, 8, &bytes).unwrap();
        assert_eq!(shape.paint, bytes);
        assert!(shape.flip.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn a_masked_pointer_paints_or_flips_pixel_by_pixel() {
        let bytes = [10, 20, 30, 0, 40, 50, 60, 255];
        let shape = shape(Kind::MaskedColor, 2, 1, 8, &bytes).unwrap();
        assert_eq!(pixel(&shape.paint, 2, 0, 0), [10, 20, 30, 255]);
        assert_eq!(pixel(&shape.flip, 2, 0, 0), CLEAR);
        assert_eq!(pixel(&shape.paint, 2, 1, 0), CLEAR);
        assert_eq!(pixel(&shape.flip, 2, 1, 0), [40, 50, 60, 255]);
    }

    #[test]
    fn a_shape_the_bytes_cannot_hold_is_refused() {
        assert_eq!(shape(Kind::Color, 4, 4, 16, &[0; 63]), None);
        assert_eq!(shape(Kind::Color, 4, 4, 8, &[0; 64]), None);
        assert_eq!(shape(Kind::Monochrome, 8, 4, 1, &[0; 3]), None);
        assert_eq!(shape(Kind::Monochrome, 0, 4, 1, &[0; 4]), None);
        assert!(shape(Kind::Monochrome, 8, 4, 1, &[0; 4]).is_some());
    }
}
