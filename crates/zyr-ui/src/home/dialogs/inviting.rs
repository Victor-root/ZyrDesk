//! Asking someone to become a contact, by the username they have on the
//! same server.

use zyr_draw::Rect;
use zyr_draw::icons;
use zyr_i18n::say;

use super::{Action, Shape, trouble};
use crate::home::state::Target;
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Kind, Painter, size};
use crate::shell::app::App;

pub fn shape(painter: &Painter) -> Shape {
    let waiting = painter.state.waiting;
    let label = if waiting {
        say!("inviting.sending")
    } else {
        say!("inviting.send")
    };
    Shape {
        title: say!("inviting.title"),
        lead: Some(say!("inviting.lead")),
        width: 480.0,
        left: Vec::new(),
        right: vec![
            Action::new(say!("dialog.cancel"), Kind::Quiet, Target::CloseDialog),
            Action::new(label, Kind::Primary, Target::Confirm)
                .with_icon(&icons::USER_PLUS)
                .enabled(!written().is_empty() && !waiting),
        ],
    }
}

pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let (left, width) = (at.left, size(at).0);
    let mut y = at.top;
    y += painter.field((left, y, width), Field::User, &say!("inviting.who"), None);
    y += trouble(painter, (left, y, width));
    y - at.top
}

fn written() -> String {
    fields::text(Field::User).trim().to_string()
}

/// Sends the request. The dialog stays open until the server has taken
/// it: a name nobody has is said inside it, where it can be corrected.
pub fn confirm(app: &App) {
    let username = written();
    if username.is_empty() {
        return;
    }
    {
        let mut state = crate::home::state();
        if state.waiting {
            return;
        }
        state.waiting = true;
        state.trouble = None;
    }
    crate::home::redraw(app);

    let app = app.clone();
    crate::shell::app::spawn(async move {
        let sent = crate::shell::desk::invite(username.clone()).await;
        {
            let mut state = crate::home::state();
            state.waiting = false;
            if let Err(reason) = &sent {
                state.trouble = Some(zyr_i18n::fact(reason));
            }
        }
        if sent.is_err() {
            crate::home::redraw(&app);
            return;
        }
        // Closed on the thread that owns its field.
        let held = app.clone();
        let _ = app.run_on_main_thread(move || super::close(&held));
        crate::home::seen::again(&app).await;
        crate::home::state().tell_news(say!("inviting.sent", username = username));
        crate::home::redraw(&app);
    });
}
