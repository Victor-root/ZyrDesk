//! The floating button of a session.
//!
//! During a session the picture takes the whole screen and belongs to
//! the engine. This is the one thing of ours left on top of it: a small
//! button, hanging in a corner, that opens what can be done without
//! leaving the picture.
//!
//! It is a window of our own rather than something drawn inside the
//! picture. Drawing inside would mean teaching the engine what ZyrDesk
//! is, which is exactly what the engines are kept from knowing; and a
//! window of ours can be hit by the mouse without the engine having to
//! hand it over.
//!
//! Two things make that work, and both are why a session runs in a
//! borderless window rather than an exclusive one. A window that owns
//! the screen exclusively lets nothing be drawn above it. And the
//! pointer, in the ordinary desktop mode, stays free to leave the
//! picture: it is hidden over the picture, where the far computer's own
//! cursor stands in for it, and the system shows it again the moment it
//! crosses onto this button.
//!
//! What the menu does, it asks of the engine through the engine's own
//! keyboard shortcuts, aimed at the session window and at nothing else.

// Off Windows there is no session to float over, and the shortcut the
// letters belong to is never typed. The rest stays compiled and tested
// everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

// What the button did goes into the same journal as everything else: the
// window has nowhere else to say it, standing behind the picture, and a
// menu entry that seems to do nothing is exactly the kind of thing that
// cannot be diagnosed from a screenshot.
use crate::journal::note;

/// Name this window is known by, inside the program.
pub const WINDOW: &str = "flottant";

/// Name the page listens on to be told to show its menu.
const OPEN: &str = "floating-open";

/// How often the session is looked for.
///
/// Short enough that the button is there by the time the picture is, and
/// gone shortly after it.
const LOOK: Duration = Duration::from_secs(1);

/// Distance kept from the corner of the picture.
const MARGIN: i32 = 16;

/// Size of the button alone, in real pixels, before the page has had a
/// chance to measure itself.
const BUTTON: u32 = 52;

/// How far the mouse has to travel, while holding the button, before it
/// is a drag and no longer a click.
///
/// Without it a hand that shakes would move the button every time
/// somebody wanted to open the menu, and the other way round.
const GRIP: i32 = 4;

/// How often the button catches up with the mouse while being dragged.
const FOLLOW: Duration = Duration::from_millis(8);

/// How long the picture is given to come back in front before an entry
/// of the menu gives up on it.
const FRONT_TAKES: Duration = Duration::from_millis(400);

/// Pause between two looks at which window is in front.
const FRONT_STEP: Duration = Duration::from_millis(10);

/// How long the picture is given to go once the far computer has been
/// asked to hand its desktop back.
const CLOSING_SHOWS: Duration = Duration::from_secs(3);

/// How long a drag may last before it is called over.
///
/// A mouse unplugged mid-drag, or a button released where nothing
/// noticed, would otherwise leave the button following a cursor nobody
/// is holding for as long as the program runs.
const AT_MOST: Duration = Duration::from_secs(60);

/// What the menu can ask of the session.
///
/// All but the last are shortcuts the engine already answers to, so
/// nothing here asks it to learn anything new. Leaving and closing are
/// two entries and not one: leaving keeps the far computer's desktop
/// open and waiting, closing hands it back.
#[derive(Clone, Copy)]
pub enum Act {
    Fullscreen,
    Stats,
    MouseMode,
    Leave,
    Close,
}

impl Act {
    fn read(name: &str) -> Option<Self> {
        match name {
            "fullscreen" => Some(Act::Fullscreen),
            "stats" => Some(Act::Stats),
            "mouse" => Some(Act::MouseMode),
            "leave" => Some(Act::Leave),
            "close" => Some(Act::Close),
            _ => None,
        }
    }

    /// Letter of the engine's Ctrl+Alt+Shift shortcut, for the ones that
    /// have one.
    fn letter(self) -> Option<u8> {
        match self {
            Act::Fullscreen => Some(b'X'),
            Act::Stats => Some(b'S'),
            Act::MouseMode => Some(b'M'),
            Act::Leave => Some(b'Q'),
            // Asked of the far computer over the tunnel, not of the
            // player through its keyboard.
            Act::Close => None,
        }
    }

    /// Where that letter sits on the keyboard.
    ///
    /// The engine is built on a library that reads a key by its place
    /// before it reads it by its name, and the two come apart on the
    /// keyboards this product is used on: the key engraved A in France
    /// is the key engraved Q elsewhere. A key sent by name leaves the
    /// place to be worked out by whatever the system happens to think
    /// the keyboard is, which is one guess too many for a keystroke
    /// nobody typed.
    fn where_it_sits(self) -> Option<u16> {
        match self {
            Act::Fullscreen => Some(0x2D),
            Act::Stats => Some(0x1F),
            Act::MouseMode => Some(0x32),
            Act::Leave => Some(0x10),
            Act::Close => None,
        }
    }
}

impl std::fmt::Display for Act {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Act::Fullscreen => "plein écran",
            Act::Stats => "statistiques",
            Act::MouseMode => "mode de la souris",
            Act::Leave => "départ de la session",
            Act::Close => "fermeture sur l'ordinateur distant",
        })
    }
}

/// The session the button belongs to.
#[derive(Default)]
pub struct Floating {
    watched: Mutex<Option<Watched>>,
    /// Set while this window is asking the far computer to close the
    /// session.
    ///
    /// The engine loses its stream when that happens and stops on a
    /// failure, which from the outside is exactly what a session that
    /// broke looks like. Without this, the one thing the person asked for
    /// would be reported back to them as an error.
    closing: std::sync::atomic::AtomicBool,
    /// Where the person dragged the button last, as a distance from the
    /// corner of the picture. Kept for as long as the program runs, so
    /// it does not walk back to the corner at every session.
    nudge: Mutex<(i32, i32)>,
    /// Player this window has just started and the service does not know
    /// about yet.
    ///
    /// A session is only handed to the service once it has been watched
    /// long enough to be believed, and the button would arrive that many
    /// seconds after the picture. Whoever started the engine knows its
    /// number straight away, and the button hangs on nothing else.
    expected: Mutex<Option<u32>>,
}

impl Floating {
    /// Says a close is being asked for, and takes it back when it was
    /// refused: a session still running must be told apart from one this
    /// window brought down.
    fn closing(app: &AppHandle, asked: bool) {
        app.state::<Floating>()
            .closing
            .store(asked, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether the session that just ended was closed on purpose, and
    /// forgets it either way.
    pub fn was_closed_on_purpose(app: &AppHandle) -> bool {
        app.state::<Floating>()
            .closing
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}

struct Watched {
    /// Player the button hangs on, and the only window our keystrokes
    /// may reach.
    process: u32,
    /// Corner it hangs from, in real pixels: the top right of the
    /// picture, brought in by a margin and by whatever dragging moved
    /// it since.
    anchor: (i32, i32),
}

/// Says which player this window has just started, before anybody else
/// knows.
pub fn expect(app: &AppHandle, process: u32) {
    *app.state::<Floating>()
        .expected
        .lock()
        .expect("session attendue") = Some(process);
}

/// Forgets it, the session being over one way or another.
pub fn expect_nothing(app: &AppHandle) {
    *app.state::<Floating>()
        .expected
        .lock()
        .expect("session attendue") = None;
}

/// The player the button belongs to right now.
///
/// The service first: it knows every session on this computer, including
/// those another window opened. Failing that, the one this window has
/// just started, for as long as it has a picture up. That second answer
/// is what puts the button on screen with the picture rather than
/// several seconds behind it.
async fn player(app: &AppHandle) -> Option<u32> {
    if let Some(session) = crate::session::sessions().await.into_iter().next() {
        return Some(session.process);
    }
    let expected = *app
        .state::<Floating>()
        .expected
        .lock()
        .expect("session attendue");
    expected.filter(|process| picture_of(*process).is_some())
}

/// Follows the sessions for as long as the program runs, and puts the
/// button up and down with them.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(LOOK).await;
            match player(&app).await {
                Some(process) => raise(&app, process),
                None => lower(&app),
            }
        }
    });
}

/// Puts the button up for that player, if it is not up already.
///
/// Waits for the player to have a window before showing anything. The
/// service calls a session held from the moment the player starts, but
/// the engine only opens its window once the far computer has answered
/// and the stream stands: showing the button any earlier would put it
/// over a screen that has no picture on it yet.
fn raise(app: &AppHandle, process: u32) {
    let state = app.state::<Floating>();
    let mut watched = state.watched.lock().expect("session suivie");
    if watched.as_ref().is_some_and(|seen| seen.process == process) {
        return;
    }

    let Some(picture) = picture_of(process) else {
        return;
    };
    let nudge = *state.nudge.lock().expect("position du bouton");
    let anchor = hung_from(picture, nudge);
    *watched = Some(Watched { process, anchor });
    drop(watched);

    // A leftover window from a session that ended in a way we did not
    // see: put it where the new one is rather than open a second.
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.set_position(PhysicalPosition::new(anchor.0 - BUTTON as i32, anchor.1));
        let _ = window.show();
        return;
    }

    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("bouton.html".into()))
        .title("ZyrDesk")
        .inner_size(f64::from(BUTTON), f64::from(BUTTON))
        .position(f64::from(anchor.0 - BUTTON as i32), f64::from(anchor.1))
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        // Never takes the picture's place: the engine keeps the keyboard
        // and the mouse, and this only catches what is clicked on it.
        .focused(false)
        .build();

    match built {
        Ok(window) => {
            // Everything here is counted in real pixels, and the size
            // asked for at build time is counted in the other kind. On a
            // screen at a hundred and seventy-five per cent the window
            // came out that much bigger than the button drawn in it, and
            // what showed around the button was the window itself.
            let _ = window.set_size(PhysicalSize::new(BUTTON, BUTTON));
            keep_out_of_the_way(&window);
        }
        // A button that could not be drawn is not a reason to disturb a
        // session that is otherwise fine.
        Err(e) => eprintln!("le bouton flottant n'a pas pu s'ouvrir : {e}"),
    }
}

/// Takes the button down.
fn lower(app: &AppHandle) {
    let state = app.state::<Floating>();
    if state
        .watched
        .lock()
        .expect("session suivie")
        .take()
        .is_none()
    {
        return;
    }
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.close();
    }
    // The home window had stepped aside for the session, and only the
    // button was keeping the program up. Nothing is left to keep.
    if crate::home_is_hidden(app) {
        app.exit(0);
    }
}

/// Resizes the button to what the page turned out to need, keeping the
/// corner it hangs from.
///
/// The page measures itself rather than being told a size: the menu's
/// height depends on what is in it, and a number written twice would
/// stop matching the first time an entry is added.
#[tauri::command]
pub fn floating_size(app: AppHandle, width: u32, height: u32) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|e| e.to_string())?;

    let anchor = app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .map(|watched| watched.anchor);
    let Some((right, top)) = anchor else {
        return Ok(());
    };
    let size = window.outer_size().map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(right - size.width as i32, top))
        .map_err(|e| e.to_string())
}

/// Hides the button until the next session.
#[tauri::command]
pub fn floating_hide(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    window.hide().map_err(|e| e.to_string())
}

/// Takes hold of the button, moves it with the mouse until it is let go,
/// and says whether the whole thing turned out to be a plain click.
///
/// The gesture is followed here and not in the page. That window is
/// fifty pixels wide: the mouse leaves it on the first movement, and
/// what a web view reports as the place of a pointer is not always where
/// that pointer is on the screen. Where the system says the cursor is is
/// neither of those things, and it is always true.
///
/// Nothing is asked of the page while this runs, and the menu is left
/// open if it was: a window that changes size under the mouse gets away
/// from it.
#[tauri::command]
pub async fn floating_grab(app: AppHandle) -> Result<bool, String> {
    let held = {
        let state = app.state::<Floating>();
        let watched = state.watched.lock().expect("session suivie");
        watched.as_ref().map(|seen| (seen.process, seen.anchor))
    };
    let Some((process, from)) = held else {
        return Ok(true);
    };
    // The picture is read once: it does not move while the button is
    // being dragged over it, and looking for it again at every step
    // would mean enumerating every window on the machine a hundred times
    // a second.
    let (Some(start), Some(picture)) = (cursor_now(), picture_of(process)) else {
        return Ok(true);
    };

    let until = std::time::Instant::now() + AT_MOST;
    let mut moved = false;
    while held_down() && std::time::Instant::now() < until {
        let Some(now) = cursor_now() else {
            break;
        };
        let (dx, dy) = (now.0 - start.0, now.1 - start.1);
        if moved || dx.abs() >= GRIP || dy.abs() >= GRIP {
            moved = true;
            slide(&app, picture, from, dx, dy)?;
        }
        tokio::time::sleep(FOLLOW).await;
    }
    Ok(!moved)
}

/// Puts the button where the mouse has dragged it to.
///
/// The distance from the corner of the picture is what is remembered
/// rather than the place on screen: a session opened later on another
/// screen, or at another size, then finds the button where it was left
/// rather than off the edge.
fn slide(
    app: &AppHandle,
    picture: (i32, i32, i32, i32),
    from: (i32, i32),
    dx: i32,
    dy: i32,
) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let anchor = held_inside(
        (from.0 + dx, from.1 + dy),
        picture,
        size.width as i32,
        size.height as i32,
    );

    let state = app.state::<Floating>();
    if let Some(seen) = state.watched.lock().expect("session suivie").as_mut() {
        seen.anchor = anchor;
    }
    *state.nudge.lock().expect("position du bouton") = (
        anchor.0 - (picture.2 - MARGIN),
        anchor.1 - (picture.1 + MARGIN),
    );

    window
        .set_position(PhysicalPosition::new(
            anchor.0 - size.width as i32,
            anchor.1,
        ))
        .map_err(|e| e.to_string())
}

/// Where the button hangs: the top right of the picture, moved by
/// whatever dragging has moved it since.
fn hung_from(picture: (i32, i32, i32, i32), nudge: (i32, i32)) -> (i32, i32) {
    let corner = (picture.2 - MARGIN + nudge.0, picture.1 + MARGIN + nudge.1);
    held_inside(corner, picture, BUTTON as i32, BUTTON as i32)
}

/// Keeps the button against the picture, whatever it was asked.
///
/// A button dragged towards an edge, or a session opened on a smaller
/// screen than the last, would otherwise end up somewhere nobody can
/// click it.
fn held_inside(
    corner: (i32, i32),
    picture: (i32, i32, i32, i32),
    width: i32,
    height: i32,
) -> (i32, i32) {
    let (left, top, right, bottom) = picture;
    (
        corner.0.clamp(left + width, right),
        corner.1.clamp(top, (bottom - height).max(top)),
    )
}

/// Brings the button back and opens its menu.
///
/// What a shortcut needs to be able to do above all else: hiding the
/// button is otherwise a decision with no way back before the session
/// ends.
pub fn show_the_menu(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("aucune session en cours")?;
    window.show().map_err(|e| e.to_string())?;
    window.emit(OPEN, ()).map_err(|e| e.to_string())
}

/// Asks the session for something, in its own language.
#[tauri::command]
pub async fn floating_act(app: AppHandle, what: String) -> Result<(), String> {
    let act = Act::read(&what).ok_or_else(|| format!("action inconnue : {what}"))?;
    ask(&app, act).await
}

/// The same, from anywhere in the program rather than from the menu.
pub async fn ask(app: &AppHandle, act: Act) -> Result<(), String> {
    let process = app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .map(|watched| watched.process)
        .ok_or("aucune session en cours")?;

    match act.letter() {
        Some(_) => {
            put_the_picture_in_front(process).await?;
            shortcut(act, process)
        }
        None => close_on_the_far_computer(app).await,
    }
}

/// Puts the picture back in front, and waits for it to actually be
/// there.
///
/// Keystrokes go to whatever window is in front. Clicking the button is
/// not supposed to move the picture out of the way, but a web view can
/// take the focus on its own; bringing the picture back is both the fix
/// and what the person expects after using the menu.
///
/// The waiting is the whole point. Windows does not always change the
/// front window on the spot: the ask is posted, the call returns, and a
/// window read straight afterwards is still the old one. Asking once and
/// believing the answer immediately is why every entry in this menu
/// reported that the picture was not in front and did nothing.
async fn put_the_picture_in_front(process: u32) -> Result<(), String> {
    if in_front(process) {
        return Ok(());
    }
    bring_forward(process);

    let until = std::time::Instant::now() + FRONT_TAKES;
    while std::time::Instant::now() < until {
        tokio::time::sleep(FRONT_STEP).await;
        if in_front(process) {
            return Ok(());
        }
    }

    note(&format!(
        "la fenêtre du lecteur {process} n'est pas passée au premier plan"
    ));
    Err("la fenêtre de la session n'est pas au premier plan.\n  \
         Cliquez d'abord dans l'image."
        .to_string())
}

/// Whether that player has stopped showing anything.
///
/// Given a moment: the picture does not go the instant the far computer
/// is asked to let go of its desktop.
async fn the_picture_is_gone(process: u32) -> bool {
    let until = std::time::Instant::now() + CLOSING_SHOWS;
    while std::time::Instant::now() < until {
        if picture_of(process).is_none() {
            return true;
        }
        tokio::time::sleep(FRONT_STEP).await;
    }
    false
}

/// Hands the far computer's desktop back, instead of merely leaving it.
///
/// Where to ask comes from the service rather than from anything this
/// window remembers: the tunnel address is the only one the engine can
/// reach that computer at, and it exists for exactly as long as the way
/// does.
async fn close_on_the_far_computer(app: &AppHandle) -> Result<(), String> {
    let session = crate::session::sessions()
        .await
        .into_iter()
        .next()
        .ok_or("aucune session en cours")?;

    note(&format!(
        "fermeture demandée sur {} à travers {}",
        session.towards, session.at
    ));
    // Said before the asking. The engine can lose its stream and stop
    // before the far computer has finished answering, and a session
    // reported as broken to whoever just closed it would be a lie.
    Floating::closing(app, true);

    let process = session.process;
    // On a thread of its own: this asks the far computer a question over
    // the network, and the window must not stop drawing while it waits.
    let answered = tauri::async_runtime::spawn_blocking(move || {
        zyr_session::close_on_the_far_computer(&session.towards, &session.at)
    })
    .await;

    match answered {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            // Asking this well is what takes the answer away. The far
            // computer lets its desktop go, the stream stops, the tunnel
            // that carried the question goes with it, and nothing comes
            // back. A silence that leaves no picture behind is the thing
            // having worked, and saying otherwise would put a red line
            // across the screen every time it did.
            if the_picture_is_gone(process).await {
                note(&format!("fermeture faite, sans réponse ({e})"));
                return Ok(());
            }
            note(&format!("fermeture refusée : {e}"));
            // Refused, so the session is still standing: whatever befalls
            // it later is nobody's doing but its own.
            Floating::closing(app, false);
            Err(e.to_string())
        }
        Err(e) => {
            Floating::closing(app, false);
            Err(e.to_string())
        }
    }
}

/* ---- Ce qui appartient à Windows ------------------------------------- */

/// Keeps the button from ever taking the place of the picture.
///
/// Without this, clicking it would put the session window in the
/// background: the engine would let go of the keyboard, and the next
/// keystroke would land who knows where.
#[cfg(windows)]
fn keep_out_of_the_way(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    let Ok(handle) = window.hwnd() else {
        return;
    };
    // SAFETY: the handle belongs to a window we have just built, and
    // only its extended style is read and written back.
    unsafe {
        let style = GetWindowLongPtrW(handle.0 as _, GWL_EXSTYLE);
        SetWindowLongPtrW(
            handle.0 as _,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
}

#[cfg(not(windows))]
fn keep_out_of_the_way(_window: &tauri::WebviewWindow) {}

/// Where the mouse is on the screen, in real pixels.
#[cfg(windows)]
fn cursor_now() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: the slot is ours, and a refusal is one of the answers.
    if unsafe { GetCursorPos(&mut point) } != 0 {
        Some((point.x, point.y))
    } else {
        None
    }
}

/// Whether the left mouse button is down right now.
///
/// Asked of the system rather than waited for as an event: the window
/// this is dragging is too small to keep the mouse inside it, and a
/// release that happened over the picture is a release all the same.
#[cfg(windows)]
fn held_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

    // SAFETY: no argument, and the answer is a plain bit field.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON)) };
    state as u16 & 0x8000 != 0
}

#[cfg(not(windows))]
fn cursor_now() -> Option<(i32, i32)> {
    None
}

#[cfg(not(windows))]
fn held_down() -> bool {
    false
}

/// That player's biggest window on screen, and where it sits.
///
/// The biggest rather than the first: the engine keeps a few small
/// windows of its own, and the picture is never the small one.
#[cfg(windows)]
fn biggest_window_of(
    process: u32,
) -> Option<(
    windows_sys::Win32::Foundation::HWND,
    windows_sys::Win32::Foundation::RECT,
)> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };
    use windows_sys::core::BOOL;

    struct Looking {
        process: u32,
        widest: i64,
        found: Option<(HWND, RECT)>,
    }

    unsafe extern "system" fn consider(window: HWND, carried: LPARAM) -> BOOL {
        // SAFETY: the pointer is the one handed to EnumWindows just
        // below, and lives for the whole of the call.
        let looking = unsafe { &mut *(carried as *mut Looking) };

        let mut owner = 0u32;
        // SAFETY: the window comes from the enumeration and the slot is
        // ours.
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        // SAFETY: same window.
        if owner != looking.process || unsafe { IsWindowVisible(window) } == 0 {
            return TRUE;
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: same window, and the rectangle is ours.
        if unsafe { GetWindowRect(window, &mut rect) } != 0 {
            let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
            if area > looking.widest {
                looking.widest = area;
                looking.found = Some((window, rect));
            }
        }
        TRUE
    }

    let mut looking = Looking {
        process,
        widest: 0,
        found: None,
    };
    // SAFETY: the callback above is what reads the pointer, and the
    // enumeration is over before this function returns.
    unsafe { EnumWindows(Some(consider), &mut looking as *mut Looking as LPARAM) };
    looking.found
}

#[cfg(windows)]
fn main_window_of(process: u32) -> Option<windows_sys::Win32::Foundation::HWND> {
    biggest_window_of(process).map(|(window, _)| window)
}

/// Where that player's picture is, as left, top, right and bottom in
/// real pixels.
#[cfg(windows)]
fn picture_of(process: u32) -> Option<(i32, i32, i32, i32)> {
    biggest_window_of(process)
        .map(|(_, rect)| (rect.left, rect.top, rect.right, rect.bottom))
        .filter(|(left, top, right, bottom)| right > left && bottom > top)
}

#[cfg(not(windows))]
fn picture_of(_process: u32) -> Option<(i32, i32, i32, i32)> {
    None
}

/// Types the engine's shortcut, at the session and nowhere else.
#[cfg(windows)]
fn shortcut(act: Act, process: u32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
        SendInput,
    };

    // The picture was brought in front and waited for by the caller. If
    // it slipped away since, the keys would land in someone else's lap,
    // and a quit combo in the wrong window is not a mistake worth
    // risking.
    if !in_front(process) {
        note(&format!(
            "{act} refusé : la fenêtre au premier plan n'est pas celle du lecteur {process}"
        ));
        return Err("la fenêtre de la session n'est pas au premier plan.\n  \
             Cliquez d'abord dans l'image."
            .to_string());
    }

    let (Some(letter), Some(key)) = (act.letter(), act.where_it_sits()) else {
        return Ok(());
    };
    // Where the modifiers sit, in the same numbering: the left-hand ones,
    // which is what a person would press.
    const CTRL: u16 = 0x1D;
    const ALT: u16 = 0x38;
    const SHIFT: u16 = 0x2A;

    let keys = [CTRL, ALT, SHIFT, key, key, SHIFT, ALT, CTRL];
    let events: Vec<INPUT> = keys
        .iter()
        .enumerate()
        .map(|(rank, key)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    // The place is what is sent, and the name is left for
                    // the far end to work out from its own keyboard.
                    wVk: 0,
                    wScan: *key,
                    // The first half presses, the second half releases,
                    // in the mirror order: no key is left down.
                    dwFlags: KEYEVENTF_SCANCODE
                        | if rank >= keys.len() / 2 {
                            KEYEVENTF_KEYUP
                        } else {
                            0
                        },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        })
        .collect();

    // SAFETY: the events are ours and well formed, and their size is the
    // one the call is told to expect.
    let sent = unsafe {
        SendInput(
            events.len() as u32,
            events.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize == events.len() {
        // Said out loud because nothing else can say it: if the picture
        // does not react, this line is what tells a keystroke that never
        // left from one the engine chose to ignore.
        note(&format!(
            "{act} envoyé au lecteur {process} : Ctrl+Alt+Maj+{}, à la place {key:#04x}",
            char::from(letter)
        ));
        Ok(())
    } else {
        note(&format!(
            "{act} refusé par Windows pour le lecteur {process}"
        ));
        Err("Windows a refusé la combinaison de touches".to_string())
    }
}

/// Brings that player's picture back in front.
#[cfg(windows)]
fn bring_forward(process: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let Some(window) = main_window_of(process) else {
        return;
    };
    // SAFETY: the window comes from the enumeration just above. Windows
    // may refuse to change the foreground, which the caller checks for
    // rather than trusts.
    unsafe { SetForegroundWindow(window) };
}

/// Whether the window in front belongs to that process.
#[cfg(windows)]
fn in_front(process: u32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: no argument, and a null answer is one of the answers.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return false;
    }
    let mut owner = 0u32;
    // SAFETY: the window comes from the call above and the slot is ours.
    unsafe { GetWindowThreadProcessId(window, &mut owner) };
    owner == process
}

#[cfg(not(windows))]
fn shortcut(_act: Act, _process: u32) -> Result<(), String> {
    Err("les sessions ne tournent que sous Windows".to_string())
}

#[cfg(not(windows))]
fn bring_forward(_process: u32) {}

#[cfg(not(windows))]
fn in_front(_process: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_entry_names_a_shortcut_the_engine_answers_to() {
        // Les lettres sont celles du moteur client : les changer sans le
        // moteur ferait taper une combinaison qui ne fait rien, ou pire,
        // une autre que celle voulue.
        // Et les places sont celles d'un clavier, indépendantes de ce
        // qui est gravé dessus : c'est par là que le moteur reconnaît
        // une touche en premier.
        for (name, letter, place) in [
            ("fullscreen", b'X', 0x2Du16),
            ("stats", b'S', 0x1F),
            ("mouse", b'M', 0x32),
            ("leave", b'Q', 0x10),
        ] {
            let act = Act::read(name).expect(name);
            assert_eq!(act.letter(), Some(letter), "sur « {name} »");
            assert_eq!(act.where_it_sits(), Some(place), "sur « {name} »");
        }
        // Fermer pour de bon ne passe pas par le clavier du lecteur : ça
        // se demande à l'ordinateur d'en face, à travers le tunnel.
        assert_eq!(Act::read("close").expect("close").letter(), None);
        assert!(Act::read("teleport").is_none());
    }
}
