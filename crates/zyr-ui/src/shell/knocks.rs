//! A contact knocking at this computer: noticed, put in front of the
//! person, and answered.
//!
//! The service holds a contact at the door until someone here says yes,
//! and asks nobody: what is waiting is read from here, twice a second,
//! and the question opens over the home window, brought in front. Ctrl+F1
//! answers yes from anywhere, a game in front included, and is held from
//! the system only while somebody waits: the rest of the time it belongs
//! to whatever else uses it.

// Holding a combination is a Windows matter; the watching is compiled and
// checked everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::Duration;

use crate::shell::app::App;

/// What this module's lines are filed under.
const TAG: &str = "knocks";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::shell::journal::note_about(TAG, what);
}

/// How often what waits at the door is read: a contact waits half a
/// minute at most, and a question that came late would eat into it.
const LOOK: Duration = Duration::from_millis(500);

/// Keeps an eye on the door for as long as the program runs.
///
/// One question at a time, the first come first: the next opens once
/// this one is answered or gone.
pub fn watch(app: App) {
    crate::shell::app::spawn(async move {
        let mut shown: Option<u64> = None;
        loop {
            let waiting = crate::shell::desk::knocks().await;
            hold_the_key(!waiting.is_empty());
            if let Some(id) = shown
                && waiting.iter().all(|knock| knock.id != id)
            {
                shown = None;
                let held = app.clone();
                let _ = app.run_on_main_thread(move || crate::home::no_longer_knocking(&held, id));
            }
            if let (None, Some(knock)) = (shown, waiting.first().cloned()) {
                note(&format!("{} asks to come in", knock.who));
                shown = Some(knock.id);
                let held = app.clone();
                let _ = app.run_on_main_thread(move || {
                    crate::home::knocked(&held, knock.id, knock.who, knock.rights);
                });
            }
            tokio::time::sleep(LOOK).await;
        }
    });
}

/// Lets in the first contact waiting, for Ctrl+F1.
fn let_the_first_in(app: &App) {
    let app = app.clone();
    crate::shell::app::spawn(async move {
        let Some(first) = crate::shell::desk::knocks().await.into_iter().next() else {
            return;
        };
        let held = app.clone();
        let _ = app.run_on_main_thread(move || {
            crate::home::no_longer_knocking(&held, first.id);
        });
        if let Err(e) = crate::shell::desk::accord(first.id, true).await {
            note(&format!("Ctrl+F1 let nobody in: {e}"));
        }
    });
}

/// The thread holding Ctrl+F1, once started.
#[cfg(windows)]
static KEY: std::sync::Mutex<Option<(u32, bool)>> = std::sync::Mutex::new(None);

/// Asks the thread to take the combination, or to give it back.
#[cfg(windows)]
const HOLD: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 21;

/// The number the combination is held under, on its thread.
#[cfg(windows)]
const CTRL_F1: i32 = 1;

/// Starts the thread that holds Ctrl+F1 when asked to.
#[cfg(windows)]
pub fn listen(app: App) {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;

    std::thread::Builder::new()
        .name("zyrdesk-accord-key".to_string())
        .spawn(move || {
            // SAFETY: plain call about this very thread.
            let thread = unsafe { GetCurrentThreadId() };
            *KEY.lock().expect("accord key") = Some((thread, false));
            hold_until_told(&app);
        })
        .map(|_| ())
        .unwrap_or_else(|e| note(&format!("Ctrl+F1 will not let anybody in: {e}")));
}

#[cfg(not(windows))]
pub fn listen(_app: App) {}

/// Takes Ctrl+F1 from the system while somebody waits, and gives it back
/// after: told only when that changes.
#[cfg(windows)]
fn hold_the_key(wanted: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

    let mut key = KEY.lock().expect("accord key");
    let Some((thread, held)) = key.as_mut() else {
        return;
    };
    if *held == wanted {
        return;
    }
    *held = wanted;
    // SAFETY: the thread is the one this program started above, and the
    // message carries a plain number.
    unsafe { PostThreadMessageW(*thread, HOLD, usize::from(wanted), 0) };
}

#[cfg(not(windows))]
fn hold_the_key(_wanted: bool) {}

/// Waits for what it is told and for the combination, on the thread the
/// combination is held by.
#[cfg(windows)]
fn hold_until_told(app: &App) {
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_F1,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    let mut message = MSG {
        hwnd: null_mut(),
        message: 0,
        wParam: 0,
        lParam: 0,
        time: 0,
        pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
    };
    let mut holding = false;
    // SAFETY: the slot is ours, and no window means every message of
    // this thread, which is where the combination lands.
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        match message.message {
            HOLD if message.wParam == 1 && !holding => {
                // SAFETY: no window, so the combination belongs to this
                // thread, and the number is ours.
                holding = unsafe {
                    RegisterHotKey(
                        null_mut(),
                        CTRL_F1,
                        MOD_CONTROL | MOD_NOREPEAT,
                        u32::from(VK_F1),
                    )
                } != 0;
                if !holding {
                    note("Ctrl+F1 is held by another program: answer in the window");
                }
            }
            HOLD if message.wParam == 0 && holding => {
                // SAFETY: the number is the one this thread held above.
                unsafe { UnregisterHotKey(null_mut(), CTRL_F1) };
                holding = false;
            }
            WM_HOTKEY if message.wParam == CTRL_F1 as usize => let_the_first_in(app),
            _ => {}
        }
    }
}
