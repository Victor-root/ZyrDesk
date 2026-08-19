//! The far computer's picture, inside the product's own window.
//!
//! The engine draws in a window of its own, which it must: nothing of
//! ZyrDesk is on the path of a frame, and a picture handed to a web view
//! would cost exactly what this product exists to save. But two windows
//! for one thing is one too many, so that window is stripped of its
//! frame, laid over the inside of ours, and made to follow it.
//!
//! Laid over rather than put inside. Windows offers both: a child window
//! really is part of its parent, and an owned window stays a window of
//! its own that never leaves the front of the one that owns it. The
//! second is what is used here, and the difference is not cosmetic. The
//! engine takes the keyboard and the mouse by asking the system for
//! them, and what the system grants depends on its window being the one
//! at the front. A child window is never at the front of anything: it
//! would have cost the very thing a remote desktop is for.
//!
//! Which leaves the product's window standing behind the picture,
//! holding the title bar, the frame and whatever is drawn around it, and
//! the picture following it from screen to screen as one thing.

// Windows only, and only ever a session: the rest of the product is
// tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

/// The engine window this program has taken in hand.
#[derive(Default)]
pub struct Picture {
    /// Player it belongs to, so it is only taken in hand once.
    held: Mutex<Option<u32>>,
}

/// Takes that player's window in hand, and lays it over ours.
///
/// Called again for as long as the session lasts: the engine resizes its
/// own window when the far computer changes resolution, and puts it back
/// where it thinks it belongs.
pub fn hold(app: &AppHandle, process: u32) {
    let state = app.state::<Picture>();
    let first = {
        let mut held = state.held.lock().expect("image tenue");
        let first = *held != Some(process);
        *held = Some(process);
        first
    };
    if first {
        take_the_frame_away(app, process);
    }
    fit(app);
}

/// Lets go, the session being over.
pub fn let_go(app: &AppHandle) {
    *app.state::<Picture>().held.lock().expect("image tenue") = None;
}

/// Puts our window on the whole screen, or takes it back off.
///
/// The picture follows it, and it is our window that does this now: the
/// engine is always started in a window of ordinary size, and how much
/// of the screen the session takes stopped being its business the day
/// its window went inside ours.
pub fn take_the_screen(app: &AppHandle, whole: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(crate::HOME)
        .ok_or("la fenêtre de ZyrDesk n'est plus là")?;
    window.set_fullscreen(whole).map_err(|e| e.to_string())?;
    fit(app);
    Ok(())
}

/// The same, the other way from wherever it is.
pub fn toggle_the_screen(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(crate::HOME)
        .ok_or("la fenêtre de ZyrDesk n'est plus là")?;
    let whole = window.is_fullscreen().map_err(|e| e.to_string())?;
    take_the_screen(app, !whole)
}

/* ---- Ce qui appartient à Windows ------------------------------------- */

/// Strips the engine's window of everything a window of its own would
/// carry, and hands it to ours.
#[cfg(windows)]
fn take_the_frame_away(app: &AppHandle, process: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, SetWindowLongPtrW, WS_CAPTION,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME,
    };

    let (Some(engine), Some(home)) = (crate::floating::window_of(process), home_window(app)) else {
        return;
    };

    // SAFETY: both windows exist, and only their styles are read and
    // written back.
    unsafe {
        let style = GetWindowLongPtrW(engine, GWL_STYLE);
        let bare = style
            & !((WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX)
                as isize);
        SetWindowLongPtrW(engine, GWL_STYLE, bare | WS_POPUP as isize);

        // Owned by our window: always in front of it, minimised with it,
        // and gone from the taskbar and from alt-tab, where it would
        // otherwise stand as a second ZyrDesk.
        SetWindowLongPtrW(engine, GWLP_HWNDPARENT, home as isize);

        let extended = GetWindowLongPtrW(engine, GWL_EXSTYLE);
        SetWindowLongPtrW(
            engine,
            GWL_EXSTYLE,
            (extended | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
        );
    }
    crate::journal::note(&format!(
        "image du lecteur {process} posée dans la fenêtre de ZyrDesk"
    ));
}

/// Lays the picture over the inside of our window, exactly.
///
/// Called on every move and every resize of our window, and at every
/// turn of the session watch: the engine has its own reasons to move its
/// window, and this is what puts it back.
#[cfg(windows)]
pub fn fit(app: &AppHandle) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HWND_TOP, SWP_NOACTIVATE, SetWindowPos,
    };

    let process = *app.state::<Picture>().held.lock().expect("image tenue");
    let (Some(process), Some(home)) = (process, home_window(app)) else {
        return;
    };
    let Some(engine) = crate::floating::window_of(process) else {
        return;
    };

    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut corner = POINT { x: 0, y: 0 };
    // SAFETY: the window exists and both slots are ours.
    if unsafe { GetClientRect(home, &mut inside) } == 0 {
        return;
    }
    // SAFETY: same window, and the point is ours; it goes in as a
    // position inside the window and comes back as one on the screen.
    unsafe { ClientToScreen(home, &mut corner) };

    // SAFETY: the engine's window is one we have already taken in hand,
    // and nothing here activates it: the keyboard stays where it is.
    unsafe {
        SetWindowPos(
            engine,
            HWND_TOP,
            corner.x,
            corner.y,
            inside.right - inside.left,
            inside.bottom - inside.top,
            SWP_NOACTIVATE,
        )
    };
}

/// Our own window, as the system knows it.
#[cfg(windows)]
fn home_window(app: &AppHandle) -> Option<windows_sys::Win32::Foundation::HWND> {
    app.get_webview_window(crate::HOME)
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as _)
}

#[cfg(not(windows))]
fn take_the_frame_away(_app: &AppHandle, _process: u32) {}

#[cfg(not(windows))]
pub fn fit(_app: &AppHandle) {}
