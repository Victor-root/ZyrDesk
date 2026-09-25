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
//! dropped, and counted by whoever dropped it. The buffers a picture's
//! datagrams were written from go back to the pipeline, which cuts the
//! next pictures into them rather than into new ones.
//!
//! The link closes once every thread that writes to it has let go of its
//! queues and what they left in them is out, or when it fails.

use std::io;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Instant;

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
use crate::session::{self, Asked, Event, Heard};
use crate::timeline::Written;

/// Pictures waiting for the link at most: a few frames, more than any
/// burst of the encoder, far less than a second.
const VIDEO_QUEUE: usize = 8;

/// Sound packets waiting for the link at most: 160 ms.
const AUDIO_QUEUE: usize = 16;

/// Written buffers kept for the pipeline at most; the rest are freed.
const SPARE_BUFFERS: usize = 2;

/// What the other threads hand the link.
#[derive(Clone)]
pub(crate) struct Outbox {
    reliable: queue::UnboundedSender<(Channel, Vec<u8>)>,
    video: queue::Sender<Handed>,
    audio: queue::Sender<Vec<u8>>,
}

/// A picture's datagrams in the link's queue, and when they got there.
pub(crate) struct Handed {
    pub(crate) at: Instant,
    pub(crate) packets: Packets,
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
        let _ = self.reliable.send((Channel::Control, out));
    }

    pub(crate) fn service(&self, message: &ToService) {
        let _ = self.reliable.send((Channel::Service, message.encode()));
    }

    pub(crate) fn video(&self, packets: Packets) -> Sent {
        sent(self.video.try_send(Handed {
            at: Instant::now(),
            packets,
        }))
    }

    pub(crate) fn audio(&self, datagram: Vec<u8>) -> Sent {
        sent(self.audio.try_send(datagram))
    }
}

fn sent<T>(result: Result<(), queue::error::TrySendError<T>>) -> Sent {
    match result {
        Ok(()) => Sent::Queued,
        Err(queue::error::TrySendError::Full(_)) => Sent::Crowded,
        Err(queue::error::TrySendError::Closed(_)) => Sent::Closed,
    }
}

/// The buffers the link has written a picture's datagrams from, for the
/// pipeline to cut the next pictures into: once one has grown to the
/// largest picture, cutting allocates nothing.
pub(crate) struct Spares {
    written: mpsc::Receiver<Packets>,
}

impl Spares {
    /// A written buffer, or a new one while none has come back.
    pub(crate) fn take(&self) -> Packets {
        self.written.try_recv().unwrap_or_default()
    }
}

/// An outbox with no link behind it, whose pictures the test takes from
/// a queue of `room` frames; messages and sound are dropped.
#[cfg(test)]
pub(crate) fn detached(room: usize) -> (Outbox, Spares, queue::Receiver<Handed>) {
    let (reliable, _) = queue::unbounded_channel();
    let (video, pictures) = queue::channel(room);
    let (audio, _) = queue::channel(AUDIO_QUEUE);
    let (_, written) = mpsc::sync_channel(SPARE_BUFFERS);
    (
        Outbox {
            reliable,
            video,
            audio,
        },
        Spares { written },
        pictures,
    )
}

/// Where the link hands what it reads.
pub(crate) struct Handlers {
    pub(crate) events: mpsc::Sender<Event>,
    pub(crate) input: mpsc::Sender<input::Command>,
}

/// Carries `link`, made on `runtime`, on a thread of its own until the
/// link closes or every [`Outbox`] is gone.
pub(crate) fn start(
    runtime: Runtime,
    link: Link,
    handlers: Handlers,
    clock: HostClock,
    log: Log,
) -> io::Result<(Outbox, Spares, JoinHandle<()>)> {
    let (reliable, reliable_queue) = queue::unbounded_channel();
    let (video, video_queue) = queue::channel(VIDEO_QUEUE);
    let (audio, audio_queue) = queue::channel(AUDIO_QUEUE);
    let outbox = Outbox {
        reliable,
        video,
        audio,
    };
    let (spares, written) = mpsc::sync_channel(SPARE_BUFFERS);
    // Pongs have a queue of their own, which the link alone holds: the
    // others' queues closing is what ends it.
    let (pongs, pong_queue) = queue::unbounded_channel();
    let queues = Queues {
        reliable: reliable_queue,
        pongs: pong_queue,
        video: video_queue,
        audio: audio_queue,
        spares,
    };
    let events = handlers.events.clone();
    let thread = session::spawn("link", events, move || {
        let (reader, writer) = link.split();
        let mut written = Written::new(&log);
        let ended = runtime.block_on(async {
            tokio::select! {
                ended = read(reader, &handlers, &pongs, clock, &log) => ended,
                ended = write(writer, queues, &mut written) => ended,
            }
        });
        written.write();
        if let Some(e) = &ended {
            log.write(&format!("link failed: {e}"));
        }
        // The engine may be gone already, having ended the link.
        let _ = handlers.events.send(Event::Link(Heard::Ended));
    })?;
    Ok((outbox, Spares { written }, thread))
}

/// Reads the link until it closes: `None` when it closed, the error when
/// it failed.
async fn read(
    mut reader: LinkReader,
    handlers: &Handlers,
    pongs: &queue::UnboundedSender<Vec<u8>>,
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
    pongs: &queue::UnboundedSender<Vec<u8>>,
    clock: HostClock,
) {
    let asked = match message {
        Ok(FromPlayer::Input(event)) => {
            let _ = handlers.input.send(input::Command::Event(event));
            return;
        }
        Ok(FromPlayer::Ping { sent_us }) => {
            let mut pong = Vec::new();
            ToPlayer::Pong {
                sent_us,
                host_us: clock.now(),
            }
            .write(&mut pong);
            // The writer, on this same thread, lives as long as the reader.
            let _ = pongs.send(pong);
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

/// What the link writes, from whom, and where the written pictures'
/// buffers go back to.
struct Queues {
    reliable: queue::UnboundedReceiver<(Channel, Vec<u8>)>,
    pongs: queue::UnboundedReceiver<Vec<u8>>,
    video: queue::Receiver<Handed>,
    audio: queue::Receiver<Vec<u8>>,
    spares: mpsc::SyncSender<Packets>,
}

/// Writes what the others hand in, until none of them is left: `None`
/// then, the error if the link failed. What each picture waited and took
/// goes to `written`.
async fn write(
    mut writer: LinkWriter,
    mut queues: Queues,
    written_down: &mut Written,
) -> Option<String> {
    loop {
        let written = tokio::select! {
            biased;
            message = queues.reliable.recv() => match message {
                Some((channel, bytes)) => writer.send(channel, &bytes).await,
                None => return None,
            },
            Some(pong) = queues.pongs.recv() => writer.send(Channel::Control, &pong).await,
            Some(datagram) = queues.audio.recv() => writer.send(Channel::Audio, &datagram).await,
            Some(Handed { at, packets }) = queues.video.recv() => {
                let started = Instant::now();
                let mut written = Ok(());
                let mut bytes = 0;
                for datagram in packets.iter() {
                    bytes += datagram.len();
                    written = writer.send(Channel::Video, datagram).await;
                    if written.is_err() {
                        break;
                    }
                }
                written_down.picture(at, started, Instant::now(), packets.len(), bytes);
                // Never waits: with enough spares already, or the
                // pipeline gone, the buffer is freed.
                let _ = queues.spares.try_send(packets);
                written
            }
        };
        if let Err(e) = written {
            return Some(e.to_string());
        }
    }
}
