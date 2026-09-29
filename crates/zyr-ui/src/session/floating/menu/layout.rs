//! Where everything on the card falls, in real pixels.
//!
//! The measures of a line in page pixels, then the geometry the drawing
//! and the mouse share: a card whose lines are drawn in one place and
//! clicked in another is a card that serves the wrong menu.

use super::*;

/// The height a line never goes below.
pub(super) const LINE: f32 = 38.0;
/// An icon's side, and the gap between it and the word.
pub(super) const ICON: f32 = 18.0;
/// What separates the word from what is written to its
/// right.
pub(super) const AFTER_THE_LABEL: f32 = 24.0;
/// The thickness of a separating stroke, and that of a border.
pub(super) const HAIRLINE: f32 = 1.0;
/// The width each reading keeps whatever its number, so that the bar
/// does not breathe in time with the figures.
pub(super) const READING: f32 = 78.0;
/// What separates two readings, and what separates their word
/// from their number.
pub(super) const BETWEEN_READINGS: f32 = 16.0;
pub(super) const UNDER_THE_LABEL: f32 = 2.0;
/// The height of a switch: its caption, what surrounds it above
/// and below, and its border. Stated, since nothing here lays a line
/// of text out by itself.
pub(super) const TOGGLE: f32 = 24.0;
/// The room a slider takes, its thumb included.
pub(super) const SLIDER: f32 = 18.0;
/// The thickness of a slider's bar, and the side of its thumb.
pub(super) const BAR: f32 = 4.0;
pub(super) const THUMB: f32 = 14.0;
/// The side of a chevron and of a tick: smaller than a line's icon,
/// because they are marks and not drawings.
pub(super) const BRAND: f32 = 16.0;

/// What the card takes up, in real pixels.
///
/// As wide as its longest line, which no number written by hand could
/// hold to: a label made longer would cut off its shortcut.
pub(super) fn size(canvas: &Canvas) -> (i32, i32) {
    let scale = scale();
    let overflow_px = shadow_overflow(scale);
    let panel = panels_width(canvas, scale);
    let width = card_width(scale)
        + if panel > 0.0 {
            panel + design::SPACE_2 * scale
        } else {
            0.0
        };
    let height = content(scale).max(panels_height(scale));
    (
        (width + overflow_px * 2.0).ceil() as i32,
        (height + overflow_px * 2.0).ceil() as i32,
    )
}

/// How wide the card is: its longest line.
///
/// What no number written by hand could hold to: a label made longer
/// would cut off its shortcut.
/// Measured over **all** of its lines, including the ones not showing
/// right now: a card that shrinks when a line goes away is a card that
/// changes width under the hand.
fn card_width(scale: f32) -> f32 {
    load(&CARD_WIDTH).max(design::SPACE_2 * 2.0 * scale)
}

/// The same, measured. Stored afterwards, because the layout asks
/// for it again at every frame and measuring text costs.
pub(super) fn measure_the_card(canvas: &Canvas, scale: f32) {
    let session_menu = SESSION_MENU.lock().expect("menu's settings");
    let mut width: f32 = 0.0;
    for line in &LINES {
        width = width.max(match line {
            Line::Measures => {
                (READING * 4.0 + BETWEEN_READINGS * 3.0 + design::SPACE_2 * 2.0) * scale
            }
            // Wrapped to the width the other lines decide: a refusal is
            // a sentence, and a card as wide as a sentence would be a
            // card twice too wide for everything else.
            Line::Separator | Line::Refusal => 0.0,
            Line::Entry(entry) => {
                let right =
                    canvas.width_of(&entry.trailing.text(), Pen::of(design::CAPTION * scale));
                around(canvas, &zyr_i18n::text(entry.label), right, scale)
            }
            Line::Toggle(toggle) => around(
                canvas,
                &zyr_i18n::text(toggle.label),
                sides_width(canvas, &toggle.words(), scale),
                scale,
            ),
            // Its words are asked for with the settings already in hand:
            // asking the line for them again would take again the lock
            // being held, which stops the drawing thread for good.
            Line::Choice(choice) => match session_menu.as_ref() {
                Some(menu) => around(
                    canvas,
                    &zyr_i18n::text(choice.label),
                    sides_width(canvas, &words_of(menu, choice.setting), scale),
                    scale,
                ),
                None => 0.0,
            },
            // Its bar takes the whole width, so it asks for none: it is
            // its head that decides, as for the others.
            Line::Slider(slider) => {
                let value = session_menu
                    .as_ref()
                    .map_or_else(String::new, |menu| slider.setting.summary(menu));
                let right = canvas.width_of(&value, Pen::of(design::BODY * scale));
                around(canvas, &zyr_i18n::text(slider.label), right, scale)
            }
            Line::List(list) => {
                let value = session_menu
                    .as_ref()
                    .map_or_else(String::new, |menu| list.setting.summary(menu));
                let right = canvas.width_of(&value, Pen::of(design::CAPTION * scale))
                    + (design::SPACE_2 + BRAND) * scale;
                around(canvas, &zyr_i18n::text(list.label), right, scale)
            }
        });
    }
    drop(session_menu);
    store(&CARD_WIDTH, width);
}

/// How wide a line is: its icon, its word, what comes on the right,
/// and everything around them.
///
/// The same measurement for every kind of line, because it is the same
/// layout: what changes is what is on the right.
pub(super) fn around(canvas: &Canvas, label: &str, right: f32, scale: f32) -> f32 {
    canvas.width_of(label, Pen::of(design::BODY * scale))
        + right
        + (design::SPACE_2 * 2.0 + ICON + design::SPACE_3 + AFTER_THE_LABEL) * scale
}

/// How wide the sides of a choice line are, together.
fn sides_width(canvas: &Canvas, words: &[String], scale: f32) -> f32 {
    words
        .iter()
        .map(|label| side_width(canvas, label, scale))
        .sum()
}

/// And what one side takes: its word and what surrounds it.
fn side_width(canvas: &Canvas, label: &str, scale: f32) -> f32 {
    canvas.width_of(label, Pen::of(design::CAPTION * scale)) + design::SPACE_3 * 2.0 * scale
}

/// Where the sides of a choice line fall, pushed to the right edge
/// and stuck to one another.
///
/// They form a single object, with one border around them all and
/// nothing between them.
pub(super) fn sides_of(canvas: &Canvas, at: Rect, words: &[String], scale: f32) -> Vec<Rect> {
    let widths: Vec<f32> = words
        .iter()
        .map(|label| side_width(canvas, label, scale))
        .collect();
    let height = TOGGLE * scale;
    let top = at.top + (at.bottom - at.top - height) / 2.0;
    let mut left = at.right - design::SPACE_2 * scale - widths.iter().sum::<f32>();
    widths
        .iter()
        .map(|width| {
            let place = Rect::at(left, top, *width, height);
            left += width;
            place
        })
        .collect()
}

/// The bar of a slider, under the head of its line.
pub(super) fn slider_bar(at: Rect, scale: f32) -> Rect {
    let edge = design::SPACE_2 * scale;
    let top =
        at.top + edge + load(&BODY_HEIGHT) + UNDER_THE_LABEL * scale + (SLIDER - BAR) * scale / 2.0;
    Rect::at(
        at.left + edge,
        top,
        at.right - at.left - edge * 2.0,
        BAR * scale,
    )
}

/// The settings that open a list, in the card's order.
///
/// Read from the lines rather than written a second time: adding a
/// list to the menu is then enough to give it its panel.
fn with_a_panel() -> impl Iterator<Item = Setting> {
    LINES.iter().filter_map(|line| match line {
        Line::List(list) => Some(list.setting),
        _ => None,
    })
}

/// How wide the widest of the panels is, or nothing when none has what
/// it takes to open.
///
/// The widest and not the one that is open: the window cannot change
/// width at the moment a list is opened without the drawing it carries
/// moving at the same instant.
fn panels_width(canvas: &Canvas, scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("menu's settings");
    let Some(menu) = session_menu.as_ref() else {
        return 0.0;
    };
    with_a_panel()
        .map(|setting| panel_width(canvas, menu, setting, scale))
        .fold(0.0, f32::max)
}

/// How wide a panel is: its longest value.
fn panel_width(canvas: &Canvas, menu: &SessionMenu, setting: Setting, scale: f32) -> f32 {
    setting
        .values(menu)
        .iter()
        .map(|value| {
            let aside = canvas.width_of(
                &setting.aside(menu, value),
                Pen::of(design::CAPTION * scale),
            );
            around(canvas, &setting.label(menu, value), aside, scale)
        })
        .fold(0.0, f32::max)
}

/// The height of the tallest panel, for the same reason.
fn panels_height(scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("menu's settings");
    let Some(menu) = session_menu.as_ref() else {
        return 0.0;
    };
    with_a_panel()
        .map(|setting| panel_height(menu, setting, scale))
        .fold(0.0, f32::max)
}

/// How tall a panel is: its values, and nothing else.
///
/// No title: one knows where one is, the line that opened it is facing
/// it and its chevron says so. One more line to repeat the word next to
/// it would be one line less for the values.
fn panel_height(menu: &SessionMenu, setting: Setting, scale: f32) -> f32 {
    let how_many = setting.values(menu).len();
    if how_many == 0 {
        return 0.0;
    }
    (design::SPACE_2 * 2.0 + LINE * how_many as f32) * scale
}

/// The open panel in its window, on the side of the card it did not
/// set out from: on its left as a rule, on its right when the card
/// itself is stuck to the window's left edge.
pub(super) fn panel(canvas: &Canvas, setting: Setting, scale: f32) -> Option<Rect> {
    // The card and the line first, the settings lock afterwards:
    // measuring them takes that same lock, and a lock taken again while
    // it is held stops the drawing thread for good.
    let card = card(scale);
    let line = panel_line(setting, scale)?;
    let session_menu = SESSION_MENU.lock().expect("menu's settings");
    let menu = session_menu.as_ref()?;
    let height = panel_height(menu, setting, scale);
    if height <= 0.0 {
        return None;
    }
    let width = panel_width(canvas, menu, setting, scale);
    // Opened facing the line that opens it, its first value level with
    // it: a panel of two values stuck at the top of the card while a
    // line at the bottom is being clicked is a panel one has to search
    // for with one's eyes. It comes down by as much as it takes to fit
    // in the window, which is built tall enough for the largest of them.
    let edge = design::SPACE_2 * scale;
    let inside = shadow_overflow(scale);
    let bottom = (HEIGHT.load(Ordering::Relaxed) as f32 - inside - height).max(inside);
    let left = if RIGHTWARD.load(Ordering::Relaxed) {
        card.right + edge
    } else {
        card.left - edge - width
    };
    Some(Rect::at(
        left,
        (line.top - edge).clamp(inside, bottom),
        width,
        height,
    ))
}

/// Where the line that opens this panel falls, when it shows.
fn panel_line(setting: Setting, scale: f32) -> Option<Rect> {
    walk(scale)
        .into_iter()
        .find(|(_, line, _)| matches!(line, Line::List(list) if list.setting == setting))
        .map(|(_, _, place)| place)
}

/// The place of each of the open panel's values.
pub(super) fn panel_walk(canvas: &Canvas, setting: Setting, scale: f32) -> Vec<Rect> {
    let Some(panel) = panel(canvas, setting, scale) else {
        return Vec::new();
    };
    let edge = design::SPACE_2 * scale;
    let how_many = SESSION_MENU
        .lock()
        .expect("menu's settings")
        .as_ref()
        .map_or(0, |menu| setting.values(menu).len());
    let mut top = panel.top + edge;
    (0..how_many)
        .map(|_| {
            let place = Rect::at(
                panel.left + edge,
                top,
                panel.right - panel.left - edge * 2.0,
                LINE * scale,
            );
            top = place.bottom;
            place
        })
        .collect()
}

/// How far the shadow spills out of the card, on each side.
pub(super) fn shadow_overflow(scale: f32) -> f32 {
    let shadow = palette().shadow_2;
    (shadow.soft + shadow.down.abs().max(shadow.across.abs())) * scale
}

/// The height of the readings bar.
pub(super) fn readings_height(scale: f32) -> f32 {
    design::SPACE_2 * scale
        + load(&CAPTION_HEIGHT)
        + UNDER_THE_LABEL * scale
        + load(&BODY_HEIGHT)
        + design::SPACE_1 * scale
        + load(&CAPTION_HEIGHT)
        + design::SPACE_1 * scale
}

/// The height of a slider line: its head, then the bar below.
pub(super) fn slider_height(scale: f32) -> f32 {
    (design::SPACE_2 + UNDER_THE_LABEL + SLIDER + design::SPACE_3) * scale + load(&BODY_HEIGHT)
}

/// The card in its window.
///
/// As tall as what it shows, and no taller. Lines come and go with the
/// session, and the window is built once for the largest card possible: so
/// this one is stuck to the edge the menu opens from, which is the only
/// one nobody must see move.
pub(super) fn card(scale: f32) -> Rect {
    let (width, height) = (
        WIDTH.load(Ordering::Relaxed) as f32,
        HEIGHT.load(Ordering::Relaxed) as f32,
    );
    let overflow_px = shadow_overflow(scale);
    let inside = height - overflow_px * 2.0;
    let show = content(scale).min(inside);
    let top = if UPWARD.load(Ordering::Relaxed) {
        overflow_px + inside - show
    } else {
        overflow_px
    };
    let left = if RIGHTWARD.load(Ordering::Relaxed) {
        overflow_px
    } else {
        width - overflow_px - card_width(scale)
    };
    Rect::at(left, top, card_width(scale), show)
}

/// The height of what the card is showing right now.
fn content(scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("menu's settings");
    design::SPACE_2 * scale * 2.0
        + LINES
            .iter()
            .filter(|line| line.is_visible(session_menu.as_ref()))
            .map(|line| line.height(scale))
            .sum::<f32>()
}

/// Each visible line and the room it takes, from the top of the card
/// downwards.
///
/// Read by the drawing and by the mouse, written only once: a card whose
/// lines are drawn in one place and clicked in another is a card that
/// serves the wrong menu.
pub(super) fn walk(scale: f32) -> Vec<(usize, &'static Line, Rect)> {
    let card = card(scale);
    let edge = design::SPACE_2 * scale;
    let session_menu = SESSION_MENU.lock().expect("menu's settings");
    let mut top = card.top + edge;
    let mut placed = Vec::with_capacity(LINES.len());
    for (rank, line) in LINES.iter().enumerate() {
        if !line.is_visible(session_menu.as_ref()) {
            continue;
        }
        let height = line.height(scale);
        placed.push((
            rank,
            line,
            Rect::at(
                card.left + edge,
                top,
                card.right - card.left - edge * 2.0,
                height,
            ),
        ));
        top += height;
    }
    placed
}

/// What is under this point of the window, when it is something that
/// gets clicked.
///
/// What comes in pieces, the sides of a switch and the values of a
/// panel, needs to know where they fall, and so something to measure
/// text with: the window's canvas, the very one they were drawn on. A
/// mouse aiming by another measurement than the drawing would miss.
pub(super) fn under(point: (i32, i32)) -> Option<Target> {
    let (x, y) = (point.0 as f32, point.1 as f32);
    let scale = scale();
    let inside =
        |place: &Rect| x >= place.left && x < place.right && y >= place.top && y < place.bottom;

    if let Some(setting) = *PANEL.lock().expect("menu's panel") {
        let in_the_panel = CANVAS.with_borrow(|canvas| {
            panel_walk(canvas.as_ref()?, setting, scale)
                .iter()
                .position(inside)
                .map(Target::Value)
        });
        if in_the_panel.is_some() {
            return in_the_panel;
        }
    }

    let (rank, line, place) = walk(scale)
        .into_iter()
        .find(|(_, _, place)| inside(place))?;
    match line {
        Line::Entry(_) | Line::List(_) => Some(Target::Line(rank)),
        Line::Toggle(toggle) => CANVAS.with_borrow(|canvas| {
            sides_of(canvas.as_ref()?, place, &toggle.words(), scale)
                .iter()
                .position(inside)
                .map(|side| Target::Side(rank, side))
        }),
        Line::Choice(choice) => CANVAS.with_borrow(|canvas| {
            let canvas = canvas.as_ref()?;
            sides_of(canvas, place, &choice.words()?, scale)
                .iter()
                .position(inside)
                .map(|side| Target::Side(rank, side))
        }),
        Line::Slider(_) => inside(&slider_bar(place, scale).grown(
            // The bar is four pixels tall: aiming at four pixels with a
            // mouse is a chore, and nobody asked for a chore. What gets
            // caught is the thumb's height.
            (THUMB - BAR) * scale / 2.0,
        ))
        .then_some(Target::Bar(rank)),
        Line::Measures | Line::Separator | Line::Refusal => None,
    }
}
