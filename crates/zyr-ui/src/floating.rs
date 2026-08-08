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

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

/// Name this window is known by, inside the program.
pub const WINDOW: &str = "flottant";

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

/// What the menu can ask of the session.
///
/// Each one is a shortcut the engine already answers to, so nothing here
/// asks the engine to learn anything new.
#[derive(Clone, Copy)]
enum Act {
    Fullscreen,
    Stats,
    MouseMode,
    Stop,
}

impl Act {
    fn read(name: &str) -> Option<Self> {
        match name {
            "fullscreen" => Some(Act::Fullscreen),
            "stats" => Some(Act::Stats),
            "mouse" => Some(Act::MouseMode),
            "stop" => Some(Act::Stop),
            _ => None,
        }
    }

    /// Letter of the engine's Ctrl+Alt+Shift shortcut.
    fn letter(self) -> u16 {
        match self {
            Act::Fullscreen => b'X' as u16,
            Act::Stats => b'S' as u16,
            Act::MouseMode => b'M' as u16,
            Act::Stop => b'Q' as u16,
        }
    }
}

/// The session the button belongs to.
#[derive(Default)]
pub struct Floating {
    watched: Mutex<Option<Watched>>,
}

struct Watched {
    /// Player the button hangs on, and the only window our keystrokes
    /// may reach.
    process: u32,
    /// Corner it hangs from, in real pixels: the top right of the
    /// picture, brought in by a margin.
    anchor: (i32, i32),
}

/// Follows the sessions for as long as the program runs, and puts the
/// button up and down with them.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(LOOK).await;
            let session = crate::session::sessions().await.into_iter().next();
            match session {
                Some(session) => raise(&app, session.process),
                None => lower(&app),
            }
        }
    });
}

/// Puts the button up for that player, if it is not up already.
fn raise(app: &AppHandle, process: u32) {
    let state = app.state::<Floating>();
    let mut watched = state.watched.lock().expect("session suivie");
    if watched.as_ref().is_some_and(|seen| seen.process == process) {
        return;
    }

    let anchor = corner_of(process).unwrap_or_else(|| screen_corner(app));
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
        Ok(window) => keep_out_of_the_way(&window),
        // A button that could not be drawn is not a reason to disturb a
        // session that is otherwise fine.
        Err(e) => eprintln!("le bouton flottant n'a pas pu s'ouvrir : {e}"),
    }
}

/// Whether a session is under way, and the button with it.
pub fn busy(app: &AppHandle) -> bool {
    app.state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .is_some()
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
pub fn floating_size(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
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

/// Asks the session for something, in its own language.
#[tauri::command]
pub fn floating_act(app: AppHandle, what: String) -> Result<(), String> {
    let act = Act::read(&what).ok_or_else(|| format!("action inconnue : {what}"))?;
    let process = app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .map(|watched| watched.process)
        .ok_or("aucune session en cours")?;
    shortcut(act, process)
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

/// The top right corner of that player's window, in real pixels.
#[cfg(windows)]
fn corner_of(process: u32) -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Looking {
        process: u32,
        found: Option<RECT>,
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
        if unsafe { GetWindowRect(window, &mut rect) } != 0 && rect.right > rect.left {
            looking.found = Some(rect);
            return 0;
        }
        TRUE
    }

    let mut looking = Looking {
        process,
        found: None,
    };
    // SAFETY: the callback above is what reads the pointer, and the
    // enumeration is over before this function returns.
    unsafe { EnumWindows(Some(consider), &mut looking as *mut Looking as LPARAM) };
    looking
        .found
        .map(|rect| (rect.right - MARGIN, rect.top + MARGIN))
}

#[cfg(not(windows))]
fn corner_of(_process: u32) -> Option<(i32, i32)> {
    None
}

/// Where to hang the button when the session's own window cannot be
/// found: the corner of the screen, which is where it would have been.
fn screen_corner(app: &AppHandle) -> (i32, i32) {
    match app.primary_monitor() {
        Ok(Some(monitor)) => {
            let position = monitor.position();
            let size = monitor.size();
            (position.x + size.width as i32 - MARGIN, position.y + MARGIN)
        }
        _ => (MARGIN, MARGIN),
    }
}

/// Types the engine's shortcut, at the session and nowhere else.
#[cfg(windows)]
fn shortcut(act: Act, process: u32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CONTROL,
        VK_MENU, VK_SHIFT,
    };

    // Keystrokes go to whatever window is in front. If that is not the
    // session, they would land in someone else's lap: a quit combo in
    // the wrong window is not a mistake worth risking.
    if !in_front(process) {
        return Err("la fenêtre de la session n'est pas au premier plan.\n  \
             Cliquez d'abord dans l'image."
            .to_string());
    }

    let keys = [
        VK_CONTROL,
        VK_MENU,
        VK_SHIFT,
        act.letter(),
        act.letter(),
        VK_SHIFT,
        VK_MENU,
        VK_CONTROL,
    ];
    let events: Vec<INPUT> = keys
        .iter()
        .enumerate()
        .map(|(rank, key)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: *key,
                    wScan: 0,
                    // The first half presses, the second half releases,
                    // in the mirror order: no key is left down.
                    dwFlags: if rank >= keys.len() / 2 {
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
        Ok(())
    } else {
        Err("Windows a refusé la combinaison de touches".to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_entry_names_a_shortcut_the_engine_answers_to() {
        // Les lettres sont celles du moteur client : les changer sans le
        // moteur ferait taper une combinaison qui ne fait rien, ou pire,
        // une autre que celle voulue.
        for (name, letter) in [
            ("fullscreen", b'X'),
            ("stats", b'S'),
            ("mouse", b'M'),
            ("stop", b'Q'),
        ] {
            let act = Act::read(name).expect(name);
            assert_eq!(act.letter(), u16::from(letter), "sur « {name} »");
        }
        assert!(Act::read("teleport").is_none());
    }
}
