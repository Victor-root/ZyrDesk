//! What to do about a duplication that stops before it has given a
//! single image.
//!
//! After the screens changed under it, Windows sometimes hands out a
//! duplication that fails at its first image, and again at the first
//! image of the next one made. Made again at once each time, as one that
//! stops for a mode change or a desktop switch is, the capture spins on
//! it: a processor full, the desktop's own threads busy answering, and a
//! picture that stays frozen for as long as it lasts.
//!
//! So once it happens twice in a row the tries are spaced, and the more
//! it repeats the less is trusted of what was kept from the screens
//! before: from the third, they are listed again from a new factory
//! rather than taken from the last list, and every tenth the Direct3D
//! device is made again as well.
//!
//! Every decision takes the time as an argument, so that it can be tried
//! against a simulated clock.

use std::time::{Duration, Instant};

/// How far apart the tries are at first, and for how long.
const QUICKLY: Duration = Duration::from_millis(5);
const QUICK_FOR: Duration = Duration::from_millis(400);

/// And after that.
const SLOWLY: Duration = Duration::from_millis(25);

/// From which duplication in a row the screens are listed again.
const LISTS_FROM: u32 = 3;

/// And the device is made again, every that many.
const DEVICE_EVERY: u32 = 10;

/// What is not trusted any more of what was kept from the screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Distrust {
    /// Nothing yet: the screen and the lists it was found in stand.
    Nothing,
    /// The lists: the screens are listed again from a new factory.
    Lists,
    /// The lists, and the Direct3D device with them.
    ListsAndDevice,
}

/// The duplications that stopped before giving anything, in a row.
#[derive(Debug, Default, Clone)]
pub(crate) struct ShortLived {
    /// Whether the duplication in hand gave an image, or waited for one
    /// without a complaint.
    works: bool,
    /// How many stopped before that, one after the other.
    in_a_row: u32,
    /// When the first of them stopped.
    since: Option<Instant>,
    /// The count at which the device was last made again.
    device_made_at: u32,
}

impl ShortLived {
    /// A duplication was made: it has yet to show it works.
    pub(crate) fn made(&mut self) {
        self.works = false;
    }

    /// The duplication in hand gave an image, or waited for one without
    /// a complaint: what stopped before it is behind.
    pub(crate) fn works(&mut self) {
        self.works = true;
        self.in_a_row = 0;
        self.since = None;
        self.device_made_at = 0;
    }

    /// The duplication in hand stopped, and whether that was before it
    /// gave anything.
    pub(crate) fn stopped(&mut self, now: Instant) -> bool {
        if self.works {
            return false;
        }
        self.in_a_row = self.in_a_row.saturating_add(1);
        self.since.get_or_insert(now);
        true
    }

    /// How many stopped before giving anything, in a row.
    pub(crate) fn in_a_row(&self) -> u32 {
        self.in_a_row
    }

    /// When the first of them stopped.
    pub(crate) fn since(&self) -> Option<Instant> {
        self.since
    }

    /// How long to wait before making the next one.
    ///
    /// The first stop is what a mode change or a desktop switch looks
    /// like, and is answered at once.
    pub(crate) fn wait(&self, now: Instant) -> Duration {
        match self.since {
            Some(since) if self.in_a_row >= 2 => {
                if now.saturating_duration_since(since) < QUICK_FOR {
                    QUICKLY
                } else {
                    SLOWLY
                }
            }
            _ => Duration::ZERO,
        }
    }

    /// What is not trusted for the next try. The device is made again
    /// once at each tenth, however many tries that count sees.
    pub(crate) fn distrust(&mut self) -> Distrust {
        if self.in_a_row >= DEVICE_EVERY
            && self.in_a_row.is_multiple_of(DEVICE_EVERY)
            && self.device_made_at != self.in_a_row
        {
            self.device_made_at = self.in_a_row;
            Distrust::ListsAndDevice
        } else if self.in_a_row >= LISTS_FROM {
            Distrust::Lists
        } else {
            Distrust::Nothing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stopping(times: u32) -> ShortLived {
        let mut short_lived = ShortLived::default();
        let now = Instant::now();
        for _ in 0..times {
            short_lived.made();
            assert!(short_lived.stopped(now));
        }
        short_lived
    }

    #[test]
    fn a_duplication_that_gave_something_did_not_stop_too_soon() {
        let mut short_lived = ShortLived::default();
        short_lived.made();
        short_lived.works();
        assert!(!short_lived.stopped(Instant::now()));
        assert_eq!(short_lived.in_a_row(), 0);
    }

    #[test]
    fn the_first_stop_before_an_image_is_answered_at_once() {
        let short_lived = stopping(1);
        assert_eq!(short_lived.wait(Instant::now()), Duration::ZERO);
        assert_eq!(short_lived.clone().distrust(), Distrust::Nothing);
    }

    #[test]
    fn from_the_second_the_tries_are_spaced_and_then_spaced_further() {
        let short_lived = stopping(2);
        let since = short_lived.since().unwrap();
        assert_eq!(short_lived.wait(since), QUICKLY);
        assert_eq!(short_lived.wait(since + QUICK_FOR), SLOWLY);
        assert_eq!(short_lived.wait(since + Duration::from_secs(30)), SLOWLY);
    }

    #[test]
    fn the_lists_are_read_again_from_the_third_and_not_before() {
        assert_eq!(stopping(2).clone().distrust(), Distrust::Nothing);
        assert_eq!(stopping(3).clone().distrust(), Distrust::Lists);
        assert_eq!(stopping(9).clone().distrust(), Distrust::Lists);
    }

    #[test]
    fn the_device_is_made_again_once_at_each_tenth() {
        let mut short_lived = stopping(10);
        assert_eq!(short_lived.distrust(), Distrust::ListsAndDevice);
        // Tried again with the same count, the device is not made again.
        assert_eq!(short_lived.distrust(), Distrust::Lists);
        short_lived.made();
        assert!(short_lived.stopped(Instant::now()));
        assert_eq!(short_lived.distrust(), Distrust::Lists);

        let mut short_lived = stopping(20);
        assert_eq!(short_lived.distrust(), Distrust::ListsAndDevice);
    }

    #[test]
    fn a_duplication_that_works_ends_the_run() {
        let mut short_lived = stopping(12);
        short_lived.made();
        short_lived.works();
        assert_eq!(short_lived.in_a_row(), 0);
        assert_eq!(short_lived.since(), None);
        assert_eq!(short_lived.distrust(), Distrust::Nothing);
        // The next one to stop at once starts a new run.
        short_lived.made();
        assert!(short_lived.stopped(Instant::now()));
        assert_eq!(short_lived.in_a_row(), 1);
    }
}
