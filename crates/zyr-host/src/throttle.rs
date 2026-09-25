//! Saying something that repeats, without saying it at every turn.
//!
//! A drop or a failure is always counted, and always said, but a place
//! that fails sixty times a second would fill the log: the first time is
//! said at once, and after that at most once per interval, with how many
//! times it happened in between.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct Throttle {
    every: Duration,
    next: Option<Instant>,
    unsaid: u64,
}

impl Throttle {
    pub(crate) fn new(every: Duration) -> Self {
        Self {
            every,
            next: None,
            unsaid: 0,
        }
    }

    /// Whether to say it now, and if so how many times it happened
    /// unsaid since the last time it was said.
    pub(crate) fn allow(&mut self, now: Instant) -> Option<u64> {
        if self.next.is_some_and(|next| now < next) {
            self.unsaid += 1;
            return None;
        }
        self.next = now.checked_add(self.every);
        Some(std::mem::take(&mut self.unsaid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_time_is_said_then_once_per_interval_with_the_count() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut throttle = Throttle::new(10 * second);
        assert_eq!(throttle.allow(start), Some(0));
        for n in 1..=5 {
            assert_eq!(throttle.allow(start + n * second), None);
        }
        assert_eq!(throttle.allow(start + 10 * second), Some(5));
        assert_eq!(throttle.allow(start + 11 * second), None);
        assert_eq!(throttle.allow(start + 30 * second), Some(1));
    }
}
