//! What a session does to the product's own window.
//!
//! The picture is a window of ours laid inside ours (see `video`), and a
//! child window goes wherever its parent goes: nothing here carries it.
//! What is left is what a session asks of the window that holds it:
//! taking the whole screen and giving it back, with no frame left on it;
//! keeping the window the shape of the picture; measuring the screen a
//! session is watched on; and shutting the pointer in the picture when a
//! game or the whole screen calls for it.
//!
//! The window is given the picture's shape rather than the picture the
//! window's. The player draws what arrives in the shape it arrives in and
//! never in another: given a window of a different shape it centres the
//! picture and fills the rest with black. Every black band this end of
//! the product could show comes from that one fact, and the only way not
//! to show one is not to ask for one.

// Windows only, and only ever a session: the rest of the product is
// tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering};

use crate::app::App;
use zyr_proto::session::{DisplayMode, Screen};

/// What this module files its journal lines under.
const TAG: &str = "picture";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// The shape of the picture: the size the far computer's stream is made
/// at, as the player last said it, or nought outside a session.
///
/// A number and not a lock: it is read inside the system's own call into
/// our window, which cannot wait on anything.
static SHAPE: AtomicI64 = AtomicI64::new(0);

/// Set while the person is dragging an edge or the title bar of our
/// window.
///
/// The shape is held during the drag, before each resize; tidying it up
/// afterwards as well would resize the window a second time for nothing.
static DRAGGED: AtomicBool = AtomicBool::new(false);

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
static HELD: AtomicU8 = AtomicU8::new(0);

/// The left or the right edge.
const A_SIDE: u8 = 1;
/// The top or the bottom edge.
const TOP_OR_BOTTOM: u8 = 2;

/// Width and height of the picture, as the handler reads them.
fn shape() -> (i32, i32) {
    let both = SHAPE.load(Ordering::Relaxed);
    ((both >> 32) as i32, both as i32)
}

/// Writes the shape down where the handler reads it, and says whether it
/// changed.
fn note_the_shape(shape: (i32, i32)) -> bool {
    let both = (i64::from(shape.0) << 32) | i64::from(shape.1) & 0xFFFF_FFFF;
    SHAPE.swap(both, Ordering::Relaxed) != both
}

/// The picture has that shape from now on: the far computer's stream is
/// made at that size, as its player just said.
///
/// Said again at every new stream, which is every change of size asked
/// for from the menu: the window follows the picture it carries.
pub fn takes_the_shape(app: &App, (wide, high): (u32, u32)) {
    let shape = (
        i32::try_from(wide).unwrap_or(i32::MAX),
        i32::try_from(high).unwrap_or(i32::MAX),
    );
    if !note_the_shape(shape) {
        return;
    }
    let held = app.clone();
    let _ = app.run_on_main_thread(move || hold_the_shape(&held));
}

/// Lets go of the window, the session being over: no shape to hold, no
/// pointer to shut in, and the window's messages its own again.
pub fn let_go(app: &App) {
    note_the_shape((0, 0));
    DRAGGED.store(false, Ordering::Relaxed);
    shut_the_pointer_in(Cage::Free);
    give_the_window_back(app);
}

/// Puts our window on the whole screen, or takes it back off.
///
/// The picture follows it, being its inside.
pub fn take_the_screen(app: &App, whole: bool) -> Result<(), String> {
    if crate::main_window::handle() == 0 {
        return Err("la fenêtre de ZyrDesk n'est plus là".to_string());
    }
    // The window writes down what it is becoming before it moves, never
    // after: taking the screen is what makes the system ask which frame
    // it will have, and the answer depends on it.
    let was = crate::main_window::holds_the_screen();
    crate::main_window::take_the_screen(whole);
    if was != whole {
        no_frame_on_the_whole_screen(app);
    }
    Ok(())
}

/// Puts the window where a session is meant to be watched from, and takes
/// it in hand for the length of the session.
///
/// The whole screen when that is what was asked for. Otherwise as large
/// as a window goes, and not the size it happened to be left at. A
/// session shows somebody else's desktop, drawn over there at the size
/// this end asked for; a window smaller than it could be is that picture
/// shrunk again on arrival, for nothing. Nobody opens a remote desktop
/// meaning to watch it in a corner, and whoever does still has the
/// window's own corner to drag.
///
/// Only ever on the way in, and the whole-screen road is left exactly as
/// it was. A session ending hands the screen back but leaves the window
/// the size it is: taking somebody's window down a size after they have
/// spent an hour in it is not ours to do.
pub fn take_the_screen_for_a_session(app: &App, whole: bool) -> Result<(), String> {
    take_the_window_in_hand(app);
    take_the_screen(app, whole)?;
    if whole {
        return Ok(());
    }
    crate::main_window::maximize();
    Ok(())
}

/// The same, the other way from wherever it is, and remembered.
///
/// This one is a person deciding, which the two calls above are not: one
/// applies what was decided before, the other takes the screen back at
/// the end of a session. So this is the only one that writes anything
/// down, and what it writes is what the next session opens as.
pub fn toggle_the_screen(app: &App) -> Result<(), String> {
    if crate::main_window::handle() == 0 {
        return Err("la fenêtre de ZyrDesk n'est plus là".to_string());
    }
    let whole = !crate::main_window::holds_the_screen();
    take_the_screen(app, whole)?;

    // Writing it down means asking the service, which is a round trip
    // over a pipe: the picture has already moved, and nothing waits for
    // this.
    crate::app::spawn(async move {
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
/// This is the tidying-up, not the mechanism. A window being dragged is
/// held to shape while it is dragged, before the resize happens, which is
/// the only way that is smooth; see `the_drag_keeps_the_shape`. What is
/// left for here is everything that resizes a window without dragging it:
/// the picture arriving or changing size, the screen being given back,
/// the system putting the window against an edge.
///
/// Only the height moves. Both would fight whichever edge is being
/// dragged, and a window that resists in two directions at once cannot
/// be resized at all.
pub fn hold_the_shape(_app: &App) {
    if DRAGGED.load(Ordering::Relaxed) {
        return;
    }
    let (wide, high) = shape();
    if wide <= 0 || high <= 0 {
        return;
    }
    if crate::main_window::handle() == 0 {
        return;
    }
    // Covering the screen is a shape nobody chose and nobody drags, and
    // so is a window put against the edges of the screen by the system.
    if crate::main_window::holds_the_screen() || crate::main_window::is_maximized() {
        return;
    }
    let inside = crate::main_window::inside();
    let Ok(width) = i32::try_from(inside.0) else {
        return;
    };
    let wanted = across(width, wide, high);
    if wanted <= 0 || inside.1.abs_diff(wanted as u32) <= ROUNDING {
        return;
    }
    crate::main_window::set_the_inside(inside.0, wanted as u32);
}

/* ---- What belongs to Windows ----------------------------------------- */

/// The screen this window sits on: its size in real pixels, how many
/// times a second it refreshes, and how much larger than life it draws.
///
/// Real pixels and not the ones a page is laid out with: a screen at a
/// hundred and fifty per cent reports two thirds of what it draws, and
/// asking the far computer for two thirds of a screen is exactly the
/// mistake this measurement exists to prevent. The magnification is
/// measured all the same and carried beside the size, because the far
/// computer needs it to draw at the size this one reads at: real pixels
/// answer how sharp the picture is, and this answers how large anything
/// in it looks.
///
/// The screen the window is on rather than the main one, because that is
/// the screen the picture will be shown on. Two screens on one desk are
/// rarely the same panel, and a hundred and forty-four next to a sixty
/// is the ordinary case, not the odd one.
#[cfg(windows)]
pub fn the_screen_of_this_computer(app: &App) -> Option<Screen> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromWindow,
    };

    let home = home_window(app)?;
    let mut about: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    about.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: our own window, and the slot is ours with its size written
    // in it as the call requires. The wider slot is asked for by that
    // size, and it is what carries the screen's name out.
    let screen = unsafe {
        let monitor = MonitorFromWindow(home, MONITOR_DEFAULTTONEAREST);
        (GetMonitorInfoW(monitor, (&raw mut about).cast()) != 0).then_some(about)
    }?;
    let edges = screen.monitorInfo.rcMonitor;
    let wide = u32::try_from(edges.right - edges.left).ok()?;
    let high = u32::try_from(edges.bottom - edges.top).ok()?;
    (wide > 0 && high > 0).then(|| Screen {
        wide,
        high,
        refresh: the_rate_of(&screen.szDevice),
        scale: the_magnification(app),
    })
}

/// How many times a second the named screen refreshes.
///
/// The name comes out of the same call that gave the size, so the rate
/// read here is that screen's and not the desk's main one. Nought is what
/// the system itself answers for a screen whose rate it does not hold,
/// and it is what a failed read answers with here: the two mean the same
/// thing, and what a session makes of it is settled where the rate is
/// turned into what it asks for.
#[cfg(windows)]
fn the_rate_of(name: &[u16; 32]) -> u32 {
    use windows_sys::Win32::Graphics::Gdi::{
        DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW,
    };

    let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
    mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    // SAFETY: a name the system just wrote and ended itself, and a slot
    // of ours with its size written in it as the call requires.
    let read = unsafe { EnumDisplaySettingsW(name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) };
    if read == 0 {
        0
    } else {
        mode.dmDisplayFrequency
    }
}

#[cfg(not(windows))]
pub fn the_screen_of_this_computer(_app: &App) -> Option<Screen> {
    None
}

/// How much the system is drawing this window's page bigger than life.
///
/// Asked of the window rather than of the screen, which is the same
/// answer read the short way: Windows hands a per-monitor aware window
/// the magnification of the screen it sits on, and that is the screen
/// measured just above. A hundred and twenty dots to the inch is a
/// quarter larger than life, and the ninety-six it is counted from is
/// the number Windows has meant by life size since it had a settings
/// page at all.
#[cfg(windows)]
fn the_magnification(app: &App) -> u32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use zyr_proto::session::LIFE_SIZE;

    const LIFE_SIZE_IN_DOTS: u32 = 96;

    let Some(home) = home_window(app) else {
        return LIFE_SIZE;
    };
    // SAFETY: our own window, read only.
    unsafe { GetDpiForWindow(home) }.max(LIFE_SIZE_IN_DOTS) * LIFE_SIZE / LIFE_SIZE_IN_DOTS
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
    screen: Option<Screen>,
    asked: zyr_proto::session::Asked,
    settings: &zyr_proto::session::SessionSettings,
) {
    let (wide, high) = (settings.width, settings.height);
    let seen = match screen {
        Some(measured) => format!(
            "écran de cet ordinateur : {}x{} pixels réels à {} Hz, agrandissement {} %",
            measured.wide, measured.high, measured.refresh, measured.scale
        ),
        None => "écran de cet ordinateur : pas mesurable, taille courante supposée".to_string(),
    };
    let why = match screen {
        Some(measured) if (measured.wide, measured.high) == (wide, high) => {
            "l'écran est demandé entier, un pixel envoyé pour un pixel affiché".to_string()
        }
        Some(measured) => format!(
            "taille choisie à la main ({asked}) : {:.2} fois moins large et {:.2} fois moins haut que l'écran, donc autant de détail en moins et l'image est étirée à l'arrivée",
            f64::from(measured.wide) / f64::from(wide),
            f64::from(measured.high) / f64::from(high),
        ),
        None => format!("taille demandée : {asked}"),
    };
    note(&format!(
        "{seen} ; image demandée au loin en {wide}x{high} à {} images/s et {} Mb/s en {}, {why}",
        settings.fps,
        settings.bitrate_kbps / 1000,
        settings.codec,
    ));
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
    let whole = crate::main_window::holds_the_screen();
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
    note(&format!(
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
fn no_frame_on_the_whole_screen(app: &App) {
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
fn no_frame_on_the_whole_screen(_app: &App) {}

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
    // A window down in the taskbar answers a two hundred pixel rectangle
    // sitting at minus thirty-two thousand, which is how the system says
    // "nowhere". Printed as it comes, it reads like a measurement and is
    // not one.
    if !crate::main_window::on_screen() {
        note("cadre de la fenêtre : elle est rangée dans la barre des tâches");
        return;
    }
    let screen = about.rcMonitor;
    note(&format!(
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

/// Who holds the front, in the words the journal uses everywhere.
///
/// One phrasing for the whole program, so that the lines saying it can
/// be read against one another. Somebody else's window is named rather
/// than merely spotted: a session that stops answering is almost always a
/// session something else is in front of, and nothing else says what.
#[cfg(windows)]
pub(crate) fn the_front_in_words() -> String {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: no argument, and a null answer is one of the answers.
    let front = unsafe { GetForegroundWindow() };
    let mut owner = 0u32;
    // SAFETY: the window is the system's answer and the slot is ours; a
    // window that has gone answers nought, which is nobody.
    unsafe { GetWindowThreadProcessId(front, &mut owner) };
    // SAFETY: no argument.
    if !front.is_null() && owner == unsafe { GetCurrentProcessId() } {
        "à ZyrDesk".to_string()
    } else {
        format!("ailleurs : {}", describe(front))
    }
}

#[cfg(not(windows))]
pub(crate) fn the_front_in_words() -> String {
    "hors de Windows, où il n'y a pas de session".to_string()
}

/// Names that window: its program and its title.
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

/// Name our handler answers to, so it can be taken off again.
#[cfg(windows)]
const IN_HAND: usize = 1;

/// Steps in front of our window's messages for the length of a session.
///
/// A window answers what the system asks before acting, and that is the
/// only moment an answer changes anything: what size a drag is about to
/// give it, and what frame it has on the whole screen. Stepping in front
/// of a window's messages is done from the thread that draws it, and the
/// callers here are not that thread: handed over rather than done on the
/// spot.
#[cfg(windows)]
fn take_the_window_in_hand(app: &App) {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;

    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(home) = home_window(&asked) else {
            return;
        };
        // SAFETY: our own window, from the thread that owns it, and the
        // handler outlives the subclass: it is a plain function of this
        // program. Put on twice, the same handler under the same name is
        // only put on once.
        unsafe { SetWindowSubclass(home, Some(in_hand), IN_HAND, 0) };
        round_the_window(home, true);
        // A session can open straight onto the whole screen, in which
        // case the window took it before this handler was on it and the
        // system asked about the frame with nobody there to answer.
        // Asked again now, with the handler in place.
        if crate::main_window::holds_the_screen() {
            no_frame_on_the_whole_screen(&asked);
        }
        tell_the_frame(home);
    });
}

#[cfg(not(windows))]
fn take_the_window_in_hand(_app: &App) {}

/// Takes our handler back off, the session being over.
#[cfg(windows)]
fn give_the_window_back(app: &App) {
    use windows_sys::Win32::UI::Shell::RemoveWindowSubclass;

    let asked = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(home) = home_window(&asked) else {
            return;
        };
        // SAFETY: same window, same thread and same handler as were put
        // on it.
        unsafe { RemoveWindowSubclass(home, Some(in_hand), IN_HAND) };
        round_the_window(home, false);
    });
}

#[cfg(not(windows))]
fn give_the_window_back(_app: &App) {}

#[cfg(windows)]
unsafe extern "system" fn in_hand(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _name: usize,
    _data: usize,
) -> isize {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SC_MOVE, SC_SIZE, WINDOWPOS, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCCALCSIZE,
        WM_SYSCOMMAND, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING,
    };

    match message {
        // A hand about to take the window, and the system saying which of
        // the two gestures it is before either has begun. Written down
        // here because nothing later says it: the drag messages that
        // follow are the same for both.
        WM_SYSCOMMAND if matches!((wparam & 0xFFF0) as u32, SC_MOVE | SC_SIZE) => {
            BY_AN_EDGE.store((wparam & 0xFFF0) as u32 == SC_SIZE, Ordering::Relaxed);
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        WM_ENTERSIZEMOVE => {
            DRAGGED.store(true, Ordering::Relaxed);
            HELD.store(0, Ordering::Relaxed);
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            DRAGGED.store(false, Ordering::Relaxed);
            // SAFETY: the arguments the system handed in, untouched.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        // A window about to take a new size: the system says what it is
        // about to apply and takes back whatever is written there, before
        // anything moves. Only a hand has a shape to hold; held here
        // rather than corrected afterwards, since corrected afterwards
        // every step of a drag resized the window twice.
        WM_WINDOWPOSCHANGING if DRAGGED.load(Ordering::Relaxed) => {
            // SAFETY: for this message the system passes a WINDOWPOS of
            // ours to read and amend, and it lives for the length of the
            // call.
            let wanted = unsafe { &mut *(lparam as *mut WINDOWPOS) };
            the_drag_keeps_the_shape(window, wanted);
            // SAFETY: handed on: what was written only becomes the
            // window's size in the system's own handling of this message.
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
        // On the whole screen, the inside is the whole window.
        //
        // The system reserves a strip along the top and the sides of
        // every ordinary window for a border, and inside starts below it;
        // a window covering the screen is still an ordinary window to the
        // system, so the strip lands on the screen's own edge. That is the
        // pale line along the top of a full screen session, and the reason
        // the far computer's picture sat a few pixels below where it
        // should. What the block already holds is the window itself, and
        // nought is « that rectangle stands ».
        WM_NCCALCSIZE if wparam != 0 && crate::main_window::holds_the_screen() => 0,
        // Our window has just moved, been resized, shown or hidden: the
        // button and the badges go with the picture, here and now.
        WM_WINDOWPOSCHANGED => {
            // SAFETY: the arguments the system handed in, untouched. The
            // system finishes moving this window, and its inside with it,
            // before the button is laid on what it has become.
            let answer = unsafe { DefSubclassProc(window, message, wparam, lparam) };
            if let Some(picture) = crate::video::where_it_is() {
                crate::floating::lay_the_button(picture);
            }
            answer
        }
        // SAFETY: same.
        _ => unsafe { DefSubclassProc(window, message, wparam, lparam) },
    }
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
    // Its size is moving and the hand is on the title bar, so the hand
    // is not what is moving it: the window has been carried against an
    // edge of the screen and the system is snapping it there, which is
    // « agrandir » wearing a different hat. Held to the picture's
    // proportions, the rectangle the system had chosen stopped being the
    // screen's, and the window landed at a size of its own making, with
    // the desktop showing beside it. Left alone.
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

/// Our own window, as the system knows it.
#[cfg(windows)]
fn home_window(_app: &App) -> Option<windows_sys::Win32::Foundation::HWND> {
    let home = crate::main_window::handle() as windows_sys::Win32::Foundation::HWND;
    (!home.is_null()).then_some(home)
}

/// Where this computer's pointer is kept during a session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cage {
    /// Free to go anywhere on the desk.
    Free,
    /// Inside the picture, which is the whole screen.
    Picture,
    /// On one point in the middle of the picture, for a game.
    Point,
}

impl Cage {
    fn number(self) -> u8 {
        match self {
            Cage::Free => 0,
            Cage::Picture => 1,
            Cage::Point => 2,
        }
    }

    /// Where the pointer is to be kept, the session standing as it does.
    ///
    /// A game is played with movement and not with a place: the pointer
    /// of this computer has nothing to point at, it is hidden, and if it
    /// is left free it walks off the picture behind the hand that is
    /// playing, onto whatever takes the clicks there, or off to a second
    /// screen. Shut on one point it cannot walk anywhere, while the
    /// movement the hand makes goes on being read from the device itself.
    ///
    /// A picture that is the whole screen keeps the pointer inside it as
    /// well: the far computer's screen is all there is to point at, and a
    /// pointer that slips onto a second screen leaves the session behind
    /// without a word. A window lets it go, since reaching the other
    /// windows of this computer is the whole reason somebody is not in
    /// full screen.
    ///
    /// Free while the floating menu is open, since a hand reading the
    /// menu is aiming at something else, and while another program has
    /// the front: the pointer belongs to it then.
    pub fn for_the(game: bool, whole_screen: bool, menu_open: bool, in_front: bool) -> Cage {
        if menu_open || !in_front {
            Cage::Free
        } else if game {
            Cage::Point
        } else if whole_screen {
            Cage::Picture
        } else {
            Cage::Free
        }
    }
}

/// What the pointer was last kept in, so the journal says it once and
/// not once a second.
static SHUT_IN: AtomicU8 = AtomicU8::new(0);

/// The same for a cage the system will not grant.
static CAGE_REFUSED: AtomicBool = AtomicBool::new(false);

/// Keeps this computer's pointer where the session wants it.
///
/// Asked here and not of anything drawing the picture: the system lets
/// one program shut the pointer in, and it is the one at the front.
///
/// Said at every turn of the watch rather than at the change: it is a
/// shared thing of the whole desk, anything may open it, and reasserting
/// a cage that is already the right one costs one reading.
#[cfg(windows)]
pub(crate) fn shut_the_pointer_in(cage: Cage) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClipCursor};

    let wanted = match (cage, crate::video::where_it_is()) {
        (Cage::Free, _) | (_, None) => None,
        (Cage::Picture, Some((left, top, right, bottom))) => Some(RECT {
            left,
            top,
            right,
            bottom,
        }),
        (Cage::Point, Some((left, top, right, bottom))) => {
            let (middle_x, middle_y) = ((left + right) / 2, (top + bottom) / 2);
            Some(RECT {
                left: middle_x,
                top: middle_y,
                right: middle_x + 1,
                bottom: middle_y + 1,
            })
        }
    };
    let Some(wanted) = wanted else {
        CAGE_REFUSED.store(false, Ordering::Relaxed);
        if SHUT_IN.swap(Cage::Free.number(), Ordering::Relaxed) == Cage::Free.number() {
            return;
        }
        // SAFETY: nought gives the pointer the whole desk back.
        unsafe { ClipCursor(std::ptr::null()) };
        note("pointeur rendu au bureau");
        return;
    };
    let mut now = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: the rectangle is ours.
    if unsafe { GetClipCursor(&mut now) } != 0
        && (now.left, now.top, now.right, now.bottom)
            == (wanted.left, wanted.top, wanted.right, wanted.bottom)
    {
        return;
    }
    // SAFETY: a rectangle of this desk, given to a call that reads it.
    if unsafe { ClipCursor(&wanted) } == 0 {
        // Said once for a run of refusals and not once a second, and said
        // at all: a cage nobody can see is refused exactly as silently as
        // it is granted.
        if !CAGE_REFUSED.swap(true, Ordering::Relaxed) {
            note("pointeur non enfermé dans l'image : Windows a refusé la cage");
        }
        return;
    }
    CAGE_REFUSED.store(false, Ordering::Relaxed);
    if SHUT_IN.swap(cage.number(), Ordering::Relaxed) != cage.number() {
        note(match cage {
            Cage::Point => "pointeur tenu au milieu de l'image, la souris étant celle d'un jeu",
            _ => "pointeur tenu dans l'image, qui est tout l'écran",
        });
    }
}

#[cfg(not(windows))]
pub(crate) fn shut_the_pointer_in(_cage: Cage) {}

/// Puts this computer's pointer on a window of ours, the cage having just
/// been opened for it.
///
/// A game hides the pointer over the picture. Over a window of ours the
/// system asks us instead and gets an arrow, so the pointer is seen there
/// and nowhere else: freed in the middle of the picture, it has to cross
/// the picture invisible to reach anything, which is aiming blind. Put on
/// the window that was asked for, it is under the hand and visible from
/// the first moment.
#[cfg(windows)]
pub(crate) fn put_the_pointer_on(window: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;

    let window = window as windows_sys::Win32::Foundation::HWND;
    if window.is_null() {
        return;
    }
    let Some((left, top, right, bottom)) = where_it_stands(window) else {
        return;
    };
    // SAFETY: a place on this desk, and this program is the one at the
    // front, which is what the call asks of its caller.
    unsafe { SetCursorPos((left + right) / 2, (top + bottom) / 2) };
}

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
        // 3840 x 1080 held in a 32-bit integer would come to 4 billion
        // along the way. Counted wide, the answer is right.
        assert_eq!(across(3_000_000, 1080, 1920), 5_333_333);
    }

    #[test]
    fn the_floor_leaves_room_for_the_button_whichever_edge_is_pulled() {
        // The button is 91 pixels square on a magnified screen, plus its
        // margin. Pulling an edge sets one of the two sizes and lets the
        // other follow the shape: both must leave room for the button, or
        // else the only way out left is the keyboard.
        let room = (107, 107);
        for shape in [(1920, 1080), (1080, 1920), (2560, 1080), (1024, 1024)] {
            let (wide, high) = the_least_picture(Some(room), shape);
            assert!(wide >= room.0, "largeur {wide} sur {shape:?}");
            assert!(high >= room.1, "hauteur {high} sur {shape:?}");
            // A vertical edge pulled: the width holds, the height
            // follows.
            assert!(
                across(wide, shape.0, shape.1) >= room.1,
                "hauteur suivie sur {shape:?}"
            );
            // A horizontal edge pulled: the other way
            // round.
            assert!(
                across(high, shape.1, shape.0) >= room.0,
                "largeur suivie sur {shape:?}"
            );
        }
    }

    // The window of the drag tests: placed at (100, 100), 960x540
    // inside, a frame 16 wide and 42 high, a 16:9 picture.
    const NOW: (i32, i32, i32, i32) = (100, 100, 1076, 682);
    const FRAME: (i32, i32) = (16, 42);
    const SHAPE: (i32, i32) = (1920, 1080);

    /// What the system proposes, held to the shape, the hand being
    /// where that proposal says it is.
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
        // A vertical edge, a horizontal edge, then a corner: the
        // proposal of the system leaves the other edges where they are,
        // to the pixel.
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
        // The hand brings the bottom edge down by 200: the height
        // leads, the width follows, and the other two edges do not
        // move.
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
        // The hand pulls the top edge up: the origin moves with it, and
        // the bottom of the window stays exactly where it was.
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
        // The bottom right corner held, the hand goes straight to the
        // right, then straight down. The window grows in both cases:
        // sticking to a single side for the whole drag left one of the
        // two gestures without effect.
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
        // The gesture that made the picture shake: the hand goes down
        // diagonally, a little wider at one step, a little taller at the
        // next. The window must grow at every step, never shrink back.
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
        // And it really follows the hand: forty steps of two or three
        // pixels cannot leave the window where it was.
        assert!(widths[39] - widths[0] > 60, "{widths:?}");
    }

    #[test]
    fn the_drag_cannot_take_the_window_under_the_floor() {
        // The bottom edge pushed all the way up, then the side pushed
        // all the way in: the window stops at the size where the button
        // still fits, in height as in width.
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
        // The window goes off towards the top left, its size unchanged:
        // that is a move. Holding a shape on it corrected the origin and
        // put the window back at its starting point at every step, which
        // made it immovable.
        for elsewhere in [(60, 40), (400, 300), (100, 40), (60, 100)] {
            let carried = (elsewhere.0, elsewhere.1, 976, 582);
            assert!(
                !the_size_moves(NOW, carried),
                "déplacement pris pour un redimensionnement : {carried:?}"
            );
        }
        // And a real resize is still recognised, even by one pixel.
        assert!(the_size_moves(NOW, (100, 100, 977, 582)));
        assert!(the_size_moves(NOW, (100, 100, 976, 583)));
    }

    #[test]
    fn a_size_that_did_not_change_is_left_alone() {
        let same = (100, 100, 976, 582);
        assert_eq!(drag(same, (1, 1)), same);
    }

    #[test]
    fn the_pointer_is_kept_where_the_session_needs_it() {
        // A game shuts it on a point, the whole screen inside the
        // picture, and a window lets it go.
        assert_eq!(Cage::for_the(true, false, false, true), Cage::Point);
        assert_eq!(Cage::for_the(true, true, false, true), Cage::Point);
        assert_eq!(Cage::for_the(false, true, false, true), Cage::Picture);
        assert_eq!(Cage::for_the(false, false, false, true), Cage::Free);
        // The open menu and another program at the front give it back,
        // whatever else holds.
        for game in [false, true] {
            for whole in [false, true] {
                assert_eq!(Cage::for_the(game, whole, true, true), Cage::Free);
                assert_eq!(Cage::for_the(game, whole, false, false), Cage::Free);
            }
        }
    }

    #[test]
    fn without_a_button_the_floor_is_still_a_real_size() {
        // No button means no session, so no shape to hold: still, a size
        // of zero would mean dividing by zero further on.
        let (wide, high) = the_least_picture(None, (1920, 1080));
        assert!(wide >= 1 && high >= 1);
    }
}
