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

/// The player the picture belongs to, next to it.
///
/// The handler cannot take the lock the full answer lives under, and a
/// window number alone is not an answer: the system hands numbers back
/// out when their window goes, so the number alone can name a stranger's
/// window minutes after the engine died. Owner checked against this
/// before the number is acted on.
static PLAYER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Set while the person is dragging an edge of our window.
///
/// The shape is held during the drag, before each resize; tidying it up
/// afterwards as well would resize the window a second time for nothing.
static DRAGGED: AtomicBool = AtomicBool::new(false);

/// Size the picture was last given a shape for, so it is only reshaped
/// when it really changes size.
static LAID: AtomicI64 = AtomicI64::new(0);

/// Whether that shape was the square one, for the same reason: covering
/// the screen and coming back are the two moves that change it without
/// necessarily changing anything else.
static SQUARED: AtomicBool = AtomicBool::new(false);

/// Radius the system rounds a window's corners by, in page pixels.
///
/// Windows has never offered it as a number to ask for; this is the one
/// it uses for an ordinary window, and it is scaled to the screen the
/// window is on.
const CORNER: i32 = 8;

/// One drawn frame, near enough, on the screens this product is for.
const A_FRAME: std::time::Duration = std::time::Duration::from_millis(16);

/// Width and height of the picture, as the handler reads them.
fn shape() -> (i32, i32) {
    let both = SHAPE.load(Ordering::Relaxed);
    ((both >> 32) as i32, both as i32)
}

fn remember_the_shape(engine: isize, shape: (i32, i32), process: u32) {
    // The next picture is a different window and has never been given a
    // shape, whatever size this one was left at.
    LAID.store(0, Ordering::Relaxed);
    SQUARED.store(false, Ordering::Relaxed);
    // A session can end with a hand still on an edge, or in the middle
    // of a window being carried to its new size, and nothing else would
    // ever put these down: left standing, they silently switch off
    // holding the window to the picture's shape for the rest of the
    // program's life.
    DRAGGED.store(false, Ordering::Relaxed);
    #[cfg(windows)]
    GROWS.store(false, Ordering::Relaxed);
    SHAPE.store(
        (i64::from(shape.0) << 32) | i64::from(shape.1) & 0xFFFF_FFFF,
        Ordering::Relaxed,
    );
    PLAYER.store(process, Ordering::Relaxed);
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
    // The lock is held from the reading to the writing. Two callers race
    // here every second a session opens, the watch and the opening's own
    // thread; each reading « nothing held », both took the frame away,
    // and the second one read the window's size AFTER the first had laid
    // it in ours, writing the window's incidental size down as the shape
    // of the picture. The session then held its window to the wrong
    // shape for as long as it lasted.
    {
        let mut held = state.held.lock().expect("image tenue");
        let already = held.filter(|held| held.process == process && alive(held));
        if already.is_none() {
            let Some((window, shape)) = take_the_frame_away(app, process) else {
                return false;
            };
            *held = Some(Held {
                process,
                window,
                shape,
            });
            // The shape is worth writing down: it is the size the far
            // computer's picture actually arrives at, which is the
            // answer to what a black band on screen means, and it exists
            // nowhere else once the window has been laid in ours.
            crate::journal::note(&format!(
                "image du lecteur {process} posée dans la fenêtre de ZyrDesk, en {}x{}",
                shape.0, shape.1
            ));
            remember_the_shape(window, shape, process);
            drop(held);
            take_the_window_in_hand(app);
            hold_the_shape(app);
        }
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
        remember_the_shape(0, (0, 0), 0);
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
    if on_the_move() {
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
    let Ok(width) = i32::try_from(inside.width) else {
        return;
    };
    let wanted = across(width, wide, high);
    if wanted <= 0 || inside.height.abs_diff(wanted as u32) <= ROUNDING {
        return;
    }
    let _ = window.set_size(tauri::PhysicalSize::new(inside.width, wanted as u32));
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
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

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
    let Some((corner, width, height)) = the_inside_of(home) else {
        return;
    };
    lay_on(home, engine, corner, width, height);
}

/// Where our window's inside is on the screen, and how big it is.
#[cfg(windows)]
fn the_inside_of(home: windows_sys::Win32::Foundation::HWND) -> Option<((i32, i32), i32, i32)> {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut corner = POINT { x: 0, y: 0 };
    // SAFETY: the window exists and both slots are ours.
    if unsafe { GetClientRect(home, &mut inside) } == 0 {
        return None;
    }
    // SAFETY: same window, and the point is ours; it goes in as a
    // position inside the window and comes back as one on the screen.
    unsafe { ClientToScreen(home, &mut corner) };
    Some((
        (corner.x, corner.y),
        inside.right - inside.left,
        inside.bottom - inside.top,
    ))
}

/// Lays the picture on that inside, wherever our window is about to put
/// it.
#[cfg(windows)]
fn lay_on(
    home: windows_sys::Win32::Foundation::HWND,
    engine: windows_sys::Win32::Foundation::HWND,
    corner: (i32, i32),
    width: i32,
    height: i32,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOP, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        SetWindowPos,
    };

    // What is counted, and what is put off until the window settles, are
    // two different questions: a drag is counted and told at the end,
    // while both a drag and an order being played put off the shape.
    let dragged = DRAGGED.load(Ordering::Relaxed);
    let moving = on_the_move();

    // What the picture already is. A window carried across the desk
    // keeps its size the whole way, and asking for a size it already has
    // is asking for work nobody needs.
    let stands = where_it_stands(engine);
    let same_size = stands
        .is_some_and(|(left, top, right, bottom)| (right - left, bottom - top) == (width, height));
    let same_place = stands.is_some_and(|(left, top, ..)| (left, top) == corner);

    let started = std::time::Instant::now();
    // SAFETY: the engine's window is one we have already taken in hand.
    //
    // Shown without being activated: asked to show itself the ordinary
    // way, it would take the front, and this runs every second.
    //
    // Waited for, and not handed over. That window belongs to another
    // program, so this stands still until that program has answered; it
    // is what keeps the picture and the frame that carries it in the
    // same step, and the answer is quick now that a resize no longer
    // makes the engine rebuild everything it draws with.
    //
    // Nothing of what was drawn is carried over to a NEW SIZE: the
    // system would otherwise copy a corner of the old picture into the
    // new frame and leave it there until the engine draws again, which
    // is a torn image on every step of a resize.
    //
    // But only to a new size. Asked of a window that is merely being
    // carried, that same thing throws away a picture that was perfectly
    // good and has the engine paint it again, one step at a time, all
    // the way across the desk: the edge the window was heading for
    // flickered white for the length of the carry, which is the page
    // behind showing through in the moment between the throwing away and
    // the painting.
    let moved_only = if same_size {
        SWP_NOSIZE
    } else {
        SWP_NOCOPYBITS
    };
    let stays = if same_place { SWP_NOMOVE } else { 0 };
    // Already exactly there: asking again costs a wait on another
    // program for nothing. The window it belongs to is only ever put
    // right once per move now, before ours moves, so the laying that
    // follows the move has nothing left to do.
    if !(same_size && same_place) {
        // SAFETY: the engine's window is one we have already taken in
        // hand.
        unsafe {
            SetWindowPos(
                engine,
                HWND_TOP,
                corner.0,
                corner.1,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | moved_only | stays,
            )
        };
    }
    let laid = started.elapsed();
    // Laying the picture is meant to happen inside the very frame our
    // own window changes in, which is what makes the two look like one.
    // Longer than a frame and they are seen apart, and the wait is not
    // ours: it is the player answering. Said only when it happens, and
    // never while the window is moving, where every step is counted and
    // told in one line at the end instead.
    if !moving && laid > A_FRAME {
        WAS_BUSY.store(true, Ordering::Relaxed);
        crate::journal::note(&format!(
            "image posée en {:.0} ms, soit plus d'une image : le lecteur a tardé à répondre",
            laid.as_secs_f64() * 1000.0
        ));
    } else if !moving {
        // Answered inside a frame, so the player is past its start-up
        // and will hear what size its window is.
        say_the_size_again(engine, (width, height));
    }
    tell_the_gap(
        where_it_stands(engine),
        (corner.0, corner.1, corner.0 + width, corner.1 + height),
    );

    // Not while the window is moving. Giving a window a shape costs a
    // shape built, a shape handed over and a window told to think again,
    // and the two corners it buys are two corners nobody is looking at
    // while it moves. Done once, when it settles.
    let shaped = std::time::Instant::now();
    if !moving {
        round_the_bottom(home, engine, width, height);
    }
    let shaped = shaped.elapsed();

    let buttoned = std::time::Instant::now();
    crate::floating::lay_the_button((corner.0, corner.1, corner.0 + width, corner.1 + height));
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
/// belongs to somebody else (the system, another program, a web view),
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

/// Whether the drag under way is resizing the window rather than only
/// moving it. Told apart by a change of size flowing through while the
/// hand is down: a plain move carries none.
static RESIZED: AtomicBool = AtomicBool::new(false);

/// Which edges of the window the hand is holding, over the whole drag.
///
/// A hand grabs one edge or one corner when the drag begins and holds it
/// until the drag ends, so this is a fact about the drag and not about
/// the step. It is gathered rather than decided: an edge the system has
/// once been seen to move is an edge under the hand, and one step is
/// enough to know it for good. A hand on a corner that begins by moving
/// straight sideways only shows the second edge a few steps in, which is
/// exactly when it starts to matter.
///
/// Read from the step instead, it was read wrongly: the two sides of a
/// window being dragged by the corner move by nearly the same amount, and
/// whichever of them counted as « the one being pulled » changed from
/// step to step. The other side is worked out from that one, and the two
/// answers are far apart, so the picture jumped back and forth under a
/// hand that was moving perfectly steadily.
static HELD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The left or the right edge.
const A_SIDE: u8 = 1;
/// The top or the bottom edge.
const TOP_OR_BOTTOM: u8 = 2;

/// How many times the window turned around during the drag: it was
/// growing and began to shrink, or the other way about.
///
/// A hand pulling steadily outwards turns around no times. Anything else
/// is the picture moving against the hand, which is what « it shivers »
/// looks like from in here, and the one number worth reading back off a
/// journal to know whether it still does.
static TURNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The width the last step of the drag settled on, and which way it was
/// going: below, at rest, above. Both empty outside a drag.
static WIDTH_WAS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static WIDTH_WENT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Starts counting, a drag having begun.
fn count_the_drag() {
    RESIZED.store(false, Ordering::Relaxed);
    HELD.store(0, Ordering::Relaxed);
    TURNS.store(0, Ordering::Relaxed);
    WIDTH_WAS.store(0, Ordering::Relaxed);
    WIDTH_WENT.store(0, Ordering::Relaxed);
    for what in [&LAYING, &PICTURE, &BUTTON, &SYSTEM] {
        Cost::forget(what);
    }
}

/// Notes which way this step took the window, and whether that is a
/// change of mind.
fn count_the_step(width: i32) {
    let was = WIDTH_WAS.swap(width, Ordering::Relaxed);
    if was == 0 {
        return;
    }
    let way = (width - was).signum();
    if way == 0 {
        return;
    }
    let went = WIDTH_WENT.swap(way, Ordering::Relaxed);
    if went != 0 && went != way {
        TURNS.fetch_add(1, Ordering::Relaxed);
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
    // A hand on the title bar and a hand on an edge are the same gesture
    // to the system, and cost the same work here. Only one of them is a
    // resize, and this line is read to answer a question about resizing:
    // it says which it was rather than calling both by the louder name.
    if !RESIZED.load(Ordering::Relaxed) {
        crate::journal::note(&format!(
            "déplacement : {steps} pas ; poser {laying:.0} ms (pire {worst:.1}), \
             dont image {picture:.0} ms (pire {picture_worst:.1}) et bouton {button:.0} ms ; \
             système et vue web {system:.0} ms (pire {system_worst:.1})"
        ));
        return;
    }
    let held = match HELD.load(Ordering::Relaxed) {
        A_SIDE => "un côté",
        TOP_OR_BOTTOM => "un bord horizontal",
        _ => "un coin",
    };
    let turns = TURNS.load(Ordering::Relaxed);
    crate::journal::note(&format!(
        "redimensionnement par {held} : {steps} pas, {turns} changements de sens ; \
         poser {laying:.0} ms (pire {worst:.1}), \
         dont image {picture:.0} ms (pire {picture_worst:.1}) et bouton {button:.0} ms ; \
         système et vue web {system:.0} ms (pire {system_worst:.1})"
    ));
}

/// Rounds the two bottom corners of the picture, to the curve the system
/// gives our own window, and squares them again when it stops giving it.
///
/// The picture is a window of its own and a window of its own is a plain
/// rectangle, so a rounded frame showed square corners inside it. Only
/// the bottom two: the top of the picture sits under the title bar, where
/// the frame is straight.
///
/// The cut runs along the frame the system actually draws, asked of the
/// system itself. Cut along a curve of our own, anchored on the picture,
/// it fell a pixel or two short of the frame's curve, and what showed in
/// between was the page behind the picture: a pale bite in each corner.
/// Anchored on the drawn frame, the picture reaches the corner, and the
/// pixels our cut can still miss land on the frame's own border, where a
/// window's content ends anyway.
///
/// And only while the frame really is round. A window covering the screen
/// or pushed out to its edges has square corners, which is the system's
/// own rule and not a choice of ours; rounding the picture there took two
/// bites out of the far computer's screen with nothing to justify them.
///
/// Applied only when something changes. A window given a shape is redrawn
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
    let square = the_frame_is_square(home);
    // Both are put down whatever happens: they are what « unchanged »
    // means next time, and one left behind would answer for a window that
    // is no longer there.
    let same_size = LAID.swap(laid, Ordering::Relaxed) == laid;
    let same_frame = SQUARED.swap(square, Ordering::Relaxed) == square;
    if (same_size && same_frame) || width <= 0 || height <= 0 {
        return;
    }
    if square {
        // SAFETY: a window this program took in hand; no shape means the
        // whole rectangle, which is what a window is without one.
        let taken = unsafe { SetWindowRgn(engine, std::ptr::null_mut(), 0) };
        tell_the_cut(engine, "retirée", taken, (width, height));
        return;
    }

    // SAFETY: our own window.
    let dpi = unsafe { GetDpiForWindow(home) };
    let round = CORNER * dpi.max(96) as i32 / 96;

    // How thick the border the system draws around the window is, which
    // is the whole of the difference between the two curves: the outer
    // one, which the frame turns on, and the inner one, which is where
    // the content of a window stops. The picture is content, so it is
    // the inner one it is cut against.
    //
    // Cut against the outer one, the picture kept the pixels that lie
    // between the two, and those pixels are where the system draws the
    // border itself. A window's own accent-coloured border went dark in
    // both bottom corners for the length of a session, since what was
    // painted over it was the far computer's picture.
    //
    // Without an answer, no border and the picture's own rectangle: a
    // cut a pixel out is still the curve, and the alternative is a
    // square corner sticking out of a round one.
    let border = the_drawn_frame(home, engine)
        .map(|(left, _, right, bottom)| (-left).max(right - width).max(bottom - height).max(0))
        .unwrap_or(0);
    let round = (round - border).clamp(0, height.min(width));
    tell_the_corner(width, height, border, round);

    // SAFETY: both are ours until the system takes the combined one.
    unsafe {
        // The picture's own rectangle, which is the inside of our window
        // to the pixel: the two are laid on one another by `lay_it_out`.
        //
        // One more pixel each way, because a shape is cut short of the
        // edges it is given. Cut on the picture's own numbers the shape
        // came back one row shy of the bottom, measured, and that row
        // is the page behind the picture showing through as a pale line
        // along the whole width. Anything past the picture is clipped by
        // the picture, so asking for one more costs nothing.
        let shape = CreateRoundRectRgn(0, 0, width + 1, height + 1, round * 2, round * 2);
        if shape.is_null() {
            return;
        }
        // Everything above the arcs stays a plain rectangle: the top of
        // the picture sits under the title bar, where the frame is
        // straight.
        let straight = CreateRectRgn(0, 0, width + 1, height - round);
        if !straight.is_null() {
            CombineRgn(shape, shape, straight, RGN_OR);
            DeleteObject(straight);
        }
        // The system owns the shape from here and frees it itself, but
        // only once it has taken it: refused, it is still ours to free.
        // Not asked to redraw on the spot: this happens on every step of
        // a resize, and the engine is drawing sixty times a second anyway.
        let taken = SetWindowRgn(engine, shape, 0);
        if taken == 0 {
            DeleteObject(shape);
        }
        tell_the_cut(engine, "posée", taken, (width, height));
    }
}

/// The last difference between what the picture was asked to be and what
/// it turned out to be, so a change of it can be written down and
/// nothing else.
#[cfg(windows)]
static GAP: AtomicI64 = AtomicI64::new(0);

/// Says when the picture did not take the size it was given.
///
/// It is told to cover the whole inside of our window, so anything it
/// leaves uncovered is a strip of the page behind it showing along an
/// edge: a pale line under the picture, where there should be nothing
/// between the picture and the frame.
///
/// Asked of the system rather than assumed. That window belongs to
/// another program, and a program may answer a resize with a size of its
/// own choosing: a smallest size, a step it rounds to, or the size the
/// system hands it when the two of us do not measure a screen the same
/// way. Which of those it is cannot be read off a screenshot, and the
/// difference is the whole of what a pale line is.
#[cfg(windows)]
fn tell_the_gap(stood: Option<(i32, i32, i32, i32)>, asked: (i32, i32, i32, i32)) {
    let Some(got) = stood else {
        return;
    };
    if got == asked {
        return;
    }
    // The whole rectangle and not merely its size. A picture that is the
    // right size in the wrong place leaves a strip along one edge just
    // as surely as one that is too small, and only the two rectangles
    // side by side say which of the two it is.
    let off = (
        got.0 - asked.0,
        got.1 - asked.1,
        got.2 - asked.2,
        got.3 - asked.3,
    );
    let both = (i64::from(off.0 - off.2) << 32) | i64::from(off.1 - off.3) & 0xFFFF_FFFF;
    if GAP.swap(both, Ordering::Relaxed) == both {
        return;
    }
    crate::journal::note(&format!(
        "image demandée en {asked:?}, posée en {got:?} : écart de {off:?} sur les quatre bords"
    ));
}

/// The border and radius the last cut used, so a change of them can be
/// written down and nothing else.
#[cfg(windows)]
static CUT: AtomicI64 = AtomicI64::new(-1);

/// Says what the corners of the picture are being cut with.
///
/// Two numbers, written once each time they change. « The corner is not
/// quite right » is the one report that cannot be chased from a
/// screenshot: a border of one pixel and a border of two put the curve
/// in different places, and which of the two a screen has is not
/// something to guess at.
#[cfg(windows)]
fn tell_the_corner(width: i32, height: i32, border: i32, round: i32) {
    let both = (i64::from(border) << 32) | i64::from(round) & 0xFFFF_FFFF;
    if CUT.swap(both, Ordering::Relaxed) != both {
        crate::journal::note(&format!(
            "coins de l'image : image {width}x{height}, bordure de {border} px, \
             rayon de {round} px"
        ));
    }
}

/// The frame the system draws for our window, in the picture's own
/// coordinates.
///
/// Not the rectangle the system reserves for it: that one is wider by the
/// invisible bands a resize can be grabbed in, and no curve runs there.
/// The corners turn on the drawn frame, so that is the one the picture is
/// cut against.
#[cfg(windows)]
fn the_drawn_frame(
    home: windows_sys::Win32::Foundation::HWND,
    engine: windows_sys::Win32::Foundation::HWND,
) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let drawn = the_drawn_frame_of(home)?;
    let mut picture = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window this program took in hand, and the rectangle is
    // ours.
    if unsafe { GetWindowRect(engine, &mut picture) } == 0 {
        return None;
    }
    Some((
        drawn.0 - picture.left,
        drawn.1 - picture.top,
        drawn.2 - picture.left,
        drawn.3 - picture.top,
    ))
}

/// Takes the picture's shape off for the length of a drag.
///
/// The shape costs too much to redo on every step, so it is only put
/// back when the hand stops. Left on during the drag, it is the shape of
/// the size the window had when the drag began: a window growing under
/// it is clipped to where it used to end, and the strip of new window
/// beyond that shows the page behind the picture. Sixty times a second,
/// that is the picture flickering along its own edges.
#[cfg(windows)]
fn let_the_corners_go(engine: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::SetWindowRgn;

    // SAFETY: a window this program took in hand; no shape means the
    // whole rectangle, which is what a window is without one.
    unsafe { SetWindowRgn(engine, std::ptr::null_mut(), 0) };
    // Forgotten, so the shape is put back when the hand stops even if
    // the window ends the drag at exactly the size it started at.
    LAID.store(0, Ordering::Relaxed);
}

/// Whether the system is drawing our window with square corners.
///
/// It rounds an ordinary window and squares one that covers the screen or
/// has been pushed out to its edges. There is no number to ask for, so
/// the two cases are read where they show: a window at its full size, and
/// a window whose title bar has been taken away, which is what covering
/// the screen does to it.
#[cfg(windows)]
fn the_frame_is_square(home: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetWindowLongPtrW, IsZoomed, WS_CAPTION,
    };

    // SAFETY: our own window, and both calls only read its state.
    unsafe { IsZoomed(home) != 0 || GetWindowLongPtrW(home, GWL_STYLE) & WS_CAPTION as isize == 0 }
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
/// here on the window answers what the system asks before acting rather
/// than after, which is the only moment an answer changes anything:
/// whether it is still the active window, and what size a drag is about
/// to give it.
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
        offer_a_picture_of_the_session(home, true);
        round_the_window(home, true);
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
        offer_a_picture_of_the_session(home, false);
        round_the_window(home, false);
    });
}

/// Message our window sends itself to have its title bar drawn the way
/// the front really stands.
///
/// The system asks whether the window is still active at the moment it
/// is taking the front away, and what it is giving it to is not settled
/// yet: read there, the front was sometimes still ours and sometimes
/// already a stranger's, whatever was really happening. So the question
/// is asked twice, once on the spot for the drawing that follows
/// immediately, and once more through a message posted to ourselves,
/// which the system delivers only after it has finished. The second
/// answer is the true one, and it costs a redraw of a title bar.
#[cfg(windows)]
const BAR: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Has the title bar drawn the way the front stands right now.
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
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

    // SAFETY: our own window, from the thread that owns it, and the
    // message is one of ours.
    unsafe { SendMessageW(home, BAR, 0, 0) };
}

/// What the title bar was last drawn as, so a change of it can be
/// written down and nothing else.
#[cfg(windows)]
static BAR_LIT: AtomicBool = AtomicBool::new(false);

/// Draws the title bar lit or dim according to who holds the front, and
/// says so in the journal when that changes.
///
/// The whole of « the window looks unused while it is being used » is
/// decided here, so it is the one place worth having in writing: which
/// of the three it was, ours, the player's, or somebody else's.
#[cfg(windows)]
fn draw_the_bar(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_NCACTIVATE;

    let front = who_holds_the_front();
    let lit = front != Front::Elsewhere;
    if BAR_LIT.swap(lit, Ordering::Relaxed) != lit {
        crate::journal::note(&format!(
            "barre de titre {} : le premier plan est {}",
            if lit { "active" } else { "inactive" },
            match front {
                Front::Ours => "à ZyrDesk",
                Front::ThePlayer => "à l'image",
                Front::Elsewhere => "ailleurs",
            }
        ));
    }
    // SAFETY: our own window, from the thread that owns it, and the
    // message is the one the system itself sends to say active or not.
    // Handed straight to the system's own handling: what it is to be
    // drawn as has just been decided, and passing it through ours again
    // would only ask the same question a second time.
    unsafe { DefSubclassProc(window, WM_NCACTIVATE, usize::from(lit), 0) };
}

/// Who the window at the front belongs to.
#[cfg(windows)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Front {
    /// One of ours: the home window, or the floating button.
    Ours,
    /// The player's, which during a session means the picture.
    ThePlayer,
    /// Another program's, or nobody's.
    Elsewhere,
}

/// Whether the window at the front belongs to this session.
///
/// Ours or the player's, since the picture is another program's window
/// and holds the front for most of a session. Read from the system on
/// the spot and never from a lock: this is called from inside the
/// system's own call into our window.
///
/// Ours and not merely the picture's. The floating button is a window of
/// ours too, and so is the home window itself; asked only about the
/// picture, this answered « somebody else » the moment a hand touched
/// the button, and the title bar went dim under a session being used.
#[cfg(windows)]
pub fn who_holds_the_front() -> Front {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: no argument, and a null answer is one of the answers.
    let front = unsafe { GetForegroundWindow() };
    if front.is_null() {
        return Front::Elsewhere;
    }
    let mut owner = 0u32;
    // SAFETY: the window comes from the call above and the slot is ours.
    unsafe { GetWindowThreadProcessId(front, &mut owner) };
    // SAFETY: no argument.
    if owner == unsafe { GetCurrentProcessId() } {
        Front::Ours
    } else if owner == PLAYER.load(Ordering::Relaxed) {
        Front::ThePlayer
    } else {
        Front::Elsewhere
    }
}

/// Asks the compositor to round our window's corners, and takes the ask
/// back at the end of the session.
///
/// Asked once and never taken up again while the session lasts. « Round
/// them if that suits the window » is what this asks, and what suits it
/// is the compositor's own business: a window spread over a screen is
/// not rounded whatever is asked, and comes back rounded when it comes
/// back down. Told to square them and then to round them again, across
/// a maximise, it took the first and not the second, and the window came
/// back down with the flat top of Windows 10. Told once, there is
/// nothing to take back and nothing to miss.
#[cfg(windows)]
fn round_the_window(home: windows_sys::Win32::Foundation::HWND, may: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_ROUND, DwmSetWindowAttribute,
    };

    let how: i32 = if may { DWMWCP_ROUND } else { DWMWCP_DEFAULT };
    // SAFETY: our own window, an attribute made to be set, and the value
    // is ours, of the size the call is told.
    let answer = unsafe {
        DwmSetWindowAttribute(
            home,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const how).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    crate::journal::note(&format!(
        "coins de la fenêtre : {} demandés, le compositeur a répondu {answer:#x}",
        if may {
            "arrondis"
        } else {
            "au choix du système"
        }
    ));
}

/// Says what the picture is really cut to, asked of the system after the
/// cut rather than assumed from what was handed to it.
///
/// Everything the picture shows or fails to show comes down to this one
/// rectangle. Short of the window at the bottom, a strip of the page
/// behind shows through as a pale line; level with the window at the
/// corners, the picture covers the curve the frame turns on and the
/// coloured border with it. Both are what has been reported, and neither
/// can be told from the other without the numbers.
#[cfg(windows)]
fn tell_the_cut(
    engine: windows_sys::Win32::Foundation::HWND,
    what: &str,
    taken: i32,
    asked: (i32, i32),
) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::GetWindowRgnBox;

    let mut box_of = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window this program took in hand, and the rectangle is
    // ours. A window with no shape answers that it has none.
    let kind = unsafe { GetWindowRgnBox(engine, &mut box_of) };
    crate::journal::note(&format!(
        "découpe de l'image {what} ({taken}) sur {}x{} : elle couvre ({}, {}, {}, {}), sorte {kind}",
        asked.0, asked.1, box_of.left, box_of.top, box_of.right, box_of.bottom
    ));
}

/// Tells the player the size of its own window, once more, out loud.
///
/// The player throws away the size changes that reach it while it is
/// still clearing its queue at start-up, and never asks again: its
/// window says one size and what it draws is another, so a strip along
/// the bottom of the picture stays the colour of an empty window for the
/// whole session. Its own log names them, « dropping window event during
/// flush », and the one it drops is ours.
///
/// When to say it again is the whole difficulty, and it is not a delay
/// to guess at: the player is busy for exactly as long as it is busy.
/// But how busy it is can be read, because moving its window waits on
/// it. Still starting up, that wait runs to a quarter of a second; up
/// and running, it answers inside a drawn frame. So this is called on
/// the first laying the player answers quickly and on no earlier one,
/// and it happens once.
///
/// Said by moving the window rather than by saying it, because a size
/// that has not changed is not a size change and reaches nobody: a pixel
/// narrower and then right again, in one go, with nothing drawn between.
///
/// A patch to the engine is what would end this properly. Until then,
/// this costs two calls, once per session.
#[cfg(windows)]
static SIZE_SAID: AtomicBool = AtomicBool::new(false);

/// Whether the player has been seen busy at all yet.
///
/// It answers quickly before it starts building its decoder as well as
/// after, and the journal caught this out: the size was said on that
/// first quick answer, which is earlier than the moment being waited
/// for. So a slow answer has to have been seen first. Busy, then not
/// busy, is a start-up that has finished; not busy from the outset is a
/// start-up that has not begun.
#[cfg(windows)]
static WAS_BUSY: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
fn say_the_size_again(engine: windows_sys::Win32::Foundation::HWND, size: (i32, i32)) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
    };

    if size.0 <= 1
        || size.1 <= 1
        || !WAS_BUSY.load(Ordering::Relaxed)
        || SIZE_SAID.swap(true, Ordering::Relaxed)
    {
        return;
    }
    for said in [(size.0 - 1, size.1 - 1), size] {
        // SAFETY: a window this program took in hand, resized where it
        // stands without being activated.
        unsafe {
            SetWindowPos(
                engine,
                std::ptr::null_mut(),
                0,
                0,
                said.0,
                said.1,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }
    crate::journal::note(&format!(
        "taille de l'image redite au lecteur : {}x{}",
        size.0, size.1
    ));
}

/* ---- Ce que les autres fenêtres montrent de la nôtre ----------------- */

/// Offers the system a picture of the session whenever it wants to show
/// this window somewhere small, or stops offering.
///
/// What Alt+Tab and the taskbar show is a photograph the system takes of
/// a window. It photographs one window, and the session is in another
/// laid over it, so what it got was the home screen underneath: a
/// session shown as the page it is hiding. Told that this window has a
/// picture of its own, the system asks for it instead, which is the two
/// messages the handler answers below.
///
/// Given back at the end of the session, when the home screen really is
/// what this window shows.
#[cfg(windows)]
fn offer_a_picture_of_the_session(home: windows_sys::Win32::Foundation::HWND, may: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_FORCE_ICONIC_REPRESENTATION, DWMWA_HAS_ICONIC_BITMAP, DwmSetWindowAttribute,
    };

    let yes: i32 = i32::from(may);
    for what in [DWMWA_HAS_ICONIC_BITMAP, DWMWA_FORCE_ICONIC_REPRESENTATION] {
        // SAFETY: our own window, an attribute made to be set, and the
        // value is ours, of the size the call is told.
        unsafe {
            DwmSetWindowAttribute(
                home,
                what as u32,
                (&raw const yes).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        };
    }
}

/// Hands the system a picture of the session, at no more than that size.
///
/// Answers false when there is nothing to hand over, and the system then
/// falls back on the photograph it would have taken by itself: the home
/// screen, which is what was shown before any of this and is a poor
/// answer rather than a wrong one. Better that than a black square.
///
/// The picture belongs to another program and is drawn straight on the
/// graphics card, which an ordinary copy of a window does not reach.
/// `PW_RENDERFULLCONTENT` is the one that does, and it is the whole
/// reason this is possible at all.
#[cfg(windows)]
fn hand_over_a_picture(
    home: windows_sys::Win32::Foundation::HWND,
    at_most: (i32, i32),
    live: bool,
) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetIconicLivePreviewBitmap, DwmSetIconicThumbnail};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, HALFTONE, ReleaseDC, SRCCOPY,
        SelectObject, SetStretchBltMode, StretchBlt,
    };
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::PW_RENDERFULLCONTENT;

    let Some(engine) = the_engines_window() else {
        return false;
    };
    let Some((left, top, right, bottom)) = where_it_stands(engine) else {
        return false;
    };
    let (wide, high) = (right - left, bottom - top);
    if wide <= 0 || high <= 0 {
        return false;
    }
    // As large as fits inside what was asked for, at the picture's own
    // shape: a thumbnail of another shape would be the black bands this
    // product exists to be rid of, in miniature.
    let shrunk = at_most.0.min(across(at_most.1, high, wide)).max(1);
    let size = (shrunk, across(shrunk, wide, high).max(1));

    // SAFETY: every object made here is ours, and every one of them is
    // freed on every road out.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let taking = CreateCompatibleDC(screen);
        let holding = CreateCompatibleDC(screen);
        ReleaseDC(std::ptr::null_mut(), screen);
        if taking.is_null() || holding.is_null() {
            DeleteDC(taking);
            DeleteDC(holding);
            return false;
        }
        // Two surfaces: the picture at its own size, and the small one
        // the system asked for. Both are told to be the right way up and
        // with a channel the system reads as « all of it shows », which
        // is what a thumbnail is handed as.
        let whole = plain_surface(taking, (wide, high));
        let small = plain_surface(holding, size);
        let mut done = false;
        if !whole.is_null() && !small.is_null() {
            let was_taking = SelectObject(taking, whole.cast());
            let was_holding = SelectObject(holding, small.cast());
            if PrintWindow(engine, taking, PW_RENDERFULLCONTENT) != 0 {
                SetStretchBltMode(holding, HALFTONE);
                if StretchBlt(
                    holding, 0, 0, size.0, size.1, taking, 0, 0, wide, high, SRCCOPY,
                ) != 0
                {
                    // Handed over before it is freed, and the system
                    // copies what it needs during the call.
                    let given = if live {
                        DwmSetIconicLivePreviewBitmap(home, small.cast(), std::ptr::null(), 0)
                    } else {
                        DwmSetIconicThumbnail(home, small.cast(), 0)
                    };
                    done = given >= 0;
                }
            }
            SelectObject(taking, was_taking);
            SelectObject(holding, was_holding);
        }
        if !whole.is_null() {
            DeleteObject(whole.cast());
        }
        if !small.is_null() {
            DeleteObject(small.cast());
        }
        DeleteDC(taking);
        DeleteDC(holding);
        done
    }
}

/// A surface of that size the system is willing to take as a thumbnail:
/// four channels, the right way up.
///
/// SAFETY: the caller owns the drawing context and frees what comes back.
#[cfg(windows)]
unsafe fn plain_surface(
    onto: windows_sys::Win32::Graphics::Gdi::HDC,
    size: (i32, i32),
) -> windows_sys::Win32::Graphics::Gdi::HBITMAP {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS,
    };

    let mut about: BITMAPINFO = unsafe { std::mem::zeroed() };
    about.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size.0,
        // Counted downwards, which is the way round the system reads a
        // thumbnail: given the other way it arrives upside down.
        biHeight: -size.1,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let mut pixels = std::ptr::null_mut();
    // SAFETY: the description above is filled in whole, and the two
    // slots the call may write to are ours.
    unsafe {
        CreateDIBSection(
            onto,
            &about,
            DIB_RGB_COLORS,
            &mut pixels,
            std::ptr::null_mut(),
            0,
        )
    }
}

/* ---- Agrandir et réduire, en portant l'image avec ------------------- */

/// A window on its way to a rectangle, and what to tell the system once
/// it is there.
#[cfg(windows)]
struct Growing {
    to: (i32, i32, i32, i32),
    began: std::time::Instant,
    /// When the window was last carried a little further, which is what
    /// the next step is measured from.
    moved: std::time::Instant,
    /// The order the person gave, held back until the move is played.
    then: usize,
    /// Where the window sits when it is not spread over the screen, read
    /// before anything moved it.
    ///
    /// Kept because carrying the window is the same thing, to the
    /// system, as a person dragging it somewhere: each step of the move
    /// wrote itself down as the window's own place. Maximising therefore
    /// ended with « its own place » being the whole screen, and coming
    /// back down came back to almost nothing. It is written back at the
    /// end, exactly as it was read.
    placed: windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT,
    /// How many times the window has been carried a little further, so
    /// the journal can say whether the move was played smoothly or in
    /// three jerks.
    steps: u32,
    /// The widest single step of the move, in pixels. What « it plays
    /// and then snaps » looks like as a number, and the one way to tell
    /// a move that was carried from a move that jumped.
    widest: i32,
}

// Only ever touched from the thread that owns the window, inside its own
// message handler: no lock, and nothing another thread could be holding.
#[cfg(windows)]
thread_local! {
    static GROWING: std::cell::RefCell<Option<Growing>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether the window is being carried from one rectangle to another
/// right now.
///
/// Read from other threads, which is why it is not in the cell above:
/// what a moving window must not have done to it is decided in places
/// this handler does not run.
#[cfg(windows)]
static GROWS: AtomicBool = AtomicBool::new(false);

/// Whether the window is on the move, by a hand or by an order.
///
/// Both mean the same thing to everything that tidies up after a resize:
/// wait. Rounding the picture's corners costs a shape built and a window
/// redrawn, and putting the window back on the picture's shape fights
/// whatever is moving it.
fn on_the_move() -> bool {
    #[cfg(windows)]
    let grows = GROWS.load(Ordering::Relaxed);
    #[cfg(not(windows))]
    let grows = false;
    DRAGGED.load(Ordering::Relaxed) || grows
}

/// How fast the window closes the distance left in front of it.
///
/// The move is not played against a clock running out. It is played as a
/// distance being closed: every step takes the same share of whatever is
/// still ahead, so the steps get smaller and the window slows into its
/// place on its own.
///
/// Played against a clock, a step that arrived late found the clock
/// already out and jumped whatever was left in one go. That is a move
/// that starts, plays, and then snaps, which is what was seen. Played
/// against the distance, a late step is a bigger step and the one after
/// it is smaller again: the move takes longer and stays whole, which is
/// the right way round.
///
/// This is how long it takes to close about two thirds of the distance.
const CLOSES_IN: std::time::Duration = std::time::Duration::from_millis(40);

/// How near counts as arrived. Four pixels of a move nobody can see.
const NEAR_ENOUGH: i32 = 4;

/// The longest a move may take before it is simply finished.
///
/// Nothing should ever reach this: it is there so that a window cannot
/// be left halfway by a machine too busy to carry it.
#[cfg(windows)]
const AT_THE_LATEST: std::time::Duration = std::time::Duration::from_millis(500);

/// Name the timer that plays it answers to.
#[cfg(windows)]
const GROWING_ON: usize = 2;

/// Starts carrying the window towards what that order asks for, instead
/// of letting the system jump it there.
///
/// The system does animate this, and animates it well; what it cannot do
/// is animate two windows as one. It holds the window's own drawing,
/// stretches it towards the new rectangle over about a fifth of a second
/// and only then shows what is really there. The picture is a window of
/// its own and is left out of that: it took its new size at once, so
/// maximising showed the far computer's screen jump to full size and the
/// frame catch up behind it, which is the two windows this whole file
/// exists to hide.
///
/// So the move is played here instead, one step per drawn frame, and
/// each step goes through the very path a hand dragging an edge goes
/// through: our window is moved, and the picture is laid on it inside
/// the same message. What made dragging smooth makes this smooth, and
/// makes the two arrive together because there is only ever one of them
/// being moved.
///
/// The order itself is handed to the system at the end, from where the
/// window already is: what it would animate is then a move of nothing.
///
/// Answers false when there is nothing to play, and the order goes
/// straight to the system.
#[cfg(windows)]
fn play_the_order(window: windows_sys::Win32::Foundation::HWND, order: usize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, IsIconic, SetTimer, WINDOWPLACEMENT,
    };

    // A window down in the taskbar has no rectangle to leave from, and
    // coming back up from there is the system's own animation, which is
    // about an icon and not about a rectangle.
    // SAFETY: our own window, read only.
    if unsafe { IsIconic(window) } != 0 {
        return false;
    }
    let Some(from) = where_it_stands(window) else {
        return false;
    };
    let Some(to) = where_the_order_leads(window, order) else {
        return false;
    };
    if to == from || to.2 - to.0 <= 0 || to.3 - to.1 <= 0 {
        return false;
    }
    // Read before the first step moves anything, since every step is a
    // move like any other as far as the system is concerned, and it
    // would write each of them down here.
    let mut placed: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placed.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    // SAFETY: our own window and the slot is ours, with its size written
    // in it as the call requires.
    if unsafe { GetWindowPlacement(window, &mut placed) } == 0 {
        return false;
    }

    let now = std::time::Instant::now();
    GROWING.with_borrow_mut(|growing| {
        *growing = Some(Growing {
            to,
            began: now,
            moved: now,
            then: order,
            placed,
            steps: 0,
            widest: 0,
        });
    });
    GROWS.store(true, Ordering::Relaxed);
    // The corners go for the length of the move, as they do for a drag:
    // a shape is the size the window had when it was given, and a window
    // growing under one is clipped to where it used to end.
    if let Some(engine) = the_engines_window() {
        let_the_corners_go(engine);
    }
    // Every drawn frame, near enough: the system rounds this up to its
    // own tick, which is a frame.
    // SAFETY: our own window, from the thread that owns it.
    unsafe { SetTimer(window, GROWING_ON, 8, None) };
    true
}

/// The rectangle the window covers right now.
#[cfg(windows)]
fn where_it_stands(window: windows_sys::Win32::Foundation::HWND) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let mut now = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: our own window and the rectangle is ours.
    (unsafe { GetWindowRect(window, &mut now) } != 0)
        .then_some((now.left, now.top, now.right, now.bottom))
}

/// Lets the system play its own animations for this window, or stops it
/// for a moment.
///
/// Only ever off for the handful of pixels the order at the end of a
/// move has left to travel. Off for longer, the window would stop
/// growing and shrinking like every other window on the machine, which
/// is not what is wanted; on for those pixels, they are played as a
/// little move of their own at the end of the real one.
#[cfg(windows)]
fn transitions(window: windows_sys::Win32::Foundation::HWND, may: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute,
    };

    let stopped: i32 = i32::from(!may);
    // SAFETY: our own window, an attribute made to be set, and the value
    // is ours, of the size the call is told.
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&raw const stopped).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

/// Where the window ends up once that order has been carried out.
///
/// Worked out rather than found out. Finding out means letting the
/// system do it and reading the answer, and between the doing and the
/// reading the window is already there: one drawn frame of it at its
/// full size is the very jump being taken out.
///
/// It does not have to be exact. The order is handed to the system at
/// the end of the move, and the system puts the window exactly where it
/// belongs; a few pixels out here is a last step of a few pixels.
#[cfg(windows)]
fn where_the_order_leads(
    window: windows_sys::Win32::Foundation::HWND,
    order: usize,
) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, SC_MAXIMIZE, WINDOWPLACEMENT,
    };

    // SAFETY: our own window; the nearest monitor is always an answer.
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let mut screen: MONITORINFO = unsafe { std::mem::zeroed() };
    screen.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: the monitor comes from the call above and the slot is ours,
    // with its size written in it as the call requires.
    if unsafe { GetMonitorInfoW(monitor, &mut screen) } == 0 {
        return None;
    }

    if order == SC_MAXIMIZE as usize {
        // A maximised window is not the size of the desktop: it is the
        // desktop plus the invisible bands a resize can be grabbed in,
        // which is why it looks flush with the edges. Those bands are
        // whatever separates this window's rectangle from the frame it
        // actually draws, measured on it as it stands.
        return Some(spread_over(
            (
                screen.rcWork.left,
                screen.rcWork.top,
                screen.rcWork.right,
                screen.rcWork.bottom,
            ),
            spread_bands(window),
        ));
    }

    let mut placed: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placed.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    // SAFETY: our own window and the slot is ours, with its size written
    // in it as the call requires.
    if unsafe { GetWindowPlacement(window, &mut placed) } == 0 {
        return None;
    }
    // The system keeps that rectangle counted from the corner of the
    // desktop rather than the corner of the screen, and the two differ
    // by wherever the taskbar sits when it sits at the top or the left.
    let (dx, dy) = (
        screen.rcWork.left - screen.rcMonitor.left,
        screen.rcWork.top - screen.rcMonitor.top,
    );
    let normal = placed.rcNormalPosition;
    Some((
        normal.left + dx,
        normal.top + dy,
        normal.right + dx,
        normal.bottom + dy,
    ))
}

/// The frame this window actually draws, in screen coordinates.
#[cfg(windows)]
fn the_drawn_frame_of(
    window: windows_sys::Win32::Foundation::HWND,
) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};

    let mut drawn = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: our own window, an attribute made to be asked for, and the
    // slot is ours, of the size the call is told.
    if unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut drawn).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } != 0
    {
        return None;
    }
    Some((drawn.left, drawn.top, drawn.right, drawn.bottom))
}

/// Carries the window one step further, and finishes the job on the last
/// one.
#[cfg(windows)]
fn grow_a_step(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsZoomed, KillTimer, SC_MAXIMIZE, SW_SHOWMAXIMIZED, SW_SHOWNORMAL, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPlacement, SetWindowPos,
        WM_SYSCOMMAND,
    };

    // Where the window really is, and not where the last step meant to
    // put it: the system has the last word on what it granted, and a
    // move worked out from what it granted cannot drift away from it.
    let Some(now) = where_it_stands(window) else {
        return;
    };
    let Some((step, done)) = GROWING.with_borrow_mut(|growing| {
        let growing = growing.as_mut()?;
        growing.steps += 1;
        let waited = growing.moved.elapsed();
        growing.moved = std::time::Instant::now();
        let step = along(now, growing.to, closed_in(waited, CLOSES_IN));
        growing.widest = growing
            .widest
            .max(((step.2 - step.0) - (now.2 - now.0)).abs());
        // Arrived when what is left cannot be seen, and finished anyway
        // once it has gone on far too long.
        let done = within(step, growing.to, NEAR_ENOUGH) || growing.began.elapsed() > AT_THE_LATEST;
        Some((step, done))
    }) else {
        return;
    };

    // SAFETY: our own window, moved without being resized in z-order or
    // activated. The picture is laid on it inside this very call, in the
    // handler for the message this one sends.
    unsafe {
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            step.0,
            step.1,
            step.2 - step.0,
            step.3 - step.1,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    if !done {
        return;
    }

    let played = GROWING.with_borrow_mut(|growing| growing.take());
    GROWS.store(false, Ordering::Relaxed);
    // SAFETY: our own window, from the thread that owns it.
    unsafe { KillTimer(window, GROWING_ON) };
    if let Some(played) = played.as_ref() {
        // One line per gesture. A move played in far fewer steps than
        // there are drawn frames in it was played jerkily, and what a
        // step waits on is the player taking its new size: that is where
        // to look, and there is nowhere else to read it from.
        crate::journal::note(&format!(
            "{} joué en {:.0} ms, {} pas, plus grand pas {} px",
            if played.then == SC_MAXIMIZE as usize {
                "agrandissement"
            } else {
                "retour en fenêtre"
            },
            played.began.elapsed().as_secs_f64() * 1000.0,
            played.steps,
            played.widest,
        ));
    }
    if let Some(played) = played {
        // The order now, from a few pixels short of where it leads. Not
        // from exactly there: the compositor draws a window's frame from
        // what it works out about it, and it works it out again when the
        // window moves. Landed exactly on the mark, nothing moved on
        // this last step and the frame stayed the one the window had
        // while it was spread over the screen, flat-topped and without
        // its border, until the next time a hand moved the window. A few
        // pixels left for the system to travel is a frame worked out
        // again.
        //
        // With its own animation off for the length of it, and only for
        // that: those few pixels are a move like any other to the
        // system, and it would play them as one.
        transitions(window, false);
        // SAFETY: the order the person gave, at the window it was given
        // to. Handed straight to the system's own handling, since ours
        // would only take it back and play it again.
        unsafe { DefSubclassProc(window, WM_SYSCOMMAND, played.then, 0) };

        // And the window's own place after it, put back as it was read
        // before the first step. Every step of the move looked to the
        // system exactly like a person dragging the window somewhere,
        // and it wrote each of them down: maximising ended with « where
        // this window belongs » being the whole screen, so coming back
        // down came back to almost the same thing.
        //
        // After and never before. Asked to place and to maximise in one
        // move, the system does them in that order: it put the window
        // back at its small size and then grew it again, which is the
        // little move that played itself out at the end of every
        // maximise. Done afterwards, the state is already the one that
        // was asked for and this only writes down a rectangle.
        let mut placed = played.placed;
        placed.showCmd = if played.then == SC_MAXIMIZE as usize {
            SW_SHOWMAXIMIZED as u32
        } else {
            SW_SHOWNORMAL as u32
        };
        // SAFETY: our own window, and the placement is the one the
        // system itself gave us with one field changed.
        unsafe { SetWindowPlacement(window, &placed) };

        // And the frame drawn again from scratch. A window carried about
        // while the system counted it as spread over the screen keeps
        // what it was given then: square corners and no border, which is
        // right for a window filling a screen and wrong for the one that
        // has just come back down from it.
        // SAFETY: our own window; nothing is moved, resized or
        // activated, and the frame is the only thing asked for.
        unsafe {
            SetWindowPos(
                window,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        };
        transitions(window, true);
        // The whole of what the window ended up as. « It came back with
        // square edges » is about the frame the compositor draws, and
        // the frame is worked out from these: whether the system counts
        // the window as spread over a screen, where it stands, and where
        // it draws. Those three side by side say which of them is wrong.
        // SAFETY: our own window, read only.
        crate::journal::note(&format!(
            "fenêtre {} après le mouvement : visé {:?}, obtenu {:?}, cadre dessiné {:?}",
            if unsafe { IsZoomed(window) } != 0 {
                "agrandie"
            } else {
                "en fenêtre"
            },
            played.to,
            where_it_stands(window),
            the_drawn_frame_of(window),
        ));
    }
    // The shape was left alone while the window was moving; it has
    // stopped.
    if let Some(engine) = the_engines_window() {
        lay_it_out(window, engine);
    }
}

/// The rectangle a window covers once it is spread over the desktop.
///
/// Not the desktop itself: a window carries invisible bands all round it
/// that a resize can be grabbed in, and spread out it hangs those bands
/// off the edges of the screen so that what it draws lands flush with
/// them. `whole` is the rectangle the system counts the window as, and
/// `drawn` the frame it really draws; what separates them is the bands.
fn spread_over(work: (i32, i32, i32, i32), band: (i32, i32)) -> (i32, i32, i32, i32) {
    (
        work.0 - band.0,
        work.1 - band.1,
        work.2 + band.0,
        work.3 + band.1,
    )
}

/// How far a window spread over a screen hangs off each edge of it.
///
/// The bands a resize can be grabbed in, which are invisible and which a
/// spread window hangs off the screen so that what it draws lands flush
/// with the edges. Asked of the system at this window's own scale.
#[cfg(windows)]
fn spread_bands(window: windows_sys::Win32::Foundation::HWND) -> (i32, i32) {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CYSIZEFRAME,
    };

    // SAFETY: our own window, and plain metrics at that scale.
    unsafe {
        let dpi = GetDpiForWindow(window).max(96);
        let padded = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        (
            GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + padded,
            GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + padded,
        )
    }
}

/// Where the window stands when it is that far along.
fn along(from: (i32, i32, i32, i32), to: (i32, i32, i32, i32), part: f64) -> (i32, i32, i32, i32) {
    let between = |a: i32, b: i32| a + ((f64::from(b - a)) * part).round() as i32;
    (
        between(from.0, to.0),
        between(from.1, to.1),
        between(from.2, to.2),
        between(from.3, to.3),
    )
}

/// What share of the distance still ahead a step of that length closes.
///
/// The same share of whatever is left, every time, which is what makes
/// the steps get smaller and the window slow into its place instead of
/// arriving at speed and stopping dead. Every window on this system
/// slows into its place, so this one does too.
///
/// Worked out from how long the step actually took rather than from how
/// long it was meant to take: a step that waited twice as long closes
/// twice as much of what is left, so the window travels the same path
/// whether the machine is giving us sixty steps or six. That is what a
/// move cannot have: a shape that depends on the machine's mood.
fn closed_in(waited: std::time::Duration, closes_in: std::time::Duration) -> f64 {
    1.0 - (-waited.as_secs_f64() / closes_in.as_secs_f64()).exp()
}

/// Whether those two rectangles are within that many pixels of each
/// other on every edge.
fn within(here: (i32, i32, i32, i32), there: (i32, i32, i32, i32), near: i32) -> bool {
    (here.0 - there.0).abs() <= near
        && (here.1 - there.1).abs() <= near
        && (here.2 - there.2).abs() <= near
        && (here.3 - there.3).abs() <= near
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
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostMessageW, SC_MAXIMIZE, SC_RESTORE, WINDOWPOS, WM_ACTIVATEAPP,
        WM_DWMSENDICONICLIVEPREVIEWBITMAP, WM_DWMSENDICONICTHUMBNAIL, WM_ENTERSIZEMOVE,
        WM_EXITSIZEMOVE, WM_NCACTIVATE, WM_SYSCOMMAND, WM_TIMER, WM_WINDOWPOSCHANGED,
        WM_WINDOWPOSCHANGING,
    };

    match message {
        // « Agrandir » and « Niveau inférieur », from the title bar
        // button or a double click on the bar itself. Played by hand
        // rather than left to the system, which animates one window and
        // would leave the picture behind; see `play_the_order`.
        //
        // The low four bits are the system's own, and carry which corner
        // of a menu the order came from.
        WM_SYSCOMMAND
            if matches!((wparam & 0xFFF0) as u32, SC_MAXIMIZE | SC_RESTORE)
                && the_engines_window().is_some()
                && play_the_order(window, wparam & 0xFFF0) =>
        {
            0
        }
        // The system wants something to show this window as, small: in
        // Alt+Tab, or hovering its button in the taskbar. It would take
        // a photograph of this window, and the session is in another one
        // laid over it, so it would show the page the session is hiding.
        // Handed the session instead.
        //
        // The size it will take is in the message for a thumbnail, and
        // its own for a preview, which is shown at the window's size.
        WM_DWMSENDICONICTHUMBNAIL
            if hand_over_a_picture(
                window,
                ((lparam >> 16) as u16 as i32, lparam as u16 as i32),
                false,
            ) =>
        {
            0
        }
        WM_DWMSENDICONICLIVEPREVIEWBITMAP
            if where_it_stands(window).is_some_and(|(left, top, right, bottom)| {
                hand_over_a_picture(window, (right - left, bottom - top), true)
            }) =>
        {
            0
        }
        // One step of that move.
        WM_TIMER if wparam == GROWING_ON => {
            grow_a_step(window);
            0
        }
        // The system asking whether this window is still active, which
        // is the one question that decides how the title bar is drawn.
        //
        // Answered « yes » whenever the front belongs to this session:
        // the picture is our own window's inside, and the floating
        // button is ours outright, so neither of them taking the front
        // means the window has stopped being used.
        //
        // And asked again straight after. The front is not settled while
        // this message is being sent, so the answer given here can be
        // the wrong one either way; a message to ourselves comes back
        // once the system has finished, and settles it.
        WM_NCACTIVATE => {
            // SAFETY: our own window, from the thread that owns it, and
            // the message is one of ours.
            unsafe { PostMessageW(window, BAR, 0, 0) };
            let lit = if wparam == 0 && who_holds_the_front() != Front::Elsewhere {
                1
            } else {
                wparam
            };
            // SAFETY: the arguments the system handed in, with one
            // boolean possibly turned around.
            unsafe { DefSubclassProc(window, message, lit, lparam) }
        }
        // The front has settled; draw the bar the way it really stands.
        BAR => {
            draw_the_bar(window);
            0
        }
        // A window about to take a new size, while a hand is on an edge:
        // the system says what it is about to apply and takes back
        // whatever is written there, before anything moves. Holding the
        // shape here is what costs nothing: corrected afterwards, every
        // step of a drag resized the window twice.
        //
        // This message and not the sizing one of the drag loop, which was
        // answered here before and never arrived: the journal counted the
        // steps of a drag through this message's own echo while the shape
        // ran free. Every change of size becomes real by passing through
        // here, whoever asked for it, so here is where the shape holds.
        WM_WINDOWPOSCHANGING if DRAGGED.load(Ordering::Relaxed) => {
            // SAFETY: for this message the system passes a WINDOWPOS of
            // ours to read and amend, and it lives for the length of the
            // call.
            let wanted = unsafe { &mut *(lparam as *mut WINDOWPOS) };
            the_drag_keeps_the_shape(window, wanted);
            // The picture goes first, onto the inside our window is about
            // to have, and our window follows inside this same message.
            //
            // The order matters and it is the whole of this fix. Moving
            // our window costs nothing: it is ours, on this thread.
            // Moving the picture costs a wait on another program, a
            // millisecond or so. Done in that order, that millisecond is
            // a millisecond in which the frame has moved and the picture
            // has not, and the compositor draws what it finds: a strip of
            // the page along the edge the window is heading for. Done the
            // other way about, the wait falls before anything has moved
            // and what is left afterwards is too short to be caught.
            if let Some(engine) = the_engines_window()
                && let Some((corner, width, height)) = the_inside_after(window, wanted)
            {
                lay_on(window, engine, corner, width, height);
            }
            // Handed on: what was written only becomes the window's size
            // in the system's own handling of this message.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        // A hand on the window. Which of the two gestures it is, moving
        // it or resizing it, is not said and is not asked here: the
        // corners are only in the way of one of them, and they come off
        // at the first step that changes the size rather than at the
        // first step at all. Taken off here, carrying the window across
        // the desk squared the picture's corners for the length of the
        // carry, over a frame that had kept its own.
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
            if let Some(engine) = the_engines_window() {
                lay_it_out(window, engine);
            }
            answer
        }
        // Another program has taken the front, or given it back. The
        // floating button hangs on the picture and is drawn above every
        // window on the machine: left up, it hangs over whatever was
        // switched to. Laid again, which is where that is decided.
        WM_ACTIVATEAPP => {
            // SAFETY: the arguments the system handed in, untouched.
            let answer = unsafe { DefSubclassProc(window, message, wparam, lparam) };
            if let Some(engine) = the_engines_window() {
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
            if let Some(engine) = the_engines_window() {
                lay_it_out(window, engine);
            }
            answer
        }
        // SAFETY: same.
        _ => unsafe { DefSubclassProc(window, message, wparam, lparam) },
    }
}

/// Where our window's inside will be once that proposal is applied.
///
/// Worked out from the proposal and from what separates our window's
/// edges from its inside, which a move does not change and a resize does
/// not change either: the bands and the title bar are the same whatever
/// size the window is.
#[cfg(windows)]
fn the_inside_after(
    home: windows_sys::Win32::Foundation::HWND,
    wanted: &windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPOS,
) -> Option<((i32, i32), i32, i32)> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOMOVE, SWP_NOSIZE};

    let (left, top, right, bottom) = where_it_stands(home)?;
    let (corner, width, height) = the_inside_of(home)?;
    let (x, y) = if wanted.flags & SWP_NOMOVE != 0 {
        (left, top)
    } else {
        (wanted.x, wanted.y)
    };
    let (cx, cy) = if wanted.flags & SWP_NOSIZE != 0 {
        (right - left, bottom - top)
    } else {
        (wanted.cx, wanted.cy)
    };
    Some((
        (x + (corner.0 - left), y + (corner.1 - top)),
        cx - ((right - left) - width),
        cy - ((bottom - top) - height),
    ))
}

/// Holds the size a drag is about to apply to the shape of the picture.
#[cfg(windows)]
fn the_drag_keeps_the_shape(
    window: windows_sys::Win32::Foundation::HWND,
    wanted: &mut windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPOS,
) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, SWP_NOSIZE};

    // Only a change of size: the same message carries plain moves and
    // z-order changes, which have no shape to hold.
    if wanted.flags & SWP_NOSIZE != 0 {
        return;
    }
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

    let now = (outside.left, outside.top, outside.right, outside.bottom);
    let asked = (wanted.x, wanted.y, wanted.cx, wanted.cy);
    // A window merely being carried has nothing to hold: its size is
    // not moving. Corrected all the same, the origin was put back where
    // the drag began at every step, and the window could not be moved
    // at all.
    if !the_size_moves(now, asked) {
        return;
    }
    // The first step that really changes the size, and only that one:
    // the shape a window was given is the shape of the size it had when
    // it was given, so a window growing under one is clipped to where it
    // used to end. Off for the rest of the gesture, and put back when the
    // hand stops.
    if !RESIZED.swap(true, Ordering::Relaxed)
        && let Some(engine) = the_engines_window()
    {
        let_the_corners_go(engine);
    }

    // Gathered and never re-decided: the hand cannot let go of one edge
    // and take another without ending the drag, so an edge seen to move
    // once is held for the rest of it.
    let seen = the_edges_under_the_hand(now, asked);
    let held = HELD.fetch_or(seen, Ordering::Relaxed) | seen;

    let (x, y, cx, cy) = what_the_drag_becomes(
        now,
        asked,
        frame,
        (wide, high),
        the_least_picture(crate::floating::room_for_the_button(), (wide, high)),
        held,
    );
    count_the_step(cx);
    wanted.x = x;
    wanted.y = y;
    wanted.cx = cx;
    wanted.cy = cy;
}

/// Whether that proposal changes the window's size at all.
///
/// A hand on the title bar and a hand on an edge send the very same
/// message, and only the second one has a shape to hold.
fn the_size_moves(now: (i32, i32, i32, i32), wanted: (i32, i32, i32, i32)) -> bool {
    let (left, top, right, bottom) = now;
    wanted.2 != right - left || wanted.3 != bottom - top
}

/// Which edges of the window that proposal moves.
///
/// The whole of what the system says about where the hand is. An edge it
/// leaves exactly where it stands is an edge nobody is holding: a window
/// dragged by its right side keeps its top and its bottom to the pixel,
/// for as long as the drag lasts.
fn the_edges_under_the_hand(now: (i32, i32, i32, i32), wanted: (i32, i32, i32, i32)) -> u8 {
    let (left, top, right, bottom) = now;
    let (x, y, cx, cy) = wanted;
    let mut held = 0;
    if x != left || x + cx != right {
        held |= A_SIDE;
    }
    if y != top || y + cy != bottom {
        held |= TOP_OR_BOTTOM;
    }
    held
}

/// The place and size a dragged window takes instead of what the hand
/// asked, so the picture keeps its shape.
///
/// The edge under the hand follows the hand exactly and the other side is
/// worked out from it. A corner holds both at once and cannot have both,
/// since only one size out of the two is free: it gets the halfway point
/// between the width it asked for and the width its height asks for,
/// which follows a hand going in any direction and, unlike choosing one
/// of the two, never jumps when the hand changes direction slightly.
///
/// Sizes are inner sizes once the frame is paid for, floored at the
/// smallest picture the button still fits in.
///
/// And the edge opposite the hand stands still. The system moves the
/// window's origin when the left or top edge is dragged; whenever it has,
/// whatever the shape added to the size is taken off the origin again, so
/// the far edge does not walk out from under a hand that is not on it.
/// Taken off what the system proposed and never off where the window
/// stands: the two are the same only until the shape corrects something,
/// and reading the second put a window that was merely being carried
/// straight back where it started.
///
/// Only ever asked about a proposal that really changes the size; see
/// `the_size_moves`.
fn what_the_drag_becomes(
    now: (i32, i32, i32, i32),
    wanted: (i32, i32, i32, i32),
    frame: (i32, i32),
    shape: (i32, i32),
    least: (i32, i32),
    held: u8,
) -> (i32, i32, i32, i32) {
    let (left, top, ..) = now;
    let (x, y, cx, cy) = wanted;

    let (held_cx, held_cy) = if held == TOP_OR_BOTTOM {
        let inner = (cy - frame.1).max(least.1);
        (across(inner, shape.1, shape.0) + frame.0, inner + frame.1)
    } else {
        let asked = cx - frame.0;
        let inner = if held == A_SIDE {
            asked
        } else {
            (asked + across(cy - frame.1, shape.1, shape.0)) / 2
        };
        let inner = inner.max(least.0);
        (inner + frame.0, across(inner, shape.0, shape.1) + frame.1)
    };

    (
        if x != left { x - (held_cx - cx) } else { x },
        if y != top { y - (held_cy - cy) } else { y },
        held_cx,
        held_cy,
    )
}

/// One side of the picture worked out from the other, keeping its shape.
///
/// Counted wide so that a large window at a narrow shape cannot run past
/// what a whole number holds on the way.
fn across(side: i32, of: i32, to: i32) -> i32 {
    (i64::from(side) * i64::from(to) / i64::from(of)) as i32
}

/// The same, never falling short.
///
/// A shape is held by dividing down, which is right for following a hand:
/// the picture may end a pixel narrow and nobody sees it. It is wrong for
/// working out a floor, where a pixel short is not a floor at all.
fn across_at_least(side: i32, of: i32, to: i32) -> i32 {
    let (side, of, to) = (i64::from(side), i64::from(of), i64::from(to));
    ((side * to + of - 1) / of) as i32
}

/// The smallest the picture may be taken down to, at its own shape.
///
/// The system bounds a window it is resizing before offering the
/// rectangle to us, and does not bound it again after: whatever is
/// written back is taken as it stands. Holding a shape therefore means
/// holding a floor as well, or the window walks straight through the
/// smallest size Windows would ever have allowed and comes out the other
/// side a sliver with nothing usable in it.
///
/// The floor is the floating button, which is the one thing of ours left
/// on the picture and the first to stop fitting. Both of its sides count,
/// since a shape ties the two together: the width has to leave room for
/// the button, and so has the height that width works out to.
///
/// No button means no session, and a picture with no shape to hold is
/// never taken down this road at all; the floor is then whatever the
/// shape makes of a single pixel, which is as good as no floor and is
/// still never zero.
fn the_least_picture(room: Option<(i32, i32)>, shape: (i32, i32)) -> (i32, i32) {
    let (wide, high) = shape;
    let room = room.unwrap_or((1, 1));
    (
        room.0.max(across_at_least(room.1, high, wide)),
        room.1.max(across_at_least(room.0, wide, high)),
    )
}

/// The engine's window as the handler may act on it, or nothing.
///
/// Nothing when there is none, and nothing when the number no longer
/// names the engine's window: the system hands numbers back out when
/// their window goes, and for the seconds between the engine dying and
/// the session being tidied away, this number can come to name a
/// stranger's window. Acting on it then would move that window, resize
/// it to our inside and force it shown.
#[cfg(windows)]
fn the_engines_window() -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

    let engine = ENGINE.load(Ordering::Relaxed) as windows_sys::Win32::Foundation::HWND;
    if engine.is_null() {
        return None;
    }
    // SAFETY: a window number, which the call is made to weigh.
    if unsafe { IsWindow(engine) } == 0 {
        return None;
    }
    let mut owner = 0u32;
    // SAFETY: same window, and the slot is ours.
    unsafe { GetWindowThreadProcessId(engine, &mut owner) };
    (owner == PLAYER.load(Ordering::Relaxed)).then_some(engine)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_side_worked_out_from_the_other_keeps_the_shape() {
        assert_eq!(across(1920, 1920, 1080), 1080);
        assert_eq!(across(960, 1920, 1080), 540);
        assert_eq!(across(1080, 1080, 1920), 1920);
    }

    #[test]
    fn a_wide_window_at_a_narrow_shape_does_not_run_past_a_whole_number() {
        // 3840 x 1080 tenu par un entier de 32 bits ferait 4 milliards en
        // chemin. Compté large, la réponse est juste.
        assert_eq!(across(3_000_000, 1080, 1920), 5_333_333);
    }

    #[test]
    fn the_floor_leaves_room_for_the_button_whichever_edge_is_pulled() {
        // Le bouton fait 91 pixels de côté sur un écran agrandi, plus sa
        // marge. Tirer un bord fixe une des deux tailles et laisse l'autre
        // suivre la forme : les deux doivent laisser la place au bouton,
        // sinon il n'y a plus de sortie qu'au clavier.
        let room = (107, 107);
        for shape in [(1920, 1080), (1080, 1920), (2560, 1080), (1024, 1024)] {
            let (wide, high) = the_least_picture(Some(room), shape);
            assert!(wide >= room.0, "largeur {wide} sur {shape:?}");
            assert!(high >= room.1, "hauteur {high} sur {shape:?}");
            // Un bord vertical tiré : la largeur tient, la hauteur suit.
            assert!(
                across(wide, shape.0, shape.1) >= room.1,
                "hauteur suivie sur {shape:?}"
            );
            // Un bord horizontal tiré : l'inverse.
            assert!(
                across(high, shape.1, shape.0) >= room.0,
                "largeur suivie sur {shape:?}"
            );
        }
    }

    // La fenêtre des essais du glissement : posée en (100, 100), 960x540
    // dedans, un cadre de 16 de large et 42 de haut, une image 16:9.
    const NOW: (i32, i32, i32, i32) = (100, 100, 1076, 682);
    const FRAME: (i32, i32) = (16, 42);
    const SHAPE: (i32, i32) = (1920, 1080);

    /// Ce que le système propose, tenu à la forme, la main étant là où
    /// cette proposition dit qu'elle est.
    fn drag(wanted: (i32, i32, i32, i32), least: (i32, i32)) -> (i32, i32, i32, i32) {
        what_the_drag_becomes(
            NOW,
            wanted,
            FRAME,
            SHAPE,
            least,
            the_edges_under_the_hand(NOW, wanted),
        )
    }

    #[test]
    fn the_edges_the_hand_holds_are_the_ones_that_move() {
        // Un bord vertical, un bord horizontal, puis un coin : la
        // proposition du système laisse les autres bords au pixel près.
        assert_eq!(the_edges_under_the_hand(NOW, (100, 100, 1276, 582)), A_SIDE);
        assert_eq!(the_edges_under_the_hand(NOW, (60, 100, 1016, 582)), A_SIDE);
        assert_eq!(
            the_edges_under_the_hand(NOW, (100, 100, 976, 782)),
            TOP_OR_BOTTOM
        );
        assert_eq!(
            the_edges_under_the_hand(NOW, (100, 60, 976, 622)),
            TOP_OR_BOTTOM
        );
        assert_eq!(
            the_edges_under_the_hand(NOW, (100, 100, 1076, 682)),
            A_SIDE | TOP_OR_BOTTOM
        );
    }

    #[test]
    fn pulling_the_bottom_edge_makes_the_width_follow() {
        // La main descend le bord du bas de 200 : la hauteur mène, la
        // largeur suit, et les deux autres bords ne bougent pas.
        let (x, y, cx, cy) = drag((100, 100, 976, 782), (1, 1));
        assert_eq!((x, y, cy), (100, 100, 782));
        assert_eq!(cx, across(782 - 42, 1080, 1920) + 16);
    }

    #[test]
    fn pulling_a_side_makes_the_height_follow() {
        let (x, y, cx, cy) = drag((100, 100, 1276, 582), (1, 1));
        assert_eq!((x, y, cx), (100, 100, 1276));
        assert_eq!(cy, across(1276 - 16, 1920, 1080) + 42);
    }

    #[test]
    fn pulling_the_top_edge_keeps_the_bottom_where_it_is() {
        // La main remonte le bord du haut : l'origine bouge avec elle, et
        // le bas de la fenêtre reste exactement où il était.
        let (_, y, _, cy) = drag((100, 60, 976, 622), (1, 1));
        assert_eq!(y + cy, 682);
    }

    #[test]
    fn pulling_the_left_edge_keeps_the_right_where_it_is() {
        let (x, _, cx, _) = drag((60, 100, 1016, 582), (1, 1));
        assert_eq!(x + cx, 1076);
    }

    #[test]
    fn a_corner_answers_a_hand_going_in_either_direction() {
        // Le coin bas droit tenu, la main part droit à droite, puis droit
        // en bas. La fenêtre grandit dans les deux cas : s'en tenir à un
        // seul côté pour tout le glissement rendait l'un des deux gestes
        // sans effet.
        let corner = A_SIDE | TOP_OR_BOTTOM;
        let sideways =
            what_the_drag_becomes(NOW, (100, 100, 1016, 582), FRAME, SHAPE, (1, 1), corner);
        let downwards =
            what_the_drag_becomes(NOW, (100, 100, 976, 622), FRAME, SHAPE, (1, 1), corner);
        assert!(sideways.2 > 976 && sideways.3 > 582, "{sideways:?}");
        assert!(downwards.2 > 976 && downwards.3 > 582, "{downwards:?}");
    }

    #[test]
    fn a_corner_moving_steadily_does_not_send_the_window_back_and_forth() {
        // Le geste qui faisait trembler l'image : la main descend en
        // diagonale, un peu plus large à un pas, un peu plus haut au pas
        // suivant. La fenêtre doit grandir à chaque pas, jamais reculer.
        let (mut left, mut top, mut right, mut bottom) = NOW;
        let mut widths = Vec::new();
        for step in 0..40 {
            let (dx, dy) = if step % 2 == 0 { (3, 2) } else { (2, 3) };
            let wanted = (left, top, right - left + dx, bottom - top + dy);
            let (x, y, cx, cy) = what_the_drag_becomes(
                (left, top, right, bottom),
                wanted,
                FRAME,
                SHAPE,
                (1, 1),
                A_SIDE | TOP_OR_BOTTOM,
            );
            (left, top, right, bottom) = (x, y, x + cx, y + cy);
            widths.push(cx);
        }
        for pair in widths.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "la fenêtre a reculé : {:?} dans {widths:?}",
                pair
            );
        }
        // Et elle suit vraiment la main : quarante pas de deux ou trois
        // pixels ne peuvent pas laisser la fenêtre sur place.
        assert!(widths[39] - widths[0] > 60, "{widths:?}");
    }

    #[test]
    fn the_drag_cannot_take_the_window_under_the_floor() {
        // Le bord du bas remonté à fond, puis le côté rentré à fond : la
        // fenêtre s'arrête à la taille où le bouton tient encore, en
        // hauteur comme en largeur.
        let least = the_least_picture(Some((107, 107)), SHAPE);
        for wanted in [
            (100, 100, 976, 100),
            (100, 100, 100, 582),
            (100, 100, 100, 100),
        ] {
            let (_, _, cx, cy) = drag(wanted, least);
            assert!(
                cx - 16 >= 107,
                "largeur au sol : {} sur {wanted:?}",
                cx - 16
            );
            assert!(
                cy - 42 >= 107,
                "hauteur au sol : {} sur {wanted:?}",
                cy - 42
            );
        }
    }

    #[test]
    fn a_window_being_carried_is_not_a_window_being_resized() {
        // La fenêtre part vers le haut à gauche, taille inchangée : c'est
        // un déplacement. Tenir une forme là-dessus corrigeait l'origine
        // et remettait la fenêtre à son point de départ à chaque pas, ce
        // qui la rendait immobile.
        for ailleurs in [(60, 40), (400, 300), (100, 40), (60, 100)] {
            let porte = (ailleurs.0, ailleurs.1, 976, 582);
            assert!(
                !the_size_moves(NOW, porte),
                "déplacement pris pour un redimensionnement : {porte:?}"
            );
        }
        // Et un vrai redimensionnement reste reconnu, même d'un pixel.
        assert!(the_size_moves(NOW, (100, 100, 977, 582)));
        assert!(the_size_moves(NOW, (100, 100, 976, 583)));
    }

    #[test]
    fn a_size_that_did_not_change_is_left_alone() {
        let same = (100, 100, 976, 582);
        assert_eq!(drag(same, (1, 1)), same);
    }

    #[test]
    fn a_window_spread_over_the_desktop_hangs_its_grab_bands_off_the_edges() {
        // Les bandes de préhension font 7 pixels de côté et 9 en
        // hauteur : étalée, la fenêtre doit dépasser de l'écran
        // d'exactement autant, pour que ce qui se dessine tombe pile sur
        // les bords.
        let work = (0, 0, 1920, 1040);
        assert_eq!(spread_over(work, (7, 9)), (-7, -9, 1927, 1049));
    }

    #[test]
    fn a_window_without_grab_bands_is_spread_to_the_desktop_itself() {
        let work = (0, 0, 1920, 1040);
        assert_eq!(spread_over(work, (0, 0)), work);
    }

    /// Le mouvement joué, un pas toutes les `chaque`, jusqu'à l'arrivée.
    /// Rend la liste des rectangles traversés.
    fn joue(
        from: (i32, i32, i32, i32),
        to: (i32, i32, i32, i32),
        chaque: std::time::Duration,
    ) -> Vec<(i32, i32, i32, i32)> {
        let mut passe = vec![from];
        let mut ici = from;
        for _ in 0..200 {
            if within(ici, to, NEAR_ENOUGH) {
                break;
            }
            ici = along(ici, to, closed_in(chaque, CLOSES_IN));
            passe.push(ici);
        }
        passe
    }

    #[test]
    fn a_step_closes_more_of_the_way_the_longer_it_waited() {
        // Deux pas courts doivent mener au même endroit qu'un pas long :
        // c'est ce qui fait que le mouvement a la même allure que la
        // machine nous donne soixante pas ou six.
        let quart = std::time::Duration::from_millis(10);
        let demi = std::time::Duration::from_millis(20);
        let un = closed_in(quart, CLOSES_IN);
        let deux = 1.0 - (1.0 - un) * (1.0 - un);
        assert!((deux - closed_in(demi, CLOSES_IN)).abs() < 1e-9);
        // Et un pas de durée nulle ne bouge rien.
        assert_eq!(closed_in(std::time::Duration::ZERO, CLOSES_IN), 0.0);
    }

    #[test]
    fn the_move_never_goes_backwards_and_slows_into_its_place() {
        let (from, to) = ((100, 100, 1100, 700), (-7, 0, 1927, 1047));
        let passe = joue(from, to, std::time::Duration::from_millis(16));
        let mut avant = from;
        let mut dernier = i32::MAX;
        for (pas, ici) in passe.iter().enumerate().skip(1) {
            assert!(ici.2 >= avant.2, "la fenêtre a reculé au pas {pas}");
            let fait = ici.2 - avant.2;
            assert!(fait <= dernier, "elle a accéléré au pas {pas}");
            dernier = fait;
            avant = *ici;
        }
        assert!(within(avant, to, 4), "arrivée à {avant:?} pour {to:?}");
    }

    #[test]
    fn a_move_played_in_few_steps_follows_the_same_path_as_one_played_in_many() {
        // Le défaut à ne jamais revoir : le mouvement qui commence, joue,
        // puis saute d'un coup à l'arrivée parce que l'horloge était
        // finie. Compté en distance et non en temps, une machine qui ne
        // donne que trois pas les fait plus grands, et la fenêtre est au
        // même endroit au même instant qu'avec vingt pas. Une machine
        // lente coûte des images, jamais la forme du geste.
        let (from, to) = ((100, 100, 1100, 700), (-7, 0, 1927, 1047));
        let serre = joue(from, to, std::time::Duration::from_millis(15));
        let large = joue(from, to, std::time::Duration::from_millis(60));
        // Au bout de 120 ms : huit pas d'un côté, deux de l'autre.
        assert!(
            (serre[8].2 - large[2].2).abs() <= 2,
            "{serre:?} / {large:?}"
        );
        // Et les deux arrivent, l'un en plus d'images que l'autre.
        assert!(serre.len() > large.len() * 2);
        for passe in [&serre, &large] {
            assert!(within(*passe.last().unwrap(), to, NEAR_ENOUGH));
        }
    }

    #[test]
    fn without_a_button_the_floor_is_still_a_real_size() {
        // Aucun bouton veut dire aucune session, donc aucune forme à
        // tenir : il reste qu'une taille de zéro ferait diviser par zéro
        // plus loin.
        let (wide, high) = the_least_picture(None, (1920, 1080));
        assert!(wide >= 1 && high >= 1);
    }
}
