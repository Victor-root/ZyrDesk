//! What to do when the host engine stops.
//!
//! A service that blindly restarts an engine dying at every start spins
//! in a loop and hides the fault: instead of a clear message, the user
//! sees a computer that never gets there. A service that restarts
//! nothing, on the other hand, abandons a machine a passing incident
//! would have been enough to put back on its feet.
//!
//! The rule tells three cases apart. An engine stopping because Windows
//! is shutting down is not an incident. An engine falling after holding
//! for a long time deserves an immediate restart. An engine falling the
//! moment it starts, several times over, will not be saved by one more
//! attempt.
//!
//! This module knows neither Windows nor processes: it decides, from
//! what it is told.

use std::time::Duration;

/// Code the engine returns when it stops because Windows shuts down.
///
/// It is the system's own, `ERROR_SHUTDOWN_IN_PROGRESS`. The upstream
/// engine uses it to tell its own end from an incident, and the upstream
/// service leans on it to avoid restarting during a shutdown.
pub const WINDOWS_SHUTDOWN: i32 = 1115;

/// Past this, the engine counts as having held: the failure counter
/// goes back to zero.
const HEALTHY_LIFE: Duration = Duration::from_secs(60);

/// Delay before the first restart after a failure.
const INITIAL_DELAY: Duration = Duration::from_secs(2);

/// Ceiling on the restart delay. Beyond it, the wait would be more
/// painful than the fault.
const MAXIMUM_DELAY: Duration = Duration::from_secs(60);

/// Closely spaced failures allowed before giving up.
const MAX_FAILURES: u32 = 5;

/// What ought to happen after the engine stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Restart after this delay.
    Restart(Duration),
    /// Give up and say so: the engine will not stand.
    GiveUp,
    /// Do nothing: the stop was wanted.
    Finish,
}

/// Decides what comes next, remembering recent failures.
#[derive(Debug, Default)]
pub struct Policy {
    failures: u32,
}

impl Policy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of closely spaced failures piled up.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Decides what comes next after the engine stops.
    ///
    /// `code` is the exit code, absent when the engine was interrupted.
    /// `lifetime` is how long it held.
    pub fn after_stop(&mut self, code: Option<i32>, lifetime: Duration) -> Next {
        if code == Some(WINDOWS_SHUTDOWN) {
            return Next::Finish;
        }

        // An engine that held its time is not at fault: whatever just
        // felled it is isolated, and the slate is wiped.
        if lifetime >= HEALTHY_LIFE {
            self.failures = 0;
            return Next::Restart(Duration::ZERO);
        }

        self.failures += 1;
        if self.failures > MAX_FAILURES {
            return Next::GiveUp;
        }
        Next::Restart(delay(self.failures))
    }
}

/// Delay before restart number `failure`, doubling each time.
fn delay(failure: u32) -> Duration {
    let factor = 1u32 << (failure.saturating_sub(1)).min(16);
    INITIAL_DELAY.saturating_mul(factor).min(MAXIMUM_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(count: u64) -> Duration {
        Duration::from_secs(count)
    }

    #[test]
    fn a_windows_shutdown_is_not_an_incident() {
        let mut policy = Policy::new();
        // Even fallen at once: it is the machine going down.
        assert_eq!(
            policy.after_stop(Some(WINDOWS_SHUTDOWN), Duration::ZERO),
            Next::Finish
        );
        assert_eq!(policy.failures(), 0);
    }

    #[test]
    fn an_engine_that_held_starts_again_at_once() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.after_stop(Some(1), HEALTHY_LIFE),
            Next::Restart(Duration::ZERO)
        );
    }

    #[test]
    fn closely_spaced_failures_space_the_restarts_out() {
        let mut policy = Policy::new();
        let mut previous = Duration::ZERO;
        for _ in 0..MAX_FAILURES {
            let Next::Restart(delay) = policy.after_stop(Some(1), seconds(1)) else {
                panic!("the engine should still have its chance");
            };
            assert!(
                delay >= previous,
                "{delay:?} after {previous:?}: the wait has to grow"
            );
            assert!(delay <= MAXIMUM_DELAY);
            previous = delay;
        }
        assert!(previous > INITIAL_DELAY, "the wait never grew at all");
    }

    #[test]
    fn an_engine_that_never_holds_is_eventually_left_alone() {
        let mut policy = Policy::new();
        for _ in 0..MAX_FAILURES {
            assert!(matches!(
                policy.after_stop(Some(1), seconds(1)),
                Next::Restart(_)
            ));
        }
        assert_eq!(policy.after_stop(Some(1), seconds(1)), Next::GiveUp);
    }

    #[test]
    fn one_success_wipes_the_earlier_failures() {
        let mut policy = Policy::new();
        for _ in 0..MAX_FAILURES {
            policy.after_stop(Some(1), seconds(1));
        }
        assert_eq!(policy.failures(), MAX_FAILURES);

        // The engine starts again, holds its time, then falls once more:
        // it has to get all of its chances back, or a machine left on for
        // weeks would end up restarting nothing.
        policy.after_stop(Some(1), HEALTHY_LIFE);
        assert_eq!(policy.failures(), 0);
        assert!(matches!(
            policy.after_stop(Some(1), seconds(1)),
            Next::Restart(_)
        ));
    }

    #[test]
    fn an_engine_interrupted_without_a_code_counts_as_a_failure() {
        let mut policy = Policy::new();
        assert!(matches!(
            policy.after_stop(None, seconds(1)),
            Next::Restart(_)
        ));
        assert_eq!(policy.failures(), 1);
    }

    #[test]
    fn the_delay_never_overflows() {
        // The shift that doubles the delay must not run wild on an
        // absurd value.
        for failure in [0u32, 1, 5, 100, u32::MAX] {
            assert!(delay(failure) <= MAXIMUM_DELAY, "failure {failure}");
        }
    }
}
