//! The ZyrDesk application: the only window the user ever sees.
//!
//! It holds nothing. The service owns the identity, the tunnels and the
//! engines; this program asks it questions and shows the answers. That
//! is what lets the window be closed, updated or killed in the middle of
//! a session without the picture stopping.
//!
//! The video is never here either: the player is a separate native
//! window, so nothing about this interface is on the path of a frame.
//! The one thing this program keeps on screen during a session is the
//! floating button, which is a window of its own.

// A second console window opening behind the interface would give the
// game away immediately.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desk;
mod floating;
mod folders;
mod journal;
mod service;
mod session;
mod settings;
mod shortcuts;
mod startup;
mod theme;
mod tray;

#[cfg(windows)]
mod elevated;

use tauri::{Manager, WindowEvent};

/// The home window, as the configuration names it.
const HOME: &str = "main";

fn main() {
    let building = tauri::Builder::default();

    // Two ZyrDesk running at once would put two floating buttons on the
    // same session. Whoever starts the second one wanted the window
    // back, which is what they get.
    #[cfg(windows)]
    let building = building.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        show_home(app);
    }));

    building
        .manage(floating::Floating::default())
        .manage(tray::Shown::default())
        .invoke_handler(tauri::generate_handler![
            desk::standing,
            desk::build,
            desk::peers,
            desk::set_hosting,
            desk::set_trust,
            desk::authorize,
            desk::forget,
            desk::set_at_boot,
            desk::start_service,
            desk::stop_service,
            folders::engines,
            folders::logs_folder,
            folders::open_folder,
            journal::journal,
            journal::clear_journal,
            session::connect,
            session::sessions,
            settings::settings,
            settings::choose,
            shortcuts::shortcuts,
            shortcuts::bind,
            floating::floating_size,
            floating::floating_hide,
            floating::floating_grab,
            floating::floating_act,
            theme::set_theme
        ])
        .setup(|app| {
            journal::opened();
            // The icon first: from here on, something on screen says this
            // program is running, whatever becomes of the window.
            if let Err(e) = tray::raise(app.handle()) {
                journal::note(&format!("no icon in the notification area: {e}"));
            }
            // Nothing of this product runs while nobody is using it, so
            // opening it is what puts the service back on its feet.
            service::wake_the_service();
            tray::watch(app.handle().clone());
            floating::watch(app.handle().clone());
            // A session gives the keyboard to the far computer, so what
            // is left to us has to be asked of the system rather than
            // waited for as an ordinary key press.
            shortcuts::listen(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the home window never ends anything. It steps
            // aside, the icon beside the clock stays, and « Quitter »
            // there is the one thing that stops the product. A window
            // whose cross cut a session in progress would be worse than
            // one that does not close at all.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == HOME
            {
                api.prevent_close();
                floating::Floating::put_away_on_purpose(window.app_handle());
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("l'interface ZyrDesk n'a pas pu démarrer");
}

/// Brings the home window back, wherever it was left.
pub fn show_home(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(HOME) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Puts the home window aside, without ending anything.
pub fn hide_home(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(HOME) {
        let _ = window.hide();
    }
}

/// Whether the home window is standing aside, waiting for a session to
/// end.
pub fn home_is_hidden(app: &tauri::AppHandle) -> bool {
    app.get_webview_window(HOME)
        .is_some_and(|window| !window.is_visible().unwrap_or(true))
}
