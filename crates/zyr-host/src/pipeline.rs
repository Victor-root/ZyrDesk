//! The pictures: capture, conversion, encoding and packets, on one
//! thread.
//!
//! The thread waits on the screen, and the cadence the viewer asked for
//! decides which images go (see `zyr_media::pace`). An image that goes is
//! drawn straight into the encoder's own frame, encoded, cut into
//! datagrams with their parity, and handed to the link. Everything is
//! done in that one place and in that order, so a picture never waits on
//! another thread.
//!
//! A key frame goes at the start of each stream and when the player asks
//! for one, never more often than every 100 ms, and never on a timer. An
//! encoder is built again only for what it cannot take in place: a new
//! size, rate of pictures or codec, or a new rate of bits it cannot
//! change between two frames. Each build starts a new stream, which the
//! player hears about before its first packet.
//!
//! The encoders are tried at start, on the graphics card the screen is
//! captured on, while the engine waits for the player. The best one for
//! the codec agreed is used; one that fails, at the start or later, is
//! left for the next, and the viewer is told.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use zyr_codec::{Backend, EncoderConfig, Ffmpeg, Input, VideoEncoder, probe};
use zyr_media::codec::{CodecSet, VideoCodec, negotiate};
use zyr_media::control::{NoticeKind, ToPlayer, Wanted};
use zyr_media::pace::{Cadence, Due, Now};
use zyr_media::service::{Display, ToService};
use zyr_media::video::{DEFAULT_FEC_PERCENT, OutgoingFrame, Packetizer, Packets};
use zyr_proto::log::Log;

use crate::clock::HostClock;
use crate::input;
use crate::link::{Outbox, Sent};
use crate::parts::{Aimed, Captured, Drawing, Feed, MakeScreen, Screen, ScreenError};
use crate::picture::{Mapping, Rect, Size, picture_size, placement};
use crate::session::Event;
use crate::sound::OPUS_BITRATE;
use crate::throttle::Throttle;

/// Fewest milliseconds between two key frames.
pub(crate) const KEY_FLOOR: Duration = Duration::from_millis(100);

/// Longest wait on the screen before looking whether the engine said
/// something.
const LOOKING: Duration = Duration::from_millis(10);

/// How often the screens are looked at while nobody watches.
const IDLE_LOOK: Duration = Duration::from_secs(2);

/// How often the counts are written.
const REPORT_EVERY: Duration = Duration::from_secs(10);

/// Fewest kilobits a second an encoder is given, whatever is asked.
const LEAST_KBPS: u32 = 500;

/// What the pipeline is told.
pub(crate) enum Command {
    /// Which screen to film: a display id, or "" for the main one.
    Film {
        display: String,
    },
    /// Start streaming, in datagrams of at most `datagram_budget` bytes.
    Start {
        wanted: Wanted,
        decodable: CodecSet,
        datagram_budget: u16,
    },
    /// The viewer changed what it wants.
    Change {
        wanted: Wanted,
    },
    /// The player lost a frame of that stream and asks for a key frame.
    Recover {
        stream: u16,
    },
    Quit,
}

/// What the pipeline tells the engine.
pub(crate) enum Report {
    /// The screen could not be had at all: nothing can be filmed.
    NoScreen(String),
    /// The screen filmed now.
    Aimed(Display),
    /// The encoders were tried.
    Probed {
        encodable: CodecSet,
        encoders: String,
        displays: Vec<Display>,
    },
    /// The session cannot go on: the viewer is to be told this.
    Fatal { kind: NoticeKind, text: String },
}

/// What the pipeline shares with the rest of the engine.
pub(crate) struct Shared {
    pub(crate) ffmpeg: Arc<Ffmpeg>,
    pub(crate) outbox: Outbox,
    pub(crate) events: mpsc::Sender<Event>,
    pub(crate) input: mpsc::Sender<input::Command>,
    pub(crate) clock: HostClock,
    pub(crate) log: Log,
}

pub(crate) fn start(
    make: MakeScreen,
    shared: Shared,
) -> std::io::Result<(mpsc::Sender<Command>, JoinHandle<()>)> {
    let (commands, received) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("engine pictures".to_string())
        .spawn(move || match make() {
            Ok(screen) => Pipeline::new(screen, shared).run(&received),
            Err(e) => {
                shared
                    .log
                    .write(&format!("the screen cannot be captured: {e}"));
                let _ = shared.events.send(Event::Pipeline(Report::NoScreen(e.0)));
                // Nothing to do but wait to be told to stop.
                while !matches!(received.recv(), Ok(Command::Quit) | Err(_)) {}
            }
        })?;
    Ok((commands, thread))
}

/// Kilobits a second the encoder is given when `asked` is the rate the
/// viewer wants on the wire: the parity added to each frame and the
/// sound beside the pictures come out of it.
pub(crate) fn encoder_kbps(asked: u32, fec_percent: u8, sound: bool) -> u32 {
    let pictures = u64::from(asked) * 100 / (100 + u64::from(fec_percent));
    let sound_kbps = if sound { OPUS_BITRATE / 1000 } else { 0 };
    (pictures as u32).saturating_sub(sound_kbps).max(LEAST_KBPS)
}

/// What an encoder is opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) codec: VideoCodec,
    pub(crate) picture: Size,
    pub(crate) fps: u32,
    pub(crate) kbps: u32,
}

/// What a change of settings asks of the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plan {
    Keep,
    /// Only the rate of bits changed.
    Rate(u32),
    /// A new encoder, and a new stream.
    Rebuild,
}

pub(crate) fn plan(now: &Settings, next: &Settings) -> Plan {
    if now.codec != next.codec || now.picture != next.picture || now.fps != next.fps {
        Plan::Rebuild
    } else if now.kbps != next.kbps {
        Plan::Rate(next.kbps)
    } else {
        Plan::Keep
    }
}

/// Key frames asked for, and the floor between two of them.
#[derive(Debug, Clone, Default)]
pub(crate) struct KeyFrames {
    wanted: bool,
    last: Option<Instant>,
}

impl KeyFrames {
    pub(crate) fn ask(&mut self) {
        self.wanted = true;
    }

    /// Whether the next picture is to be a key frame.
    pub(crate) fn due(&self, now: Instant) -> bool {
        self.wanted && self.last.is_none_or(|last| now >= last + KEY_FLOOR)
    }

    /// When one asked for may go, if it has to wait for the floor.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.last
            .filter(|_| self.wanted)
            .map(|last| last + KEY_FLOOR)
    }

    /// Whether the picture going now is to be a key frame; it counts as
    /// sent if so.
    pub(crate) fn take(&mut self, now: Instant) -> bool {
        let due = self.due(now);
        if due {
            self.wanted = false;
            self.last = Some(now);
        }
        due
    }

    /// The encoder made a key frame of its own accord: it answers what
    /// was asked, and counts for the floor.
    pub(crate) fn made(&mut self, now: Instant) {
        self.wanted = false;
        self.last = Some(now);
    }
}

/// An image on its way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Going {
    /// Captured then, and shown for the first time.
    Fresh(Instant),
    /// The screen did not change: the last image again.
    Repeat,
}

/// The encoder at work, and its stream.
struct Encoding {
    encoder: VideoEncoder,
    backend: Backend,
    feed: Feed,
    settings: Settings,
    stream: u16,
    next_frame: u32,
    placement: Rect,
}

/// What the viewer asked for, once streaming.
struct Streaming {
    wanted: Wanted,
    decodable: CodecSet,
    packetizer: Packetizer,
    cadence: Cadence,
    /// The picture the cadence holds back until its turn.
    held: Option<Going>,
    keys: KeyFrames,
    encoding: Option<Encoding>,
    /// Whether the viewer was told its codec could not be had.
    told_codec: bool,
}

#[derive(Debug, Default)]
struct Counts {
    captured: u64,
    pointer_only: u64,
    /// Held back by the cadence, then replaced by a newer image.
    held_replaced: u64,
    fresh: u64,
    repeats: u64,
    keys: u64,
    recovers: u64,
    recovers_ignored: u64,
    bytes: u64,
    datagrams: u64,
    /// Frames the link had no room for.
    crowded: u64,
    too_large: u64,
    draw_failed: u64,
    streams: u64,
    /// Time from drawing to the packets being handed over, this interval.
    work_us: u64,
    work_max_us: u64,
    worked: u64,
}

impl Counts {
    fn said(&self) -> String {
        let average = self.work_us.checked_div(self.worked).unwrap_or(0);
        format!(
            "pictures: {} captured, {} pointer only, {} replaced while held, {} sent fresh, \
             {} repeats, {} key frames, {} recovers ({} for older streams), {} bytes in {} \
             datagrams, {} dropped on a full link, {} too large, {} not drawn, {} streams; \
             draw and encode {:.2} ms on average, {:.2} ms at most",
            self.captured,
            self.pointer_only,
            self.held_replaced,
            self.fresh,
            self.repeats,
            self.keys,
            self.recovers,
            self.recovers_ignored,
            self.bytes,
            self.datagrams,
            self.crowded,
            self.too_large,
            self.draw_failed,
            self.streams,
            average as f64 / 1000.0,
            self.work_max_us as f64 / 1000.0,
        )
    }
}

struct Pipeline {
    screen: Box<dyn Screen>,
    shared: Shared,
    /// The screen asked for, "" for the main one.
    film: String,
    aimed: Option<Aimed>,
    /// The screens last said to the service.
    displays: Vec<Display>,
    /// The encoders that work, best first, less those that failed.
    encoders: Vec<(VideoCodec, Backend)>,
    /// The device they were tried on.
    probed_on: Option<u64>,
    streaming: Option<Streaming>,
    last_stream: u16,
    counts: Counts,
    /// Whether the viewer was told the capture fails, since it last
    /// worked.
    told_capture: bool,
    troubles: Throttle,
    crowded: Throttle,
}

impl Pipeline {
    fn new(screen: Box<dyn Screen>, shared: Shared) -> Self {
        Self {
            screen,
            shared,
            film: String::new(),
            aimed: None,
            displays: Vec::new(),
            encoders: Vec::new(),
            probed_on: None,
            streaming: None,
            last_stream: 0,
            counts: Counts::default(),
            told_capture: false,
            troubles: Throttle::new(REPORT_EVERY),
            crowded: Throttle::new(REPORT_EVERY),
        }
    }

    fn run(mut self, commands: &mpsc::Receiver<Command>) {
        self.aim();
        self.probe();
        self.displays = self.screen.displays();
        let encodable: CodecSet = self.encoders.iter().map(|(codec, _)| *codec).collect();
        let encoders = self
            .encoders
            .iter()
            .filter_map(|(codec, backend)| backend.encoder_name(*codec))
            .collect::<Vec<_>>()
            .join(" ");
        self.report(Report::Probed {
            encodable,
            encoders,
            displays: self.displays.clone(),
        });

        let mut report = Instant::now() + REPORT_EVERY;
        loop {
            let command = if self.is_streaming() {
                match commands.try_recv() {
                    Ok(command) => Some(command),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(Command::Quit),
                }
            } else {
                match commands.recv_timeout(IDLE_LOOK) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => {
                        self.look_at_the_screens();
                        None
                    }
                    Err(RecvTimeoutError::Disconnected) => Some(Command::Quit),
                }
            };
            match command {
                Some(command) => {
                    if !self.obey(command) {
                        break;
                    }
                }
                None if self.is_streaming() => self.step(),
                None => {}
            }
            let now = Instant::now();
            if now >= report {
                self.shared.log.debug(|| self.counts.said());
                self.counts.work_us = 0;
                self.counts.work_max_us = 0;
                self.counts.worked = 0;
                report = now + REPORT_EVERY;
            }
        }
        self.shared.log.write(&self.counts.said());
    }

    fn is_streaming(&self) -> bool {
        self.streaming
            .as_ref()
            .is_some_and(|streaming| streaming.encoding.is_some())
    }

    fn report(&self, report: Report) {
        let _ = self.shared.events.send(Event::Pipeline(report));
    }

    /// Does what the engine says; false when told to stop.
    fn obey(&mut self, command: Command) -> bool {
        match command {
            Command::Film { display } => {
                self.film = display;
                self.aim();
            }
            Command::Start {
                wanted,
                decodable,
                datagram_budget,
            } => {
                self.streaming = Some(Streaming {
                    wanted,
                    decodable,
                    packetizer: Packetizer::new(usize::from(datagram_budget), DEFAULT_FEC_PERCENT),
                    cadence: Cadence::new(u32::from(wanted.fps), wanted.steady),
                    held: None,
                    keys: KeyFrames::default(),
                    encoding: None,
                    told_codec: false,
                });
                self.open_stream();
            }
            Command::Change { wanted } => self.change(wanted),
            Command::Recover { stream } => self.recover(stream),
            Command::Quit => return false,
        }
        true
    }

    /// A key frame for the player, if it lost a frame of the current
    /// stream: one of an older stream is past already.
    fn recover(&mut self, stream: u16) {
        let Some(streaming) = &mut self.streaming else {
            return;
        };
        if streaming
            .encoding
            .as_ref()
            .is_some_and(|encoding| encoding.stream == stream)
        {
            self.counts.recovers += 1;
            streaming.keys.ask();
        } else {
            self.counts.recovers_ignored += 1;
        }
    }

    /// Aims the capture at the screen asked for.
    fn aim(&mut self) {
        match self.screen.aim(&self.film) {
            Ok(aimed) => self.took(aimed),
            Err(e) => self.screen_trouble(&e),
        }
    }

    /// Tries the encoders on the device the screen is captured on.
    fn probe(&mut self) {
        let started = Instant::now();
        self.encoders = probe(
            &self.shared.ffmpeg,
            &self.screen.encoder_input(),
            self.screen.vendor(),
        );
        self.probed_on = self.aimed.as_ref().map(|aimed| aimed.device);
        let found = self
            .encoders
            .iter()
            .map(|(codec, backend)| format!("{} by {}", codec.name(), backend.name()))
            .collect::<Vec<_>>();
        self.shared.log.write(&format!(
            "encoders tried in {} ms on the graphics card ({:?}): {}",
            started.elapsed().as_millis(),
            self.screen.vendor(),
            if found.is_empty() {
                "none works".to_string()
            } else {
                found.join(", ")
            }
        ));
    }

    /// Takes in what the capture is aimed at now.
    fn took(&mut self, aimed: Aimed) {
        let before = self.aimed.replace(aimed.clone());
        self.told_capture = false;
        let display = &aimed.display;
        self.shared.log.write(&format!(
            "filming {} ({}), {}x{} at {},{}",
            display.name,
            display.id,
            aimed.area.width,
            aimed.area.height,
            aimed.area.x,
            aimed.area.y
        ));
        self.shared.outbox.service(&ToService::Filming {
            display: display.id.clone(),
            width: display.width,
            height: display.height,
        });
        self.report(Report::Aimed(display.clone()));
        if let Some(before) = &before
            && before.display.id != display.id
            && !self.film.is_empty()
            && display.id != self.film
            && self.streaming.is_some()
        {
            self.shared.outbox.player(&ToPlayer::Notice {
                kind: NoticeKind::DisplayChanged,
                text: format!(
                    "L'écran filmé n'est plus là : la session montre maintenant {}.",
                    display.name
                ),
            });
        }
        let displays = self.screen.displays();
        if displays != self.displays && self.probed_on.is_some() {
            self.shared
                .outbox
                .service(&ToService::Displays(displays.clone()));
        }
        self.displays = displays;
        let new_device = self.probed_on.is_some_and(|device| device != aimed.device);
        if new_device {
            self.shared
                .log
                .write("the screen is captured on another device: trying the encoders again");
            self.probe();
        }
        if !self.is_streaming() {
            return;
        }
        let picture = self.picture();
        let rebuild = new_device
            || self
                .streaming
                .as_ref()
                .and_then(|streaming| streaming.encoding.as_ref())
                .is_some_and(|encoding| encoding.settings.picture != picture);
        if rebuild {
            self.open_stream();
        } else {
            self.place();
        }
    }

    /// The picture for the screen filmed and what the viewer asked.
    fn picture(&self) -> Size {
        let screen = self
            .aimed
            .as_ref()
            .map_or(Size::new(0, 0), |aimed| aimed.area.size());
        let asked = self
            .streaming
            .as_ref()
            .map_or(Size::new(0, 0), |streaming| {
                Size::new(
                    u32::from(streaming.wanted.width),
                    u32::from(streaming.wanted.height),
                )
            });
        picture_size(screen, asked)
    }

    /// Places the screen's image in the encoder's picture, and tells the
    /// input where that lies on the desktop.
    fn place(&mut self) {
        let (Some(aimed), Some(streaming)) = (&self.aimed, &mut self.streaming) else {
            return;
        };
        let Some(encoding) = &mut streaming.encoding else {
            return;
        };
        encoding.placement = placement(aimed.area.size(), encoding.settings.picture);
        let _ = self.shared.input.send(input::Command::Map(Mapping {
            picture: encoding.settings.picture,
            placement: encoding.placement,
            screen: aimed.area,
            desktop: aimed.desktop,
        }));
    }

    /// The settings the viewer's wishes come to, for `codec`.
    fn settings(&self, codec: VideoCodec, wanted: &Wanted) -> Settings {
        Settings {
            codec,
            picture: self.picture(),
            fps: u32::from(wanted.fps.max(1)),
            kbps: encoder_kbps(wanted.bitrate_kbps, DEFAULT_FEC_PERCENT, wanted.audio),
        }
    }

    /// The codec for the session: what both sides can do, as close to
    /// what was asked as can be.
    fn codec(&mut self) -> Option<VideoCodec> {
        let streaming = self.streaming.as_mut()?;
        let encodable: CodecSet = self.encoders.iter().map(|(codec, _)| *codec).collect();
        let chosen = negotiate(streaming.wanted.codec, encodable, streaming.decodable)?;
        if let Some(asked) = streaming.wanted.codec.codec()
            && asked != chosen
            && !streaming.told_codec
        {
            streaming.told_codec = true;
            self.shared.outbox.player(&ToPlayer::Notice {
                kind: NoticeKind::NoEncoder,
                text: format!(
                    "Le codec {} n'est pas possible pour cette session : l'image passe en {}.",
                    asked.name(),
                    chosen.name()
                ),
            });
        }
        Some(chosen)
    }

    /// Builds the encoder and starts a new stream, with the best encoder
    /// that opens, and says what it serves.
    fn open_stream(&mut self) {
        if let Some(streaming) = &mut self.streaming {
            streaming.encoding = None;
        }
        if self.aimed.is_none() {
            // It failed at the start: once more, now that it is needed.
            self.aim();
        }
        if self.aimed.is_none() {
            self.fatal(
                NoticeKind::CaptureTrouble,
                "L'écran de l'ordinateur d'en face ne peut pas être filmé.".to_string(),
            );
            return;
        }
        loop {
            let Some(codec) = self.codec() else {
                self.fatal(
                    NoticeKind::NoEncoder,
                    "Aucun encodeur de l'ordinateur d'en face ne produit une image que cet \
                     ordinateur sait décoder."
                        .to_string(),
                );
                return;
            };
            let Some(streaming) = &self.streaming else {
                return;
            };
            let settings = self.settings(codec, &streaming.wanted);
            let backends: Vec<Backend> = self
                .encoders
                .iter()
                .filter(|(each, _)| *each == codec)
                .map(|(_, backend)| *backend)
                .collect();
            for backend in backends {
                match self.open(backend, settings) {
                    Ok(encoding) => {
                        self.started(encoding);
                        return;
                    }
                    Err(e) => self.left_out(codec, backend, &e),
                }
            }
        }
    }

    /// Opens one encoder.
    fn open(&self, backend: Backend, settings: Settings) -> Result<Encoding, String> {
        let input = if backend == Backend::Software {
            Input::Cpu
        } else {
            self.screen.encoder_input()
        };
        let feed = match input {
            Input::Cpu => Feed::Memory,
            #[cfg(windows)]
            Input::D3d11 { .. } => Feed::Texture,
        };
        let encoder = VideoEncoder::open(
            &self.shared.ffmpeg,
            EncoderConfig {
                codec: settings.codec,
                width: settings.picture.width,
                height: settings.picture.height,
                fps: settings.fps,
                bitrate_kbps: settings.kbps,
                backend,
                input,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Encoding {
            encoder,
            backend,
            feed,
            settings,
            stream: 0,
            next_frame: 0,
            placement: Rect::new(0, 0, settings.picture.width, settings.picture.height),
        })
    }

    /// An encoder that will not do is left out for the rest of the
    /// session, and the viewer told.
    fn left_out(&mut self, codec: VideoCodec, backend: Backend, why: &str) {
        self.shared.log.write(&format!(
            "{} by {} left out: {why}",
            codec.name(),
            backend.name()
        ));
        self.encoders.retain(|each| *each != (codec, backend));
        let next = self
            .encoders
            .iter()
            .find(|(each, _)| *each == codec)
            .or_else(|| self.encoders.first())
            .map(|(_, backend)| backend.name());
        if let Some(next) = next {
            self.shared.outbox.player(&ToPlayer::Notice {
                kind: NoticeKind::EncoderTrouble,
                text: format!(
                    "L'encodeur {} de l'ordinateur d'en face a échoué : l'image passe par {next}.",
                    backend.name()
                ),
            });
        }
    }

    /// A new stream begins with this encoder.
    ///
    /// The input hears where the picture lies before the player hears of
    /// the stream: a pointer the viewer places on it always finds it.
    fn started(&mut self, mut encoding: Encoding) {
        self.last_stream = self.last_stream.wrapping_add(1);
        encoding.stream = self.last_stream;
        self.counts.streams += 1;
        let (stream, backend, settings) = (encoding.stream, encoding.backend, encoding.settings);
        let Some(streaming) = &mut self.streaming else {
            return;
        };
        streaming.keys.ask();
        streaming.encoding = Some(encoding);
        self.place();
        self.shared.log.write(&format!(
            "stream {stream} started: {}x{} at {} fps, {} by {}, {} kb/s",
            settings.picture.width,
            settings.picture.height,
            settings.fps,
            settings.codec.name(),
            backend.name(),
            settings.kbps
        ));
        self.shared.outbox.player(&ToPlayer::Streaming {
            stream,
            codec: settings.codec,
            width: u16::try_from(settings.picture.width).unwrap_or(u16::MAX),
            height: u16::try_from(settings.picture.height).unwrap_or(u16::MAX),
            fps: u16::try_from(settings.fps).unwrap_or(u16::MAX),
        });
        self.serving(&settings);
    }

    fn serving(&self, settings: &Settings) {
        self.shared.outbox.service(&ToService::Serving {
            kbps: settings.kbps,
            fps: u16::try_from(settings.fps).unwrap_or(u16::MAX),
        });
    }

    /// The session cannot go on.
    fn fatal(&mut self, kind: NoticeKind, text: String) {
        self.shared.log.write(&format!("pictures stop: {text}"));
        self.streaming = None;
        self.report(Report::Fatal { kind, text });
    }

    fn change(&mut self, wanted: Wanted) {
        let Some(streaming) = &mut self.streaming else {
            return;
        };
        let before = std::mem::replace(&mut streaming.wanted, wanted);
        if before.fps != wanted.fps {
            streaming.cadence.set_fps(u32::from(wanted.fps));
        }
        if before.steady != wanted.steady {
            streaming.cadence.set_steady(wanted.steady);
        }
        if before.codec != wanted.codec {
            streaming.told_codec = false;
        }
        let Some(now) = streaming
            .encoding
            .as_ref()
            .map(|encoding| encoding.settings)
        else {
            return;
        };
        let Some(codec) = self.codec() else {
            self.open_stream();
            return;
        };
        let next = self.settings(codec, &wanted);
        match plan(&now, &next) {
            Plan::Keep => {}
            Plan::Rebuild => self.open_stream(),
            Plan::Rate(kbps) => self.rate(kbps),
        }
    }

    /// A new rate of bits, in place when the encoder takes it.
    fn rate(&mut self, kbps: u32) {
        let Some(encoding) = self
            .streaming
            .as_mut()
            .and_then(|streaming| streaming.encoding.as_mut())
        else {
            return;
        };
        match encoding.encoder.set_bitrate(kbps) {
            Ok(zyr_codec::Applied::InPlace) => {
                encoding.settings.kbps = kbps;
                let settings = encoding.settings;
                self.shared
                    .log
                    .write(&format!("stream {} now at {kbps} kb/s", encoding.stream));
                self.serving(&settings);
            }
            Ok(zyr_codec::Applied::NeedsRebuild) => self.open_stream(),
            Err(e) => {
                let (codec, backend) = (encoding.settings.codec, encoding.backend);
                self.left_out(codec, backend, &e.to_string());
                self.open_stream();
            }
        }
    }

    /// One turn of the streaming loop: what is due goes, else the screen
    /// is waited on until something is.
    fn step(&mut self) {
        let now = Instant::now();
        let Some(streaming) = &mut self.streaming else {
            return;
        };
        if let Some(due) = streaming.cadence.due(now) {
            let going = match due {
                Due::EmitHeld => streaming.held.take().unwrap_or(Going::Repeat),
                Due::Repeat => Going::Repeat,
            };
            self.emit(going);
            return;
        }
        // A key frame asked for goes at once: whatever the screen has, or
        // the latest image again. One held by the cadence carries it when
        // it goes.
        let key_now = streaming.keys.due(now) && streaming.held.is_none();
        let until = if key_now {
            now
        } else {
            [
                streaming.cadence.next_wakeup(),
                streaming.keys.deadline(),
                Some(now + LOOKING),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(now + LOOKING)
        };
        let draw_pointer = streaming.wanted.draw_pointer;
        match self.screen.wait(until) {
            Ok(Captured::Image { at }) => self.captured(Going::Fresh(at), at),
            Ok(Captured::Pointer) if draw_pointer => {
                let at = Instant::now();
                self.captured(Going::Fresh(at), at);
            }
            Ok(Captured::Pointer) => {
                self.counts.pointer_only += 1;
                if key_now {
                    self.captured(Going::Repeat, now);
                }
            }
            Ok(Captured::Nothing) => {
                if key_now {
                    self.captured(Going::Repeat, now);
                }
            }
            Ok(Captured::Moved(aimed)) => self.took(aimed),
            Err(e) => {
                self.screen_trouble(&e);
                thread::sleep(LOOKING);
            }
        }
    }

    /// A picture to send: now, or when the cadence says. The latest image
    /// sent again to carry a key frame goes through the cadence like a
    /// new one, so that repeats and new images keep their pace after it.
    fn captured(&mut self, going: Going, at: Instant) {
        if let Going::Fresh(_) = going {
            self.counts.captured += 1;
            self.told_capture = false;
        }
        let Some(streaming) = &mut self.streaming else {
            return;
        };
        match streaming.cadence.on_captured(at) {
            Now::Emit => {
                streaming.held = None;
                self.emit(going);
            }
            Now::Hold => {
                if streaming.held.replace(going).is_some() {
                    self.counts.held_replaced += 1;
                }
            }
        }
    }

    /// Draws, encodes and sends one picture.
    fn emit(&mut self, going: Going) {
        let started = Instant::now();
        let Some(streaming) = &mut self.streaming else {
            return;
        };
        let Some(encoding) = &mut streaming.encoding else {
            return;
        };
        let drawing = Drawing {
            placement: encoding.placement,
            pointer: streaming.wanted.draw_pointer,
        };
        let frame = match self.screen.draw(&encoding.encoder, encoding.feed, &drawing) {
            Ok(frame) => frame,
            Err(e) => {
                self.counts.draw_failed += 1;
                self.screen_trouble(&e);
                return;
            }
        };
        let key = streaming.keys.take(started);
        if let Err(e) = encoding.encoder.encode(frame, key) {
            self.encoder_failed(&e.to_string());
            return;
        }
        let captured = match going {
            Going::Fresh(at) => {
                self.counts.fresh += 1;
                at
            }
            Going::Repeat => {
                self.counts.repeats += 1;
                started
            }
        };
        loop {
            let Some(streaming) = &mut self.streaming else {
                return;
            };
            let Some(encoding) = &mut streaming.encoding else {
                return;
            };
            let packet = match encoding.encoder.receive() {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(e) => {
                    self.encoder_failed(&e.to_string());
                    return;
                }
            };
            if packet.key {
                self.counts.keys += 1;
                streaming.keys.made(started);
            } else if key {
                self.shared
                    .log
                    .write("the encoder did not make the key frame it was asked for");
            }
            let frame = encoding.next_frame;
            encoding.next_frame = frame.wrapping_add(1);
            let mut packets = Packets::new();
            let sent_at = Instant::now();
            let cut = streaming.packetizer.packetize(
                &OutgoingFrame {
                    data: &packet.data,
                    key: packet.key,
                    repeat: going == Going::Repeat,
                    stream: encoding.stream,
                    frame,
                    captured_us: self.shared.clock.micros(captured) as u32,
                    host_latency_us: u32::try_from(
                        sent_at.saturating_duration_since(captured).as_micros(),
                    )
                    .unwrap_or(u32::MAX),
                    codec: encoding.settings.codec,
                },
                &mut packets,
            );
            if let Err(e) = cut {
                self.counts.too_large += 1;
                self.shared
                    .log
                    .write(&format!("frame {frame} could not be sent: {e}"));
                streaming.keys.ask();
                continue;
            }
            let datagrams = packets.len() as u64;
            match self.shared.outbox.video(packets) {
                Sent::Queued => {
                    self.counts.bytes += packet.data.len() as u64;
                    self.counts.datagrams += datagrams;
                }
                Sent::Crowded => {
                    // The player cannot decode past the hole: the next
                    // picture starts afresh.
                    self.counts.crowded += 1;
                    streaming.keys.ask();
                    if let Some(unsaid) = self.crowded.allow(sent_at) {
                        self.shared.log.write(&format!(
                            "frame {frame} found the link full and was dropped \
                             ({unsaid} more unsaid)"
                        ));
                    }
                }
                Sent::Closed => {}
            }
        }
        let worked = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.counts.work_us += worked;
        self.counts.work_max_us = self.counts.work_max_us.max(worked);
        self.counts.worked += 1;
    }

    /// The encoder failed in the middle of a stream: the next one takes
    /// over.
    fn encoder_failed(&mut self, why: &str) {
        let Some(encoding) = self
            .streaming
            .as_ref()
            .and_then(|streaming| streaming.encoding.as_ref())
        else {
            return;
        };
        let (codec, backend) = (encoding.settings.codec, encoding.backend);
        self.left_out(codec, backend, why);
        self.open_stream();
    }

    /// The screen failed: said to the log at a measured pace, and to the
    /// viewer once until it works again.
    fn screen_trouble(&mut self, e: &ScreenError) {
        if let Some(unsaid) = self.troubles.allow(Instant::now()) {
            self.shared
                .log
                .write(&format!("capture trouble ({unsaid} more unsaid): {e}"));
        }
        if !self.told_capture && self.streaming.is_some() {
            self.told_capture = true;
            self.shared.outbox.player(&ToPlayer::Notice {
                kind: NoticeKind::CaptureTrouble,
                text: e.0.clone(),
            });
        }
    }

    /// While nobody watches, says when the screens change.
    fn look_at_the_screens(&mut self) {
        let displays = self.screen.displays();
        if displays != self.displays {
            self.shared.log.write(&format!(
                "the screens changed: {}",
                displays
                    .iter()
                    .map(|display| format!("{} ({})", display.name, display.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            self.shared
                .outbox
                .service(&ToService::Displays(displays.clone()));
            self.displays = displays;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoder_gets_what_is_asked_less_parity_and_sound() {
        assert_eq!(encoder_kbps(20_000, 20, true), 16_538);
        assert_eq!(encoder_kbps(20_000, 20, false), 16_666);
        assert_eq!(encoder_kbps(20_000, 0, false), 20_000);
        assert_eq!(encoder_kbps(80_000, 20, true), 66_538);
    }

    #[test]
    fn a_rate_too_low_still_leaves_the_encoder_something() {
        assert_eq!(encoder_kbps(100, 20, true), LEAST_KBPS);
        assert_eq!(encoder_kbps(0, 20, false), LEAST_KBPS);
        assert_eq!(encoder_kbps(u32::MAX, 20, true), 3_579_139_284);
    }

    fn settings() -> Settings {
        Settings {
            codec: VideoCodec::Hevc,
            picture: Size::new(1920, 1080),
            fps: 60,
            kbps: 20_000,
        }
    }

    #[test]
    fn only_a_new_rate_of_bits_is_taken_in_place() {
        let now = settings();
        assert_eq!(plan(&now, &now), Plan::Keep);
        assert_eq!(
            plan(
                &now,
                &Settings {
                    kbps: 30_000,
                    ..now
                }
            ),
            Plan::Rate(30_000)
        );
        for next in [
            Settings {
                codec: VideoCodec::H264,
                ..now
            },
            Settings {
                picture: Size::new(1280, 720),
                ..now
            },
            Settings { fps: 144, ..now },
            Settings {
                fps: 30,
                kbps: 10_000,
                ..now
            },
        ] {
            assert_eq!(plan(&now, &next), Plan::Rebuild, "{next:?}");
        }
    }

    #[test]
    fn a_key_frame_asked_for_goes_at_once_the_first_time() {
        let start = Instant::now();
        let mut keys = KeyFrames::default();
        assert!(!keys.due(start));
        keys.ask();
        assert!(keys.due(start));
        assert_eq!(keys.deadline(), None);
        assert!(keys.take(start));
        assert!(!keys.due(start));
        assert!(!keys.take(start + KEY_FLOOR));
    }

    #[test]
    fn key_frames_are_never_closer_than_the_floor() {
        let start = Instant::now();
        let mut keys = KeyFrames::default();
        keys.ask();
        assert!(keys.take(start));
        let soon = start + Duration::from_millis(30);
        keys.ask();
        assert!(!keys.due(soon));
        assert!(!keys.take(soon));
        assert_eq!(keys.deadline(), Some(start + KEY_FLOOR));
        assert!(keys.take(start + KEY_FLOOR));
        assert_eq!(keys.deadline(), None);
    }

    #[test]
    fn a_key_frame_the_encoder_made_itself_answers_what_was_asked() {
        let start = Instant::now();
        let mut keys = KeyFrames::default();
        keys.ask();
        keys.made(start);
        assert!(!keys.due(start + KEY_FLOOR * 2));
        keys.ask();
        assert!(!keys.due(start + KEY_FLOOR / 2));
        assert!(keys.due(start + KEY_FLOOR));
    }
}
