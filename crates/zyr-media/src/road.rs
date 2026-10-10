//! What the road between the two computers bears, read from how each
//! picture arrives, and the rate the host is asked to keep under.
//!
//! The rate the person chose is the most a session sends, and nothing
//! said whether the road carried it. When it does not, the narrowest link
//! on the way queues what it cannot pass: every picture arrives a little
//! later than the one before, until the queue overflows and the pictures
//! are lost in runs that no parity repairs. Only a lower rate helps then.
//!
//! Each picture is a train of packets that left the host together, and
//! how long it took to come is a measure of the road. The least time a
//! first packet has taken over the last ten seconds is the road empty:
//! one packet, nothing ahead of it. The time the last packet of a
//! picture took, less that, is what the picture spent waiting behind
//! others and squeezing through the narrowest link. Over the time
//! between two pictures, it is how full that link is (as Tencent's
//! Pudica reads it, NSDI 2024): past one, each picture takes longer to
//! pass than the time until the next, and a queue builds. The two clocks
//! never need to agree, their difference going away with the least.
//!
//! When pictures keep arriving slower than they leave, or several are
//! lost in a second, the ceiling falls to a little under what actually
//! arrived; when the road has been calm for a while, it climbs back,
//! quickly while far under what the road last bore, slowly near it, and
//! goes away once it reaches the rate the person chose. Every decision
//! takes the time as an argument, so that it can be tried against a
//! simulated road.

use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

/// How far back the least delay is looked for: long enough to have seen
/// a picture pass an empty road, short enough to follow the clocks'
/// drift and a road that changes.
const LEAST_OVER: Duration = Duration::from_secs(10);

/// What the arrival rate is measured over.
const RATE_OVER: Duration = Duration::from_millis(500);

/// What the fullness is smoothed over.
const SMOOTHED_OVER: Duration = Duration::from_millis(200);

/// Pictures in a row arriving slower than they leave that say the road
/// is full, rather than a large picture passing.
const OVERFULL_PICTURES: u32 = 3;

/// Pictures lost within a second that say the road is overflowing.
const LOST_IN_A_SECOND: usize = 3;

/// What the ceiling is set to, of what arrived, once the road is full:
/// enough below it for its queue to drain.
const BELOW_WHAT_ARRIVED: f64 = 0.85;

/// The most and the least a single fall takes off the rate.
const MOST_FALL: f64 = 0.5;
const LEAST_FALL: f64 = 0.85;

/// How long after a change nothing else is decided: the encoder takes
/// the new rate at its next picture, and what queued before it still
/// has to drain.
const SETTLE: Duration = Duration::from_millis(500);

/// How calm the road stays before the ceiling climbs: far under what it
/// last bore, and near it.
const CALM_FAR: Duration = Duration::from_secs(2);
const CALM_NEAR: Duration = Duration::from_secs(6);

/// A climb far under what the road last bore, and near it.
const CLIMB_FAR: f64 = 1.25;
const CLIMB_NEAR: f64 = 1.08;

/// Under this share of what the road last bore, a climb is a far one.
const NEAR: f64 = 0.75;

/// The share of what the road last bore the ceiling stays under, for as
/// long as that is remembered: a climb past it would only fill the road
/// again.
const HELD_UNDER: f64 = 0.95;

/// Fullness under which the road is plainly wider than what passes, and
/// the share of the ceiling the stream has to use for that to say
/// anything: a still screen sends little whatever the road.
const FREE_FULLNESS: f64 = 0.5;
const USED: f64 = 0.8;

/// How long the pictures queued behind a key frame are not held against
/// the road, at most: until one passes as fast as it left.
const KEY_PASSING: Duration = Duration::from_millis(500);

/// Fullness under which the road is calm.
const CALM_FULLNESS: f64 = 0.85;

/// How long what the road bore at its last fall is believed: a road
/// that was crowded may no longer be.
const BORE_REMEMBERED: Duration = Duration::from_secs(30);

/// The shortest time fullness is counted over: at a high rate of
/// pictures the road's own jitter is a large part of an interval, and
/// would read as a full road when nothing queues.
const SHORTEST_INTERVAL: Duration = Duration::from_micros(16_667);

/// The lowest ceiling, in kilobits a second: under it a picture is no
/// longer worth looking at, and the person's own choice decides.
pub const LOWEST_KBPS: u32 = 3_000;

/// A picture that arrived whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrival {
    /// When the host handed it over, on the host's clock, in
    /// microseconds (low 32 bits).
    pub sent_us: u32,
    /// When its first packet arrived here, on this computer's clock, in
    /// microseconds.
    pub first_us: u64,
    /// When its last packet did.
    pub last_us: u64,
    /// The same moment, on the monotonic clock.
    pub at: Instant,
    /// A key frame, which may be many times the others.
    pub key: bool,
}

/// Why the ceiling moved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Because {
    /// Pictures kept arriving slower than they left: how full the road
    /// was, on average over the last moment.
    Full { fullness: f64 },
    /// Pictures were lost.
    Lost { pictures: usize },
    /// The road stayed calm.
    Calm,
}

impl fmt::Display for Because {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Because::Full { fullness } => write!(
                f,
                "pictures kept arriving slower than they left, the road {:.0} % full",
                fullness * 100.0
            ),
            Because::Lost { pictures } => write!(f, "{pictures} pictures lost within a second"),
            Because::Calm => f.write_str("the road stayed calm"),
        }
    }
}

/// A new ceiling, and what led to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ceiling {
    /// The most the host is to send, in kilobits a second on the wire;
    /// none once it reaches the rate the person chose.
    pub kbps: Option<u32>,
    /// What arrived over the last moment, in kilobits a second.
    pub arrived_kbps: u32,
    pub because: Because,
}

/// What the road bears, and the ceiling it calls for.
#[derive(Debug, Clone)]
pub struct Road {
    /// The rate the person chose, in kilobits a second.
    wanted_kbps: u32,
    /// Time between two pictures at the session's rate.
    interval: Duration,
    /// The first delay seen, which the others are counted from: the two
    /// clocks wrap, and their difference means nothing on its own.
    reference: Option<u32>,
    /// The least delay of each second, over the last ten.
    least: VecDeque<(Instant, i64)>,
    /// The fullness of each picture over the last moment.
    fullness: VecDeque<(Instant, f64)>,
    /// Pictures in a row that took longer than an interval to pass.
    overfull: u32,
    /// When the last key frame came, while the pictures behind it still
    /// queue.
    key_passing: Option<Instant>,
    /// Bytes arrived, by 10 ms.
    arrived: VecDeque<(Instant, u64)>,
    /// When the first byte arrived: no rate is believed before a whole
    /// measure of it.
    first_arrival: Option<Instant>,
    lost: VecDeque<Instant>,
    /// The ceiling in force, if any.
    ceiling: Option<u32>,
    /// When the ceiling last changed, or the road was last seen full.
    last_change: Option<Instant>,
    /// When the road was last not calm.
    unsettled: Option<Instant>,
    /// What arrived when the road was last full, and when.
    bore: Option<(u32, Instant)>,
    /// The pictures since the last reading, and how full the road was.
    pictures: u32,
    fullness_sum: f64,
    fullness_most: f64,
}

/// How the road went since the last reading, for the journal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub pictures: u32,
    /// How full the road was, on average and at worst, in percent.
    pub fullness: u32,
    pub fullness_most: u32,
    pub arrived_kbps: Option<u32>,
    pub ceiling: Option<u32>,
}

impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} pictures, the road {} % full on average and {} % at worst",
            self.pictures, self.fullness, self.fullness_most
        )?;
        if let Some(kbps) = self.arrived_kbps {
            write!(f, ", {kbps} kb/s arrived")?;
        }
        match self.ceiling {
            Some(kbps) => write!(f, ", the host kept under {kbps} kb/s"),
            None => f.write_str(", no ceiling"),
        }
    }
}

impl Road {
    /// A road for a session at `fps` pictures a second, which the person
    /// wants at `wanted_kbps`.
    pub fn new(wanted_kbps: u32, fps: u16) -> Self {
        Self {
            wanted_kbps,
            interval: interval_of(fps),
            reference: None,
            least: VecDeque::new(),
            fullness: VecDeque::new(),
            overfull: 0,
            key_passing: None,
            arrived: VecDeque::new(),
            first_arrival: None,
            lost: VecDeque::new(),
            ceiling: None,
            last_change: None,
            unsettled: None,
            bore: None,
            pictures: 0,
            fullness_sum: 0.0,
            fullness_most: 0.0,
        }
    }

    /// The person chose another rate: whatever the road was found to
    /// bear is forgotten, the choice being theirs to try.
    pub fn wanted(&mut self, kbps: u32) {
        self.wanted_kbps = kbps;
        self.ceiling = None;
        self.bore = None;
    }

    /// The pictures now come `fps` a second.
    pub fn fps(&mut self, fps: u16) {
        self.interval = interval_of(fps);
    }

    /// The ceiling in force, if any.
    pub fn ceiling(&self) -> Option<u32> {
        self.ceiling
    }

    /// A video datagram of `bytes` arrived at `at`.
    pub fn datagram(&mut self, bytes: usize, at: Instant) {
        self.first_arrival.get_or_insert(at);
        let bytes = bytes as u64;
        match self.arrived.back_mut() {
            Some((bucket, sum)) if at.saturating_duration_since(*bucket) < BUCKET => *sum += bytes,
            _ => self.arrived.push_back((at, bytes)),
        }
        while self
            .arrived
            .front()
            .is_some_and(|(bucket, _)| at.saturating_duration_since(*bucket) > RATE_OVER)
        {
            self.arrived.pop_front();
        }
    }

    /// A picture arrived whole.
    pub fn whole(&mut self, arrival: Arrival) {
        let first = self.delay(arrival.first_us, arrival.sent_us);
        let delay = self.delay(arrival.last_us, arrival.sent_us);
        let at = arrival.at;
        match self.least.back_mut() {
            Some((second, least)) if at.saturating_duration_since(*second) < SECOND => {
                *least = (*least).min(first);
            }
            _ => self.least.push_back((at, first)),
        }
        while self
            .least
            .front()
            .is_some_and(|(second, _)| at.saturating_duration_since(*second) > LEAST_OVER)
        {
            self.least.pop_front();
        }
        let least = self
            .least
            .iter()
            .map(|(_, least)| *least)
            .min()
            .unwrap_or(first);
        let waited = Duration::from_micros(u64::try_from(delay - least).unwrap_or(0));
        let fullness = waited.as_secs_f64() / self.interval.as_secs_f64();
        if arrival.key {
            self.key_passing = Some(at);
        }
        // A key frame is many pictures' worth, and those that left
        // behind it wait for it to pass: a queue that says nothing of the
        // road, as long as it drains.
        if let Some(key) = self.key_passing {
            if fullness > 1.0 && at.saturating_duration_since(key) < KEY_PASSING {
                return;
            }
            self.key_passing = None;
        }
        self.overfull = if fullness > 1.0 { self.overfull + 1 } else { 0 };
        self.pictures += 1;
        self.fullness_sum += fullness;
        self.fullness_most = self.fullness_most.max(fullness);
        self.fullness.push_back((at, fullness));
        while self
            .fullness
            .front()
            .is_some_and(|(when, _)| at.saturating_duration_since(*when) > SMOOTHED_OVER)
        {
            self.fullness.pop_front();
        }
    }

    /// From `sent_us` on the host's clock to `here_us` on this one, in
    /// microseconds from the first delay seen.
    fn delay(&mut self, here_us: u64, sent_us: u32) -> i64 {
        let raw = (here_us as u32).wrapping_sub(sent_us);
        let reference = *self.reference.get_or_insert(raw);
        i64::from(raw.wrapping_sub(reference) as i32)
    }

    /// A picture was lost at `at`.
    pub fn lost(&mut self, at: Instant) {
        self.lost.push_back(at);
    }

    /// The ceiling the road calls for at `now`, when it changed.
    pub fn decide(&mut self, now: Instant) -> Option<Ceiling> {
        while self
            .lost
            .front()
            .is_some_and(|when| now.saturating_duration_since(*when) > SECOND)
        {
            self.lost.pop_front();
        }
        let arrived_kbps = self.arrived_kbps(now)?;
        let smoothed = self.smoothed_fullness();
        let fullness = smoothed.unwrap_or(0.0);
        let full = self.overfull >= OVERFULL_PICTURES && fullness > 1.0;
        let lost = self.lost.len();
        let crowded = if full {
            Some(Because::Full { fullness })
        } else if lost >= LOST_IN_A_SECOND {
            Some(Because::Lost { pictures: lost })
        } else {
            None
        };
        if crowded.is_some() || fullness > CALM_FULLNESS || !self.lost.is_empty() {
            self.unsettled = Some(now);
        }
        let settled = self
            .last_change
            .is_none_or(|change| now.saturating_duration_since(change) >= SETTLE);
        if !settled {
            return None;
        }
        let current = self.ceiling.unwrap_or(self.wanted_kbps);
        if let Some(because) = crowded {
            // A full road arrives at what it bears; a lost picture says
            // nothing of how much passes.
            if full {
                self.bore = Some((arrived_kbps, now));
            }
            let fallen = (f64::from(arrived_kbps) * BELOW_WHAT_ARRIVED).clamp(
                f64::from(current) * MOST_FALL,
                f64::from(current) * LEAST_FALL,
            );
            let kbps = (fallen as u32).max(LOWEST_KBPS.min(self.wanted_kbps));
            self.last_change = Some(now);
            if kbps >= current {
                return None;
            }
            return Some(self.set(Some(kbps), arrived_kbps, because));
        }
        let ceiling = self.ceiling?;
        // A stream using its ceiling on a road this empty is far under
        // what the road bears now, whatever it bore before.
        if smoothed.is_some_and(|fullness| fullness < FREE_FULLNESS)
            && f64::from(arrived_kbps) >= f64::from(ceiling) * USED
        {
            self.bore = None;
        }
        // Near what the road last bore, however long ago, climbs are
        // small; under it, for as long as it is remembered, they stop.
        let far = self
            .bore
            .is_none_or(|(bore, _)| f64::from(ceiling) < f64::from(bore) * NEAR);
        let held = self
            .bore
            .filter(|(_, when)| now.saturating_duration_since(*when) < BORE_REMEMBERED)
            .map(|(bore, _)| bore);
        let calm_for = if far { CALM_FAR } else { CALM_NEAR };
        let calm_since = [self.unsettled, self.last_change]
            .into_iter()
            .flatten()
            .max()?;
        if now.saturating_duration_since(calm_since) < calm_for {
            return None;
        }
        let climb = if far { CLIMB_FAR } else { CLIMB_NEAR };
        let climbed = match held {
            Some(bore) => (f64::from(ceiling) * climb).min(f64::from(bore) * HELD_UNDER),
            None => f64::from(ceiling) * climb,
        } as u32;
        if climbed <= ceiling {
            return None;
        }
        self.last_change = Some(now);
        let kbps = (climbed < self.wanted_kbps).then_some(climbed);
        Some(self.set(kbps, arrived_kbps, Because::Calm))
    }

    /// How the road went since the last reading, at `now`; the counts
    /// start again.
    pub fn reading(&mut self, now: Instant) -> Reading {
        let percent = |fullness: f64| (fullness * 100.0).round() as u32;
        let reading = Reading {
            pictures: self.pictures,
            fullness: percent(self.fullness_sum / f64::from(self.pictures.max(1))),
            fullness_most: percent(self.fullness_most),
            arrived_kbps: self.arrived_kbps(now),
            ceiling: self.ceiling,
        };
        self.pictures = 0;
        self.fullness_sum = 0.0;
        self.fullness_most = 0.0;
        reading
    }

    fn set(&mut self, kbps: Option<u32>, arrived_kbps: u32, because: Because) -> Ceiling {
        self.ceiling = kbps;
        // What the road did under the old ceiling says nothing of the new.
        self.overfull = 0;
        self.fullness.clear();
        self.lost.clear();
        Ceiling {
            kbps,
            arrived_kbps,
            because,
        }
    }

    /// What arrived over the last moment, once a whole moment has.
    fn arrived_kbps(&self, now: Instant) -> Option<u32> {
        let first = self.first_arrival?;
        if now.saturating_duration_since(first) < RATE_OVER {
            return None;
        }
        let bytes: u64 = self
            .arrived
            .iter()
            .filter(|(bucket, _)| now.saturating_duration_since(*bucket) <= RATE_OVER)
            .map(|(_, bytes)| bytes)
            .sum();
        let kbps = bytes * 8 / RATE_OVER.as_millis() as u64;
        Some(u32::try_from(kbps).unwrap_or(u32::MAX))
    }

    /// How full the road was over the last moment, once a picture has
    /// said so since the last change.
    fn smoothed_fullness(&self) -> Option<f64> {
        if self.fullness.is_empty() {
            return None;
        }
        let sum: f64 = self.fullness.iter().map(|(_, full)| full).sum();
        Some(sum / self.fullness.len() as f64)
    }
}

/// How bytes arrived are grouped.
const BUCKET: Duration = Duration::from_millis(10);

const SECOND: Duration = Duration::from_secs(1);

fn interval_of(fps: u16) -> Duration {
    (Duration::from_secs(1) / u32::from(fps.max(1))).max(SHORTEST_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: u16 = 60;

    /// A simulated road: pictures leave the host every interval and
    /// cross a link of `kbps`, queueing behind each other when they come
    /// faster than it passes them, `base` after they left. The host's
    /// clock runs `offset` ahead of this one, wrapping.
    struct Simulated {
        road: Road,
        start: Instant,
        /// When the link is free again, from the start.
        free_at: Duration,
        sent: u32,
        base: Duration,
        offset: u32,
        /// Each change, and how many pictures had left by then.
        ceilings: Vec<(u32, Ceiling)>,
    }

    impl Simulated {
        fn new(wanted_kbps: u32) -> Self {
            Self {
                road: Road::new(wanted_kbps, FPS),
                start: Instant::now(),
                free_at: Duration::ZERO,
                sent: 0,
                base: Duration::from_millis(20),
                offset: u32::MAX - 3_000_000,
                ceilings: Vec::new(),
            }
        }

        fn rate(&self) -> u32 {
            self.road.ceiling().unwrap_or(self.road.wanted_kbps)
        }

        /// Sends one picture of what the rate in force allows, across a
        /// link of `link_kbps`, and lets the road decide.
        fn picture(&mut self, link_kbps: u32) {
            let left = interval_of(FPS) * self.sent;
            self.sent += 1;
            let bytes = u64::from(self.rate()) * 1000 / 8 / u64::from(FPS);
            let passing = Duration::from_micros(bytes * 8 * 1000 / u64::from(link_kbps));
            let starts = self.free_at.max(left + self.base);
            self.free_at = starts + passing;
            let arrived = self.start + self.free_at;
            let packets = bytes.div_ceil(1_200);
            for _ in 0..packets {
                self.road.datagram((bytes / packets) as usize, arrived);
            }
            self.road.whole(Arrival {
                sent_us: (left.as_micros() as u32).wrapping_add(self.offset),
                first_us: (starts + passing / packets as u32).as_micros() as u64,
                last_us: self.free_at.as_micros() as u64,
                at: arrived,
                key: false,
            });
            if let Some(ceiling) = self.road.decide(arrived) {
                self.ceilings.push((self.sent, ceiling));
            }
        }

        fn for_seconds(&mut self, seconds: u32, link_kbps: u32) {
            for _ in 0..seconds * u32::from(FPS) {
                self.picture(link_kbps);
            }
        }
    }

    #[test]
    fn a_road_that_bears_the_rate_chosen_is_left_alone() {
        let mut simulated = Simulated::new(20_000);
        simulated.for_seconds(30, 50_000);
        assert_eq!(simulated.ceilings, []);
        assert_eq!(simulated.road.ceiling(), None);
    }

    #[test]
    fn a_road_narrower_than_the_rate_brings_it_under_what_it_bears() {
        let mut simulated = Simulated::new(40_000);
        simulated.for_seconds(10, 20_000);
        let (_, first) = simulated.ceilings.first().expect("the ceiling fell");
        assert!(matches!(first.because, Because::Full { .. }), "{first:?}");
        let ceiling = simulated.road.ceiling().expect("a ceiling in force");
        assert!(
            (14_000..20_000).contains(&ceiling),
            "{ceiling} kb/s on a road of 20000"
        );
        // And it stays under the road, climbing only to fall again.
        let before = simulated.ceilings.len();
        simulated.for_seconds(30, 20_000);
        let ceiling = simulated.road.ceiling().expect("still a ceiling");
        assert!(ceiling < 20_000, "{ceiling}");
        assert!(
            simulated.ceilings.len() - before <= 3,
            "{} changes in thirty seconds",
            simulated.ceilings.len() - before
        );
    }

    #[test]
    fn the_ceiling_falls_within_a_fraction_of_a_second_and_again_while_too_high() {
        let mut simulated = Simulated::new(40_000);
        simulated.for_seconds(2, 100_000);
        assert_eq!(simulated.ceilings, []);
        let narrowed = simulated.sent;
        simulated.for_seconds(3, 10_000);
        let (when, fell) = simulated.ceilings[0];
        assert!(fell.kbps.unwrap() < 40_000);
        assert!(
            when - narrowed <= u32::from(FPS) / 4,
            "{} pictures before the first fall",
            when - narrowed
        );
        let (_, last) = simulated.ceilings.last().unwrap();
        assert!(
            last.kbps.unwrap() < 10_000,
            "it falls again while the road is still too narrow: {last:?}"
        );
    }

    #[test]
    fn once_the_road_widens_again_the_ceiling_goes_away() {
        let mut simulated = Simulated::new(30_000);
        simulated.for_seconds(5, 10_000);
        assert!(simulated.road.ceiling().is_some());
        simulated.for_seconds(60, 200_000);
        assert_eq!(simulated.road.ceiling(), None);
        assert_eq!(simulated.ceilings.last().unwrap().1.kbps, None);
        assert!(
            simulated
                .ceilings
                .iter()
                .any(|(_, ceiling)| ceiling.because == Because::Calm)
        );
    }

    #[test]
    fn a_large_picture_alone_is_not_a_full_road() {
        let mut simulated = Simulated::new(20_000);
        simulated.for_seconds(2, 40_000);
        // A key frame ten times the others.
        let left = interval_of(FPS) * simulated.sent;
        simulated.sent += 1;
        let passing = Duration::from_micros(10 * 20_000 * 1000 / u64::from(FPS) / 40);
        let starts = simulated.free_at.max(left + simulated.base);
        simulated.free_at = starts + passing;
        simulated.road.whole(Arrival {
            sent_us: (left.as_micros() as u32).wrapping_add(simulated.offset),
            first_us: starts.as_micros() as u64,
            last_us: simulated.free_at.as_micros() as u64,
            at: simulated.start + simulated.free_at,
            key: true,
        });
        simulated.for_seconds(2, 40_000);
        assert_eq!(simulated.ceilings, []);
    }

    #[test]
    fn pictures_lost_bring_the_ceiling_down() {
        let mut road = Road::new(20_000, FPS);
        let start = Instant::now();
        for n in 0..60u32 {
            let at = start + interval_of(FPS) * n;
            road.datagram(40_000, at);
            road.whole(Arrival {
                sent_us: n * 16_667,
                first_us: u64::from(n) * 16_667 + 9_000,
                last_us: u64::from(n) * 16_667 + 10_000,
                at,
                key: false,
            });
        }
        let at = start + Duration::from_secs(1);
        for _ in 0..LOST_IN_A_SECOND {
            road.lost(at);
        }
        let ceiling = road.decide(at).expect("the ceiling fell");
        assert_eq!(
            ceiling.because,
            Because::Lost {
                pictures: LOST_IN_A_SECOND
            }
        );
        assert!(ceiling.kbps.unwrap() < 20_000);
    }

    #[test]
    fn nothing_is_decided_before_a_whole_measure_has_arrived() {
        let mut road = Road::new(20_000, FPS);
        let start = Instant::now();
        road.datagram(1_000, start);
        for _ in 0..10 {
            road.lost(start);
        }
        assert_eq!(road.decide(start + RATE_OVER / 2), None);
    }

    #[test]
    fn the_person_choosing_a_rate_lifts_the_ceiling() {
        let mut simulated = Simulated::new(40_000);
        simulated.for_seconds(5, 10_000);
        assert!(simulated.road.ceiling().is_some());
        simulated.road.wanted(25_000);
        assert_eq!(simulated.road.ceiling(), None);
    }

    #[test]
    fn the_ceiling_never_goes_under_the_lowest_unless_the_person_asks_less() {
        let mut simulated = Simulated::new(20_000);
        simulated.for_seconds(20, 1_000);
        assert_eq!(simulated.road.ceiling(), Some(LOWEST_KBPS));

        let mut simulated = Simulated::new(2_000);
        simulated.for_seconds(20, 500);
        assert!(simulated.road.ceiling().is_none_or(|kbps| kbps >= 2_000));
    }

    #[test]
    fn the_clocks_wrapping_changes_nothing() {
        let mut simulated = Simulated::new(20_000);
        // The host's clock wraps a second and a half in.
        simulated.offset = u32::MAX - 1_500_000;
        simulated.for_seconds(10, 50_000);
        assert_eq!(simulated.ceilings, []);
    }

    #[test]
    fn a_reading_says_how_the_road_went_and_starts_again() {
        let mut simulated = Simulated::new(20_000);
        simulated.for_seconds(2, 40_000);
        let now = simulated.start + simulated.free_at;
        let reading = simulated.road.reading(now);
        assert_eq!(reading.pictures, 120);
        assert!((45..=55).contains(&reading.fullness), "{reading:?}");
        assert!(reading.fullness_most >= reading.fullness);
        let line = reading.to_string();
        assert!(line.starts_with("120 pictures, the road "), "{line}");
        assert!(line.ends_with("kb/s arrived, no ceiling"), "{line}");
        assert_eq!(simulated.road.reading(now).pictures, 0);
    }

    #[test]
    fn a_reason_reads_as_a_sentence() {
        assert_eq!(
            Because::Full { fullness: 1.234 }.to_string(),
            "pictures kept arriving slower than they left, the road 123 % full"
        );
        assert_eq!(
            Because::Lost { pictures: 4 }.to_string(),
            "4 pictures lost within a second"
        );
        assert_eq!(Because::Calm.to_string(), "the road stayed calm");
    }
}
