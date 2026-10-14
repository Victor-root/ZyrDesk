//! How the person arranged their computers, kept from one opening of
//! the window to the next: as cards in a grid or as rows in a list.

use crate::home::note;
use crate::home::state::View;

impl View {
    /// What the file writes.
    fn name(self) -> &'static str {
        match self {
            View::Grid => "grid",
            View::List => "list",
        }
    }
}

/// Reads back how the computers were last laid out, once, before the
/// window is drawn: the thread that draws touches no disk.
pub fn read_back() {
    let Ok(written) = std::fs::read_to_string(zyr_proto::paths::home_layout()) else {
        return;
    };
    let view = zyr_proto::files::settings(&written)
        .find(|(key, _)| *key == "view")
        .and_then(|(_, said)| {
            [View::Grid, View::List]
                .into_iter()
                .find(|view| view.name() == said)
        });
    if let Some(view) = view {
        crate::home::state().view = view;
    }
}

/// Writes down how the computers are laid out, after a click chose it,
/// away from the thread that draws.
pub fn keep(view: View) {
    crate::shell::app::spawn_blocking(move || write(view));
}

fn write(view: View) {
    let written = format!(
        "# How the computers of ZyrDesk's home screen are laid out: grid or list.\n\
         # Written by ZyrDesk, can be corrected by hand.\n\
         view = {}\n",
        view.name()
    );
    if let Err(e) = zyr_proto::files::replace(&zyr_proto::paths::home_layout(), &written) {
        note(&format!("layout of the computers not kept: {e}"));
    }
}
