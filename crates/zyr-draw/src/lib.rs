//! What ZyrDesk is drawn with.
//!
//! Every window of the product is painted by the program itself, with no
//! browser and no toolkit: this is what they paint with. The canvas, the
//! design system its colours and sizes come from, the icons, the ZyrDesk
//! mark, and the beat whatever moves keeps time to.
//!
//! It knows how to draw and nothing about what: which screen shows what,
//! and when, belongs to the windows that use it. The design system and
//! the icons are plain values, read and tried on any system; what turns
//! them into pixels is Windows', like the windows it dresses.

pub mod design;
pub mod icons;
#[cfg(any(windows, test))]
mod path;

#[cfg(windows)]
mod canvas;
#[cfg(windows)]
pub mod mark;
#[cfg(windows)]
pub mod pulse;

#[cfg(windows)]
pub use canvas::{Align, Canvas, Overflow, Pen, Rect};
pub use icons::{Icon, Stroke};
