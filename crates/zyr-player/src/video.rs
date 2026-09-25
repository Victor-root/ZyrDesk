//! The picture: datagrams in, pictures on the surface, on one thread.
//!
//! Assembling, decoding and drawing follow each other on the same
//! thread, with nothing handed from one thread to another on the way:
//! a frame is drawn the moment its last packet is in and decoded.
//! Frames that arrive together are all decoded, since each one is the
//! reference of the next, and only the newest is drawn.
//!
//! A frame lost for good leaves the decoder without what the frames
//! after it refer to. They are passed over until a key frame comes, and
//! one is asked for, once, then again every quarter second while none
//! comes, never once per frame: a flood of requests is what a struggling
//! network can afford least.

use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use zyr_codec::{DecodedFrame, Ffmpeg, VideoDecoder};
use zyr_media::codec::VideoCodec;
use zyr_media::video::{Assembled, AssembledFrame, Assembler, AssemblyLimits};
use zyr_proto::log::Log;

use crate::lock;
use crate::present::{Fault, Presenter, Rect};
use crate::seldom::Seldom;
use crate::stats::Tally;
use crate::tallies::{PictureTallies, Tallies};

/// The tag of the picture's lines in the journal.
pub const TAG: &str = "picture";

/// How long a request for a key frame waits for its answer before it is
/// made again.
pub const RESEND: Duration = Duration::from_millis(250);

/// Datagrams taken in one go before the frames they complete are
/// settled: a key frame's worth, so that a burst is assembled whole
/// rather than settled after every packet.
const BURST: usize = 1024;

/// A key frame to ask the host for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recover {
    pub stream: u16,
    /// The last frame of the stream decoded.
    pub frame: u32,
}

/// What the video thread has to tell the rest of the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Recover(Recover),
    FirstPicture,
    /// A sentence in French, for the person.
    Notice(String),
    /// Nothing can be shown any more: the session has to end, for this
    /// reason, in French.
    Failed(String),
}

/// Whether a whole frame can be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Decode,
    /// It refers to a frame the decoder never had.
    Skip,
}

/// When frames can be decoded, and when to ask for a key frame.
#[derive(Debug)]
pub struct Recovery {
    stream: Option<u16>,
    decoded: u32,
    needs_key: bool,
    /// When the request for a key frame still unanswered was last sent.
    asked_at: Option<Instant>,
}

impl Default for Recovery {
    fn default() -> Self {
        Self::new()
    }
}

impl Recovery {
    pub fn new() -> Self {
        Self {
            stream: None,
            decoded: 0,
            // A stream starts at a key frame, and the decoder with it.
            needs_key: true,
            asked_at: None,
        }
    }

    /// A whole frame came out of the assembler.
    pub fn whole(&mut self, frame: &AssembledFrame) -> Verdict {
        self.follow(frame.stream);
        if frame.key {
            self.needs_key = false;
            self.asked_at = None;
        }
        if self.needs_key {
            Verdict::Skip
        } else {
            Verdict::Decode
        }
    }

    /// That frame of the current stream was decoded.
    pub fn decoded(&mut self, frame: u32) {
        self.decoded = frame;
    }

    /// The assembler gave a frame of `stream` up.
    pub fn lost(&mut self, stream: u16, now: Instant) -> Option<Recover> {
        self.follow(stream);
        self.broken(now)
    }

    /// Nothing decodes any more until a key frame: the decoder refused a
    /// frame, or had to be made again. The request, unless one is
    /// already waiting for its answer.
    pub fn broken(&mut self, now: Instant) -> Option<Recover> {
        self.needs_key = true;
        let stream = self.stream?;
        if self.asked_at.is_some() {
            return None;
        }
        self.asked_at = Some(now);
        Some(Recover {
            stream,
            frame: self.decoded,
        })
    }

    /// The request again, when it has waited long enough.
    pub fn due(&mut self, now: Instant) -> Option<Recover> {
        let stream = self.stream?;
        if now < self.next_resend()? {
            return None;
        }
        self.asked_at = Some(now);
        Some(Recover {
            stream,
            frame: self.decoded,
        })
    }

    pub fn next_resend(&self) -> Option<Instant> {
        self.asked_at.map(|at| at + RESEND)
    }

    /// A newer stream begins with a key frame of its own: whatever was
    /// asked of the older one is moot.
    fn follow(&mut self, stream: u16) {
        if self.stream != Some(stream) {
            self.stream = Some(stream);
            self.decoded = 0;
            self.needs_key = true;
            self.asked_at = None;
        }
    }
}

/// The decoder of the current stream.
struct Decoding {
    ff: Arc<Ffmpeg>,
    opened: Option<Opened>,
    /// A stream no decoder could be opened for, said once and passed
    /// over from then on.
    refused: Option<u16>,
}

struct Opened {
    decoder: VideoDecoder,
    stream: u16,
    codec: VideoCodec,
    /// What the decoder had replaced before the last frame.
    replaced: u64,
}

enum Decoded {
    Picture {
        picture: Option<DecodedFrame>,
        took: Duration,
        /// Pictures the decoder dropped for a newer one of the same
        /// packet.
        replaced: u64,
    },
    /// The decoder refused the frame, in its words.
    Broken(String),
    /// No decoder for this stream: `Some` the first time, with what to
    /// tell the person.
    Refused(Option<String>),
}

impl Decoding {
    fn decode(&mut self, frame: &AssembledFrame, presenter: &impl Presenter, log: &Log) -> Decoded {
        if self.refused == Some(frame.stream) {
            return Decoded::Refused(None);
        }
        let current = self
            .opened
            .as_ref()
            .is_some_and(|opened| opened.stream == frame.stream && opened.codec == frame.codec);
        if !current {
            // The old one first, whose pictures take room on the card.
            self.opened = None;
            let Some(output) = presenter.output() else {
                return Decoded::Refused(None);
            };
            match VideoDecoder::open(&self.ff, frame.codec, output) {
                Ok(decoder) => {
                    log.write(&format!(
                        "decoding stream {} in {}{}",
                        frame.stream,
                        frame.codec.name(),
                        sampling(&decoder)
                    ));
                    self.opened = Some(Opened {
                        decoder,
                        stream: frame.stream,
                        codec: frame.codec,
                        replaced: 0,
                    });
                }
                Err(e) => {
                    log.write(&format!(
                        "no decoder for stream {} in {}: {e}",
                        frame.stream,
                        frame.codec.name()
                    ));
                    self.refused = Some(frame.stream);
                    return Decoded::Refused(Some(format!(
                        "Cet ordinateur ne sait pas décoder l'image en {} : {e}",
                        frame.codec.name()
                    )));
                }
            }
        }
        let Some(opened) = self.opened.as_mut() else {
            return Decoded::Refused(None);
        };
        let started = Instant::now();
        match opened.decoder.decode(&frame.data) {
            Ok(picture) => {
                let replaced = opened.decoder.replaced();
                let newly = replaced - opened.replaced;
                opened.replaced = replaced;
                Decoded::Picture {
                    picture,
                    took: started.elapsed(),
                    replaced: newly,
                }
            }
            Err(e) => {
                // A fresh decoder for the key frame to come: whatever state
                // the refused frame left behind goes with this one.
                self.opened = None;
                Decoded::Broken(e.to_string())
            }
        }
    }

    fn close(&mut self) {
        self.opened = None;
        self.refused = None;
    }
}

/// How the decoded textures are reached, for the journal.
#[cfg(windows)]
fn sampling(decoder: &VideoDecoder) -> String {
    decoder
        .sampling()
        .map(|sampling| format!(" on the graphics card ({sampling:?})"))
        .unwrap_or_default()
}

#[cfg(not(windows))]
fn sampling(_decoder: &VideoDecoder) -> String {
    String::new()
}

/// The lines about what is passed over, each at its own pace.
#[derive(Default)]
struct Hushed {
    malformed: Seldom,
    lost: Seldom,
    skipped: Seldom,
    broken: Seldom,
    unshown: Seldom,
    undrawn: Seldom,
}

/// Datagrams in, pictures out.
pub struct Video<P: Presenter> {
    presenter: P,
    assembler: Assembler,
    recovery: Recovery,
    decoding: Decoding,
    /// The picture on the surface, drawn again when the surface changes
    /// size.
    on_screen: Option<(DecodedFrame, u32)>,
    first_shown: bool,
    counters: PictureTallies,
    hushed: Hushed,
    tally: Arc<Mutex<Tally>>,
    tallies: Arc<Mutex<Tallies>>,
    rect: Arc<Mutex<Option<Rect>>>,
    log: Log,
}

impl<P: Presenter> Video<P> {
    pub fn new(
        ff: Arc<Ffmpeg>,
        presenter: P,
        tally: Arc<Mutex<Tally>>,
        tallies: Arc<Mutex<Tallies>>,
        rect: Arc<Mutex<Option<Rect>>>,
        log: Log,
    ) -> Self {
        Self {
            presenter,
            assembler: Assembler::new(AssemblyLimits::default()),
            recovery: Recovery::new(),
            decoding: Decoding {
                ff,
                opened: None,
                refused: None,
            },
            on_screen: None,
            first_shown: false,
            counters: PictureTallies::default(),
            hushed: Hushed::default(),
            tally,
            tallies,
            rect,
            log,
        }
    }

    /// Takes one datagram in.
    pub fn push(&mut self, datagram: &[u8], now: Instant) {
        if let Err(e) = self.assembler.push(datagram, now) {
            self.hushed.malformed.note(&self.log, now, |times| {
                format!("video datagram refused: {e} ({times} since last said)")
            });
        }
    }

    /// When [`Video::settle`] has something to do with no new datagram.
    pub fn next_wakeup(&self) -> Option<Instant> {
        match (self.assembler.next_deadline(), self.recovery.next_resend()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Decodes every frame settled by now, draws the newest, and asks
    /// for a key frame when one is needed.
    pub fn settle(&mut self, now: Instant) -> Vec<Said> {
        let mut said = Vec::new();
        let mut newest: Option<(DecodedFrame, u32)> = None;
        while let Some(assembled) = self.assembler.poll(now) {
            match assembled {
                Assembled::Lost { stream, frame } => {
                    lock(&self.tally).lost(now);
                    self.hushed.lost.note(&self.log, now, |times| {
                        format!("frame {frame} of stream {stream} lost ({times} since last said)")
                    });
                    if let Some(recover) = self.recovery.lost(stream, now) {
                        self.ask(recover, &mut said);
                    }
                }
                Assembled::Frame(frame) => {
                    lock(&self.tally).assembled(now, frame.data.len(), frame.host_latency_us);
                    if let Some(picture) = self.decode(&frame, now, &mut said)
                        && newest.replace((picture, frame.captured_us)).is_some()
                    {
                        self.unshown(now, 1);
                    }
                    self.assembler.recycle(frame.data);
                }
            }
        }
        if let Some(recover) = self.recovery.due(now) {
            self.ask(recover, &mut said);
        }
        if let Some((picture, captured_us)) = newest {
            self.show(picture, captured_us, now, &mut said);
        }
        self.publish();
        said
    }

    /// The surface is now this large: the picture is drawn again at once
    /// rather than left stretched until the next one.
    pub fn resize(&mut self, width: u32, height: u32, now: Instant) -> Vec<Said> {
        let mut said = Vec::new();
        match self.presenter.resize(width, height) {
            Ok(()) => {
                if let Some((picture, _)) = &self.on_screen {
                    match self.presenter.present(picture) {
                        Ok(_) => self.counters.redrawn += 1,
                        Err(fault) => self.fault(fault, now, &mut said),
                    }
                }
            }
            Err(fault) => self.fault(fault, now, &mut said),
        }
        self.publish();
        said
    }

    fn decode(
        &mut self,
        frame: &AssembledFrame,
        now: Instant,
        said: &mut Vec<Said>,
    ) -> Option<DecodedFrame> {
        if self.recovery.whole(frame) == Verdict::Skip {
            self.counters.skipped += 1;
            self.hushed.skipped.note(&self.log, now, |times| {
                format!(
                    "frame {} of stream {} passed over, waiting for a key frame ({times} since \
                     last said)",
                    frame.frame, frame.stream
                )
            });
            return None;
        }
        match self.decoding.decode(frame, &self.presenter, &self.log) {
            Decoded::Picture {
                picture,
                took,
                replaced,
            } => {
                lock(&self.tally).decoded(now, took);
                self.counters.decoded += 1;
                self.recovery.decoded(frame.frame);
                self.unshown(now, replaced);
                picture
            }
            Decoded::Broken(reason) => {
                self.counters.broken += 1;
                self.hushed.broken.note(&self.log, now, |times| {
                    format!(
                        "frame {} of stream {} refused by the decoder: {reason} ({times} since \
                         last said)",
                        frame.frame, frame.stream
                    )
                });
                if let Some(recover) = self.recovery.broken(now) {
                    self.ask(recover, said);
                }
                None
            }
            Decoded::Refused(notice) => {
                self.counters.skipped += 1;
                if let Some(text) = notice {
                    said.push(Said::Notice(text));
                }
                None
            }
        }
    }

    fn show(
        &mut self,
        picture: DecodedFrame,
        captured_us: u32,
        now: Instant,
        said: &mut Vec<Said>,
    ) {
        let started = Instant::now();
        match self.presenter.present(&picture) {
            Ok(shown) => {
                let done = Instant::now();
                lock(&self.tally).shown(done, done - started, captured_us);
                self.counters.shown += 1;
                self.counters.checksum = shown.checksum;
                if !self.first_shown {
                    self.first_shown = true;
                    self.log.write("first picture shown");
                    said.push(Said::FirstPicture);
                }
                self.on_screen = Some((picture, captured_us));
            }
            Err(fault) => self.fault(fault, now, said),
        }
    }

    fn fault(&mut self, fault: Fault, now: Instant, said: &mut Vec<Said>) {
        match fault {
            Fault::Failed(reason) => {
                self.counters.undrawn += 1;
                self.hushed.undrawn.note(&self.log, now, |times| {
                    format!("a picture could not be drawn: {reason} ({times} since last said)")
                });
            }
            Fault::Lost(reason) => {
                self.log.write(&format!(
                    "the graphics card went away ({reason}): making everything again"
                ));
                // Everything made on the old card goes before the new one
                // is made.
                self.on_screen = None;
                self.decoding.close();
                self.counters.renewed += 1;
                match self.presenter.renew() {
                    Ok(()) => {
                        self.log.write("the graphics card is back");
                        if let Some(recover) = self.recovery.broken(now) {
                            self.ask(recover, said);
                        }
                    }
                    Err(reason) => {
                        self.log.write(&format!(
                            "the graphics card could not be made again: {reason}"
                        ));
                        said.push(Said::Failed(format!(
                            "La carte graphique de cet ordinateur ne répond plus : {reason}"
                        )));
                    }
                }
            }
        }
    }

    fn ask(&mut self, recover: Recover, said: &mut Vec<Said>) {
        self.counters.recovers += 1;
        self.log.debug(|| {
            format!(
                "asking for a key frame of stream {} after frame {}",
                recover.stream, recover.frame
            )
        });
        said.push(Said::Recover(recover));
    }

    fn unshown(&mut self, now: Instant, count: u64) {
        if count == 0 {
            return;
        }
        self.counters.unshown += count;
        lock(&self.tally).unshown(now, count);
        self.hushed.unshown.note(&self.log, now, |times| {
            format!("a newer picture took the place of one not yet shown ({times} since last said)")
        });
    }

    /// What the rest of the player reads: the counters, and where the
    /// picture is.
    fn publish(&self) {
        let mut tallies = lock(&self.tallies);
        tallies.assembly = self.assembler.counters();
        tallies.pictures = self.counters;
        drop(tallies);
        *lock(&self.rect) = self.presenter.picture_rect();
    }
}

/// What reaches the video thread.
#[derive(Debug)]
pub enum VideoInput {
    Datagram(Bytes),
    /// A wake-up: the surface changed size. The size itself waits in a
    /// slot of its own, only the newest one mattering.
    Resized,
}

/// The video thread, once its presenter is made, until the link lets go
/// of `input`. Hands what `Video` says to `tell`, which answers whether
/// to go on.
pub fn run<P: Presenter>(
    mut video: Video<P>,
    input: &Receiver<VideoInput>,
    size: &Mutex<Option<(u32, u32)>>,
    mut tell: impl FnMut(Said) -> bool,
) {
    loop {
        let first = match video.next_wakeup() {
            Some(at) => input.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => input.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match first {
            Ok(message) => take(&mut video, message),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for _ in 0..BURST {
            match input.try_recv() {
                Ok(message) => take(&mut video, message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let now = Instant::now();
        let mut said = Vec::new();
        // Looked at on every turn, the wake-up having possibly found the
        // way full.
        let resized = lock(size).take();
        if let Some((width, height)) = resized {
            said.extend(video.resize(width, height, now));
        }
        said.extend(video.settle(now));
        for one in said {
            if !tell(one) {
                return;
            }
        }
    }
}

fn take<P: Presenter>(video: &mut Video<P>, message: VideoInput) {
    match message {
        VideoInput::Datagram(datagram) => video.push(&datagram, Instant::now()),
        VideoInput::Resized => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::{Headless, Shown};
    use crate::stats::Clock;
    use crate::testing::{self, Encoded, HEIGHT, WIDTH, h264};
    use zyr_codec::DecodeOutput;
    use zyr_media::codec::CodecSet;

    /// Records the fingerprint of every picture drawn, and can be told
    /// to lose its graphics card.
    #[derive(Default)]
    struct Recording {
        inner: Headless,
        drawn: Vec<u64>,
        lose_next: bool,
        renewed: u32,
    }

    impl Presenter for Recording {
        fn output(&self) -> Option<DecodeOutput> {
            Some(DecodeOutput::Cpu)
        }

        fn decodable(&self, ff: &Arc<Ffmpeg>) -> CodecSet {
            self.inner.decodable(ff)
        }

        fn present(&mut self, picture: &DecodedFrame) -> Result<Shown, Fault> {
            if std::mem::take(&mut self.lose_next) {
                return Err(Fault::Lost("DXGI_ERROR_DEVICE_REMOVED".to_string()));
            }
            let shown = self.inner.present(picture)?;
            self.drawn.extend(shown.checksum);
            Ok(shown)
        }

        fn resize(&mut self, width: u32, height: u32) -> Result<(), Fault> {
            self.inner.resize(width, height)
        }

        fn picture_rect(&self) -> Option<Rect> {
            self.inner.picture_rect()
        }

        fn renew(&mut self) -> Result<(), String> {
            self.renewed += 1;
            Ok(())
        }
    }

    fn video(presenter: Recording) -> Video<Recording> {
        Video::new(
            testing::ffmpeg(),
            presenter,
            Arc::new(Mutex::new(Tally::new(Clock::new()))),
            Arc::new(Mutex::new(Tallies::default())),
            Arc::new(Mutex::new(None)),
            testing::log(TAG),
        )
    }

    fn frame_in(video: &mut Video<Recording>, stream: u16, n: u32, packet: &Encoded, now: Instant) {
        for datagram in testing::datagrams(stream, n, packet, n * 16_000) {
            video.push(&datagram, now);
        }
    }

    fn recovers(said: &[Said]) -> Vec<Recover> {
        said.iter()
            .filter_map(|one| match one {
                Said::Recover(recover) => Some(*recover),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_lost_frame_is_asked_for_once_then_again_and_the_rest_waits_for_a_key_frame() {
        let (packets, looks) = h264(12, &[9]);
        let mut video = video(Recording::default());
        let at = Instant::now();
        let mut first_pictures = 0;
        let mut asked = Vec::new();
        let ms = |n: u64| at + Duration::from_millis(n);

        for n in 0..5u32 {
            frame_in(
                &mut video,
                1,
                n,
                &packets[n as usize],
                ms(u64::from(n) * 16),
            );
            let said = video.settle(ms(u64::from(n) * 16));
            first_pictures += said.iter().filter(|s| **s == Said::FirstPicture).count();
        }
        // Frame 5 never arrives. Frame 6 does, and waits out the grace
        // left to a late packet of 5 before 5 is given up.
        frame_in(&mut video, 1, 6, &packets[6], ms(96));
        assert!(video.settle(ms(96)).is_empty());
        asked.extend(recovers(&video.settle(ms(100))));
        assert_eq!(
            asked,
            [Recover {
                stream: 1,
                frame: 4
            }]
        );
        for n in 7..9u32 {
            frame_in(
                &mut video,
                1,
                n,
                &packets[n as usize],
                ms(u64::from(n) * 16),
            );
            asked.extend(recovers(&video.settle(ms(u64::from(n) * 16))));
        }
        assert_eq!(asked.len(), 1, "one request while it is fresh");
        assert_eq!(video.next_wakeup(), Some(ms(350)));
        asked.extend(recovers(&video.settle(ms(349))));
        assert_eq!(asked.len(), 1);
        asked.extend(recovers(&video.settle(ms(350))));
        assert_eq!(asked.len(), 2, "asked again after a quarter second");
        for n in 9..12u32 {
            frame_in(
                &mut video,
                1,
                n,
                &packets[n as usize],
                ms(400 + u64::from(n)),
            );
            asked.extend(recovers(&video.settle(ms(400 + u64::from(n)))));
        }
        asked.extend(recovers(&video.settle(ms(2_000))));
        assert_eq!(asked.len(), 2, "the key frame ends the asking");

        assert_eq!(first_pictures, 1);
        let drawn = &video.presenter.drawn;
        assert_eq!(drawn[..5], looks[..5]);
        assert_eq!(drawn[5..], looks[9..]);
        assert_eq!(video.counters.skipped, 3);
        assert_eq!(video.counters.recovers, 2);
        assert_eq!(video.counters.shown, 8);
        assert_eq!(video.assembler.counters().frames_lost, 1);
    }

    #[test]
    fn frames_that_arrive_together_are_all_decoded_and_only_the_newest_drawn() {
        let (packets, looks) = h264(4, &[]);
        let mut video = video(Recording::default());
        let at = Instant::now();
        for (n, packet) in packets.iter().enumerate() {
            frame_in(&mut video, 1, n as u32, packet, at);
        }
        let said = video.settle(at);
        assert_eq!(said, [Said::FirstPicture]);
        assert_eq!(video.presenter.drawn, [looks[3]]);
        assert_eq!(video.counters.decoded, 4);
        assert_eq!(video.counters.unshown, 3);
        assert_eq!(video.counters.checksum, Some(looks[3]));
    }

    #[test]
    fn a_new_stream_gets_a_new_decoder_and_starts_at_its_key_frame() {
        let (packets, looks) = h264(3, &[]);
        let mut video = video(Recording::default());
        let at = Instant::now();
        frame_in(&mut video, 1, 0, &packets[0], at);
        frame_in(&mut video, 1, 1, &packets[1], at);
        video.settle(at);
        // Stream 2 starts over at frame 0, but its key frame is lost:
        // frame 1 is passed over and the key frame asked for, of
        // stream 2.
        frame_in(&mut video, 2, 1, &packets[1], at);
        let later = at + Duration::from_millis(5);
        assert_eq!(
            recovers(&video.settle(later)),
            [Recover {
                stream: 2,
                frame: 0
            }]
        );
        assert_eq!(video.counters.skipped, 1);
        // Stream 3 begins with its key frame and plays.
        for (n, packet) in packets.iter().enumerate() {
            frame_in(&mut video, 3, n as u32, packet, later);
            video.settle(later);
        }
        assert_eq!(video.presenter.drawn.last(), Some(&looks[2]));
        assert_eq!(video.next_wakeup(), None, "stream 3 needs nothing");
    }

    #[test]
    fn a_graphics_card_lost_is_made_again_and_a_key_frame_asked_for() {
        let (packets, _) = h264(3, &[]);
        let mut video = video(Recording::default());
        let at = Instant::now();
        frame_in(&mut video, 1, 0, &packets[0], at);
        video.settle(at);
        video.presenter.lose_next = true;
        frame_in(&mut video, 1, 1, &packets[1], at);
        assert_eq!(
            recovers(&video.settle(at)),
            [Recover {
                stream: 1,
                frame: 1
            }]
        );
        assert_eq!(video.presenter.renewed, 1);
        assert_eq!(video.counters.renewed, 1);
        // Frame 2 refers to what the lost decoder held: passed over.
        frame_in(&mut video, 1, 2, &packets[2], at);
        video.settle(at);
        assert_eq!(video.counters.skipped, 1);
        assert_eq!(video.presenter.drawn.len(), 1);
    }

    #[test]
    fn a_resize_draws_the_picture_again_where_it_now_goes() {
        let (packets, looks) = h264(1, &[]);
        let mut video = video(Recording::default());
        let at = Instant::now();
        frame_in(&mut video, 1, 0, &packets[0], at);
        video.settle(at);
        assert_eq!(*lock(&video.rect), Some((0, 0, WIDTH, HEIGHT)));
        video.resize(640, 640, at);
        assert_eq!(video.counters.redrawn, 1);
        assert_eq!(video.presenter.drawn, [looks[0], looks[0]]);
        assert_eq!(*lock(&video.rect), Some((0, 80, 640, 480)));
    }

    #[test]
    fn garbage_is_counted_and_never_stops_the_picture() {
        let (packets, _) = h264(2, &[]);
        let mut video = video(Recording::default());
        let at = Instant::now();
        video.push(&[1, 2, 3], at);
        video.push(&[], at);
        frame_in(&mut video, 1, 0, &packets[0], at);
        frame_in(&mut video, 1, 1, &packets[1], at);
        video.settle(at);
        assert_eq!(video.assembler.counters().malformed, 2);
        assert_eq!(video.counters.decoded, 2);
    }
}
