//! When the host sends a picture.
//!
//! The screen gives an image whenever something changed on it, at its
//! own rate, and a session wants at most `fps` pictures a second. A
//! captured image goes at once if it was filmed at least 7/8 of a period
//! after the last captured image that went: a screen refreshing at the
//! rate asked (60 Hz for 60 fps) never waits, its jitter never costs a
//! frame of delay, and no delay builds up. Sooner than that, the image is
//! held a whole period after that one, and a newer capture replaces it:
//! an image given twice, two milliseconds apart, is the one left out,
//! and the screen's next image, on time, takes its place.
//!
//! Counted from when the images were filmed, never from when they went:
//! a held image goes late, and counting from its going would hold the
//! next one, on time, by as much, and the one after by a little less,
//! for a handful of images.
//!
//! A screen crowded with images, one more than the rate asked over the
//! last [`CROWDED`] periods, is also held to the rate over the long run: each
//! image that goes books a period on a grid, and an image may run at most
//! a quarter of a period ahead of it. Bursts and screens much faster than
//! the rate are then sent at the rate and not beyond it: the player shows
//! one picture a refresh, and every picture beyond the rate waited behind
//! another there before being thrown away. A quarter of a period is the
//! jitter that does not count as crowding.
//!
//! Only a crowded screen: on a screen at the rate, a grid would remember
//! one late image and hold every image after it by its lateness, for as
//! long as the screen kept its pace. Each image of a screen that is not
//! crowded books its period from where it went, and the grid forgets.
//!
//! With `steady`, a screen that stays still is sent again at the full
//! rate, for a player that paces its display on arrivals: the first
//! repeat a period and an eighth after the last picture, then one every
//! period on that grid. Repeats never hold a capture back: the 7/8 rule
//! counts captured images only.
//!
//! Repeats and captures share one allowance of `fps` pictures a second,
//! a slot per period. A capture never waits for its slot: it goes at once
//! and takes the next one free, even if a repeat has just taken the one
//! before. A repeat only goes in a slot left free. A screen changing a
//! little less often than the rate asked, a 50 fps video in a 60 fps
//! session, is then not sent twice per change, repeated just before each
//! new image and again with it. The slots taken ahead never run more than
//! two periods past the last capture, so that a burst from a faster
//! screen does not silence the repeats once it stops.
//!
//! Without `steady`, a still screen sends nothing, and the player is
//! told instead, every [`STILL_EVERY`], that the picture it has is still
//! the screen: a player that hears nothing for a while takes the picture
//! for frozen, and the two must not be confused.
//!
//! Every decision takes the time as an argument, so that the cadence can
//! be tested against a simulated clock.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How often a still screen that is not sent again is said to be still:
/// well within the third of a second after which the player takes a
/// silent picture for frozen.
pub const STILL_EVERY: Duration = Duration::from_millis(100);

/// Over how many periods a screen is seen crowded, when it gave one
/// image more than these periods hold.
///
/// Long enough that a screen at the rate, a little faster than the clock
/// it is measured with or bunched by its own jitter, is never taken for
/// crowded: held to the grid, it would wait for it a period at a time.
/// Short enough that a burst, or a screen much faster than the rate, is
/// caught within a few of its images.
const CROWDED: u32 = 16;

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
    /// The screen is still and nothing is sent again: the player is to
    /// be told its picture is still the screen.
    Still,
}

#[derive(Debug, Clone)]
pub struct Cadence {
    period: Duration,
    steady: bool,
    /// When the last captured image went.
    captured_sent: Option<Instant>,
    /// When the last captured image that went was filmed.
    filmed: Option<Instant>,
    /// When the image held was filmed.
    held_filmed: Option<Instant>,
    /// When the next captured image is due on the grid of the rate.
    captured_due: Option<Instant>,
    /// When the last captures came, up to [`CROWDED`] and one of them
    /// and the latest.
    captures: VecDeque<Instant>,
    /// Whether the screen gave more images than the rate over the last
    /// [`CROWDED`] periods.
    crowded: bool,
    /// When the last picture went, captured or repeated.
    last_sent: Option<Instant>,
    holding: bool,
    /// The first slot of the allowance still free.
    next_slot: Option<Instant>,
    next_repeat: Option<Instant>,
    next_still: Option<Instant>,
}

impl Cadence {
    /// A cadence of `fps` pictures a second, at least one.
    pub fn new(fps: u32, steady: bool) -> Self {
        Self {
            period: period_of(fps),
            steady,
            captured_sent: None,
            filmed: None,
            held_filmed: None,
            captured_due: None,
            captures: VecDeque::new(),
            crowded: false,
            last_sent: None,
            holding: false,
            next_slot: None,
            next_repeat: None,
            next_still: None,
        }
    }

    pub fn set_fps(&mut self, fps: u32) {
        self.period = period_of(fps);
        // Counted in periods of the old rate.
        self.next_slot = None;
        self.captured_due = self
            .captured_sent
            .and_then(|at| at.checked_add(self.period));
        self.restart_idle();
    }

    pub fn set_steady(&mut self, steady: bool) {
        self.steady = steady;
        self.restart_idle();
    }

    pub fn on_captured(&mut self, now: Instant) -> Now {
        self.captures.push_back(now);
        if self.captures.len() > CROWDED as usize + 2 {
            self.captures.pop_front();
        }
        let window = self.period * CROWDED - self.period / 4;
        self.crowded = self.captures.len() > CROWDED as usize + 1
            && self
                .captures
                .front()
                .is_some_and(|first| now.saturating_duration_since(*first) < window);
        match self.may_go(self.period * 7 / 8) {
            Some(until) if now < until => {
                self.holding = true;
                self.held_filmed = Some(now);
                Now::Hold
            }
            _ => {
                self.captured_went(now, now);
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
            let filmed = self.held_filmed.unwrap_or(now);
            self.captured_went(now, filmed);
            return Some(Due::EmitHeld);
        }
        if !self.steady {
            self.next_still.filter(|at| now >= *at)?;
            // From now and not from the point missed: a loop woken late
            // says it once.
            self.next_still = now.checked_add(STILL_EVERY);
            return Some(Due::Still);
        }
        let at = self.next_repeat.filter(|at| now >= *at)?;
        self.last_sent = Some(now);
        // The next point of the grid after now: a loop woken late skips
        // the points it missed rather than sending them in a burst. What
        // is left of a period is below a second, so it fits in u64.
        let into = (now - at).as_nanos() % self.period.as_nanos();
        self.next_slot = now.checked_add(self.period - Duration::from_nanos(into as u64));
        self.next_repeat = self.next_slot;
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
            self.next_still
        }
    }

    /// When the image held goes: a period after the last one that went
    /// was filmed.
    fn held_until(&self) -> Option<Instant> {
        self.may_go(self.period)
    }

    /// When a captured image may go: `after` the last one that went was
    /// filmed, and on the grid when the screen is crowded.
    fn may_go(&self, after: Duration) -> Option<Instant> {
        let after_the_last = self.filmed?.checked_add(after)?;
        let on_the_grid = self
            .captured_due
            .filter(|_| self.crowded)
            .and_then(|due| due.checked_sub(self.period / 4));
        Some(on_the_grid.map_or(after_the_last, |grid| grid.max(after_the_last)))
    }

    /// A captured image went at `at`, filmed at `filmed`.
    fn captured_went(&mut self, at: Instant, filmed: Instant) {
        self.captured_sent = Some(at);
        self.filmed = Some(filmed);
        self.held_filmed = None;
        let booked = match self.captured_due {
            Some(due) if self.crowded => due.max(at),
            _ => at,
        };
        self.captured_due = booked.checked_add(self.period);
        self.last_sent = Some(at);
        self.holding = false;
        let taken = self.next_slot.map_or(at, |free| free.max(at));
        self.next_slot = taken.checked_add(self.period).map(|free| {
            at.checked_add(self.period * 2)
                .map_or(free, |most| free.min(most))
        });
        self.restart_idle();
    }

    /// What goes while the screen stays still, counted again from the
    /// last picture sent.
    fn restart_idle(&mut self) {
        self.next_repeat = self
            .last_sent
            .and_then(|at| at.checked_add(self.period + self.period / 8))
            .map(|after| self.next_slot.map_or(after, |free| free.max(after)));
        self.next_still = self.last_sent.and_then(|at| at.checked_add(STILL_EVERY));
    }
}

/// The time between two pictures, never zero: a rate of 0 is taken as 1,
/// and one beyond a picture a nanosecond as that.
fn period_of(fps: u32) -> Duration {
    (Duration::from_secs(1) / fps.max(1)).max(Duration::from_nanos(1))
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
        Still,
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
                    Some(Due::Still) => Sent::Still,
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
        let pictures: Vec<_> = sent
            .iter()
            .filter(|(_, what)| *what != Sent::Still)
            .collect();
        assert_eq!(pictures.len(), captures.len());
        for ((at, what), captured) in pictures.iter().zip(&captures) {
            assert_eq!((*at, *what), (*captured, Sent::Captured));
        }
    }

    #[test]
    fn a_screen_at_the_rate_is_not_held_back_by_one_late_image() {
        let period = Duration::from_secs(1) / 60;
        let mut captures: Vec<_> = (0..60).map(|n| period * n).collect();
        captures[10] += us(3_000);
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, Duration::from_secs(1));
        let pictures: Vec<_> = sent
            .iter()
            .filter(|(_, what)| *what != Sent::Still)
            .collect();
        assert_eq!(pictures.len(), captures.len());
        // The one right after it was filmed closer than seven eighths of
        // a period and waits a period after it; every one after goes as
        // it comes.
        assert_eq!(*pictures[11], (captures[10] + period, Sent::Held));
        for (n, ((at, what), captured)) in pictures.iter().zip(&captures).enumerate() {
            if n != 11 {
                assert_eq!((*at, *what), (*captured, Sent::Captured));
            }
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
    fn a_screen_at_the_rate_with_an_image_twice_now_and_then_is_not_crowded() {
        let period = Duration::from_secs(1) / 60;
        let mut noise = Noise::new(55);
        let mut captures = source(period, 10, us(1_000), &mut noise);
        // An image given twice every half second, two milliseconds apart.
        let twice: Vec<_> = captures
            .iter()
            .step_by(30)
            .map(|at| *at + us(2_000))
            .collect();
        captures.extend(&twice);
        captures.sort();
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, Duration::from_secs(11));
        let waited = sent
            .iter()
            .filter(|(_, what)| *what != Sent::Still)
            .filter(|(at, _)| {
                let captured = captures.iter().rev().find(|capture| *capture <= at);
                *at - *captured.unwrap() > us(3_000)
            })
            .count();
        // Only the image given twice waits, for the seven eighths after
        // the one before it.
        assert!(
            waited <= twice.len(),
            "{waited} waited, {} twice",
            twice.len()
        );
    }

    #[test]
    fn a_screen_back_at_the_rate_after_a_burst_is_no_longer_held() {
        let period = Duration::from_secs(1) / 60;
        let mut captures: Vec<_> = (0..20).map(|n| period * n).collect();
        // A burst of images four milliseconds apart, then the screen's
        // pace again, its phase moved by the burst.
        let burst_end = period * 20 + us(4_000) * 12;
        captures.extend((0..12).map(|n| period * 20 + us(4_000) * n));
        captures.extend((1..60).map(|n| burst_end + period * n));
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, burst_end + period * 61);
        let pictures: Vec<_> = sent
            .iter()
            .filter(|(_, what)| *what != Sent::Still)
            .collect();
        let after: Vec<_> = captures
            .iter()
            .filter(|at| **at > burst_end + period * (CROWDED + 2))
            .collect();
        for capture in &after {
            assert!(
                pictures.contains(&&(**capture, Sent::Captured)),
                "{capture:?} held"
            );
        }
    }

    #[test]
    fn a_faster_screen_never_goes_beyond_the_rate() {
        let period = Duration::from_secs(1) / 60;
        let mut noise = Noise::new(52);
        let sources = [
            source(Duration::from_secs(1) / 144, 10, Duration::ZERO, &mut noise),
            {
                // Bursts: a screen giving one to three images a refresh.
                let mut at = Duration::ZERO;
                (0..1200)
                    .map(|_| {
                        at += us(1_000 + noise.below(12_000) as u64);
                        at
                    })
                    .collect()
            },
        ];
        for captures in &sources {
            let mut cadence = Cadence::new(60, false);
            let sent = run(&mut cadence, captures, Duration::from_secs(10));
            let pictures: Vec<_> = sent
                .iter()
                .filter(|(_, what)| *what != Sent::Still)
                .collect();
            // Counted once the screen has been seen crowded.
            let held = &pictures[CROWDED as usize..];
            for pair in held.windows(2) {
                assert!(pair[1].0 - pair[0].0 >= period * 3 / 4, "{pair:?}");
            }
            let (first, last) = (held[0].0, held[held.len() - 1].0);
            assert!(
                last - first >= period * (held.len() as u32 - 1) - period / 4,
                "{} pictures from {first:?} to {last:?}",
                held.len()
            );
            let lasted = (*captures.last().unwrap()).min(Duration::from_secs(10));
            let at_the_rate = (lasted.as_secs_f64() * 60.0) as usize;
            assert!(pictures.len() + 2 >= at_the_rate, "{}", pictures.len());
            assert!(pictures.iter().any(|(_, what)| *what == Sent::Held));
        }
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
    fn a_still_screen_is_said_still_otherwise() {
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &[Duration::ZERO], Duration::from_secs(5));
        assert_eq!(sent[0], (Duration::ZERO, Sent::Captured));
        assert_eq!(sent.len(), 51);
        for (n, (at, what)) in sent.iter().enumerate().skip(1) {
            assert_eq!((*at, *what), (STILL_EVERY * n as u32, Sent::Still));
        }
    }

    #[test]
    fn a_loop_woken_late_says_still_once() {
        let at = Instant::now();
        let mut cadence = Cadence::new(60, false);
        cadence.on_captured(at);
        let late = at + STILL_EVERY * 10;
        assert_eq!(cadence.due(late), Some(Due::Still));
        assert_eq!(cadence.due(late), None);
        assert_eq!(cadence.next_wakeup(), Some(late + STILL_EVERY));
    }

    #[test]
    fn a_capture_right_after_a_repeat_goes_at_once_and_takes_the_next_slot() {
        let period = Duration::from_secs(1) / 60;
        let repeat = period + period / 8;
        let captures = [Duration::ZERO, repeat + us(500)];
        let mut cadence = Cadence::new(60, true);
        let sent = run(&mut cadence, &captures, repeat + period * 2);
        // The capture went at once, in the slot after the repeat's: the
        // next repeat waits for the one after that.
        assert_eq!(
            sent,
            [
                (Duration::ZERO, Sent::Captured),
                (repeat, Sent::Repeat),
                (repeat + us(500), Sent::Captured),
                (repeat + period * 2, Sent::Repeat),
            ]
        );
    }

    #[test]
    fn a_screen_changing_below_the_rate_is_never_sent_beyond_it() {
        let period = Duration::from_secs(1) / 60;
        let mut noise = Noise::new(53);
        let sources = [
            source(Duration::from_secs(1) / 50, 10, Duration::ZERO, &mut noise),
            source(Duration::from_secs(1) / 55, 10, us(1_000), &mut noise),
            source(Duration::from_secs(1) / 30, 10, us(2_000), &mut noise),
            {
                let mut at = Duration::ZERO;
                (0..400)
                    .map(|_| {
                        at += period + us(noise.below(2 * period.as_micros() as usize) as u64);
                        at
                    })
                    .collect()
            },
        ];
        for captures in &sources {
            let mut cadence = Cadence::new(60, true);
            let until = *captures.last().unwrap() + period;
            let sent = run(&mut cadence, captures, until);
            let fresh: Vec<_> = sent
                .iter()
                .filter(|(_, what)| *what != Sent::Repeat)
                .collect();
            assert_eq!(fresh.len(), captures.len());
            for ((at, what), capture) in fresh.iter().zip(captures) {
                assert_eq!((*at, *what), (*capture, Sent::Captured));
            }
            for (n, (start, _)) in sent.iter().enumerate() {
                let within = sent[n..]
                    .iter()
                    .take_while(|(at, _)| *at < *start + Duration::from_secs(1))
                    .count();
                assert!(
                    within <= 61,
                    "{within} pictures in the second from {start:?}"
                );
            }
            assert!(sent.iter().any(|(_, what)| *what == Sent::Repeat));
        }
    }

    #[test]
    fn repeats_come_back_soon_after_a_faster_screen_stops() {
        let period = Duration::from_secs(1) / 60;
        let captures = source(
            Duration::from_secs(1) / 144,
            10,
            Duration::ZERO,
            &mut Noise::new(54),
        );
        let last = *captures.last().unwrap();
        let mut cadence = Cadence::new(60, true);
        let sent = run(&mut cadence, &captures, last + period * 3);
        let (at, what) = sent.last().unwrap();
        assert_eq!(*what, Sent::Repeat);
        assert!(*at - sent[sent.len() - 2].0 <= period * 2, "{sent:?}");
    }

    #[test]
    fn a_burst_sends_its_first_image_then_its_last() {
        let period = Duration::from_secs(1) / 60;
        let captures = [us(0), us(1_000), us(2_000), us(3_000), us(40_000)];
        let mut cadence = Cadence::new(60, false);
        let sent = run(&mut cadence, &captures, Duration::from_millis(100));
        assert_eq!(
            sent,
            vec![
                (us(0), Sent::Captured),
                (period, Sent::Held),
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
        let due = at + Duration::from_secs(1) / 60;
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
        assert_eq!(cadence.next_wakeup(), Some(at + period));
        assert_eq!(cadence.due(at + period), Some(Due::EmitHeld));

        let sent = at + period;
        cadence.set_steady(true);
        assert_eq!(cadence.next_wakeup(), Some(sent + period + period / 8));
        cadence.set_steady(false);
        assert_eq!(cadence.next_wakeup(), Some(sent + STILL_EVERY));
        assert_eq!(cadence.due(sent + STILL_EVERY - us(1)), None);
        assert_eq!(cadence.due(sent + STILL_EVERY), Some(Due::Still));
    }

    #[test]
    fn nothing_is_repeated_before_a_first_picture() {
        let mut cadence = Cadence::new(0, true);
        assert_eq!(cadence.next_wakeup(), None);
        assert_eq!(cadence.due(Instant::now()), None);
        assert_eq!(cadence.period, Duration::from_secs(1));
    }

    #[test]
    fn a_rate_beyond_a_picture_a_nanosecond_still_keeps_a_grid() {
        let at = Instant::now();
        let mut cadence = Cadence::new(u32::MAX, true);
        assert_eq!(cadence.period, Duration::from_nanos(1));
        assert_eq!(cadence.on_captured(at), Now::Emit);
        let late = at + us(1_000);
        assert_eq!(cadence.due(late), Some(Due::Repeat));
        assert_eq!(cadence.next_wakeup(), Some(late + Duration::from_nanos(1)));
    }
}
