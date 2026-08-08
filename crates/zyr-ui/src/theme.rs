//! Accorder la fenêtre à ce que la page a choisi.
//!
//! The page decides the theme: it knows what the system asks for and
//! what the person chose over it. What it cannot reach is the window
//! frame, which belongs to Windows. Without this, a light interface
//! would keep a dark title bar, which is exactly the kind of seam a
//! product is judged on.

use tauri::{Theme, Window};

#[tauri::command]
pub fn set_theme(window: Window, clair: bool) {
    // A window that refuses the change is not worth stopping for: the
    // page is already drawn in the right theme, only its frame is not.
    let _ = window.set_theme(Some(if clair { Theme::Light } else { Theme::Dark }));
}
