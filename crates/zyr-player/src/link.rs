//! The player's end of the local link, on a small runtime of its own.
//!
//! One thread reads the link and sorts what arrives: the control
//! stream is read here, the video and sound datagrams go on to their
//! threads, and what the service says of the tunnel goes to the
//! measures. Everything the player says to the engine leaves from here
//! too, in the order it was said, with nothing held back to be grouped:
//! what is already waiting when a message leaves goes with it, nothing
//! more.
//!
//! The media threads are never kept waiting by the link: a datagram
//! finding its thread behind is dropped and counted rather than queued
//! without end.

use std::io;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use zyr_control::link::{Channel, Link, LinkWriter};
use zyr_media::MEDIA_VERSION;
use zyr_media::WireError;
use zyr_media::codec::CodecSet;
use zyr_media::control::{ByeReason, ControlReader, ToEngine, ToPlayer, Wanted};
use zyr_media::input::{InputEvent, Outgoing};
use zyr_media::service;
use zyr_proto::log::Log;

use crate::seldom::Seldom;
use crate::stats::{Clock, Tally};
use crate::tallies::{LinkTallies, Tallies};
use crate::video::{Recover, VideoInput};
use crate::{Ending, Event, Events, lock};

/// How long the link may take to be reached. It is on this computer and
/// answers at once; this only bounds a service stuck elsewhere.
const CONNECT_WITHIN: Duration = Duration::from_secs(5);

/// How often the engine's clock is asked for.
const PING_EVERY: Duration = Duration::from_millis(500);

/// How long the last words to the engine may take to leave once the
/// session is over.
const FLUSH_WITHIN: Duration = Duration::from_secs(1);

/// How long the video thread may take to let go of the window once the
/// session is over.
const LET_GO_WITHIN: Duration = Duration::from_secs(2);

/// Control messages gathered into one frame of the link at most, in
/// bytes: whatever is waiting, up to this.
const GATHERED: usize = 64 * 1024;

/// What the person's side asks of the player.
#[derive(Debug)]
pub enum Order {
    Change(Wanted),
    Input(InputEvent),
    /// The keyboard repeating a key held down.
    Repeat {
        scancode: u8,
        extended: bool,
    },
    ReleaseEverything,
    Resize {
        width: u32,
        height: u32,
    },
    Stop,
}

/// What the video thread asks of the link.
#[derive(Debug)]
pub enum Inner {
    /// The decoder is ready: the codecs it decodes.
    Hello(CodecSet),
    Recover(Recover),
    /// Nothing can be shown: the session ends, for this reason in French.
    Failed(String),
}

/// Everything the link thread works with.
pub struct Parts {
    pub name: String,
    pub wanted: Wanted,
    pub orders: UnboundedReceiver<Order>,
    pub inner: UnboundedReceiver<Inner>,
    pub video: SyncSender<VideoInput>,
    pub audio: SyncSender<Bytes>,
    /// Where the newest size of the surface waits for the video thread.
    pub size: Arc<Mutex<Option<(u32, u32)>>>,
    pub clock: Clock,
    pub tally: Arc<Mutex<Tally>>,
    pub tallies: Arc<Mutex<Tallies>>,
    /// Where the codecs the engine can encode are kept once it says.
    pub encodable: Arc<Mutex<Option<CodecSet>>>,
    pub events: Arc<Events>,
    /// Held for as long as the session lasts: the stats thread stops when
    /// it goes.
    pub stats_stop: std::sync::mpsc::Sender<()>,
    pub log: Log,
}

/// The link thread: reaches the link, says whether it could on
/// `connected`, then carries the session until it ends.
pub fn run(parts: Parts, connected: SyncSender<io::Result<()>>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = connected.send(Err(e));
            return;
        }
    };
    runtime.block_on(async move {
        let reached = tokio::time::timeout(CONNECT_WITHIN, zyr_control::link::connect(&parts.name))
            .await
            .unwrap_or_else(|_| {
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "le lien local ne répond pas",
                ))
            });
        match reached {
            Ok(link) => {
                parts
                    .log
                    .write(&format!("connected to the engine through {}", parts.name));
                let _ = connected.send(Ok(()));
                session(link, parts).await;
            }
            Err(e) => {
                parts.log.write(&format!(
                    "the link {} could not be reached: {e}",
                    parts.name
                ));
                let _ = connected.send(Err(e));
            }
        }
    });
}

/// Where a session stands, as the link sees it.
struct Session {
    wanted: Wanted,
    hello_sent: bool,
    outgoing: Outgoing,
    control: ControlReader<ToPlayer>,
    /// The last notice from the engine, which a fatal goodbye explains.
    last_notice: Option<String>,
    relayed: Option<bool>,
    counters: LinkTallies,
    out: UnboundedSender<Vec<u8>>,
    video: SyncSender<VideoInput>,
    audio: SyncSender<Bytes>,
    size: Arc<Mutex<Option<(u32, u32)>>>,
    clock: Clock,
    tally: Arc<Mutex<Tally>>,
    tallies: Arc<Mutex<Tallies>>,
    encodable: Arc<Mutex<Option<CodecSet>>>,
    events: Arc<Events>,
    hushed: Hushed,
    log: Log,
}

#[derive(Default)]
struct Hushed {
    video_crowded: Seldom,
    sound_crowded: Seldom,
    too_early: Seldom,
    unreadable: Seldom,
}

async fn session(link: Link, parts: Parts) {
    let Parts {
        wanted,
        mut orders,
        mut inner,
        video,
        audio,
        size,
        clock,
        tally,
        tallies,
        encodable,
        events,
        stats_stop,
        log,
        ..
    } = parts;
    let (mut reader, writer) = link.split();
    let (out, pending) = unbounded_channel();
    let writing = tokio::spawn(write(writer, pending, log.clone()));
    let mut session = Session {
        wanted,
        hello_sent: false,
        outgoing: Outgoing::new(),
        control: ControlReader::new(),
        last_notice: None,
        relayed: None,
        counters: LinkTallies::default(),
        out,
        video,
        audio,
        size,
        clock,
        tally,
        tallies,
        encodable,
        events,
        hushed: Hushed::default(),
        log,
    };
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let ending = loop {
        tokio::select! {
            frame = reader.next() => match frame {
                Ok(Some((channel, payload))) => {
                    if let Some(ending) = session.arrived(channel, payload) {
                        break ending;
                    }
                }
                Ok(None) => {
                    session.log.write("the link closed");
                    break Ending::LinkLost;
                }
                Err(e) => {
                    session.log.write(&format!("the link broke: {e}"));
                    break Ending::LinkLost;
                }
            },
            order = orders.recv() => {
                // Every handle of the player let go of is a stop too.
                if let Some(ending) = session.order(order.unwrap_or(Order::Stop)) {
                    break ending;
                }
            }
            Some(asked) = inner.recv() => {
                if let Some(ending) = session.inner(asked) {
                    break ending;
                }
            }
            _ = ping.tick(), if session.hello_sent => {
                let sent_us = session.clock.us(Instant::now());
                session.say(&ToEngine::Ping { sent_us });
            }
        }
    };
    session.log.write(&format!("session over: {ending:?}"));
    let Session {
        out,
        video,
        audio,
        events,
        log,
        ..
    } = session;
    // The media threads stop once nothing more comes to them.
    drop((video, audio));
    // The last words to the engine leave before the link closes, unless
    // the service has stopped reading it.
    drop(out);
    if tokio::time::timeout(FLUSH_WITHIN, writing).await.is_err() {
        log.write("the last messages to the engine could not leave in time");
    }
    // The end is told once the video thread has let go of the window,
    // which it has when it lets go of its end of `inner`: the window may
    // be destroyed as soon as it hears the end.
    let let_go = async { while inner.recv().await.is_some() {} };
    if tokio::time::timeout(LET_GO_WITHIN, let_go).await.is_err() {
        log.write("the video thread did not let go of the window in time");
    }
    events.end(ending);
    drop(stats_stop);
}

impl Session {
    /// A frame from the link. The end of the session, if it says so.
    fn arrived(&mut self, channel: Channel, payload: Bytes) -> Option<Ending> {
        match channel {
            Channel::Control => {
                self.control.feed(&payload);
                while let Some(message) = self.control.next() {
                    let ending = match message {
                        Ok(message) => self.heard(message),
                        Err(e) => self.unreadable(e),
                    };
                    if ending.is_some() {
                        return ending;
                    }
                }
            }
            Channel::Video => {
                let datagram = VideoInput::Datagram {
                    datagram: payload,
                    arrived: Instant::now(),
                };
                if let Err(refused) = self.video.try_send(datagram) {
                    self.video_refused(refused);
                }
            }
            Channel::Audio => {
                if let Err(refused) = self.audio.try_send(payload) {
                    self.audio_refused(refused);
                }
            }
            Channel::Service => self.heard_from_service(&payload),
        }
        None
    }

    fn heard(&mut self, message: ToPlayer) -> Option<Ending> {
        match message {
            ToPlayer::Welcome {
                version,
                encodable,
                display_width,
                display_height,
            } => {
                self.log.write(&format!(
                    "the engine answers in version {version}, encodes {}, films a screen of \
                     {display_width}x{display_height}",
                    names(encodable)
                ));
                *lock(&self.encodable) = Some(encodable);
            }
            ToPlayer::Streaming {
                stream,
                codec,
                width,
                height,
                fps,
            } => {
                self.log.write(&format!(
                    "stream {stream}: {} {width}x{height} at {fps} fps",
                    codec.name()
                ));
                let (width, height) = (u32::from(width), u32::from(height));
                lock(&self.tally).streaming(codec, width, height);
                self.events.say(Event::Streaming {
                    codec,
                    width,
                    height,
                });
            }
            ToPlayer::Pong { sent_us, host_us } => {
                lock(&self.tally).pong(Instant::now(), sent_us, host_us);
            }
            ToPlayer::Notice { kind, text } => {
                self.log
                    .write(&format!("notice from the engine ({kind:?}): {text}"));
                self.last_notice = Some(text.clone());
                self.events.say(Event::Notice(text));
            }
            ToPlayer::Still { stream, frame } => {
                // Dropped with the thread behind: its picture is then
                // not current anyway.
                let _ = self.video.try_send(VideoInput::Still { stream, frame });
            }
            ToPlayer::Bye { reason } => {
                self.log
                    .write(&format!("the engine says goodbye: {reason:?}"));
                return Some(match reason {
                    ByeReason::Asked | ByeReason::ServiceStop => Ending::HostLeft,
                    ByeReason::Fatal => {
                        Ending::EngineFailed(self.last_notice.take().unwrap_or_else(|| {
                            "Le moteur de l'ordinateur distant s'est arrêté sur une erreur."
                                .to_string()
                        }))
                    }
                });
            }
        }
        None
    }

    /// A control message that could not be read. The stream cannot go
    /// on when its framing is lost, nor with an engine of another
    /// version; anything else costs that message alone.
    fn unreadable(&mut self, error: WireError) -> Option<Ending> {
        self.counters.unreadable += 1;
        self.publish();
        match error {
            WireError::Framing => {
                self.log
                    .write("the control stream from the engine lost its framing");
                Some(Ending::EngineFailed(
                    "Le moteur de l'ordinateur distant envoie des messages illisibles.".to_string(),
                ))
            }
            WireError::Version(version) => {
                self.log.write(&format!(
                    "the engine speaks version {version}, this player {MEDIA_VERSION}"
                ));
                Some(Ending::EngineFailed(format!(
                    "L'ordinateur distant a une autre version de ZyrDesk (moteur {version}, \
                     ici {MEDIA_VERSION}) : mettez les deux à jour."
                )))
            }
            other => {
                let log = &self.log;
                self.hushed.unreadable.note(log, Instant::now(), |times| {
                    format!("control message from the engine unreadable: {other} ({times} since last said)")
                });
                None
            }
        }
    }

    fn heard_from_service(&mut self, payload: &[u8]) {
        match service::ToPlayer::decode(payload) {
            Ok(service::ToPlayer::Tunnel { rtt_us, relayed }) => {
                lock(&self.tally).tunnel(Instant::now(), rtt_us);
                if self.relayed != Some(relayed) {
                    self.relayed = Some(relayed);
                    self.log.write(if relayed {
                        "the tunnel goes through the relay"
                    } else {
                        "the tunnel goes straight to the host"
                    });
                }
            }
            Err(e) => {
                self.counters.unreadable += 1;
                self.publish();
                let log = &self.log;
                self.hushed.unreadable.note(log, Instant::now(), |times| {
                    format!("message from the service unreadable: {e} ({times} since last said)")
                });
            }
        }
    }

    fn order(&mut self, order: Order) -> Option<Ending> {
        match order {
            Order::Change(wanted) => {
                self.wanted = wanted;
                // Before the hello, the hello itself carries it.
                if self.hello_sent {
                    self.log
                        .write(&format!("asking for {}", described(&wanted)));
                    self.say(&ToEngine::Change { wanted });
                }
            }
            Order::Input(event) => {
                if !self.early() {
                    match self.outgoing.pass(event) {
                        Some(event) => self.input(event),
                        None => {
                            self.counters.pressed_again += 1;
                            self.publish();
                        }
                    }
                }
            }
            Order::Repeat { scancode, extended } => {
                if !self.early() {
                    let event = self.outgoing.pass_repeat(scancode, extended);
                    self.input(event);
                }
            }
            Order::ReleaseEverything => {
                for event in self.outgoing.release_everything() {
                    self.input(event);
                }
            }
            Order::Resize { width, height } => {
                *lock(&self.size) = Some((width, height));
                // A wake-up only: a thread too busy to take it reads the
                // size on its next turn anyway.
                let _ = self.video.try_send(VideoInput::Resized);
            }
            Order::Stop => {
                self.say(&ToEngine::Bye);
                return Some(Ending::Asked);
            }
        }
        None
    }

    fn inner(&mut self, asked: Inner) -> Option<Ending> {
        match asked {
            Inner::Hello(decodable) => {
                self.log.write(&format!(
                    "hello to the engine: {}, decodes {}",
                    described(&self.wanted),
                    names(decodable)
                ));
                self.say(&ToEngine::Hello {
                    version: MEDIA_VERSION,
                    wanted: self.wanted,
                    decodable,
                });
                self.hello_sent = true;
            }
            Inner::Recover(Recover { stream, frame }) => {
                self.say(&ToEngine::Recover { stream, frame });
            }
            Inner::Failed(reason) => {
                self.say(&ToEngine::Bye);
                return Some(Ending::EngineFailed(reason));
            }
        }
        None
    }

    /// Input before the hello: nothing is shown yet for it to aim at,
    /// and the engine would not know what to make of it.
    fn early(&mut self) -> bool {
        if self.hello_sent {
            return false;
        }
        self.counters.too_early += 1;
        self.publish();
        let log = &self.log;
        self.hushed.too_early.note(log, Instant::now(), |times| {
            format!("input before the session was open, not sent ({times} since last said)")
        });
        true
    }

    fn input(&mut self, event: InputEvent) {
        self.counters.sent += 1;
        self.publish();
        self.say(&ToEngine::Input(event));
    }

    fn say(&self, message: &ToEngine) {
        let mut bytes = Vec::new();
        message.write(&mut bytes);
        // The writer only goes away with the session.
        let _ = self.out.send(bytes);
    }

    fn video_refused(&mut self, refused: TrySendError<VideoInput>) {
        // A video thread gone has said why on its way out.
        if let TrySendError::Full(_) = refused {
            self.counters.video_crowded += 1;
            self.publish();
            let log = &self.log;
            self.hushed.video_crowded.note(log, Instant::now(), |times| {
                format!("video datagram dropped, the video thread being behind ({times} since last said)")
            });
        }
    }

    fn audio_refused(&mut self, refused: TrySendError<Bytes>) {
        let now = Instant::now();
        let log = &self.log;
        match refused {
            TrySendError::Full(_) => {
                self.counters.sound_crowded += 1;
                self.hushed.sound_crowded.note(log, now, |times| {
                    format!("sound datagram dropped, the sound thread being behind ({times} since last said)")
                });
            }
            TrySendError::Disconnected(_) => {
                if self.counters.sound_unplayed == 0 {
                    log.write("sound arrives with no sound card to play it: it is dropped");
                }
                self.counters.sound_unplayed += 1;
            }
        }
        self.publish();
    }

    fn publish(&self) {
        lock(&self.tallies).link = self.counters;
    }
}

/// Sends what the session says, gathering into one frame of the link
/// whatever is already waiting.
async fn write(mut writer: LinkWriter, mut pending: UnboundedReceiver<Vec<u8>>, log: Log) {
    while let Some(mut bytes) = pending.recv().await {
        while bytes.len() < GATHERED {
            match pending.try_recv() {
                Ok(more) => bytes.extend_from_slice(&more),
                Err(_) => break,
            }
        }
        if let Err(e) = writer.send(Channel::Control, &bytes).await {
            log.write(&format!("the engine can no longer be told anything: {e}"));
            return;
        }
    }
}

fn names(codecs: CodecSet) -> String {
    let names: Vec<&str> = codecs.iter().map(|codec| codec.name()).collect();
    if names.is_empty() {
        "nothing".to_string()
    } else {
        names.join(" ")
    }
}

fn described(wanted: &Wanted) -> String {
    format!(
        "{}x{} at {} fps, {} kb/s, codec {:?}, pointer drawn {}, sound {}, steady {}",
        wanted.width,
        wanted.height,
        wanted.fps,
        wanted.bitrate_kbps,
        wanted.codec,
        wanted.draw_pointer,
        wanted.audio,
        wanted.steady
    )
}
