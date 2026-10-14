//! The scenery the banner of the home window and the bar on its left are
//! dressed in, in each theme.
//!
//! Soft dunes made for ZyrDesk: under a veiled sun in the light theme,
//! under the moon in the dark one. The banner and the bar show the same
//! picture, each covering its own place, so the two read as one view.
//! Carried by the program, not read from beside it, so nothing can be
//! missing; and read once, the first time a window asks for it.

use std::sync::OnceLock;

use crate::picture::Picture;

/// The scenery of this theme, or nothing in the rare case the system
/// could not read it.
pub fn scenery(light: bool) -> Option<&'static Picture> {
    static READ: [OnceLock<Option<Picture>>; 2] = [const { OnceLock::new() }; 2];
    let (rank, bytes): (usize, &'static [u8]) = if light {
        (0, include_bytes!("../scenery/light.jpg"))
    } else {
        (1, include_bytes!("../scenery/dark.jpg"))
    };
    READ[rank].get_or_init(|| Picture::decode(bytes)).as_ref()
}
