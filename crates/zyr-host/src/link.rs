//! The local link to the service, carried on a thread of its own.
//!
//! One small runtime reads and writes the link, and nothing else runs on
//! it. What comes in is handed to whoever it is for without waiting:
//! keys and pointer straight to the input thread, the rest to the
//! engine; pings are answered here, so that the time they measure is the
//! link's and nothing else's. What goes out comes from the other threads
//! through queues: the messages of the conversation first, never dropped,
//! then sound, then pictures. Sound and pictures go through bounded
//! queues that a media thread never waits on: what finds one full is
//! dropped, and counted by whoever dropped it.

use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use tokio::runtime::Runtime;
use tokio::sync::mpsc as queue;
use zyr_control::link::{Channel, Link, LinkReader, LinkWriter};
use zyr_media::WireError;
use zyr_media::control::{ControlReader, ToEngine as FromPlayer, ToPlayer};
use zyr_media::service::{ToEngine as FromService, ToService};
use zyr_media::video::Packets;
use zyr_proto::log::Log;

use crate::clock::HostClock;
use crate::input;
use crate::session::{Asked, Event, Heard};

/// Pictures waiting for the link at most: a few frames, more than any
/// burst of the encoder, far less than a second.
const VIDEO_QUEUE: usize = 8;

/// Sound packets waiting for the link at most: 160 ms.
const AUDIO_QUEUE: usize = 16;

/// What the other threads hand the link.
#[derive(Clone)]
pub(crate) struct Outbox {
    reliable: queue::UnboundedSender<Reliable>,
    video: queue::Sender<Packets>,
    audio: queue::Sender<Vec<u8>>,
}

enum Reliable {
    Frame(Channel, Vec<u8>),
    /// Nothing more will be written: the link can close once what came
    /// before is out.
    End,
}

/// What became of a packet handed to the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sent {
    Queued,
    /// The queue was full: dropped.
    Crowded,
    /// The link is gone.
    Closed,
}

impl Outbox {
    pub(crate) fn player(&self, message: &ToPlayer) {
        let mut out = Vec::new();
        message.write(&mut out);
        // A closed queue means the link is gone, which the engine hears
        // from the link itself.
        let _ = self.reliable.send(Reliable::Frame(Channel::Control, out));
    }

    pub(crate) fn service(&self, message: &ToService) {
        let _ = self
            .reliable
            .send(Reliable::Frame(Channel::Service, message.encode()));
    }

    pub(crate) fn video(&self, packets: Packets) -> Sent {
        sent(self.video.try_send(packets))
    }

    pub(crate) fn audio(&self, datagram: Vec<u8>) -> Sent {
        sent(self.audio.try_send(datagram))
    }

    /// The last thing written: the link closes after what came before.
    pub(crate) fn end(&self) {
        let _ = self.reliable.send(Reliable::End);
    }
}

fn sent<T>(result: Result<(), queue::error::TrySendError<T>>) -> Sent {
    match result {
        Ok(()) => Sent::Queued,
        Err(queue::error::TrySendError::Full(_)) => Sent::Crowded,
        Err(queue::error::TrySendError::Closed(_)) => Sent::Closed,
    }
}

/// Where the link hands what it reads.
pub(crate) struct Handlers {
    pub(crate) events: mpsc::Sender<Event>,
    pub(crate) input: mpsc::Sender<input::Command>,
}

/// Carries `link`, made on `runtime`, on a thread of its own until the
/// link closes or [`Outbox::end`] is written.
pub(crate) fn start(
    runtime: Runtime,
    link: Link,
    handlers: Handlers,
    clock: HostClock,
    log: Log,
) -> io::Result<(Outbox, JoinHandle<()>)> {
    let (reliable, reliable_queue) = queue::unbounded_channel();
    let (video, video_queue) = queue::channel(VIDEO_QUEUE);
    let (audio, audio_queue) = queue::channel(AUDIO_QUEUE);
    let outbox = Outbox {
        reliable,
        video,
        audio,
    };
    let pongs = outbox.clone();
    let thread = thread::Builder::new()
        .name("engine link".to_string())
        .spawn(move || {
            let (reader, writer) = link.split();
            let ended = runtime.block_on(async {
                tokio::select! {
                    ended = read(reader, &handlers, &pongs, clock, &log) => ended,
                    ended = write(writer, reliable_queue, video_queue, audio_queue) => ended,
                }
            });
            if let Some(e) = &ended {
                log.write(&format!("link failed: {e}"));
            }
            // The engine may be gone already, having ended the link.
            let _ = handlers.events.send(Event::Link(Heard::Ended));
        })?;
    Ok((outbox, thread))
}

/// Reads the link until it closes: `None` when it closed, the error when
/// it failed.
async fn read(
    mut reader: LinkReader,
    handlers: &Handlers,
    pongs: &Outbox,
    clock: HostClock,
    log: &Log,
) -> Option<String> {
    let mut control = ControlReader::<FromPlayer>::new();
    let mut strays = 0u64;
    loop {
        let (channel, frame) = match reader.next().await {
            Ok(Some(frame)) => frame,
            Ok(None) => return None,
            Err(e) => return Some(e.to_string()),
        };
        match channel {
            Channel::Service => match FromService::decode(&frame) {
                Ok(message) => {
                    let _ = handlers.events.send(Event::Link(Heard::Service(message)));
                }
                Err(e) => log.write(&format!("unreadable message from the service: {e}")),
            },
            Channel::Control => {
                control.feed(&frame);
                for message in control.by_ref() {
                    pass_on(message, handlers, pongs, clock);
                }
            }
            Channel::Video | Channel::Audio => {
                strays += 1;
                if strays.is_power_of_two() {
                    log.write(&format!(
                        "{strays} media frames came in on the link, which carries none this way: dropped"
                    ));
                }
            }
        }
    }
}

/// Hands one message of the player to whoever it is for.
fn pass_on(
    message: Result<FromPlayer, WireError>,
    handlers: &Handlers,
    pongs: &Outbox,
    clock: HostClock,
) {
    let asked = match message {
        Ok(FromPlayer::Input(event)) => {
            let _ = handlers.input.send(input::Command::Event(event));
            return;
        }
        Ok(FromPlayer::Ping { sent_us }) => {
            pongs.player(&ToPlayer::Pong {
                sent_us,
                host_us: clock.now(),
            });
            let _ = handlers.input.send(input::Command::Heard);
            return;
        }
        // Of this engine's version: another one is read as an error.
        Ok(FromPlayer::Hello {
            wanted, decodable, ..
        }) => Heard::Player(Asked::Hello { wanted, decodable }),
        Ok(FromPlayer::Change { wanted }) => Heard::Player(Asked::Change { wanted }),
        Ok(FromPlayer::Recover { stream, frame }) => {
            Heard::Player(Asked::Recover { stream, frame })
        }
        Ok(FromPlayer::Bye) => Heard::Player(Asked::Bye),
        Err(e) => Heard::Unreadable(e),
    };
    let _ = handlers.input.send(input::Command::Heard);
    let _ = handlers.events.send(Event::Link(asked));
}

/// Writes what the others hand in, until [`Reliable::End`]: `None` then,
/// the error if the link failed.
async fn write(
    mut writer: LinkWriter,
    mut reliable: queue::UnboundedReceiver<Reliable>,
    mut video: queue::Receiver<Packets>,
    mut audio: queue::Receiver<Vec<u8>>,
) -> Option<String> {
    loop {
        let written = tokio::select! {
            biased;
            message = reliable.recv() => match message {
                Some(Reliable::Frame(channel, bytes)) => writer.send(channel, &bytes).await,
                Some(Reliable::End) | None => return None,
            },
            Some(datagram) = audio.recv() => writer.send(Channel::Audio, &datagram).await,
            Some(packets) = video.recv() => {
                let mut written = Ok(());
                for datagram in packets.iter() {
                    written = writer.send(Channel::Video, datagram).await;
                    if written.is_err() {
                        break;
                    }
                }
                written
            }
        };
        if let Err(e) = written {
            return Some(e.to_string());
        }
    }
}
