//! The picture of a session: a window of ours, inside ours.
//!
//! The player draws the far computer's screen into a window this program
//! makes, a child of the main window laid over the whole of its inside,
//! above the home canvas. The player's own thread draws; this window only
//! exists, takes the size it is given, and hears the keyboard and the
//! mouse, which it hands to the player as they come.
//!
//! Made when a session's way stands and kept hidden until the player says
//! its first picture is drawn: the opening screen is the home canvas under
//! it, and it stays in sight for as long as there is nothing better to
//! show. Destroyed when the session ends, after the player has let go of
//! it.
//!
//! One program and one thread for the window that has the keyboard and
//! the window the keyboard is read in. Everything that was hard about the
//! keyboard came from those being two programs: the picture could never
//! be the window at the front, the focus had to be handed across with the
//! two programs' input joined, and every switch was a keystroke typed at
//! another program in the hope that it listened. Here the focus is a call
//! on our own thread, and what is typed goes to the player as it is read.
//!
//! Keys travel by their place on the keyboard, the scan code, never by
//! their name: the key engraved A in France is the key engraved Q
//! elsewhere, and the far computer reads it with its own layout.
//!
//! The mouse has two ways. On a desktop (« Bureau ») the pointer is where
//! the hand puts it: its place over the picture is sent, and this
//! computer draws its own pointer in the shape the far one has, with no
//! round trip behind the hand. In a game (« Jeu ») what counts is
//! movement: it is read straight from the mouse, the pointer here is
//! hidden and shut in the middle of the picture, and the far computer
//! draws its own into the picture.

// A window only exists on Windows. What decides what a message means is
// arithmetic, compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use zyr_player::{Button, InputEvent};

use crate::app::App;

/// What this module files its journal lines under.
const TAG: &str = "video";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// The window, as the system knows it, or nought.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Whether it is on screen: from the player's first picture to the end of
/// the session.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Whether the mouse belongs to a game right now.
///
/// Kept here because this is where it is read: every movement of the
/// mouse over the picture asks it, and the pointer's shape does too.
static GAME: AtomicBool = AtomicBool::new(false);

/// The last place sent, so that a pointer standing still is not sent
/// again: Windows says the mouse moved for reasons of its own, a window
/// appearing under it among them.
static LAST_PLACE: AtomicU32 = AtomicU32::new(u32::MAX);

/// The window, or nought while there is none.
pub fn its_window() -> isize {
    ITS_WINDOW.load(Ordering::Relaxed)
}

/// Whether a session's picture is on screen.
pub fn shown() -> bool {
    SHOWN.load(Ordering::Relaxed) && its_window() != 0
}

/// Whether the mouse belongs to a game.
pub fn in_a_game() -> bool {
    GAME.load(Ordering::Relaxed)
}

/* ---- What a message means ------------------------------------------- */

/// Where a key sits, as a keystroke message carries it: the scan code in
/// bits 16 to 23 of its second word, the E0 prefix in bit 24.
fn the_place_of(with: isize) -> (u8, bool) {
    (((with >> 16) & 0xFF) as u8, (with >> 24) & 1 != 0)
}

/// Whether the key was already down, which is the keyboard repeating a
/// key held: bit 30.
fn held_before(with: isize) -> bool {
    (with >> 30) & 1 != 0
}

/// Whether Alt was held with it: bit 29, which the system sets on the
/// messages it calls a system key's.
fn with_alt(with: isize) -> bool {
    (with >> 29) & 1 != 0
}

/// The place Windows gives a key by its name, for the few keystrokes that
/// arrive without one: its answer carries E0 in its second byte.
fn from_its_name(code: u32) -> Option<(u8, bool)> {
    let scancode = (code & 0xFF) as u8;
    (scancode != 0).then_some((scancode, code & 0xFF00 == 0xE000))
}

/// Where a point of the window falls on the picture, from 0 to 65535
/// across it, the picture being `left, top, width, height` in the window.
///
/// Held to the picture: the bands the player leaves around a picture of
/// another shape are not part of the far screen, and a hand in them
/// points at its nearest edge. So does a hand dragging past the window,
/// which the window follows while a button is held.
fn on_the_picture(point: (i32, i32), picture: (i32, i32, u32, u32)) -> (u16, u16) {
    fn across(at: i32, from: i32, length: u32) -> u16 {
        if length <= 1 {
            return 0;
        }
        let last = i64::from(length) - 1;
        let offset = (i64::from(at) - i64::from(from)).clamp(0, last);
        (offset * i64::from(u16::MAX) / last) as u16
    }
    (
        across(point.0, picture.0, picture.2),
        across(point.1, picture.1, picture.3),
    )
}

/// What the mouse says of itself, straight from the device, in the flags
/// Windows writes it with (`RI_MOUSE_*`).
mod raw {
    pub const LEFT_DOWN: u16 = 0x0001;
    pub const LEFT_UP: u16 = 0x0002;
    pub const RIGHT_DOWN: u16 = 0x0004;
    pub const RIGHT_UP: u16 = 0x0008;
    pub const MIDDLE_DOWN: u16 = 0x0010;
    pub const MIDDLE_UP: u16 = 0x0020;
    pub const X1_DOWN: u16 = 0x0040;
    pub const X1_UP: u16 = 0x0080;
    pub const X2_DOWN: u16 = 0x0100;
    pub const X2_UP: u16 = 0x0200;
    pub const WHEEL: u16 = 0x0400;
    pub const WHEEL_ACROSS: u16 = 0x0800;
}

/// What one report of the mouse comes to for a game: the movement first,
/// then the buttons, then the wheel.
///
/// Only a movement counted from the last one is movement. A mouse that
/// reports where it is instead, which is what a remote desktop or a
/// virtual machine gives this computer, has no movement to give a game.
fn what_the_mouse_did(
    moved: (i32, i32),
    relative: bool,
    buttons: u16,
    data: u16,
) -> Vec<InputEvent> {
    let mut events = Vec::new();
    if relative && moved != (0, 0) {
        let short = |value: i32| value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        events.push(InputEvent::PointerBy {
            dx: short(moved.0),
            dy: short(moved.1),
        });
    }
    for (flag, button, down) in [
        (raw::LEFT_DOWN, Button::Left, true),
        (raw::LEFT_UP, Button::Left, false),
        (raw::RIGHT_DOWN, Button::Right, true),
        (raw::RIGHT_UP, Button::Right, false),
        (raw::MIDDLE_DOWN, Button::Middle, true),
        (raw::MIDDLE_UP, Button::Middle, false),
        (raw::X1_DOWN, Button::X1, true),
        (raw::X1_UP, Button::X1, false),
        (raw::X2_DOWN, Button::X2, true),
        (raw::X2_UP, Button::X2, false),
    ] {
        if buttons & flag != 0 {
            events.push(InputEvent::Button { button, down });
        }
    }
    // The wheel's turn is a signed count carried in an unsigned word.
    let turned = data as i16;
    if buttons & raw::WHEEL != 0 {
        events.push(InputEvent::Wheel {
            vertical: turned,
            horizontal: 0,
        });
    }
    if buttons & raw::WHEEL_ACROSS != 0 {
        events.push(InputEvent::Wheel {
            vertical: 0,
            horizontal: turned,
        });
    }
    events
}

/// Hands one event to the player of the session on screen, if there is
/// one.
fn send(event: InputEvent) {
    if let Some(player) = crate::session::player() {
        player.send(event);
    }
}

/* ---- The window ------------------------------------------------------- */

/// Makes the window, hidden, the size of the main window's inside, and
/// answers it for the player to draw into.
///
/// On the thread that owns the main window, since a window belongs to the
/// thread that made it; this waits for it there, and is called from the
/// thread that drives the session, which may wait.
pub fn open(app: &App) -> Result<isize, String> {
    let (said, heard) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = said.send(build());
    })?;
    heard
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "la fenêtre de ZyrDesk n'a pas répondu".to_string())?
}

/// Puts the picture on screen, the player having drawn its first one, and
/// gives it the keyboard; the floating button goes up over it in the same
/// turn rather than at the next round of its watch.
pub fn show(app: &App) {
    let shown = app.clone();
    let _ = app.run_on_main_thread(move || {
        bring_it_up();
        crate::floating::keep_up_with_the_picture(&shown);
    });
}

/// Takes the window down for good, the player having let go of it.
pub fn close(app: &App) {
    SHOWN.store(false, Ordering::Relaxed);
    GAME.store(false, Ordering::Relaxed);
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window != 0 {
        let _ = app.run_on_main_thread(move || tear_down(window));
    }
}

/// Gives the mouse to a game, or back to a desktop.
///
/// The movement of a game is read from the device itself, which takes the
/// window asking for it to be told, on the thread that owns the window.
pub fn play_a_game(app: &App, game: bool) {
    GAME.store(game, Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || {
        if shown() {
            listen_to_the_mouse(game);
        }
    });
}

#[cfg(windows)]
fn build() -> Result<isize, String> {
    use windows_sys::Win32::Foundation::{GetLastError, HWND, RECT};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, GetClientRect, RegisterClassW, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS,
    };

    let outer = crate::main_window::handle() as HWND;
    if outer.is_null() {
        return Err("la fenêtre de ZyrDesk n'est plus là".to_string());
    }
    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: our own window, whose rectangle is read into ours.
    unsafe { GetClientRect(outer, &mut inside) };
    let class_name: Vec<u16> = "ZyrDeskImage".encode_utf16().chain(Some(0)).collect();
    // SAFETY: a class declared once and a window built on it, on the
    // thread that pumps the main window's messages. A class declared
    // twice is refused with no other effect, which is every session after
    // the first: the answer is left unread.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            // No cursor of its own: which one shows over the picture is
            // decided at every move, see `WM_SETCURSOR` below.
            hCursor: std::ptr::null_mut(),
            // No background either: the player covers every pixel, and a
            // background painted by the system would flash under it at
            // every resize.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            // Hidden until the first picture. Clipped by its siblings,
            // as the home canvas is: the two share the same inside, and
            // neither may draw over the other.
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            inside.right.max(1),
            inside.bottom.max(1),
            outer,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        // SAFETY: no argument; the error of the call just above.
        let code = unsafe { GetLastError() };
        note(&format!(
            "fenêtre de l'image refusée par Windows (CreateWindowExW, erreur {code})"
        ));
        return Err(format!(
            "la fenêtre de l'image n'a pas pu s'ouvrir (erreur {code})"
        ));
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    LAST_PLACE.store(u32::MAX, Ordering::Relaxed);
    note(&format!(
        "fenêtre de l'image prête, {}x{} px",
        inside.right, inside.bottom
    ));
    Ok(window as isize)
}

#[cfg(not(windows))]
fn build() -> Result<isize, String> {
    Err("l'image ne s'affiche que sous Windows".to_string())
}

/// Shows the window above the home canvas, over the whole inside, and
/// hands it the keyboard when this program has it.
#[cfg(windows)]
fn bring_it_up() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOP, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
    };

    let window = its_window() as HWND;
    if window.is_null() {
        return;
    }
    let (width, height) = crate::main_window::inside();
    // A window down in the taskbar has no inside: the picture keeps its
    // size until the window comes back up, which resizes it.
    let sized = if width == 0 || height == 0 {
        SWP_NOSIZE
    } else {
        0
    };
    // SAFETY: our own window, on the thread that made it, put on top of
    // its siblings and shown at the size of its parent's inside.
    unsafe {
        SetWindowPos(
            window,
            HWND_TOP,
            0,
            0,
            width as i32,
            height as i32,
            SWP_SHOWWINDOW | SWP_NOACTIVATE | sized,
        )
    };
    SHOWN.store(true, Ordering::Relaxed);
    if in_a_game() {
        listen_to_the_mouse(true);
    }
    crate::system_keys::take_them();
    // The keyboard, when the person is still in this program. Handed to
    // a window of a program in the background, the focus would pull that
    // program's window forward, and a person reading something else while
    // the session opened would have it taken from under their eyes. Their
    // way back into our window gives the keyboard to the picture anyway.
    //
    // SAFETY: no argument, and a window of this thread given the focus.
    unsafe {
        if GetActiveWindow() == crate::main_window::handle() as HWND {
            SetFocus(window);
        }
    }
}

#[cfg(not(windows))]
fn bring_it_up() {}

/// Destroys the window on the thread that made it.
#[cfg(windows)]
fn tear_down(window: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

    listen_to_the_mouse(false);
    // SAFETY: a window of ours, destroyed on the thread that made it.
    unsafe { DestroyWindow(window as HWND) };
    // The keyboard goes back to the home canvas, through the main window,
    // which hands it on: the window that had it is gone.
    let home = crate::main_window::handle() as HWND;
    // SAFETY: our own window, given the focus on its own thread.
    unsafe {
        if !home.is_null() && GetActiveWindow() == home {
            SetFocus(home);
        }
    }
    note("fenêtre de l'image fermée");
}

#[cfg(not(windows))]
fn tear_down(_window: isize) {}

/// Takes the size of the main window's inside, as that window changes.
///
/// Called from inside the main window's own answer to its new size, so
/// the picture follows in the same step as its frame.
#[cfg(windows)]
pub fn fit(width: i32, height: i32) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
    };

    let window = its_window() as HWND;
    // A window put down in the taskbar has no inside: the picture keeps
    // the size it had, for when it comes back.
    if window.is_null() || width <= 0 || height <= 0 {
        return;
    }
    // SAFETY: our own window, resized on the thread that made it.
    unsafe {
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            0,
            0,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
        )
    };
}

#[cfg(not(windows))]
pub fn fit(_width: i32, _height: i32) {}

/// Where the picture is on the screen, as left, top, right and bottom in
/// real pixels, while it is on screen.
#[cfg(windows)]
pub fn where_it_is() -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    if !shown() {
        return None;
    }
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: our own window, whose rectangle is read into ours.
    if unsafe { GetWindowRect(its_window() as HWND, &mut place) } == 0 {
        return None;
    }
    Some((place.left, place.top, place.right, place.bottom))
        .filter(|(left, top, right, bottom)| right > left && bottom > top)
}

#[cfg(not(windows))]
pub fn where_it_is() -> Option<(i32, i32, i32, i32)> {
    None
}

/// Hands the keyboard to the picture, for the main window when it is
/// given it: the picture is its inside during a session.
///
/// Answers whether it did, so the main window knows whether to hand it to
/// the home canvas instead.
#[cfg(windows)]
pub fn take_the_keyboard() -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;

    if !shown() {
        return false;
    }
    // SAFETY: a window of this thread, given the focus.
    unsafe { SetFocus(its_window() as HWND) };
    true
}

#[cfg(not(windows))]
pub fn take_the_keyboard() -> bool {
    false
}

/// Says to the window that the far pointer changed shape.
///
/// The system only asks which pointer to draw when the pointer moves: one
/// standing still over a word that turns into a link would keep its old
/// shape until the hand moved.
#[cfg(windows)]
pub fn the_pointer_changed() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    let window = its_window() as HWND;
    if !window.is_null() {
        // SAFETY: a window of ours, and a message of ours.
        unsafe { PostMessageW(window, NEW_SHAPE, 0, 0) };
    }
}

#[cfg(not(windows))]
pub fn the_pointer_changed() {}

/// The message by which the pointer's new shape is announced.
#[cfg(windows)]
const NEW_SHAPE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

/// Asks for the mouse's own reports, for a game, or stops asking.
///
/// Addressed to this window and heard even when it does not have the
/// front, since it never can be the window at the front: it is a child of
/// ours. What is heard is only used while the pointer is over the picture
/// (see `WM_INPUT`), so a hand on the floating button or on the taskbar
/// does not drive the game.
#[cfg(windows)]
fn listen_to_the_mouse(yes: bool) {
    use windows_sys::Win32::Foundation::{GetLastError, HWND};
    use windows_sys::Win32::UI::Input::{
        RAWINPUTDEVICE, RIDEV_INPUTSINK, RIDEV_REMOVE, RegisterRawInputDevices,
    };

    static LISTENING: AtomicBool = AtomicBool::new(false);
    if LISTENING.load(Ordering::Relaxed) == yes {
        return;
    }
    let window = its_window() as HWND;
    if yes && window.is_null() {
        return;
    }
    // Page 1, usage 2: the generic desktop's mouse. Taken back with no
    // window named, which is what the system asks for a removal.
    let device = RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: 2,
        dwFlags: if yes { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
        hwndTarget: if yes { window } else { std::ptr::null_mut() },
    };
    // SAFETY: one description of ours, of the size the call is told.
    let done = unsafe {
        RegisterRawInputDevices(&device, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32)
    } != 0;
    if !done {
        // SAFETY: no argument; the error of the call just above.
        let code = unsafe { GetLastError() };
        note(&format!(
            "souris de jeu {} refusée par Windows (RegisterRawInputDevices, erreur {code})",
            if yes { "demandée" } else { "rendue" }
        ));
        return;
    }
    LISTENING.store(yes, Ordering::Relaxed);
    note(if yes {
        "souris lue sur l'appareil, pour un jeu"
    } else {
        "souris rendue au bureau"
    });
}

#[cfg(not(windows))]
fn listen_to_the_mouse(_yes: bool) {}

/// SAFETY: called by the system on the thread that made this window, with
/// the arguments it documents.
#[cfg(windows)]
unsafe extern "system" fn answer(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Graphics::Gdi::ValidateRect;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetCapture, GetFocus, ReleaseCapture, SetCapture, SetFocus, VK_F4,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTCLIENT, WM_ERASEBKGND, WM_INPUT, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
        WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SIZE,
        WM_SYSCHAR, WM_SYSDEADCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
        XBUTTON1,
    };

    match message {
        // The player covers every pixel: nothing to erase and nothing to
        // paint. Told the window is painted all the same, or the system
        // would keep asking.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            // SAFETY: our own window, all of it declared painted.
            unsafe { ValidateRect(window, std::ptr::null()) };
            0
        }
        // The player's swap chain follows the window, at its size in real
        // pixels.
        WM_SIZE => {
            let (width, height) = ((with & 0xFFFF) as u32, ((with >> 16) & 0xFFFF) as u32);
            if width > 0
                && height > 0
                && let Some(player) = crate::session::player()
            {
                player.resize(width, height);
            }
            0
        }
        // The keyboard has come to the picture: the hook on the system's
        // keys is laid again, newest of the chain, see `system_keys`, and
        // the pointer goes back where the session keeps it.
        WM_SETFOCUS => {
            crate::system_keys::lay_again();
            if let Some(app) = crate::main_window::program() {
                crate::floating::keep_up_with_the_picture(&app);
            }
            0
        }
        // And gone: whatever was held is let go of over there, since the
        // releases will come to somebody else, and the pointer is given
        // back to the desk, which is where the person has gone.
        WM_KILLFOCUS => {
            if let Some(player) = crate::session::player() {
                player.release_everything();
            }
            crate::picture::shut_the_pointer_in(crate::picture::Cage::Free);
            // SAFETY: no argument; the mouse is only let go if ours.
            unsafe {
                if GetCapture() == window {
                    ReleaseCapture();
                }
            }
            0
        }
        // Keys, and system keys, which are never handed to the system
        // during a session: Alt alone would open our window's menu, F10
        // the same, Alt+Space its system menu. The far computer is the one
        // they are for.
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // Alt+F4 with the keyboard shared closes the session, as the
            // cross does: it is this computer's key while the switch says
            // the system's keys are its own.
            if message == WM_SYSKEYDOWN
                && holding == usize::from(VK_F4)
                && with_alt(with)
                && !crate::system_keys::immersive()
            {
                if let Some(app) = crate::main_window::program() {
                    note("Alt+F4 sur l'image, le clavier étant partagé : la session se termine");
                    crate::session::end_it(&app);
                }
                return 0;
            }
            key(holding, with, true);
            0
        }
        WM_KEYUP | WM_SYSKEYUP => {
            key(holding, with, false);
            0
        }
        WM_SYSCHAR | WM_SYSDEADCHAR => 0,
        WM_MOUSEMOVE => {
            if !in_a_game() {
                pointer_at(with);
            }
            0
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_LBUTTONUP
        | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
            let (button, down) = match message {
                WM_LBUTTONDOWN => (Button::Left, true),
                WM_LBUTTONUP => (Button::Left, false),
                WM_RBUTTONDOWN => (Button::Right, true),
                WM_RBUTTONUP => (Button::Right, false),
                WM_MBUTTONDOWN => (Button::Middle, true),
                WM_MBUTTONUP => (Button::Middle, false),
                // Which of the two side buttons is in the high word.
                _ => (
                    if ((holding >> 16) & 0xFFFF) as u16 == XBUTTON1 {
                        Button::X1
                    } else {
                        Button::X2
                    },
                    message == WM_XBUTTONDOWN,
                ),
            };
            // SAFETY: our own window, on its own thread. The keyboard
            // comes with a click, as it does to any window clicked; the
            // mouse is held for as long as a button is, so that a drag
            // that leaves the picture still ends over there.
            unsafe {
                if down && GetFocus() != window {
                    SetFocus(window);
                }
                if down {
                    SetCapture(window);
                } else if no_button_left(holding) && GetCapture() == window {
                    ReleaseCapture();
                }
            }
            // A game reads its buttons from the device, with its
            // movement.
            if !in_a_game() {
                pointer_at(with);
                send(InputEvent::Button { button, down });
            }
            // The side buttons are answered « done », which is what the
            // system asks of whoever takes them.
            if matches!(message, WM_XBUTTONDOWN | WM_XBUTTONUP) {
                1
            } else {
                0
            }
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            if !in_a_game() {
                let turned = ((holding >> 16) & 0xFFFF) as u16 as i16;
                send(if message == WM_MOUSEWHEEL {
                    InputEvent::Wheel {
                        vertical: turned,
                        horizontal: 0,
                    }
                } else {
                    InputEvent::Wheel {
                        vertical: 0,
                        horizontal: turned,
                    }
                });
            }
            0
        }
        WM_INPUT => {
            if in_a_game() && the_pointer_is_over(window) {
                for event in read_the_mouse(with) {
                    send(event);
                }
            }
            // Handed on in every case: the system cleans up after the
            // report there.
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
        WM_SETCURSOR if (with & 0xFFFF) as u32 == HTCLIENT => {
            put_on_the_pointer();
            1
        }
        NEW_SHAPE => {
            if the_pointer_is_over(window) {
                put_on_the_pointer();
            }
            0
        }
        // SAFETY: the system's own answer to everything else.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// Whether a button message leaves no button down, from what it says of
/// all of them.
#[cfg(windows)]
fn no_button_left(holding: usize) -> bool {
    use windows_sys::Win32::System::SystemServices::{
        MK_LBUTTON, MK_MBUTTON, MK_RBUTTON, MK_XBUTTON1, MK_XBUTTON2,
    };

    holding as u32 & (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON | MK_XBUTTON1 | MK_XBUTTON2) == 0
}

/// One keystroke, to the player.
#[cfg(windows)]
fn key(named: usize, with: isize, down: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, VK_LWIN, VK_RWIN,
    };

    // The Windows keys are this computer's while the keyboard is shared:
    // the Start menu opens here, and must not open over there as well.
    if (named == usize::from(VK_LWIN) || named == usize::from(VK_RWIN))
        && !crate::system_keys::immersive()
    {
        return;
    }
    let (scancode, extended) = match the_place_of(with) {
        (0, _) => {
            // A keystroke typed by a program rather than a keyboard can
            // come with its name alone: its place is asked of the layout.
            // SAFETY: a plain translation, of a key name.
            let code = unsafe { MapVirtualKeyW(named as u32, MAPVK_VK_TO_VSC_EX) };
            match from_its_name(code) {
                Some(place) => place,
                None => return,
            }
        }
        place => place,
    };
    let Some(player) = crate::session::player() else {
        return;
    };
    if down && held_before(with) {
        player.send_repeat(scancode, extended);
    } else {
        player.send(InputEvent::Key {
            scancode,
            extended,
            down,
        });
    }
}

/// Where the pointer is over the picture, to the player.
#[cfg(windows)]
fn pointer_at(with: isize) {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let Some(player) = crate::session::player() else {
        return;
    };
    // Signed: a pointer held by a drag reads negative left of the window.
    let point = (
        (with & 0xFFFF) as u16 as i16 as i32,
        ((with >> 16) & 0xFFFF) as u16 as i16 as i32,
    );
    let picture = player.picture_rect().unwrap_or_else(|| {
        let mut inside = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: our own window, whose rectangle is read into ours.
        unsafe { GetClientRect(its_window() as HWND, &mut inside) };
        (
            0,
            0,
            inside.right.max(0) as u32,
            inside.bottom.max(0) as u32,
        )
    });
    let (x, y) = on_the_picture(point, picture);
    let packed = (u32::from(x) << 16) | u32::from(y);
    if LAST_PLACE.swap(packed, Ordering::Relaxed) == packed {
        return;
    }
    player.send(InputEvent::PointerAt { x, y });
}

/// Whether the pointer stands over that window, and not over the floating
/// button, the taskbar or another screen.
#[cfg(windows)]
fn the_pointer_is_over(window: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, WindowFromPoint};

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: a point of ours, filled in by the system, then asked about.
    unsafe { GetCursorPos(&mut point) != 0 && WindowFromPoint(point) == window }
}

/// What the mouse reported, as the events a game is sent.
#[cfg(windows)]
fn read_the_mouse(with: isize) -> Vec<InputEvent> {
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWINPUT, RAWINPUTHEADER, RID_INPUT,
        RIM_TYPEMOUSE,
    };

    // SAFETY: a plain block of numbers, filled in below.
    let mut report: RAWINPUT = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    // SAFETY: the report the system named in this message, read into a
    // block of ours whose size is given; a mouse's report fits in it.
    let read = unsafe {
        GetRawInputData(
            with as HRAWINPUT,
            RID_INPUT,
            (&raw mut report).cast(),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || report.header.dwType != RIM_TYPEMOUSE {
        return Vec::new();
    }
    // SAFETY: the report is a mouse's, as its header says.
    let (mouse, buttons) = unsafe {
        let mouse = report.data.mouse;
        (mouse, mouse.Anonymous.Anonymous)
    };
    what_the_mouse_did(
        (mouse.lLastX, mouse.lLastY),
        mouse.usFlags & MOUSE_MOVE_ABSOLUTE == 0,
        buttons.usButtonFlags,
        buttons.usButtonData,
    )
}

/// Draws the pointer over the picture: the far computer's shape on a
/// desktop, none at all in a game or while the far computer draws its own
/// into the picture.
#[cfg(windows)]
fn put_on_the_pointer() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL,
        IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_WAIT, LoadCursorW, SetCursor,
    };
    use zyr_proto::session::Pointer;

    let shape = if in_a_game() {
        None
    } else {
        match crate::pointer::the_far_shape() {
            Pointer::Arrow => Some(IDC_ARROW),
            Pointer::Text => Some(IDC_IBEAM),
            Pointer::Hand => Some(IDC_HAND),
            Pointer::Wait => Some(IDC_WAIT),
            Pointer::WaitingArrow => Some(IDC_APPSTARTING),
            Pointer::Cross => Some(IDC_CROSS),
            Pointer::SizeAcross => Some(IDC_SIZEWE),
            Pointer::SizeDown => Some(IDC_SIZENS),
            Pointer::SizeFalling => Some(IDC_SIZENWSE),
            Pointer::SizeRising => Some(IDC_SIZENESW),
            Pointer::SizeAll => Some(IDC_SIZEALL),
            Pointer::Refused => Some(IDC_NO),
            Pointer::Theirs => None,
        }
    };
    // SAFETY: one of the system's own pointers, which are never freed,
    // or none; set from the thread of the window the pointer is over.
    unsafe {
        SetCursor(match shape {
            Some(named) => LoadCursorW(std::ptr::null_mut(), named),
            None => std::ptr::null_mut(),
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keystroke's second word, as Windows writes it.
    fn keystroke(scancode: u8, extended: bool, repeated: bool, alt: bool) -> isize {
        (1 | (isize::from(scancode) << 16))
            | (isize::from(extended) << 24)
            | (isize::from(alt) << 29)
            | (isize::from(repeated) << 30)
    }

    #[test]
    fn a_key_travels_by_where_it_sits() {
        // The A of an AZERTY keyboard is the Q of a QWERTY one: 0x10, the
        // place, is what goes, never the letter.
        assert_eq!(
            the_place_of(keystroke(0x10, false, false, false)),
            (0x10, false)
        );
        // Right Ctrl and the arrows carry the E0 prefix, left Ctrl none.
        assert_eq!(
            the_place_of(keystroke(0x1D, true, false, false)),
            (0x1D, true)
        );
        assert_eq!(
            the_place_of(keystroke(0x1D, false, false, false)),
            (0x1D, false)
        );
        assert_eq!(
            the_place_of(keystroke(0x4B, true, true, false)),
            (0x4B, true)
        );
        // A key going up carries bits 30 and 31: they are not part of the
        // place.
        let up = keystroke(0x1E, false, true, false) | (1 << 31);
        assert_eq!(the_place_of(up), (0x1E, false));
    }

    #[test]
    fn a_key_held_down_is_told_apart_from_a_new_press() {
        assert!(!held_before(keystroke(0x1E, false, false, false)));
        assert!(held_before(keystroke(0x1E, false, true, false)));
        assert!(with_alt(keystroke(0x3E, false, false, true)));
        assert!(!with_alt(keystroke(0x3E, false, false, false)));
    }

    #[test]
    fn a_key_known_by_its_name_alone_gets_its_place_from_the_layout() {
        assert_eq!(from_its_name(0x1E), Some((0x1E, false)));
        assert_eq!(from_its_name(0xE01D), Some((0x1D, true)));
        // No place at all is no key to send.
        assert_eq!(from_its_name(0), None);
    }

    #[test]
    fn the_corners_of_the_picture_are_the_corners_of_the_far_screen() {
        let whole = (0, 0, 1920, 1080);
        assert_eq!(on_the_picture((0, 0), whole), (0, 0));
        assert_eq!(on_the_picture((1919, 1079), whole), (u16::MAX, u16::MAX));
        let (x, y) = on_the_picture((960, 540), whole);
        assert!(x.abs_diff(u16::MAX / 2) <= 32, "{x}");
        assert!(y.abs_diff(u16::MAX / 2) <= 32, "{y}");
    }

    #[test]
    fn the_bands_around_a_picture_point_at_its_nearest_edge() {
        // A 16:10 picture in a 16:9 window: bands left and right.
        let picture = (96, 0, 1728, 1080);
        assert_eq!(on_the_picture((96, 0), picture), (0, 0));
        assert_eq!(on_the_picture((10, 500), picture).0, 0);
        assert_eq!(on_the_picture((1900, 500), picture).0, u16::MAX);
        assert_eq!(on_the_picture((1823, 1079), picture), (u16::MAX, u16::MAX));
        // And a drag carried past the window, where a point reads
        // negative, stays on the picture.
        assert_eq!(on_the_picture((-300, -40), picture), (0, 0));
        assert_eq!(on_the_picture((5000, 5000), picture), (u16::MAX, u16::MAX));
    }

    #[test]
    fn a_picture_of_no_size_points_nowhere_rather_than_dividing_by_nought() {
        assert_eq!(on_the_picture((10, 10), (0, 0, 0, 0)), (0, 0));
        assert_eq!(on_the_picture((10, 10), (0, 0, 1, 1)), (0, 0));
    }

    #[test]
    fn a_game_hears_movement_then_buttons_then_the_wheel() {
        let events = what_the_mouse_did(
            (5, -3),
            true,
            raw::LEFT_DOWN | raw::X2_UP | raw::WHEEL,
            (-120i16) as u16,
        );
        assert_eq!(
            events,
            vec![
                InputEvent::PointerBy { dx: 5, dy: -3 },
                InputEvent::Button {
                    button: Button::Left,
                    down: true
                },
                InputEvent::Button {
                    button: Button::X2,
                    down: false
                },
                InputEvent::Wheel {
                    vertical: -120,
                    horizontal: 0
                },
            ]
        );
        assert_eq!(
            what_the_mouse_did((0, 0), true, raw::WHEEL_ACROSS, 240),
            vec![InputEvent::Wheel {
                vertical: 0,
                horizontal: 240
            }]
        );
    }

    #[test]
    fn a_mouse_that_says_where_it_is_gives_a_game_no_movement() {
        // What a remote desktop or a virtual machine hands this computer.
        assert!(what_the_mouse_did((30_000, 20_000), false, 0, 0).is_empty());
        // A report of nothing is nothing.
        assert!(what_the_mouse_did((0, 0), true, 0, 0).is_empty());
        // And a jump beyond what a report carries is held to it.
        assert_eq!(
            what_the_mouse_did((100_000, -100_000), true, 0, 0),
            vec![InputEvent::PointerBy {
                dx: i16::MAX,
                dy: i16::MIN
            }]
        );
    }
}
