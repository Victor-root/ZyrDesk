//! Renaming a device of the account: the name the account shows of it,
//! on every other device.

use zyr_draw::Rect;
use zyr_i18n::say;

use super::{Action, Shape, trouble};
use crate::home::state::{Dialog, Target};
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Kind, Painter, size};
use crate::shell::app::App;

pub fn shape(_painter: &Painter, name: &str) -> Shape {
    let new_name = fields::text(Field::NewName);
    let new_name = new_name.trim();
    Shape {
        title: say!("renaming.title", name = name),
        lead: Some(say!("renaming.lead")),
        width: 480.0,
        left: Vec::new(),
        right: vec![
            Action::new(say!("dialog.cancel"), Kind::Quiet, Target::CloseDialog),
            Action::new(say!("renaming.rename"), Kind::Primary, Target::Confirm)
                .enabled(!new_name.is_empty() && new_name != name),
        ],
    }
}

pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let (left, width) = (at.left, size(at).0);
    let mut y = at.top;
    y += painter.field(
        (left, y, width),
        Field::NewName,
        &say!("renaming.new_name"),
        None,
    );
    y += trouble(painter, (left, y, width));
    y - at.top
}

/// Renames the device with what is written, if it is a new name.
pub fn confirm(app: &App) {
    let new_name = fields::text(Field::NewName).trim().to_string();
    let Some(Dialog::Renaming { device, name }) = crate::home::state().dialog.clone() else {
        return;
    };
    if new_name.is_empty() || new_name == name {
        return;
    }
    super::close(app);

    let app = app.clone();
    crate::shell::app::spawn(async move {
        if let Err(reason) = crate::shell::desk::rename_device(device, new_name).await {
            crate::home::state().tell_trouble(&reason);
        }
        crate::home::seen::again(&app).await;
        crate::home::redraw(&app);
    });
}
