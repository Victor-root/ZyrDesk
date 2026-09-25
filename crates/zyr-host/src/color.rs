//! From the screen's colours to the encoders' own.
//!
//! Screens hand out gamma-encoded red, green and blue; encoders take
//! luma and two chroma planes, the chroma at half size each way (NV12).
//! The pictures are BT.709 in limited range, which is what the encoders
//! announce in the stream and what every decoder expects when nothing
//! says otherwise.

use zyr_codec::Nv12Planes;

use crate::picture::Size;

/// Weights and offset turning gamma-encoded red, green and blue from 0
/// to 1 into one of Y, U or V from 0 to 1 of the 8-bit range:
/// `r * w[0] + g * w[1] + b * w[2] + w[3]`.
pub type Weights = [f32; 4];

/// BT.709 in limited range: luma from 16 to 235, chroma from 16 to 240
/// around 128, out of 255. Rows for Y, U, then V.
pub fn bt709_limited() -> [Weights; 3] {
    const KR: f32 = 0.2126;
    const KB: f32 = 0.0722;
    const KG: f32 = 1.0 - KR - KB;
    let luma = 219.0 / 255.0;
    let chroma = 224.0 / 255.0;
    let middle = 128.0 / 255.0;
    [
        [KR * luma, KG * luma, KB * luma, 16.0 / 255.0],
        [
            -KR / (2.0 * (1.0 - KB)) * chroma,
            -KG / (2.0 * (1.0 - KB)) * chroma,
            0.5 * chroma,
            middle,
        ],
        [
            0.5 * chroma,
            -KG / (2.0 * (1.0 - KR)) * chroma,
            -KB / (2.0 * (1.0 - KR)) * chroma,
            middle,
        ],
    ]
}

/// Luma of black, and chroma of every grey, in limited range.
pub const BLACK: (u8, u8) = (16, 128);

/// Converts a picture of blue, green, red and a spare byte per pixel into
/// NV12, each chroma sample from the average of its two by two pixels.
///
/// `size` is the picture's, both sides even; `bgra` holds its rows every
/// `stride` bytes.
pub fn bgra_to_nv12(bgra: &[u8], stride: usize, size: Size, out: &mut Nv12Planes<'_>) {
    let [y, u, v] = bt709_limited();
    let apply = |w: &Weights, rgb: [f32; 3]| -> u8 {
        let value = rgb[0] * w[0] + rgb[1] * w[1] + rgb[2] * w[2] + w[3];
        (value * 255.0 + 0.5).clamp(0.0, 255.0) as u8
    };
    let (width, height) = (size.width as usize, size.height as usize);
    for row in (0..height).step_by(2) {
        for column in (0..width).step_by(2) {
            let mut sum = [0.0f32; 3];
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let at = (row + dy) * stride + (column + dx) * 4;
                let rgb = [
                    f32::from(bgra[at + 2]) / 255.0,
                    f32::from(bgra[at + 1]) / 255.0,
                    f32::from(bgra[at]) / 255.0,
                ];
                out.luma[(row + dy) * out.luma_stride + column + dx] = apply(&y, rgb);
                for (total, value) in sum.iter_mut().zip(rgb) {
                    *total += value;
                }
            }
            let average = sum.map(|total| total / 4.0);
            let at = row / 2 * out.chroma_stride + column;
            out.chroma[at] = apply(&u, average);
            out.chroma[at + 1] = apply(&v, average);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn converted(rgb: [u8; 3]) -> (u8, u8, u8) {
        let pixel = [rgb[2], rgb[1], rgb[0], 255];
        let bgra: Vec<u8> = pixel.iter().copied().cycle().take(4 * 4).collect();
        let mut luma = [0u8; 4];
        let mut chroma = [0u8; 2];
        let mut planes = Nv12Planes {
            luma: &mut luma,
            luma_stride: 2,
            chroma: &mut chroma,
            chroma_stride: 2,
        };
        bgra_to_nv12(&bgra, 8, Size::new(2, 2), &mut planes);
        assert!(luma.iter().all(|value| *value == luma[0]));
        (luma[0], chroma[0], chroma[1])
    }

    #[test]
    fn black_white_and_grey_have_the_limited_range_values() {
        assert_eq!(converted([0, 0, 0]), (16, 128, 128));
        assert_eq!(converted([255, 255, 255]), (235, 128, 128));
        assert_eq!(converted([128, 128, 128]), (126, 128, 128));
    }

    #[test]
    fn primaries_land_where_bt709_puts_them() {
        // ITU-R BT.709 in 8-bit limited range, as reference tables give
        // them for 100 % colour bars.
        assert_eq!(converted([255, 0, 0]), (63, 102, 240));
        assert_eq!(converted([0, 255, 0]), (173, 42, 26));
        assert_eq!(converted([0, 0, 255]), (32, 240, 118));
    }

    #[test]
    fn chroma_is_the_average_of_its_four_pixels() {
        // Left column red, right column blue.
        let red = [0u8, 0, 255, 255];
        let blue = [255u8, 0, 0, 255];
        let bgra: Vec<u8> = [red, blue, red, blue].concat();
        let mut luma = [0u8; 4];
        let mut chroma = [0u8; 2];
        let mut planes = Nv12Planes {
            luma: &mut luma,
            luma_stride: 2,
            chroma: &mut chroma,
            chroma_stride: 2,
        };
        bgra_to_nv12(&bgra, 8, Size::new(2, 2), &mut planes);
        assert_eq!(luma, [63, 32, 63, 32]);
        // Half red and half blue: (U, V) halfway between theirs.
        assert!(chroma[0].abs_diff(171) <= 1, "{chroma:?}");
        assert!(chroma[1].abs_diff(179) <= 1, "{chroma:?}");
    }
}
