//! What a stream went through, a second at a time, for the journal.
//!
//! The measures a session shows are means over the last second, which is
//! what a person watching wants and exactly what hides an uneven stream:
//! sixty frames in a second, a third of them a frame late, still reads as
//! sixty frames a second. The journal gets each second whole instead: the
//! spread of every step, and every frame one after the other, numbered as
//! both computers number it, so that a second on the host and the same
//! second on the player can be laid side by side frame for frame.

use std::fmt;
use std::time::{Duration, Instant};

/// How often a second is written out.
pub const EVERY: Duration = Duration::from_secs(1);

/// A duration in milliseconds.
pub fn ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

/// The values one step took over a second: written as its median, its
/// 95th percentile and its worst, which is where an uneven stream shows.
#[derive(Debug, Clone, Default)]
pub struct Spread {
    values: Vec<f32>,
}

impl Spread {
    pub fn add(&mut self, took: Duration) {
        self.values.push(ms(took));
    }

    /// A value that is not a duration, a size for instance.
    pub fn add_value(&mut self, value: f32) {
        self.values.push(value);
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Median, 95th percentile and worst (nearest rank).
    pub fn quantiles(&self) -> Option<(f32, f32, f32)> {
        let mut sorted = self.values.clone();
        sorted.sort_by(f32::total_cmp);
        let at = |fraction: f32| {
            let rank = (fraction * sorted.len() as f32).ceil() as usize;
            sorted[rank.clamp(1, sorted.len()) - 1]
        };
        let worst = *sorted.last()?;
        Some((at(0.5), at(0.95), worst))
    }
}

/// `median/95th/worst`, to the tenth, or a dash when nothing was added.
impl fmt::Display for Spread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.quantiles() {
            Some((median, p95, worst)) => write!(f, "{median:.1}/{p95:.1}/{worst:.1}"),
            None => f.write_str("-"),
        }
    }
}

/// When the next second is to be written.
///
/// Counted from the first thing that happened rather than from a clock
/// of its own: nothing to write, nothing to count from, and a stream
/// that stops for a minute does not come back to sixty empty seconds.
#[derive(Debug, Clone, Copy, Default)]
pub struct Seconds {
    next: Option<Instant>,
}

impl Seconds {
    /// Something happened at `now`: the first thing of a second starts
    /// it.
    pub fn started(&mut self, now: Instant) {
        if self.next.is_none() {
            self.next = now.checked_add(EVERY);
        }
    }

    /// Whether the second begun is over at `now`. Once it says so, the
    /// next second starts with the next thing that happens.
    pub fn over(&mut self, now: Instant) -> bool {
        match self.next {
            Some(next) if now >= next => {
                self.next = None;
                true
            }
            _ => false,
        }
    }

    /// Whether a second is under way.
    pub fn under_way(&self) -> bool {
        self.next.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spread_says_its_median_its_95th_and_its_worst() {
        let mut spread = Spread::default();
        assert_eq!(spread.to_string(), "-");
        for n in 1..=100 {
            spread.add(Duration::from_micros(n * 100));
        }
        let (median, p95, worst) = spread.quantiles().unwrap();
        assert!((median - 5.0).abs() < 1e-3, "{median}");
        assert!((p95 - 9.5).abs() < 1e-3, "{p95}");
        assert!((worst - 10.0).abs() < 1e-3, "{worst}");
        assert_eq!(spread.to_string(), "5.0/9.5/10.0");

        spread.clear();
        spread.add_value(42.0);
        assert_eq!(spread.to_string(), "42.0/42.0/42.0");
    }

    #[test]
    fn a_second_starts_with_what_happens_and_not_before() {
        let at = Instant::now();
        let mut seconds = Seconds::default();
        assert!(!seconds.over(at + EVERY * 5));
        assert!(!seconds.under_way());
        seconds.started(at);
        seconds.started(at + EVERY / 2);
        assert!(!seconds.over(at + EVERY / 2));
        assert!(seconds.over(at + EVERY));
        // Said once, and nothing counted until something happens again.
        assert!(!seconds.over(at + EVERY * 3));
        seconds.started(at + EVERY * 3);
        assert!(seconds.over(at + EVERY * 4));
    }
}
