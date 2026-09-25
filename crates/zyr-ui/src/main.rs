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

mod app;
mod design;
mod desk;
mod floating;
mod folders;
mod icon;
mod journal;

// The floating button's logo, drawn by this program: it only exists
// on Windows, like the window that carries it.
#[cfg(windows)]
mod logo;

// What draws the interface without a browser. Windows only, like the
// windows it dresses.
#[cfg(windows)]
mod paint;

// The icons this program draws, shared by all of its screens.
#[cfg(windows)]
mod icons;

// The beat of whatever moves, tuned to the Windows compositor.
#[cfg(windows)]
mod pulse;

// The floating button's menu, drawn by this program.
#[cfg(windows)]
mod menu;

// The home window, drawn by this program.
#[cfg(windows)]
mod home;

// The window itself, opened by this program.
mod main_window;

/// Outside Windows there is no home window, and no session either: what
/// a session says while it opens then falls into the void, like
/// everything else that draws.
#[cfg(not(windows))]
mod home {
    use crate::app::App;

    pub fn step(_app: &App, _detail: &str) {}
    pub fn coming_back(_app: &App, _attempt: u32) {}
    pub fn put_the_opening_away(_app: &App) {}
    pub fn failed(_app: &App, _text: &str) {}
}

mod picture;
mod pointer;
mod service;
mod session;
mod settings;
mod shortcuts;
mod startup;
// The figures of a session, in the corner of its picture. What is
// written compiles everywhere; the card is a window, so Windows'.
mod statistics;
// The keys Windows keeps for itself, taken for the session on request.
// The decision compiles everywhere; the hook is Windows'.
mod system_keys;
mod theme;
// What the floating button shows of the files arriving: the pane of the
// mark fills like a loading bar. The reading compiles everywhere; the
// drawing is the button's.
mod transfer;
mod tray;
// The two badges of a session, in the corner of the picture opposite the
// floating button. What decides compiles everywhere; the badges are a
// window, so they belong to Windows.
mod badges;

// The picture of a session, in a window of ours. What a message means
// compiles everywhere; the window is Windows'.
mod video;

#[cfg(windows)]
mod elevated;

// A hook of the system lives on a thread of its own, which only Windows
// has to offer.
#[cfg(windows)]
mod hook;

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

    let app = app::App::new();
    if let Err(e) = app::open_the_mailbox() {
        journal::note(&format!("ZyrDesk ne démarre pas : {e}"));
        return;
    }
    // What the person chose to look at, read again before the window
    // opens: a window that opened in the wrong theme, even for the
    // length of a beat, would be seen.
    theme::what_was_chosen();
    if let Err(e) = main_window::open(&app) {
        journal::note(&format!("ZyrDesk ne démarre pas : {e}"));
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
        journal::note(&format!("pas d'icône dans la zone de notification : {e}"));
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
pub fn show_home(_app: &crate::app::App) {
    main_window::show();
}
