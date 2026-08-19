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
//! Finding it in the first place means going through every window on the
//! machine and picking the one the engine has just opened, which is
//! recognised by the title our own rebranding put on it: the engine
//! opens other windows, one of them larger than the picture, and taking
//! the biggest meant laying an empty window inside ours and leaving the
//! picture beside it. Remembering it afterwards is what makes coming back
//! from a minimised window possible, since a window that is not on screen
//! is not among the ones that answer.

// Windows only, and only ever a session: the rest of the product is
// tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, Ordering};

use tauri::{AppHandle, Manager};
use zyr_proto::session::DisplayMode;

/// The engine window this program has taken in hand.
#[derive(Default)]
pub struct Picture {
    held: Mutex<Option<Held>>,
}

/// The same picture, and its shape, where our window's own message
/// handler can reach them.
///
/// That handler runs inside the system's call into our window and cannot
/// wait on anything: a lock held by a thread that is itself waiting on
/// the window would stop both for good. What it needs is two numbers, so
/// two numbers are what it is given.
static ENGINE: AtomicIsize = AtomicIsize::new(0);
static SHAPE: AtomicI64 = AtomicI64::new(0);

/// Set while the person is dragging an edge of our window.
///
/// The shape is held during the drag, before each resize; tidying it up
/// afterwards as well would resize the window a second time for nothing.
static DRAGGED: AtomicBool = AtomicBool::new(false);

/// Size the picture was last given a shape for, so it is only reshaped
/// when it really changes size.
static LAID: AtomicI64 = AtomicI64::new(0);

/// Radius the system rounds a window's corners by, in page pixels.
///
/// Windows has never offered it as a number to ask for; this is the one
/// it uses for an ordinary window, and it is scaled to the screen the
/// window is on.
const CORNER: i32 = 8;

/// Width and height of the picture, as the handler reads them.
fn shape() -> (i32, i32) {
    let both = SHAPE.load(Ordering::Relaxed);
    ((both >> 32) as i32, both as i32)
}

fn remember_the_shape(engine: isize, shape: (i32, i32)) {
    // The next picture is a different window and has never been given a
    // shape, whatever size this one was left at.
    LAID.store(0, Ordering::Relaxed);
    SHAPE.store(
        (i64::from(shape.0) << 32) | i64::from(shape.1) & 0xFFFF_FFFF,
        Ordering::Relaxed,
    );
    ENGINE.store(engine, Ordering::Relaxed);
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
        remember_the_shape(window, shape);
        take_the_window_in_hand(app);
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
        remember_the_shape(0, (0, 0));
        give_the_window_back(app);
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
/// picture's shape rather than the picture the window's.
///
/// This is the tidying-up, not the mechanism. A window being dragged is
/// held to shape while it is dragged, before the resize happens, which is
/// the only way that is smooth; see `the_drag_keeps_the_shape`. What is
/// left for here is everything that resizes a window without dragging it:
/// the picture arriving, the screen being given back, the system putting
/// the window against an edge.
///
/// Only the height moves. Both would fight whichever edge is being
/// dragged, and a window that resists in two directions at once cannot
/// be resized at all.
pub fn hold_the_shape(app: &AppHandle) {
    if DRAGGED.load(Ordering::Relaxed) {
        return;
    }
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

    let engine = crate::floating::window_of(process, crate::floating::Looked::Fresh)?;
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
    let Some(held) = taken(app) else {
        return;
    };
    if !alive(&held) {
        return;
    }
    let Some(home) = home_window(app) else {
        return;
    };
    lay_it_out(home, held.window as windows_sys::Win32::Foundation::HWND);
}

/// The one place a session is laid out, and the only one.
///
/// Nothing of the toolkit and nothing that waits: this runs on every step
/// of a drag of our window, called from that window's own message
/// handler. Asking the toolkit where a window is and putting another one
/// somewhere else are trips through its event queue, and doing that a
/// hundred times a second is most of what made resizing a session judder.
#[cfg(windows)]
fn lay_it_out(
    home: windows_sys::Win32::Foundation::HWND,
    engine: windows_sys::Win32::Foundation::HWND,
) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HWND_TOP, IsIconic, IsWindowVisible, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
        SWP_NOCOPYBITS, SWP_SHOWWINDOW, SetWindowPos,
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
    let dragged = DRAGGED.load(Ordering::Relaxed);

    let started = std::time::Instant::now();
    // SAFETY: the engine's window is one we have already taken in hand.
    //
    // Shown without being activated: asked to show itself the ordinary
    // way, it would take the front, and this runs every second.
    //
    // Nothing of what was drawn is carried over to the new size. The
    // system would otherwise copy a corner of the old picture into the
    // new frame and leave it there until the engine draws again, which
    // is a torn image on every step of a resize.
    //
    // Handed over rather than waited for while a hand is dragging. That
    // window belongs to another program, so moving it the ordinary way
    // means standing still until that program has answered, and it is
    // busy decoding and drawing sixty pictures a second. Once at the end
    // of the drag nobody notices; a hundred times while the hand moves,
    // it is our own window that stops following the mouse.
    //
    // Only while dragging. Handed over, the picture arrives a frame
    // behind, which is invisible under a moving hand and would not be
    // when the hand stops: the last word is always the ordinary way,
    // said from the end of the drag.
    unsafe {
        SetWindowPos(
            engine,
            HWND_TOP,
            corner.x,
            corner.y,
            width,
            height,
            SWP_NOACTIVATE
                | SWP_SHOWWINDOW
                | SWP_NOCOPYBITS
                | if dragged { SWP_ASYNCWINDOWPOS } else { 0 },
        )
    };
    let laid = started.elapsed();

    // Not while a hand is dragging. Giving a window a shape costs a
    // shape built, a shape handed over and a window told to think again,
    // and the two corners it buys are two corners nobody is looking at
    // during a drag. Done once, when the hand lets go.
    let shaped = std::time::Instant::now();
    if !dragged {
        round_the_bottom(home, engine, width, height);
    }
    let shaped = shaped.elapsed();

    let buttoned = std::time::Instant::now();
    crate::floating::lay_the_button((corner.x, corner.y, corner.x + width, corner.y + height));
    let buttoned = buttoned.elapsed();

    if dragged {
        Cost::add(&LAYING, laid + shaped + buttoned);
        Cost::add(&PICTURE, laid);
        Cost::add(&BUTTON, buttoned);
    }
}

/// What one part of a resize costs, counted while a hand is dragging and
/// said out loud when it lets go.
///
/// Kept because there is no other way to know. Everything a resize moves
/// belongs to somebody else — the system, another program, a web view —
/// and which of them is slow cannot be guessed at from the outside; it
/// has been guessed at twice already, wrongly both times. One line at the
/// end of a drag settles it, and costs nothing until somebody drags.
struct Cost {
    times: std::sync::atomic::AtomicU32,
    total: std::sync::atomic::AtomicU64,
    worst: std::sync::atomic::AtomicU64,
}

impl Cost {
    const fn new() -> Self {
        Self {
            times: std::sync::atomic::AtomicU32::new(0),
            total: std::sync::atomic::AtomicU64::new(0),
            worst: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn add(what: &Cost, took: std::time::Duration) {
        let micros = took.as_micros() as u64;
        what.times.fetch_add(1, Ordering::Relaxed);
        what.total.fetch_add(micros, Ordering::Relaxed);
        what.worst.fetch_max(micros, Ordering::Relaxed);
    }

    fn forget(what: &Cost) {
        what.times.store(0, Ordering::Relaxed);
        what.total.store(0, Ordering::Relaxed);
        what.worst.store(0, Ordering::Relaxed);
    }

    /// Total and worst of it, in milliseconds, and how many times.
    fn told(what: &Cost) -> (u32, f64, f64) {
        (
            what.times.load(Ordering::Relaxed),
            what.total.load(Ordering::Relaxed) as f64 / 1000.0,
            what.worst.load(Ordering::Relaxed) as f64 / 1000.0,
        )
    }
}

/// Laying the whole session out: the picture, its shape and the button.
static LAYING: Cost = Cost::new();
/// Of that, moving the picture, which belongs to the engine's own program.
static PICTURE: Cost = Cost::new();
/// Of that, moving the floating button, which is ours.
static BUTTON: Cost = Cost::new();
/// What the system and the toolkit do with the resize before we get to
/// it: our own window moved, and the web view under the picture told to
/// take the new size.
static SYSTEM: Cost = Cost::new();

/// Starts counting, a drag having begun.
fn count_the_drag() {
    for what in [&LAYING, &PICTURE, &BUTTON, &SYSTEM] {
        Cost::forget(what);
    }
}

/// Says what the drag cost, and where.
fn tell_the_drag() {
    let (steps, laying, worst) = Cost::told(&LAYING);
    if steps == 0 {
        return;
    }
    let (_, picture, picture_worst) = Cost::told(&PICTURE);
    let (_, button, _) = Cost::told(&BUTTON);
    let (_, system, system_worst) = Cost::told(&SYSTEM);
    crate::journal::note(&format!(
        "redimensionnement : {steps} pas ; poser {laying:.0} ms (pire {worst:.1}), \
         dont image {picture:.0} ms (pire {picture_worst:.1}) et bouton {button:.0} ms ; \
         système et vue web {system:.0} ms (pire {system_worst:.1})"
    ));
}

/// Rounds the two bottom corners of the picture, to the curve the system
/// gives our own window.
///
/// The picture is a window of its own and a window of its own is a plain
/// rectangle, so a rounded frame showed square corners inside it. Only
/// the bottom two: the top of the picture sits under the title bar, where
/// the frame is straight.
///
/// Applied only when the size changes. A window given a shape is redrawn
/// on the spot, and doing that once a second on a picture would show.
#[cfg(windows)]
fn round_the_bottom(
    home: windows_sys::Win32::Foundation::HWND,
    engine: windows_sys::Win32::Foundation::HWND,
    width: i32,
    height: i32,
) {
    use windows_sys::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
    };
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let laid = (i64::from(width) << 32) | i64::from(height) & 0xFFFF_FFFF;
    if LAID.swap(laid, Ordering::Relaxed) == laid || width <= 0 || height <= 0 {
        return;
    }
    // SAFETY: our own window.
    let dpi = unsafe { GetDpiForWindow(home) };
    let round = (CORNER * dpi.max(1) as i32 / 96).min(height);

    // SAFETY: both are ours until the system takes the combined one.
    unsafe {
        // The rectangle is given one more pixel each way: a shape is cut
        // exclusive of its right and bottom edge, and a picture short of
        // its last row is a picture with a line missing.
        let shape = CreateRoundRectRgn(0, 0, width + 1, height + 1, round * 2, round * 2);
        let square = CreateRectRgn(0, 0, width, height - round);
        CombineRgn(shape, shape, square, RGN_OR as i32);
        DeleteObject(square.into());
        // The system owns the shape from here and frees it itself. Not
        // asked to redraw on the spot: this happens on every step of a
        // resize, and the engine is drawing sixty times a second anyway.
        SetWindowRgn(engine, shape, 0);
    }
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

    // Handing the front to the picture is what dims our title bar, and
    // the system only asks about it while it is doing so. Said again
    // straight after, so the answer is given with the front already
    // where it was being put.
    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(home) = home_window(&asked) {
            light_the_bar(home);
        }
    });
}

/// Makes our window behave as the one window a session is, and steps in
/// front of its messages for as long as the picture is in it.
///
/// Two windows laid on one another are not one window until the system is
/// told so, and this is where it is told: the title bar is lit, and from
/// here on the window answers the three questions the system asks before
/// acting rather than after. Those three are the whole of it, because
/// before is the only moment any of them can be answered.
///
/// Stepping in front of a window's messages is done from the thread that
/// draws it, and none of the callers here is that thread: one drives the
/// session, the other watches it. Handed over rather than done on the
/// spot.
#[cfg(windows)]
fn take_the_window_in_hand(app: &AppHandle) {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;

    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(home) = home_window(&asked) else {
            return;
        };
        // SAFETY: our own window, from the thread that owns it, and the
        // handler outlives the subclass: it is a plain function of this
        // program.
        unsafe { SetWindowSubclass(home, Some(lit), LIT, 0) };
        light_the_bar(home);
    });
}

/// Puts all of that back the way it was, the session being over.
#[cfg(windows)]
fn give_the_window_back(app: &AppHandle) {
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

/// Says out loud that our window is active, and has its title bar
/// redrawn.
///
/// Needed because the handler below only answers a question, and the
/// question is asked once, when the front is taken away. The picture
/// takes the front at the very moment the session opens, which is before
/// there is a picture to know about and therefore before the handler is
/// there to answer: the bar was drawn dim and stayed dim until something
/// else made the system ask again. Clicking the title bar was that
/// something else.
#[cfg(windows)]
fn light_the_bar(home: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_NCACTIVATE};

    // SAFETY: our own window, from the thread that owns it, and the
    // message is the one the system itself sends to say « active ».
    unsafe { SendMessageW(home, WM_NCACTIVATE, 1, 0) };
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
    _data: usize,
) -> isize {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCACTIVATE, WM_SIZING, WM_WINDOWPOSCHANGED,
    };

    match message {
        // Told to dim, while the front belongs to the picture: what took
        // it is our own picture inside our own window, so this window is
        // still the one being used. Answered « active ».
        //
        // Only the picture counts, and not merely anything of ours. Read
        // any wider, this would keep the bar lit after switching to
        // another program.
        WM_NCACTIVATE if wparam == 0 && the_picture_is_at_the_front() => {
            // SAFETY: the arguments the system handed in, with one
            // boolean turned around.
            unsafe { DefSubclassProc(window, message, 1, lparam) }
        }
        // A window being dragged by a corner, before it is resized: the
        // system offers the rectangle it is about to take, and takes
        // back whatever is written there.
        //
        // This is the only place the shape can be held without it
        // costing anything. Corrected afterwards instead, every step of
        // the drag resized the window twice, and every resize is a
        // message to the engine's own process and a swap chain rebuilt
        // there. That was the whole of the stutter.
        WM_SIZING => {
            // SAFETY: for this message the system passes a rectangle of
            // ours to fill in, and it lives for the length of the call.
            let wanted = unsafe { &mut *(lparam as *mut RECT) };
            the_drag_keeps_the_shape(window, wparam as u32, wanted);
            1
        }
        WM_ENTERSIZEMOVE => {
            DRAGGED.store(true, Ordering::Relaxed);
            count_the_drag();
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            DRAGGED.store(false, Ordering::Relaxed);
            tell_the_drag();
            // SAFETY: the arguments the system handed in, untouched.
            let answer = unsafe { DefSubclassProc(window, message, wparam, lparam) };
            // The shape was left alone while the hand was moving; the
            // hand has stopped.
            let engine = ENGINE.load(Ordering::Relaxed) as windows_sys::Win32::Foundation::HWND;
            if !engine.is_null() {
                lay_it_out(window, engine);
            }
            answer
        }
        // Our window has just moved, been resized, shown or hidden: the
        // picture and the button go with it, here and now.
        //
        // Here rather than through the toolkit's own account of the same
        // thing. That account arrives a queue later, and a picture a
        // queue behind the window it lives in is a picture that visibly
        // lags the frame during a drag.
        WM_WINDOWPOSCHANGED => {
            // SAFETY: the arguments the system handed in, untouched. Let
            // the system finish moving this window before the picture is
            // laid on what it has become. What that costs is counted:
            // most of it is not ours, our own window being carried by the
            // toolkit and a web view under the picture being told to take
            // the new size, and knowing which is which is the whole point
            // of counting.
            let waited = std::time::Instant::now();
            let answer = unsafe { DefSubclassProc(window, message, wparam, lparam) };
            if DRAGGED.load(Ordering::Relaxed) {
                Cost::add(&SYSTEM, waited.elapsed());
            }
            let engine = ENGINE.load(Ordering::Relaxed) as windows_sys::Win32::Foundation::HWND;
            if !engine.is_null() {
                lay_it_out(window, engine);
            }
            answer
        }
        // SAFETY: same.
        _ => unsafe { DefSubclassProc(window, message, wparam, lparam) },
    }
}

/// Holds the rectangle a drag is about to take to the shape of the
/// picture.
///
/// Which side moves depends on which edge is being dragged: a corner or a
/// vertical edge sets the height from the width, a horizontal edge does
/// the opposite. Anything else would fight the hand.
#[cfg(windows)]
fn the_drag_keeps_the_shape(
    window: windows_sys::Win32::Foundation::HWND,
    edge: u32,
    wanted: &mut windows_sys::Win32::Foundation::RECT,
) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, WMSZ_BOTTOM, WMSZ_TOP, WMSZ_TOPLEFT, WMSZ_TOPRIGHT,
    };

    let (wide, high) = shape();
    if wide <= 0 || high <= 0 {
        return;
    }

    // What the frame costs, so the shape is held on the inside of the
    // window and not on its outside: the title bar is not part of the
    // picture.
    let mut outside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut inside = outside;
    // SAFETY: our own window, and both rectangles are ours.
    if unsafe { GetWindowRect(window, &mut outside) } == 0
        || unsafe { GetClientRect(window, &mut inside) } == 0
    {
        return;
    }
    let frame = (
        (outside.right - outside.left) - (inside.right - inside.left),
        (outside.bottom - outside.top) - (inside.bottom - inside.top),
    );

    if edge == WMSZ_TOP || edge == WMSZ_BOTTOM {
        let height = (wanted.bottom - wanted.top - frame.1).max(1);
        let width = (i64::from(height) * i64::from(wide) / i64::from(high)) as i32 + frame.0;
        wanted.right = wanted.left + width;
        return;
    }

    let width = (wanted.right - wanted.left - frame.0).max(1);
    let height = (i64::from(width) * i64::from(high) / i64::from(wide)) as i32 + frame.1;
    // Dragged from the top: the bottom stays where it is and the top
    // moves, or the window would walk down the screen under the hand.
    if edge == WMSZ_TOPLEFT || edge == WMSZ_TOPRIGHT {
        wanted.top = wanted.bottom - height;
    } else {
        wanted.bottom = wanted.top + height;
    }
}

/// Whether the window at the front is the picture.
#[cfg(windows)]
fn the_picture_is_at_the_front() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let engine = ENGINE.load(Ordering::Relaxed);
    // SAFETY: no argument, and a null answer is one of the answers.
    engine != 0 && unsafe { GetForegroundWindow() } as isize == engine
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
fn take_the_window_in_hand(_app: &AppHandle) {}

#[cfg(not(windows))]
fn give_the_window_back(_app: &AppHandle) {}

#[cfg(not(windows))]
pub fn fit(_app: &AppHandle) {}
