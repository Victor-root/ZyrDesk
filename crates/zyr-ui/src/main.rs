//! The ZyrDesk application: the only window the user ever sees.
//!
//! It holds almost nothing. The service owns the identity, the tunnels
//! and the far computer's side of everything; this program asks it
//! questions and shows the answers.
//!
//! And it draws them itself. There is no web view left anywhere in this
//! product: the window the toolkit opens is a bare one, and its inside is
//! a canvas this program paints, like the floating button and its menu.
//!
//! The one thing it does hold is the player of a session, which plays in
//! this program and goes when it goes. That is deliberate: a player left
//! running behind a window that is no longer there would hold the far
//! computer's desktop with nothing on screen to give it back.
//!
//! The player draws the picture into a window of this program, a child of
//! the main one, from threads of its own: nothing about this interface is
//! on the path of a frame, and there is one window on screen, not two.

// A second console window opening behind the interface would give the
// game away immediately.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The program around the windows: the program itself, its main window,
// the icon by the clock, the journal, the theme, what the person chose
// and the keyboard shortcuts they set, and what every part asks of the
// service.
mod shell;

// The home window, drawn by this program.
#[cfg(windows)]
mod home;

/// Outside Windows there is no home window, and no session either: what
/// a session says while it opens then falls into the void, like
/// everything else that draws.
#[cfg(not(windows))]
mod home {
    use zyr_proto::fact::Fact;

    use crate::shell::app::App;

    pub fn step(_app: &App, _detail: &Fact) {}
    pub fn coming_back(_app: &App, _attempt: u32) {}
    pub fn put_the_opening_away(_app: &App) {}
    pub fn failed(_app: &App, _why: &Fact) {}
}

// A session, as this program opens it, plays it and dresses its picture:
// the floating button and its menu, the badges and the figures.
mod session;

use session::floating;
#[cfg(windows)]
use shell::win32;
use shell::{app, icon, journal, main_window, service, shortcuts, theme, tray};

fn main() {
    // Two ZyrDesk running at once would put two floating buttons on the
    // same session. Whoever starts the second one wanted the window
    // back, which is what they get.
    if app::already_open() {
        return;
    }
    // What this program draws is counted in real pixels, on every
    // screen: said before a single window exists, or else the system
    // would itself enlarge what is already at the right size.
    app::count_in_real_pixels();
    journal::opened();
    // The words, in the language the person reads Windows in, chosen
    // before a single one is shown.
    #[cfg(windows)]
    journal::note(&format!(
        "words in {}",
        zyr_i18n::speak(&win32::preferred_languages())
    ));

    let app = app::App::new();
    if let Err(e) = app::open_the_mailbox() {
        journal::note(&format!("ZyrDesk does not start: {e}"));
        return;
    }
    // What the person chose to look at, read again before the window
    // opens: a window that opened in the wrong theme, even for the
    // length of a beat, would be seen.
    theme::what_was_chosen();
    if let Err(e) = main_window::open(&app) {
        journal::note(&format!("ZyrDesk does not start: {e}"));
        return;
    }
    theme::on_the_window();
    // What Windows wants, followed for as long as the program
    // runs.
    theme::watch(app.clone());
    // The window's own icon: taken from the compiled resource at the two
    // sizes Windows is about to draw it at.
    icon::on_the_window();
    // The icon beside the clock: from here on, something on screen says
    // this program is running, whatever becomes of the window.
    if let Err(e) = tray::raise() {
        journal::note(&format!("no icon in the notification area: {e}"));
    }
    // Nothing of this product runs while nobody is using it, so opening
    // it is what puts the service back on its feet.
    service::wake_the_service();
    tray::watch(app.clone());
    // Where a hand last left the floating button, read before any session
    // can ask for it.
    floating::where_it_was_left();
    floating::watch(app.clone());
    // A session gives the keyboard to the far computer, so what is left
    // to us has to be asked of the system rather than waited for as an
    // ordinary key press.
    shortcuts::listen(app.clone());
    // And the home screen itself, drawn in the inside of this window.
    // Last: it asks the service what it shows, and the service has just
    // been woken up.
    #[cfg(windows)]
    home::raise(&app);
    main_window::show();

    app::run();
}

/// Brings the home window back, wherever it was left.
///
/// The picture and the floating button come back with it without being
/// told: both are windows the system knows this one owns, and it puts
/// them back up when it puts this one back up.
pub fn show_home(_app: &crate::shell::app::App) {
    main_window::show();
}
