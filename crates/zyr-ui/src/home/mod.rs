//! ZyrDesk's home window: the home page, the settings, « About », and the
//! dialogs over them.
//!
//! It fits in an ordinary window framed by Windows, whose inside this
//! program draws: the same drawing as the floating button and the session
//! menu, the same design system, the same icons.
//!
//! **It decides nothing.** What it shows is gathered for it (`seen`):
//! the service says what this computer is doing and which computers it
//! knows, Windows says what this computer is and who sits at it. What a
//! click does goes back the same way (`act`), and the window draws what
//! comes back.
//!
//! What it is made of, from the bottom up: what it shows (`seen`) and
//! what it remembers from one frame to the next (`state`) and from one
//! opening to the next (`arranged`); the pieces it is drawn with (`ui`),
//! the words (`words`); the frame around every page (`frame`), the pages
//! (`pages`), the dialogs (`dialogs`) and the opening of a session
//! (`opening`); and the window itself, the hand and the keys (`window`,
//! `input`, `act`).

mod act;
mod arranged;
mod dialogs;
mod frame;
mod input;
mod licences;
mod opening;
mod pages;
mod seen;
mod state;
mod ui;
mod window;
mod words;

use std::sync::{LazyLock, Mutex, MutexGuard};

use crate::shell::app::App;

pub use dialogs::{knocked, no_longer_knocking};
pub use opening::{coming_back, failed, put_the_opening_away, step};
pub use window::measure_the_screen;

/// What this module's lines are filed under.
const TAG: &str = "home";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::shell::journal::note_about(TAG, what);
}

/// The window's memory, shared between the thread that draws and the
/// tasks that come back from the service.
static STATE: LazyLock<Mutex<state::State>> = LazyLock::new(|| Mutex::new(state::State::new()));

/// The window's memory, for the length of what holds it.
///
/// Never taken while what is shown is held by someone else on the same
/// thread the other way round: the drawing holds what is shown, then
/// this, and every other part takes them in that order or one at a time.
fn state() -> MutexGuard<'static, state::State> {
    STATE.lock().expect("the home window's memory")
}

/// Opens the home window in the window Windows frames, and starts
/// gathering what it shows.
pub fn raise(app: &App) {
    arranged::read_back();
    if window::open(app) {
        seen::watch(app.clone());
    }
}

/// The home window, as Windows knows it: the window that carries it sizes
/// it with itself and hands it the keyboard.
pub fn its_canvas() -> isize {
    window::handle()
}

/// Asks for a frame, from any thread.
pub fn redraw(app: &App) {
    if window::handle() == 0 {
        return;
    }
    let _ = app.run_on_main_thread(window::invalidate);
}

/// Draws everything again in the theme now in force, the fields of an
/// open dialog included, from any thread.
pub fn theme_changed(app: &App) {
    if window::handle() == 0 {
        return;
    }
    let _ = app.run_on_main_thread(window::invalidate_with_the_fields);
}
