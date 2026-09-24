//! When the host sends a picture.
//!
//! The screen gives an image whenever something changed on it, at its
//! own rate, and a session wants at most `fps` pictures a second. A
//! captured image goes at once if at least 7/8 of a period passed since
//! the last captured image went: a screen refreshing at the rate asked
//! (60 Hz for 60 fps), or slightly faster, never waits, its jitter never
//! costs a frame of delay, and no delay builds up. Sooner than that, the
//! image is held (a newer capture replacing it) and goes when its time
//! comes, which keeps a faster screen below 8/7 of the rate asked.
//!
//! With `steady`, a screen that stays still is sent again at the full
//! rate, for a player that paces its display on arrivals: the first
//! repeat a period and an eighth after the last picture, then one every
//! period on that grid. Repeats never hold a capture back: the 7/8 rule
//! counts captured images only.
//!
//! Every decision takes the time as an argument, so that the cadence can
//! be tested against a simulated clock.

use std::time::{Duration, Instant};

/// What to do with an image just captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Now {
    Emit,
    /// Keep it (the newest replacing it) until [`Due::EmitHeld`].
    Hold,
}

/// What is due with no new capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// The image held is to go now.
    EmitHeld,
    /// The screen is still: the last picture is to be sent again.
    Repeat,
}

#[derive(Debug, Clone)]
pub struct Cadence {
    period: Duration,
    steady: bool,
    /// When the last captured image went.
    captured_sent: Option<Instant>,
    /// When the last picture went, captured or repeated.
    last_sent: Option<Instant>,
    holding: bool,
    next_repeat: Option<Instant>,
}

impl Cadence {
    /// A cadence of `fps` pictures a second, at least one.
    pub fn new(fps: u32, steady: bool) -> Self {
        Self {
            period: period_of(fps),
            steady,
            captured_sent: None,
            last_sent: None,
            holding: false,
            next_repeat: None,
        }
    }

    pub fn set_fps(&mut self, fps: u32) {
        self.period = period_of(fps);
        self.restart_repeats();
    }

    pub fn set_steady(&mut self, steady: bool) {
        self.steady = steady;
        self.restart_repeats();
    }

    pub fn on_captured(&mut self, now: Instant) -> Now {
        match self.held_until() {
            Some(until) if now < until => {
                self.holding = true;
                Now::Hold
            }
            _ => {
                self.captured_went(now);
                Now::Emit
            }
        }
    }

    pub fn due(&mut self, now: Instant) -> Option<Due> {
        if self.holding {
            let until = self.held_until()?;
            if now < until {
                return None;
            }
            self.captured_went(now);
            return Some(Due::EmitHeld);
        }
        let at = self.next_repeat.filter(|at| self.steady && now >= *at)?;
        self.last_sent = Some(now);
        // The next point of the grid after now: a loop woken late skips
        // the points it missed rather than sending them in a burst.
        let missed = (now - at).as_nanos() / self.period.as_nanos();
        self.next_repeat = u32::try_from(missed + 1)
            .ok()
            .and_then(|periods| self.period.checked_mul(periods))
            .and_then(|ahead| at.checked_add(ahead));
        Some(Due::Repeat)
    }

    /// Until when the capture loop may wait for a new image, if it must
    /// wake without one.
    pub fn next_wakeup(&self) -> Option<Instant> {
        if self.holding {
            self.held_until()
        } else if self.steady {
            self.next_repeat
        } else {
            None
        }
    }

    /// When a captured image may go, after the last one that went.
    fn held_until(&self) -> Option<Instant> {
        self.captured_sent?.checked_add(self.period * 7 / 8)
    }

    fn captured_went(&mut self, at: Instant) {
        self.captured_sent = Some(at);
        self.last_sent = Some(at);
        self.holding = false;
        self.restart_repeats();
    }

    fn restart_repeats(&mut self) {
        self.next_repeat = self
            .last_sent
            .and_then(|at| at.checked_add(self.period + self.period / 8));
    }
}

fn period_of(fps: u32) -> Duration {
    Duration::from_secs(1) / fps.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Noise;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Sent {
        Captured,
        Held,
        Repeat,
    }

    fn us(n: u64) -> Duration {
        Duration::from_micros(n)
    }

    /// Runs the host's capture loop: it waits for the next image, at most
    /// until the cadence wants to wake, and says what went when.
    fn run(cadence: &mut Cadence, captures: &[Duration], until: Duration) -> Vec<(Duration, Sent)> {
        let start = Instant::now();
        let mut captures = captures.iter().map(|at| start + *at).peekable();
        let mut sent = Vec::new();
        loop {
            let capture = captures.peek().copied();
            let at = match (capture, cadence.next_wakeup()) {
                (Some(capture), Some(wake)) => capture.min(wake),
                (Some(at), None) | (None, Some(at)) => at,
                (None, None) => break,
            };
            if at > start + until {
                break;
            }
            if capture == Some(at) {
                captures.next();
                if cadence.on_captured(at) == Now::Emit {
                    sent.push((at - start, Sent::Captured));
                }
            } else {
                let what = match cadence.due(at) {
                    Some(Due::EmitHeld) => Sent::Held,
                    Some(Due::Repeat) => Sent::Repeat,
                    None => panic!("woken at {:?} for nothing", at - start),
                };
                sent.push((at - start, what));
            }
        }
        sent
    }

    /// Captures every `interval`, for `seconds`, each moved by up to
    /// `jitter` either way.
    fn source(
        interval: Duration,
        seconds: u64,
        jitter: Duration,
        noise: &mut Noise,
    ) -> Vec<Duration> {
        let count = Duration::from_secs(seconds).as_nanos() / interval.as_nanos();
        (1..count as u32)
            .map(|n| {
                let shift = noise.below(2 * jitter.as_micros() as usize + 1) as u64;
                interval * n + us(shift) - jitter
            })
            .collect()
    }

    #[test]
    fn a_screen_at_the_rate_asked_is_sent_at_once_despite_its_jitter() {
        let mut noise = Noise::new(50);
        let captures = source(Duration::from_secs(1) / 60, 10, us(1_000), &mut noise);
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, Duration::from_secs(11));
        assert_eq!(sent.len(), captures.len());
        for ((at, what), captured) in sent.iter().zip(&captures) {
            assert_eq!((*at, *what), (*captured, Sent::Captured));
        }
    }

    #[test]
    fn a_screen_slightly_faster_never_drifts() {
        let interval = Duration::from_secs_f64(1.0 / 60.1);
        let captures = source(interval, 10, Duration::ZERO, &mut Noise::new(51));
        let mut cadence = Cadence::new(60, true);
        let sent = run(&mut cadence, &captures, Duration::from_secs(11));
        let captured: Vec<_> = sent
            .iter()
            .filter(|(_, what)| *what != Sent::Repeat)
            .collect();
        assert_eq!(captured.len(), captures.len());
        for ((at, what), capture) in captured.iter().zip(&captures) {
            assert_eq!((*at, *what), (*capture, Sent::Captured));
        }
    }

    #[test]
    fn a_faster_screen_stays_below_eight_sevenths_of_the_rate() {
        let interval = Duration::from_secs(1) / 144;
        let captures = source(interval, 10, Duration::ZERO, &mut Noise::new(52));
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, Duration::from_secs(10));
        let period = Duration::from_secs(1) / 60;
        for pair in sent.windows(2) {
            assert!(pair[1].0 - pair[0].0 >= period * 7 / 8, "{pair:?}");
        }
        let per_second = sent.len() as f64 / 10.0;
        assert!(per_second <= 60.0 * 8.0 / 7.0 + 0.1, "{per_second}");
        assert!(per_second >= 60.0, "{per_second}");
        assert!(sent.iter().any(|(_, what)| *what == Sent::Held));
    }

    #[test]
    fn a_still_screen_is_repeated_exactly_at_the_rate_when_steady() {
        let mut cadence = Cadence::new(50, true);
        let sent = run(&mut cadence, &[Duration::ZERO], Duration::from_secs(10));
        let period = us(20_000);
        assert_eq!(sent[0], (Duration::ZERO, Sent::Captured));
        assert_eq!(sent[1], (period + period / 8, Sent::Repeat));
        for pair in sent[1..].windows(2) {
            assert_eq!(pair[1].0 - pair[0].0, period);
            assert_eq!(pair[1].1, Sent::Repeat);
        }
        for second in 1..9 {
            let within = sent.iter().filter(|(at, _)| at.as_secs() == second).count();
            assert_eq!(within, 50, "second {second}");
        }
    }

    #[test]
    fn a_still_screen_is_not_repeated_otherwise() {
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &[Duration::ZERO], Duration::from_secs(5));
        assert_eq!(sent, vec![(Duration::ZERO, Sent::Captured)]);
        assert_eq!(cadence.next_wakeup(), None);
    }

    #[test]
    fn a_capture_right_after_a_repeat_goes_at_once() {
        let period = Duration::from_secs(1) / 60;
        let repeat = period + period / 8;
        let captures = [Duration::ZERO, repeat + us(500)];
        let mut cadence = Cadence::new(60, true);
        let sent = run(&mut cadence, &captures, repeat * 2);
        assert_eq!(
            &sent[..3],
            &[
                (Duration::ZERO, Sent::Captured),
                (repeat, Sent::Repeat),
                (repeat + us(500), Sent::Captured),
            ]
        );
        // The grid starts again from that capture: on the old one, a
        // repeat would have gone a period after the first.
        assert_eq!(sent.len(), 3);
    }

    #[test]
    fn a_burst_sends_its_first_image_then_its_last() {
        let period = Duration::from_secs(1) / 60;
        let captures = [us(0), us(1_000), us(2_000), us(3_000), us(40_000)];
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, Duration::from_secs(1));
        assert_eq!(
            sent,
            vec![
                (us(0), Sent::Captured),
                (period * 7 / 8, Sent::Held),
                (us(40_000), Sent::Captured),
            ]
        );
    }

    #[test]
    fn a_held_image_waits_for_its_time() {
        let at = Instant::now();
        let mut cadence = Cadence::new(60, false);
        assert_eq!(cadence.on_captured(at), Now::Emit);
        assert_eq!(cadence.on_captured(at + us(5_000)), Now::Hold);
        let due = at + Duration::from_secs(1) / 60 * 7 / 8;
        assert_eq!(cadence.next_wakeup(), Some(due));
        assert_eq!(cadence.due(due - us(1)), None);
        assert_eq!(cadence.due(due), Some(Due::EmitHeld));
        assert_eq!(cadence.due(due + us(1)), None);
    }

    #[test]
    fn a_loop_woken_late_does_not_burst_repeats() {
        let at = Instant::now();
        let period = us(20_000);
        let mut cadence = Cadence::new(50, true);
        cadence.on_captured(at);
        let late = at + period * 10;
        assert_eq!(cadence.due(late), Some(Due::Repeat));
        assert_eq!(cadence.due(late), None);
        assert_eq!(
            cadence.next_wakeup(),
            Some(at + period + period / 8 + period * 9)
        );
    }

    #[test]
    fn the_rate_and_the_steadiness_change_on_the_fly() {
        let at = Instant::now();
        let mut cadence = Cadence::new(60, false);
        cadence.on_captured(at);
        cadence.set_fps(30);
        let period = Duration::from_secs(1) / 30;
        assert_eq!(cadence.on_captured(at + us(20_000)), Now::Hold);
        assert_eq!(cadence.next_wakeup(), Some(at + period * 7 / 8));
        assert_eq!(cadence.due(at + period * 7 / 8), Some(Due::EmitHeld));

        let sent = at + period * 7 / 8;
        cadence.set_steady(true);
        assert_eq!(cadence.next_wakeup(), Some(sent + period + period / 8));
        cadence.set_steady(false);
        assert_eq!(cadence.next_wakeup(), None);
        assert_eq!(cadence.due(sent + period * 5), None);
    }

    #[test]
    fn nothing_is_repeated_before_a_first_picture() {
        let mut cadence = Cadence::new(0, true);
        assert_eq!(cadence.next_wakeup(), None);
        assert_eq!(cadence.due(Instant::now()), None);
        assert_eq!(cadence.period, Duration::from_secs(1));
    }
}
