//! What shape the pointer has on this computer.
//!
//! A desktop says what a click is about to do through the shape of the
//! pointer and through almost nothing else: an upright bar means the
//! click lands between two letters, a hand means something to follow, a
//! ring means wait. The computer watching draws its own pointer, so that
//! it answers the hand with no network in between; without this it would
//! answer with an arrow and nothing else, whatever is under it.
//!
//! One moment answers no shape at all, and it is the one moment this
//! computer draws its pointer into the picture itself: a window being
//! dragged. See `zyr_system::pointer_shape`.
//!
//! Nothing here is done to the machine. It is a reading, and the whole
//! module exists because of where the reading has to happen. A service
//! sits in a session with no screen, no keyboard and no pointer, on a
//! window station carrying none of them, and the desktop that owns the
//! input belongs to another session entirely: it cannot be opened from
//! here, and no right makes it so. It is the same blindness that made
//! this computer answer that it had no screens, and it has the same
//! answer: this program is started again in the session that owns the
//! screen, and it reads from there.
//!
//! That helper writes one word to a file and the service reads it. A
//! file rather than anything cleverer, for the reason everything else
//! between these two programs is a file: it can be read with the eyes,
//! and it survives whoever wrote it.
//!
//! It only runs while somebody is asking. Its life is short and it is
//! started again for as long as the questions keep coming, so a service
//! that stops asking, or that stops altogether, leaves nothing behind for
//! more than a few seconds. A machine nobody is watching reads nothing.

// Outside Windows nothing calls this module: the service does not exist
// there. The shape of it stays compiled and tested everywhere, and the
// reading itself is the one part that cannot be.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::log::Log;
use zyr_proto::session::Pointer;

/// What this module's lines are filed under.
const TAG: &str = "pointer";

/// How often the helper reads the pointer.
///
/// About a drawn frame. The answer travels to another computer and is
/// drawn there, so reading faster than that machine can show it buys
/// nothing; reading much slower is a hand that reaches a text field and
/// waits to be told.
const READ_EVERY: Duration = Duration::from_millis(30);

/// How long a helper lives before it goes home of its own accord.
///
/// It is what stops one outliving the service that started it: nobody
/// terminates it, it simply ends. Short enough that a service which
/// crashes leaves nothing running for long, long enough that starting
/// them again is a few times a minute and not a few times a second.
const HELPER_LIVES: Duration = Duration::from_secs(10);

/// How late is too late to count on the one that is running.
///
/// Another is started before the last has ended, so the reading never
/// stops between two of them.
const START_ANOTHER_AFTER: Duration = Duration::from_secs(7);

/// How long the service goes on keeping a helper after the last question.
const AFTER_THE_LAST_QUESTION: Duration = Duration::from_secs(2);

/// Whether the thread that keeps a helper alive is running.
static KEEPING: AtomicBool = AtomicBool::new(false);

/// When the last question came, so the keeper knows when to stop.
static ASKED: Mutex<Option<Instant>> = Mutex::new(None);

/// The shape the pointer has right now.
///
/// Never blocks and never fails: what comes back is the last word the
/// helper wrote, which is at most one reading old, and the ordinary
/// arrow before the first one, which is corrected within the frame that
/// follows. A pointer that arrives right an instant late is worth far
/// more than an answer that holds up the channel it travels on.
pub fn shape(log: &Log) -> Pointer {
    let log = &log.about(TAG);
    *ASKED.lock().expect("last question") = Some(Instant::now());
    if !KEEPING.swap(true, Ordering::SeqCst) {
        keep_a_helper(log.clone());
    }
    written_shape()
}

/// What the helper last wrote, or the ordinary arrow.
fn written_shape() -> Pointer {
    std::fs::read_to_string(zyr_proto::paths::pointer_here())
        .ok()
        .and_then(|word| word.trim().parse().ok())
        .unwrap_or_default()
}

/// Whether the last question is far enough behind to stop.
fn nobody_is_asking() -> bool {
    ASKED
        .lock()
        .expect("last question")
        .is_none_or(|asked| asked.elapsed() > AFTER_THE_LAST_QUESTION)
}

/// Keeps a helper reading in the session that owns the screen, for as
/// long as anybody is asking.
///
/// A thread of its own because starting a program in another session
/// takes milliseconds, and the threads that answer the far computer are
/// shared with everything else this service does.
#[cfg(windows)]
fn keep_a_helper(log: Log) {
    let log = log.about(TAG);
    std::thread::spawn(move || {
        log.write("a session is asking what shape this computer's pointer has");
        let mut started: Option<Instant> = None;
        let mut refused = false;
        while !nobody_is_asking() {
            if started.is_none_or(|at| at.elapsed() > START_ANOTHER_AFTER) {
                match crate::errands::start_reading_the_pointer() {
                    Ok(()) => {
                        if started.is_none() {
                            log.write("reading it from the session that owns the screen");
                        }
                        refused = false;
                        started = Some(Instant::now());
                    }
                    // Said once and not every second: a machine at its
                    // sign-in screen has no session to read from, and
                    // that is a state it can sit in for hours.
                    Err(e) => {
                        if !refused {
                            refused = true;
                            log.write(&format!("nothing can read the pointer here: {e}"));
                        }
                        started = None;
                    }
                }
            }
            std::thread::sleep(READ_EVERY);
        }
        log.write(&format!(
            "nobody is asking any more, the last shape read was {}",
            written_shape()
        ));
        // The word goes with the asking: the next session starts on the
        // ordinary pointer rather than on whatever shape this one was
        // left under.
        let _ = std::fs::remove_file(zyr_proto::paths::pointer_here());
        KEEPING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
fn keep_a_helper(_log: Log) {
    KEEPING.store(false, Ordering::SeqCst);
}

/// Reads the pointer of the desktop this program is standing on, and
/// writes it down, until its time is up.
///
/// This is the helper, and it only ever runs in the session that owns the
/// screen: started anywhere else it reads a desktop with no pointer on
/// it. It ends by itself so that nothing has to end it.
#[cfg(windows)]
pub fn follow_the_pointer_here() {
    let until = Instant::now() + HELPER_LIVES;
    let mut written = None;
    while Instant::now() < until {
        let shape = zyr_system::pointer_shape();
        if written != Some(shape) {
            // Replaced whole and never written in place: the service
            // reads between two writes, and a word caught half written
            // would be a shape nobody named.
            let path = zyr_proto::paths::pointer_here();
            if zyr_proto::files::replace(&path, &format!("{shape}\n")).is_ok() {
                written = Some(shape);
            }
        }
        std::thread::sleep(READ_EVERY);
    }
}

#[cfg(not(windows))]
pub fn follow_the_pointer_here() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nobody_asks_as_long_as_nobody_has_asked() {
        // That is what decides that a computer nobody is watching
        // reads nothing at all: with no question, no helper is started
        // again and the last one goes out by itself.
        *ASKED.lock().unwrap() = None;
        assert!(nobody_is_asking());
        *ASKED.lock().unwrap() = Some(Instant::now());
        assert!(!nobody_is_asking());
        *ASKED.lock().unwrap() = Instant::now().checked_sub(AFTER_THE_LAST_QUESTION * 2);
        assert!(nobody_is_asking());
    }

    #[test]
    fn a_helper_is_restarted_before_the_previous_one_dies() {
        // Without this overlap, the reading would stop between two
        // helpers and the pointer would freeze on its last shape while
        // another one starts.
        assert!(
            START_ANOTHER_AFTER < HELPER_LIVES,
            "a helper must be started again before the previous one ends"
        );
    }

    #[test]
    fn a_missing_word_is_the_ordinary_arrow() {
        // The service reads this file before any helper has had the time
        // to write: that moment must be an arrow and not a refusal, or
        // else the first session would have no pointer.
        let _ = std::fs::remove_file(zyr_proto::paths::pointer_here());
        assert_eq!(written_shape(), Pointer::Arrow);
    }
}
