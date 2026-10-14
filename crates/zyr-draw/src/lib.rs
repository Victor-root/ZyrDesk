//! What ZyrDesk is drawn with.
//!
//! Every window of the product is painted by the program itself, with no
//! browser and no toolkit: this is what they paint with. The canvas, the
//! design system its colours and sizes come from, the icons, the ZyrDesk
//! mark, the abstract picture it draws for itself, the picture the home
//! window lies on, the pictures it reads from files, the beat whatever moves keeps time to, and how far along
//! its way each moving thing is.
//!
//! It knows how to draw and nothing about what: which screen shows what,
//! and when, belongs to the windows that use it. The design system, the
//! icons and what moves are plain values, read and tried on any system;
//! what turns them into pixels is Windows', like the windows it dresses.

#[cfg(any(windows, test))]
mod art;
pub mod design;
pub mod icons;
pub mod motion;
#[cfg(any(windows, test))]
mod path;
#[cfg(any(windows, test))]
mod picture;

#[cfg(windows)]
pub mod backdrop;
#[cfg(windows)]
mod canvas;
#[cfg(windows)]
pub mod mark;
#[cfg(windows)]
pub mod pulse;

#[cfg(windows)]
pub use art::Scene;
#[cfg(windows)]
pub use canvas::{Align, Canvas, Family, Overflow, Pen, Rect};
pub use icons::{Icon, Stroke};
#[cfg(windows)]
pub use picture::Picture;
