//! The journal: what this computer, or a far one, has written, read here
//! and copied whole when something goes wrong.
//!
//! The lines scroll in their own box, the most recent at the bottom, and
//! a line does not wrap: it is read by moving along it. Under them, the
//! names the page says can be asked for, ticked rather than typed, and
//! the sift they fill, which narrows the page by itself.

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_draw::icons;
use zyr_i18n::say;

use super::{Action, Shape};
use crate::home::state::{Copyable, Dialog, FarJournal, Scroller, Target};
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Button, Kind, Painter, size};
use crate::shell::app::App;

/// How much of the window the lines take, and the most and the least
/// they ever take: the dialog has to fit, and a box of three lines is no
/// journal.
const LINES_PART: f32 = 0.5;
const LINES_AT_MOST: f32 = 560.0;
const LINES_AT_LEAST: f32 = 180.0;

/// Far enough that a line is never cut by its own rectangle: the box is
/// what holds it back, and the box scrolls.
const FAR_AWAY: f32 = 20_000.0;

/// Lower than any page, brought back to the real bottom by the drawing:
/// asking for the very bottom before the page is measured is the only
/// way to be there from its first frame.
const VERY_BOTTOM: f32 = f32::MAX;

pub fn shape(painter: &Painter, of: Option<&FarJournal>) -> Shape {
    let title = match of {
        Some(far) => say!("journal.of", name = far.name),
        None => say!("journal.title"),
    };
    let lead = if of.is_some() {
        say!("journal.far_lead")
    } else {
        say!("journal.lead")
    };
    let armed = painter.state.is_armed(&Target::JournalEmpty);
    let copied = painter.state.has_copied(Copyable::Journal);
    let copy = if copied {
        say!("dialog.copied")
    } else if fields::text(Field::Sift).trim().is_empty() {
        say!("journal.copy_all")
    } else {
        say!("journal.copy_sifted")
    };
    // Emptying sits at the other end from copying: the two are clicked
    // within the same minute, and a slip costs everything that was about
    // to be copied.
    let left = vec![
        Action::new(
            if armed {
                say!("dialog.confirm")
            } else {
                say!("journal.empty")
            },
            if armed { Kind::Warning } else { Kind::Ghost },
            Target::JournalEmpty,
        )
        .with_icon(&icons::ERASER),
    ];
    let mut right = Vec::new();
    // The folder only on this computer: the far one's is over there.
    if of.is_none() {
        right.push(
            Action::new(say!("journal.open_folder"), Kind::Quiet, Target::OpenLogs)
                .with_icon(&icons::FOLDER),
        );
    }
    right.push(
        Action::new(say!("journal.refresh"), Kind::Quiet, Target::JournalRefresh)
            .with_icon(&icons::REFRESH),
    );
    right.push(
        Action::new(copy, Kind::Primary, Target::Copy(Copyable::Journal)).with_icon(if copied {
            &icons::CHECK
        } else {
            &icons::COPY
        }),
    );
    Shape {
        title,
        lead: Some(lead),
        width: 920.0,
        left,
        right,
    }
}

pub fn draw(painter: &mut Painter, at: Rect, window_height: f32) -> f32 {
    let colours = painter.colours;
    let (left, width) = (at.left, size(at).0);
    let gap = painter.px(design::SPACE_4);

    // The names are measured before the lines are laid: the lines give
    // them their room, since one more row of names growing the dialog
    // would push « Copy » out of the window.
    let names = painter.measured(|painter| tags(painter, (left, 0.0, width)));
    let lines = ((window_height * LINES_PART).min(painter.px(LINES_AT_MOST)) - names)
        .max(painter.px(LINES_AT_LEAST));
    lines_box(painter, Rect::at(left, at.top, width, lines));
    let mut y = at.top + lines + gap;
    y += tags(painter, (left, y, width));
    y += painter.field(
        (left, y, width),
        Field::Sift,
        &say!("journal.sift"),
        Some((&say!("journal.sift_hint"), colours.text_soft)),
    );
    y - at.top
}

/// The lines, in their box, from the one the box is scrolled to.
fn lines_box(painter: &mut Painter, at: Rect) {
    let colours = painter.colours;
    let radius = painter.px(design::RADIUS_SMALL);
    painter.fill(at, radius, colours.surface_2);
    painter.stroke(at, radius, colours.border);
    if painter.is_silent() {
        return;
    }
    let inside = at.grown(-painter.px(design::SPACE_3));
    let pen = painter.caption().monospaced().overflowing();
    let height = painter.line(pen);
    let content = height * painter.state.lines.len() as f32;
    let room = size(inside).1;
    let bottom = (content - room).max(0.0);
    let scrolled = &mut painter.state.lines_scroll;
    scrolled.goal = scrolled.goal.min(bottom);
    let now = painter.scrolled_to(Scroller::Lines);
    let across = painter.state.lines_across;

    let first = (now / height).floor().max(0.0) as usize;
    let count = (room / height).ceil() as usize + 1;
    let plumbing = format!(" {} [", zyr_proto::log::HUNTS);
    let shown: Vec<(usize, String)> = painter
        .state
        .lines
        .iter()
        .enumerate()
        .skip(first)
        .take(count)
        .map(|(rank, line)| (rank, line.clone()))
        .collect();
    painter.clipped(inside, |painter| {
        for (rank, line) in &shown {
            // The plumbing that worked, quieter than what the product
            // says it did: it is there to be found, not to be read first.
            let ink = if line.contains(&plumbing) {
                colours.text_faint
            } else {
                colours.text_soft
            };
            painter.text(
                line,
                pen,
                ink,
                Rect::at(
                    inside.left - across,
                    inside.top + *rank as f32 * height - now,
                    FAR_AWAY,
                    height,
                ),
            );
        }
    });
    painter.scrollbar(Scroller::Lines, inside, content, 0.0);
}

/// The names the page says can be asked for, as buttons that tick them
/// in the sift, from left to right and from one row to the next. Gives
/// back what they take, nothing for a page that names none, as a page
/// from an older half of the product does.
fn tags(painter: &mut Painter, (left, top, width): (f32, f32, f32)) -> f32 {
    let names = painter.state.tags.clone();
    if names.is_empty() {
        return 0.0;
    }
    let ticked: Vec<String> = fields::text(Field::Sift)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let height = painter.px(30.0);
    let between = painter.px(design::SPACE_2);
    let (mut x, mut y) = (left, top);
    for name in names {
        let wide = painter.button_width(&name, None);
        // To the next row only when a name would overflow: a narrow
        // dialog holds two to a row and a wide one all of them, with
        // nothing counted beforehand.
        if x > left && x + wide > left + width {
            x = left;
            y += height + between;
        }
        let kind = if ticked.contains(&name) {
            Kind::Primary
        } else {
            Kind::Quiet
        };
        painter.button(
            Rect::at(x, y, wide, height),
            Target::JournalTag(name.clone()),
            Button::new(&name, kind),
        );
        x += wide + between;
    }
    y + height + painter.px(design::SPACE_4) - top
}

/// What is done with the page once it is read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum After {
    Show,
    /// Showing it and copying it: « Copy the sift » clicked on a page
    /// that did not answer the sift yet.
    Take,
}

/// Opens the journal of this computer, or of a far one.
///
/// What was written in the sift is kept: the same sift is asked of this
/// journal and then of the far one, and typing it twice is half the
/// work.
pub fn open(app: &App, of: Option<FarJournal>) {
    let sift = fields::text(Field::Sift);
    super::open(app, Dialog::Journal { of });
    {
        let mut state = crate::home::state();
        state.lines = vec![say!("journal.reading")];
        // The names of one page are not those of another, least of all
        // from this journal to a far one.
        state.tags = Vec::new();
        state.sift = None;
        state.lines_scroll.reset();
        state.lines_across = 0.0;
    }
    fields::write(Field::Sift, &sift);
    reread(app, After::Show);
}

/// Asks again for the page of the open journal, sifted as the sift says.
///
/// On the thread that draws, the only one that reads the fields.
pub fn reread(app: &App, after: After) {
    // A reading waiting for the pause after a letter has no reason left
    // to be: this one replaces it.
    crate::home::window::forget_the_sift_pause();
    let sift = fields::text(Field::Sift).trim().to_string();
    let of = {
        let mut state = crate::home::state();
        let Some(Dialog::Journal { of }) = state.dialog.clone() else {
            return;
        };
        state.sift_asked = sift.clone();
        of
    };
    let app = app.clone();
    crate::shell::app::spawn(async move {
        let text = match &of {
            None => crate::shell::journal::journal(&sift).await,
            // Said in the journal itself: that is where the person who
            // has just clicked is looking, and a computer that does not
            // answer is already half the answer.
            Some(far) => crate::shell::journal::far_journal(
                far.address.clone(),
                far.fingerprint.clone(),
                sift.clone(),
            )
            .await
            .unwrap_or_else(|reason| zyr_i18n::fact(&reason)),
        };
        // Reaching a far computer takes the time it takes: the journal
        // may have been closed, turned to another computer or sifted
        // otherwise meanwhile, and what arrives late does not cover what
        // is on screen.
        let taken = {
            let mut state = crate::home::state();
            let current =
                matches!(&state.dialog, Some(Dialog::Journal { of: open }) if *open == of);
            if !current || state.sift_asked != sift {
                return;
            }
            state.lines = text.lines().map(str::to_string).collect();
            state.tags = zyr_proto::journal::names_in(&text);
            state.sift = Some(sift);
            // The most recent is at the bottom: it is what has just
            // happened, and what the journal is opened to read.
            state.lines_scroll.goal = VERY_BOTTOM;
            (after == After::Take).then(|| state.lines.join("\n"))
        };
        crate::home::redraw(&app);
        if let Some(whole) = taken {
            let held = app.clone();
            let _ = app.run_on_main_thread(move || {
                crate::home::act::copy(&held, &whole, Copyable::Journal)
            });
        }
    });
}

/// Ticks or unticks that name in the sift.
///
/// Added to what is written, or taken out of it, rather than written in
/// its place: ticking two names keeps both subjects, and what was typed
/// by hand beside them stays.
pub fn toggle_tag(app: &App, name: &str) {
    let mut words: Vec<String> = fields::text(Field::Sift)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    match words.iter().position(|word| word == name) {
        Some(at) => {
            words.remove(at);
        }
        None => words.push(name.to_string()),
    }
    fields::write(Field::Sift, &words.join(" "));
    // Read again at once: a click has said what it wanted, there is no
    // letter left to wait for.
    reread(app, After::Show);
}

/// Copies the page, which has to answer what the sift says.
///
/// The button says « Copy the sift » and must never take anything else:
/// between a sift pasted in and the page narrowing there is the pause and
/// the round trip to the service, just long enough to click. A page that
/// is behind is read again, and its answer is what goes.
pub fn copy(app: &App) {
    let sift = fields::text(Field::Sift).trim().to_string();
    let page = {
        let state = crate::home::state();
        (state.sift.as_deref() == Some(sift.as_str())).then(|| state.lines.join("\n"))
    };
    match page {
        Some(whole) => crate::home::act::copy(app, &whole, Copyable::Journal),
        None => reread(app, After::Take),
    }
}

/// Empties the journal where it is written, after a second click:
/// emptying wipes out the only trace of what has just happened.
pub fn empty(app: &App) {
    if !crate::home::act::second_click(app, Target::JournalEmpty) {
        return;
    }
    let of = {
        let mut state = crate::home::state();
        let Some(Dialog::Journal { of }) = state.dialog.clone() else {
            return;
        };
        state.lines = vec![say!("journal.emptying")];
        of
    };
    crate::home::redraw(app);

    let app = app.clone();
    crate::shell::app::spawn(async move {
        let done = match &of {
            None => crate::shell::journal::clear_journal(),
            Some(far) => {
                crate::shell::journal::clear_far_journal(
                    far.address.clone(),
                    far.fingerprint.clone(),
                )
                .await
            }
        };
        if let Err(reason) = done {
            crate::home::state().lines = zyr_i18n::fact(&reason)
                .lines()
                .map(str::to_string)
                .collect();
            crate::home::redraw(&app);
            return;
        }
        let held = app.clone();
        let _ = app.run_on_main_thread(move || reread(&held, After::Show));
    });
}
