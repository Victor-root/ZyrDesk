//! The dialogs: what opens over the page, darkened behind it.
//!
//! Every dialog has the same frame: a head that says what it is about
//! with the cross that closes it, what it holds, which scrolls when the
//! window is too short for it, and its buttons at the bottom, the main
//! one on the right and what destroys apart on the left. Each dialog
//! says what it holds, which buttons it has, and what its main button
//! does; opening and closing are the same for all of them, and written
//! here.

mod adding;
mod details;
mod journal;
mod licence;
mod renaming;
mod signin;

use zyr_draw::Rect;
use zyr_draw::design::{self, Colour};
use zyr_draw::icons::{self, Icon};
use zyr_i18n::say;

use crate::home::frame::VEIL;
use crate::home::state::{Copyable, Dialog, Scroller, Target};
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Button, Kind, Painter, STEADY, middle, size, square};
use crate::shell::app::App;

pub use journal::{After, open as open_the_journal, reread as reread_the_journal};
pub use journal::{copy as copy_the_journal, empty as empty_the_journal, toggle_tag};

/// What a dialog says about itself.
pub struct Shape {
    pub title: String,
    pub lead: Option<String>,
    /// How wide it is, in page pixels, when the window has the room.
    pub width: f32,
    /// Its buttons: on the left, apart, and on the right, the main one
    /// last.
    pub left: Vec<Action>,
    pub right: Vec<Action>,
}

/// A button at the bottom of a dialog.
pub struct Action {
    pub label: String,
    pub kind: Kind,
    pub target: Target,
    pub enabled: bool,
    pub icon: Option<&'static Icon>,
}

impl Action {
    pub fn new(label: String, kind: Kind, target: Target) -> Self {
        Action {
            label,
            kind,
            target,
            enabled: true,
            icon: None,
        }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Action { enabled, ..self }
    }

    pub fn with_icon(self, icon: &'static Icon) -> Self {
        Action {
            icon: Some(icon),
            ..self
        }
    }
}

/// The fields a dialog writes in, opened with it.
pub fn fields_of(dialog: &Dialog) -> &'static [Field] {
    match dialog {
        Dialog::Adding => &[Field::Fingerprint, Field::Address, Field::Name],
        Dialog::SignIn => &[
            Field::Server,
            Field::User,
            Field::Password,
            Field::DeviceName,
            Field::Email,
            Field::Invitation,
        ],
        Dialog::Renaming { .. } => &[Field::NewName],
        Dialog::Journal { .. } => &[Field::Sift],
        Dialog::Details | Dialog::Licence(_) => &[],
    }
}

/// Opens a dialog over the page, its fields empty, the keyboard in the
/// first of them.
///
/// Emptied at every opening: a dialog opened again full of what was
/// written the time before would let the same computer be added twice
/// with a double click.
pub fn open(app: &App, dialog: Dialog) {
    let which = fields_of(&dialog);
    {
        let mut state = crate::home::state();
        state.dialog = Some(dialog.clone());
        state.dialog_scroll.reset();
        state.trouble = None;
        state.armed = None;
        state.listening = None;
        state.menu = None;
        state.drawer = false;
        state.pinning = None;
        state.attaching = false;
        state.sign_up = false;
    }
    fields::open(
        crate::home::window::handle(),
        which,
        crate::home::window::scale(),
    );
    if let (Some(first), false) = (which.first(), matches!(dialog, Dialog::Journal { .. })) {
        fields::focus(*first);
    }
    crate::home::redraw(app);
}

/// Closes the open dialog, and forgets what it was waiting for.
pub fn close(app: &App) {
    {
        let mut state = crate::home::state();
        state.dialog = None;
        state.trouble = None;
        state.armed = None;
        state.pinning = None;
        state.attaching = false;
    }
    fields::close();
    crate::home::redraw(app);
}

/// Does what the main button of the open dialog does, which is also what
/// Enter does.
pub fn confirm(app: &App) {
    let open = crate::home::state().dialog.clone();
    match open {
        Some(Dialog::Adding) => adding::confirm(app),
        Some(Dialog::SignIn) => signin::confirm(app),
        Some(Dialog::Renaming { .. }) => renaming::confirm(app),
        // Enter in the sift reads again at once, without waiting for
        // the pause after the last letter.
        Some(Dialog::Journal { .. }) => journal::reread(app, After::Show),
        Some(Dialog::Details | Dialog::Licence(_)) => close(app),
        None => {}
    }
}

/// The dialog's shape, and what it holds, drawn within `at` from its top,
/// which gives back the height it takes.
fn shape(painter: &Painter, dialog: &Dialog) -> Shape {
    match dialog {
        Dialog::Adding => adding::shape(painter),
        Dialog::Details => details::shape(painter),
        Dialog::SignIn => signin::shape(painter),
        Dialog::Renaming { name, .. } => renaming::shape(painter, name),
        Dialog::Journal { of } => journal::shape(painter, of.as_ref()),
        Dialog::Licence(rank) => licence::shape(*rank),
    }
}

fn inside(painter: &mut Painter, dialog: &Dialog, at: Rect, window_height: f32) -> f32 {
    match dialog {
        Dialog::Adding => adding::draw(painter, at),
        Dialog::Details => details::draw(painter, at),
        Dialog::SignIn => signin::draw(painter, at),
        Dialog::Renaming { .. } => renaming::draw(painter, at),
        Dialog::Journal { .. } => journal::draw(painter, at, window_height),
        Dialog::Licence(rank) => licence::draw(painter, at, *rank, window_height),
    }
}

/// Draws the open dialog over a window of that size.
pub fn draw(painter: &mut Painter, width: f32, height: f32) {
    let Some(dialog) = painter.state.dialog.clone() else {
        return;
    };
    let shown = painter.arriving(&format!("dialog {dialog:?}"), STEADY);
    let whole = Rect::at(0.0, 0.0, width, height);
    painter.fill(whole, 0.0, Colour::BLACK.faded(VEIL * shown));
    painter.absorbs(whole);

    let shape = shape(painter, &dialog);
    let margin = painter.px(design::SPACE_6);
    let edge = painter.px(design::SPACE_6);
    let dialog_width = painter.px(shape.width).min(width - margin * 2.0);
    let inner_width = dialog_width - edge * 2.0;

    let head_height =
        painter.measured(|painter| head(painter, &shape, Rect::at(0.0, 0.0, dialog_width, 0.0)));
    let body_height = painter.measured(|painter| {
        inside(
            painter,
            &dialog,
            Rect::at(0.0, 0.0, inner_width, 0.0),
            height,
        )
    });
    let foot_height = if shape.left.is_empty() && shape.right.is_empty() {
        0.0
    } else {
        painter.px(34.0 + design::SPACE_5) + painter.px(1.0)
    };
    let wanted = head_height + body_height + foot_height + edge;
    let dialog_height = wanted.min(height - margin * 2.0);
    let at = Rect::at(
        (width - dialog_width) / 2.0,
        (height - dialog_height) / 2.0,
        dialog_width,
        dialog_height,
    );
    painter.grown_in(at, shown, |painter| {
        frame(
            painter,
            &dialog,
            &shape,
            at,
            height,
            (head_height, body_height, foot_height),
        );
    });
}

/// The dialog itself, in `at`: its frame, its head, what it holds and its
/// buttons.
fn frame(
    painter: &mut Painter,
    dialog: &Dialog,
    shape: &Shape,
    at: Rect,
    height: f32,
    (head_height, body_height, foot_height): (f32, f32, f32),
) {
    let colours = painter.colours;
    let edge = painter.px(design::SPACE_6);
    let inner_width = size(at).0 - edge * 2.0;
    let radius = painter.px(design::RADIUS_LARGE);
    painter.shadow(at, radius, colours.shadow_2);
    painter.fill(at, radius, colours.surface_1);
    painter.stroke(at, radius, colours.border_strong);

    head(painter, shape, at);
    let room = Rect {
        top: at.top + head_height,
        bottom: at.bottom - foot_height,
        ..at
    };
    let now = painter.scrolled_to(Scroller::Dialog);
    painter.clipped(room, |painter| {
        inside(
            painter,
            dialog,
            Rect::at(at.left + edge, room.top - now, inner_width, 0.0),
            height,
        );
    });
    painter.scrollbar(
        Scroller::Dialog,
        room,
        body_height + painter.px(design::SPACE_2),
        0.0,
    );
    if foot_height > 0.0 {
        footer(
            painter,
            shape,
            Rect {
                top: at.bottom - foot_height,
                ..at
            },
        );
    }
}

/// The head: the title, what the dialog is about, and the cross. Gives
/// back what it takes.
fn head(painter: &mut Painter, shape: &Shape, at: Rect) -> f32 {
    let colours = painter.colours;
    let edge = painter.px(design::SPACE_6);
    let cross = painter.px(32.0);
    let width = size(at).0 - edge * 2.0 - cross - painter.px(design::SPACE_3);
    let title = painter.pen(design::TITLE - 2.0).in_bold();
    let title_height = painter.line(title);
    let top = at.top + edge - painter.px(4.0);
    painter.text(
        &shape.title,
        title.ellipsized(),
        colours.text,
        Rect::at(at.left + edge, top, width, title_height),
    );
    let mut taken = edge - painter.px(4.0) + title_height;
    if let Some(lead) = shape.lead.as_deref() {
        taken += painter.px(design::SPACE_1);
        taken += painter.paragraph(
            lead,
            painter.caption(),
            colours.text_soft,
            (at.left + edge, at.top + taken, width),
        );
    }
    painter.icon_button(
        Rect::at(
            at.right - edge - cross + painter.px(6.0),
            top + (title_height - cross) / 2.0,
            cross,
            cross,
        ),
        &icons::CROSS,
        Target::CloseDialog,
        say!("dialog.close"),
    );
    taken + painter.px(design::SPACE_5)
}

/// The buttons at the bottom, under a rule.
fn footer(painter: &mut Painter, shape: &Shape, at: Rect) {
    let edge = painter.px(design::SPACE_6);
    painter.rule(at.left, at.top, size(at).0);
    let height = painter.px(34.0);
    let (_, centre) = middle(Rect {
        top: at.top + painter.px(1.0),
        bottom: at.bottom - painter.px(design::SPACE_1),
        ..at
    });
    let top = centre - height / 2.0;
    let gap = painter.px(design::SPACE_2);
    let mut x = at.left + edge;
    for action in &shape.left {
        let width = painter.button_width(&action.label, action.icon);
        lay(painter, Rect::at(x, top, width, height), action);
        x += width + gap;
    }
    let mut x = at.right - edge;
    for action in shape.right.iter().rev() {
        let width = painter.button_width(&action.label, action.icon);
        x -= width;
        lay(painter, Rect::at(x, top, width, height), action);
        x -= gap;
    }
}

fn lay(painter: &mut Painter, at: Rect, action: &Action) {
    let mut button = Button::new(&action.label, action.kind).enabled(action.enabled);
    if let Some(icon) = action.icon {
        button = button.with_icon(icon);
    }
    painter.button(at, action.target.clone(), button);
}

/// A framed block inside a dialog: a word on something that matters, in
/// the tint of a role. Gives back the height it takes.
pub fn callout(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    tint: Colour,
    inside: impl Fn(&mut Painter, (f32, f32, f32)) -> f32,
) -> f32 {
    let pad = painter.px(design::SPACE_4);
    let height = painter.measured(|painter| inside(painter, (left + pad, 0.0, width - pad * 2.0)))
        + pad * 2.0;
    let at = Rect::at(left, top, width, height);
    let radius = painter.px(design::RADIUS);
    painter.fill(at, radius, tint.faded(0.08));
    painter.stroke(at, radius, tint.faded(0.3));
    inside(painter, (left + pad, top + pad, width - pad * 2.0));
    height
}

/// The disc an icon sits in, and the space around what a group's frame
/// holds.
pub const DISC: f32 = 36.0;
pub const PADDING: f32 = 16.0;

/// A group's heading inside a dialog: its icon in the accent, and its
/// name. Gives back its height.
pub fn group_heading(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    icon: &Icon,
    title: &str,
) -> f32 {
    let colours = painter.colours;
    let pen = painter.body().in_bold();
    let height = painter.line(pen);
    let mark = painter.px(18.0);
    painter.icon(
        icon,
        Rect::at(left, top + (height - mark) / 2.0, mark, mark),
        colours.accent,
    );
    let words = left + mark + painter.px(design::SPACE_2);
    painter.text(
        title,
        pen,
        colours.text,
        Rect::at(words, top, left + width - words, height),
    );
    height
}

/// A group's frame inside a dialog, around what `inside` draws from its
/// top left corner. Gives back the height it takes.
pub fn framed(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    inside: impl Fn(&mut Painter, (f32, f32, f32)) -> f32,
) -> f32 {
    let colours = painter.colours;
    let height = painter.measured(|painter| inside(painter, (left, top, width)));
    let at = Rect::at(left, top, width, height);
    let radius = painter.px(design::RADIUS);
    painter.fill(at, radius, colours.surface_2);
    painter.stroke(at, radius, colours.border);
    inside(painter, (left, top, width));
    height
}

/// An icon in its disc, in the accent, as the facts of a group show it.
pub fn disc(painter: &Painter, at: Rect, icon: &Icon) {
    let colours = painter.colours;
    painter.fill(at, size(at).0 / 2.0, colours.accent.faded(0.12));
    painter.icon(icon, square(middle(at), painter.px(18.0)), colours.accent);
}

/// This computer's fingerprint, written so that it reads in groups, and
/// the button that copies it. Gives back the height it takes.
pub fn fingerprint(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    fingerprint: &str,
) -> f32 {
    let colours = painter.colours;
    let copied = painter.state.has_copied(Copyable::Fingerprint);
    let label = if copied {
        say!("dialog.copied")
    } else {
        say!("dialog.copy")
    };
    let icon = if copied { &icons::CHECK } else { &icons::COPY };
    let button_width = painter.button_width(&label, Some(icon));
    let button_height = painter.px(34.0);
    let words_width = width - button_width - painter.px(design::SPACE_4);
    let pen = painter.caption().monospaced().spaced(0.02);
    let shown = if fingerprint.is_empty() {
        say!("dialog.fingerprint_unknown")
    } else {
        grouped(fingerprint)
    };
    let words_height = painter.height_of(&shown, pen, words_width);
    let height = words_height.max(button_height);
    painter.text(
        &shown,
        pen,
        colours.text,
        Rect::at(
            left,
            top + (height - words_height) / 2.0,
            words_width,
            words_height,
        ),
    );
    painter.button(
        Rect::at(
            left + width - button_width,
            top + (height - button_height) / 2.0,
            button_width,
            button_height,
        ),
        Target::Copy(Copyable::Fingerprint),
        Button::new(&label, Kind::Quiet)
            .with_icon(icon)
            .enabled(!fingerprint.is_empty()),
    );
    height
}

/// A fingerprint in groups of eight, which is how two people compare one
/// read out loud.
fn grouped(fingerprint: &str) -> String {
    fingerprint
        .as_bytes()
        .chunks(8)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// What went wrong in the dialog, said inside it. Gives back the height
/// it takes, nothing when nothing went wrong.
pub fn trouble(painter: &mut Painter, (left, top, width): (f32, f32, f32)) -> f32 {
    let Some(said) = painter.state.trouble.clone() else {
        return 0.0;
    };
    let tint = painter.colours.error;
    painter.px(design::SPACE_4)
        + callout(
            painter,
            (left, top + painter.px(design::SPACE_4), width),
            tint,
            |painter, (left, top, width)| {
                let icon = painter.px(18.0);
                painter.icon(
                    &icons::TROUBLE,
                    Rect::at(left, top, icon, icon),
                    painter.colours.error,
                );
                painter
                    .paragraph(
                        &said,
                        painter.caption(),
                        painter.colours.text,
                        (
                            left + icon + painter.px(design::SPACE_2),
                            top,
                            width - icon - painter.px(design::SPACE_2),
                        ),
                    )
                    .max(icon)
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_reads_in_groups_of_eight() {
        assert_eq!(grouped("0829cc7ecb9e9ba5"), "0829cc7e cb9e9ba5");
        assert_eq!(grouped("0829"), "0829");
        assert_eq!(grouped(""), "");
    }
}
