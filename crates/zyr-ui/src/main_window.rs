//! ZyrDesk's window, the one the system puts a frame around.
//!
//! Made by this program and not by a toolkit. It was the last thing a
//! toolkit still handled for us, and it is the one that matters most: it
//! is **the same window** that carries the home screen and carries the
//! picture of a session, and everything delicate that `picture` does is
//! played out in the messages it receives.
//!
//! What this changes, plainly: the frame, full screen, the maximised state
//! and following the screen are written here, on one page, instead of
//! being reproduced by a layer aimed at something else. What `picture`
//! laid on top is still laid on top, exactly the same: a guard steps in
//! front of this window as it stepped in front of the other one.
//!
//! Lengths are counted in **page pixels** when they are written here, and
//! in real pixels everywhere else: `scale` makes the conversion, once,
//! when building and at each change of screen.

// A window is a thing of the system, and this product only opens one on
// Windows. Elsewhere, every answer is that of a window that does not
// exist, so that all the rest of the file stays compiled and checked.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use crate::app::App;

/// What this module files its journal lines under.
const TAG: &str = "window";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// How wide and how tall the window is when it opens, and what it never
/// goes below, in page pixels.
///
/// The floor is not a preference: it is the room needed for the
/// computers' cards to fit in a row and for the menu of a session to
/// have somewhere to open.
const OPENS_AT: (i32, i32) = (1060, 720);
const NEVER_SMALLER: (i32, i32) = (880, 600);

/// The window, as the system knows it.
static HANDLE: AtomicIsize = AtomicIsize::new(0);
/// Whether it takes the whole screen.
///
/// Remembered rather than read again from the window, because the places
/// that ask for it ask at moments when the window cannot answer: the
/// system asks what frame it will have while it is still the size it was,
/// and the compositor learns how to round its corners before it has
/// moved. The one door into and out of full screen writes it, so it is
/// right before either question is asked.
static FULL_SCREEN: AtomicBool = AtomicBool::new(false);

/// Where it was and what it looked like before taking the screen.
///
/// Both together: taking back its place without taking back its frame
/// would leave it without a title bar in the middle of the desktop.
static BEFORE_FULL_SCREEN: Mutex<Option<(isize, isize, [u8; PLACE])>> = Mutex::new(None);

/// The size of the block the system writes a window's place into.
///
/// Kept as bytes and not in its own type: this structure belongs to
/// Windows, and this file has nothing to read from it, only to give it
/// back as it was taken.
#[cfg(windows)]
const PLACE: usize =
    std::mem::size_of::<windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>();
#[cfg(not(windows))]
const PLACE: usize = 1;

/// The program, kept here because nothing hands one to a window of the
/// system: what happens to this one comes from the system, not from a
/// loop that would know whom to talk to.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

fn program() -> Option<App> {
    PROGRAM.lock().expect("programme de la fenêtre").clone()
}

/// The window, or zero as long as it is not open.
pub fn handle() -> isize {
    HANDLE.load(Ordering::Relaxed)
}

/// The name of its class, under which a second ZyrDesk finds it.
const CLASS_NAME: &str = "ZyrDesk";

/// The message by which a second ZyrDesk asks the one running to show
/// itself, rather than opening a second window.
#[cfg(windows)]
const SHOW_YOURSELF: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

/// Asks the window of the ZyrDesk already running to come
/// back.
#[cfg(windows)]
pub fn show_the_one_running() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

    let class_name: Vec<u16> = CLASS_NAME.encode_utf16().chain(Some(0)).collect();
    // SAFETY: a name that outlives the call, and a message that belongs
    // to us alone, posted to a window of our own class.
    unsafe {
        let already = FindWindowW(class_name.as_ptr(), std::ptr::null());
        if !already.is_null() {
            PostMessageW(already, SHOW_YOURSELF, 0, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn show_the_one_running() {}

/* ---- L'ouvrir ------------------------------------------------------- */

/// Opens the window, hidden.
///
/// Hidden: what fills it is not drawn yet, and a window shown before
/// it has been painted is seen empty. It is `show` that uncovers it,
/// once the home screen is laid inside.
#[cfg(windows)]
pub fn open(app: &App) -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, GetSystemMetrics, IDC_ARROW, LoadCursorW, RegisterClassW,
        SM_CXSCREEN, SM_CYSCREEN, WNDCLASSW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    };

    if handle() != 0 {
        return Ok(());
    }
    *PROGRAM.lock().expect("programme de la fenêtre") = Some(app.clone());

    let class_name = wide(CLASS_NAME);
    let title = wide(CLASS_NAME);
    // SAFETY: no argument beyond what is asked for.
    let dpi = unsafe { GetDpiForSystem() };
    let (width, height) = (
        scaled(OPENS_AT.0, dpi as i32),
        scaled(OPENS_AT.1, dpi as i32),
    );
    // In the middle of the main screen: that is where a window opens the
    // first time, and the system does not do it by itself.
    //
    // SAFETY: no argument beyond the metric asked for.
    let (screen_width, screen_height) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let (x, y) = if screen_width > width && screen_height > height {
        ((screen_width - width) / 2, (screen_height - height) / 2)
    } else {
        (CW_USEDEFAULT, CW_USEDEFAULT)
    };

    // SAFETY: a class registered once and a window built from it, on
    // the thread that will pump its messages.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(answers),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            // No background: all of its inside is a child window that
            // paints itself, and a background laid by the system would
            // be one more colour, seen for the length of a frame at
            // every resize.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            // Clipped by its children: the home screen and the picture
            // of a session are among them, and without this the system
            // would paint underneath before they paint on top.
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err("la fenêtre de ZyrDesk n'a pas pu s'ouvrir".to_string());
    }
    HANDLE.store(hwnd as isize, Ordering::Relaxed);
    note(&format!(
        "fenêtre ouverte par ZyrDesk, {width}x{height} px à {} %",
        dpi * 100 / 96
    ));
    Ok(())
}

#[cfg(not(windows))]
pub fn open(_app: &App) -> Result<(), String> {
    Err("ZyrDesk n'ouvre de fenêtre que sous Windows".to_string())
}

/// A page length in real pixels, on a screen of this
/// magnification.
fn scaled(page: i32, dpi: i32) -> i32 {
    page * dpi / 96
}

/// A word in the characters Windows counts, ended by the zero it looks
/// for.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/* ---- Ce que la fenêtre répond --------------------------------------- */

/// SAFETY: called by the system on the thread that made this window,
/// with the arguments it documents.
#[cfg(windows)]
unsafe extern "system" fn answers(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, MINMAXINFO, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WM_CLOSE,
        WM_DPICHANGED, WM_GETMINMAXINFO, WM_SETFOCUS, WM_SIZE,
    };

    match message {
        // The cross means two things, and which one depends on what the
        // window shows.
        //
        // On a session, it ends the session and the window stays: the
        // picture is inside, and a cross that only put the window away
        // would leave the remote computer held by something that no longer
        // has anything on screen to give it back.
        //
        // On the home screen, it puts the window away without stopping
        // anything. This computer can be reachable without anyone looking
        // at a window, the icon near the clock says so, and "Quitter" over
        // there is the only thing that stops the product.
        WM_CLOSE => {
            if let Some(app) = program() {
                if crate::floating::a_session_is_up(&app) || crate::session::opening() {
                    // While a session is only opening there is sometimes
                    // nothing to end; the request then reaches only the
                    // journal, and the window stays. Putting it away let
                    // the opening carry on unseen, and the session arrived
                    // as a bare rectangle on the desktop.
                    crate::session::end_it(&app);
                } else {
                    hide();
                }
            }
            0
        }
        // Its size has changed: what it carries follows, here and now.
        // What an event loop would say about it would arrive one trip
        // through the queue later, and an inside lagging behind its frame
        // shows for the whole of a resize.
        WM_SIZE => {
            say_whether_it_goes_down_or_up(holding);
            let (width, height) = ((with & 0xFFFF) as i32, ((with >> 16) & 0xFFFF) as i32);
            let inside = crate::home::its_canvas();
            if inside != 0 {
                // SAFETY: a window of ours, laid over the inside of
                // the one that has just changed size.
                unsafe {
                    SetWindowPos(
                        inside as windows_sys::Win32::Foundation::HWND,
                        std::ptr::null_mut(),
                        0,
                        0,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                };
            }
            // Put off to the next turn and not done here: holding it
            // to its shape resizes it, which would bring this very
            // message back while it is being answered.
            if let Some(app) = program() {
                let held = app.clone();
                let _ = app.run_on_main_thread(move || crate::picture::hold_the_shape(&held));
            }
            0
        }
        // What it never goes below, counted on the screen it occupies:
        // the floor is in page pixels.
        WM_GETMINMAXINFO => {
            // SAFETY: the system passes a block of its own here, alive
            // for the length of the call, and only one field of it is
            // written.
            unsafe {
                let dpi = GetDpiForWindow(window).max(96) as i32;
                let info = with as *mut MINMAXINFO;
                (*info).ptMinTrackSize.x = scaled(NEVER_SMALLER.0, dpi);
                (*info).ptMinTrackSize.y = scaled(NEVER_SMALLER.1, dpi);
            }
            0
        }
        // It has moved to another screen, or its screen has changed
        // magnification. The system says where to put it so that it
        // keeps its apparent size, and everything counted in real pixels
        // is counted again.
        WM_DPICHANGED => {
            // SAFETY: the system passes a rectangle of its own here,
            // alive for the length of the call.
            let wanted = unsafe { *(with as *const RECT) };
            // SAFETY: a window of ours, put where the system wants it.
            unsafe {
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    wanted.left,
                    wanted.top,
                    wanted.right - wanted.left,
                    wanted.bottom - wanted.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
            crate::icon::on_the_window();
            if let Some(app) = program() {
                crate::home::measure_the_screen(&app);
            }
            0
        }
        // The keyboard goes to what is drawn inside: this window draws
        // nothing and has nothing to read. Except during a session: the
        // keyboard then belongs to the picture, and taking it back from
        // it when coming back to the window would take it away from the
        // far computer.
        WM_SETFOCUS => {
            let inside = crate::home::its_canvas();
            if inside != 0 && crate::picture::the_engines_window().is_none() {
                // SAFETY: a window of ours, on the thread that owns it.
                unsafe { SetFocus(inside as windows_sys::Win32::Foundation::HWND) };
            }
            0
        }
        _ => {
            // A second ZyrDesk has just been started: the one running
            // shows itself, and the other one stops without opening
            // anything.
            if message == SHOW_YOURSELF {
                show();
                return 0;
            }
            // SAFETY: the system's own answer to everything not
            // answered here.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
    }
}

/// Where the window stood the last time it was said.
#[cfg(windows)]
static MINIMIZED: AtomicBool = AtomicBool::new(false);

/// Says when the window goes down into the taskbar and when it comes back
/// up from it, with what holds the front at that moment.
///
/// Two lines per round trip and not one more. A window that does not come
/// back up is the kind of trouble no one can take a photograph of, and
/// these two lines say the only two things that settle it: whether the
/// order to come back up even got this far, and who owned the front when
/// it went down. The second one alone accounts for the case where the
/// system asks for nothing again because it believes we are already in
/// front.
#[cfg(windows)]
fn say_whether_it_goes_down_or_up(what: usize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SIZE_MINIMIZED;

    let minimized = what as u32 == SIZE_MINIMIZED;
    if MINIMIZED.swap(minimized, Ordering::Relaxed) == minimized {
        return;
    }
    note(&format!(
        "fenêtre {} ; le premier plan est {}",
        if minimized {
            "rangée dans la barre des tâches"
        } else {
            "ressortie de la barre des tâches"
        },
        crate::picture::the_front_in_words()
    ));
}

/* ---- La montrer, la ranger ------------------------------------------ */

/// Brings it back, wherever it was left.
#[cfg(windows)]
pub fn show() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return;
    }
    // SAFETY: a window of ours.
    unsafe {
        ShowWindow(
            hwnd,
            if IsIconic(hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(hwnd);
    }
}

#[cfg(not(windows))]
pub fn show() {}

/// Puts it away without stopping anything.
#[cfg(windows)]
pub fn hide() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if !hwnd.is_null() {
        // SAFETY: a window of ours.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

#[cfg(not(windows))]
pub fn hide() {}

/// Whether it is on screen: shown, and not put away in the taskbar.
///
/// Both together because both count for the same thing: a minimised
/// window still calls itself visible, and the floating button laid on it
/// would then be the only thing on screen, hanging in a corner over
/// someone else's work.
#[cfg(windows)]
pub fn on_screen() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    // SAFETY: a window number, which the calls are made to weigh.
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
}

#[cfg(not(windows))]
pub fn on_screen() -> bool {
    false
}

/* ---- Ce qu'elle mesure ---------------------------------------------- */

/// How much a page pixel counts for on the screen it is on.
#[cfg(windows)]
pub fn scale() -> f32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return 1.0;
    }
    // SAFETY: a window of ours, and only one of its
    // measurements is read.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

#[cfg(not(windows))]
pub fn scale() -> f32 {
    1.0
}

/// What its inside measures, in real pixels.
#[cfg(windows)]
pub fn inside() -> (u32, u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, whose rectangle is read into ours.
    if hwnd.is_null() || unsafe { GetClientRect(hwnd, &mut place) } == 0 {
        return (0, 0);
    }
    (place.right.max(0) as u32, place.bottom.max(0) as u32)
}

#[cfg(not(windows))]
pub fn inside() -> (u32, u32) {
    (0, 0)
}

/// Gives its inside that size, with the frame coming on top.
#[cfg(windows)]
pub fn set_the_inside(width: u32, height: u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        SetWindowPos,
    };

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return;
    }
    let mut wanted = RECT {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    // SAFETY: a window of ours, whose two styles are read so that the
    // system counts the frame they ask for around the wanted inside.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let others = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let dpi = GetDpiForWindow(hwnd).max(96);
        AdjustWindowRectExForDpi(&mut wanted, style, 0, others, dpi);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            wanted.right - wanted.left,
            wanted.bottom - wanted.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
pub fn set_the_inside(_large: u32, _height: u32) {}

/* ---- L'agrandir, lui donner l'écran --------------------------------- */

/// Maximises it to what the desktop leaves.
#[cfg(windows)]
pub fn maximize() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MAXIMIZE, ShowWindow};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if !hwnd.is_null() {
        // SAFETY: a window of ours.
        unsafe { ShowWindow(hwnd, SW_MAXIMIZE) };
    }
}

#[cfg(not(windows))]
pub fn maximize() {}

/// Gives it back the size it had before being maximised.
///
/// Only if it is: otherwise the call would only bring it back to the
/// front, for nothing.
///
/// And without the system playing it, because this is not a state
/// anything stops in: the window takes the screen straight after, and
/// that size is only a way through. Now the compositor plays changes of
/// state at its own pace and not at ours. ShowWindow returns at once,
/// the animation carries on behind, and the window has already taken the
/// screen while the compositor is still shrinking it. The remote
/// desktop, which is a window carried by this one and so drawn in its
/// composition, goes along with it: that is the strange enlargement
/// inside the stream, at the first full screen of a session.
///
/// Restored right after: "maximise" from the title bar is a gesture
/// where the system's animation is wanted, and where a whole part of
/// picture.rs counts on it.
#[cfg(windows)]
fn restore_its_size() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_RESTORE, ShowWindow};

    // Maximised already means it exists.
    if is_maximized() {
        let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
        play_the_transitions(hwnd, false);
        // SAFETY: a window of ours.
        unsafe { ShowWindow(hwnd, SW_RESTORE) };
        play_the_transitions(hwnd, true);
    }
}

/// Asks the compositor to play, or not to play, this window's changes
/// of state.
///
/// A refusal is the answer of a Windows that does not have this
/// setting, and costs only the animation we wanted to avoid.
#[cfg(windows)]
fn play_the_transitions(hwnd: windows_sys::Win32::Foundation::HWND, yes: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute,
    };

    let disabled: i32 = i32::from(!yes);
    // SAFETY: a window of ours, and four bytes of ours whose size is
    // given.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&raw const disabled).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

#[cfg(windows)]
pub fn is_maximized() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed;

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    // SAFETY: a window number, which the call is made to weigh.
    !hwnd.is_null() && unsafe { IsZoomed(hwnd) != 0 }
}

#[cfg(not(windows))]
pub fn is_maximized() -> bool {
    false
}

/// Whether it takes the whole screen.
pub fn holds_the_screen() -> bool {
    FULL_SCREEN.load(Ordering::Relaxed)
}

/// Gives it the whole screen, or takes it back.
///
/// Its place and its frame are set aside together and taken back
/// together: a window that gets its place back without getting its title
/// bar back is a window no one can grab any more.
#[cfg(windows)]
pub fn take_the_screen(whole: bool) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, GetWindowPlacement, HWND_TOP, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT,
        WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE,
        WS_THICKFRAME,
    };

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() || FULL_SCREEN.swap(whole, Ordering::Relaxed) == whole {
        return;
    }
    let mut before = BEFORE_FULL_SCREEN.lock().expect("place de la fenêtre");
    if whole {
        let mut place: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        place.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let mut about: MONITORINFO = unsafe { std::mem::zeroed() };
        about.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: a window of ours, and two blocks of ours with their
        // size written inside, as the calls ask.
        let (style, others, read) = unsafe {
            (
                GetWindowLongPtrW(hwnd, GWL_STYLE),
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                GetWindowPlacement(hwnd, &mut place) != 0
                    && GetMonitorInfoW(
                        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                        &mut about,
                    ) != 0,
            )
        };
        if !read {
            FULL_SCREEN.store(false, Ordering::Relaxed);
            return;
        }
        // SAFETY: the system's structure, copied as it is to be given back
        // as it is: this file reads nothing from it.
        let kept: [u8; PLACE] = unsafe { std::mem::transmute(place) };
        *before = Some((style, others, kept));

        // A maximised window is still maximised once its frame is
        // removed, and the system holds it in the place it gave it: it
        // goes back there at the next recount of the frame, which comes
        // at once since removing the frame means asking for one. What
        // shows then is a borderless window at the maximised
        // measurements, spilling over the screen by the few pixels
        // Windows keeps for the border of a maximised window, and cut
        // short at the bottom by the height of the taskbar. So it is
        // given back its size before taking the screen; the maximised
        // state is already in the record that will give it back later.
        restore_its_size();

        // Read again after that, and not taken from above: the
        // maximised state is read in the style itself, and writing back
        // the one from before would tell the system again that it is
        // maximised when it no longer is.
        //
        // SAFETY: a window of ours, whose style is read again.
        let current_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };

        let without_frame = current_style & !((WS_CAPTION | WS_THICKFRAME) as isize);
        let without_border = others
            & !((WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE)
                as isize);
        let area: RECT = about.rcMonitor;
        // SAFETY: a window of ours, given its frame and its place,
        // with a request for the frame to be counted again.
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, without_frame);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, without_border);
            SetWindowPos(
                hwnd,
                HWND_TOP,
                area.left,
                area.top,
                area.right - area.left,
                area.bottom - area.top,
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
        return;
    }

    let Some((style, others, kept)) = before.take() else {
        return;
    };
    // SAFETY: the system's structure, given back as it was taken.
    let place: WINDOWPLACEMENT = unsafe { std::mem::transmute(kept) };
    // SAFETY: a window of ours, given back its frame and then its place.
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, others);
        SetWindowPlacement(hwnd, &place);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED
                | SWP_NOACTIVATE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        );
    }
}

#[cfg(not(windows))]
pub fn take_the_screen(_whole: bool) {}

/* ---- Son cadre ------------------------------------------------------ */

/// Matches the window's frame to the theme.
///
/// The frame belongs to Windows and not to us: it is the only part of the
/// window this program does not draw, and without this a light interface
/// would keep a dark title bar.
#[cfg(windows)]
pub fn dress_the_frame(light: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return;
    }
    let dark: i32 = i32::from(!light);
    // SAFETY: a window of ours, and four bytes of ours whose size is
    // given. A refusal is the answer of a Windows too old for that bar,
    // and prevents nothing else.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&raw const dark).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

#[cfg(not(windows))]
pub fn dress_the_frame(_light: bool) {}
