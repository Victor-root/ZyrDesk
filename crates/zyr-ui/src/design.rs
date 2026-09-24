//! The ZyrDesk design system, on the side of what is drawn.
//!
//! The colours, spacings, radii, shadows and text sizes are written only
//! once, in `design.css`, and read from here at compile time. Nothing is
//! copied: two copies of a palette make two palettes, and the first
//! colour changed in one of them is the day the product stops looking
//! like itself.
//!
//! No browser reads this file any more. It keeps its notation because
//! that notation writes two themes side by side, and because reading it
//! at compile time is what checks that both really name the same roles.
//!
//! Every role is extracted, including the ones nothing draws yet: the
//! design system is a palette, not a shopping list. Extracting only what
//! is used today would mean reopening it for every screen brought over,
//! which opens the door to a second palette written by hand in the
//! meantime.
#![allow(dead_code)]

/// A colour, as four numbers between zero and one, which is how
/// everything that draws wants them.
#[derive(Clone, Copy)]
pub struct Colour {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Colour {
    /// Nothing at all.
    ///
    /// What a layered window starts its drawing on: wherever the
    /// canvas stays this colour, what is behind the window shows
    /// through, and clicks go through there.
    pub const TRANSPARENT: Colour = Colour {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 0.0,
    };

    /// Solid black.
    ///
    /// Of which only the portion used matters: it is what a dialog
    /// box lays over what it covers, and the only place in the
    /// product where a colour is not a role.
    pub const BLACK: Colour = Colour {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };

    /// This one mixed with that one, in this proportion.
    ///
    /// What the stylesheet writes as `color-mix(in srgb, ... 8%,
    /// ...)`: the tint of a role washed over a background, where
    /// laying down a second solid colour would give one more colour to
    /// maintain.
    pub fn mixed_with(self, background: Colour, part: f32) -> Colour {
        let between = |mine: f32, theirs: f32| theirs + (mine - theirs) * part;
        Colour {
            red: between(self.red, background.red),
            green: between(self.green, background.green),
            blue: between(self.blue, background.blue),
            alpha: between(self.alpha, background.alpha),
        }
    }

    /// The same, laid on as a veil.
    ///
    /// What the stylesheet writes as `color-mix(in srgb, ... 12%,
    /// transparent)`: the tint of a role used as a background, where
    /// repainting with a second colour would give one more colour to
    /// maintain.
    pub fn faded(self, part: f32) -> Colour {
        Colour {
            alpha: self.alpha * part,
            ..self
        }
    }
}

/// A drop shadow: how far it is offset, how blurred it is, and in what
/// colour.
#[derive(Clone, Copy)]
pub struct Shadow {
    pub across: f32,
    pub down: f32,
    pub soft: f32,
    pub tint: Colour,
}

include!(concat!(env!("OUT_DIR"), "/design.rs"));

/// The palette of the theme the window wears.
///
/// Asked of the system rather than kept here: it is the same theme as
/// the home window, and the home window already has it from its own
/// window.
pub fn palette(light: bool) -> Palette {
    if light { LIGHT } else { DARK }
}
