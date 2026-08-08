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
mod service;
mod session;
mod settings;
mod theme;

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
        .invoke_handler(tauri::generate_handler![
            desk::standing,
            desk::peers,
            desk::set_hosting,
            session::connect,
            session::sessions,
            settings::settings,
            settings::choose,
            settings::logs_folder,
            settings::open_logs,
            floating::floating_size,
            floating::floating_hide,
            floating::floating_move,
            floating::floating_act,
            theme::set_theme
        ])
        .setup(|app| {
            floating::watch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the home window during a session must not take the
            // floating button with it. The window steps aside instead,
            // and comes back whole when ZyrDesk is started again.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == HOME
                && floating::busy(window.app_handle())
            {
                api.prevent_close();
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

/// Whether the home window is standing aside, waiting for a session to
/// end.
pub fn home_is_hidden(app: &tauri::AppHandle) -> bool {
    app.get_webview_window(HOME)
        .is_some_and(|window| !window.is_visible().unwrap_or(true))
}
