//! The product's icons, taken stroke for stroke from the pages that used
//! to carry them.
//!
//! Copied and not redrawn: they are the same icons, and redrawing them
//! would give different ones.
//!
//! All together and not each beside its own screen: the session menu and
//! the home window share some, and an icon drawn in two places is
//! heading for the day one of the two changes.
//!
//! All on a grid of twenty-four with a stroke of one point eight, which
//! is what the stylesheet asked of every one of them without exception.
//! The ones with a different grid say so.

use crate::paint::{Icon, Stroke};

/// The common grid and stroke, written once.
const fn icon_of(strokes: &'static [Stroke]) -> Icon {
    Icon {
        grid: 24.0,
        thickness: 1.8,
        strokes,
    }
}

pub const FULL_SCREEN: Icon = icon_of(&[
    Stroke::SvgPath("M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3"),
    Stroke::SvgPath("M8 21H5a2 2 0 0 1-2-2v-3M16 21h3a2 2 0 0 0 2-2v-3"),
]);

pub const STATISTICS: Icon = icon_of(&[Stroke::SvgPath("M3 20V10M9 20V4M15 20v-7M21 20V8")]);

pub const MOUSE: Icon = icon_of(&[
    Stroke::RoundRect(7.0, 2.5, 10.0, 19.0, 5.0),
    Stroke::SvgPath("M12 7v3"),
]);

pub const SOUND: Icon = icon_of(&[
    Stroke::SvgPath("M11 5 6.5 9H3v6h3.5L11 19z"),
    Stroke::SvgPath("M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13"),
]);

pub const KEYBOARD: Icon = icon_of(&[
    Stroke::RoundRect(2.0, 5.0, 20.0, 14.0, 2.0),
    Stroke::SvgPath("M6 9h1M9.5 9h1M13 9h1M16.5 9h1M6 13h1M9.5 13h5M17 13h1"),
]);

/// The clipboard: the board, the clip that holds it at the top, and the
/// two lines of what is laid on it.
pub const CLIPBOARD: Icon = icon_of(&[
    Stroke::RoundRect(4.0, 4.5, 16.0, 17.5, 2.0),
    Stroke::RoundRect(8.5, 2.0, 7.0, 4.5, 1.5),
    Stroke::SvgPath("M8 12h8M8 16h5"),
]);

pub const CAD: Icon = icon_of(&[
    Stroke::RoundRect(2.5, 6.0, 19.0, 12.0, 2.0),
    Stroke::SvgPath("M6 10h1M9.5 10h1M13 10h1M16.5 10h1M6 14h12"),
]);

pub const LOCK: Icon = icon_of(&[
    Stroke::RoundRect(4.0, 10.5, 16.0, 10.5, 2.0),
    Stroke::SvgPath("M8 10.5V7a4 4 0 0 1 8 0v3.5M12 14.5v2.5"),
]);

pub const HIDE: Icon = icon_of(&[
    Stroke::SvgPath(
        "M10.6 6.2A9.9 9.9 0 0 1 12 6c5 0 9 4.5 10 6a15 15 0 0 1-3 3.6M6.1 8.3C4.4 9.5 3.3 11 3 12c1 1.5 5 6 9 6a9.6 9.6 0 0 0 3.6-.7",
    ),
    Stroke::SvgPath("M9.9 9.9a3 3 0 0 0 4.2 4.2M3 3l18 18"),
]);

pub const QUIT: Icon = icon_of(&[
    Stroke::SvgPath("M12 3v9"),
    Stroke::SvgPath("M18.4 6.6a9 9 0 1 1-12.8 0"),
]);

pub const RESOLUTION: Icon = icon_of(&[
    Stroke::RoundRect(2.5, 4.0, 19.0, 13.0, 2.0),
    Stroke::SvgPath("M9 21h6M12 17v4"),
]);

/// The two computers of a session, as the product's logo draws
/// them: the far one behind, the one looking at it in front.
pub const HOST_SCREEN: Icon = icon_of(&[OVER_THERE, HERE]);

/// Each of the two alone.
///
/// What it takes to light up one of them over the pair without redrawing
/// the pair, which is how a badge says which of the two computers is
/// holding things up. Written from the same two strokes as the pair:
/// copied, they would drift away from it at the first pixel changed.
///
/// The far one is only drawn over the pair, never alone: its outline
/// stops where the other begins, and only the pair puts back what it is
/// missing.
pub const SCREEN_OVER_THERE: Icon = icon_of(&[OVER_THERE]);
pub const SCREEN_HERE: Icon = icon_of(&[HERE]);

/// The far one: the same rectangle as the other, but open at the two
/// places where the one in front covers it.
///
/// Open and not whole, because two whole outlines cross each other: four
/// strokes used to cross in a square two units high, and at the size of a
/// badge that makes a smudge instead of two computers. Stopping where the
/// other begins is how a drawing says "behind" without needing to be
/// filled, so without having to know the colour of what lies underneath.
const OVER_THERE: Stroke = Stroke::SvgPath(
    "M15 11V5.5A1.5 1.5 0 0 0 13.5 4H3.5A1.5 1.5 0 0 0 2 5.5V11.5A1.5 1.5 0 0 0 3.5 13H9",
);
const HERE: Stroke = Stroke::RoundRect(9.0, 11.0, 13.0, 9.0, 1.5);

pub const BITRATE: Icon = icon_of(&[Stroke::SvgPath("M3 12h3l3-7 4 14 3-7h5")]);

pub const CODEC: Icon = icon_of(&[
    Stroke::RoundRect(5.0, 5.0, 14.0, 14.0, 2.0),
    Stroke::SvgPath("M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"),
]);

pub const FAR_SCREEN: Icon = icon_of(&[
    Stroke::RoundRect(2.5, 4.0, 19.0, 13.0, 2.0),
    Stroke::SvgPath("M9 21h6M12 17v4M7 10.5h3l1.5-3 2 6 1.5-3h2"),
]);

/// The link between the two computers: three arcs and a dot, the drawing
/// everyone reads as "network" without it having to be written.
///
/// The three arcs and the dot turn around one and the same centre, at
/// (12, 18.30), opened by the same angle and spaced by the same step:
/// that is what leaves the same gap of two units and four tenths
/// everywhere. The arcs before had a centre each, so gaps of one and a
/// half, then three and a half, then four, and the dot ended up stuck
/// under the smallest one.
pub const LINK: Icon = icon_of(&[
    Stroke::SvgPath("M2.62 8.59A13.5 13.5 0 0 1 21.38 8.59"),
    Stroke::SvgPath("M5.54 11.61A9.3 9.3 0 0 1 18.46 11.61"),
    Stroke::SvgPath("M8.46 14.63A5.1 5.1 0 0 1 15.54 14.63"),
    Stroke::RoundRect(11.1, 17.4, 1.8, 1.8, 0.9),
]);

pub const CHEVRON: Icon = icon_of(&[Stroke::SvgPath("M9 5l7 7-7 7")]);

pub const BACK: Icon = icon_of(&[Stroke::SvgPath("M15 5l-7 7 7 7")]);

/// The mark of what is chosen in a list. Thicker than the others, as
/// in the page: it is a tick and not a drawing.
pub const TICK: Icon = Icon {
    grid: 24.0,
    thickness: 2.2,
    strokes: &[Stroke::SvgPath("M4 12.5l5.5 5.5L20 6")],
};
/* ---- The home window ------------------------------------------------- */

pub const JOURNAL: Icon = icon_of(&[
    Stroke::SvgPath("M5 3h11l3 3v15H5z"),
    Stroke::SvgPath("M9 9h6M9 13h6M9 17h4"),
]);

/// A house: reaching a computer without leaving here.
///
/// The drawing says the place and not the wire, because the place is
/// what makes the difference: the session does not leave the house, and
/// nothing outside can bring it down.
pub const LOCAL_NETWORK: Icon = icon_of(&[
    Stroke::SvgPath("M3 10.5 12 3.5l9 7V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"),
    Stroke::SvgPath("M9.5 21v-6h5v6"),
]);

pub const SETTINGS: Icon = icon_of(&[
    Stroke::SvgPath("M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3"),
    Stroke::SvgPath("M14 2v4M8 10v4M16 18v4"),
]);

pub const CROSS: Icon = icon_of(&[Stroke::SvgPath("M18 6 6 18M6 6l12 12")]);

pub const PLUS: Icon = icon_of(&[Stroke::SvgPath("M12 5v14M5 12h14")]);

/// The chevron of the "Avancé" fold, opening downwards.
pub const CHEVRON_DOWN: Icon = icon_of(&[Stroke::SvgPath("M5 9l7 7 7-7")]);

/// The drawing of the empty screen: a computer, on its own grid.
///
/// Its own because it is not square, and that is what gives it the shape
/// of a screen standing on its foot.
pub const NO_COMPUTER: Icon = Icon {
    grid: 64.0,
    thickness: 2.0,
    strokes: &[
        Stroke::RoundRect(1.5, 1.5, 45.0, 32.0, 4.0),
        Stroke::SvgPath("M17 41h27M24 33.5v7"),
    ],
};
