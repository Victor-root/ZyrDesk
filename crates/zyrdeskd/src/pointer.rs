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

// Outside Windows the service does not run, and nothing asks for the
// shape. It stays compiled and tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use zyr_proto::log::Log;
use zyr_proto::session::Pointer;

use crate::keeper::{Keeper, Timing};

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

/// The keeping of the helpers that read it.
static KEEPER: Keeper = Keeper::new(Timing {
    look_every: READ_EVERY,
    start_another_after: START_ANOTHER_AFTER,
    after_the_last_question: AFTER_THE_LAST_QUESTION,
});

/// The shape the pointer has right now.
///
/// Never blocks and never fails: what comes back is the last word the
/// helper wrote, which is at most one reading old, and the ordinary
/// arrow before the first one, which is corrected within the frame that
/// follows. A pointer that arrives right an instant late is worth far
/// more than an answer that holds up the channel it travels on.
pub fn shape(log: &Log) -> Pointer {
    let log = &log.about(TAG);
    if KEEPER.asked() {
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

/// Keeps a helper reading in the session that owns the screen, for as
/// long as anybody is asking.
fn keep_a_helper(log: Log) {
    std::thread::spawn(move || {
        log.write("a session is asking what shape this computer's pointer has");
        KEEPER.keep(
            crate::errands::start_reading_the_pointer,
            || true,
            || false,
            "read the pointer",
            &log,
        );
        log.write(&format!(
            "nobody is asking any more, the last shape read was {}",
            written_shape()
        ));
        // The word goes with the asking: the next session starts on the
        // ordinary pointer rather than on whatever shape this one was
        // left under.
        let _ = std::fs::remove_file(zyr_proto::paths::pointer_here());
        KEEPER.over();
    });
}

/// Reads the pointer of the desktop this program is standing on, and
/// writes it down, until its time is up.
///
/// This is the helper, and it only ever runs in the session that owns the
/// screen: started anywhere else it reads a desktop with no pointer on
/// it. It ends by itself so that nothing has to end it.
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

#[cfg(test)]
mod tests {
    use super::*;

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
