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

/// Set while the window is spread over the whole screen.
///
/// Held rather than read from the window, because the two places that
/// need it are asked at moments when the window cannot answer: the
/// system asks what the frame is going to be while the window is still
/// the size it was, and the compositor is told how to draw the corners
/// before the window has moved. The one door in and out of full screen
/// writes it, so it is right before either question is asked.
static WHOLE_SCREEN: AtomicBool = AtomicBool::new(false);

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
    // A session can end with a hand still on an edge, or with the
    // picture still held inside our window, and a player that dies
    // outright goes through none of the tidying up: left standing, these
    // silently switch off holding the window to the picture's shape for
    // the rest of the program's life. The style kept beside the second
    // one belonged to a window that is gone, so there is nothing to give
    // back and only the latch to put down.
    DRAGGED.store(false, Ordering::Relaxed);
    CARRIED.store(0, Ordering::Relaxed);
    FOCUS_TOLD.store(0, Ordering::Relaxed);
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
    // Written down before the window moves, not after. Taking the screen
    // is what makes the system ask what the frame should be, and the
    // answer depends on this: asked with the old value, the window comes
    // back with a frame it should not have.
    let was = WHOLE_SCREEN.swap(whole, Ordering::Relaxed);
    window.set_fullscreen(whole).map_err(|e| e.to_string())?;
    if was != whole {
        no_frame_on_the_whole_screen(app);
    }
    fit(app);
    // Taking the screen activates our window, which the toolkit does on
    // purpose and cannot be asked not to, and our own page takes the
    // focus inside it. The picture loses the keyboard that way. Handed
    // straight back.
    the_keyboard_back(app);
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
    if a_gesture_is_running() {
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
        HWND_TOP, IsWindowVisible, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
    };

    // SAFETY: a window this program took in hand, read only.
    let up = unsafe { IsWindowVisible(engine) } != 0;

    // The one moment in a session where the picture can be taken into
    // our window without a soul seeing it: it has not been shown yet.
    //
    // The crossing between the two ways of reading its numbers is about
    // a millisecond and a half, and nothing shortens it further; taken
    // in at the first gesture, as it was, that millisecond and a half
    // fell on a window standing in plain sight and was drawn about one
    // time in eleven. It was drawn: the journal caught the picture at
    // (594, 278) where (297, 139) had been asked for, twice the corner
    // of our inside to the pixel, once, at the first gesture, which is
    // exactly what was reported. Done here it falls inside the moment
    // the session appears, on a window with nothing on the screen to
    // read wrongly.
    if CARRIED.load(Ordering::Relaxed) == 0 && !up {
        carry_the_picture(home, engine);
    }

    // Carried as our window's own child, the picture has no place of its
    // own to be put at: it is drawn wherever its parent is, which is the
    // point of carrying it. Only its size still has to follow, and only
    // when our inside really changes size, which a carry across the desk
    // never does and an order to maximise does exactly once.
    if CARRIED.load(Ordering::Relaxed) != 0 {
        let started = std::time::Instant::now();
        let same_size = where_it_stands(engine).is_some_and(|(left, top, right, bottom)| {
            (right - left, bottom - top) == (width, height)
        });
        if !same_size || !up {
            // SAFETY: a window this program took in hand, put over the
            // whole of its parent's inside.
            //
            // Nothing of what was drawn is carried over into the new
            // size, for the same reason it is not on the other road
            // through here and which was forgotten on this one: the
            // system would otherwise copy the old picture into the
            // corner of the new frame and leave it sitting there until
            // the player draws again. The player draws thirty-seven
            // times a second, so that is up to twenty-seven
            // milliseconds of the far computer's screen at the wrong
            // size in the corner of the right one, which is one hop,
            // seen sometimes and not always. It is asked for exactly
            // once per gesture, so it costs nothing to ask.
            //
            // And not asked for the top either: it was put there when
            // it was taken in, nothing has come between the two since,
            // and asking again has the compositor take the whole stack
            // of windows apart for a window that has not moved in it.
            //
            // And shown, on the one laying where it is not up yet, which
            // is the first of a session. It is taken into our window
            // just above while it is still hidden, so this is the call
            // that puts the session on the screen at all.
            unsafe {
                SetWindowPos(
                    engine,
                    std::ptr::null_mut(),
                    0,
                    0,
                    width,
                    height,
                    SWP_NOACTIVATE
                        | SWP_NOZORDER
                        | SWP_NOCOPYBITS
                        | if up { 0 } else { SWP_SHOWWINDOW },
                )
            };
            LAID_WHILE_CARRIED.fetch_add(1, Ordering::Relaxed);
        }
        let laid = started.elapsed();
        // The player throws away the size it is given while it is still
        // clearing its queue at start-up, and it is told again on the
        // first laying it answers quickly to; see `say_the_size_again`.
        // That has to happen on this road too now, and it did not
        // before, the picture only ever being carried after the session
        // had settled. It is not a nicety: the journal has caught the
        // player drawing a hundred and fifty-five pixels short of its
        // own window and this is what put it right.
        if laid > A_FRAME {
            WAS_BUSY.store(true, Ordering::Relaxed);
        } else if !a_gesture_is_running() {
            say_the_size_again(engine, (width, height));
        }
        // And the two bottom corners cut to the curve of the frame, as
        // they are on the other road through here and were not on this
        // one. The cut is dropped when a gesture starts, since a shape
        // is the size the window had when it was given and a window
        // growing under one is clipped to where it used to end; nothing
        // put it back once the picture stopped taking the other road,
        // so the session was left square-cornered inside a rounded
        // frame for the rest of its life.
        let shaped = std::time::Instant::now();
        if !a_gesture_is_running() {
            round_the_bottom(home, engine, width, height);
        }
        let shaped = shaped.elapsed();
        let buttoned = std::time::Instant::now();
        crate::floating::lay_the_button((corner.0, corner.1, corner.0 + width, corner.1 + height));
        if DRAGGED.load(Ordering::Relaxed) {
            let buttoned = buttoned.elapsed();
            Cost::add(&LAYING, laid + shaped + buttoned);
            Cost::add(&PICTURE, laid);
            Cost::add(&BUTTON, buttoned);
        }
        return;
    }

    // This road is only ever taken before the picture has been taken
    // into our window, which is the first laying of a session, and by a
    // hand on an edge in the rare case where the system refused to take
    // it in at all. So what is counted and what is put off until the
    // window settles are the same question here, and a hand answers it.
    let dragged = DRAGGED.load(Ordering::Relaxed);
    let moving = dragged;

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

    // Shown and brought to the top only when it is not on screen, which
    // is once, when the session starts. Asked at every step of a carry,
    // as it was, both of them make the compositor take the whole stack
    // of windows apart and put it back together again for a window that
    // had not moved in the stack at all: the picture was pulled out and
    // pushed back sixty times a second, and the edge the window was
    // heading for flickered on every one of them.
    //
    // Nothing else keeps it up there anyway. It belongs to our window,
    // so the system never lets another window come between the two.
    let shown = if up { SWP_NOZORDER } else { SWP_SHOWWINDOW };

    // Already exactly there: asking again costs a wait on another
    // program for nothing. The window it belongs to is only ever put
    // right once per move now, before ours moves, so the laying that
    // follows the move has nothing left to do.
    if !(same_size && same_place && up) {
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
                SWP_NOACTIVATE | shown | moved_only | stays,
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

/// Whether the hand that took the window took it by an edge rather than
/// by the title bar.
///
/// The system says which of the two it is before either has begun, and
/// nothing after that does: the drag messages are the same for a window
/// being carried and a window being stretched, and so are the changes of
/// size they carry. It matters because one of those two changes of size
/// is not the hand's doing at all. A window carried against an edge of
/// the screen is snapped there by the system, which is « agrandir » by
/// another road, and everything this file does about a hand on an edge
/// is wrong for it.
static BY_AN_EDGE: AtomicBool = AtomicBool::new(false);

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
    //
    // Which it was is asked of the system, and not worked out from
    // whether the size ever changed: it changes at the end of a carry
    // too, when the window is dropped against an edge of the screen and
    // snapped there, and that gesture was being written down as a
    // resize by a corner nobody had touched.
    if !BY_AN_EDGE.load(Ordering::Relaxed) {
        let ending = if CARRIED.load(Ordering::Relaxed) == 0 {
            ""
        } else if RESIZED.load(Ordering::Relaxed) {
            " (image portée par la fenêtre, fini en ancrage)"
        } else {
            " (image portée par la fenêtre)"
        };
        crate::journal::note(&format!(
            "déplacement{ending} : {steps} pas ; poser {laying:.0} ms (pire {worst:.1}), \
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

/// Size of the screen this window sits on, in real pixels.
///
/// Real pixels and not the ones a page is laid out with: a screen at a
/// hundred and fifty per cent reports two thirds of what it draws, and
/// asking the far computer for two thirds of a screen is exactly the
/// mistake this measurement exists to prevent.
///
/// The screen the window is on rather than the main one, because that is
/// the screen the picture will be shown on.
#[cfg(windows)]
pub fn the_screen_of_this_computer(app: &AppHandle) -> Option<(u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };

    let home = home_window(app)?;
    let mut about: MONITORINFO = unsafe { std::mem::zeroed() };
    about.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: our own window, and the slot is ours with its size written
    // in it as the call requires.
    let screen = unsafe {
        let monitor = MonitorFromWindow(home, MONITOR_DEFAULTTONEAREST);
        (GetMonitorInfoW(monitor, &mut about) != 0).then_some(about.rcMonitor)
    }?;
    let across = u32::try_from(screen.right - screen.left).ok()?;
    let down = u32::try_from(screen.bottom - screen.top).ok()?;
    (across > 0 && down > 0).then_some((across, down))
}

#[cfg(not(windows))]
pub fn the_screen_of_this_computer(_app: &AppHandle) -> Option<(u32, u32)> {
    None
}

/// How much the system is drawing this window's page bigger than life.
///
/// Not used to decide anything, only to explain a measurement that
/// surprises: a screen reported at two thirds of its size is a window
/// that was asked the question in the wrong units, and that shows here.
#[cfg(windows)]
fn the_magnification(app: &AppHandle) -> u32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let Some(home) = home_window(app) else {
        return 100;
    };
    // SAFETY: our own window, read only.
    unsafe { GetDpiForWindow(home) }.max(96) * 100 / 96
}

#[cfg(not(windows))]
fn the_magnification(_app: &AppHandle) -> u32 {
    100
}

/// Says what screen the picture is about to land on, what was asked of
/// the far computer, and which of the two decided.
///
/// The one comparison that decides how sharp a session can possibly
/// look. A picture asked for smaller than the screen it lands on is
/// stretched here, and nothing stretched puts back a pixel that was
/// never sent, so the moment that number is settled is the moment to
/// write it down.
pub fn tell_what_is_asked_for(
    app: &AppHandle,
    screen: Option<(u32, u32)>,
    asked: zyr_proto::session::Asked,
    settings: &zyr_proto::session::SessionSettings,
) {
    let (wide, high) = (settings.width, settings.height);
    let seen = match screen {
        Some((across, down)) => format!(
            "écran de cet ordinateur : {across}x{down} pixels réels, agrandissement {} %",
            the_magnification(app)
        ),
        None => "écran de cet ordinateur : pas mesurable, taille courante supposée".to_string(),
    };
    let why = match screen {
        Some(measured) if measured == (wide, high) => {
            "l'écran est demandé entier, un pixel envoyé pour un pixel affiché".to_string()
        }
        Some((across, down)) => format!(
            "taille choisie à la main ({asked}) : {:.2} fois moins large et {:.2} fois moins haut que l'écran, donc autant de détail en moins et l'image est étirée à l'arrivée",
            f64::from(across) / f64::from(wide),
            f64::from(down) / f64::from(high),
        ),
        None => format!("taille demandée : {asked}"),
    };
    crate::journal::note(&format!(
        "{seen} ; image demandée au loin en {wide}x{high} à {} Mb/s en {}, {why}",
        settings.bitrate_kbps / 1000,
        settings.codec,
    ));
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

/// Gives the keyboard back to the picture and draws our title bar to
/// match, on the thread that owns both windows.
///
/// The front is not the road, and this is the one thing about a session
/// worth having straight. The picture is carried as a child of our
/// window for the whole of a session, and a child window is never the
/// window at the front: the system hands the front to the top of the
/// family, which is ours. Asking for the front to go to the picture
/// therefore does nothing but activate our own window again, which is
/// where it already was, and the journal shows exactly that, session
/// after session: the front reads « à ZyrDesk » from the moment the
/// picture is taken in and never once reads « à l'image » again.
///
/// What carries the keyboard is the other road entirely: this program
/// joins its input to the engine's, and hands the focus over inside the
/// pair. That is the road, and this is where it is asked for again after
/// every gesture that took the focus away.
///
/// The title bar first and the keyboard after it, which is an order and
/// not a preference. Drawing the bar goes through our own window's
/// handler, and that handler is where a window is told it has been
/// activated; done the other way about, the picture was handed the
/// keyboard and had it taken straight back off by our own page, and the
/// session went silent the moment the floating menu was touched.
///
/// And asked for through a message rather than by calling the drawing
/// straight. That drawing hands the message on to whatever was handling
/// this window before us, which is only a thing that can be done from
/// inside a handler; called from outside one, it reaches into a window
/// mid-nothing and the toolkit's own handler acts on an activation that
/// never happened.
#[cfg(windows)]
fn give_the_keyboard_to_the_picture(app: &AppHandle) -> bool {
    if let Some(home) = home_window(app) {
        light_the_bar(home);
    }
    // Said again after it, and read: the message above puts the keyboard
    // back as part of its work, but says nothing about where it landed,
    // and nothing may be typed at a picture that does not have it.
    the_keyboard_to_the_picture()
}

/// Gives the keyboard back to the picture, asked from anywhere in the
/// program.
///
/// For the floating button above all, which is the one thing that takes
/// the keyboard away without the system noticing it left. That window is
/// marked so a click on it never makes it the active one, and it never
/// does; but its page takes the focus inside this program all the same,
/// and the focus is what the keyboard follows. The session then goes
/// deaf while looking exactly as it should.
///
/// Handed to the thread that draws, since callers include a watch that
/// runs on a worker thread of its own, and handing another program's
/// window the focus is only possible from the thread whose input was
/// joined to that program's.
#[cfg(windows)]
pub fn the_keyboard_back(app: &AppHandle) {
    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        give_the_keyboard_to_the_picture(&asked);
    });
}

#[cfg(not(windows))]
pub fn the_keyboard_back(_app: &AppHandle) {}

/// Gives the session back the keyboard the floating menu took from it.
///
/// The front is not asked for back, and was, twice. Closing that menu
/// drops the front on the desktop with nothing having taken it, and taking
/// it back from there is a road Windows keeps shut: it hands the front
/// only to the program that already holds it or that received the last
/// keystroke, and by then the shell has had both. The journal wrote the
/// answer in one word, « refusé », on every session it was tried on.
///
/// What a fallen front costs is the session's Alt+Tab, for as long as this
/// program is the one taking those keys; the answer to that is not here
/// but in who takes them, and it is the engine in the mode that asks it to
/// ([D43](../../docs/DECISIONS.md)).
#[cfg(windows)]
pub fn the_session_back(app: &AppHandle) {
    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        give_the_keyboard_to_the_picture(&asked);
    });
}

#[cfg(not(windows))]
pub fn the_session_back(_app: &AppHandle) {}

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
        // Where the front is going, said by the system as it moves it.
        // It takes a thread of its own here and gives it back below.
        watch_the_front();
        offer_a_picture_of_the_session(home, true);
        round_the_window(home, true);
        light_the_bar(home);
        // A session can open straight onto the whole screen, in which
        // case the window took it before this handler was on it and the
        // system asked about the frame with nobody there to answer.
        // Asked again now, with the handler in place.
        if WHOLE_SCREEN.load(Ordering::Relaxed) {
            no_frame_on_the_whole_screen(&asked);
        }
        tell_the_frame(home);
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
        // Before the handler comes off, and not after: a session can end
        // in the middle of a gesture, with the picture still held inside
        // our window and a timer due to let go of it. Taken off first,
        // that timer would ring into a window that no longer listens,
        // and the picture would stay a child of ours for good, which
        // silently switches off everything that asks whether the window
        // is in the middle of a gesture.
        let_the_picture_go(home);
        // And out of our window for good, which nothing else does now:
        // it is taken in at the first gesture of a session and kept
        // there, so this is the one place it comes back out.
        put_the_picture_back(home);
        // SAFETY: same window, same thread and same handler as were put
        // on it.
        unsafe { RemoveWindowSubclass(home, Some(lit), LIT) };
        stop_watching_the_front();
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

/// The watch that follows the front, held for the length of a session;
/// see `crate::hook`.
#[cfg(windows)]
static FRONT_WATCH: crate::hook::Held = crate::hook::Held::new();

/// Has the system say where the front is every time it moves it.
///
/// Nothing decides on this any more: what is left of it is the one line
/// that names the program which has just stepped in front of a session,
/// and that line is worth a thread on its own. A session that stops
/// answering is almost always a session something else is in front of,
/// and nothing else anywhere says what.
///
/// A watch of this kind is not a hook on the road a keystroke travels:
/// it is handed to this thread through its messages, after the fact, so
/// a slow answer here delays nothing of the computer's. What it costs is
/// one thread and one line in the journal per move of the front.
#[cfg(windows)]
fn watch_the_front() {
    let Some(taken) = FRONT_WATCH.hold(put_it_on, take_it_off) else {
        return;
    };
    if !taken {
        crate::journal::note("premier plan non suivi : Windows a refusé le crochet des fenêtres");
    }
}

/// Asks the system to say so, on the thread that is to be told.
#[cfg(windows)]
fn put_it_on() -> isize {
    use windows_sys::Win32::UI::Accessibility::SetWinEventHook;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    };

    // SAFETY: one event is asked for, of every program, and the callback
    // is a plain function of this one; no module is named, which is what
    // is wanted for a watch that is told after the fact rather than run
    // inside another program.
    unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(the_front_moved),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        ) as isize
    }
}

/// And stops asking.
#[cfg(windows)]
fn take_it_off(hook: isize) {
    use windows_sys::Win32::UI::Accessibility::UnhookWinEvent;

    // SAFETY: the watch that thread put on, given back once, from the
    // thread that owns it.
    unsafe { UnhookWinEvent(hook as _) };
}

/// The session being over, the front is nobody's business here again.
#[cfg(windows)]
fn stop_watching_the_front() {
    FRONT_WATCH.let_go();
}

/// Said by the system every time the front moves, to any window of any
/// program.
#[cfg(windows)]
unsafe extern "system" fn the_front_moved(
    _watch: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    window: windows_sys::Win32::Foundation::HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _when: u32,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CHILDID_SELF, EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW,
    };

    // The window itself and not a part of one: the same event is said of
    // the pieces a window is made of, and none of those is the front.
    if event != EVENT_SYSTEM_FOREGROUND || object != OBJID_WINDOW || child != CHILDID_SELF as i32 {
        return;
    }
    say_where_the_front_went(window);
}

/// Says in the journal where the front went.
///
/// Nothing acts on this and nothing should: where the front is, when it
/// is needed, is asked of the system on the spot. This is a line to read
/// afterwards and nothing else.
///
/// The window is the system's own word for it rather than a fresh ask:
/// asked for, it can come back as no window at all for the moment one is
/// handing it to the next.
///
/// Only during a session, since outside one nobody cares where the front
/// is and the journal would fill with every window of the day.
#[cfg(windows)]
fn say_where_the_front_went(window: windows_sys::Win32::Foundation::HWND) {
    if CARRIED.load(Ordering::Relaxed) == 0 {
        return;
    }
    crate::journal::note(&format!(
        "le premier plan passe {}",
        in_these_words(whose_window(window), window)
    ));
}

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

    let lit = who_holds_the_front() != Front::Elsewhere;
    if BAR_LIT.swap(lit, Ordering::Relaxed) != lit {
        crate::journal::note(&format!(
            "barre de titre {} : le premier plan est {}",
            if lit { "active" } else { "inactive" },
            the_front_in_words()
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
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY: no argument, and a null answer is one of the answers.
    whose_window(unsafe { GetForegroundWindow() })
}

/// Whose window that one is, of the same three.
#[cfg(windows)]
fn whose_window(window: windows_sys::Win32::Foundation::HWND) -> Front {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    if window.is_null() {
        return Front::Elsewhere;
    }
    let mut owner = 0u32;
    // SAFETY: the window is the caller's and the slot is ours; a window
    // that has gone answers nought, which is nobody.
    unsafe { GetWindowThreadProcessId(window, &mut owner) };
    // SAFETY: no argument.
    if owner == unsafe { GetCurrentProcessId() } {
        Front::Ours
    } else if owner == PLAYER.load(Ordering::Relaxed) {
        Front::ThePlayer
    } else {
        Front::Elsewhere
    }
}

/// Who holds the front, in the words the journal uses everywhere.
///
/// One phrasing for the whole program: which of the three it is turns up
/// in the title bar's own line, in every ask for the front to come back,
/// and in every shortcut refused for want of it, and reading those lines
/// against one another is the only way this is ever untangled.
#[cfg(windows)]
pub(crate) fn the_front_in_words() -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY: no argument, and a null answer is one of the answers.
    let front = unsafe { GetForegroundWindow() };
    in_these_words(whose_window(front), front)
}

/// The same words, about a window already found.
///
/// Taken rather than asked for again, since the two asks race and the
/// words then name a window other than the one they are about.
#[cfg(windows)]
fn in_these_words(whose: Front, window: windows_sys::Win32::Foundation::HWND) -> String {
    match whose {
        Front::Ours => "à ZyrDesk".to_string(),
        Front::ThePlayer => "à l'image".to_string(),
        // Named rather than merely spotted: nothing here expects a third
        // window to ever hold the front during a session, so a report of
        // it happening is worth more with a name on it than without one.
        Front::Elsewhere => format!("ailleurs : {}", describe(window)),
    }
}

/// Names that window, for the one answer `whose_window` cannot explain on
/// its own: `Front::Elsewhere` says it belongs to neither this program nor
/// the player, and stops there. Nothing here expects a third window to
/// ever hold the front during a session, so when one does, a name is
/// worth more than the bare fact.
#[cfg(windows)]
fn describe(window: windows_sys::Win32::Foundation::HWND) -> String {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId};

    if window.is_null() {
        return "aucune fenêtre au premier plan".to_string();
    }
    let mut pid = 0u32;
    // SAFETY: the window is the caller's, and the slot is ours.
    unsafe { GetWindowThreadProcessId(window, &mut pid) };

    let mut buffer = [0u16; 128];
    // SAFETY: the window is the caller's, and the buffer is ours with its
    // length given.
    let read = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    let title = String::from_utf16_lossy(&buffer[..read.max(0) as usize]);

    let exe = 'named: {
        // SAFETY: the pid comes from a live window, and a refusal is one
        // of the answers.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            break 'named String::new();
        }
        let mut path = [0u16; 260];
        let mut length = path.len() as u32;
        // SAFETY: the handle is live, and the buffer and its length are
        // ours.
        let named =
            unsafe { QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut length) };
        // SAFETY: the handle came from the call above and is closed once.
        unsafe { CloseHandle(handle) };
        if named == 0 {
            break 'named String::new();
        }
        String::from_utf16_lossy(&path[..length as usize])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_string()
    };

    match (exe.is_empty(), title.is_empty()) {
        (false, false) => format!("processus {pid} ({exe}), titre « {title} »"),
        (false, true) => format!("processus {pid} ({exe})"),
        (true, false) => format!("processus {pid}, titre « {title} »"),
        (true, true) => format!("processus {pid}"),
    }
}

/// Asks the compositor to round our window's corners, and takes the ask
/// back at the end of the session.
///
/// Three answers and not two, because « round them if that suits the
/// window » turned out not to cover the one case where it matters. The
/// compositor squares a window it maximised itself, and that is the case
/// the first version of this was written against; a window spread over
/// the screen by being moved and resized to it is an ordinary window as
/// far as the compositor is concerned, and it rounds it. Two bites out
/// of the far computer's screen, in a mode whose whole point is that
/// there is nothing but the screen.
///
/// So a window covering the screen is told to square them outright, and
/// its border is turned off with them: the compositor draws one around
/// every window it rounds, and on the screen's own edge that border is a
/// pale line with the session pushed off it.
#[cfg(windows)]
fn round_the_window(home: windows_sys::Win32::Foundation::HWND, may: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_DEFAULT, DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmSetWindowAttribute,
    };

    // Kept, so that taking the screen or giving it back can ask again
    // without having to know whether a session is running.
    ROUNDS_WANTED.store(may, Ordering::Relaxed);
    let whole = WHOLE_SCREEN.load(Ordering::Relaxed);
    let how: i32 = match (may, whole) {
        (_, true) => DWMWCP_DONOTROUND,
        (true, false) => DWMWCP_ROUND,
        (false, false) => DWMWCP_DEFAULT,
    };
    let edge: u32 = if whole {
        DWMWA_COLOR_NONE
    } else {
        DWMWA_COLOR_DEFAULT
    };
    // SAFETY: our own window, two attributes made to be set, and both
    // values are ours, of the size each call is told.
    let answer = unsafe {
        let corners = DwmSetWindowAttribute(
            home,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const how).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        DwmSetWindowAttribute(
            home,
            DWMWA_BORDER_COLOR as u32,
            (&raw const edge).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        corners
    };
    crate::journal::note(&format!(
        "coins de la fenêtre : {} demandés, le compositeur a répondu {answer:#x}",
        match (may, whole) {
            (_, true) => "droits, sans bordure, la fenêtre couvrant l'écran",
            (true, false) => "arrondis",
            (false, false) => "au choix du système",
        }
    ));
}

/// Takes the frame off the window while it covers the screen, and hands
/// it back when it comes down.
///
/// A window covering the screen keeps the frame of an ordinary one: the
/// system reserves a strip along the top and the sides for a border, and
/// what is inside the window starts below it. That strip is the pale
/// line along the top of a full screen session, and the reason the far
/// computer's picture sits a few pixels lower than it should.
///
/// The strip is not removed by asking; it is removed by answering the
/// question that creates it, which the handler below does. All that is
/// wanted here is for the system to ask it again, which it only does
/// when told the frame may have changed.
#[cfg(windows)]
fn no_frame_on_the_whole_screen(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetWindowPos,
    };

    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(home) = home_window(&asked) else {
            return;
        };
        // SAFETY: our own window, from the thread that owns it, and
        // nothing is moved, resized or reordered.
        unsafe {
            SetWindowPos(
                home,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        };
        round_the_window(home, ROUNDS_WANTED.load(Ordering::Relaxed));
        tell_the_frame(home);
    });
}

/// What was last asked of the corners, so the ask survives a change of
/// screen without this file having to ask whether a session is running.
#[cfg(windows)]
static ROUNDS_WANTED: AtomicBool = AtomicBool::new(false);

#[cfg(not(windows))]
fn no_frame_on_the_whole_screen(_app: &AppHandle) {}

/// Measures what the window and its inside really came to, against the
/// screen they are on.
///
/// The one measurement that settles a pale line along an edge: a window
/// whose inside is smaller than itself has a frame, and the difference
/// is where the line is. Nothing else can be read from a photograph.
#[cfg(windows)]
fn tell_the_frame(home: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect};

    let mut frame = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut inside = frame;
    let mut about: MONITORINFO = unsafe { std::mem::zeroed() };
    about.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: our own window, and all three slots are ours, the last one
    // with its size written in it as the call requires.
    let read = unsafe {
        let monitor = MonitorFromWindow(home, MONITOR_DEFAULTTONEAREST);
        GetWindowRect(home, &mut frame) != 0
            && GetClientRect(home, &mut inside) != 0
            && GetMonitorInfoW(monitor, &mut about) != 0
    };
    if !read {
        return;
    }
    let screen = about.rcMonitor;
    crate::journal::note(&format!(
        "cadre de la fenêtre : écran {}x{} en ({}, {}), fenêtre {}x{} en ({}, {}), intérieur {}x{} ; \
         il reste {} px de cadre en largeur et {} px en hauteur",
        screen.right - screen.left,
        screen.bottom - screen.top,
        screen.left,
        screen.top,
        frame.right - frame.left,
        frame.bottom - frame.top,
        frame.left,
        frame.top,
        inside.right - inside.left,
        inside.bottom - inside.top,
        (frame.right - frame.left) - (inside.right - inside.left),
        (frame.bottom - frame.top) - (inside.bottom - inside.top),
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

/* ---- Porter l'image le temps d'un déplacement ----------------------- */

/// Style the engine's window wore before our window took it in as a
/// child for the length of a move, and zero the rest of the time.
///
/// A move is played by the system: it moves our window, and the picture
/// has to be sent after it, one call per step. However close together
/// the two calls land, they are two transactions, and the compositor
/// draws whatever is standing when it wakes: every so often that is one
/// window moved and the other not, and what shows in the difference is
/// a strip of the page behind the picture, along the very edge the
/// window is heading for. No ordering of the calls closes that gap; it
/// only decides which side of it the strip falls on, which is what
/// three reorderings proved in a row.
///
/// One transaction is the only fix, and the one transaction the system
/// offers is the window tree itself: a child has no place of its own on
/// the screen, it is drawn where its parent is, inside the parent's own
/// composition, so the system cannot show the one without the other.
/// Carried that way, a step of the move costs nothing at all.
///
/// Only for the length of the gesture. A child is never the window at
/// the front, and the engine asks the system for the keyboard, which
/// goes to the front: held as a child for good, the session would lose
/// it. During a drag the front is already ours, taken by the click on
/// the title bar, so the gesture is exactly the stretch in which being
/// a child costs nothing.
static CARRIED: AtomicIsize = AtomicIsize::new(0);

/// How many times the picture has been given a new size since our window
/// took it in.
///
/// One per gesture is the whole point of handing the move back to the
/// system: the window changes size once, the picture inside it changes
/// size once, and the compositor stretches the pair from the old
/// rectangle to the new one on its own clock. Anything above one means
/// something is resizing things frame by frame again.
///
/// Counted from the moment the picture is taken in and not from the
/// moment the wait is armed, which is what it was and which made it
/// useless: the wait is armed after the order has been handed over, and
/// the one resize of the whole gesture happens inside that handing over.
/// It read zero every time.
static LAID_WHILE_CARRIED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Whether the window is in the middle of a gesture, by a hand or by an
/// order.
///
/// The tidying that follows a resize waits for this to be false: cutting
/// the picture's corners costs a shape built and a window redrawn, and
/// the two corners it buys are two nobody is looking at while the window
/// is on the move.
///
/// It used to answer « a hand is on it, or the picture is held inside
/// it », which was the same thing back when the picture was only ever
/// held for the length of a gesture. It is held for a whole session now,
/// so that reading answered yes from the first laying to the last and
/// the tidying never ran again: the far computer's screen kept square
/// corners inside a rounded frame, and the size the player is told again
/// at start-up was never told. Both were written to run here and neither
/// did.
///
/// A hand on the window, or a move the system is playing out, and
/// nothing else.
fn a_gesture_is_running() -> bool {
    #[cfg(windows)]
    let played = HOLDING.with_borrow(Option::is_some);
    #[cfg(not(windows))]
    let played = false;
    DRAGGED.load(Ordering::Relaxed) || played
}

/// Whether there is a picture riding inside our window at all.
///
/// What the two messages that carry a coming size ask before doing
/// anything: outside a session there is nothing to lay, and before the
/// picture has been taken in there is a hand to answer to instead.
fn the_picture_rides() -> bool {
    DRAGGED.load(Ordering::Relaxed) || CARRIED.load(Ordering::Relaxed) != 0
}

/// Thread the picture belongs to while our window is holding it, and
/// zero the rest of the time; see `hand_the_keyboard_over`.
static ITS_THREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Puts the keyboard where the picture is, or takes it back.
///
/// A window held inside another is never the window at the front, and
/// the keyboard goes to the front. Held for a whole session, as it now
/// is, the session lost the keyboard for that whole session: tried,
/// reported, and the reason this exists.
///
/// It cannot be answered by passing the keys along by hand. The keys do
/// not reach us to begin with, the web view under the picture taking
/// them first, and even forwarded they would arrive without the state
/// that says which of shift, control and alt are down, since that state
/// belongs to the thread that really received them. Every modifier and
/// every shortcut would be wrong.
///
/// So the two threads are given one input state between them, which is
/// what this call is for, and the focus is then handed across it. The
/// price is that a thread which stops answering holds the other one's
/// input with it. Both of these already wait on each other several
/// times a second, so neither can stop answering without the session
/// stopping anyway.
#[cfg(windows)]
fn hand_the_keyboard_over(engine: windows_sys::Win32::Foundation::HWND, over: bool) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    // SAFETY: no argument, and a window this program took in hand with
    // no slot asked for.
    let ours = unsafe { GetCurrentThreadId() };
    let theirs = if over {
        unsafe { GetWindowThreadProcessId(engine, std::ptr::null_mut()) }
    } else {
        ITS_THREAD.load(Ordering::Relaxed)
    };
    if theirs == 0 || theirs == ours {
        return;
    }
    // SAFETY: two threads of this machine, and the same pair is handed
    // back later. Detaching a pair that was never attached answers
    // false and does nothing.
    let joined = unsafe { AttachThreadInput(ours, theirs, i32::from(over)) } != 0;
    ITS_THREAD.store(if over && joined { theirs } else { 0 }, Ordering::Relaxed);
    if !over {
        crate::journal::note("clavier repris à la session");
        return;
    }
    // SAFETY: a window this program took in hand, which the call above
    // has just put on the same input as ours, which is what lets the
    // focus be given to a window of another program at all.
    //
    // What the ask answers is the window that held the focus before it,
    // and nought is one of its ordinary answers: nobody held it. Read as a
    // refusal, as it was, a focus perfectly well given read as one denied.
    // Where the focus really went is asked after, of the shared input this
    // very call joined, exactly as `the_keyboard_to_the_picture` does.
    let took = joined
        && unsafe {
            SetFocus(engine);
            GetFocus()
        } == engine;
    crate::journal::note(if took {
        "clavier confié à la session : les deux programmes partagent une entrée, l'image a le focus"
    } else if joined {
        "clavier confié à la session mais le focus n'a pas atterri sur l'image"
    } else {
        "clavier non confié : les deux programmes n'ont pas pu partager une entrée"
    });
}

/// Gives the keyboard back to the picture, the two programs already
/// sharing one input; see `hand_the_keyboard_over`.
///
/// Asked again and not only once, because the focus does not stay put.
/// Ending a gesture activates the window that was dragged, which is
/// ours, and our own web view takes the focus inside it; so does
/// clicking anywhere on our window. The picture then holds nothing, and
/// the session is deaf while looking exactly as it should. Sharing the
/// input is what makes this call able to reach a window of another
/// program at all, and it is cheap, so it is made at every moment that
/// can have taken the focus away.
///
/// Says where the focus really went the first time it moves, since
/// « asked for » and « granted » have already been two different things
/// once here. And answers it to the caller as well as to the journal:
/// nothing may be typed at the picture that does not have it.
///
/// Asked from the thread that owns these windows, always. Handing
/// another program's window the focus is only possible from the thread
/// whose input this program joined to that program's, and reading the
/// focus back from any other thread reads that other thread's, which is
/// nobody's.
#[cfg(windows)]
pub(crate) fn the_keyboard_to_the_picture() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};

    if CARRIED.load(Ordering::Relaxed) == 0 {
        return false;
    }
    let Some(engine) = the_engines_window() else {
        return false;
    };
    // SAFETY: a window this program took in hand, on the same input as
    // ours, and a reading of where the focus of that shared input is.
    let landed = unsafe {
        SetFocus(engine);
        GetFocus()
    };
    let told = if landed == engine { 1 } else { 2 };
    if FOCUS_TOLD.swap(told, Ordering::Relaxed) != told {
        // The front alongside the focus, always, and not only when the
        // focus was refused. The two come apart on purpose here, and a
        // session where the keyboard reads as « bien à la session » while
        // the front is on our own window is exactly the state where the
        // far computer stops answering Alt+Tab: the line has to be able
        // to show that, or it reads as everything being well.
        crate::journal::note(&format!(
            "{} ; le premier plan est {}",
            if told == 1 {
                "le clavier est bien à la session"
            } else {
                "le clavier n'est pas à la session : le focus a été refusé à l'image"
            },
            the_front_in_words()
        ));
    }
    told == 1
}

/// What the journal last said about where the keyboard went, so it is
/// said when it changes and not once a second.
static FOCUS_TOLD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Takes the picture in as a child of our window, so a move carries
/// both as one; see `CARRIED`.
#[cfg(windows)]
fn carry_the_picture(
    home: windows_sys::Win32::Foundation::HWND,
    engine: windows_sys::Win32::Foundation::HWND,
) {
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetWindowLongPtrW, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOSENDCHANGING, SetParent, SetWindowLongPtrW, SetWindowPos, WS_CHILD, WS_POPUP,
        WS_VISIBLE,
    };

    if CARRIED.load(Ordering::Relaxed) != 0 {
        return;
    }
    let Some((_, width, height)) = the_inside_of(home) else {
        return;
    };

    // SAFETY: a window this program took in hand. The style is read,
    // amended for the child it is about to be, and put back whole if
    // the system refuses the adoption.
    unsafe {
        // Everything the picture wears except whether it is shown. That
        // one is not ours to put back: the picture is taken in before it
        // has ever been shown, so what is read here is a hidden window's
        // style, and giving it back whole later hid a session that had
        // been on the screen for minutes. What showed instead was our
        // own page, and the picture came straight back in through the
        // road that takes in a picture nobody can see, so the two took
        // turns for as long as the hand kept resizing.
        let style = GetWindowLongPtrW(engine, GWL_STYLE) & !(WS_VISIBLE as isize);
        // The parent first and the style after it, which is the ordering
        // that shortens the wrong reading rather than the work; the
        // other end of a gesture is put back the same way about, and for
        // the same reason.
        //
        // The picture is already this window's to begin with, owned by
        // it rather than part of it, so naming the same window as its
        // parent while it still wears the style of one of its own moves
        // nothing and is read no differently. What flips the reading is
        // the style, and that call is ours alone and costs nothing,
        // while this one has to reach the player's program and wait for
        // it. Done the other way about, as it was, the wrong reading
        // covered that wait as well.
        //
        // The system can refuse: two windows that do not measure the
        // screen the same way cannot be family. Told apart from the
        // legitimate « no parent before » answer by the error slot, and
        // asked before anything of ours has been changed, so there is
        // nothing to put back.
        SetLastError(0);
        if SetParent(engine, home).is_null() && GetLastError() != 0 {
            let why = GetLastError();
            crate::journal::note(&format!(
                "l'image n'a pas pu être portée par la fenêtre ({why:#x}), déplacement pas à pas"
            ));
            return;
        }
        // From here the numbers it is wearing are read against our
        // inside and no longer against the screen, and they are still
        // its old ones: the crossing starts on this line.
        let crossing = std::time::Instant::now();
        SetWindowLongPtrW(
            engine,
            GWL_STYLE,
            (style & !(WS_POPUP as isize)) | WS_CHILD as isize,
        );
        let adopted = crossing.elapsed();
        // A child's place is counted inside its parent, and the system
        // does not recount it on adoption: put straight back over the
        // whole inside, above the web view, before anything is drawn.
        SetWindowPos(
            engine,
            HWND_TOP,
            0,
            0,
            width,
            height,
            SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_NOSENDCHANGING,
        );
        CARRIED.store(style, Ordering::Relaxed);
        // And the keyboard with it, which being a child costs.
        hand_the_keyboard_over(engine, true);
        // The same crossing as the one at the other end of a gesture,
        // and the same thing worth knowing about it: from the moment the
        // style becomes a child's, the numbers the picture is wearing
        // are read against our inside instead of against the screen, and
        // they are its old ones until the call above puts them right.
        // One call that must reach the player's program and wait for it
        // stands in between.
        //
        // This one falls at the very start of a gesture, on the click
        // itself, which is where « the picture hops once, just as it
        // grows » would fall. Counted rather than reasoned about: the
        // reasoning has been wrong twice.
        crate::journal::note(&format!(
            "image portée par la fenêtre : mauvaise lecture pendant {:.1} ms \
             (adoptée {:.1}, remise {:.1}) ; visée (0, 0, {width}, {height}), \
             obtenue {:?}",
            crossing.elapsed().as_secs_f64() * 1000.0,
            adopted.as_secs_f64() * 1000.0,
            (crossing.elapsed() - adopted).as_secs_f64() * 1000.0,
            where_it_stands(engine),
        ));
    }
}

/// Puts the picture at the front, and says whether Windows agreed.
///
/// For a picture that has just stopped being a child of our window and
/// nothing else. A child is never the window at the front, the system
/// giving the front to the head of a family and never to a member of it,
/// so this asked of a carried picture does not fail: it succeeds at
/// activating our own window, which is where the front already was, and
/// reads afterwards exactly like a picture that has it. That was believed
/// to be the road back to the keyboard for a whole round of fixes; the
/// road is the focus, which `the_keyboard_to_the_picture` hands over.
#[cfg(windows)]
fn the_front_to_the_picture(engine: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    // SAFETY: a window this program took in hand, and just handed back.
    let taken = unsafe { SetForegroundWindow(engine) } != 0;
    crate::journal::note(&format!(
        "premier plan redemandé pour l'image rendue à elle-même : Windows a {} ; il est {}",
        if taken { "accepté" } else { "refusé" },
        the_front_in_words()
    ));
}

/// Puts the picture back to the window of its own it ordinarily is,
/// owned rather than held, the gesture being over.
#[cfg(windows)]
fn put_the_picture_back(home: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SetParent, SetWindowLongPtrW,
        SetWindowPos, WS_VISIBLE,
    };

    let style = CARRIED.swap(0, Ordering::Relaxed);
    if style == 0 {
        return;
    }
    let Some(engine) = the_engines_window() else {
        return;
    };
    // The keyboard first, and before anything that can give up: it was
    // handed over on the strength of the picture being a child, and two
    // programs left sharing one input after a session has ended is one
    // of them waiting on a thread that no longer answers.
    hand_the_keyboard_over(engine, false);
    let Some((corner, width, height)) = the_inside_of(home) else {
        return;
    };
    // How long the picture spends wearing numbers that are read against
    // the wrong origin, which is the whole of this and the one thing no
    // reading of the result can show: what it ends up as is right every
    // time, and it is the crossing that flashes.
    //
    // A window keeps its numbers when it stops being a child, and the
    // screen starts reading them against a different origin: what meant
    // « the top left of our inside » comes to mean « the top left of the
    // screen ». One of the two readings is wrong for as long as the
    // crossing takes, and every call here that has to reach the player's
    // program and wait for it lengthens that. There were three of them,
    // and the wrong reading spanned the first two.
    //
    // SAFETY: a window this program took in hand, given the numbers it
    // must wear on its own, the style that makes them be read that way,
    // and then let out of the window.
    let crossing = std::time::Instant::now();
    unsafe {
        // The numbers first, while it is still a child, so that nothing
        // has to be put right afterwards. Put right afterwards, as it
        // was at first, the wrong reading is a picture standing at the
        // corner of the desktop at its full size, outside our window
        // altogether. Put right beforehand, the wrong reading is the
        // picture sitting that far down and to the right of our inside,
        // with a corner of the page showing where it no longer reaches;
        // a child is clipped by its parent, so that stays inside our
        // window whatever the numbers say. That is the difference
        // between the two mistakes and the reason for this order.
        SetWindowPos(
            engine,
            std::ptr::null_mut(),
            corner.0,
            corner.1,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSENDCHANGING,
        );
        let moved = crossing.elapsed();
        // The style next and the parent after it, which is the ordering
        // that shortens the wrong reading rather than the work. A window
        // that has lost the style of a child is read against the screen
        // from that moment, and this call is ours alone and costs
        // nothing, while letting it out of its parent has to reach the
        // other program. Done the other way about, as it was, the wrong
        // reading covered that wait as well.
        //
        // And it is only a question of when it ends, never of whether:
        // should the system hold off until the parent really goes, the
        // numbers are the arrival numbers by then either way.
        // With whether it is shown taken from the window itself rather
        // than from what it wore when it was taken in; see
        // `carry_the_picture`.
        SetWindowLongPtrW(
            engine,
            GWL_STYLE,
            style | (GetWindowLongPtrW(engine, GWL_STYLE) & WS_VISIBLE as isize),
        );
        let styled = crossing.elapsed();
        SetParent(engine, std::ptr::null_mut());
        // Owned again: owned is what has it come and go with our window
        // without being part of it.
        SetWindowLongPtrW(engine, GWLP_HWNDPARENT, home as isize);
        let out = crossing.elapsed();
        // Above our window again, and nothing moved: it is already where
        // it belongs, so this cannot be caught wearing anything wrong.
        SetWindowPos(
            engine,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        // The front with it, since it has just stopped being a child and
        // a child cannot hold the front. Left out, every gesture would
        // quietly cost the session its keyboard.
        //
        // Only when the front is still ours to give. The picture is held
        // for a moment after a gesture, and a person can spend that
        // moment clicking on something else entirely; taking the front
        // back from them then would be this program snatching the screen
        // out of their hands half a second after they left it.
        if who_holds_the_front() == Front::Ours {
            the_front_to_the_picture(engine);
        }
        crate::journal::note(&format!(
            "image rendue à elle-même : mauvaise lecture pendant {:.1} ms \
             (déplacée {:.1}, style {:.1}, sortie {:.1}, dessus {:.1}) ; \
             visée ({}, {}, {}, {}), obtenue {:?}",
            styled.as_secs_f64() * 1000.0,
            moved.as_secs_f64() * 1000.0,
            (styled - moved).as_secs_f64() * 1000.0,
            (out - styled).as_secs_f64() * 1000.0,
            (crossing.elapsed() - out).as_secs_f64() * 1000.0,
            corner.0,
            corner.1,
            corner.0 + width,
            corner.1 + height,
            where_it_stands(engine),
        ));
    }
}

/* ---- Agrandir et réduire, en laissant le système jouer -------------- */

/// How long the picture stays inside our window after an order to
/// maximise or come back down, and how long it stays inside afterwards
/// with nothing happening before it is handed back.
///
/// The system plays that move itself and does not say when it has
/// finished, and how long it takes is written nowhere. So this is a
/// margin over it rather than a measurement of it, and the margin is
/// free: a picture held a little too long is drawn exactly where it
/// belongs, since it is drawn wherever its parent is, while one let go
/// too early takes its own place on the screen at once and shows the
/// far computer's screen jump to full size with the frame catching up
/// behind it. That jump is the whole reason any of this exists, so the
/// two mistakes are not worth the same and this errs on the safe side.
///
/// The same margin then does a second job, which is why it outlasts the
/// move. Taking the picture in and handing it back are each a moment
/// where the numbers it wears are read against the wrong origin, and
/// each lasts one call out to the player's program, one to three
/// milliseconds against a screen that redraws every seventeen. Short,
/// but it was happening twice for every gesture, including every little
/// nudge of the window: thirty nudges in a row is sixty of them, and at
/// roughly one chance in ten of being drawn, several are seen. Held
/// across the gap between two gestures, a run of them costs one crossing
/// instead of sixty.
///
/// What it costs in exchange: a picture inside our window is not the
/// window at the front, so for this long after a gesture the keyboard
/// does not reach the far computer. The mouse still does, since a click
/// goes to whatever is under it. Asked about and thought a fair trade,
/// on the grounds that nobody drives the far computer with the hand that
/// is moving the window.
#[cfg(windows)]
const KEPT_INSIDE: std::time::Duration = std::time::Duration::from_millis(500);

/// Name the timer that ends the hold answers to.
#[cfg(windows)]
const LET_GO: usize = 2;

/// An order handed to the system, and what the journal will say about it
/// once the picture has been let go of.
#[cfg(windows)]
struct Holding {
    /// What the gesture was, so the line can name it. A snap against an
    /// edge of the screen is not an order and has no number of its own.
    what: &'static str,
    /// When it was handed over.
    began: std::time::Instant,
}

// Only ever touched from the thread that owns the window, inside its own
// message handler.
#[cfg(windows)]
thread_local! {
    static HOLDING: std::cell::RefCell<Option<Holding>> =
        const { std::cell::RefCell::new(None) };
}

/// Hands « agrandir » or « niveau inférieur » to the system, with the
/// picture tucked inside our window so that the two are one thing while
/// the system plays it.
///
/// The system animates this, and animates it far better than anything
/// done by hand here: it holds what the window looks like, stretches
/// that towards the new rectangle on the compositor, at the screen's own
/// rate, and shows what is really there at the end. The window itself
/// changes size once.
///
/// Played by hand instead, the same move meant changing the window's
/// size at every drawn frame, and each of those made the system throw
/// away the surface it draws that window into and allocate a bigger one,
/// nine megabytes of it once the window covers a screen, and redraw the
/// whole frame around it. The journal put three quarters of the cost of
/// a step there, on a chip with a hundred and twenty-eight megabytes to
/// itself, and the move could not hold sixty frames a second because of
/// it. Every step of that, and the curve, and the clock it was read off,
/// and the last pixels handed back to the system, existed only to work
/// around a move nobody had to play.
///
/// What stopped us handing it over in the first place is that the system
/// animates one window, and the picture is a window of its own: it took
/// its new size at once and the frame caught up behind it, which is the
/// two windows this whole file exists to hide. That is no longer true.
/// The picture goes in as a child of our window first, and a child has
/// no place of its own on the screen: it is drawn inside its parent's
/// own composition, so what the compositor stretches is the pair.
///
/// Answers false when there is nothing to hand over this way, and the
/// order goes to the system on its own.
#[cfg(windows)]
fn play_the_order(window: windows_sys::Win32::Foundation::HWND, order: usize) -> bool {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, SC_MAXIMIZE, WM_SYSCOMMAND};

    // A window down in the taskbar has no rectangle to leave from, and
    // coming back up from there is the system's own animation about an
    // icon, which has nothing to do with the picture.
    // SAFETY: our own window, read only.
    if unsafe { IsIconic(window) } != 0 {
        return false;
    }
    let Some(engine) = the_engines_window() else {
        return false;
    };
    carry_the_picture(window, engine);
    // Refused, and said so in the journal where it was refused. The
    // order still has to be carried out, and the system carries it out
    // better than we would; what it will not do is take the picture with
    // it, so it goes back to being two windows for that one gesture.
    if CARRIED.load(Ordering::Relaxed) == 0 {
        return false;
    }
    // The corners go for the length of it, as they do for a drag: a
    // shape is the size the window had when it was given, and a window
    // growing under one is clipped to where it used to end.
    let_the_corners_go(engine);

    LAID_WHILE_CARRIED.store(0, Ordering::Relaxed);
    // SAFETY: the order the person gave, at the window it was given to,
    // handed straight to the system's own handling since ours would only
    // take it back again.
    unsafe { DefSubclassProc(window, WM_SYSCOMMAND, order, 0) };
    // After the order and not before, so the wait covers the move and
    // not the handing over of it.
    keep_the_picture_inside(
        window,
        if order as u32 == SC_MAXIMIZE {
            "agrandissement"
        } else {
            "retour en fenêtre"
        },
    );
    true
}

/// Leaves the picture inside our window for as long as the system can
/// still be playing a move, and has it let go of afterwards.
///
/// The two gestures the system plays for us end up here: the order from
/// the title bar, and the window being snapped against an edge of the
/// screen by a hand that was only carrying it. Both animate, neither
/// says when it has finished.
#[cfg(windows)]
fn keep_the_picture_inside(window: windows_sys::Win32::Foundation::HWND, what: &'static str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetTimer;

    HOLDING.with_borrow_mut(|holding| {
        *holding = Some(Holding {
            what,
            began: std::time::Instant::now(),
        });
    });
    // SAFETY: our own window, from the thread that owns it. Setting a
    // timer that is already set only puts it back to the start, so two
    // gestures running into each other cannot leave two of them.
    unsafe { SetTimer(window, LET_GO, KEPT_INSIDE.as_millis() as u32, None) };
}

/// The system has had its time; the gesture is over and is written down.
///
/// The picture stays inside our window. It used to be handed back out
/// here, and that was the whole of the flash that outlived every other
/// fix: taking it in and handing it back are each a moment where the
/// numbers it wears are read against the wrong origin, about a
/// millisecond and a half against a screen that redraws every
/// seventeen. One in eleven is drawn, and there were two per gesture, so
/// a session of twenty gestures showed about four. The arithmetic
/// matched what was reported to the flash.
///
/// Nothing shortens that to nothing, so it is the count that had to go.
/// Taken in at the first gesture of a session and kept until the last,
/// there is one crossing where there were forty, and one crossing is
/// one chance in eleven of ever being seen at all.
#[cfg(windows)]
fn let_the_picture_go(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsZoomed, KillTimer};

    // SAFETY: our own window, from the thread that owns it.
    unsafe { KillTimer(window, LET_GO) };
    // The gesture activated our window, which took the keyboard off the
    // picture; it goes back.
    the_keyboard_to_the_picture();
    let Some(held) = HOLDING.with_borrow_mut(|holding| holding.take()) else {
        return;
    };
    // One line per gesture, and it has one job: to say which of the two
    // halves failed if the old fault comes back. « The picture jumped
    // and the frame followed » looks the same from the outside whether
    // the picture was put somewhere wrong, which these numbers show, or
    // whether the compositor stretched our window without stretching
    // what was inside it, which they cannot show but which is then the
    // only thing left. And the count of layings tells at a glance
    // whether the move really was handed over or is being played step by
    // step again behind our backs.
    // SAFETY: our own window, read only.
    crate::journal::note(&format!(
        "{} rendu au système : {} en {:.0} ms, image redimensionnée {} fois ; \
         fenêtre {:?}, cadre dessiné {:?}, image {:?}, dedans {:?}",
        held.what,
        if unsafe { IsZoomed(window) } != 0 {
            "agrandie"
        } else {
            "en fenêtre"
        },
        held.began.elapsed().as_secs_f64() * 1000.0,
        LAID_WHILE_CARRIED.load(Ordering::Relaxed),
        where_it_stands(window),
        the_drawn_frame_of(window),
        the_engines_window().and_then(where_it_stands),
        the_inside_of(window),
    ));
    // The shape was left alone while the window was moving; it has
    // stopped.
    if let Some(engine) = the_engines_window() {
        lay_it_out(window, engine);
    }
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
        NCCALCSIZE_PARAMS, PostMessageW, SC_MAXIMIZE, SC_MOVE, SC_RESTORE, SC_SIZE, WINDOWPOS,
        WM_ACTIVATEAPP, WM_DWMSENDICONICLIVEPREVIEWBITMAP, WM_DWMSENDICONICTHUMBNAIL,
        WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCACTIVATE, WM_NCCALCSIZE, WM_SYSCOMMAND, WM_TIMER,
        WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING,
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
        // A hand about to take the window, and the system saying which of
        // the two gestures it is before either has begun. Written down
        // here because nothing later says it: the drag messages that
        // follow are the same for both.
        WM_SYSCOMMAND if matches!((wparam & 0xFFF0) as u32, SC_MOVE | SC_SIZE) => {
            BY_AN_EDGE.store((wparam & 0xFFF0) as u32 == SC_SIZE, Ordering::Relaxed);
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
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
        // The system has had its time to play the move; the picture
        // goes back to being a window of its own.
        //
        // Unless a hand has taken the window since. Letting go under a
        // hand plays the rest of that gesture with two windows again,
        // which is the flicker all this exists to prevent, and the
        // gesture's own end already lets go of the picture or holds it
        // longer if it finishes in a snap. The wait comes back round
        // until then.
        WM_TIMER if wparam == LET_GO => {
            if !DRAGGED.load(Ordering::Relaxed) {
                let_the_picture_go(window);
            }
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
        // The front has settled; draw the bar the way it really stands,
        // and the keyboard goes back to the picture, this being the one
        // moment that can have taken it away.
        BAR => {
            draw_the_bar(window);
            the_keyboard_to_the_picture();
            0
        }
        // A window about to take a new size: the system says what it is
        // about to apply and takes back whatever is written there, before
        // anything moves.
        //
        // This message and not the sizing one of the drag loop, which was
        // answered here before and never arrived: the journal counted the
        // steps of a drag through this message's own echo while the shape
        // ran free.
        //
        // Answered for an order and not only for a hand, which is the
        // whole of one report. « Agrandir » was watched closely enough
        // to be described, and what showed was our own page, flashing
        // inside the window while it grew. It could hardly have been
        // anything else: the picture was given the size of the inside as
        // it stood when the gesture began, the system then made that
        // inside a screen wide, and the picture was only sent after it
        // through the message that says the window has already moved.
        // Between the two, the picture was the smaller of the two by the
        // whole of what the window had just gained, and a child does not
        // grow with its parent. The rule that answers this was written
        // during the work on dragging, and reads « the picture is never
        // the smaller of the two »; it was simply never asked when the
        // one doing the growing was the system.
        WM_WINDOWPOSCHANGING if the_picture_rides() => {
            // SAFETY: for this message the system passes a WINDOWPOS of
            // ours to read and amend, and it lives for the length of the
            // call.
            let wanted = unsafe { &mut *(lparam as *mut WINDOWPOS) };
            // Only a hand has a shape to hold. A window on its way to
            // what « agrandir » asks for is going to the size of a
            // screen, and the picture's own proportions have nothing to
            // say about it. Held here rather than corrected afterwards,
            // since corrected afterwards every step of a drag resized
            // the window twice.
            if DRAGGED.load(Ordering::Relaxed) {
                the_drag_keeps_the_shape(window, wanted);
            }
            // Handed on: what was written only becomes the window's size
            // in the system's own handling of this message.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        // The system working out what inside the window will have once
        // the size just settled is applied, which is the one place that
        // answer exists before anything has moved. Handed on first, so
        // that what comes back is the system's own answer and not a
        // guess of ours, and the picture laid on it while the window
        // still wears its old size.
        //
        // Guessed at, as it was for one commit, by taking the proposal
        // and subtracting what the frame costs today. That holds under a
        // hand, where the frame is the same before and after, and is
        // wrong for « agrandir », where the frame itself changes: the
        // picture was given a size that was nobody's, then given the
        // right one through the message that follows. Two sizes for one
        // gesture, the first of them wrong, and a picture wearing a size
        // its player has not drawn at yet is stretched by the compositor
        // to fill it. That is the far computer's screen appearing zoomed
        // for an instant, like a change of resolution that is over far
        // too quickly to be one.
        //
        // And on the whole screen, the answer is given rather than asked
        // for. The system reserves a strip along the top and the sides
        // of every ordinary window for a border, and inside starts below
        // it; a window covering the screen is still an ordinary window
        // to the system, so the strip lands on the screen's own edge.
        // That is the pale line along the top of a full screen session,
        // and the reason the far computer's picture sat a few pixels
        // below where it should. Answering that inside is the whole
        // window leaves no strip to draw and nothing to push down.
        WM_NCCALCSIZE
            if wparam != 0 && (the_picture_rides() || WHOLE_SCREEN.load(Ordering::Relaxed)) =>
        {
            let whole = WHOLE_SCREEN.load(Ordering::Relaxed);
            // SAFETY: the arguments the system handed in, untouched.
            // Left alone when the window covers the screen: what the
            // block already holds is the window itself, which is the
            // answer wanted, and zero is « that rectangle stands ».
            let answer = if whole {
                0
            } else {
                unsafe { DefSubclassProc(window, message, wparam, lparam) }
            };
            // SAFETY: for this message the system passes a block of ours
            // whose first rectangle it has just written the coming
            // inside into, in screen coordinates, and it lives for the
            // length of the call.
            let inside = unsafe { (*(lparam as *const NCCALCSIZE_PARAMS)).rgrc[0] };
            let (width, height) = (inside.right - inside.left, inside.bottom - inside.top);
            // Only when that leaves the picture the bigger of the two;
            // see `the_picture_leads`. Shrinking, it stays as it is and
            // follows once the window has moved.
            if width > 0
                && height > 0
                && let Some(engine) = the_engines_window()
                && the_picture_leads(window, (width, height))
            {
                lay_on(window, engine, (inside.left, inside.top), width, height);
            }
            answer
        }
        // A hand on the window. Which of the two gestures it is, moving
        // it or resizing it, is not said and is not asked here: the
        // corners are only in the way of one of them, and they come off
        // at the first step that changes the size rather than at the
        // first step at all. Taken off here, carrying the window across
        // the desk squared the picture's corners for the length of the
        // carry, over a frame that had kept its own.
        // A hand has taken the window, to carry it or to resize it;
        // which of the two is not said. The picture is taken in as a
        // child either way: a carry then costs nothing at all, and the
        // first step that turns out to change the size hands it back
        // (a child does not resize with its parent).
        WM_ENTERSIZEMOVE => {
            DRAGGED.store(true, Ordering::Relaxed);
            count_the_drag();
            LAID_WHILE_CARRIED.store(0, Ordering::Relaxed);
            if let Some(engine) = the_engines_window() {
                carry_the_picture(window, engine);
            }
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            DRAGGED.store(false, Ordering::Relaxed);
            tell_the_drag();
            // The picture stays inside for a moment rather than being
            // handed straight back, whatever the gesture was; see
            // `KEPT_INSIDE`. A hand let go on an edge of the screen is
            // the system about to snap the window there and animate it,
            // and handing the picture back now would have it take its
            // own place at once and jump while the frame is still on its
            // way. A hand let go anywhere else has nothing left to
            // finish, and the wait is there for the next gesture: a run
            // of little nudges then costs one crossing instead of two
            // per nudge.
            //
            // Nothing to hold at all when a hand on an edge has already
            // handed the picture back at its first step.
            if CARRIED.load(Ordering::Relaxed) != 0 {
                keep_the_picture_inside(
                    window,
                    if RESIZED.load(Ordering::Relaxed) {
                        "ancrage"
                    } else {
                        "déplacement"
                    },
                );
            } else {
                let_the_picture_go(window);
            }
            // SAFETY: the arguments the system handed in, untouched.
            let answer = unsafe { DefSubclassProc(window, message, wparam, lparam) };
            // The shape was left alone while the hand was moving; the
            // hand has stopped. Not while the picture is still held: the
            // timer above ends with the very same tidying up, and doing
            // it here as well would cut a shape for a size the window is
            // still on its way to.
            if CARRIED.load(Ordering::Relaxed) == 0
                && let Some(engine) = the_engines_window()
            {
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

/// Whether the picture should take a new size before our window takes
/// its own, or after it.
///
/// The two are two windows and two transactions, however tightly the
/// calls are written together, and the compositor draws whatever is
/// standing when it wakes. For a few milliseconds, which is what waiting
/// on the player costs, the two disagree, and there are only ever two
/// ways they can: either the picture is the bigger of the two and hangs
/// a little over the frame, or the frame is, and a strip of the page
/// behind the picture shows inside the window.
///
/// The first is barely visible. The second is a bright band where the
/// far computer's screen ought to be.
///
/// So the rule is not an order but a size: the picture is never the
/// smaller of the two. Growing, it goes first and overhangs; shrinking,
/// it waits and overhangs. Reversing the order outright would only walk
/// the band from one half of the gesture to the other, which is what
/// three reorderings proved during the work on dragging.
#[cfg(windows)]
fn the_picture_leads(home: windows_sys::Win32::Foundation::HWND, after: (i32, i32)) -> bool {
    the_inside_of(home).is_none_or(|(_, width, height)| after.0 > width || after.1 > height)
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
    // used to end. Off for the rest of the gesture, and cut again when
    // the hand stops.
    if !RESIZED.swap(true, Ordering::Relaxed)
        && let Some(engine) = the_engines_window()
    {
        let_the_corners_go(engine);
    }

    // Its size is moving and the hand is on the title bar, so the hand
    // is not what is moving it: the window has been carried against an
    // edge of the screen and the system is snapping it there, which is
    // « agrandir » wearing a different hat. Everything below is about a
    // hand on an edge and none of it applies.
    //
    // Held to the picture's proportions, the rectangle the system had
    // chosen stopped being the screen's, and the window landed at a size
    // of its own making, neither the one it had nor the one of the
    // screen, with the desktop showing beside it. And the picture was
    // handed back out of our window at the size it had before the snap,
    // which is why what showed inside was our own page and not the
    // session. Left alone, the system's rectangle is applied whole and
    // the picture rides inside our window as it does for the order.
    if !BY_AN_EDGE.load(Ordering::Relaxed) {
        return;
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
pub(crate) fn the_engines_window() -> Option<windows_sys::Win32::Foundation::HWND> {
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
pub(crate) fn the_front_in_words() -> String {
    "hors de Windows, où il n'y a pas de session".to_string()
}

#[cfg(not(windows))]
pub(crate) fn the_keyboard_to_the_picture() -> bool {
    false
}

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
    fn without_a_button_the_floor_is_still_a_real_size() {
        // Aucun bouton veut dire aucune session, donc aucune forme à
        // tenir : il reste qu'une taille de zéro ferait diviser par zéro
        // plus loin.
        let (wide, high) = the_least_picture(None, (1920, 1080));
        assert!(wide >= 1 && high >= 1);
    }
}
