//! The ZyrDesk application: the only window the user ever sees.
//!
//! It holds nothing. The service owns the identity, the tunnels and the
//! engines; this program asks it questions and shows the answers. That
//! is what lets the window be closed, updated or killed in the middle of
//! a session without the picture stopping.
//!
//! The video is never here either: the player is a separate native
//! window, so nothing about this interface is on the path of a frame.

// A second console window opening behind the interface would give the
// game away immediately.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desk;
mod session;
mod theme;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            desk::standing,
            desk::peers,
            desk::set_hosting,
            session::connect,
            theme::set_theme
        ])
        .run(tauri::generate_context!())
        .expect("l'interface ZyrDesk n'a pas pu démarrer");
}
