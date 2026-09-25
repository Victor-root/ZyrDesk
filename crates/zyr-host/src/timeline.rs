//! What every picture went through on this computer, second by second.
//!
//! A session that plays at sixty frames a second and still feels uneven
//! has lost its time somewhere between the screen and the network, and a
//! mean says nowhere. Each second is written out whole instead: how often
//! the screen gave an image and how late this thread woke to it, how late
//! a held image or a repeat went against the time it was due, how long
//! each step of each picture took, and every picture one after the other,
//! under the frame number the player counts it by.
//!
//! Two lines a second while pictures leave, and nothing otherwise.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use zyr_media::trace::{Seconds, Spread, ms};
use zyr_proto::log::Log;

/// What these lines are filed under in the journal.
pub(crate) const TAG: &str = "pace";

/// How a picture came to leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Left {
    /// A new image, gone as soon as it was captured.
    Fresh,
    /// A new image the cadence held back until its turn.
    Held,
    /// The last image again.
    Repeat,
}

/// One picture handed to the link, with when each step of it ended.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Picture {
    pub(crate) stream: u16,
    pub(crate) frame: u32,
    pub(crate) left: Left,
    pub(crate) key: bool,
    /// When the image was captured; for a repeat, when it was decided.
    pub(crate) captured: Instant,
    /// When drawing it into the encoder's frame began.
    pub(crate) started: Instant,
    pub(crate) drawn: Instant,
    /// When its packet came out of the encoder.
    pub(crate) encoded: Instant,
    /// When its datagrams were in the link's queue.
    pub(crate) handed: Instant,
    pub(crate) bytes: usize,
    pub(crate) datagrams: usize,
}

/// A held image or a repeat going out, against when it was due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Due {
    Held,
    Repeat,
    Still,
}

pub(crate) struct Timeline {
    log: Log,
    seconds: Seconds,
    second: Second,
    /// When the previous picture left, and the previous image came.
    last_left: Option<Instant>,
    last_capture: Option<Instant>,
}

/// What one second gathered.
#[derive(Default)]
struct Second {
    stream: Option<u16>,
    first: Option<u32>,
    last: Option<u32>,
    fresh: u32,
    held: u32,
    repeats: u32,
    keys: u32,
    datagrams: usize,
    bytes: usize,
    left_every: Spread,
    host: Spread,
    waited: Spread,
    drawing: Spread,
    encoding: Spread,
    handing: Spread,
    kilobytes: Spread,
    captures: u32,
    capture_every: Spread,
    woken: Spread,
    pointer_only: u32,
    held_late: Spread,
    repeat_late: Spread,
    stills: u32,
    crowded: u32,
    skipped: u32,
    /// Every picture, in the order it left: `left every/host/KB` in
    /// milliseconds and kilobytes, then a letter for what it was.
    each: String,
}

impl Timeline {
    pub(crate) fn new(log: &Log) -> Self {
        Self {
            log: log.about(TAG),
            seconds: Seconds::default(),
            second: Second::default(),
            last_left: None,
            last_capture: None,
        }
    }

    /// The screen gave an image made at `at`, and this thread had it at
    /// `got`.
    pub(crate) fn captured(&mut self, at: Instant, got: Instant) {
        self.seconds.started(got);
        let second = &mut self.second;
        second.captures += 1;
        if let Some(before) = self.last_capture.replace(at) {
            second
                .capture_every
                .add(at.saturating_duration_since(before));
        }
        second.woken.add(got.saturating_duration_since(at));
    }

    /// The pointer moved and nothing else did.
    pub(crate) fn pointer_only(&mut self, now: Instant) {
        self.seconds.started(now);
        self.second.pointer_only += 1;
    }

    /// Something the cadence decided went out `late` after it was due.
    pub(crate) fn due(&mut self, due: Due, late: Duration, now: Instant) {
        self.seconds.started(now);
        match due {
            Due::Held => self.second.held_late.add(late),
            Due::Repeat => self.second.repeat_late.add(late),
            Due::Still => self.second.stills += 1,
        }
    }

    /// A picture found the link full and was dropped.
    pub(crate) fn crowded(&mut self, now: Instant) {
        self.seconds.started(now);
        self.second.crowded += 1;
    }

    /// A picture was left out, waiting for a key frame.
    pub(crate) fn skipped(&mut self, now: Instant) {
        self.seconds.started(now);
        self.second.skipped += 1;
    }

    pub(crate) fn left(&mut self, picture: Picture) {
        // A new stream counts its frames from nought: what the older one
        // gathered is its own second.
        if self
            .second
            .stream
            .is_some_and(|stream| stream != picture.stream)
        {
            self.write();
        }
        self.seconds.started(picture.handed);
        let every = self
            .last_left
            .replace(picture.started)
            .map(|before| picture.started.saturating_duration_since(before));
        let second = &mut self.second;
        second.stream = Some(picture.stream);
        second.first.get_or_insert(picture.frame);
        second.last = Some(picture.frame);
        match picture.left {
            Left::Fresh => second.fresh += 1,
            Left::Held => second.held += 1,
            Left::Repeat => second.repeats += 1,
        }
        second.keys += u32::from(picture.key);
        second.datagrams += picture.datagrams;
        second.bytes += picture.bytes;
        if let Some(every) = every {
            second.left_every.add(every);
        }
        let host = picture.handed.saturating_duration_since(picture.captured);
        second.host.add(host);
        second
            .waited
            .add(picture.started.saturating_duration_since(picture.captured));
        second
            .drawing
            .add(picture.drawn.saturating_duration_since(picture.started));
        second
            .encoding
            .add(picture.encoded.saturating_duration_since(picture.drawn));
        second
            .handing
            .add(picture.handed.saturating_duration_since(picture.encoded));
        let kilobytes = picture.bytes as f32 / 1024.0;
        second.kilobytes.add_value(kilobytes);
        let _ = write!(
            second.each,
            " {:.1}/{:.1}/{kilobytes:.0}{}{}",
            every.map_or(0.0, ms),
            ms(host),
            match picture.left {
                Left::Fresh => "",
                Left::Held => "h",
                Left::Repeat => "r",
            },
            if picture.key { "k" } else { "" },
        );
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
        let nothing_left = second.first.is_none();
        if nothing_left && second.captures == 0 && second.stills == 0 {
            return;
        }
        let frames = match (second.stream, second.first, second.last) {
            (Some(stream), Some(first), Some(last)) => {
                format!("stream {stream} frames {first}-{last}")
            }
            _ => "no picture".to_string(),
        };
        self.log.debug(|| {
            format!(
                "{frames}: {} out ({} fresh, {} held, {} repeats, {} key), {} datagrams, {} KB; \
                 out every {} ms; host {} ms = waited {} + drawing {} + encoding {} + handing {}; \
                 size {} KB; screen gave {} images every {} ms, woke {} ms after each, {} \
                 pointer only; held out {} ms late, repeats {} ms late, {} stills; {} dropped \
                 on a full link, {} left out for a key frame (ms and KB as median/95th/worst)",
                second.fresh + second.held + second.repeats,
                second.fresh,
                second.held,
                second.repeats,
                second.keys,
                second.datagrams,
                second.bytes / 1024,
                second.left_every,
                second.host,
                second.waited,
                second.drawing,
                second.encoding,
                second.handing,
                second.kilobytes,
                second.captures,
                second.capture_every,
                second.woken,
                second.pointer_only,
                second.held_late,
                second.repeat_late,
                second.stills,
                second.crowded,
                second.skipped,
            )
        });
        if !nothing_left {
            self.log.debug(|| {
                format!(
                    "{frames}, each as out every ms/host ms/KB, h held, r repeat, k key:{}",
                    second.each
                )
            });
        }
    }
}

/// What the link made of the pictures, second by second: how long each
/// waited in its queue, and how long writing its datagrams onto the local
/// link took. A picture written one datagram at a time is as many turns
/// of the system as it has datagrams, and a large one is exactly the
/// picture a window being dragged makes.
pub(crate) struct Written {
    log: Log,
    seconds: Seconds,
    pictures: u32,
    datagrams: usize,
    bytes: usize,
    queued: Spread,
    writing: Spread,
}

impl Written {
    pub(crate) fn new(log: &Log) -> Self {
        Self {
            log: log.about(TAG),
            seconds: Seconds::default(),
            pictures: 0,
            datagrams: 0,
            bytes: 0,
            queued: Spread::default(),
            writing: Spread::default(),
        }
    }

    /// A picture handed to the link at `handed` was written from
    /// `started` to `done`.
    pub(crate) fn picture(
        &mut self,
        handed: Instant,
        started: Instant,
        done: Instant,
        datagrams: usize,
        bytes: usize,
    ) {
        self.seconds.started(done);
        self.pictures += 1;
        self.datagrams += datagrams;
        self.bytes += bytes;
        self.queued.add(started.saturating_duration_since(handed));
        self.writing.add(done.saturating_duration_since(started));
        if self.seconds.over(done) {
            self.write();
        }
    }

    /// Writes what is gathered, whatever the time.
    pub(crate) fn write(&mut self) {
        if self.pictures == 0 {
            return;
        }
        self.log.debug(|| {
            format!(
                "link: {} pictures, {} datagrams, {} KB onto the local link; waited in its \
                 queue {} ms, written in {} ms (median/95th/worst)",
                self.pictures,
                self.datagrams,
                self.bytes / 1024,
                self.queued,
                self.writing,
            )
        });
        self.pictures = 0;
        self.datagrams = 0;
        self.bytes = 0;
        self.queued.clear();
        self.writing.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::end_to_end::TestLog;

    /// Everything the timeline wrote.
    fn written(journal: &TestLog) -> String {
        journal.lines_with("").join("\n")
    }

    fn picture(at: Instant, frame: u32, left: Left) -> Picture {
        let ms = |n: u64| at + Duration::from_micros(n * 100);
        Picture {
            stream: 1,
            frame,
            left,
            key: frame == 0,
            captured: at,
            started: ms(10),
            drawn: ms(15),
            encoded: ms(55),
            handed: ms(56),
            bytes: 40 * 1024,
            datagrams: 36,
        }
    }

    #[test]
    fn each_second_says_every_picture_and_where_its_time_went() {
        let journal = TestLog::new("timeline");
        let mut timeline = Timeline::new(&journal.log);
        let at = Instant::now();
        let period = Duration::from_micros(16_667);
        for n in 0..3u32 {
            let when = at + period * n;
            timeline.captured(when, when + Duration::from_micros(800));
            timeline.left(picture(
                when,
                n,
                if n == 2 { Left::Held } else { Left::Fresh },
            ));
        }
        timeline.due(Due::Repeat, Duration::from_micros(300), at + period * 3);
        timeline.left(picture(at + period * 3, 3, Left::Repeat));
        timeline.look(at + period * 4);
        assert!(
            written(&journal).is_empty(),
            "written before the second was over"
        );
        timeline.look(at + Duration::from_secs(2));

        let said = written(&journal);
        assert!(
            said.contains("stream 1 frames 0-3: 4 out (2 fresh, 1 held, 1 repeats, 1 key)"),
            "{said}"
        );
        // Five and a half milliseconds from capture to the link's queue,
        // four of them encoding.
        assert!(said.contains("host 5.6/5.6/5.6 ms"), "{said}");
        assert!(said.contains("encoding 4.0/4.0/4.0"), "{said}");
        assert!(
            said.contains("screen gave 3 images every 16.7/16.7/16.7 ms"),
            "{said}"
        );
        assert!(said.contains("repeats 0.3/0.3/0.3 ms late"), "{said}");
        assert!(
            said.contains("each as out every ms/host ms/KB, h held, r repeat, k key: 0.0/5.6/40k 16.7/5.6/40 16.7/5.6/40h 16.7/5.6/40r"),
            "{said}"
        );
    }

    #[test]
    fn a_new_stream_closes_the_second_of_the_old_one() {
        let journal = TestLog::new("timeline-streams");
        let mut timeline = Timeline::new(&journal.log);
        let at = Instant::now();
        timeline.left(picture(at, 7, Left::Fresh));
        timeline.left(Picture {
            stream: 2,
            ..picture(at, 0, Left::Fresh)
        });
        let said = written(&journal);
        assert!(said.contains("stream 1 frames 7-7"), "{said}");
        assert!(!said.contains("stream 2"), "{said}");
        timeline.write();
        assert!(written(&journal).contains("stream 2 frames 0-0"));
    }

    #[test]
    fn the_link_says_how_long_its_pictures_waited_and_took() {
        let journal = TestLog::new("timeline-link");
        let mut link = Written::new(&journal.log);
        let at = Instant::now();
        let after = |n: u64| at + Duration::from_micros(n * 100);
        link.picture(at, after(2), after(12), 36, 40 * 1024);
        assert!(written(&journal).is_empty());
        link.picture(after(10_000), after(10_001), after(10_031), 40, 44 * 1024);
        let said = written(&journal);
        assert!(
            said.contains(
                "link: 2 pictures, 76 datagrams, 84 KB onto the local link; waited in its queue \
                 0.1/0.2/0.2 ms, written in 1.0/3.0/3.0 ms"
            ),
            "{said}"
        );
    }

    #[test]
    fn nothing_is_written_for_a_second_where_nothing_happened() {
        let journal = TestLog::new("timeline-quiet");
        let mut timeline = Timeline::new(&journal.log);
        timeline.look(Instant::now() + Duration::from_secs(5));
        timeline.write();
        assert!(written(&journal).is_empty());
    }
}
