//! The beat of what moves on screen.
//!
//! A system timer beats no finer than its tick, fifteen and a half
//! milliseconds, and it rounds up to the next tick: a beat asked for
//! every sixteen milliseconds really lands at thirty-one, that is
//! thirty-two frames a second on a screen that shows sixty. It is visible
//! to the naked eye on anything that slides.
//!
//! So what moves beats to the pulse of the Windows compositor, which is
//! the pulse of the screen: a thread waits for the end of each
//! composition and wakes the windows that are animating something. Asking
//! for one frame more than the screen shows would not be seen and would
//! cost for nothing; asking for fewer shows at once.
//!
//! One thread for the whole product: the compositor beats for everybody
//! at once, and two threads waiting for it would be waiting for the same
//! instant.

use std::sync::{Condvar, Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;

/// The windows moving right now, and the message that redraws each of
/// them. The handle is kept as a number: that is what crosses a thread.
static MOVING: Mutex<Vec<(isize, u32)>> = Mutex::new(Vec::new());

/// What puts the thread back to sleep when nothing moves any more:
/// without it, it would spin idly at the pulse of the screen while the
/// product does nothing.
static WAKE: Condvar = Condvar::new();

/// The time of one frame when the compositor does not answer. Since
/// Windows 8 it can no longer be turned off, but a thread that spun
/// without ever waiting would take a whole core.
const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

/// Makes that window beat: it will receive this message at every frame
/// until it asks to stop.
///
/// Asking again for a window that already beats does nothing.
pub fn beat(window: HWND, message: u32) {
    let mut moving = MOVING.lock().expect("rythme");
    let this_one = window as isize;
    if moving.iter().any(|(w, _)| *w == this_one) {
        return;
    }
    moving.push((this_one, message));
    start();
    WAKE.notify_one();
}

/// Stops the beat of this window. Nothing if it was not beating.
pub fn stop(window: HWND) {
    let this_one = window as isize;
    MOVING
        .lock()
        .expect("rythme")
        .retain(|(w, _)| *w != this_one);
}

/// The thread that waits for the compositor, started at the first thing
/// that moves and kept afterwards: it sleeps as long as nothing moves.
fn start() {
    static THREAD: OnceLock<()> = OnceLock::new();
    THREAD.get_or_init(|| {
        std::thread::Builder::new()
            .name("rythme".to_string())
            .spawn(run)
            .expect("fil du rythme");
    });
}

fn run() {
    use windows_sys::Win32::Graphics::Dwm::DwmFlush;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    loop {
        let beating = {
            let mut moving = MOVING.lock().expect("rythme");
            while moving.is_empty() {
                moving = WAKE.wait(moving).expect("rythme");
            }
            moving.clone()
        };
        // SAFETY: nothing to pass it, and the wait belongs to no
        // window.
        if unsafe { DwmFlush() } < 0 {
            std::thread::sleep(FRAME);
        }
        for (window, message) in beating {
            // SAFETY: a message dropped in the queue of a window. It may
            // have gone in the meantime, and the system then answers no
            // without anything being touched.
            unsafe { PostMessageW(window as HWND, message, 0, 0) };
        }
    }
}
