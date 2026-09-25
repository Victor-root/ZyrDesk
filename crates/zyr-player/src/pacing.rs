//! When each decoded picture goes on the screen.
//!
//! The screen shows a new picture only at its refreshes, sixty times a
//! second or whatever its rate. A picture presented the moment it is
//! decoded is shown at the first refresh the compositor can still make,
//! and pictures that come a few milliseconds early or late around that
//! moment land unevenly: two on one refresh, the first never seen, then
//! a refresh with nothing new that shows the one before again. Sixty
//! pictures a second still move by fits. The old player never did: it
//! was always run with Moonlight's own pacing.
//!
//! So each refresh gets one picture at most, in the order they came. The
//! time between two refreshes is a window, open from just after a
//! refresh until a little before the next one, as long before it as the
//! compositor needs to still show a picture presented then. A picture
//! decoded while the window of a refresh without a picture is open is
//! presented at once; any other waits for the next window to open. A
//! late picture then costs one refresh showing the one before again,
//! never a jumble, and the pictures after it follow one refresh apart.
//!
//! What waits adds a refresh of delay. Once every picture of the last two
//! seconds could have gone a refresh earlier, one is dropped to take that
//! refresh back: the delay stays where the network's own unevenness puts
//! it and goes no further.
//!
//! A picture the host sent again, its screen not having changed, shows
//! nothing new and never makes a newer one wait: waiting, it is left out
//! as soon as a newer picture comes; presented for a refresh, it gives
//! that refresh up to a newer picture that comes while its window is
//! still open. The host sends the first one a period and an eighth after
//! the last picture, late on the screen's rhythm more often than not:
//! keeping a refresh of its own, it would push every picture after it a
//! refresh later.
//!
//! The window closes 6 ms before the refresh, a little more than a
//! laptop's compositor was measured needing, and earlier when the screen
//! says so: a picture shown after the refresh it was presented for says
//! the window closed too late, and it closes a millisecond earlier than
//! the margin that picture missed with. Every twenty seconds without
//! another, it eases back towards the 6 ms by a quarter of a millisecond,
//! and never past them: a miss the system fails to report must not be
//! invited.
//!
//! The screen's rhythm comes from the refreshes the system times, one
//! with each present. A refresh lasts the middle of the lengths measured
//! across the last two seconds or so, and the newest refresh was when
//! the middle of the newest few, each carried to it, says: a refresh the
//! system misdates, as it did while the network was in trouble, moves
//! neither. The screen has changed its rate only when the newest steps
//! from one refresh to the next agree on another length.
//!
//! Until the screen has said when its refreshes are, a picture is
//! presented as soon as it is decoded, the newest replacing any not yet
//! presented.

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use zyr_media::trace::{Seconds, Spread, ms};
use zyr_proto::log::Log;

use crate::flow::TAG;
use crate::present::Displayed;

/// How long after a refresh the window for the next one opens: what the
/// compositor shows at the refresh just gone is settled by then.
const OPENING: Duration = Duration::from_micros(500);

/// How long before a refresh the window closes, unless the screen said
/// it must close earlier: a little more than the 3.3 to 5.7 ms a
/// laptop's compositor was measured needing.
const CUTOFF: Duration = Duration::from_millis(6);

/// How much earlier than the margin a picture missed its refresh with
/// the window closes from then on.
const CUTOFF_MARGIN: Duration = Duration::from_millis(1);

/// How long an earlier close holds before easing back towards
/// [`CUTOFF`], and by how much at a time.
const CUTOFF_HELD: Duration = Duration::from_secs(20);
const CUTOFF_EASED: Duration = Duration::from_micros(250);

/// How long every picture presented must have been early, and how many
/// at least, before one is dropped to take a refresh of delay back.
const CATCH_UP_AFTER: Duration = Duration::from_secs(2);
const CATCH_UP_LEAST: u32 = 30;

/// How long before the window of the refresh before closed a picture
/// must have been decoded to count as early.
const EARLY_BY: Duration = Duration::from_millis(1);

/// Pictures waiting at most. Past that, they piled up behind a stall,
/// and only the newest is kept: better shown at once than one by one.
const MOST_WAITING: usize = 2;

/// Presents remembered until the screen says what became of them.
const MOST_PENDING: usize = 16;

/// The refreshes the system timed that the screen's rhythm is drawn
/// from, the newest kept.
const TIMED_KEPT: usize = 128;

/// The refreshes those must span before a refresh's length is measured
/// from them rather than taken from the system.
const MEASURED_OVER: u64 = 60;

/// The newest refreshes timed that say, between them, when the newest
/// was: each carried to it, the middle one counts.
const PLACED_BY: usize = 9;

/// The newest steps from one refresh timed to the next whose middle says
/// whether the screen changed its rate, and how many at least.
const STEPS_SEEN: usize = 5;
const STEPS_LEAST: usize = 3;

/// How far the middle of those steps may be from the length known
/// before the screen is taken to have changed its rate: a fifth.
const CHANGED_RATE: u32 = 5;

/// A picture to present now.
pub(crate) struct Turn<T> {
    pub(crate) picture: T,
    /// The refresh it is meant for, once the screen's refreshes are
    /// known.
    pub(crate) refresh: Option<u64>,
    /// Pictures never to be shown because of it: dropped while waiting,
    /// or presented for the same refresh and replaced before it.
    pub(crate) unshown: u64,
}

/// Refreshes of the screen: one the system timed, and how long each
/// lasts.
#[derive(Debug, Clone, Copy)]
struct Refreshes {
    count: u64,
    at: Instant,
    period: Duration,
}

impl Refreshes {
    /// How long each refresh lasted between two the system timed.
    fn length(from: (u64, Instant), to: (u64, Instant)) -> Option<Duration> {
        let refreshes = u32::try_from(to.0.checked_sub(from.0)?)
            .ok()
            .filter(|refreshes| *refreshes > 0)?;
        Some(to.1.checked_duration_since(from.1)? / refreshes)
    }

    /// When refresh `count` is, before or after the one timed.
    fn when(&self, count: u64) -> Option<Instant> {
        let apart = |refreshes: u64| {
            u32::try_from(refreshes)
                .ok()
                .and_then(|refreshes| self.period.checked_mul(refreshes))
        };
        if count >= self.count {
            self.at.checked_add(apart(count - self.count)?)
        } else {
            self.at.checked_sub(apart(self.count - count)?)
        }
    }

    /// The last refresh at or before `at`.
    fn last_by(&self, at: Instant) -> u64 {
        let period = self.period.as_nanos().max(1);
        if at >= self.at {
            let refreshes = (at - self.at).as_nanos() / period;
            self.count
                .saturating_add(u64::try_from(refreshes).unwrap_or(u64::MAX))
        } else {
            let refreshes = (self.at - at).as_nanos().div_ceil(period);
            self.count
                .saturating_sub(u64::try_from(refreshes).unwrap_or(u64::MAX))
        }
    }
}

struct Waiting<T> {
    picture: T,
    /// When it was decoded.
    ready: Instant,
    /// Sent again by the host, its screen unchanged.
    repeat: bool,
}

/// A present the screen has not said anything about yet.
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// Its number among the surface's presents.
    id: u64,
    refresh: u64,
    at: Instant,
}

/// What one second of pacing did, for the journal.
#[derive(Default)]
struct Second {
    /// Presented before the screen's refreshes were known.
    unpaced: u32,
    /// Presented as soon as decoded, and held for the window of their
    /// refresh.
    at_once: u32,
    held: u32,
    /// How long presented pictures waited for their window.
    waited: Spread,
    /// Dropped because newer ones piled up behind them.
    piled_up: u64,
    /// Dropped, or replaced before they showed, to take a refresh back.
    caught_up: u32,
    /// Pictures sent again that gave way to a newer one, waiting or
    /// replaced before they showed.
    gave_way: u64,
    /// Refreshes from one picture presented to the next: one, two,
    /// three, more.
    apart: [u32; 4],
    /// What the screen said of the pictures presented for a refresh:
    /// shown at it, after it (and of those, how many had half a refresh
    /// to spare all the same), before it.
    on_time: u32,
    late: u32,
    late_anyway: u32,
    early: u32,
}

/// Decoded pictures, each held until the window of its refresh.
pub(crate) struct Pacer<T> {
    log: Log,
    refreshes: Option<Refreshes>,
    /// The refreshes the system timed, each once, oldest first.
    timed: VecDeque<(u64, Instant)>,
    waiting: VecDeque<Waiting<T>>,
    /// The refresh the last picture was presented for.
    served: Option<u64>,
    /// Whether that picture was sent again by the host.
    served_repeat: bool,
    /// When the last picture was presented.
    presented_at: Option<Instant>,
    /// How long before a refresh its window closes, as learnt; never
    /// more than two thirds of a refresh when used.
    cutoff: Duration,
    /// When the close last moved.
    cutoff_moved: Option<Instant>,
    pending: VecDeque<Pending>,
    /// Since when every picture presented could have gone a refresh
    /// earlier, and how many.
    early_since: Option<Instant>,
    early_run: u32,
    /// A picture is to be dropped to take a refresh back.
    catching_up: bool,
    seconds: Seconds,
    second: Second,
}

impl<T> Pacer<T> {
    pub(crate) fn new(log: &Log) -> Self {
        Self {
            log: log.about(TAG),
            refreshes: None,
            timed: VecDeque::with_capacity(TIMED_KEPT),
            waiting: VecDeque::new(),
            served: None,
            served_repeat: false,
            presented_at: None,
            cutoff: CUTOFF,
            cutoff_moved: None,
            pending: VecDeque::new(),
            early_since: None,
            early_run: 0,
            catching_up: false,
            seconds: Seconds::default(),
            second: Second::default(),
        }
    }

    /// A picture decoded at `ready`, a `repeat` if the host sent it again.
    /// Says how many waiting pictures were dropped for it: before the
    /// refreshes are known, only the newest waits.
    pub(crate) fn ready(&mut self, picture: T, ready: Instant, repeat: bool) -> u64 {
        self.seconds.started(ready);
        let before = self.waiting.len();
        self.waiting.retain(|waiting| !waiting.repeat);
        let gave_way = (before - self.waiting.len()) as u64;
        self.waiting.push_back(Waiting {
            picture,
            ready,
            repeat,
        });
        let most = if self.refreshes.is_some() {
            MOST_WAITING
        } else {
            1
        };
        let piled_up = if self.waiting.len() > most {
            self.waiting.len() - 1
        } else {
            0
        };
        self.waiting.drain(..piled_up);
        if self.refreshes.is_some() {
            self.second.gave_way += gave_way;
            self.second.piled_up += piled_up as u64;
        }
        gave_way + piled_up as u64
    }

    /// The picture to present at `now`, if one is due.
    pub(crate) fn due(&mut self, now: Instant) -> Option<Turn<T>> {
        let Some(refreshes) = self.refreshes else {
            let waiting = self.waiting.pop_front()?;
            self.second.unpaced += 1;
            self.served_repeat = waiting.repeat;
            return Some(Turn {
                picture: waiting.picture,
                refresh: None,
                unshown: 0,
            });
        };
        if self.waiting.is_empty() {
            return None;
        }
        let (refresh, opening) = self.turn(&refreshes, now)?;
        let again = self.served == Some(refresh);
        if now < opening
            || self.served.is_some_and(|served| served > refresh)
            || (again && !self.catching_up && !self.served_repeat)
        {
            return None;
        }
        let mut unshown = 0;
        if again {
            // The picture presented for this refresh is replaced before it
            // shows, on purpose: not a miss. Either way the refresh is
            // taken back.
            unshown = 1;
            self.pending.retain(|pending| pending.refresh != refresh);
            if self.served_repeat {
                self.second.gave_way += 1;
            } else {
                self.second.caught_up += 1;
            }
            self.catching_up = false;
        } else if self.catching_up && self.waiting.len() > 1 {
            self.waiting.pop_front();
            unshown = 1;
            self.second.caught_up += 1;
            self.catching_up = false;
        }
        let waiting = self.waiting.pop_front()?;
        let before_closed = refreshes
            .when(refresh.saturating_sub(1))
            .and_then(|before| before.checked_sub(self.cutoff_for(&refreshes) + EARLY_BY));
        self.early(
            before_closed.is_some_and(|closed| waiting.ready <= closed),
            now,
        );
        if let Some(served) = self.served
            && refresh > served
        {
            let apart = usize::try_from(refresh - served).unwrap_or(usize::MAX);
            self.second.apart[apart.min(4) - 1] += 1;
        }
        if waiting.ready < opening {
            self.second.held += 1;
        } else {
            self.second.at_once += 1;
        }
        self.second
            .waited
            .add(now.saturating_duration_since(waiting.ready));
        self.served = Some(refresh);
        self.served_repeat = waiting.repeat;
        self.seconds.started(now);
        Some(Turn {
            picture: waiting.picture,
            refresh: Some(refresh),
            unshown,
        })
    }

    /// When a waiting picture can next be presented, if one waits for
    /// its window; one presented as soon as decoded needs no wake-up.
    pub(crate) fn next_wakeup(&self, now: Instant) -> Option<Instant> {
        if self.waiting.is_empty() {
            return None;
        }
        let refreshes = self.refreshes?;
        let (refresh, opening) = self.turn(&refreshes, now)?;
        match self.served {
            Some(served) if served >= refresh => refreshes.when(served)?.checked_add(OPENING),
            _ => Some(opening),
        }
    }

    /// A picture was presented at `presented`, for `refresh` when it was
    /// paced, and `screen` is what the screen said right after.
    pub(crate) fn after(
        &mut self,
        refresh: Option<u64>,
        presented: Instant,
        screen: Option<&Displayed>,
    ) {
        self.presented_at = Some(presented);
        let Some(screen) = screen else {
            return;
        };
        if let Some(refresh) = refresh {
            if self.pending.len() == MOST_PENDING {
                self.pending.pop_front();
            }
            self.pending.push_back(Pending {
                id: screen.presented,
                refresh,
                at: presented,
            });
        }
        self.screen(screen, presented);
    }

    /// Lets go of the waiting pictures and of all the screen said: the
    /// surface is being made again.
    pub(crate) fn clear(&mut self) {
        self.waiting.clear();
        self.presented_at = None;
        self.forget_the_screen();
    }

    /// Writes the second out once it is over.
    pub(crate) fn look(&mut self, now: Instant) {
        if self.seconds.over(now) {
            self.write();
        }
    }

    /// The refresh a picture presented at `at` would be shown at, and
    /// when the window for that refresh opens.
    fn turn(&self, refreshes: &Refreshes, at: Instant) -> Option<(u64, Instant)> {
        let mut refresh = refreshes.last_by(at).checked_add(1)?;
        if at
            >= refreshes
                .when(refresh)?
                .checked_sub(self.cutoff_for(refreshes))?
        {
            refresh += 1;
        }
        let opening = refreshes.when(refresh - 1)?.checked_add(OPENING)?;
        Some((refresh, opening))
    }

    /// How long before a refresh its window closes: as learnt, but always
    /// leaving a third of a refresh open.
    fn cutoff_for(&self, refreshes: &Refreshes) -> Duration {
        self.cutoff.min(refreshes.period * 2 / 3)
    }

    /// Whether the picture just presented could have gone a refresh
    /// earlier: once all of them could for long enough, one is dropped.
    fn early(&mut self, early: bool, now: Instant) {
        if !early {
            self.early_since = None;
            self.early_run = 0;
            return;
        }
        let since = *self.early_since.get_or_insert(now);
        self.early_run += 1;
        if self.early_run >= CATCH_UP_LEAST
            && now.saturating_duration_since(since) >= CATCH_UP_AFTER
        {
            self.catching_up = true;
            self.early_since = None;
            self.early_run = 0;
        }
    }

    /// Everything the screen said: its refreshes are counted afresh.
    fn forget_the_screen(&mut self) {
        self.refreshes = None;
        self.timed.clear();
        self.served = None;
        self.served_repeat = false;
        self.pending.clear();
        self.early_since = None;
        self.early_run = 0;
        self.catching_up = false;
    }

    /// Takes in what the screen said: when its refreshes are, and what
    /// became of the pictures presented.
    fn screen(&mut self, screen: &Displayed, now: Instant) {
        // A count gone back is a count started again: nothing said before
        // compares with it.
        if self
            .timed
            .back()
            .is_some_and(|&(count, _)| screen.timed < count)
        {
            self.forget_the_screen();
        }
        if self
            .timed
            .back()
            .is_none_or(|&(count, _)| screen.timed > count)
        {
            if self.timed.len() == TIMED_KEPT {
                self.timed.pop_front();
            }
            self.timed.push_back((screen.timed, screen.timed_at));
        }
        let Some(refreshes) = self.rhythm(screen.refresh) else {
            return;
        };
        if self.refreshes.is_none() {
            // The picture presented last is on its refresh already.
            self.served = self
                .presented_at
                .and_then(|at| self.turn(&refreshes, at))
                .map(|(refresh, _)| refresh);
        }
        self.refreshes = Some(refreshes);
        self.resolve(screen, now);
        if self.cutoff > CUTOFF
            && self
                .cutoff_moved
                .is_some_and(|moved| now.saturating_duration_since(moved) >= CUTOFF_HELD)
        {
            self.cutoff = self.cutoff.saturating_sub(CUTOFF_EASED).max(CUTOFF);
            self.cutoff_moved = Some(now);
        }
    }

    /// The screen's refreshes as the ones timed say: how long each lasts,
    /// and when the newest was, each of the newest few carried to it and
    /// the middle one kept. A refresh the system misdated moves neither.
    fn rhythm(&mut self, reported: Duration) -> Option<Refreshes> {
        let period = self.period(reported);
        let &(count, _) = self.timed.back()?;
        if period.is_zero() {
            return None;
        }
        let mut placed: Vec<Instant> = self
            .timed
            .iter()
            .rev()
            .take(PLACED_BY)
            .filter_map(|&(timed, at)| {
                let apart = u32::try_from(count - timed).ok()?;
                at.checked_add(period.checked_mul(apart)?)
            })
            .collect();
        let at = middle(&mut placed)?;
        Some(Refreshes { count, at, period })
    }

    /// How long a refresh lasts: the middle of the lengths measured
    /// between refreshes timed half the ones kept apart, once those span
    /// [`MEASURED_OVER`] of them, and the system's word until then. The
    /// middle of the newest steps overrules both when a fifth away: the
    /// screen changed its rate, and what it timed before is let go of.
    fn period(&mut self, reported: Duration) -> Duration {
        let known = self.refreshes.map_or(reported, |known| known.period);
        let newest: Vec<(u64, Instant)> = self
            .timed
            .iter()
            .rev()
            .take(STEPS_SEEN + 1)
            .copied()
            .collect();
        let mut steps: Vec<Duration> = newest
            .windows(2)
            .filter_map(|pair| Refreshes::length(pair[1], pair[0]))
            .collect();
        if steps.len() >= STEPS_LEAST
            && let Some(step) = middle(&mut steps)
            && step.abs_diff(known) > known / CHANGED_RATE
        {
            self.timed.drain(..self.timed.len() - 1);
            return step;
        }
        let (Some(&first), Some(&last)) = (self.timed.front(), self.timed.back()) else {
            return known;
        };
        if last.0 - first.0 < MEASURED_OVER {
            return known;
        }
        let half = self.timed.len() / 2;
        let mut lengths: Vec<Duration> = (0..half)
            .filter_map(|index| Refreshes::length(self.timed[index], self.timed[index + half]))
            .collect();
        middle(&mut lengths).unwrap_or(known)
    }

    /// What the screen says of the presents waiting for its word.
    fn resolve(&mut self, screen: &Displayed, now: Instant) {
        while let Some(pending) = self.pending.front().copied() {
            match screen.shown.cmp(&pending.id) {
                Ordering::Equal => {
                    self.pending.pop_front();
                    match screen.at_refresh.cmp(&pending.refresh) {
                        Ordering::Equal => self.second.on_time += 1,
                        Ordering::Greater => self.missed(pending, now),
                        Ordering::Less => self.second.early += 1,
                    }
                }
                // Shown and followed between two looks, or replaced before
                // its refresh: the screen does not say which.
                Ordering::Greater => {
                    self.pending.pop_front();
                }
                // Its refresh came and went with an older picture on the
                // screen.
                Ordering::Less if screen.timed >= pending.refresh => {
                    self.pending.pop_front();
                    self.missed(pending, now);
                }
                Ordering::Less => break,
            }
        }
    }

    /// A picture did not make the refresh it was presented for.
    fn missed(&mut self, pending: Pending, now: Instant) {
        self.second.late += 1;
        let Some(due) = self
            .refreshes
            .and_then(|refreshes| refreshes.when(pending.refresh))
        else {
            return;
        };
        let margin = due.saturating_duration_since(pending.at);
        let period = self.refreshes.map_or(Duration::ZERO, |known| known.period);
        if margin >= period / 2 {
            // Late with half a refresh to spare: not the close but the
            // card or the system being slow. Nothing to learn from it.
            self.second.late_anyway += 1;
            return;
        }
        self.cutoff = self.cutoff.max(margin + CUTOFF_MARGIN);
        self.cutoff_moved = Some(now);
    }

    fn write(&mut self) {
        let second = std::mem::take(&mut self.second);
        let paced = second.at_once + second.held;
        if paced == 0 && second.unpaced == 0 {
            return;
        }
        let screen = match self.refreshes {
            Some(refreshes) => format!(
                "windows close {:.2} ms before a refresh, one every {:.3} ms",
                ms(self.cutoff_for(&refreshes)),
                ms(refreshes.period)
            ),
            None => "the screen's refreshes are not known".to_string(),
        };
        let [one, two, three, more] = second.apart;
        self.log.debug(|| {
            format!(
                "pacing: {paced} presented one a refresh ({} as soon as decoded, {} held for \
                 their refresh, waiting {} ms), {} before the refreshes were known; {} dropped as \
                 newer ones piled up, {} dropped to take a refresh back, {} sent again that gave \
                 way to a newer one; refreshes from one picture to the next: {one} one, {two} \
                 two, {three} three, {more} more; the screen showed {} at their refresh, {} later \
                 ({} of them presented with half a refresh to spare), {} sooner; {screen}",
                second.at_once,
                second.held,
                second.waited,
                second.unpaced,
                second.piled_up,
                second.caught_up,
                second.gave_way,
                second.on_time,
                second.late,
                second.late_anyway,
                second.early,
            )
        });
    }
}

/// The middle one of `values`, which it puts in order around it.
fn middle<V: Ord + Copy>(values: &mut [V]) -> Option<V> {
    if values.is_empty() {
        return None;
    }
    let half = values.len() / 2;
    Some(*values.select_nth_unstable(half).1)
}

/// What the last second gathered goes to the journal with the session,
/// however it ends.
impl<T> Drop for Pacer<T> {
    fn drop(&mut self) {
        self.write();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// Sixty refreshes a second, in nanoseconds.
    const SIXTY_HZ: u64 = 16_666_667;

    /// A compositor: a present made before `deadline` ahead of a refresh
    /// is shown from that refresh on, the newest winning.
    struct Compositor {
        start: Instant,
        period: Duration,
        /// From that refresh on, the screen refreshes this often instead.
        changed: Option<(u64, Duration)>,
        /// What the system says a refresh lasts.
        reported: Duration,
        /// Refreshes the system says came that many microseconds late, or
        /// early if fewer than nought.
        misdated: Vec<(u64, i64)>,
        deadline: Duration,
        /// Every present, and the picture it was.
        presents: Vec<(Instant, u32)>,
    }

    impl Compositor {
        fn new(start: Instant, period_ns: u64, deadline: Duration) -> Self {
            Self {
                start,
                period: Duration::from_nanos(period_ns),
                changed: None,
                reported: Duration::from_nanos(period_ns),
                misdated: Vec::new(),
                deadline,
                presents: Vec::new(),
            }
        }

        fn refresh(&self, count: u64) -> Instant {
            let after = |from: Instant, count: u64, period: Duration| {
                from + Duration::from_nanos(count * period.as_nanos() as u64)
            };
            match self.changed {
                Some((from, period)) if count > from => {
                    after(after(self.start, from, self.period), count - from, period)
                }
                _ => after(self.start, count, self.period),
            }
        }

        fn last_by(&self, at: Instant) -> u64 {
            match self.changed {
                Some((from, period)) if at >= self.refresh(from) => {
                    from + ((at - self.refresh(from)).as_nanos() / period.as_nanos()) as u64
                }
                _ => ((at - self.start).as_nanos() / self.period.as_nanos()) as u64,
            }
        }

        /// When the system says refresh `count` was.
        fn said_at(&self, count: u64) -> Instant {
            let off = self
                .misdated
                .iter()
                .find(|(misdated, _)| *misdated == count)
                .map_or(0, |(_, micros)| *micros);
            let shift = Duration::from_micros(off.unsigned_abs());
            if off < 0 {
                self.refresh(count) - shift
            } else {
                self.refresh(count) + shift
            }
        }

        /// The first refresh a present made at `at` can be shown at.
        fn target(&self, at: Instant) -> u64 {
            let mut refresh = self.last_by(at) + 1;
            while self.refresh(refresh) - self.deadline <= at {
                refresh += 1;
            }
            refresh
        }

        /// What the system says at `now`, as Windows does: the last
        /// present shown by the last refresh, and when that was.
        fn displayed(&self, now: Instant) -> Option<Displayed> {
            let timed = self.last_by(now);
            let (index, &(at, _)) = self
                .presents
                .iter()
                .enumerate()
                .rev()
                .find(|(_, (at, _))| self.target(*at) <= timed)?;
            let at_refresh = self.target(at);
            Some(Displayed {
                presented: self.presents.len() as u64,
                shown: index as u64 + 1,
                at_refresh,
                at: self.refresh(at_refresh),
                timed,
                timed_at: self.said_at(timed),
                refresh: self.reported,
                compositor_missed: 0,
                compositor_dropped: 0,
            })
        }

        /// The picture each refresh from `first` to `last` showed.
        fn shown(&self, first: u64, last: u64) -> Vec<Option<u32>> {
            (first..=last)
                .map(|refresh| {
                    self.presents
                        .iter()
                        .rev()
                        .find(|(at, _)| self.target(*at) <= refresh)
                        .map(|&(_, picture)| picture)
                })
                .collect()
        }
    }

    /// A player's video thread, woken `late` after it asks.
    struct Thread {
        pacer: Pacer<u32>,
        compositor: Compositor,
        late: Duration,
        /// Every picture presented, for the refresh it was meant for.
        presented: Vec<(u32, Option<u64>)>,
        unshown: u64,
        /// The pictures the host sent again, by number.
        repeats: Vec<usize>,
    }

    impl Thread {
        fn new(compositor: Compositor, late: Duration, log: &Log) -> Self {
            Self {
                pacer: Pacer::new(log),
                compositor,
                late,
                presented: Vec::new(),
                unshown: 0,
                repeats: Vec::new(),
            }
        }

        /// Pictures decoded at those times, numbered from nought, and the
        /// thread doing what it does until `until`.
        fn run(&mut self, decoded: &[Instant], until: Instant) {
            let mut next = 0;
            let mut now = decoded[0];
            while now <= until {
                while next < decoded.len() && decoded[next] <= now {
                    let repeat = self.repeats.contains(&next);
                    self.unshown += self.pacer.ready(next as u32, decoded[next], repeat);
                    next += 1;
                }
                if let Some(turn) = self.pacer.due(now) {
                    self.unshown += turn.unshown;
                    self.compositor.presents.push((now, turn.picture));
                    self.presented.push((turn.picture, turn.refresh));
                    let screen = self.compositor.displayed(now);
                    self.pacer.after(turn.refresh, now, screen.as_ref());
                    continue;
                }
                let woken = self.pacer.next_wakeup(now).map(|at| at + self.late);
                now = [decoded.get(next).copied(), woken]
                    .into_iter()
                    .flatten()
                    .min()
                    .map_or(until + Duration::from_nanos(1), |at| {
                        at.max(now + Duration::from_micros(1))
                    });
            }
        }

        /// Per refresh from `first` to `last`, how far the picture moved
        /// on from the refresh before: 1 smooth, 0 a picture shown again,
        /// 2 or more pictures never seen.
        fn steps(&self, first: u64, last: u64) -> Vec<u32> {
            self.compositor
                .shown(first - 1, last)
                .windows(2)
                .map(|pair| match pair {
                    [Some(before), Some(now)] => now - before,
                    _ => 0,
                })
                .collect()
        }
    }

    /// Numbers that look random enough and are always the same.
    struct Noise(u64);

    impl Noise {
        /// Between `-spread` and `spread`.
        fn next(&mut self, spread: Duration) -> Duration {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            Duration::from_nanos((self.0 >> 33) % (2 * spread.as_nanos() as u64 + 1))
        }
    }

    /// A picture every `period_ns`, decoded `phase` after refresh `from`
    /// give or take `spread`, until refresh `to`.
    fn arrivals(
        start: Instant,
        period_ns: u64,
        phase: Duration,
        spread: Duration,
        pictures: u64,
    ) -> Vec<Instant> {
        let mut noise = Noise(7);
        (0..pictures)
            .map(|n| {
                start + Duration::from_nanos(period_ns * (n + 1)) + phase - spread
                    + noise.next(spread)
            })
            .collect()
    }

    fn counted(steps: &[u32], step: u32) -> usize {
        steps.iter().filter(|&&one| one == step).count()
    }

    #[test]
    fn pictures_around_the_compositors_moment_are_shown_one_a_refresh() {
        let journal = testing::OwnLog::new("pacing-around");
        let start = Instant::now();
        let deadline = Duration::from_micros(4_500);
        // Decoded 4.5 ms before a refresh, give or take 3 ms: right on the
        // compositor's moment, as the laptop measured.
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_nanos(SIXTY_HZ) - deadline,
            Duration::from_millis(3),
            600,
        );
        let until = start + Duration::from_secs(10) + Duration::from_millis(30);

        // Presented as soon as decoded, as before: a jumble.
        let mut at_once = Compositor::new(start, SIXTY_HZ, deadline);
        for (n, at) in decoded.iter().enumerate() {
            at_once.presents.push((*at, n as u32));
        }
        let before: Vec<u32> = at_once
            .shown(59, 600)
            .windows(2)
            .map(|pair| pair[1].unwrap() - pair[0].unwrap())
            .collect();
        assert!(counted(&before, 0) > 100, "{before:?}");

        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, deadline),
            Duration::from_millis(1),
            &journal.log,
        );
        thread.run(&decoded, until);
        // The first second finds the screen and its rhythm; then one
        // picture a refresh.
        let steps = thread.steps(60, 598);
        assert!(counted(&steps, 1) >= steps.len() - 2, "{steps:?}");
        assert!(counted(&steps, 0) <= 1, "{steps:?}");
    }

    #[test]
    fn pictures_inside_the_window_are_presented_as_soon_as_decoded() {
        let journal = testing::OwnLog::new("pacing-inside");
        let start = Instant::now();
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(8),
            Duration::from_millis(2),
            300,
        );
        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, Duration::from_millis(3)),
            Duration::from_millis(1),
            &journal.log,
        );
        thread.run(
            &decoded,
            start + Duration::from_secs(5) + Duration::from_millis(30),
        );
        let steps = thread.steps(10, 298);
        assert!(steps.iter().all(|&step| step == 1), "{steps:?}");
        // Each shown at the first refresh after it was decoded: picture n
        // comes 8 ms after refresh n + 1.
        let shown = thread.compositor.shown(10, 298);
        for (refresh, picture) in (10..).zip(shown) {
            assert_eq!(picture, Some(refresh as u32 - 2));
        }
        assert_eq!(thread.unshown, 0);
    }

    #[test]
    fn a_refresh_of_delay_is_taken_back_once_every_picture_could_go_earlier() {
        let journal = testing::OwnLog::new("pacing-catch-up");
        let start = Instant::now();
        let mut decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(8),
            Duration::from_millis(1),
            400,
        );
        // One picture held up on the way, the next right behind it: the
        // ones after it wait a refresh each.
        decoded[60] += Duration::from_millis(14);
        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, Duration::from_millis(3)),
            Duration::from_millis(1),
            &journal.log,
        );
        thread.run(&decoded, start + Duration::from_secs(7));
        let shown = thread.compositor.shown(0, 400);
        // Picture n comes 8 ms after refresh n + 1 and is shown at the
        // next one; after the late one, each waits a refresh more.
        assert_eq!(shown[50], Some(48));
        assert_eq!(shown[80], Some(77));
        // Two seconds on, the refresh is taken back, and stays so.
        assert_eq!(shown[250], Some(248));
        assert_eq!(shown[390], Some(388));
        // The late picture's refresh shows the one before again, and the
        // refresh taken back drops one: nothing else is uneven.
        let steps = thread.steps(20, 395);
        assert_eq!(counted(&steps, 0), 1, "{steps:?}");
        assert_eq!(counted(&steps, 2), 1, "{steps:?}");
    }

    #[test]
    fn the_window_closes_earlier_once_a_picture_misses_its_refresh() {
        let journal = testing::OwnLog::new("pacing-cutoff");
        let start = Instant::now();
        // This compositor needs 8 ms; pictures come 7 ms before a refresh.
        let deadline = Duration::from_millis(8);
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_nanos(SIXTY_HZ) - Duration::from_millis(7),
            Duration::from_micros(500),
            300,
        );
        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, deadline),
            Duration::from_millis(1),
            &journal.log,
        );
        thread.run(
            &decoded,
            start + Duration::from_secs(5) + Duration::from_millis(30),
        );
        assert!(thread.pacer.cutoff > deadline, "{:?}", thread.pacer.cutoff);
        let steps = thread.steps(60, 298);
        assert!(steps.iter().all(|&step| step == 1), "{steps:?}");
    }

    #[test]
    fn a_faster_screen_shows_every_picture_and_each_on_its_own_refresh() {
        let journal = testing::OwnLog::new("pacing-faster");
        let start = Instant::now();
        // A 144 Hz screen, whose rate the system misreports as 60 Hz.
        let period = 6_944_444;
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(3),
            Duration::from_millis(2),
            300,
        );
        let mut compositor = Compositor::new(start, period, Duration::from_millis(2));
        compositor.reported = Duration::from_nanos(SIXTY_HZ);
        let mut thread = Thread::new(compositor, Duration::from_millis(1), &journal.log);
        thread.run(
            &decoded,
            start + Duration::from_secs(5) + Duration::from_millis(30),
        );
        thread.pacer.look(start + Duration::from_secs(7));
        let written = journal.written();
        assert!(written.contains("one every 6.944 ms"), "{written}");
        assert_eq!(thread.unshown, 0);
        let mut meant: Vec<u64> = thread
            .presented
            .iter()
            .filter_map(|(_, refresh)| *refresh)
            .collect();
        let presents = meant.len();
        meant.dedup();
        assert_eq!(meant.len(), presents, "two pictures meant for one refresh");
        let shown: Vec<u32> = thread
            .compositor
            .shown(200, 700)
            .into_iter()
            .flatten()
            .collect();
        assert!(
            shown.windows(2).all(|pair| pair[1] <= pair[0] + 1),
            "{shown:?}"
        );
    }

    #[test]
    fn a_screen_a_little_off_the_hosts_rate_slips_a_picture_now_and_then_and_only_that() {
        let journal = testing::OwnLog::new("pacing-drift");
        // At 59.95 Hz a picture too many comes every twenty seconds, and
        // one is dropped; at 60.05 Hz one is missing as often, and a
        // refresh shows the picture before again. Nothing else changes.
        for (period, dropped, again) in [(16_680_567, 1, 0), (16_652_793, 0, 1)] {
            let start = Instant::now();
            let decoded = arrivals(
                start,
                SIXTY_HZ,
                Duration::from_millis(8),
                Duration::from_millis(3),
                1_800,
            );
            let mut thread = Thread::new(
                Compositor::new(start, period, Duration::from_micros(4_500)),
                Duration::from_millis(1),
                &journal.log,
            );
            thread.run(
                &decoded,
                start + Duration::from_secs(30) + Duration::from_millis(50),
            );
            let steps = thread.steps(60, 1_780);
            assert!(
                (dropped..=dropped * 2).contains(&counted(&steps, 2)),
                "{period}: {steps:?}"
            );
            assert!(
                (again..=again * 2).contains(&counted(&steps, 0)),
                "{period}: {steps:?}"
            );
            assert_eq!(
                counted(&steps, 1) + counted(&steps, 2) + counted(&steps, 0),
                steps.len()
            );
        }
    }

    #[test]
    fn a_screen_that_changes_its_rate_is_followed_at_once() {
        let journal = testing::OwnLog::new("pacing-rate");
        let start = Instant::now();
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(3),
            Duration::from_millis(2),
            300,
        );
        // Sixty refreshes a second for two seconds, then 144.
        let mut compositor = Compositor::new(start, SIXTY_HZ, Duration::from_millis(2));
        compositor.changed = Some((120, Duration::from_nanos(6_944_444)));
        let mut thread = Thread::new(compositor, Duration::from_millis(1), &journal.log);
        thread.run(
            &decoded,
            start + Duration::from_secs(5) + Duration::from_millis(30),
        );
        thread.pacer.look(start + Duration::from_secs(7));
        let written = journal.written();
        assert!(written.contains("one every 6.944 ms"), "{written}");
        let mut meant: Vec<u64> = thread
            .presented
            .iter()
            .filter_map(|(_, refresh)| *refresh)
            .collect();
        let presents = meant.len();
        meant.dedup();
        assert_eq!(meant.len(), presents, "two pictures meant for one refresh");
        let shown: Vec<u32> = thread
            .compositor
            .shown(150, 700)
            .into_iter()
            .flatten()
            .collect();
        assert!(
            shown.windows(2).all(|pair| pair[1] <= pair[0] + 1),
            "{shown:?}"
        );
    }

    #[test]
    fn a_misdated_refresh_moves_neither_the_rhythm_nor_the_windows() {
        let journal = testing::OwnLog::new("pacing-misdated");
        let start = Instant::now();
        let mut compositor = Compositor::new(start, SIXTY_HZ, Duration::from_millis(3));
        // Now and then a refresh said 7 ms late, and once two in a row said
        // 9 ms early, as while the network was in trouble.
        compositor.misdated = (100..580).step_by(13).map(|count| (count, 7_000)).collect();
        compositor.misdated.extend([(300, -9_000), (301, -9_000)]);
        let mut thread = Thread::new(compositor, Duration::from_millis(1), &journal.log);
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(8),
            Duration::from_millis(2),
            600,
        );
        thread.run(
            &decoded,
            start + Duration::from_secs(10) + Duration::from_millis(30),
        );
        let steps = thread.steps(60, 598);
        assert!(steps.iter().all(|&step| step == 1), "{steps:?}");
        let period = thread.pacer.refreshes.map(|known| known.period);
        assert!(
            period.is_some_and(|period| period.abs_diff(Duration::from_nanos(SIXTY_HZ))
                < Duration::from_micros(20)),
            "{period:?}"
        );
    }

    /// What the screen says when refresh `timed` was at `at`, the last
    /// present shown then.
    fn said(presented: u64, timed: u64, at: Instant) -> Displayed {
        Displayed {
            presented,
            shown: presented,
            at_refresh: timed,
            at,
            timed,
            timed_at: at,
            refresh: Duration::from_nanos(SIXTY_HZ),
            compositor_missed: 0,
            compositor_dropped: 0,
        }
    }

    #[test]
    fn a_count_started_again_keeps_the_pictures_waiting() {
        let journal = testing::OwnLog::new("pacing-count");
        let mut pacer = Pacer::new(&journal.log);
        let at = Instant::now();
        pacer.after(None, at, Some(&said(1, 1_000, at)));
        assert!(pacer.refreshes.is_some());
        assert_eq!(pacer.ready(7u32, at + Duration::from_millis(1), false), 0);
        let later = at + Duration::from_millis(2);
        pacer.after(None, later, Some(&said(2, 5, later)));
        assert_eq!(pacer.refreshes.map(|known| known.count), Some(5));
        assert_eq!(pacer.waiting.len(), 1);
    }

    #[test]
    fn an_earlier_close_eases_back_to_six_milliseconds_and_no_further() {
        let journal = testing::OwnLog::new("pacing-ease");
        let mut pacer = Pacer::<u32>::new(&journal.log);
        let at = Instant::now();
        pacer.cutoff = Duration::from_millis(7);
        pacer.cutoff_moved = Some(at);
        let mut eased = Vec::new();
        for step in 1..=6u32 {
            let now = at + Duration::from_secs(20) * step;
            pacer.after(
                None,
                now,
                Some(&said(u64::from(step), 1_200 * u64::from(step), now)),
            );
            eased.push(pacer.cutoff.as_micros());
        }
        assert_eq!(eased, [6_750, 6_500, 6_250, 6_000, 6_000, 6_000]);
    }

    #[test]
    fn until_the_refreshes_are_known_the_newest_picture_goes_at_once() {
        let journal = testing::OwnLog::new("pacing-unknown");
        let mut pacer = Pacer::new(&journal.log);
        let now = Instant::now();
        assert_eq!(pacer.ready(1u32, now, false), 0);
        assert_eq!(pacer.ready(2, now, false), 1);
        assert_eq!(pacer.next_wakeup(now), None);
        let turn = pacer.due(now).unwrap();
        assert_eq!((turn.picture, turn.refresh, turn.unshown), (2, None, 0));
        assert!(pacer.due(now).is_none());
    }

    #[test]
    fn pictures_piled_up_behind_a_stall_are_shown_from_the_newest() {
        let journal = testing::OwnLog::new("pacing-stall");
        let start = Instant::now();
        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, Duration::from_millis(3)),
            Duration::from_millis(1),
            &journal.log,
        );
        let mut decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(8),
            Duration::ZERO,
            200,
        );
        // Six pictures held up 90 ms, then all at once.
        let stalled = decoded[106];
        for at in &mut decoded[100..106] {
            *at = stalled;
        }
        thread.run(&decoded, start + Duration::from_secs(4));
        let presented: Vec<u32> = thread
            .presented
            .iter()
            .map(|(picture, _)| *picture)
            .collect();
        let after = presented
            .iter()
            .position(|&picture| picture >= 100)
            .unwrap();
        assert_eq!(presented[after], 106, "{presented:?}");
    }

    #[test]
    fn a_picture_sent_again_late_on_the_rhythm_gives_its_refresh_to_the_next() {
        let journal = testing::OwnLog::new("pacing-repeat");
        let start = Instant::now();
        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, Duration::from_millis(3)),
            Duration::from_millis(1),
            &journal.log,
        );
        // Decoded 10 ms after a refresh, just inside its window. The host
        // had nothing new for picture 100 and sent the one before again, a
        // period and an eighth after it: 2 ms past the window's close.
        let mut decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(10),
            Duration::ZERO,
            300,
        );
        decoded[100] += Duration::from_nanos(SIXTY_HZ / 8);
        thread.repeats = vec![100];
        thread.run(&decoded, start + Duration::from_secs(5));
        thread.pacer.look(start + Duration::from_secs(7));
        let shown = thread.compositor.shown(0, 300);
        // Picture n is shown at refresh n + 2. The one sent again missed
        // the window of refresh 102, which shows picture 99 again as it
        // would anyway, and was presented for refresh 103; picture 101
        // took that refresh over, and nothing after comes a refresh late.
        assert_eq!(shown[101], Some(99));
        assert_eq!(shown[102], Some(99));
        assert_eq!(shown[103], Some(101));
        assert_eq!(shown[250], Some(248));
        assert_eq!(thread.unshown, 1);
        assert!(
            journal
                .written()
                .contains("1 sent again that gave way to a newer one"),
            "{}",
            journal.written()
        );
    }

    #[test]
    fn a_picture_sent_again_and_waiting_is_left_out_for_a_newer_one() {
        let journal = testing::OwnLog::new("pacing-repeat-waiting");
        let mut pacer = Pacer::new(&journal.log);
        let at = Instant::now();
        pacer.after(None, at, Some(&said(1, 1_000, at)));
        // Both decoded after the window of refresh 1001 closed.
        let ms = Duration::from_millis;
        assert_eq!(pacer.ready(1u32, at + ms(12), true), 0);
        assert!(pacer.due(at + ms(12)).is_none());
        assert_eq!(pacer.ready(2, at + ms(14), false), 1);
        let turn = pacer.due(at + ms(18)).unwrap();
        assert_eq!((turn.picture, turn.refresh), (2, Some(1_002)));
        assert!(pacer.waiting.is_empty());
    }

    #[test]
    fn a_second_of_pacing_is_written_down() {
        let journal = testing::OwnLog::new("pacing-journal");
        let start = Instant::now();
        let decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(8),
            Duration::ZERO,
            90,
        );
        {
            let mut thread = Thread::new(
                Compositor::new(start, SIXTY_HZ, Duration::from_millis(3)),
                Duration::ZERO,
                &journal.log,
            );
            thread.run(&decoded, start + Duration::from_millis(1_550));
            thread.pacer.look(start + Duration::from_secs(3));
        }
        let written = journal.written();
        assert!(written.contains("pacing: "), "{written}");
        assert!(
            written.contains("windows close 6.00 ms before a refresh, one every 16.667 ms"),
            "{written}"
        );
    }
}
