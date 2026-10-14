//! The picture the home window lies on, in each theme.
//!
//! Golden streams of light over a landscape, drawn for ZyrDesk: the
//! light one over a pale sky, the dark one under a night sky. They are
//! carried by the program, not read from beside it, so nothing can be
//! missing; and read once, the first time a window asks for one.

use std::sync::OnceLock;

use crate::picture::Picture;

const LIGHT: &[u8] = include_bytes!("../backdrop/light.jpg");
const DARK: &[u8] = include_bytes!("../backdrop/dark.jpg");

/// The picture of this theme, or nothing in the rare case the system
/// could not read it, where the window shows its plain background.
pub fn backdrop(light: bool) -> Option<&'static Picture> {
    static READ: [OnceLock<Option<Picture>>; 2] = [OnceLock::new(), OnceLock::new()];
    let (kept, bytes) = if light {
        (&READ[0], LIGHT)
    } else {
        (&READ[1], DARK)
    };
    kept.get_or_init(|| Picture::decode(bytes)).as_ref()
}
