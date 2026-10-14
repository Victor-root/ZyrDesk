//! What moves on a window this program draws, and how far along it is.
//!
//! A hover that fades in, a switch whose thumb slides, a dialog that
//! settles: each is a number going from one value to another over a
//! short time. The drawing asks for the value it wants for a thing, and
//! gets the value that thing has reached on its way there; a thing asked
//! for something new sets off from where it stands, never from where it
//! started, so a hand passing back and forth never makes anything jump.
//!
//! Nothing moves without a reason to: as soon as every value has arrived,
//! the window can stop being redrawn, and cost nothing until the next
//! gesture. And nothing moves at all when the person has asked Windows
//! not to animate.
//!
//! Plain numbers and a clock, tried on any system: what draws them is
//! the window's business.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// What a moving value is known by: what it belongs to, and which of its
/// aspects it is.
pub type Key = (u64, u8);

/// One value on its way.
struct Moving {
    from: f32,
    to: f32,
    since: Instant,
    over: Duration,
}

impl Moving {
    /// How far along it is right now, eased: quick to leave, gentle to
    /// arrive, which is how a thing a hand has just touched feels alive.
    fn at(&self, now: Instant) -> f32 {
        let part = if self.over.is_zero() {
            1.0
        } else {
            (now.saturating_duration_since(self.since).as_secs_f32() / self.over.as_secs_f32())
                .min(1.0)
        };
        let eased = 1.0 - (1.0 - part).powi(3);
        self.from + (self.to - self.from) * eased
    }

    fn arrived(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.since) >= self.over
    }
}

/// Every value that moves, and the clock of the frame being drawn.
pub struct Motion {
    values: HashMap<Key, Moving>,
    /// The values asked for during the frame being drawn: the others
    /// belong to things no longer on screen, and are let go.
    asked: HashSet<Key>,
    now: Instant,
    moving: bool,
    /// Whether the person lets Windows animate.
    lively: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Self::new()
    }
}

impl Motion {
    pub fn new() -> Self {
        Motion {
            values: HashMap::new(),
            asked: HashSet::new(),
            now: Instant::now(),
            moving: false,
            lively: true,
        }
    }

    /// Starts a frame, at this instant.
    pub fn begin(&mut self, lively: bool) {
        self.now = Instant::now();
        self.moving = false;
        self.lively = lively;
        self.asked.clear();
    }

    /// Ends the frame, and says whether something is still on its way,
    /// which is what asks for the next one.
    pub fn end(&mut self) -> bool {
        let asked = std::mem::take(&mut self.asked);
        self.values.retain(|key, _| asked.contains(key));
        self.asked = asked;
        self.moving
    }

    /// The value a thing has reached on its way to `goal`, which it
    /// reaches in `over`.
    ///
    /// The first time a thing is asked for, it is already there: a page
    /// that opens shows itself as it is, it does not unfold.
    pub fn follow(&mut self, key: Key, goal: f32, over: Duration) -> f32 {
        let now = self.now;
        let over = if self.lively { over } else { Duration::ZERO };
        self.asked.insert(key);
        let moving = self.values.entry(key).or_insert(Moving {
            from: goal,
            to: goal,
            since: now,
            over: Duration::ZERO,
        });
        if moving.to != goal {
            let reached = moving.at(now);
            *moving = Moving {
                from: reached,
                to: goal,
                since: now,
                over,
            };
        }
        if !moving.arrived(now) {
            self.moving = true;
        }
        moving.at(now)
    }

    /// The same, for something that comes into being: it starts from
    /// `from` the first time it is asked for, rather than already there.
    pub fn appear(&mut self, key: Key, from: f32, goal: f32, over: Duration) -> f32 {
        if !self.values.contains_key(&key) {
            self.values.insert(
                key,
                Moving {
                    from,
                    to: from,
                    since: self.now,
                    over: Duration::ZERO,
                },
            );
        }
        self.follow(key, goal, over)
    }

    /// Asks for one more frame without any value moving: what needs to be
    /// drawn again at the pace of the screen, like a bar that waits.
    pub fn keep_going(&mut self) {
        self.moving = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: Key = (1, 0);

    #[test]
    fn a_thing_first_asked_for_is_already_where_it_belongs() {
        let mut motion = Motion::new();
        motion.begin(true);
        assert_eq!(motion.follow(KEY, 1.0, Duration::from_millis(200)), 1.0);
        assert!(!motion.end());
    }

    #[test]
    fn a_thing_asked_for_something_new_sets_off_from_where_it_stands() {
        let mut motion = Motion::new();
        motion.begin(true);
        motion.follow(KEY, 0.0, Duration::from_millis(200));
        motion.end();
        motion.begin(true);
        let started = motion.follow(KEY, 1.0, Duration::from_secs(60));
        assert!(started < 0.01, "{started}");
        assert!(motion.end());
    }

    #[test]
    fn nothing_moves_for_a_person_who_asked_windows_not_to_animate() {
        let mut motion = Motion::new();
        motion.begin(false);
        motion.follow(KEY, 0.0, Duration::from_millis(200));
        motion.end();
        motion.begin(false);
        assert_eq!(motion.follow(KEY, 1.0, Duration::from_secs(60)), 1.0);
        assert!(!motion.end());
    }

    #[test]
    fn what_comes_into_being_starts_from_where_it_was_asked_to() {
        let mut motion = Motion::new();
        motion.begin(true);
        let first = motion.appear(KEY, 0.0, 1.0, Duration::from_secs(60));
        assert!(first < 0.01, "{first}");
        assert!(motion.end());
    }

    #[test]
    fn a_thing_no_longer_drawn_is_let_go() {
        let mut motion = Motion::new();
        motion.begin(true);
        motion.follow(KEY, 0.0, Duration::from_millis(200));
        motion.end();
        motion.begin(true);
        motion.end();
        assert!(motion.values.is_empty());
    }
}
