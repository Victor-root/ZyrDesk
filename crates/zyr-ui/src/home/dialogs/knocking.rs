//! A contact asking to come into this computer: who, what they will be
//! able to do once in, and the two answers. Ctrl+F1 answers yes from
//! anywhere, the window in front or not.

use zyr_draw::Rect;
use zyr_draw::icons;
use zyr_i18n::say;
use zyr_proto::rights::Rights;

use super::{Action, Shape, callout};
use crate::home::state::{Dialog, Target};
use crate::home::ui::{Kind, Painter, size};
use crate::shell::app::App;

pub fn shape(id: u64, who: &str) -> Shape {
    Shape {
        title: say!("knocking.title", who = who),
        lead: None,
        width: 480.0,
        left: Vec::new(),
        right: vec![
            Action::new(
                say!("contacts.decline"),
                Kind::Quiet,
                Target::Accord(id, false),
            ),
            Action::new(say!("contacts.accept"), Kind::Primary, Target::Confirm)
                .with_icon(&icons::CHECK),
        ],
    }
}

pub fn draw(painter: &mut Painter, at: Rect, rights: Rights) -> f32 {
    let (left, width) = (at.left, size(at).0);
    let colours = painter.colours;
    let mut y = at.top;
    let able = match (rights.keyboard, rights.mouse) {
        (false, false) => say!("knocking.watch"),
        (true, false) => say!("knocking.keyboard"),
        (false, true) => say!("knocking.mouse"),
        (true, true) => say!("knocking.both"),
    };
    y += painter.paragraph(&able, painter.body(), colours.text, (left, y, width));
    y += callout(
        painter,
        (left, y + painter.px(zyr_draw::design::SPACE_4), width),
        colours.accent,
        |painter, (left, top, width)| {
            painter.paragraph(
                &say!("knocking.how"),
                painter.caption(),
                painter.colours.text,
                (left, top, width),
            )
        },
    ) + painter.px(zyr_draw::design::SPACE_4);
    y - at.top
}

/// Lets in the contact the open dialog is about.
pub fn confirm(app: &App) {
    if let Some(Dialog::Knock { id, .. }) = crate::home::state().dialog.clone() {
        answer(app, id, true);
    }
}

/// Answers a knock, and puts its question away.
pub fn answer(app: &App, id: u64, yes: bool) {
    let showing = matches!(
        crate::home::state().dialog,
        Some(Dialog::Knock { id: open, .. }) if open == id
    );
    if showing {
        super::close(app);
    }
    let app = app.clone();
    crate::shell::app::spawn(async move {
        if let Err(reason) = crate::shell::desk::accord(id, yes).await {
            crate::home::state().tell_trouble(&reason);
            crate::home::redraw(&app);
        }
    });
}

/// Shows that question over whatever the window shows, the window
/// brought in front.
pub fn knocked(app: &App, id: u64, who: String, rights: Rights) {
    super::open(app, Dialog::Knock { id, who, rights });
    crate::shell::main_window::show();
}

/// Puts the question away once nobody waits on it any more: answered
/// from elsewhere, gone, or turned away for want of an answer.
pub fn no_longer_knocking(app: &App, id: u64) {
    let showing = matches!(
        crate::home::state().dialog,
        Some(Dialog::Knock { id: open, .. }) if open == id
    );
    if showing {
        super::close(app);
    }
}
