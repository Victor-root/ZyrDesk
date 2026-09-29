//! What a session's measures say about its health.
//!
//! Three things can hold a session up, and each is told apart: the link
//! between the two computers, when the picture freezes or frames get lost
//! on the way; the far computer, when it cannot encode a frame in the
//! time a frame lasts; and this one, when it cannot decode it. They may
//! very well struggle together, and then all of them are said.
//!
//! What makes it quick. The player measures what a session costs five
//! times a second, and one of its numbers is how long the picture has not
//! moved. That one is not averaged and not waited for, it is true the
//! moment it is read, and it is what tells of a frozen picture before the
//! hand has had time to move the mouse to check.
//!
//! Only arithmetic on a reading, and the one memory that keeps what is
//! lit from blinking: whatever shows it, the badges of the window first
//! of all, asks here.

use std::time::{Duration, Instant};

use zyr_player::Measures;
use zyr_proto::fact::Fact;

/// How long the picture must have been frozen for it to show, in
/// milliseconds.
///
/// A third of a second. Below that, it is one of the late frames that go
/// by all the time, and a badge that blinks at that pace no longer means
/// anything; above it, the person has already noticed and the badge
/// arrives after them.
const FROZEN_MS: f64 = 350.0;

/// How many frames lost on the way, as a percentage of the second gone
/// by, before it is said.
///
/// Two percent: one frame in fifty, which shows on a desktop being
/// scrolled and does not show on a still desktop.
const LOST_PCT: f64 = 2.0;

/// And how many arriving too late to be shown.
///
/// Higher than for the ones before: these did arrive, and what they
/// say is that the link is shaking rather than losing.
const TOO_LATE_PCT: f64 = 5.0;

/// How long something stays lit after its cause has stopped.
///
/// Without this it blinks: the cause holds for one reading, readings
/// arrive five times a second, and a network that is doing badly does
/// badly in fits and starts. A second and a half is what it takes for
/// the person, looking up at the corner of the picture, to still find
/// something there.
const HOLDS: Duration = Duration::from_millis(1500);

/// What a reading says about each of the three: nothing, or what is
/// wrong.
///
/// Why, and not only whether: a badge that lights up with nothing saying
/// why is a badge people end up ignoring. Told as facts, put into words
/// by whoever shows them and written as they are in the journal.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Reads {
    /// The link between the two computers.
    pub link: Option<Fact>,
    /// The picture as the far computer makes it.
    pub far: Option<Fact>,
    /// And as this one makes it again.
    pub here: Option<Fact>,
}

/// What a reading says, with no memory of any kind.
///
/// The time available for a frame is worked out from the measured frame
/// rate and not from the one asked for, and that is not a second best:
/// what we are after is whether one of the two computers is what sets
/// the pace. A host that takes twenty-five milliseconds per frame serves
/// forty frames a second, so its encoding time **is** the time
/// available, and it is said; the same host at three milliseconds on a
/// thirty-frame session has thirty-three milliseconds ahead of it and
/// gets in nobody's way. Asking for the wanted frame rate would have cost
/// a round trip to the service on every reading, and would have been
/// wrong as soon as someone changes it during the session.
pub fn read(measures: &Measures) -> Reads {
    let mut reads = Reads::default();

    if let Some(frozen) = measures.since_frame_ms.filter(|held| *held >= FROZEN_MS) {
        reads.link = Some(Fact::new("badge.frozen").with("ms", format!("{frozen:.0}")));
    } else if let Some(lost) = measures.dropped_network_pct.filter(|pct| *pct >= LOST_PCT) {
        reads.link = Some(Fact::new("badge.lost").with("pct", format!("{lost:.1}")));
    } else if let Some(late) = measures
        .dropped_jitter_pct
        .filter(|pct| *pct >= TOO_LATE_PCT)
    {
        reads.link = Some(Fact::new("badge.late").with("pct", format!("{late:.1}")));
    }

    // A missing frame rate leaves these two off: without it there is no
    // time available, so nothing to compare, and something lit for want
    // of a measure would be lit for nothing.
    //
    // Each of the two is weighed on its own and not one or the other:
    // they may very well struggle together, on two tired machines or on a
    // session too big for both, and that is said as well.
    if let Some(budget) = measures
        .fps
        .filter(|rate| *rate > 0.0)
        .map(|rate| 1000.0 / rate)
    {
        let per_frame = |late: Fact, each: f64| {
            late.with("each", format!("{each:.0}"))
                .with("budget", format!("{budget:.0}"))
        };
        if let Some(host) = measures.host_ms.filter(|each| *each >= budget) {
            reads.far = Some(per_frame(Fact::new("badge.far_too_slow"), host));
        }
        if let Some(decode) = measures.decode_ms.filter(|each| *each >= budget) {
            reads.here = Some(per_frame(Fact::new("badge.here_too_slow"), decode));
        }
    }
    reads
}

/// What stays lit, once the reading has been calmed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Lit {
    pub link: bool,
    pub far: bool,
    pub here: bool,
}

impl Lit {
    /// Nothing at all.
    pub fn nothing(self) -> bool {
        !self.link && !self.far && !self.here
    }
}

/// What keeps the three lit from one reading to the next.
///
/// One thing only, and it is all that separates a useful badge from a
/// string of fairy lights: lit on the reading that says so, off only
/// once nothing has said so for a while.
#[derive(Default)]
pub struct Steady {
    link: Option<Instant>,
    far: Option<Instant>,
    here: Option<Instant>,
}

impl Steady {
    /// What this reading leaves lit.
    pub fn after(&mut self, reads: &Reads, now: Instant) -> Lit {
        Lit {
            link: still(&mut self.link, reads.link.is_some(), now),
            far: still(&mut self.far, reads.far.is_some(), now),
            here: still(&mut self.here, reads.here.is_some(), now),
        }
    }
}

/// One of them, lit again for a while when its cause is there.
fn still(until: &mut Option<Instant>, wrong: bool, now: Instant) -> bool {
    if wrong {
        *until = Some(now + HOLDS);
    }
    until.is_some_and(|end| now < end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reading from a session that is doing well, at sixty
    /// frames.
    fn healthy() -> Measures {
        Measures {
            fps: Some(60.0),
            decode_ms: Some(0.4),
            render_ms: Some(15.0),
            host_ms: Some(2.3),
            network_ms: Some(8.0),
            dropped_network_pct: Some(0.0),
            dropped_jitter_pct: Some(0.1),
            since_frame_ms: Some(12.0),
            ..Default::default()
        }
    }

    #[test]
    fn a_session_that_is_going_well_lights_nothing() {
        assert_eq!(read(&healthy()), Reads::default());
    }

    #[test]
    fn a_reading_that_says_nothing_lights_nothing_either() {
        // A session that has just opened: the player has not yet
        // measured a single second. Nothing is known, so nothing lights
        // up.
        assert_eq!(read(&Measures::default()), Reads::default());
    }

    #[test]
    fn a_picture_that_has_stopped_lights_the_link() {
        let frozen = Measures {
            since_frame_ms: Some(FROZEN_MS),
            ..healthy()
        };
        let reads = read(&frozen);
        assert!(reads.link.is_some_and(|why| why.code() == "badge.frozen"));
        assert!(reads.far.is_none() && reads.here.is_none());
    }

    #[test]
    fn frames_lost_on_the_way_light_the_link_too() {
        let lost = Measures {
            dropped_network_pct: Some(LOST_PCT),
            ..healthy()
        };
        assert!(
            read(&lost)
                .link
                .is_some_and(|why| why.code() == "badge.lost")
        );

        let late = Measures {
            dropped_jitter_pct: Some(TOO_LATE_PCT),
            ..healthy()
        };
        assert!(
            read(&late)
                .link
                .is_some_and(|why| why.code() == "badge.late")
        );
    }

    #[test]
    fn a_host_that_cannot_keep_up_lights_the_far_picture() {
        // Twenty-five milliseconds per frame on a session that serves
        // forty: its encoding is what sets the pace.
        let slow = Measures {
            fps: Some(40.0),
            host_ms: Some(25.0),
            ..healthy()
        };
        let reads = read(&slow);
        assert!(
            reads
                .far
                .is_some_and(|why| why.code() == "badge.far_too_slow")
        );
        // And this one has nothing to do with it: the right one of the
        // two computers is named, not both.
        assert!(reads.here.is_none());
        assert!(reads.link.is_none());
    }

    #[test]
    fn a_computer_that_cannot_decode_in_time_lights_it_as_well() {
        let slow = Measures {
            fps: Some(30.0),
            decode_ms: Some(40.0),
            ..healthy()
        };
        let reads = read(&slow);
        assert!(
            reads
                .here
                .is_some_and(|why| why.code() == "badge.here_too_slow")
        );
        assert!(reads.far.is_none());
    }

    #[test]
    fn two_computers_that_both_struggle_light_both() {
        // A session too big for both machines: there is no choosing
        // which one to name, both are.
        let both = Measures {
            fps: Some(24.0),
            host_ms: Some(45.0),
            decode_ms: Some(50.0),
            ..healthy()
        };
        let reads = read(&both);
        assert!(reads.far.is_some());
        assert!(reads.here.is_some());

        let mut steady = Steady::default();
        assert_eq!(
            steady.after(&reads, Instant::now()),
            Lit {
                link: false,
                far: true,
                here: true
            }
        );
    }

    #[test]
    fn a_slow_session_that_asked_for_slow_is_not_a_fault() {
        // Thirty frames a second leave thirty-three milliseconds per
        // frame: a host at twenty is late for nothing.
        let calm = Measures {
            fps: Some(30.0),
            host_ms: Some(20.0),
            decode_ms: Some(5.0),
            ..healthy()
        };
        assert_eq!(read(&calm), Reads::default());
    }

    #[test]
    fn the_time_a_frame_waits_for_the_screen_is_not_counted() {
        // The render time includes waiting for the screen's refresh,
        // so it always comes close to the time available: counted, the
        // picture here would be lit the whole session.
        let ordinary = Measures {
            render_ms: Some(16.6),
            ..healthy()
        };
        assert_eq!(read(&ordinary), Reads::default());
    }

    #[test]
    fn it_stays_lit_for_a_moment_after_its_cause_has_gone() {
        // Without this it blinks: the cause holds for one reading, and a
        // dozen go by every second.
        let start = Instant::now();
        let mut steady = Steady::default();
        let wrong = read(&Measures {
            since_frame_ms: Some(900.0),
            ..healthy()
        });

        assert_eq!(
            steady.after(&wrong, start),
            Lit {
                link: true,
                far: false,
                here: false
            }
        );
        let well = read(&healthy());
        assert!(steady.after(&well, start + Duration::from_millis(100)).link);
        assert!(
            steady
                .after(&well, start + HOLDS - Duration::from_millis(1))
                .link
        );
        assert!(!steady.after(&well, start + HOLDS).link);
    }

    #[test]
    fn a_cause_that_comes_back_holds_it_on_from_there() {
        let start = Instant::now();
        let mut steady = Steady::default();
        let wrong = read(&Measures {
            since_frame_ms: Some(900.0),
            ..healthy()
        });
        let well = read(&healthy());

        steady.after(&wrong, start);
        let again = start + HOLDS - Duration::from_millis(10);
        steady.after(&wrong, again);
        // The second cause starts again from where it is, and not from
        // the first: otherwise a network doing badly in fits and starts
        // would turn it off in the middle of its fits.
        assert!(steady.after(&well, start + HOLDS).link);
        assert!(!steady.after(&well, again + HOLDS).link);
    }

    #[test]
    fn the_three_are_counted_apart() {
        let start = Instant::now();
        let mut steady = Steady::default();
        let only_the_far_one = read(&Measures {
            fps: Some(40.0),
            host_ms: Some(25.0),
            ..healthy()
        });
        let lit = steady.after(&only_the_far_one, start);
        assert_eq!(
            lit,
            Lit {
                link: false,
                far: true,
                here: false
            }
        );
        assert!(!lit.nothing());
        assert!(Lit::default().nothing());
    }
}
