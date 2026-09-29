//! The ZyrDesk mark: two screens, the far one behind and the near one in
//! front of it.
//!
//! Drawn from here and nowhere else: it is the same drawing on the
//! floating button, in the notification area, in the header of the home
//! screen and on the screen a session opens on, and two drawings for one
//! mark are two marks.

use crate::canvas::{Canvas, Rect};

/// The drawing, in the units of its own file, which is what makes this
/// the same logo as everywhere else in the product rather than a second
/// one that drifts.
///
/// `packaging/brand/zyrdesk.svg` draws two screens with a stroke centred
/// on their path,
/// so each is painted from half a stroke outside its rectangle to half a
/// stroke inside. Its frame runs from 36 to 476, four hundred and forty
/// units wide, and everything below is counted in those.
mod drawing {
    /// The frame's width, and the origin of its top left corner.
    pub const SIDE: f32 = 440.0;
    pub const ORIGIN: f32 = 36.0;

    use crate::design::Colour;

    /// The outline, and the two fills, in the numbers everything that
    /// draws wants.
    const fn tint(red: u8, green: u8, blue: u8) -> Colour {
        Colour {
            red: red as f32 / 255.0,
            green: green as f32 / 255.0,
            blue: blue as f32 / 255.0,
            alpha: 1.0,
        }
    }

    pub const LINE: Colour = tint(9, 13, 22);
    pub const WHITE: Colour = tint(255, 255, 255);
    pub const GOLD: Colour = tint(239, 181, 54);

    /// Half the stroke's width, which is how far it reaches either side
    /// of the path it is drawn on.
    pub const HALF_STROKE: f32 = 14.0;

    /// One rounded rectangle: its middle, its half width and height, the
    /// radius of its corners, and what fills it.
    pub struct Round {
        pub middle: (f32, f32),
        pub half: (f32, f32),
        pub radius: f32,
        pub fill: Colour,
        pub outlined: bool,
    }

    /// The four, in the order the file draws them: the far screen and its
    /// dark pane, then the near one over it and its own.
    pub const SHAPES: [Round; 4] = [
        Round {
            middle: (282.0, 193.0),
            half: (164.0, 123.0),
            radius: 68.0,
            fill: WHITE,
            outlined: true,
        },
        Round {
            middle: (282.0, 193.0),
            half: (100.0, 59.0),
            radius: 24.0,
            fill: LINE,
            outlined: false,
        },
        Round {
            middle: (230.0, 319.0),
            half: (164.0, 123.0),
            radius: 68.0,
            fill: GOLD,
            outlined: true,
        },
        Round {
            middle: (230.0, 319.0),
            half: (100.0, 59.0),
            radius: 24.0,
            fill: LINE,
            outlined: false,
        },
    ];
}

/// Lays the mark in this rect, on this canvas.
///
/// The rect is square, like the drawing's grid: a rect that is not simply
/// leaves empty space at the bottom, the logo not taking up all of its
/// height.
///
/// `part` is how much of it is left: one for the full mark, less for a
/// mark that steps back. It is what the icon near the clock uses to say
/// that this computer cannot be reached, while staying the same mark
/// rather than becoming a second drawing.
///
/// `mirrored` flips it left to right, still the right way up: that is
/// what the floating button asks for when its menu opens to its right
/// rather than its left, so that the logo faces the menu rather than
/// turning its back on it.
pub fn lay(canvas: &Canvas, rect: Rect, part: f32, mirrored: bool) {
    for shape in &drawing::SHAPES {
        let place = placed(rect, shape, mirrored);
        let radius = shape.radius * per_unit(rect);
        canvas.fill(place, radius, shape.fill.faded(part));
        if shape.outlined {
            // On the edge and not inside it: that is what a stroke does
            // in the original drawing, and an outline pulled inside would
            // make the logo thinner by half its stroke.
            canvas.stroke_on(
                place,
                radius,
                drawing::HALF_STROKE * 2.0 * per_unit(rect),
                drawing::LINE.faded(part),
            );
        }
    }
}

/// Fills that much of the near screen's pane, like a loading bar.
///
/// It fills from the side the drawing faces, so from the right when it
/// is mirrored.
pub fn fill_the_near_pane(canvas: &Canvas, rect: Rect, part: f32, mirrored: bool) {
    /// The pane of the near screen: the last of the four drawings, so
    /// the one laid over all the others.
    const PANE: usize = drawing::SHAPES.len() - 1;
    /// What always shows, with a transfer open and nothing arrived yet.
    /// An empty pane reads as a pane, not as a wait.
    const AT_LEAST: f32 = 0.08;

    let pane = placed(rect, &drawing::SHAPES[PANE], mirrored);
    let width = (pane.right - pane.left) * part.clamp(AT_LEAST, 1.0);
    let left = if mirrored {
        pane.right - width
    } else {
        pane.left
    };
    let filled = Rect::at(left, pane.top, width, pane.bottom - pane.top);
    canvas.fill(
        filled,
        drawing::SHAPES[PANE].radius * per_unit(rect),
        drawing::WHITE,
    );
}

/// Where one shape of the drawing falls inside that frame.
///
/// Apart from the drawing above because filling the near pane wants the
/// very same answer for one of those shapes: two ways of working out
/// where the near screen sits would be two screens that drift apart the
/// first time the drawing is touched.
fn placed(rect: Rect, shape: &drawing::Round, mirrored: bool) -> Rect {
    let per_unit = per_unit(rect);
    let left = if mirrored {
        rect.right - (shape.middle.0 + shape.half.0 - drawing::ORIGIN) * per_unit
    } else {
        rect.left + (shape.middle.0 - shape.half.0 - drawing::ORIGIN) * per_unit
    };
    Rect::at(
        left,
        rect.top + (shape.middle.1 - shape.half.1 - drawing::ORIGIN) * per_unit,
        shape.half.0 * 2.0 * per_unit,
        shape.half.1 * 2.0 * per_unit,
    )
}

/// What one unit of the grid the drawing is written on is worth, in this
/// rect.
fn per_unit(rect: Rect) -> f32 {
    (rect.right - rect.left) / drawing::SIDE
}
