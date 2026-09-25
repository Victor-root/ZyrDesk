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
//! it and goes no further. Pictures the network held up and then let
//! through together, decoded within a quarter of a refresh of one
//! another, never wait one behind the other: each would keep a refresh
//! of its own, and the delay would stay until the next catch-up. The
//! newest takes the place of the one before.
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
//! A refresh is known by when it is, never by the number the system
//! gives it. A screen left idle between two pictures can pass over a
//! refresh in its count, as the laptop's did after every stall: counted,
//! each refresh after it would be taken for the one before, one would go
//! by with the picture before shown again, and every picture after would
//! come a refresh late. So the refreshes between two the system timed
//! are counted from the time between them, and when its count and its
//! clock disagree, what it says of the pictures shown by then is not
//! judged.
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

/// Two pictures decoded less than this part of a refresh apart were held
/// up together and let through at once: the newer takes the older's
/// place while both wait.
const BURST: u32 = 4;

/// Presents remembered until the screen says what became of them.
const MOST_PENDING: usize = 16;

/// The refreshes the system timed that the screen's rhythm is drawn
/// from, the newest kept.
const TIMED_KEPT: usize = 128;

/// The refreshes those must span before a refresh's length is measured
/// from them rather than taken from the system.
const MEASURED_OVER: u32 = 60;

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
    /// When the refresh it is meant for is, once the screen's refreshes
    /// are known.
    pub(crate) due: Option<Instant>,
    /// Pictures never to be shown because of it: dropped while waiting,
    /// or presented for the same refresh and replaced before it.
    pub(crate) unshown: u64,
}

/// Refreshes of the screen: when one was, and how long each lasts.
#[derive(Debug, Clone, Copy)]
struct Refreshes {
    at: Instant,
    period: Duration,
}

impl Refreshes {
    /// When the last refresh at or before `at` was.
    fn last_by(&self, at: Instant) -> Option<Instant> {
        let period = self.period.as_nanos().max(1);
        let refreshes = |refreshes: u128| {
            u32::try_from(refreshes)
                .ok()
                .and_then(|refreshes| self.period.checked_mul(refreshes))
        };
        if at >= self.at {
            self.at
                .checked_add(refreshes((at - self.at).as_nanos() / period)?)
        } else {
            self.at
                .checked_sub(refreshes((self.at - at).as_nanos().div_ceil(period))?)
        }
    }

    /// How many refreshes from `from` on to `to`.
    fn apart(&self, from: Instant, to: Instant) -> Option<u32> {
        counted(to.checked_duration_since(from)?, self.period)
    }

    /// Whether `one` and `other` are the same refresh: less than half a
    /// refresh apart.
    fn same(&self, one: Instant, other: Instant) -> bool {
        one.max(other) - one.min(other) < self.period / 2
    }

    /// Whether `one` is a refresh after `other`.
    fn later(&self, one: Instant, other: Instant) -> bool {
        one > other && !self.same(one, other)
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
    /// When the refresh it was presented for is.
    due: Instant,
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
    /// to spare all the same), before it, or by a refresh its count and
    /// its clock disagreed on, which leaves when they were shown unknown.
    on_time: u32,
    late: u32,
    late_anyway: u32,
    early: u32,
    unjudged: u32,
}

/// Decoded pictures, each held until the window of its refresh.
pub(crate) struct Pacer<T> {
    log: Log,
    refreshes: Option<Refreshes>,
    /// The refreshes the system timed, each once, oldest first, with the
    /// number it gave them.
    timed: VecDeque<(u64, Instant)>,
    /// The number the system gave the refresh it timed when its count
    /// and its clock last disagreed: what it says of a picture shown by
    /// then can be a refresh off.
    disagreed: Option<u64>,
    waiting: VecDeque<Waiting<T>>,
    /// When the refresh the last picture was presented for is.
    served: Option<Instant>,
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
            disagreed: None,
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
        let burst = self.refreshes.is_some_and(|refreshes| {
            self.waiting.back().is_some_and(|newest| {
                ready.saturating_duration_since(newest.ready) < refreshes.period / BURST
            })
        });
        if burst {
            self.waiting.pop_back();
        }
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
        let piled_up = piled_up as u64 + u64::from(burst);
        if self.refreshes.is_some() {
            self.second.gave_way += gave_way;
            self.second.piled_up += piled_up;
        }
        gave_way + piled_up
    }

    /// The picture to present at `now`, if one is due.
    pub(crate) fn due(&mut self, now: Instant) -> Option<Turn<T>> {
        let Some(refreshes) = self.refreshes else {
            let waiting = self.waiting.pop_front()?;
            self.second.unpaced += 1;
            self.served_repeat = waiting.repeat;
            return Some(Turn {
                picture: waiting.picture,
                due: None,
                unshown: 0,
            });
        };
        if self.waiting.is_empty() {
            return None;
        }
        let (due, opening) = self.turn(&refreshes, now)?;
        let again = self
            .served
            .is_some_and(|served| refreshes.same(served, due));
        if now < opening
            || self
                .served
                .is_some_and(|served| refreshes.later(served, due))
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
            self.pending
                .retain(|pending| !refreshes.same(pending.due, due));
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
        let before_closed =
            due.checked_sub(refreshes.period + self.cutoff_for(&refreshes) + EARLY_BY);
        self.early(
            before_closed.is_some_and(|closed| waiting.ready <= closed),
            now,
        );
        if let Some(served) = self.served
            && refreshes.later(due, served)
        {
            let apart = refreshes
                .apart(served, due)
                .map_or(4, |apart| apart as usize);
            self.second.apart[apart.clamp(1, 4) - 1] += 1;
        }
        if waiting.ready < opening {
            self.second.held += 1;
        } else {
            self.second.at_once += 1;
        }
        self.second
            .waited
            .add(now.saturating_duration_since(waiting.ready));
        self.served = Some(due);
        self.served_repeat = waiting.repeat;
        self.seconds.started(now);
        Some(Turn {
            picture: waiting.picture,
            due: Some(due),
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
        let (due, opening) = self.turn(&refreshes, now)?;
        match self.served {
            Some(served) if !refreshes.later(due, served) => served.checked_add(OPENING),
            _ => Some(opening),
        }
    }

    /// A picture was presented at `presented`, for the refresh `due` then
    /// when it was paced, and `screen` is what the screen said right
    /// after.
    pub(crate) fn after(
        &mut self,
        due: Option<Instant>,
        presented: Instant,
        screen: Option<&Displayed>,
    ) {
        self.presented_at = Some(presented);
        let Some(screen) = screen else {
            return;
        };
        if let Some(due) = due {
            if self.pending.len() == MOST_PENDING {
                self.pending.pop_front();
            }
            self.pending.push_back(Pending {
                id: screen.presented,
                due,
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

    /// When the refresh a picture presented at `at` would be shown at is,
    /// and when the window for that refresh opens.
    fn turn(&self, refreshes: &Refreshes, at: Instant) -> Option<(Instant, Instant)> {
        let mut due = refreshes.last_by(at)?.checked_add(refreshes.period)?;
        if at >= due.checked_sub(self.cutoff_for(refreshes))? {
            due = due.checked_add(refreshes.period)?;
        }
        let opening = due.checked_sub(refreshes.period)?.checked_add(OPENING)?;
        Some((due, opening))
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
        self.disagreed = None;
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
        let newest = (screen.timed, screen.timed_at);
        if self
            .timed
            .back()
            .is_none_or(|&(count, _)| screen.timed > count)
        {
            let period = self.refreshes.map_or(screen.refresh, |known| known.period);
            if self
                .timed
                .back()
                .is_some_and(|&last| disagree(last, newest, period))
            {
                self.disagreed = Some(screen.timed);
            }
            if self.timed.len() == TIMED_KEPT {
                self.timed.pop_front();
            }
            self.timed.push_back(newest);
        }
        let Some(refreshes) = self.rhythm(screen.refresh) else {
            return;
        };
        if self.refreshes.is_none() {
            // The picture presented last is on its refresh already.
            self.served = self
                .presented_at
                .and_then(|at| self.turn(&refreshes, at))
                .map(|(due, _)| due);
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
        let &(_, newest) = self.timed.back()?;
        if period.is_zero() {
            return None;
        }
        let mut placed: Vec<Instant> = self
            .timed
            .iter()
            .rev()
            .take(PLACED_BY)
            .filter_map(|&(_, at)| {
                let apart = counted(newest.checked_duration_since(at)?, period)?;
                at.checked_add(period.checked_mul(apart)?)
            })
            .collect();
        let at = middle(&mut placed)?;
        Some(Refreshes { at, period })
    }

    /// How long a refresh lasts: the middle of the lengths measured
    /// between refreshes timed half the ones kept apart, once those span
    /// [`MEASURED_OVER`] of them, and the system's word until then. The
    /// middle of the newest steps, by the system's own count, overrules
    /// both when a fifth away: the screen changed its rate, and what it
    /// timed before is let go of.
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
            .filter_map(|pair| length(pair[1], pair[0]))
            .collect();
        if steps.len() >= STEPS_LEAST
            && let Some(step) = middle(&mut steps)
            && step.abs_diff(known) > known / CHANGED_RATE
        {
            self.timed.drain(..self.timed.len() - 1);
            return step;
        }
        let (Some(&(_, first)), Some(&(_, last))) = (self.timed.front(), self.timed.back()) else {
            return known;
        };
        if last.saturating_duration_since(first) < known * MEASURED_OVER {
            return known;
        }
        let half = self.timed.len() / 2;
        let mut lengths: Vec<Duration> = (0..half)
            .filter_map(|index| {
                let apart = self.timed[index + half]
                    .1
                    .checked_duration_since(self.timed[index].1)?;
                Some(apart / counted(apart, known).filter(|refreshes| *refreshes > 0)?)
            })
            .collect();
        middle(&mut lengths).unwrap_or(known)
    }

    /// What the screen says of the presents waiting for its word.
    fn resolve(&mut self, screen: &Displayed, now: Instant) {
        let Some(refreshes) = self.refreshes else {
            return;
        };
        // When a picture was shown is worked out from the refresh the
        // system timed, a number of refreshes back by its count: a
        // refresh it passed over in between, or the one timed misdated by
        // more than half a refresh, puts it a refresh off. Nothing is
        // judged by such a refresh, nor by one before it.
        let trusted = |counted: u64| self.disagreed.is_none_or(|disagreed| counted > disagreed);
        let (shown_trusted, timed_trusted) = (trusted(screen.at_refresh), trusted(screen.timed));
        while let Some(pending) = self.pending.front().copied() {
            match screen.shown.cmp(&pending.id) {
                Ordering::Equal => {
                    self.pending.pop_front();
                    if !shown_trusted {
                        self.second.unjudged += 1;
                    } else if refreshes.same(screen.at, pending.due) {
                        self.second.on_time += 1;
                    } else if screen.at > pending.due {
                        self.missed(pending, now);
                    } else {
                        self.second.early += 1;
                    }
                }
                // Shown and followed between two looks, or replaced before
                // its refresh: the screen does not say which.
                Ordering::Greater => {
                    self.pending.pop_front();
                }
                // Its refresh came and went with an older picture on the
                // screen.
                Ordering::Less
                    if timed_trusted && !refreshes.later(pending.due, screen.timed_at) =>
                {
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
        let Some(period) = self.refreshes.map(|known| known.period) else {
            return;
        };
        let margin = pending.due.saturating_duration_since(pending.at);
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
                 ({} of them presented with half a refresh to spare), {} sooner, {} not judged \
                 as the screen's count and clock disagreed; {screen}",
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
                second.unjudged,
            )
        });
    }
}

/// How many refreshes of `period` make up `span`, to the nearest.
fn counted(span: Duration, period: Duration) -> Option<u32> {
    let period = period.as_nanos();
    if period == 0 {
        return None;
    }
    u32::try_from((span.as_nanos() + period / 2) / period).ok()
}

/// How long each refresh lasted between two the system timed, by its
/// own count of them.
fn length(from: (u64, Instant), to: (u64, Instant)) -> Option<Duration> {
    let refreshes = u32::try_from(to.0.checked_sub(from.0)?)
        .ok()
        .filter(|refreshes| *refreshes > 0)?;
    Some(to.1.checked_duration_since(from.1)? / refreshes)
}

/// Whether the system counted another number of refreshes between two it
/// timed than the time between them holds: it passed over one in its
/// count, or misdated one by more than half a refresh.
fn disagree(from: (u64, Instant), to: (u64, Instant), period: Duration) -> bool {
    let by_time =
        to.1.checked_duration_since(from.1)
            .and_then(|apart| counted(apart, period));
    by_time.map(u64::from) != to.0.checked_sub(from.0)
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
        /// Refreshes the system's count passes over, as a screen left
        /// idle between two pictures can: from each on, it counts one
        /// fewer.
        uncounted: Vec<u64>,
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
                uncounted: Vec::new(),
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

        /// The refresh nearest `at`.
        fn nearest(&self, at: Instant) -> u64 {
            let before = self.last_by(at);
            if self.refresh(before + 1) - at < at - self.refresh(before) {
                before + 1
            } else {
                before
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

        /// The number the system gives refresh `refresh`.
        fn counted(&self, refresh: u64) -> u64 {
            let passed_over = self
                .uncounted
                .iter()
                .filter(|&&uncounted| uncounted <= refresh)
                .count();
            refresh - passed_over as u64
        }

        /// What the system says at `now`, as Windows does: the last
        /// present shown by the last refresh, and when that was, worked
        /// out as the player does from the refresh it timed.
        fn displayed(&self, now: Instant) -> Option<Displayed> {
            let timed = self.last_by(now);
            let (index, &(at, _)) = self
                .presents
                .iter()
                .enumerate()
                .rev()
                .find(|(_, (at, _))| self.target(*at) <= timed)?;
            let at_refresh = self.counted(self.target(at));
            let timed_at = self.said_at(timed);
            let timed = self.counted(timed);
            Some(Displayed {
                presented: self.presents.len() as u64,
                shown: index as u64 + 1,
                at_refresh,
                at: timed_at - self.reported * (timed - at_refresh) as u32,
                timed,
                timed_at,
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
        /// Every picture presented, for when the refresh it was meant for
        /// is.
        presented: Vec<(u32, Option<Instant>)>,
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
                    self.presented.push((turn.picture, turn.due));
                    let screen = self.compositor.displayed(now);
                    self.pacer.after(turn.due, now, screen.as_ref());
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
            .filter_map(|&(_, due)| Some(thread.compositor.nearest(due?)))
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
    fn a_screen_that_changes_its_rate_is_followed_within_a_few_refreshes() {
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
            .filter_map(|&(_, due)| Some(thread.compositor.nearest(due?)))
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
        // 9 ms early, as while the network was in trouble; and once one said
        // 9 ms late, more than half a refresh.
        compositor.misdated = (100..580).step_by(13).map(|count| (count, 7_000)).collect();
        compositor
            .misdated
            .extend([(300, -9_000), (301, -9_000), (450, 9_000)]);
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
        thread.pacer.look(start + Duration::from_secs(12));
        let steps = thread.steps(60, 598);
        assert!(steps.iter().all(|&step| step == 1), "{steps:?}");
        let period = thread.pacer.refreshes.map(|known| known.period);
        assert!(
            period.is_some_and(|period| period.abs_diff(Duration::from_nanos(SIXTY_HZ))
                < Duration::from_micros(20)),
            "{period:?}"
        );
        // Nor is a picture shown at a misdated refresh taken for one late.
        let written = journal.written();
        assert!(written.contains(", 0 later ("), "{written}");
        assert_eq!(thread.pacer.cutoff, CUTOFF);
    }

    #[test]
    fn a_refresh_the_count_passes_over_during_a_stall_moves_nothing() {
        let start = Instant::now();
        // Decoded 9.5 ms after a refresh, give or take half a millisecond:
        // presented at once, 7 ms before the refresh that shows it.
        let mut decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_micros(9_500),
            Duration::from_micros(500),
            600,
        );
        // Eight pictures held up 140 ms on the way.
        let released = decoded[208];
        for at in &mut decoded[200..208] {
            *at = released;
        }
        let run = |until: Duration, log: &Log| {
            let mut compositor = Compositor::new(start, SIXTY_HZ, Duration::from_millis(3));
            // While nothing new reached the screen, its count passed over a
            // refresh, as the laptop's did after each stall on the
            // twenty-fifth of September.
            compositor.uncounted = vec![205];
            let mut thread = Thread::new(compositor, Duration::from_millis(1), log);
            thread.run(&decoded, start + until);
            thread
        };
        let sixty_hz = |period: Option<Duration>| {
            period.is_some_and(|period| {
                period.abs_diff(Duration::from_nanos(SIXTY_HZ)) < Duration::from_micros(20)
            })
        };
        // A second after the stall, when most of the lengths measured span
        // the refresh passed over, a refresh still lasts what it lasts.
        let journal = testing::OwnLog::new("pacing-uncounted-second");
        let period = run(Duration::from_millis(4_550), &journal.log)
            .pacer
            .refreshes
            .map(|known| known.period);
        assert!(sixty_hz(period), "{period:?}");

        let journal = testing::OwnLog::new("pacing-uncounted");
        let mut thread = run(
            Duration::from_secs(10) + Duration::from_millis(30),
            &journal.log,
        );
        thread.pacer.look(start + Duration::from_secs(12));
        // Picture n is shown at refresh n + 2. After the stall the pictures
        // go on one a refresh, none of them a refresh late.
        let steps = thread.steps(212, 598);
        assert!(steps.iter().all(|&step| step == 1), "{steps:?}");
        let shown = thread.compositor.shown(0, 600);
        assert_eq!(shown[300], Some(298));
        // Nothing the screen said across the refresh it did not count is
        // taken as a picture shown late or early, nor as a longer refresh.
        let written = journal.written();
        assert!(written.contains(", 0 later ("), "{written}");
        assert!(
            written.contains(", 0 sooner, 1 not judged as the screen's count and clock"),
            "{written}"
        );
        assert_eq!(thread.pacer.cutoff, CUTOFF);
        let period = thread.pacer.refreshes.map(|known| known.period);
        assert!(sixty_hz(period), "{period:?}");
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
        // Counted afresh from the refresh just timed.
        assert_eq!(pacer.timed.len(), 1);
        assert_eq!(pacer.refreshes.map(|known| known.at), Some(later));
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
        assert_eq!((turn.picture, turn.due, turn.unshown), (2, None, 0));
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
    fn pictures_let_through_together_leave_only_the_newest_waiting() {
        let journal = testing::OwnLog::new("pacing-burst");
        let start = Instant::now();
        let mut thread = Thread::new(
            Compositor::new(start, SIXTY_HZ, Duration::from_millis(3)),
            Duration::from_millis(1),
            &journal.log,
        );
        // Decoded 12 ms after a refresh, once its window has closed: each
        // waits for the next one. Six pictures held up on the way, then
        // let through together, decoded 0.4 ms apart.
        let mut decoded = arrivals(
            start,
            SIXTY_HZ,
            Duration::from_millis(12),
            Duration::ZERO,
            300,
        );
        let released = decoded[105];
        for (n, at) in (0u32..).zip(&mut decoded[100..=105]) {
            *at = released + Duration::from_micros(400) * n;
        }
        thread.run(&decoded, start + Duration::from_secs(5));
        thread.pacer.look(start + Duration::from_secs(7));
        let shown = thread.compositor.shown(0, 300);
        // Picture n is shown at refresh n + 3. The newest of those let
        // through is shown at the first refresh it can be, and nothing
        // after it comes a refresh late.
        assert_eq!(shown[107], Some(99));
        assert_eq!(shown[108], Some(105));
        assert_eq!(shown[109], Some(106));
        assert_eq!(shown[150], Some(147));
        assert_eq!(thread.unshown, 5);
        assert!(
            journal
                .written()
                .contains("5 dropped as newer ones piled up"),
            "{}",
            journal.written()
        );
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
        let two_refreshes_on = at + Duration::from_nanos(2 * SIXTY_HZ);
        assert_eq!((turn.picture, turn.due), (2, Some(two_refreshes_on)));
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
