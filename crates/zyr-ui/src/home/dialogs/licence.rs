//! The full text of a licence, as the obligation to make it readable asks
//! (`docs/COMPLIANCE.md`).
//!
//! The files carry their text cut at eighty columns, for a terminal: here
//! each paragraph is joined again and wrapped to the dialog, which is how
//! a page is read. The words themselves are not touched.
//!
//! A licence runs to thousands of words, and laying out what nobody sees
//! at every frame would cost more than the rest of the window: each
//! paragraph is measured once for the width it is shown at, and only
//! those on screen are drawn.

use std::cell::RefCell;

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_i18n::say;

use super::{Action, Shape};
use crate::home::licences::text_of;
use crate::home::state::Target;
use crate::home::ui::{Kind, Painter, size};

pub fn shape(rank: usize) -> Shape {
    let name = text_of(rank).map_or("", |(name, _)| name);
    Shape {
        title: say!("licence.title", name = name),
        lead: None,
        width: 760.0,
        left: Vec::new(),
        right: vec![Action::new(
            say!("dialog.close"),
            Kind::Primary,
            Target::CloseDialog,
        )],
    }
}

/// The paragraphs of the licence shown, and their heights at the width
/// they were last measured for.
struct Measured {
    rank: usize,
    width: u32,
    paragraphs: Vec<(String, f32)>,
}

thread_local! {
    static MEASURED: RefCell<Option<Measured>> = const { RefCell::new(None) };
}

pub fn draw(painter: &mut Painter, at: Rect, rank: usize, window_height: f32) -> f32 {
    let Some((_, text)) = text_of(rank) else {
        return 0.0;
    };
    let colours = painter.colours;
    let pen = painter.caption();
    let width = size(at).0;
    let between = painter.px(design::SPACE_3);
    MEASURED.with_borrow_mut(|measured| {
        let key = width.round() as u32;
        if measured
            .as_ref()
            .is_none_or(|known| known.rank != rank || known.width != key)
        {
            *measured = Some(Measured {
                rank,
                width: key,
                paragraphs: paragraphs(text)
                    .into_iter()
                    .map(|paragraph| {
                        let height = painter.height_of(&paragraph, pen, width);
                        (paragraph, height)
                    })
                    .collect(),
            });
        }
        let Some(known) = measured.as_ref() else {
            return 0.0;
        };
        let mut y = at.top;
        for (paragraph, height) in &known.paragraphs {
            if y + height > 0.0 && y < window_height {
                painter.text(
                    paragraph,
                    pen,
                    colours.text_soft,
                    Rect::at(at.left, y, width, *height),
                );
            }
            y += height + between;
        }
        (y - between - at.top).max(0.0)
    })
}

/// A text cut into its paragraphs, each one joined again: a blank line
/// parts two paragraphs, a line break within one is only where the
/// terminal ended.
fn paragraphs(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            if !current.is_empty() {
                found.push(current.join(" "));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        found.push(current.join(" "));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paragraph_cut_for_a_terminal_is_joined_again() {
        let text = "  GNU GENERAL PUBLIC LICENSE\n   Version 3\n\n\nPreamble\n\nThe licenses for most\nsoftware are designed\n";
        assert_eq!(
            paragraphs(text),
            vec![
                "GNU GENERAL PUBLIC LICENSE Version 3",
                "Preamble",
                "The licenses for most software are designed",
            ]
        );
    }
}
