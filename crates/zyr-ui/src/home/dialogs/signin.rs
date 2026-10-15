//! Signing in to a ZyrDesk server, or creating the account on the way:
//! the server, the account and this computer's name; then, when nobody
//! vouches for the server, its key to compare before believing it.

use zyr_control::{Attach, Registering};
use zyr_draw::Rect;
use zyr_draw::design;
use zyr_i18n::say;

use super::{Action, Shape, callout, grouped, trouble};
use crate::home::state::{Pick, Target};
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Kind, Painter, SIDES_HEIGHT, size};
use crate::shell::app::App;
use crate::shell::desk::Attached;

/// The fields without which there is nothing to send.
const NEEDED: [Field; 3] = [Field::Server, Field::User, Field::Password];

pub fn shape(painter: &Painter) -> Shape {
    let state = &painter.state;
    let label = if state.waiting {
        say!("signin.signing_in")
    } else if state.pinning.is_some() {
        say!("signin.pin")
    } else if state.sign_up {
        say!("signin.create")
    } else {
        say!("signin.sign_in")
    };
    Shape {
        title: say!("signin.title"),
        lead: Some(say!("signin.lead")),
        width: 520.0,
        left: Vec::new(),
        right: vec![
            Action::new(say!("dialog.cancel"), Kind::Quiet, Target::CloseDialog),
            Action::new(label, Kind::Primary, Target::Confirm).enabled(filled() && !state.waiting),
        ],
    }
}

pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let colours = painter.colours;
    let (left, width) = (at.left, size(at).0);
    let gap = painter.px(design::SPACE_4);
    let sign_up = painter.state.sign_up;
    let mut y = at.top;

    let sides = vec![say!("signin.have_an_account"), say!("signin.new_account")];
    let sides_height = painter.px(SIDES_HEIGHT);
    painter.sides(
        (left + painter.sides_width(&sides), y + sides_height / 2.0),
        &sides,
        Some(usize::from(sign_up)),
        !painter.state.waiting,
        |rank| Target::Side(Pick::SignUp, rank),
    );
    y += sides_height + gap;

    y += painter.field(
        (left, y, width),
        Field::Server,
        &say!("signin.server"),
        Some((&say!("signin.server_hint"), colours.text_soft)),
    );
    y += gap;
    y += painter.field((left, y, width), Field::User, &say!("signin.user"), None);
    y += gap;
    y += painter.field(
        (left, y, width),
        Field::Password,
        &say!("signin.password"),
        None,
    );
    y += gap;
    y += painter.field(
        (left, y, width),
        Field::DeviceName,
        &say!("signin.device_name"),
        Some((&say!("signin.device_name_hint"), colours.text_soft)),
    );
    if sign_up {
        y += gap;
        y += painter.field((left, y, width), Field::Email, &say!("signin.email"), None);
        y += gap;
        y += painter.field(
            (left, y, width),
            Field::Invitation,
            &say!("signin.invitation"),
            Some((&say!("signin.invitation_hint"), colours.text_soft)),
        );
    }

    if let Some(key) = painter.state.pinning.clone() {
        y += gap;
        y += callout(
            painter,
            (left, y, width),
            colours.warning,
            |painter, (left, top, width)| {
                let colours = painter.colours;
                let mut y = top;
                let pen = painter.body().in_bold();
                let height = painter.line(pen);
                painter.text(
                    &say!("signin.pinning_title"),
                    pen,
                    colours.text,
                    Rect::at(left, y, width, height),
                );
                y += height + painter.px(2.0);
                y += painter.paragraph(
                    &say!("signin.pinning_text"),
                    painter.caption(),
                    colours.text_soft,
                    (left, y, width),
                );
                y += painter.px(design::SPACE_3);
                y += painter.paragraph(
                    &grouped(&key),
                    painter.caption().monospaced().spaced(0.02),
                    colours.text,
                    (left, y, width),
                );
                y - top
            },
        );
    }
    y += trouble(painter, (left, y, width));
    y - at.top
}

/// Whether everything needed is written.
fn filled() -> bool {
    NEEDED
        .iter()
        .all(|field| !fields::text(*field).trim().is_empty())
}

/// What is written in a field that may be left empty.
fn written(field: Field) -> Option<String> {
    let text = fields::text(field).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Attaches this computer to the account written in the fields.
///
/// A server nobody vouches for answers with its key the first time, and
/// the dialog shows it; once the person has compared it, the same button
/// sends it back, pinned.
pub fn confirm(app: &App) {
    if !filled() {
        return;
    }
    let (sign_up, pinning) = {
        let state = crate::home::state();
        if state.waiting {
            return;
        }
        (state.sign_up, state.pinning.clone())
    };
    let request = Attach {
        server: fields::text(Field::Server).trim().to_string(),
        username: fields::text(Field::User).trim().to_string(),
        password: fields::text(Field::Password),
        register: sign_up.then(|| Registering {
            email: written(Field::Email),
            invitation: written(Field::Invitation),
        }),
        name: fields::text(Field::DeviceName).trim().to_string(),
        pin: pinning.and_then(|key| key.parse().ok()),
    };
    {
        let mut state = crate::home::state();
        state.waiting = true;
        state.trouble = None;
    }
    crate::home::redraw(app);

    let app = app.clone();
    crate::shell::app::spawn(async move {
        let outcome = crate::shell::desk::attach(request).await;
        let attached = {
            let mut state = crate::home::state();
            state.waiting = false;
            match outcome {
                Ok(Attached::Done) => true,
                Ok(Attached::Unpinned(presented)) => {
                    state.pinning = Some(presented);
                    false
                }
                Err(reason) => {
                    state.trouble = Some(zyr_i18n::fact(&reason));
                    false
                }
            }
        };
        if !attached {
            crate::home::redraw(&app);
            return;
        }
        // Closed on the thread that owns its fields: they are windows of
        // the system, and a window is destroyed by its own thread or not
        // at all.
        let held = app.clone();
        let _ = app.run_on_main_thread(move || super::close(&held));
        crate::home::seen::again(&app).await;
        crate::home::state().tell_news(say!("signin.attached"));
        crate::home::redraw(&app);
    });
}
