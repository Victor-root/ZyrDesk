//! Keeping a helper running in the session that owns the screen, for as
//! long as somebody is asking.
//!
//! Some of what the far computer asks can only be read from the session
//! on screen: the shape of the pointer, the clipboard. So a helper is
//! started there, it reads for a short life and ends by itself, and
//! another is started before the last has ended, so that the reading
//! never stops between two of them. Nothing has to end a helper: a
//! service that stops asking, or that stops altogether, leaves nothing
//! behind for more than a few seconds, and a computer nobody is watching
//! reads nothing at all.
//!
//! That keeping is the same for every helper, and it is written once,
//! here. What each one reads, what it is kept for beyond the questions
//! and what is tidied away after it are its own.

use std::io;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::log::Log;

/// How a helper is kept.
pub struct Timing {
    /// How often the keeper looks at whether one is still wanted.
    pub look_every: Duration,
    /// How late is too late to count on the one that is running.
    ///
    /// Shorter than the life of a helper: another is started before the
    /// last has ended, so the reading never stops between two of them.
    pub start_another_after: Duration,
    /// How long helpers go on being kept after the last question.
    pub after_the_last_question: Duration,
}

/// The keeping of one kind of helper.
///
/// One for the whole service and one per kind, since there is one pointer
/// and one clipboard on this computer however many sessions ask about
/// them.
pub struct Keeper {
    /// Whether the thread that keeps a helper alive is running.
    keeping: AtomicBool,
    /// When the last question came, so the keeper knows when to stop.
    asked: Mutex<Option<Instant>>,
    timing: Timing,
}

impl Keeper {
    pub const fn new(timing: Timing) -> Self {
        Self {
            keeping: AtomicBool::new(false),
            asked: Mutex::new(None),
            timing,
        }
    }

    /// Notes a question, and says whether nobody is keeping a helper yet,
    /// in which case the one asking is the one to start keeping one.
    pub fn asked(&self) -> bool {
        *self.asked.lock().expect("last question") = Some(Instant::now());
        !self.keeping.swap(true, Ordering::SeqCst)
    }

    /// Whether the last question is far enough behind to stop.
    pub fn nobody_is_asking(&self) -> bool {
        self.asked
            .lock()
            .expect("last question")
            .is_none_or(|asked| asked.elapsed() > self.timing.after_the_last_question)
    }

    /// Starts helpers with `start` for as long as somebody asks or
    /// `holds_on` says so, and never while `may_start` says not. Blocks
    /// the calling thread, which is one of its own: starting a program in
    /// another session takes milliseconds, and the threads that answer
    /// the far computer are shared with everything else the service does.
    ///
    /// What a helper is for is `what`, as in « nothing can `what` here »,
    /// which is said once and not at every look: a machine at its sign-in
    /// screen has no session to read from, and that is a state it can sit
    /// in for hours.
    pub fn keep(
        &self,
        start: impl Fn() -> io::Result<()>,
        may_start: impl Fn() -> bool,
        holds_on: impl Fn() -> bool,
        what: &str,
        log: &Log,
    ) {
        let mut started: Option<Instant> = None;
        let mut refused = false;
        while !self.nobody_is_asking() || holds_on() {
            if may_start()
                && started.is_none_or(|at| at.elapsed() > self.timing.start_another_after)
            {
                match start() {
                    Ok(()) => {
                        if started.is_none() {
                            log.write("reading it from the session that owns the screen");
                        }
                        refused = false;
                        started = Some(Instant::now());
                    }
                    Err(e) => {
                        if !refused {
                            refused = true;
                            log.write(&format!("nothing can {what} here: {e}"));
                        }
                        started = None;
                    }
                }
            }
            std::thread::sleep(self.timing.look_every);
        }
    }

    /// Says the keeping is over, so that the next question starts it
    /// again.
    pub fn over(&self) {
        self.keeping.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keeper() -> Keeper {
        Keeper::new(Timing {
            look_every: Duration::from_millis(1),
            start_another_after: Duration::from_millis(7),
            after_the_last_question: Duration::from_millis(40),
        })
    }

    #[test]
    fn nobody_asks_as_long_as_nobody_has_asked() {
        // That is what decides that a computer nobody is watching reads
        // nothing at all: with no question, no helper is started again
        // and the last one goes out by itself.
        let keeper = keeper();
        assert!(keeper.nobody_is_asking());
        assert!(keeper.asked(), "the first question starts the keeping");
        assert!(!keeper.nobody_is_asking());
        assert!(!keeper.asked(), "a second one finds it started");
        std::thread::sleep(Duration::from_millis(80));
        assert!(keeper.nobody_is_asking());
        keeper.over();
        assert!(keeper.asked(), "over, the next question starts it again");
    }

    #[test]
    fn helpers_are_started_while_asked_and_never_when_they_may_not_be() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-keeper-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let log = Log::open(&folder.join("service.log")).unwrap();
        let keeper = keeper();
        keeper.asked();
        let started = std::sync::atomic::AtomicU32::new(0);
        keeper.keep(
            || {
                started.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || true,
            || false,
            "read anything",
            &log,
        );
        // Kept for the length of the patience after the question, and
        // started again before the last would have gone.
        assert!(started.load(Ordering::SeqCst) >= 2);

        keeper.asked();
        keeper.keep(
            || panic!("a helper was started where none may be"),
            || false,
            || false,
            "read anything",
            &log,
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
