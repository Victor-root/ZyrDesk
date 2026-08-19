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
//! keeping the biggest visible one, which is how it is found the first
//! time and which stops working the moment it leaves the screen: a
//! window that is not on screen is not among the ones that answer, and
//! minimising ZyrDesk takes the picture down with it. Remembering it is
//! what makes coming back from that possible.

// Windows only, and only ever a session: the rest of the product is
// tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use zyr_proto::session::DisplayMode;

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
        });
        // The shape is worth writing down: it is the size the far
        // computer's picture actually arrives at, which is the answer to
        // what a black band on screen means, and it exists nowhere else
        // once the window has been laid in ours.
        crate::journal::note(&format!(
            "image du lecteur {process} posée dans la fenêtre de ZyrDesk, en {}x{}",
            shape.0, shape.1
        ));
        keep_lighting_the_bar(app, window);
        hold_the_shape(app);
    }
    fit(app);
    true
}

/// Lets go, the session being over.
pub fn let_go(app: &AppHandle) {
    let held = app
        .state::<Picture>()
        .held
        .lock()
        .expect("image tenue")
        .take();
    if held.is_some() {
        stop_lighting_the_bar(app);
    }
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

/// The same, the other way from wherever it is, and remembered.
///
/// This one is a person deciding, which the two calls above are not: one
/// applies what was decided before, the other takes the screen back at
/// the end of a session. So this is the only one that writes anything
/// down, and what it writes is what the next session opens as.
pub fn toggle_the_screen(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(crate::HOME)
        .ok_or("la fenêtre de ZyrDesk n'est plus là")?;
    let whole = !window.is_fullscreen().map_err(|e| e.to_string())?;
    take_the_screen(app, whole)?;

    // Writing it down means asking the service, which is a round trip
    // over a pipe: the picture has already moved, and nothing waits for
    // this.
    tauri::async_runtime::spawn(async move {
        crate::settings::remember_display(if whole {
            DisplayMode::Fullscreen
        } else {
            DisplayMode::Windowed
        })
        .await;
    });
    Ok(())
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

/// Lays the picture over the inside of our window, exactly, and takes
/// the floating button along.
///
/// Called on every move and every resize of our window, and at every
/// turn of the session watch: the engine has its own reasons to move its
/// window, and this is what puts it back.
///
/// The button follows from here rather than on its own. It hangs from a
/// corner of the picture, and the picture is this rectangle: worked out
/// twice, it would be worked out once too many, and the two would come
/// apart exactly when the window moves.
#[cfg(windows)]
pub fn fit(app: &AppHandle) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HWND_TOP, IsIconic, IsWindowVisible, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SetWindowPos,
    };

    let Some(held) = taken(app) else {
        return;
    };
    if !alive(&held) {
        return;
    }
    let engine = held.window as windows_sys::Win32::Foundation::HWND;

    let Some(home) = home_window(app) else {
        return;
    };
    // Nothing to lay the picture on. Minimised, our window has no inside
    // left: laying it there would squeeze the picture to nothing and ask
    // the engine to draw for a surface of no size, once a second, for as
    // long as the window stays down. Put away, it is not on screen at
    // all, and the picture would be the only thing left showing.
    //
    // Both come back on their own: the system puts an owned window back
    // up with the one that owns it, and the watch lays this one again a
    // second later.
    // SAFETY: our own window, and both calls only read its state.
    if unsafe { IsIconic(home) } != 0 || unsafe { IsWindowVisible(home) } == 0 {
        return;
    }
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

    let (width, height) = (inside.right - inside.left, inside.bottom - inside.top);
    // SAFETY: the engine's window is one we have already taken in hand.
    // Shown without being activated: asked to show itself the ordinary
    // way, it would take the front, and this runs every second.
    unsafe {
        SetWindowPos(
            engine,
            HWND_TOP,
            corner.x,
            corner.y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
    crate::floating::follow(
        app,
        (corner.x, corner.y, corner.x + width, corner.y + height),
    );
}

/// Puts the picture back in front, so the engine gets the keyboard and
/// the mouse back.
#[cfg(windows)]
fn hand_the_keyboard_back(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let Some(held) = taken(app).filter(alive) else {
        return;
    };
    // SAFETY: a window this program took in hand. Windows may refuse to
    // change the front window, which costs nothing here.
    unsafe { SetForegroundWindow(held.window as windows_sys::Win32::Foundation::HWND) };
}

/// Keeps our title bar drawn as an active window for as long as the
/// picture is in it.
///
/// The picture is a window of its own, and it is the one the system puts
/// at the front, because that is where the engine has to be to hold the
/// keyboard and the mouse. Ours therefore loses the front, and Windows
/// draws a window that has lost the front with a dimmed title bar. What
/// took it is our own picture, inside our own window, so that dimming
/// says something untrue and says it during every windowed session.
///
/// The message that decides it is intercepted and answered « active ».
/// That is what the message is for: the system asks rather than decides,
/// precisely so a window whose companion holds the activation can say it
/// is still the one being used. The question is only answered that way
/// while the front really is ours, so switching to another program dims
/// the bar as it should.
/// Stepping in front of a window's messages is done from the thread that
/// draws it, and none of the callers here is that thread: one drives the
/// session, the other watches it. Handed over rather than done on the
/// spot.
#[cfg(windows)]
fn keep_lighting_the_bar(app: &AppHandle, engine: isize) {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;

    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(home) = home_window(&asked) else {
            return;
        };
        // SAFETY: our own window, from the thread that owns it, and the
        // handler outlives the subclass: it is a plain function of this
        // program.
        unsafe { SetWindowSubclass(home, Some(lit), LIT, engine as usize) };
    });
}

/// Puts that back the way it was, the session being over.
#[cfg(windows)]
fn stop_lighting_the_bar(app: &AppHandle) {
    use windows_sys::Win32::UI::Shell::RemoveWindowSubclass;

    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(home) = home_window(&asked) else {
            return;
        };
        // SAFETY: same window, same thread and same handler as were put
        // on it.
        unsafe { RemoveWindowSubclass(home, Some(lit), LIT) };
    });
}

/// Name our handler answers to, so it can be taken off again.
#[cfg(windows)]
const LIT: usize = 1;

#[cfg(windows)]
unsafe extern "system" fn lit(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _name: usize,
    engine: usize,
) -> isize {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_NCACTIVATE;

    // Told to dim, while the front belongs to the picture or to us: the
    // answer is that this window is still the one being used.
    let lie = message == WM_NCACTIVATE && wparam == 0 && ours_is_at_the_front(engine);
    // SAFETY: the arguments are the ones the system handed in, with at
    // most a boolean turned around.
    unsafe { DefSubclassProc(window, message, if lie { 1 } else { wparam }, lparam) }
}

/// Whether the window at the front is the picture, or another of ours.
///
/// Both answers mean the same thing here. The picture is ours in all but
/// the process it runs in, and the moment the system takes the front
/// away it has not always given it to anybody yet, which reads as ours.
#[cfg(windows)]
fn ours_is_at_the_front(engine: usize) -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: no argument, and a null answer is one of the answers.
    let front = unsafe { GetForegroundWindow() };
    if front.is_null() {
        return false;
    }
    if front as usize == engine {
        return true;
    }
    let mut owner = 0u32;
    // SAFETY: the window comes from the call above and the slot is ours.
    unsafe { GetWindowThreadProcessId(front, &mut owner) };
    // SAFETY: no argument.
    owner == unsafe { GetCurrentProcessId() }
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
fn keep_lighting_the_bar(_app: &AppHandle, _engine: isize) {}

#[cfg(not(windows))]
fn stop_lighting_the_bar(_app: &AppHandle) {}

#[cfg(not(windows))]
pub fn fit(_app: &AppHandle) {}
