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
//! The window is also given the picture's shape rather than the picture
//! the window's. The engine draws what arrives in the shape it arrives
//! in and never in another: given a window of a different shape it
//! centres the picture and fills the rest with black. Every black band
//! this end of the product could show comes from that one fact, and the
//! only way not to show one is not to ask for one.
//!
//! The window is remembered once taken, and never looked for again.
//! Looking for it means going through every window on the machine and
//! keeping the biggest visible one, which is how it was found the first
//! time and which stops working the moment it is put away: a window that
//! is not on screen is not among the ones that answer. Remembering it is
//! what makes putting it away something one can come back from.

// Windows only, and only ever a session: the rest of the product is
// tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

/// The engine window this program has taken in hand.
#[derive(Default)]
pub struct Picture {
    held: Mutex<Option<Held>>,
}

/// A window taken in hand, and what is known of it.
#[derive(Clone, Copy)]
struct Held {
    /// Player it belongs to.
    process: u32,
    /// The window itself, as a plain number: a window handle is a raw
    /// pointer, which neither travels between threads nor exists off
    /// Windows.
    window: isize,
    /// Shape of the picture, as the width and height it was drawn at
    /// before anything of ours touched that window.
    ///
    /// Read there rather than taken from what the session asked for. The
    /// engine sizes that window to the picture it is about to receive,
    /// and the picture is not always the size that was asked: the far
    /// computer answers with what its screen turned out to be able to
    /// do.
    shape: (i32, i32),
    /// Put away with the home window rather than laid over it.
    aside: bool,
}

/// Takes that player's window in hand and lays it over ours, and says
/// whether it now holds one.
///
/// Answers false for as long as the engine has not opened its window,
/// which is most of the time a session takes to start. Nothing is
/// remembered in that case: the next call tries again.
pub fn hold(app: &AppHandle, process: u32) -> bool {
    let state = app.state::<Picture>();
    let known = *state.held.lock().expect("image tenue");
    let already = known.filter(|held| held.process == process && alive(held));

    if already.is_none() {
        let Some((window, shape)) = take_the_frame_away(app, process) else {
            return false;
        };
        *state.held.lock().expect("image tenue") = Some(Held {
            process,
            window,
            shape,
            aside: false,
        });
        // The shape is worth writing down: it is the size the far
        // computer's picture actually arrives at, which is the answer to
        // what a black band on screen means, and it exists nowhere else
        // once the window has been laid in ours.
        crate::journal::note(&format!(
            "image du lecteur {process} posée dans la fenêtre de ZyrDesk, en {}x{}",
            shape.0, shape.1
        ));
        hold_the_shape(app);
    }
    fit(app);
    true
}

/// Lets go, the session being over.
pub fn let_go(app: &AppHandle) {
    *app.state::<Picture>().held.lock().expect("image tenue") = None;
}

/// Puts the picture away with the home window, or brings it back.
///
/// Closing the home window would otherwise leave the picture standing on
/// screen with nothing left to reach it by: it has no frame, no buttons
/// and no place in alt-tab any more, those having been taken away when
/// it was laid over ours.
pub fn put_aside(app: &AppHandle, aside: bool) {
    let state = app.state::<Picture>();
    {
        let mut held = state.held.lock().expect("image tenue");
        let Some(held) = held.as_mut() else {
            return;
        };
        held.aside = aside;
    }
    fit(app);
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
    // Taking the screen activates our window, which the toolkit does on
    // purpose and cannot be asked not to. The engine loses the front,
    // and with it the keyboard and the mouse it had asked the system
    // for. Handed straight back.
    hand_the_keyboard_back(app);
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

/// How far the window may be off the picture's shape before it is put
/// back on it.
///
/// One pixel: a shape worked out by dividing whole numbers lands on a
/// whole number that is sometimes half a pixel off, and a window put
/// back on a shape it is already on would ask to be put back again, for
/// as long as the session lasts.
const ROUNDING: u32 = 1;

/// Keeps our window the shape of the picture in it.
///
/// This is where the black bands come from at this end. The engine draws
/// the far computer's picture in the shape it arrives in and never in
/// another: given a window of a different shape, it centres the picture
/// and fills what is left with black. So the window is given the
/// picture's shape rather than the picture the window's, and dragging a
/// corner then changes how big the session is and never how it looks.
///
/// Only the height moves. Both would fight whichever edge is being
/// dragged, and a window that resists in two directions at once cannot
/// be resized at all.
pub fn hold_the_shape(app: &AppHandle) {
    let Some(held) = taken(app) else {
        return;
    };
    let (wide, high) = held.shape;
    if wide <= 0 || high <= 0 {
        return;
    }
    let Some(window) = app.get_webview_window(crate::HOME) else {
        return;
    };
    // Covering the screen is a shape nobody chose and nobody drags, and
    // so is a window put against the edges of the screen by the system.
    if window.is_fullscreen().unwrap_or(false) || window.is_maximized().unwrap_or(false) {
        return;
    }
    let Ok(inside) = window.inner_size() else {
        return;
    };
    let wanted = (u64::from(inside.width) * high as u64 / wide as u64) as u32;
    if wanted == 0 || inside.height.abs_diff(wanted) <= ROUNDING {
        return;
    }
    let _ = window.set_size(tauri::PhysicalSize::new(inside.width, wanted));
}

/// What is held right now, if anything.
fn taken(app: &AppHandle) -> Option<Held> {
    *app.state::<Picture>().held.lock().expect("image tenue")
}

/* ---- Ce qui appartient à Windows ------------------------------------- */

/// Strips the engine's window of everything a window of its own would
/// carry, hands it to ours, and says which window it was and what shape
/// the picture in it has.
#[cfg(windows)]
fn take_the_frame_away(app: &AppHandle, process: u32) -> Option<(isize, (i32, i32))> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GWLP_HWNDPARENT, GetClientRect, GetWindowLongPtrW,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW,
        SetWindowPos, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX,
        WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };

    let engine = crate::floating::window_of(process)?;
    let home = home_window(app)?;

    // Read before anything of ours moves that window: what it was born
    // at is the shape of the picture that is about to arrive in it, and
    // it is the last moment that can be known.
    let mut drawn = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: the window exists and the rectangle is ours.
    if unsafe { GetClientRect(engine, &mut drawn) } == 0 {
        return None;
    }
    let shape = (drawn.right - drawn.left, drawn.bottom - drawn.top);

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

        // A style written down is not a style applied: the frame is only
        // recomputed when the window is told so.
        SetWindowPos(
            engine,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Some((engine as isize, shape))
}

/// Whether that window is still the engine's.
///
/// A window number is handed back to the system when its window goes,
/// and given out again to somebody else's. Both halves are asked: the
/// window exists, and it belongs to the player we think it does.
#[cfg(windows)]
fn alive(held: &Held) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

    let window = held.window as windows_sys::Win32::Foundation::HWND;
    // SAFETY: a window number, which the call is made to weigh.
    if unsafe { IsWindow(window) } == 0 {
        return false;
    }
    let mut owner = 0u32;
    // SAFETY: same window, and the slot is ours.
    unsafe { GetWindowThreadProcessId(window, &mut owner) };
    owner == held.process
}

/// Lays the picture over the inside of our window, exactly, or puts it
/// away when that is what was asked.
///
/// Called on every move and every resize of our window, and at every
/// turn of the session watch: the engine has its own reasons to move its
/// window, and this is what puts it back.
#[cfg(windows)]
pub fn fit(app: &AppHandle) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HWND_TOP, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
    };

    let Some(held) = taken(app) else {
        return;
    };
    if !alive(&held) {
        return;
    }
    let engine = held.window as windows_sys::Win32::Foundation::HWND;

    if held.aside {
        // SAFETY: a window this program took in hand, and nothing here
        // moves, resizes or activates it.
        unsafe {
            SetWindowPos(
                engine,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW,
            )
        };
        return;
    }

    let Some(home) = home_window(app) else {
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

    // SAFETY: the engine's window is one we have already taken in hand.
    // Shown without being activated: asked to show itself the ordinary
    // way, it would take the front, and this runs every second.
    unsafe {
        SetWindowPos(
            engine,
            HWND_TOP,
            corner.x,
            corner.y,
            inside.right - inside.left,
            inside.bottom - inside.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
}

/// Puts the picture back in front, so the engine gets the keyboard and
/// the mouse back.
#[cfg(windows)]
fn hand_the_keyboard_back(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let Some(held) = taken(app).filter(|held| !held.aside && alive(held)) else {
        return;
    };
    // SAFETY: a window this program took in hand. Windows may refuse to
    // change the front window, which costs nothing here.
    unsafe { SetForegroundWindow(held.window as windows_sys::Win32::Foundation::HWND) };
}

/// Our own window, as the system knows it.
#[cfg(windows)]
fn home_window(app: &AppHandle) -> Option<windows_sys::Win32::Foundation::HWND> {
    app.get_webview_window(crate::HOME)
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as _)
}

#[cfg(not(windows))]
fn take_the_frame_away(_app: &AppHandle, _process: u32) -> Option<(isize, (i32, i32))> {
    None
}

#[cfg(not(windows))]
fn alive(_held: &Held) -> bool {
    false
}

#[cfg(not(windows))]
fn hand_the_keyboard_back(_app: &AppHandle) {}

#[cfg(not(windows))]
pub fn fit(_app: &AppHandle) {}
