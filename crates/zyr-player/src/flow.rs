//! What the pictures went through on this computer, second by second.
//!
//! Sixty frames a second can still move unevenly: a frame that arrives a
//! little late and one that arrives a little early land on the same
//! refresh of the screen, the first is never seen and the refresh before
//! showed the old picture a second time. The measures shown in a session
//! average that away. The journal gets each second whole instead: when
//! each frame came whole and how long after its capture, how long its
//! packets took to all arrive, how long decoding it took, when it was
//! presented, and what the screen really showed, refresh by refresh, as
//! the system tells it. Frames are numbered as the host numbers them, so
//! the host's second and this one can be laid side by side.
//!
//! Three lines a second while pictures come, and nothing otherwise.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use zyr_media::trace::{Seconds, Spread, ms};
use zyr_media::video::AssembledFrame;
use zyr_proto::log::Log;

use crate::present::Displayed;

/// What these lines are filed under in the journal.
pub const TAG: &str = "flow";

/// Presents remembered until the screen says it showed them: far more
/// than the few the system ever queues.
const REMEMBERED: usize = 16;

pub(crate) struct Flow {
    log: Log,
    seconds: Seconds,
    second: Second,
    last_whole: Option<Instant>,
    last_presented: Option<Instant>,
    /// What the screen said last, which the next saying is counted from.
    screen: Option<Displayed>,
    /// Presents the screen has not been seen to show yet: their number,
    /// when they were presented, and how long after their capture.
    presents: VecDeque<(u64, Instant, Option<Duration>)>,
}

/// What one second gathered.
#[derive(Default)]
struct Second {
    stream: Option<u16>,
    first: Option<u32>,
    last: Option<u32>,
    whole: u32,
    keys: u32,
    repeats: u32,
    repaired: u32,
    lost: u32,
    skipped: u32,
    behind: u32,
    refused: u32,
    datagrams: u32,
    /// How long after the link had a datagram this thread took it.
    lag: Spread,
    every: Spread,
    latency: Spread,
    spread: Spread,
    decoding: Spread,
    /// Every frame, in order: `whole every/capture to whole/first to last
    /// packet/decoding` in milliseconds, then a letter for what became
    /// of it.
    arrivals: String,
    presented: u32,
    unshown: u64,
    present_every: Spread,
    waited: Spread,
    presenting: Spread,
    /// Every present, in order: milliseconds since the one before.
    presents: String,
    screen_seen: bool,
    shown: u64,
    never_shown: u64,
    repeated: u64,
    refresh: Option<Duration>,
    to_screen: Spread,
    capture_to_screen: Spread,
    compositor_missed: u64,
    compositor_dropped: u64,
    /// Refreshes between one picture reaching the screen and the next:
    /// one is a smooth picture, two a refresh that showed the one before
    /// again; `refreshes/pictures` when several arrived between two
    /// looks.
    gaps: String,
}

impl Flow {
    pub(crate) fn new(log: &Log) -> Self {
        Self {
            log: log.about(TAG),
            seconds: Seconds::default(),
            second: Second::default(),
            last_whole: None,
            last_presented: None,
            screen: None,
            presents: VecDeque::new(),
        }
    }

    /// This thread took a datagram `lag` after the link had it.
    pub(crate) fn datagram(&mut self, lag: Duration, now: Instant) {
        self.seconds.started(now);
        self.second.datagrams += 1;
        self.second.lag.add(lag);
    }

    /// A frame came whole, `since_capture` after it was captured on the
    /// host when the clocks are known apart. What became of it follows
    /// at once: [`Flow::decoded`] or one of the others.
    pub(crate) fn whole(&mut self, frame: &AssembledFrame, since_capture: Option<Duration>) {
        if self
            .second
            .stream
            .is_some_and(|stream| stream != frame.stream)
        {
            self.write();
        }
        self.seconds.started(frame.last_packet);
        let every = self
            .last_whole
            .replace(frame.last_packet)
            .map(|before| frame.last_packet.saturating_duration_since(before));
        let spread = frame
            .last_packet
            .saturating_duration_since(frame.first_packet);
        let second = &mut self.second;
        second.stream = Some(frame.stream);
        second.first.get_or_insert(frame.frame);
        second.last = Some(frame.frame);
        second.whole += 1;
        second.keys += u32::from(frame.key);
        second.repeats += u32::from(frame.repeat);
        second.repaired += u32::from(frame.repaired);
        if let Some(every) = every {
            second.every.add(every);
        }
        if let Some(latency) = since_capture {
            second.latency.add(latency);
        }
        second.spread.add(spread);
        let _ = write!(
            second.arrivals,
            " {:.1}/{}/{:.1}",
            every.map_or(0.0, ms),
            since_capture.map_or_else(|| "?".to_string(), |latency| format!("{:.1}", ms(latency))),
            ms(spread),
        );
        for (letter, is) in [("k", frame.key), ("r", frame.repeat), ("p", frame.repaired)] {
            if is {
                second.arrivals.push_str(letter);
            }
        }
    }

    /// The frame just whole was decoded, in that long.
    pub(crate) fn decoded(&mut self, took: Duration) {
        self.second.decoding.add(took);
        let _ = write!(self.second.arrivals, "/{:.1}", ms(took));
    }

    /// The frame just whole was passed over, waiting for a key frame.
    pub(crate) fn skipped(&mut self) {
        self.second.skipped += 1;
        self.second.arrivals.push('s');
    }

    /// The frame just whole was dropped undecoded, this computer having
    /// fallen behind.
    pub(crate) fn behind(&mut self) {
        self.second.behind += 1;
        self.second.arrivals.push('b');
    }

    /// The decoder refused the frame just whole, or there was none.
    pub(crate) fn refused(&mut self) {
        self.second.refused += 1;
        self.second.arrivals.push('x');
    }

    /// A frame could not be completed.
    pub(crate) fn lost(&mut self, now: Instant) {
        self.seconds.started(now);
        self.second.lost += 1;
        self.second.arrivals.push_str(" L");
    }

    /// Pictures decoded that a newer one replaced before their turn.
    pub(crate) fn unshown(&mut self, count: u64) {
        self.second.unshown += count;
    }

    /// A picture whole at `whole` was presented from `started` to `done`,
    /// `since_capture` after its capture; `screen` is what the screen
    /// said right after.
    pub(crate) fn presented(
        &mut self,
        whole: Instant,
        started: Instant,
        done: Instant,
        since_capture: Option<Duration>,
        screen: Option<Displayed>,
    ) {
        self.seconds.started(done);
        let every = self
            .last_presented
            .replace(started)
            .map(|before| started.saturating_duration_since(before));
        let second = &mut self.second;
        second.presented += 1;
        if let Some(every) = every {
            second.present_every.add(every);
        }
        second.waited.add(started.saturating_duration_since(whole));
        second
            .presenting
            .add(done.saturating_duration_since(started));
        let _ = write!(second.presents, " {:.1}", every.map_or(0.0, ms));
        if let Some(screen) = screen {
            if self.presents.len() == REMEMBERED {
                self.presents.pop_front();
            }
            self.presents
                .push_back((screen.presented, done, since_capture));
            self.screen_said(screen);
        }
    }

    /// Counts what the screen showed since it last said.
    fn screen_said(&mut self, now: Displayed) {
        let second = &mut self.second;
        second.screen_seen = true;
        second.refresh = Some(now.refresh);
        // A surface made again counts from nought: nothing to compare.
        let Some(before) = self
            .screen
            .replace(now)
            .filter(|before| before.presented <= now.presented && before.shown <= now.shown)
        else {
            return;
        };
        second.compositor_missed += now
            .compositor_missed
            .saturating_sub(before.compositor_missed);
        second.compositor_dropped += now
            .compositor_dropped
            .saturating_sub(before.compositor_dropped);
        let pictures = now.shown - before.shown;
        if pictures == 0 {
            return;
        }
        let refreshes = now.at_refresh.saturating_sub(before.at_refresh);
        second.shown += pictures;
        second.never_shown += pictures.saturating_sub(refreshes);
        second.repeated += refreshes.saturating_sub(pictures);
        if pictures == 1 {
            let _ = write!(second.gaps, " {refreshes}");
        } else {
            let _ = write!(second.gaps, " {refreshes}/{pictures}");
        }
        // How long the picture now on the screen took to get there.
        while let Some(&(number, presented, since_capture)) = self.presents.front() {
            if number > now.shown {
                break;
            }
            self.presents.pop_front();
            if number == now.shown {
                let to_screen = now.at.saturating_duration_since(presented);
                second.to_screen.add(to_screen);
                if let Some(since_capture) = since_capture {
                    second.capture_to_screen.add(since_capture + to_screen);
                }
            }
        }
    }

    /// Writes the second out once it is over.
    pub(crate) fn look(&mut self, now: Instant) {
        if self.seconds.over(now) {
            self.write();
        }
    }

    /// Writes what is gathered, whatever the time: at the end of a stream
    /// or of a session.
    pub(crate) fn write(&mut self) {
        let second = std::mem::take(&mut self.second);
        if second.whole == 0 && second.lost == 0 && second.presented == 0 {
            return;
        }
        let frames = match (second.stream, second.first, second.last) {
            (Some(stream), Some(first), Some(last)) => {
                format!("stream {stream} frames {first}-{last}")
            }
            _ => "no frame".to_string(),
        };
        self.log.debug(|| {
            format!(
                "{frames}: {} whole ({} key, {} repeats, {} repaired), {} lost, {} passed over \
                 waiting for a key frame, {} dropped behind, {} refused; {} datagrams, taken {} ms \
                 after the link had them; whole every {} ms, capture to whole {} ms, first to last \
                 packet {} ms, decoding {} ms (median/95th/worst)",
                second.whole,
                second.keys,
                second.repeats,
                second.repaired,
                second.lost,
                second.skipped,
                second.behind,
                second.refused,
                second.datagrams,
                second.lag,
                second.every,
                second.latency,
                second.spread,
                second.decoding,
            )
        });
        if !second.arrivals.is_empty() {
            self.log.debug(|| {
                format!(
                    "{frames}, each as whole every ms/capture to whole ms/first to last packet \
                     ms/decoding ms, k key, r repeat, p repaired, s passed over, b dropped \
                     behind, x refused, L lost:{}",
                    second.arrivals
                )
            });
        }
        if second.presented > 0 {
            self.log.debug(|| screen_line(&frames, &second));
        }
    }
}

/// What the last second gathered goes to the journal with the session,
/// however it ends.
impl Drop for Flow {
    fn drop(&mut self) {
        self.write();
    }
}

/// What was presented, and what the screen made of it.
fn screen_line(frames: &str, second: &Second) -> String {
    let mut line = format!(
        "{frames} on screen: {} presented every {} ms, {} replaced before their turn; whole to \
         presented {} ms, presenting {} ms",
        second.presented, second.present_every, second.unshown, second.waited, second.presenting,
    );
    if second.screen_seen {
        let _ = write!(
            line,
            "; {} reached the screen, {} never did, {} refreshes showed the picture before \
             again, a refresh every {:.2} ms; presented to screen {} ms, capture to screen {} \
             ms; the compositor missed {} and dropped {}; refreshes between pictures on \
             screen:{}",
            second.shown,
            second.never_shown,
            second.repeated,
            second.refresh.map_or(0.0, ms),
            second.to_screen,
            second.capture_to_screen,
            second.compositor_missed,
            second.compositor_dropped,
            second.gaps,
        );
    } else {
        line.push_str("; the system said nothing of what the screen showed");
    }
    let _ = write!(line, "; presented every ms:{}", second.presents);
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use zyr_media::codec::VideoCodec;

    fn frame(at: Instant, number: u32, arrived: Duration) -> AssembledFrame {
        AssembledFrame {
            stream: 3,
            frame: number,
            key: number == 0,
            repeat: false,
            codec: VideoCodec::Hevc,
            data: Vec::new(),
            captured_us: 0,
            host_latency_us: 0,
            first_packet: at + arrived - Duration::from_micros(1_200),
            last_packet: at + arrived,
            repaired: number == 2,
        }
    }

    fn screen(at: Instant, presented: u64, shown: u64, refresh: u64) -> Displayed {
        Displayed {
            presented,
            shown,
            at_refresh: refresh,
            at: at + Duration::from_micros(refresh * 16_667),
            refresh: Duration::from_micros(16_667),
            compositor_missed: 0,
            compositor_dropped: 0,
        }
    }

    #[test]
    fn each_second_says_every_frame_and_what_the_screen_made_of_it() {
        let journal = testing::OwnLog::new("flow-each-second");
        let mut flow = Flow::new(&journal.log);
        let at = Instant::now();
        let period = Duration::from_micros(16_667);
        for n in 0..4u32 {
            let arrived = period * n + Duration::from_millis(20);
            flow.datagram(Duration::from_micros(300), at + arrived);
            flow.whole(&frame(at, n, arrived), Some(Duration::from_millis(25)));
            if n == 3 {
                flow.behind();
                continue;
            }
            flow.decoded(Duration::from_micros(2_000));
            let whole = at + arrived;
            let started = whole + Duration::from_micros(500);
            let done = started + Duration::from_micros(700);
            // The screen showed each picture one refresh after the one
            // before, but for the third, which waited a refresh more.
            let refresh = match n {
                0 => 2,
                1 => 3,
                _ => 5,
            };
            flow.presented(
                whole,
                started,
                done,
                Some(Duration::from_millis(26)),
                Some(screen(at, u64::from(n) + 1, u64::from(n) + 1, refresh)),
            );
        }
        flow.lost(at + period * 5);
        flow.look(at + Duration::from_secs(5));

        let written = journal.written();
        assert!(
            written.contains(
                "stream 3 frames 0-3: 4 whole (1 key, 0 repeats, 1 repaired), 1 lost, 0 passed \
                 over waiting for a key frame, 1 dropped behind, 0 refused; 4 datagrams, taken \
                 0.3/0.3/0.3 ms after the link had them; whole every 16.7/16.7/16.7 ms, capture \
                 to whole 25.0/25.0/25.0 ms, first to last packet 1.2/1.2/1.2 ms"
            ),
            "{written}"
        );
        assert!(
            written.contains(
                ": 0.0/25.0/1.2k/2.0 16.7/25.0/1.2/2.0 16.7/25.0/1.2p/2.0 16.7/25.0/1.2b L"
            ),
            "{written}"
        );
        assert!(
            written.contains("3 presented every 16.7/16.7/16.7 ms"),
            "{written}"
        );
        // Two pictures reached the screen after the first was seen, one
        // of them a refresh late.
        assert!(
            written.contains(
                "2 reached the screen, 0 never did, 1 refreshes showed the picture before again"
            ),
            "{written}"
        );
        assert!(
            written.contains("refreshes between pictures on screen: 1 2"),
            "{written}"
        );
    }

    #[test]
    fn a_picture_the_screen_never_showed_is_counted() {
        let journal = testing::OwnLog::new("flow-never-shown");
        let mut flow = Flow::new(&journal.log);
        let at = Instant::now();
        flow.presented(at, at, at, None, Some(screen(at, 1, 1, 10)));
        // Two more presented, one refresh later only the last is shown.
        flow.presented(at, at, at, None, Some(screen(at, 3, 3, 11)));
        flow.write();
        let written = journal.written();
        assert!(
            written.contains("2 reached the screen, 1 never did, 0 refreshes"),
            "{written}"
        );
        assert!(
            written.contains("refreshes between pictures on screen: 1/2"),
            "{written}"
        );
    }

    #[test]
    fn a_session_with_nothing_on_screen_writes_nothing() {
        let journal = testing::OwnLog::new("flow-nothing");
        let mut flow = Flow::new(&journal.log);
        flow.look(Instant::now() + Duration::from_secs(5));
        flow.write();
        assert!(journal.written().is_empty());
    }
}
