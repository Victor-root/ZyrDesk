//! The "d" of an SVG path, read into the steps that draw it.
//!
//! Apart from the drawing so that it is tried everywhere: reading a path
//! is only arithmetic, and the tests of this brick read every path the
//! product ships. What is understood is what the icons of this product
//! use, and nothing more: move to, line to, horizontal line, vertical
//! line, a curve, an arc, and close. An unknown letter stops the reading
//! rather than being skipped: a half-drawn icon looks like a defect, a
//! missing icon like an oversight, and the second one gets looked into.

/// One step of a path, in the grid the path is written in and counted
/// from its corner, whatever the path said.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    /// Lifts the pen and puts it down here, starting a new figure.
    Start((f32, f32)),
    /// A straight line to here.
    Line((f32, f32)),
    /// A Bézier curve to `to`, pulled by its two handles.
    Curve {
        first: (f32, f32),
        second: (f32, f32),
        to: (f32, f32),
    },
    /// A piece of an ellipse of these radii, turned by `rotation`
    /// degrees, to `to`: the larger or the smaller way round, one way or
    /// the other.
    Arc {
        to: (f32, f32),
        radii: (f32, f32),
        rotation: f32,
        large: bool,
        clockwise: bool,
    },
    /// Back to where the figure started, closing it.
    Close,
}

/// Reads a path, or says it cannot.
pub(crate) fn read(said: &str) -> Option<Vec<Step>> {
    let mut words = Tokens::over(said);
    let mut steps = Vec::new();
    let (mut at, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    let mut letter = ' ';
    while let Some(next) = words.letter_or_number() {
        let repeated = next.is_none();
        if let Some(this_one) = next {
            letter = this_one;
        }
        let relative = letter.is_lowercase();
        let mut number = || words.number();
        let moved = |at: (f32, f32), x: f32, y: f32| {
            if relative {
                (at.0 + x, at.1 + y)
            } else {
                (x, y)
            }
        };
        match letter.to_ascii_uppercase() {
            'M' => {
                let (x, y) = (number()?, number()?);
                at = moved(at, x, y);
                steps.push(Step::Start(at));
                start = at;
                // Numbers after a move are lines, as the language says.
                letter = if relative { 'l' } else { 'L' };
            }
            'L' => {
                let (x, y) = (number()?, number()?);
                at = moved(at, x, y);
                steps.push(Step::Line(at));
            }
            'H' => {
                let x = number()?;
                at.0 = if relative { at.0 + x } else { x };
                steps.push(Step::Line(at));
            }
            'V' => {
                let y = number()?;
                at.1 = if relative { at.1 + y } else { y };
                steps.push(Step::Line(at));
            }
            'C' => {
                // Both handles are counted from the point the curve
                // starts from, so before having left it.
                let (x1, y1) = (number()?, number()?);
                let (x2, y2) = (number()?, number()?);
                let (x, y) = (number()?, number()?);
                let (first, second) = (moved(at, x1, y1), moved(at, x2, y2));
                at = moved(at, x, y);
                steps.push(Step::Curve {
                    first,
                    second,
                    to: at,
                });
            }
            'A' => {
                let radii = (number()?, number()?);
                let rotation = number()?;
                let (large, sweep) = (number()?, number()?);
                let (x, y) = (number()?, number()?);
                at = moved(at, x, y);
                steps.push(Step::Arc {
                    to: at,
                    radii,
                    rotation,
                    large: large != 0.0,
                    clockwise: sweep != 0.0,
                });
            }
            // Closing takes no number: one that follows it has nothing
            // to belong to.
            'Z' if !repeated => {
                steps.push(Step::Close);
                at = start;
            }
            _ => return None,
        }
    }
    Some(steps)
}

/// What an SVG path says, letter by letter and number by number.
///
/// A minus sign opens a number, it does not separate: that is the rule of
/// this language, and it is what allows writing "a9 9 0 1 1-12.8 0" with
/// no space before the twelve.
struct Tokens<'a> {
    rest: &'a str,
}

impl<'a> Tokens<'a> {
    fn over(said: &'a str) -> Self {
        Tokens { rest: said }
    }

    fn skip(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', ',', '\t', '\n']);
    }

    /// The next thing: a letter, or nothing when a number is coming, or
    /// the end.
    fn letter_or_number(&mut self) -> Option<Option<char>> {
        self.skip();
        let first = self.rest.chars().next()?;
        if first.is_ascii_alphabetic() {
            self.rest = &self.rest[first.len_utf8()..];
            return Some(Some(first));
        }
        Some(None)
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let mut end = 0;
        for (at, character) in self.rest.char_indices() {
            let open = at == 0 && (character == '-' || character == '+');
            if character.is_ascii_digit() || character == '.' || open {
                end = at + character.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 {
            return None;
        }
        let (read, rest) = self.rest.split_at(end);
        self.rest = rest;
        read.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minus_sign_opens_a_number() {
        assert_eq!(
            read("M12 3a9 9 0 1 1-12.5 0"),
            Some(vec![
                Step::Start((12.0, 3.0)),
                Step::Arc {
                    to: (-0.5, 3.0),
                    radii: (9.0, 9.0),
                    rotation: 0.0,
                    large: true,
                    clockwise: true,
                },
            ])
        );
    }

    #[test]
    fn lowercase_counts_from_where_the_pen_is() {
        assert_eq!(
            read("M2 3l4 5h1v-2L0 0"),
            Some(vec![
                Step::Start((2.0, 3.0)),
                Step::Line((6.0, 8.0)),
                Step::Line((7.0, 8.0)),
                Step::Line((7.0, 6.0)),
                Step::Line((0.0, 0.0)),
            ])
        );
    }

    #[test]
    fn numbers_after_a_move_are_lines() {
        assert_eq!(
            read("m1 1 2 0 0 2z"),
            Some(vec![
                Step::Start((1.0, 1.0)),
                Step::Line((3.0, 1.0)),
                Step::Line((3.0, 3.0)),
                Step::Close,
            ])
        );
    }

    #[test]
    fn both_handles_of_a_curve_count_from_its_start() {
        assert_eq!(
            read("M10 10c1 0 2 1 3 3"),
            Some(vec![
                Step::Start((10.0, 10.0)),
                Step::Curve {
                    first: (11.0, 10.0),
                    second: (12.0, 11.0),
                    to: (13.0, 13.0),
                },
            ])
        );
    }

    #[test]
    fn closing_brings_the_pen_back_to_the_start() {
        assert_eq!(
            read("M5 5h2zl1 0"),
            Some(vec![
                Step::Start((5.0, 5.0)),
                Step::Line((7.0, 5.0)),
                Step::Close,
                Step::Line((6.0, 5.0)),
            ])
        );
    }

    #[test]
    fn what_is_not_understood_stops_the_reading() {
        assert_eq!(read("M0 0Q1 1 2 2"), None);
        assert_eq!(read("M0 0L1"), None);
        assert_eq!(read("M0 0Z#"), None);
        assert_eq!(read("1 2"), None);
    }
}
