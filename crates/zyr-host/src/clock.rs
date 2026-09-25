//! The engine's clock, as the player sees it.
//!
//! Pictures and sound carry when they were captured, and pongs when they
//! were answered, all in microseconds on one clock: from the engine's
//! start, on the monotonic clock, so that it never goes back.

use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HostClock {
    start: Instant,
}

impl HostClock {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// `at` on this clock; the start for anything before it.
    pub(crate) fn micros(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.start).as_micros()).unwrap_or(u64::MAX)
    }

    pub(crate) fn now(&self) -> u64 {
        self.micros(Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn times_are_counted_from_the_start_and_never_before_it() {
        let clock = HostClock::new();
        let later = clock.start + Duration::from_millis(1500);
        assert_eq!(clock.micros(later), 1_500_000);
        let before = clock.start.checked_sub(Duration::from_secs(1));
        if let Some(before) = before {
            assert_eq!(clock.micros(before), 0);
        }
    }
}
