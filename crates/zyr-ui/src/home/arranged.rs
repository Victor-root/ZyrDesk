//! How the person arranged their computers, kept from one opening of
//! the window to the next: as cards in a grid or as rows in a list, and
//! in the order they put them in.

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
    let said = |name: &str| {
        zyr_proto::files::settings(&written)
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    };
    let view = said("view").and_then(|said| {
        [View::Grid, View::List]
            .into_iter()
            .find(|view| view.name() == said)
    });
    let order: Vec<String> = said("order")
        .map(|said| {
            said.split(',')
                .map(str::trim)
                .filter(|fingerprint| !fingerprint.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut state = crate::home::state();
    if let Some(view) = view {
        state.view = view;
    }
    state.order = order;
}

/// Writes down how the computers are laid out, after a hand chose it,
/// away from the thread that draws.
pub fn keep(view: View, order: Vec<String>) {
    crate::shell::app::spawn_blocking(move || write(view, &order));
}

fn write(view: View, order: &[String]) {
    let written = format!(
        "# How the computers of ZyrDesk's home screen are laid out: grid or list,\n\
         # and the order they were put in, by fingerprint.\n\
         # Written by ZyrDesk, can be corrected by hand.\n\
         view = {}\n\
         order = {}\n",
        view.name(),
        order.join(",")
    );
    if let Err(e) = zyr_proto::files::replace(&zyr_proto::paths::home_layout(), &written) {
        note(&format!("layout of the computers not kept: {e}"));
    }
}
