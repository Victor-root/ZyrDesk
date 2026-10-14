//! The scenery the banner of the home window and the bar on its left are
//! dressed in, in each theme.
//!
//! Soft dunes made for ZyrDesk: under a veiled sun in the light theme,
//! under the moon in the dark one. The banner and the bar show the same
//! picture, each covering its own place, so the two read as one view.
//! Each comes with a frosted copy, the same picture blurred once when it
//! was made, for the glass laid over it: blurring it at every frame
//! would cost for nothing. Carried by the program, not read from beside
//! it, so nothing can be missing.
//!
//! The picture is kept at several sizes, each half the one before, read
//! once with the care the system takes in bringing a picture down, and
//! the one laid is the smallest still as large as where it goes. The
//! renderer smooths a picture well brought down to half its size, and no
//! further: past that, the fine ripples of the sand break into steps.

/// The widths the sharp scenery is kept at, from the largest.
const WIDTHS: [u32; 3] = [3840, 1920, 960];

/// Which of the kept sizes to lay where the picture is drawn this many
/// pixels wide: the smallest still as wide, or the largest when none is.
fn level(drawn: f32) -> usize {
    WIDTHS
        .iter()
        .rposition(|width| *width as f32 >= drawn)
        .unwrap_or(0)
}

#[cfg(windows)]
pub use reading::{frosted, read_ahead, scenery};

/// What only Windows can do: read the pictures.
#[cfg(windows)]
mod reading {
    use std::sync::OnceLock;

    use super::{WIDTHS, level};
    use crate::picture::Picture;

    const LIGHT: &[u8] = include_bytes!("../scenery/light.jpg");
    const DARK: &[u8] = include_bytes!("../scenery/dark.jpg");

    /// The sharp scenery of this theme at the size to lay over a place of
    /// `cover` pixels, which it covers; nothing in the rare case the
    /// system could not read it.
    pub fn scenery(light: bool, cover: (f32, f32)) -> Option<&'static Picture> {
        // Every size has the same shape: the smallest says it.
        let (wide, high) = at(light, WIDTHS.len() - 1)?.size();
        let drawn = cover.0.max(cover.1 * wide as f32 / high as f32);
        at(light, level(drawn))
    }

    /// The frosted scenery of this theme.
    pub fn frosted(light: bool) -> Option<&'static Picture> {
        static READ: [OnceLock<Option<Picture>>; 2] = [const { OnceLock::new() }; 2];
        let (rank, bytes): (usize, &'static [u8]) = if light {
            (0, include_bytes!("../scenery/light-frosted.jpg"))
        } else {
            (1, include_bytes!("../scenery/dark-frosted.jpg"))
        };
        READ[rank]
            .get_or_init(|| Picture::decode(bytes, (u32::MAX, u32::MAX)))
            .as_ref()
    }

    /// Reads every size of this theme's scenery now, away from the thread
    /// that draws: the largest takes a moment.
    pub fn read_ahead(light: bool) {
        for rank in 0..WIDTHS.len() {
            at(light, rank);
        }
        frosted(light);
    }

    /// The sharp scenery of this theme at the kept size of this rank,
    /// read the first time it is asked for.
    fn at(light: bool, rank: usize) -> Option<&'static Picture> {
        static READ: [[OnceLock<Option<Picture>>; 3]; 2] =
            [const { [const { OnceLock::new() }; 3] }; 2];
        let (theme, bytes) = if light { (0, LIGHT) } else { (1, DARK) };
        READ[theme][rank]
            .get_or_init(|| Picture::decode(bytes, (WIDTHS[rank], 1)))
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_laid_is_the_smallest_still_as_wide_as_where_it_goes() {
        assert_eq!(level(500.0), 2);
        assert_eq!(level(960.0), 2);
        assert_eq!(level(961.0), 1);
        assert_eq!(level(1920.0), 1);
        assert_eq!(level(3000.0), 0);
    }

    #[test]
    fn a_place_wider_than_every_size_takes_the_largest() {
        assert_eq!(level(5000.0), 0);
    }
}
