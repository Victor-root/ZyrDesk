//! What the floating button shows of the files that are arriving.
//!
//! Pasting on this computer files copied on the other one means waiting:
//! the bytes cross at the moment of the paste and not at that of the
//! copy, and they cross alongside the picture without ever cutting in
//! front of it. What this wait lacked was being seen.
//!
//! It is seen in the mark itself: the pane of the near screen fills like
//! a loading bar. No extra window, and no second drawing laid beside the
//! button.
//!
//! What is shown is what arrives **here**. When it is the far computer
//! that pastes, the wait is seen on its desktop, in the copy window
//! Windows opens itself, and that desktop is precisely the picture being
//! watched.

// Outside Windows there is no button to draw, but the reading compiles
// everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use zyr_proto::clipboard::HowFar;

/// How many times a second the bar is read again.
///
/// The service only rewrites what it wrote when the hundredth changes,
/// so reading more often would show nothing more; reading less often
/// would make a bar that moves in jumps.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// What the counter holds when nothing is arriving.
///
/// Beyond a hundred, which no hundredth ever is.
const NOTHING: u32 = u32::MAX;

/// How far along what is arriving is, in hundredths.
static COMING: AtomicU32 = AtomicU32::new(NOTHING);

/// Whether the watch is already running, one being enough per
/// session.
static WATCHING: AtomicBool = AtomicBool::new(false);

/// What the button's bar must show, from zero to one, or nothing.
pub fn how_far() -> Option<f32> {
    match COMING.load(Ordering::Relaxed) {
        NOTHING => None,
        hundredths => Some(hundredths as f32 / 100.0),
    }
}

/// Reads the progress again for as long as a session lasts, and keeps the
/// bar up to date.
///
/// Started again at every turn of the button's watch, like the badges:
/// that one turns once a second, and what is watched moving forward is
/// looked at five times as often.
pub fn watch(app: &crate::app::App) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        keep_up(&app).await;
        // The session is going away: the bar goes with it, and the
        // button learns so before it disappears.
        say(NOTHING);
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// The loop itself.
async fn keep_up(app: &crate::app::App) {
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            return;
        }
        say(coming_in().map_or(NOTHING, |far| far.hundredths()));
    }
}

/// Sets that figure, and only redraws if it has moved.
fn say(hundredths: u32) {
    if COMING.swap(hundredths, Ordering::Relaxed) != hundredths {
        #[cfg(windows)]
        crate::logo::the_bar_moved();
    }
}

/// What the service has written about the files arriving, if any are.
///
/// No file means nothing on the way: it is the service that removes it
/// when everything is there, and that is what makes the bar disappear
/// without anyone having to say it is over.
fn coming_in() -> Option<HowFar> {
    let said = std::fs::read_to_string(zyr_proto::paths::files_coming()).ok()?;
    HowFar::read(&said).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_arriving_the_bar_is_not_drawn() {
        // That is what makes it disappear by itself: the service removes
        // the file when everything is there, and there is nothing more to
        // say.
        say(NOTHING);
        assert_eq!(how_far(), None);
    }

    #[test]
    fn a_progress_reads_as_a_share_of_the_whole() {
        say(0);
        assert_eq!(how_far(), Some(0.0));
        say(50);
        assert_eq!(how_far(), Some(0.5));
        say(100);
        assert_eq!(how_far(), Some(1.0));
        say(NOTHING);
    }
}
