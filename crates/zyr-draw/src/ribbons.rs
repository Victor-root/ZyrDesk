//! The ribbons the banner of the home window and the bar on its left are
//! dressed in, in each theme.
//!
//! Ivory satin ribbons hemmed with gold, made for ZyrDesk. The dark ones
//! are the same ribbons turned to gold on night blue, and the bar's are
//! the banner's stood upright, so the two read as one cloth. All are
//! blurred in the pictures themselves, so the words written over them
//! read and the blur costs nothing when drawn. Carried by
//! the program, not read from beside it, so nothing can be missing; and
//! read once, the first time a window asks for them.

use std::sync::OnceLock;

use crate::picture::Picture;

/// Where the ribbons are laid.
#[derive(Clone, Copy)]
pub enum Place {
    Banner,
    Bar,
}

/// The ribbons of this place in this theme, or nothing in the rare case
/// the system could not read them.
pub fn ribbons(place: Place, light: bool) -> Option<&'static Picture> {
    static READ: [OnceLock<Option<Picture>>; 4] = [const { OnceLock::new() }; 4];
    let (rank, bytes): (usize, &'static [u8]) = match (place, light) {
        (Place::Banner, true) => (0, include_bytes!("../ribbons/banner-light.jpg")),
        (Place::Banner, false) => (1, include_bytes!("../ribbons/banner-dark.jpg")),
        (Place::Bar, true) => (2, include_bytes!("../ribbons/bar-light.jpg")),
        (Place::Bar, false) => (3, include_bytes!("../ribbons/bar-dark.jpg")),
    };
    READ[rank].get_or_init(|| Picture::decode(bytes)).as_ref()
}
