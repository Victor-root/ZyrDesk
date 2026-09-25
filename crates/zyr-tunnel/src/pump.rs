//! Moving bytes between the local link and the tunnel.
//!
//! This module does not know which side it sits on. It moves what one
//! engine's link carries to and from the encrypted connection, under the
//! same rules on both ends of the tunnel: the control stream rides the
//! engine's reliable stream, the picture and the sound ride datagrams,
//! in whichever direction they come, and the service's own words go to
//! the service. Only the assembly differs.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use zyr_control::link::{Channel, LinkReader, LinkWriter};
use zyr_transport::{Bytes, Connection, DatagramError, RecvStream, SendStream};

use crate::channel::{DatagramChannel, StreamChannel};
use crate::flow::Flow;
use crate::frame;
use crate::queue::DatagramQueue;
use crate::service::ServiceSide;

/// Largest piece of the engine's stream handed over in one frame.
///
/// The control stream carries small messages, a few dozen bytes each; a
/// piece this size is several of them read at once, far under what one
/// frame of the link may carry.
const PIECE: usize = 64 * 1024;

/// Control messages that may wait, in each direction, for the other end
/// to take them.
///
/// Reliable, so never dropped: a full queue makes whoever feeds it wait,
/// which only happens to a session whose other end has stopped reading.
pub(crate) const CONTROL_WAITING: usize = 256;

/// How long what one end said as it left may take to reach the other.
///
/// A goodbye is said and the link closed in one breath, and the tunnel
/// reads both at once. The session ends there, but the goodbye is what
/// tells the other end how it ended: it is carried on first, for as long
/// as this allows.
const LAST_WORDS_WITHIN: Duration = Duration::from_secs(2);

/// Tunnel counters, read by the watch over a session and by the bench.
#[derive(Debug, Default)]
pub struct Counters {
    to_tunnel: AtomicU64,
    to_link: AtomicU64,
    control_to_tunnel: AtomicU64,
    control_to_link: AtomicU64,
    too_large: AtomicU64,
    crowded: AtomicU64,
    crowded_here: AtomicU64,
    no_recipient: AtomicU64,
    unreadable: AtomicU64,
    refused: AtomicU64,
    service_dropped: AtomicU64,
}

/// Snapshot of the counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reading {
    /// Datagrams off the link, handed to the connection.
    pub to_tunnel: u64,
    /// Datagrams off the connection, written onto the link.
    pub to_link: u64,
    /// Pieces of the engine's control stream sent into the tunnel.
    pub control_to_tunnel: u64,
    /// Pieces of the engine's control stream written onto the link.
    pub control_to_link: u64,
    /// Datagrams refused for exceeding what the path accepts. Anything
    /// but zero means the engine packs more than the tunnel told it the
    /// path takes.
    pub too_large: u64,
    /// Datagrams handed to a send queue that had no room left for them.
    /// The transport took each of them by throwing an older one away,
    /// silently: this is the count of those holes in the picture.
    pub crowded: u64,
    /// Datagrams thrown away on this side, the oldest first, because
    /// whoever is at the other end of the link was not taking them fast
    /// enough.
    pub crowded_here: u64,
    /// Datagrams that arrived before anybody was at the other end of the
    /// link to take them.
    pub no_recipient: u64,
    /// Datagrams whose header names no known channel.
    pub unreadable: u64,
    /// Streams turned away: a second engine stream, or one that never
    /// said what it was.
    pub refused: u64,
    /// Service messages the service did not take in time.
    pub service_dropped: u64,
}

impl Counters {
    pub(crate) fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn refused(&self) {
        Self::bump(&self.refused);
    }

    pub fn reading(&self) -> Reading {
        Reading {
            to_tunnel: self.to_tunnel.load(Ordering::Relaxed),
            to_link: self.to_link.load(Ordering::Relaxed),
            control_to_tunnel: self.control_to_tunnel.load(Ordering::Relaxed),
            control_to_link: self.control_to_link.load(Ordering::Relaxed),
            too_large: self.too_large.load(Ordering::Relaxed),
            crowded: self.crowded.load(Ordering::Relaxed),
            crowded_here: self.crowded_here.load(Ordering::Relaxed),
            no_recipient: self.no_recipient.load(Ordering::Relaxed),
            unreadable: self.unreadable.load(Ordering::Relaxed),
            refused: self.refused.load(Ordering::Relaxed),
            service_dropped: self.service_dropped.load(Ordering::Relaxed),
        }
    }
}

/// Announces the channel at the head of a reliable stream.
pub async fn announce(sending: &mut SendStream, channel: StreamChannel) -> io::Result<()> {
    sending.write_all(&[channel.identifier()]).await?;
    Ok(())
}

/// Reads the channel announcement at the head of a reliable stream.
pub async fn read_announcement(receiving: &mut RecvStream) -> io::Result<StreamChannel> {
    let mut head = [0u8; 1];
    receiving
        .read_exact(&mut head)
        .await
        .map_err(io::Error::other)?;
    StreamChannel::from_identifier(head[0]).map_err(io::Error::other)
}

/// Sends a datagram carrying nothing, only to have it acknowledged.
///
/// A road the junction switches to is invisible to the connection above
/// it, by design: quinn goes on believing it is still waiting on
/// whatever it last sent, however long ago that was, and only a packet
/// it gets acknowledged tells it otherwise. A nudge is that packet,
/// worth sending the moment a road is seen to work again rather than
/// waiting for the connection's own doubling retries to get there on
/// their own.
pub fn nudge(connection: &Connection) -> io::Result<()> {
    connection
        .send_datagram(frame::encode_nudge().into())
        .map_err(io::Error::other)
}

/// Everything between one link and the connection, until either ends.
///
/// The link closing is the ordinary end of a session; the engine's
/// stream ending is the other side's. Either way, what the end that
/// left said before it went is carried on to the other first. The three
/// halves run in one task, so that once the session is over, the link
/// is let go of with them.
pub(crate) async fn between(
    link: zyr_control::link::Link,
    engine_stream: impl Future<Output = io::Result<(SendStream, RecvStream)>>,
    connection: &Connection,
    datagrams: &DatagramQueue,
    service: ServiceSide,
    counters: &Counters,
    flow: &Flow,
) -> io::Result<()> {
    let (reader, writer) = link.split();
    let ServiceSide { outgoing, incoming } = service;
    let (towards_the_stream, from_the_link) = mpsc::channel(CONTROL_WAITING);
    let (towards_the_link, from_the_stream) = mpsc::channel(CONTROL_WAITING);
    let reading = off_the_link(
        reader,
        towards_the_stream,
        connection,
        &incoming,
        counters,
        flow,
    );
    let writing = onto_the_link(writer, from_the_stream, datagrams, outgoing, counters, flow);
    let carrying = along_the_stream(engine_stream, towards_the_link, from_the_link, counters);
    tokio::pin!(reading, writing, carrying);
    tokio::select! {
        // The link before the stream: when both end at once, what the
        // link said last still has to go along the stream.
        biased;
        read = &mut reading => {
            read?;
            last_words(carrying).await
        }
        carried = &mut carrying => {
            carried?;
            last_words(writing).await
        }
        written = &mut writing => written,
    }
}

/// Waits for what the end that left said last to reach the other end,
/// for a moment at most.
async fn last_words(delivered: impl Future<Output = io::Result<()>>) -> io::Result<()> {
    tokio::time::timeout(LAST_WORDS_WITHIN, delivered)
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "les derniers mots d'un bout du tunnel n'ont pas atteint l'autre à temps",
            )
        })?
}

/// Hands every frame the link carries to where it belongs.
///
/// Holds the way to the stream for as long as the link is read: its
/// closing is what tells the stream that nothing more is coming.
async fn off_the_link(
    mut reader: LinkReader,
    towards_the_stream: mpsc::Sender<Bytes>,
    connection: &Connection,
    service: &mpsc::Sender<Bytes>,
    counters: &Counters,
    flow: &Flow,
) -> io::Result<()> {
    while let Some((channel, payload)) = reader.next().await? {
        match channel {
            Channel::Control => towards_the_stream
                .send(payload)
                .await
                .map_err(|_| io::Error::other("le flux du moteur s'est fermé"))?,
            // Never waited for: a service that stopped reading must not
            // hold up the picture travelling on the same link.
            Channel::Service => {
                if service.try_send(payload).is_err() {
                    Counters::bump(&counters.service_dropped);
                }
            }
            Channel::Video | Channel::Audio => {
                if let Some(datagram) = DatagramChannel::off_the_link(channel) {
                    into_the_tunnel(datagram, &payload, connection, counters, flow)?;
                }
            }
        }
    }
    Ok(())
}

/// Hands one datagram of the link to the connection.
fn into_the_tunnel(
    channel: DatagramChannel,
    payload: &[u8],
    connection: &Connection,
    counters: &Counters,
    flow: &Flow,
) -> io::Result<()> {
    let framed = frame::encode(channel, payload);
    let bytes = framed.len();
    // Asked before handing over rather than deduced afterwards: the
    // transport makes room by throwing the oldest away and says nothing,
    // so this is the only moment that loss can be counted.
    let room = connection.send_queue_room();
    if room < bytes {
        Counters::bump(&counters.crowded);
    }
    match connection.send_datagram(framed.into()) {
        Ok(()) => {
            Counters::bump(&counters.to_tunnel);
            flow.handed_to_the_tunnel(bytes, room);
        }
        // The path narrowed below what the engine was told it takes.
        // Dropping beats fragmenting: the picture's own error correction
        // exists for this.
        Err(DatagramError::TooLarge) => Counters::bump(&counters.too_large),
        Err(e) => return Err(io::Error::other(e)),
    }
    Ok(())
}

/// Writes onto the link what comes for it: the control stream first,
/// then the service, then the datagrams.
///
/// What the service said before the tunnel was up goes before anything
/// else, which is what lets the service speak first on a link it hands
/// over. Done once the far end's stream has ended and all it said is on
/// the link.
async fn onto_the_link(
    mut writer: LinkWriter,
    mut from_the_stream: mpsc::Receiver<Bytes>,
    datagrams: &DatagramQueue,
    mut service: mpsc::Receiver<Vec<u8>>,
    counters: &Counters,
    flow: &Flow,
) -> io::Result<()> {
    while let Ok(said) = service.try_recv() {
        writer.send(Channel::Service, &said).await?;
    }
    loop {
        tokio::select! {
            biased;
            piece = from_the_stream.recv() => {
                let Some(piece) = piece else {
                    return Ok(());
                };
                writer.send(Channel::Control, &piece).await?;
                Counters::bump(&counters.control_to_link);
            }
            Some(said) = service.recv() => writer.send(Channel::Service, &said).await?,
            (channel, payload, waited) = datagrams.pop() => {
                let writing = Instant::now();
                writer.send(channel.on_the_link(), &payload).await?;
                flow.onto_the_link(payload.len(), waited, writing.elapsed());
                Counters::bump(&counters.to_link);
            }
        }
    }
}

/// Carries the engine's control stream both ways, once it exists.
///
/// Until then, what the link says on it waits in its queue: nothing of a
/// reliable stream is ever dropped. A link that closes before the stream
/// exists, having said nothing, leaves nothing to carry. Done once
/// either end has left: the way to the link goes with it, which tells
/// the link nothing more is coming.
async fn along_the_stream(
    engine_stream: impl Future<Output = io::Result<(SendStream, RecvStream)>>,
    towards_the_link: mpsc::Sender<Bytes>,
    mut from_the_link: mpsc::Receiver<Bytes>,
    counters: &Counters,
) -> io::Result<()> {
    tokio::pin!(engine_stream);
    let mut first = None;
    let (sending, receiving) = loop {
        tokio::select! {
            stream = &mut engine_stream => break stream?,
            piece = from_the_link.recv(), if first.is_none() => match piece {
                Some(piece) => first = Some(piece),
                None => return Ok(()),
            },
        }
    };
    tokio::select! {
        read = stream_to_link(receiving, &towards_the_link) => read,
        written = link_to_stream(sending, first, from_the_link, counters) => written,
    }
}

/// The far end's control stream, piece by piece, towards the link. Its
/// end is the far end leaving.
async fn stream_to_link(
    mut receiving: RecvStream,
    towards_the_link: &mpsc::Sender<Bytes>,
) -> io::Result<()> {
    while let Some(piece) = receiving
        .read_chunk(PIECE, true)
        .await
        .map_err(io::Error::other)?
    {
        towards_the_link
            .send(piece.bytes)
            .await
            .map_err(|_| io::Error::other("le lien s'est fermé"))?;
    }
    Ok(())
}

/// The link's control stream, piece by piece, towards the far end, from
/// the `first` piece it said before the stream existed. Once the link
/// has closed, the stream ends, and this waits until the far end has
/// everything the link said before.
async fn link_to_stream(
    mut sending: SendStream,
    first: Option<Bytes>,
    mut from_the_link: mpsc::Receiver<Bytes>,
    counters: &Counters,
) -> io::Result<()> {
    let mut first = first;
    loop {
        let piece = match first.take() {
            Some(piece) => piece,
            None => match from_the_link.recv().await {
                Some(piece) => piece,
                None => break,
            },
        };
        sending.write_all(&piece).await.map_err(io::Error::other)?;
        Counters::bump(&counters.control_to_tunnel);
    }
    sending.finish().map_err(io::Error::other)?;
    sending.stopped().await.map_err(io::Error::other)?;
    Ok(())
}

/// Hands the link the datagrams that come out of the tunnel.
///
/// Never waits on the link: what comes off the connection is queued,
/// and the queue throws the oldest away when whoever reads the link
/// falls behind. Before anybody is at the other end of the link, there
/// is nobody to hand anything to.
pub(crate) async fn out_of_the_tunnel(
    connection: &Connection,
    datagrams: &DatagramQueue,
    somebody_there: &AtomicBool,
    counters: &Counters,
    flow: &Flow,
) -> io::Result<()> {
    loop {
        let received = connection.read_datagram().await.map_err(io::Error::other)?;
        let channel = match frame::decode(&received) {
            Ok(frame::Landed::Channel(channel, _)) => channel,
            // Asked for nothing, and its arrival was the whole point.
            Ok(frame::Landed::Nudge) => continue,
            Err(_) => {
                Counters::bump(&counters.unreadable);
                continue;
            }
        };
        if !somebody_there.load(Ordering::Relaxed) {
            Counters::bump(&counters.no_recipient);
            continue;
        }
        let (dropped, waiting) = datagrams.push(channel, received.slice(1..));
        counters.crowded_here.fetch_add(dropped, Ordering::Relaxed);
        flow.queued(waiting);
    }
}
