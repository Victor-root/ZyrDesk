//! The product's icons.
//!
//! Two families. Those of a session, taken stroke for stroke from the
//! pages that used to carry them: copied and not redrawn, since they are
//! the same icons and redrawing them would give different ones. And
//! those of the home window, which are Tabler's (MIT licence, credited on
//! the « About » page), written exactly as Tabler publishes them.
//!
//! All together and not each beside its own screen: an icon drawn in two
//! places is heading for the day one of the two changes.
//!
//! All on a grid of twenty-four. A session's are drawn with a stroke of
//! one point eight, Tabler's with one point seven five. The ones with a
//! different grid say so.

/// A piece of an icon, written in the same words as the drawing it
/// comes from.
pub enum Stroke {
    /// The "d" of an SVG path, taken as it is.
    ///
    /// Taken over and not translated: an icon transcribed by hand is an
    /// icon that ends up no longer being the same, and these are already
    /// written once. What is understood is what they use, and `path`
    /// says what that is.
    SvgPath(&'static str),
    /// A rounded rectangle: x, y, width, height and radius.
    RoundRect(f32, f32, f32, f32, f32),
}

/// An icon: its strokes, the grid they are written in, and the
/// thickness of its stroke in that grid.
///
/// It carries its grid with it, as a vector drawing does: that is what
/// lets it be placed in any rect without anyone having to know what
/// units it was drawn in.
pub struct Icon {
    pub grid: f32,
    pub thickness: f32,
    pub strokes: &'static [Stroke],
}

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

/// A speaker standing in a room, for the sound of the far computer.
pub const FAR_SPEAKERS: Icon = icon_of(&[
    Stroke::RoundRect(5.0, 3.0, 14.0, 18.0, 2.0),
    Stroke::SvgPath("M9 14a3 3 0 1 0 6 0a3 3 0 1 0 -6 0M12 7h.01"),
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
/* ---- The home window, in Tabler's icons -------------------------------- */

/// Tabler's grid and stroke, written once.
///
/// A little lighter than a session's: these sit beside text on a calm
/// page, at sizes where Tabler's own two units would weigh more than the
/// words around them.
const fn tabler(strokes: &'static [Stroke]) -> Icon {
    Icon {
        grid: 24.0,
        thickness: 1.75,
        strokes,
    }
}

/// Tabler « home ».
pub const HOME: Icon = tabler(&[
    Stroke::SvgPath("M5 12l-2 0l9 -9l9 9l-2 0"),
    Stroke::SvgPath("M5 12v7a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-7"),
    Stroke::SvgPath("M9 21v-6a2 2 0 0 1 2 -2h2a2 2 0 0 1 2 2v6"),
]);

/// Tabler « device-desktop ».
pub const COMPUTERS: Icon = tabler(&[
    Stroke::SvgPath(
        "M3 5a1 1 0 0 1 1 -1h16a1 1 0 0 1 1 1v10a1 1 0 0 1 -1 1h-16a1 1 0 0 1 -1 -1v-10",
    ),
    Stroke::SvgPath("M7 20h10"),
    Stroke::SvgPath("M9 16v4"),
    Stroke::SvgPath("M15 16v4"),
]);

/// Tabler « users ».
pub const CONTACTS: Icon = tabler(&[
    Stroke::SvgPath("M5 7a4 4 0 1 0 8 0a4 4 0 1 0 -8 0"),
    Stroke::SvgPath("M3 21v-2a4 4 0 0 1 4 -4h4a4 4 0 0 1 4 4v2"),
    Stroke::SvgPath("M16 3.13a4 4 0 0 1 0 7.75"),
    Stroke::SvgPath("M21 21v-2a4 4 0 0 0 -3 -3.85"),
]);

/// Tabler « activity ».
pub const ACTIVITY: Icon = tabler(&[Stroke::SvgPath("M3 12h4l3 8l4 -16l3 8h4")]);

/// Tabler « settings ».
pub const SETTINGS: Icon = tabler(&[
    Stroke::SvgPath(
        "M10.325 4.317c.426 -1.756 2.924 -1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543 -.94 3.31 .826 2.37 2.37a1.724 1.724 0 0 0 1.065 2.572c1.756 .426 1.756 2.924 0 3.35a1.724 1.724 0 0 0 -1.066 2.573c.94 1.543 -.826 3.31 -2.37 2.37a1.724 1.724 0 0 0 -2.572 1.065c-.426 1.756 -2.924 1.756 -3.35 0a1.724 1.724 0 0 0 -2.573 -1.066c-1.543 .94 -3.31 -.826 -2.37 -2.37a1.724 1.724 0 0 0 -1.065 -2.572c-1.756 -.426 -1.756 -2.924 0 -3.35a1.724 1.724 0 0 0 1.066 -2.573c-.94 -1.543 .826 -3.31 2.37 -2.37c1 .608 2.296 .07 2.572 -1.065",
    ),
    Stroke::SvgPath("M9 12a3 3 0 1 0 6 0a3 3 0 0 0 -6 0"),
]);

/// Tabler « info-circle ».
pub const ABOUT: Icon = tabler(&[
    Stroke::SvgPath("M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"),
    Stroke::SvgPath("M12 9h.01"),
    Stroke::SvgPath("M11 12h1v4h1"),
]);

/// Tabler « menu-2 ».
pub const MENU: Icon = tabler(&[
    Stroke::SvgPath("M4 6l16 0"),
    Stroke::SvgPath("M4 12l16 0"),
    Stroke::SvgPath("M4 18l16 0"),
]);

/// Tabler « search ».
pub const SEARCH: Icon = tabler(&[
    Stroke::SvgPath("M3 10a7 7 0 1 0 14 0a7 7 0 1 0 -14 0"),
    Stroke::SvgPath("M21 21l-6 -6"),
]);

/// Tabler « filter ».
pub const FILTER: Icon = tabler(&[Stroke::SvgPath(
    "M4 4h16v2.172a2 2 0 0 1 -.586 1.414l-4.414 4.414v7l-6 2v-8.5l-4.48 -4.928a2 2 0 0 1 -.52 -1.345v-2.227",
)]);

/// Tabler « layout-grid ».
pub const GRID: Icon = tabler(&[
    Stroke::SvgPath("M4 5a1 1 0 0 1 1 -1h4a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1l0 -4"),
    Stroke::SvgPath(
        "M14 5a1 1 0 0 1 1 -1h4a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1l0 -4",
    ),
    Stroke::SvgPath(
        "M4 15a1 1 0 0 1 1 -1h4a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1l0 -4",
    ),
    Stroke::SvgPath(
        "M14 15a1 1 0 0 1 1 -1h4a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1l0 -4",
    ),
]);

/// Tabler « list-details ».
pub const LIST: Icon = tabler(&[
    Stroke::SvgPath("M13 5h8"),
    Stroke::SvgPath("M13 9h5"),
    Stroke::SvgPath("M13 15h8"),
    Stroke::SvgPath("M13 19h5"),
    Stroke::SvgPath("M3 5a1 1 0 0 1 1 -1h4a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1l0 -4"),
    Stroke::SvgPath(
        "M3 15a1 1 0 0 1 1 -1h4a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1l0 -4",
    ),
]);

/// Tabler « device-laptop ».
pub const LAPTOP: Icon = tabler(&[
    Stroke::SvgPath("M3 19l18 0"),
    Stroke::SvgPath(
        "M5 7a1 1 0 0 1 1 -1h12a1 1 0 0 1 1 1v8a1 1 0 0 1 -1 1h-12a1 1 0 0 1 -1 -1l0 -8",
    ),
]);

/// Tabler « world ».
pub const WORLD: Icon = tabler(&[
    Stroke::SvgPath("M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"),
    Stroke::SvgPath("M3.6 9h16.8"),
    Stroke::SvgPath("M3.6 15h16.8"),
    Stroke::SvgPath("M11.5 3a17 17 0 0 0 0 18"),
    Stroke::SvgPath("M12.5 3a17 17 0 0 1 0 18"),
]);

/// Tabler « shield-check ».
pub const SHIELD: Icon = tabler(&[
    Stroke::SvgPath(
        "M11.46 20.846a12 12 0 0 1 -7.96 -14.846a12 12 0 0 0 8.5 -3a12 12 0 0 0 8.5 3a12 12 0 0 1 -.09 7.06",
    ),
    Stroke::SvgPath("M15 19l2 2l4 -4"),
]);

/// Tabler « pencil ».
pub const PENCIL: Icon = tabler(&[
    Stroke::SvgPath("M4 20h4l10.5 -10.5a2.828 2.828 0 1 0 -4 -4l-10.5 10.5v4"),
    Stroke::SvgPath("M13.5 6.5l4 4"),
]);

/// Tabler « brand-windows ».
pub const WINDOWS: Icon = tabler(&[
    Stroke::SvgPath(
        "M17.8 20l-12 -1.5c-1 -.1 -1.8 -.9 -1.8 -1.9v-9.2c0 -1 .8 -1.8 1.8 -1.9l12 -1.5c1.2 -.1 2.2 .8 2.2 1.9v12.1c0 1.2 -1.1 2.1 -2.2 1.9l0 .1",
    ),
    Stroke::SvgPath("M12 5l0 14"),
    Stroke::SvgPath("M4 12l16 0"),
]);

/// Tabler « cpu ».
pub const PROCESSOR: Icon = tabler(&[
    Stroke::SvgPath(
        "M5 6a1 1 0 0 1 1 -1h12a1 1 0 0 1 1 1v12a1 1 0 0 1 -1 1h-12a1 1 0 0 1 -1 -1l0 -12",
    ),
    Stroke::SvgPath("M9 9h6v6h-6l0 -6"),
    Stroke::SvgPath("M3 10h2"),
    Stroke::SvgPath("M3 14h2"),
    Stroke::SvgPath("M10 3v2"),
    Stroke::SvgPath("M14 3v2"),
    Stroke::SvgPath("M21 10h-2"),
    Stroke::SvgPath("M21 14h-2"),
    Stroke::SvgPath("M14 21v-2"),
    Stroke::SvgPath("M10 21v-2"),
]);

/// Tabler « memory ».
pub const MEMORY: Icon = tabler(&[
    Stroke::SvgPath("M12 11v2"),
    Stroke::SvgPath("M16 11v2"),
    Stroke::SvgPath(
        "M3 7h18a1 1 0 0 1 1 1v1a1 1 0 0 1 -1 1a1 1 0 0 0 -1 1v1a1 1 0 0 0 1 1a1 1 0 0 1 1 1v2a1 1 0 0 1 -1 1h-18a1 1 0 0 1 -1 -1v-2a1 1 0 0 1 1 -1a1 1 0 0 0 1 -1v-1a1 1 0 0 0 -1 -1a1 1 0 0 1 -1 -1v-1a1 1 0 0 1 1 -1",
    ),
    Stroke::SvgPath("M8 11v2"),
    Stroke::SvgPath("M4 17v2"),
    Stroke::SvgPath("M7 17v2"),
    Stroke::SvgPath("M10 17v2"),
    Stroke::SvgPath("M20 17v2"),
    Stroke::SvgPath("M17 17v2"),
    Stroke::SvgPath("M13 17v2"),
]);

/// Tabler « gpu ».
pub const GRAPHICS: Icon = tabler(&[
    Stroke::SvgPath("M3 4v14"),
    Stroke::SvgPath("M3 6h17a1 1 0 0 1 1 1v8a1 1 0 0 1 -1 1h-17"),
    Stroke::SvgPath("M9 13a2 2 0 1 0 0 -4a2 2 0 0 0 0 4"),
    Stroke::SvgPath("M15 9v4"),
    Stroke::SvgPath("M18 9v4"),
    Stroke::SvgPath("M7 16v3h8v-3"),
]);

/// Tabler « arrows-move ».
pub const MOVE: Icon = tabler(&[
    Stroke::SvgPath("M18 9l3 3l-3 3"),
    Stroke::SvgPath("M15 12h6"),
    Stroke::SvgPath("M6 9l-3 3l3 3"),
    Stroke::SvgPath("M3 12h6"),
    Stroke::SvgPath("M9 18l3 3l3 -3"),
    Stroke::SvgPath("M12 15v6"),
    Stroke::SvgPath("M15 6l-3 -3l-3 3"),
    Stroke::SvgPath("M12 3v6"),
]);

/// Tabler « power ».
pub const POWER: Icon = tabler(&[
    Stroke::SvgPath("M7 6a7.75 7.75 0 1 0 10 0"),
    Stroke::SvgPath("M12 4l0 8"),
]);

/// Tabler « home-signal ».
pub const LOCAL_NETWORK: Icon = tabler(&[
    Stroke::SvgPath("M15 22v-2"),
    Stroke::SvgPath("M18 22v-4"),
    Stroke::SvgPath("M21 22v-6"),
    Stroke::SvgPath("M19 12.494v-.494h2l-9 -9l-9 9h2v7a2 2 0 0 0 2 2h4"),
    Stroke::SvgPath("M9 21v-6a2 2 0 0 1 2 -2h2a2 2 0 0 1 2 2v.5"),
]);

/// Tabler « arrows-down-up ».
pub const ROUND_TRIP: Icon = tabler(&[
    Stroke::SvgPath("M17 3l0 18"),
    Stroke::SvgPath("M10 18l-3 3l-3 -3"),
    Stroke::SvgPath("M7 21l0 -18"),
    Stroke::SvgPath("M20 6l-3 -3l-3 3"),
]);

/// Tabler « user-plus ».
pub const USER_PLUS: Icon = tabler(&[
    Stroke::SvgPath("M8 7a4 4 0 1 0 8 0a4 4 0 0 0 -8 0"),
    Stroke::SvgPath("M16 19h6"),
    Stroke::SvgPath("M19 16v6"),
    Stroke::SvgPath("M6 21v-2a4 4 0 0 1 4 -4h4"),
]);

/// Tabler « plus ».
pub const PLUS: Icon = tabler(&[Stroke::SvgPath("M12 5l0 14"), Stroke::SvgPath("M5 12l14 0")]);

/// Tabler « file-description ».
pub const REPORT: Icon = tabler(&[
    Stroke::SvgPath("M14 3v4a1 1 0 0 0 1 1h4"),
    Stroke::SvgPath("M17 21h-10a2 2 0 0 1 -2 -2v-14a2 2 0 0 1 2 -2h7l5 5v11a2 2 0 0 1 -2 2"),
    Stroke::SvgPath("M9 17h6"),
    Stroke::SvgPath("M9 13h6"),
]);

/// Tabler « arrow-right ».
pub const ARROW_RIGHT: Icon = tabler(&[
    Stroke::SvgPath("M5 12l14 0"),
    Stroke::SvgPath("M13 18l6 -6"),
    Stroke::SvgPath("M13 6l6 6"),
]);

/// Tabler « chevron-down ».
pub const CHEVRON_DOWN: Icon = tabler(&[Stroke::SvgPath("M6 9l6 6l6 -6")]);

/// Tabler « x ».
pub const CROSS: Icon = tabler(&[
    Stroke::SvgPath("M18 6l-12 12"),
    Stroke::SvgPath("M6 6l12 12"),
]);

/// Tabler « copy ».
pub const COPY: Icon = tabler(&[
    Stroke::SvgPath(
        "M7 9.667a2.667 2.667 0 0 1 2.667 -2.667h8.666a2.667 2.667 0 0 1 2.667 2.667v8.666a2.667 2.667 0 0 1 -2.667 2.667h-8.666a2.667 2.667 0 0 1 -2.667 -2.667l0 -8.666",
    ),
    Stroke::SvgPath(
        "M4.012 16.737a2.005 2.005 0 0 1 -1.012 -1.737v-10c0 -1.1 .9 -2 2 -2h10c.75 0 1.158 .385 1.5 1",
    ),
]);

/// Tabler « check ».
pub const CHECK: Icon = tabler(&[Stroke::SvgPath("M5 12l5 5l10 -10")]);

/// Tabler « circle-check ».
pub const DONE: Icon = tabler(&[
    Stroke::SvgPath("M3 12a9 9 0 1 0 18 0a9 9 0 1 0 -18 0"),
    Stroke::SvgPath("M9 12l2 2l4 -4"),
]);

/// Tabler « alert-circle ».
pub const TROUBLE: Icon = tabler(&[
    Stroke::SvgPath("M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"),
    Stroke::SvgPath("M12 8v4"),
    Stroke::SvgPath("M12 16h.01"),
]);

/// Tabler « folder-open ».
pub const FOLDER: Icon = tabler(&[Stroke::SvgPath(
    "M5 19l2.757 -7.351a1 1 0 0 1 .936 -.649h12.307a1 1 0 0 1 .986 1.164l-.996 5.211a2 2 0 0 1 -1.964 1.625h-14.026a2 2 0 0 1 -2 -2v-11a2 2 0 0 1 2 -2h4l3 3h7a2 2 0 0 1 2 2v2",
)]);

/// Tabler « refresh ».
pub const REFRESH: Icon = tabler(&[
    Stroke::SvgPath("M20 11a8.1 8.1 0 0 0 -15.5 -2m-.5 -4v4h4"),
    Stroke::SvgPath("M4 13a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4"),
]);

/// Tabler « eraser ».
pub const ERASER: Icon = tabler(&[
    Stroke::SvgPath(
        "M19 20h-10.5l-4.21 -4.3a1 1 0 0 1 0 -1.41l10 -10a1 1 0 0 1 1.41 0l5 5a1 1 0 0 1 0 1.41l-9.2 9.3",
    ),
    Stroke::SvgPath("M18 13.3l-6.3 -6.3"),
]);

/// Tabler « external-link ».
pub const EXTERNAL: Icon = tabler(&[
    Stroke::SvgPath("M12 6h-6a2 2 0 0 0 -2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-6"),
    Stroke::SvgPath("M11 13l9 -9"),
    Stroke::SvgPath("M15 4h5v5"),
]);

/// Tabler « logout ».
pub const LOGOUT: Icon = tabler(&[
    Stroke::SvgPath(
        "M14 8v-2a2 2 0 0 0 -2 -2h-7a2 2 0 0 0 -2 2v12a2 2 0 0 0 2 2h7a2 2 0 0 0 2 -2v-2",
    ),
    Stroke::SvgPath("M9 12h12l-3 -3"),
    Stroke::SvgPath("M18 15l3 -3"),
]);

/// Tabler « login ».
pub const LOGIN: Icon = tabler(&[
    Stroke::SvgPath(
        "M15 8v-2a2 2 0 0 0 -2 -2h-7a2 2 0 0 0 -2 2v12a2 2 0 0 0 2 2h7a2 2 0 0 0 2 -2v-2",
    ),
    Stroke::SvgPath("M21 12h-13l3 -3"),
    Stroke::SvgPath("M11 15l-3 -3"),
]);

/// Tabler « keyboard ».
pub const SHORTCUTS: Icon = tabler(&[
    Stroke::SvgPath(
        "M2 8a2 2 0 0 1 2 -2h16a2 2 0 0 1 2 2v8a2 2 0 0 1 -2 2h-16a2 2 0 0 1 -2 -2l0 -8",
    ),
    Stroke::SvgPath("M6 10l0 .01"),
    Stroke::SvgPath("M10 10l0 .01"),
    Stroke::SvgPath("M14 10l0 .01"),
    Stroke::SvgPath("M18 10l0 .01"),
    Stroke::SvgPath("M6 14l0 .01"),
    Stroke::SvgPath("M18 14l0 .01"),
    Stroke::SvgPath("M10 14l4 .01"),
]);

/// Tabler « sun-moon ».
pub const THEME: Icon = tabler(&[
    Stroke::SvgPath("M9.173 14.83a4 4 0 1 1 5.657 -5.657"),
    Stroke::SvgPath("M11.294 12.707l.174 .247a7.5 7.5 0 0 0 8.845 2.492a9 9 0 0 1 -14.671 2.914"),
    Stroke::SvgPath("M3 12h1"),
    Stroke::SvgPath("M12 3v1"),
    Stroke::SvgPath("M5.6 5.6l.7 .7"),
    Stroke::SvgPath("M3 21l18 -18"),
]);

/// Tabler « server ».
pub const SERVER: Icon = tabler(&[
    Stroke::SvgPath("M3 7a3 3 0 0 1 3 -3h12a3 3 0 0 1 3 3v2a3 3 0 0 1 -3 3h-12a3 3 0 0 1 -3 -3v-2"),
    Stroke::SvgPath(
        "M3 15a3 3 0 0 1 3 -3h12a3 3 0 0 1 3 3v2a3 3 0 0 1 -3 3h-12a3 3 0 0 1 -3 -3l0 -2",
    ),
    Stroke::SvgPath("M7 8l0 .01"),
    Stroke::SvgPath("M7 16l0 .01"),
]);

/// Tabler « adjustments-horizontal ».
pub const ADJUSTMENTS: Icon = tabler(&[
    Stroke::SvgPath("M12 6a2 2 0 1 0 4 0a2 2 0 1 0 -4 0"),
    Stroke::SvgPath("M4 6l8 0"),
    Stroke::SvgPath("M16 6l4 0"),
    Stroke::SvgPath("M6 12a2 2 0 1 0 4 0a2 2 0 1 0 -4 0"),
    Stroke::SvgPath("M4 12l2 0"),
    Stroke::SvgPath("M10 12l10 0"),
    Stroke::SvgPath("M15 18a2 2 0 1 0 4 0a2 2 0 1 0 -4 0"),
    Stroke::SvgPath("M4 18l11 0"),
    Stroke::SvgPath("M19 18l1 0"),
]);

/// Tabler « notes ».
pub const JOURNAL: Icon = tabler(&[
    Stroke::SvgPath(
        "M5 5a2 2 0 0 1 2 -2h10a2 2 0 0 1 2 2v14a2 2 0 0 1 -2 2h-10a2 2 0 0 1 -2 -2l0 -14",
    ),
    Stroke::SvgPath("M9 7l6 0"),
    Stroke::SvgPath("M9 11l6 0"),
    Stroke::SvgPath("M9 15l4 0"),
]);

/// Tabler « license ».
pub const LICENSE: Icon = tabler(&[
    Stroke::SvgPath(
        "M15 21h-9a3 3 0 0 1 -3 -3v-1h10v2a2 2 0 0 0 4 0v-14a2 2 0 1 1 2 2h-2m2 -4h-11a3 3 0 0 0 -3 3v11",
    ),
    Stroke::SvgPath("M9 7l4 0"),
    Stroke::SvgPath("M9 11l4 0"),
]);

/// Tabler « code ».
pub const CODE: Icon = tabler(&[
    Stroke::SvgPath("M7 8l-4 4l4 4"),
    Stroke::SvgPath("M17 8l4 4l-4 4"),
    Stroke::SvgPath("M14 4l-4 16"),
]);

/// Tabler « heart ».
pub const HEART: Icon = tabler(&[Stroke::SvgPath(
    "M19.5 12.572l-7.5 7.428l-7.5 -7.428a5 5 0 1 1 7.5 -6.566a5 5 0 1 1 7.5 6.572",
)]);

/// Tabler « plug-connected-x ».
pub const DISCONNECT: Icon = tabler(&[
    Stroke::SvgPath("M20 16l-4 4"),
    Stroke::SvgPath("M7 12l5 5l-1.5 1.5a3.536 3.536 0 1 1 -5 -5l1.5 -1.5"),
    Stroke::SvgPath("M17 12l-5 -5l1.5 -1.5a3.536 3.536 0 1 1 5 5l-1.5 1.5"),
    Stroke::SvgPath("M3 21l2.5 -2.5"),
    Stroke::SvgPath("M18.5 5.5l2.5 -2.5"),
    Stroke::SvgPath("M10 11l-2 2"),
    Stroke::SvgPath("M13 14l-2 2"),
    Stroke::SvgPath("M16 16l4 4"),
]);

#[cfg(test)]
mod tests {
    /// Every path above, read the way the canvas reads it.
    ///
    /// Found in this file's own text rather than in a list, which the
    /// next icon would be missing from.
    #[test]
    fn every_path_is_read() {
        let (icons, _) = include_str!("icons.rs").split_once("#[cfg(test)]").unwrap();
        let mut read = 0;
        for after in icons.split("SvgPath(").skip(1) {
            let Some(said) = after.trim_start().strip_prefix('"') else {
                continue;
            };
            let said = &said[..said.find('"').unwrap()];
            assert!(crate::path::read(said).is_some(), "not read: « {said} »");
            read += 1;
        }
        assert_ne!(read, 0);
    }
}
