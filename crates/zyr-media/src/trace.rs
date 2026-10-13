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
use std::sync::mpsc;
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

/// How many waits are timed to tell how late a wait wakes.
const WAKES_TIMED: u32 = 12;

/// How late a timed wait wakes in this process, told for the journal.
///
/// A few waits of `wait` on a channel nothing is ever sent on, which is
/// how the threads that wait for a picture or a command are woken. The
/// system's timers decide how late, and only a wait shows what this
/// process really gets from them, whatever it asked for.
pub fn late_wakes(wait: Duration) -> String {
    let (_kept, nothing) = mpsc::channel::<()>();
    let mut late = Spread::default();
    for _ in 0..WAKES_TIMED {
        let began = Instant::now();
        let _ = nothing.recv_timeout(wait);
        late.add(began.elapsed().saturating_sub(wait));
    }
    format!(
        "{WAKES_TIMED} waits of {} ms woke {late} ms late (median/95th/worst)",
        wait.as_millis()
    )
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
}

/// A step longer than this between two of a kind is a pause, not
/// unevenness: the hand stopped.
const PAUSE: Duration = Duration::from_millis(100);

/// A step longer than a refresh of a 60 Hz screen, and a little: a
/// refresh of the screen it lands on went by with nothing new.
const LONGER_THAN_A_REFRESH: Duration = Duration::from_millis(17);

/// How evenly something comes while it keeps coming, a second at a
/// time: the steps from one to the next, pauses left out, and how many
/// were longer than a refresh.
#[derive(Debug, Clone, Default)]
pub struct Steps {
    last: Option<Instant>,
    steps: Spread,
    long: u32,
    seconds: Seconds,
}

impl Steps {
    /// One came at `now`.
    pub fn came(&mut self, now: Instant) {
        self.seconds.started(now);
        if let Some(step) = self
            .last
            .replace(now)
            .map(|last| now.saturating_duration_since(last))
            .filter(|step| *step < PAUSE)
        {
            self.steps.add(step);
            if step > LONGER_THAN_A_REFRESH {
                self.long += 1;
            }
        }
    }

    /// The second gone, once it is over at `now`: `every M/P/W ms, N
    /// longer than a refresh`.
    pub fn second(&mut self, now: Instant) -> Option<String> {
        if !self.seconds.over(now) || self.steps.is_empty() {
            return None;
        }
        let line = format!(
            "every {} ms (median/95th/worst), {} longer than a refresh",
            self.steps, self.long
        );
        self.steps.clear();
        self.long = 0;
        Some(line)
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
    fn waits_are_told_late_by_as_much_as_the_timers_let_them_be() {
        let began = Instant::now();
        let told = late_wakes(Duration::from_millis(2));
        assert!(began.elapsed() >= Duration::from_millis(2 * u64::from(WAKES_TIMED)));
        assert!(told.starts_with("12 waits of 2 ms woke "), "{told}");
        assert!(told.ends_with(" ms late (median/95th/worst)"), "{told}");
    }

    #[test]
    fn a_second_starts_with_what_happens_and_not_before() {
        let at = Instant::now();
        let mut seconds = Seconds::default();
        assert!(!seconds.over(at + EVERY * 5));
        seconds.started(at);
        seconds.started(at + EVERY / 2);
        assert!(!seconds.over(at + EVERY / 2));
        assert!(seconds.over(at + EVERY));
        // Said once, and nothing counted until something happens again.
        assert!(!seconds.over(at + EVERY * 3));
        seconds.started(at + EVERY * 3);
        assert!(seconds.over(at + EVERY * 4));
    }

    #[test]
    fn steps_leave_pauses_out_and_count_those_longer_than_a_refresh() {
        let at = Instant::now();
        let mut steps = Steps::default();
        for ms_at in [0u64, 8, 16, 40, 48, 400, 408] {
            steps.came(at + Duration::from_millis(ms_at));
        }
        assert_eq!(steps.second(at + EVERY / 2), None);
        assert_eq!(
            steps.second(at + EVERY).as_deref(),
            Some("every 8.0/24.0/24.0 ms (median/95th/worst), 1 longer than a refresh")
        );
        assert_eq!(steps.second(at + EVERY * 3), None);
    }
}
