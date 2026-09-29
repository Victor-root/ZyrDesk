//! The relay, both ends of it, and nothing of what a relay decides.
//!
//! Two computers that cannot reach each other directly reach each other
//! through a relay: each opens a connection of its own to it, presents
//! the pass its server signed, and from then on every packet one hands
//! over comes out at the other. What travels is a whole packet of the
//! tunnel, already encrypted with keys only the two computers have; the
//! relay carries an envelope it cannot open, and it is the outer layer
//! of that envelope that lives here.
//!
//! Datagrams, and never streams: a loss between a computer and the relay
//! stays a loss, absorbed exactly as it would be on a direct path. A
//! stream would hold everything behind it until the retransmission
//! arrived, which is the one thing a picture cannot afford.
//!
//! What is here is transport: the branch a device holds towards a relay,
//! the words of the first stream, and the doorway a server puts its
//! relay on, which answers the mirror on the same port. Who may pass,
//! for how long and at what rate is the server's business and lives
//! there.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use zyr_proto::fingerprint::Fingerprint;

use crate::congestion::{Media, Sending};
use crate::endpoint::{Bytes, Connection, EndpointError, GUARANTEED_MTU, TunnelEndpoint};
use crate::identity::Identity;
use crate::junction::{Aloud, Junction, Say, bind_socket};
use crate::marking::Marking;
use crate::probe;
use crate::race::first_to_answer;
use crate::sifting;

/// How long a device gets to present its pass once it is connected.
pub const PASS_PATIENCE: Duration = Duration::from_secs(3);

/// The longest pass a device may present, and the longest refusal a
/// relay may write back.
const LONGEST_WORD: usize = 4096;

/// The relay's answer: the pass is taken, and packets may flow.
const TAKEN: u8 = 1;

/// Pause before opening a branch of relay again.
///
/// A relay being restarted is back in a second or two, and a session
/// wants its fallback back the moment it is there. Short enough for
/// that, long enough that a relay that has gone for good is not asked
/// hundreds of times a minute for the length of a session.
const BRANCH_RETRY: Duration = Duration::from_secs(2);

/// Longest that pause grows to, for a branch that will not hold.
///
/// A branch that dies the instant it opens will not hold because it was
/// asked again at once: whatever killed it is still there. On the fourth
/// of September a computer whose packets left by two public addresses in
/// turn could not keep one for a whole second, and the guardian reopened
/// it thirty times in two: thirty connections at the relay for one
/// session, on a relay that carries a fixed number of them. Each attempt
/// that dies young therefore waits twice as long as the one before, up
/// to this, and a branch that held goes back to the short pause.
const BRANCH_PATIENCE: Duration = Duration::from_secs(30);

/// Why a branch towards a relay did not open.
#[derive(Debug)]
pub enum RelayError {
    Endpoint(EndpointError),
    /// The relay refused the pass, in its own words.
    Refused(String),
    /// The relay said nothing readable at all.
    Silent,
    /// The path to the relay does not carry a whole packet of the
    /// tunnel, so nothing that matters could travel on it.
    TooNarrow(u16),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::Endpoint(e) => write!(f, "{e}"),
            RelayError::Refused(why) => write!(f, "the relay refused the pass: {why}"),
            RelayError::Silent => f.write_str("the relay gave no answer to the pass"),
            RelayError::TooNarrow(room) => write!(
                f,
                "the path to the relay carries only {room} bytes a packet, and {GUARANTEED_MTU} \
                 are needed: this network cannot go through a relay"
            ),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<EndpointError> for RelayError {
    fn from(e: EndpointError) -> Self {
        RelayError::Endpoint(e)
    }
}

/// The relay a server named, and the right to use it.
#[derive(Debug, Clone)]
pub struct Wanted {
    /// Where it listens.
    pub address: SocketAddr,
    /// The fingerprint of the certificate it presents, from the server:
    /// a relay is never joined without a server having named it.
    pub fingerprint: Fingerprint,
    /// The pass, exactly as the server sealed it. Opaque here: the
    /// transport carries it and the relay reads it.
    pub pass: Vec<u8>,
}

/// A way to the far computer through a relay.
///
/// Cloneable, and cheaply: the junction holds one and the task reading
/// it holds another.
#[derive(Clone)]
pub struct Branch {
    inner: Arc<Held>,
}

struct Held {
    /// Kept because it owns the socket the branch speaks on.
    _endpoint: TunnelEndpoint,
    connection: Connection,
    address: SocketAddr,
    sent: AtomicU64,
    crowded: AtomicU64,
}

/// What a branch has carried since it opened.
///
/// A relayed road is two roads in a row, and only the first of them is
/// this computer's business. Without these, a road saturated here and a
/// far computer gone silent read exactly the same in the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carried {
    pub sent: u64,
    /// Packets the branch had no room for. The transport makes room by
    /// throwing the oldest away, which is the frame on its way out.
    pub crowded: u64,
}

impl std::fmt::Debug for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Branch")
            .field("address", &self.inner.address)
            .finish_non_exhaustive()
    }
}

impl Branch {
    /// Opens a connection to that relay and hands it the pass.
    ///
    /// Back once the relay has taken it, which is one round trip past
    /// the handshake, and never before: a branch announced ready ahead
    /// of that would be elected while the relay is still deciding, and
    /// the packets sent meanwhile would go nowhere.
    ///
    /// What it sends is what the tunnel it carries sends, and its queue
    /// is sized on that: a branch holding a megabyte of somebody else's
    /// traffic is a megabyte of staleness in series with the tunnel's
    /// own.
    pub async fn open(
        wanted: &Wanted,
        identity: &Identity,
        sending: Sending,
        media: impl Into<Media>,
        marking: Marking,
    ) -> Result<Self, RelayError> {
        let endpoint = TunnelEndpoint::towards_the_relay(
            identity,
            wanted.fingerprint,
            media,
            sending,
            marking,
            anywhere(wanted.address),
        )?;
        let connection = endpoint.connect(wanted.address).await?;
        let (mut writing, mut reading) = connection.open_stream().await?;
        writing
            .write_all(&wanted.pass)
            .await
            .map_err(|e| RelayError::Endpoint(EndpointError::Connection(e.to_string())))?;
        writing
            .finish()
            .map_err(|e| RelayError::Endpoint(EndpointError::Connection(e.to_string())))?;
        let answer = reading
            .read_to_end(LONGEST_WORD)
            .await
            .map_err(|e| RelayError::Endpoint(EndpointError::Connection(e.to_string())))?;
        match answer.split_first() {
            Some((&TAKEN, _)) => {}
            Some((_, why)) => {
                return Err(RelayError::Refused(
                    String::from_utf8_lossy(why).into_owned(),
                ));
            }
            None => return Err(RelayError::Silent),
        }
        // A packet of the tunnel travels whole or not at all.
        let room = connection.usable_datagram().unwrap_or(0);
        if room < GUARANTEED_MTU {
            return Err(RelayError::TooNarrow(room));
        }
        Ok(Self {
            inner: Arc::new(Held {
                _endpoint: endpoint,
                connection,
                address: wanted.address,
                sent: AtomicU64::new(0),
                crowded: AtomicU64::new(0),
            }),
        })
    }

    /// Where the relay carrying this branch listens.
    pub fn address(&self) -> SocketAddr {
        self.inner.address
    }

    /// How long the road to the relay itself is, which is half of what a
    /// relayed path costs. What the whole road costs is measured by the
    /// junction's own probes, like any other road.
    pub fn round_trip(&self) -> Duration {
        self.inner.connection.round_trip()
    }

    /// Hands one packet to the relay, for the far computer.
    ///
    /// Best effort, like every road: a packet the relay will not take is
    /// a packet lost on the way, and losses are what the engines' error
    /// correction exists for.
    pub fn send(&self, packet: &[u8]) -> bool {
        // Asked before handing over rather than deduced afterwards: the
        // transport makes room by throwing the oldest away and says
        // nothing, so this is the only moment that loss can be counted.
        if self.inner.connection.send_queue_room() < packet.len() {
            self.inner.crowded.fetch_add(1, Ordering::Relaxed);
        }
        let gone = self
            .inner
            .connection
            .send_datagram(Bytes::copy_from_slice(packet))
            .is_ok();
        if gone {
            self.inner.sent.fetch_add(1, Ordering::Relaxed);
        }
        gone
    }

    /// Everything the transport knows of this branch at this instant,
    /// for the journal at the moment a road starts going wrong.
    ///
    /// Its round trip is measured on the transport's own exchanges,
    /// which never wait behind the packets of a session: a branch can
    /// answer in seven milliseconds while what it carries is minutes
    /// late. Only the two together say where the wait is.
    pub fn how_it_stands(&self) -> String {
        let path = self.inner.connection.carrying();
        let carried = self.carried();
        format!(
            "{} ms to the relay, {} bytes may be out unanswered at once, {} bytes of room left in \
             its queue, {} packets handed over, {} with no room for them, {} lost on the way",
            path.round_trip.as_millis(),
            path.window,
            self.inner.connection.send_queue_room(),
            carried.sent,
            carried.crowded,
            path.lost
        )
    }

    /// What this branch has carried, and what it had no room for.
    pub fn carried(&self) -> Carried {
        Carried {
            sent: self.inner.sent.load(Ordering::Relaxed),
            crowded: self.inner.crowded.load(Ordering::Relaxed),
        }
    }

    /// Waits for the next packet the relay hands over, or nothing once
    /// the branch is gone.
    pub async fn arrived(&self) -> Option<Bytes> {
        self.inner.connection.read_datagram().await.ok()
    }

    /// Waits until this branch carries nothing any more.
    ///
    /// For whoever holds the branch rather than reads it: reading it is
    /// already somebody's work, and two readers of the same connection
    /// would take each other's packets. A relay that restarts, a server
    /// that is updated, a box that drops its translation: all of them end
    /// a branch under a session that is still running, and the session
    /// keeps its only fallback exactly as long as somebody opens another.
    pub async fn broken(&self) {
        self.inner.connection.closed().await;
    }
}

/// What one session needs to keep a branch of relay open.
pub struct Holding {
    /// The relay's name, as the server gave it.
    pub relay: String,
    /// The fingerprint of the certificate it presents, from the server.
    pub fingerprint: Fingerprint,
    /// The pass, exactly as the server sealed it.
    pub pass: Vec<u8>,
    pub identity: Arc<Identity>,
    pub junction: Junction,
    /// The card the far computer is expected behind, and the session it
    /// is held for: together they say when this may stop.
    pub card: SocketAddr,
    pub session: String,
    /// What this computer sends, which is what the branch's own queue is
    /// sized on.
    pub sending: Sending,
    pub media: Media,
    /// Whether the branch's packets leave with their congestion mark.
    pub marking: Marking,
}

/// Keeps a branch of relay open towards that card, and hands each one
/// to the junction as one more road.
///
/// Meant to be spawned and never waited on: the session leaves at once,
/// by whichever road answers first. In the ordinary case a direct road
/// is validated before the first branch is even connected, and the relay
/// carries nothing at all; where no direct road exists, this is the
/// session.
///
/// Held for as long as the session wants it, and not merely opened once.
/// A relay that restarts, a server that is updated, a box that drops its
/// translation: all of them end a branch under a session still running,
/// and what was written down as « the relay is kept warm all session, so
/// a direct road that dies comes back to it » was true only until the
/// first of those. It ends when the junction no longer holds the card
/// for this session, which is what the far side of a finished session
/// looks like from here.
///
/// One limit is known and not answered here: the pass a session was
/// handed lives five minutes, so a branch reopened long after that is
/// refused however healthy the relay is. The journal says so when it
/// happens. Asking the server for another pass mid-session is a word
/// this dialect does not have.
pub async fn hold_a_branch(held: Holding, say: Say) {
    let mut opened = 0u32;
    let mut wait = BRANCH_RETRY;
    while held.junction.still_expects(held.card, &held.session) {
        match open_a_branch(&held, &say, opened).await {
            Some(branch) => {
                opened += 1;
                let held_since = Instant::now();
                held.junction.relay_through(held.card, branch.clone());
                // Held rather than read: reading it is the junction's
                // work, and two readers of one connection would take
                // each other's packets. And let go of when the session
                // is: the junction gives its own copy back the moment it
                // stops expecting the card, and this one would otherwise
                // keep the connection, and the relay's place with it, for
                // as long as the service runs. A relay carries a fixed
                // number of sessions, and on the fourth of September it
                // was turning everybody away at the tenth, every one of
                // them a session long over.
                tokio::select! {
                    () = branch.broken() => {}
                    () = no_longer_expected(&held) => return,
                }
                // One that held is a road that works and was cut; one
                // that died young is a road that cannot carry a branch
                // yet, and asking it again at once is what turned two
                // seconds into thirty connections.
                wait = if held_since.elapsed() >= BRANCH_PATIENCE {
                    BRANCH_RETRY
                } else {
                    (wait * 2).min(BRANCH_PATIENCE)
                };
                say(
                    Aloud::Says,
                    &format!(
                        "the branch to the relay at {} is gone after {} ms, and this session \
                         still wants one: another in {} ms",
                        branch.address(),
                        held_since.elapsed().as_millis(),
                        wait.as_millis()
                    ),
                );
            }
            None => wait = (wait * 2).min(BRANCH_PATIENCE),
        }
        tokio::time::sleep(wait).await;
    }
}

/// Waits until the junction no longer holds the card for this session.
async fn no_longer_expected(held: &Holding) {
    while held.junction.still_expects(held.card, &held.session) {
        tokio::time::sleep(BRANCH_RETRY).await;
    }
}

/// Opens one branch, racing every address the relay's name leads to.
///
/// A name leads to as many addresses as the relay published, and the
/// first of them is not always one this computer can take: a machine
/// whose IPv6 is configured and broken is handed the IPv6 address of
/// every name it resolves, and reaches nothing behind it. So they are
/// all tried at once and the first branch open wins.
async fn open_a_branch(held: &Holding, say: &Say, opened: u32) -> Option<Branch> {
    let Ok(leads) = tokio::net::lookup_host(&held.relay).await else {
        say(
            Aloud::Says,
            &format!(
                "no relay: {} is not an address this computer can resolve",
                held.relay
            ),
        );
        return None;
    };
    let leads: Vec<SocketAddr> = leads.collect();
    if leads.is_empty() {
        say(
            Aloud::Says,
            &format!("no relay: {} leads nowhere", held.relay),
        );
        return None;
    }
    let started = Instant::now();
    let (address, branch) = first_to_answer(
        leads,
        |address| {
            let wanted = Wanted {
                address,
                fingerprint: held.fingerprint,
                pass: held.pass.clone(),
            };
            let identity = held.identity.clone();
            let media = held.media.clone();
            let (sending, marking) = (held.sending, held.marking);
            async move { Branch::open(&wanted, &identity, sending, media, marking).await }
        },
        // The pass a session was handed lives five minutes, and a branch
        // reopened after that is refused however healthy the relay is:
        // that is what this line will say, and there is nothing here that
        // can ask for another one.
        |address, e| {
            say(
                Aloud::Says,
                &format!(
                    "no relay branch through {address}{}: {e}",
                    if opened > 0 { ", reopening" } else { "" }
                ),
            );
        },
    )
    .await?;
    say(
        Aloud::Says,
        &format!(
            "card {}: the relay at {address} took the pass after {} ms, {} ms to it",
            held.card,
            started.elapsed().as_millis(),
            branch.round_trip().as_millis()
        ),
    );
    Some(branch)
}

/// A device at a relay's door, with its pass read and its answer owed.
pub struct Presenting {
    /// The pass, as the server sealed it.
    pub pass: Vec<u8>,
    /// The certificate the device presented, which TLS has already
    /// proven it holds the key of.
    pub fingerprint: Fingerprint,
    answering: crate::endpoint::SendStream,
}

impl Presenting {
    /// Waits for the first stream of that connection and reads the pass
    /// on it.
    ///
    /// A device that opens no stream, or writes no pass, is a device
    /// that gets nothing: the deadline belongs to the caller, which has
    /// the whole connection to drop.
    pub async fn heard(connection: &Connection) -> Result<Self, RelayError> {
        let fingerprint = connection
            .peer_fingerprint()
            .ok_or_else(|| RelayError::Refused("no certificate presented".to_string()))?;
        let (answering, mut reading) = connection.accept_stream().await?;
        let pass = reading
            .read_to_end(LONGEST_WORD)
            .await
            .map_err(|e| RelayError::Endpoint(EndpointError::Connection(e.to_string())))?;
        Ok(Self {
            pass,
            fingerprint,
            answering,
        })
    }

    /// Tells the device its pass is taken, and packets may flow.
    pub async fn taken(mut self) -> Result<(), EndpointError> {
        self.say(&[TAKEN]).await
    }

    /// Tells the device why it is not.
    ///
    /// Waited on until the device has the words, because the connection
    /// is shown out right afterwards: dropped a moment too early, the
    /// device reads a connection that broke instead of the sentence that
    /// says what happened.
    pub async fn refused(mut self, why: &str) -> Result<(), EndpointError> {
        let mut said = vec![0u8];
        said.extend_from_slice(why.as_bytes());
        said.truncate(LONGEST_WORD);
        self.say(&said).await?;
        let _ = self.answering.stopped().await;
        Ok(())
    }

    async fn say(&mut self, words: &[u8]) -> Result<(), EndpointError> {
        self.answering
            .write_all(words)
            .await
            .map_err(|e| EndpointError::Connection(e.to_string()))?;
        self.answering
            .finish()
            .map_err(|e| EndpointError::Connection(e.to_string()))
    }
}

/// The UDP port a server answers on: the mirror, and the relay behind
/// it.
///
/// One port for both, because they need each other: the mirror is what
/// makes a direct path possible at all, and the relay is what carries a
/// session when no direct path exists. A server without a relay keeps
/// the doorway all the same; it costs nothing and answers the question
/// every device asks.
#[derive(Clone)]
pub struct Doorway {
    inner: Arc<Gate>,
}

struct Gate {
    socket: Arc<dyn AsyncUdpSocket>,
    /// Whether the socket speaks IPv6, in which case every IPv4 address
    /// is handed to it in its mapped form.
    ipv6: bool,
}

impl std::fmt::Debug for Doorway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Doorway")
            .field("socket", &self.inner.socket)
            .finish_non_exhaustive()
    }
}

impl Doorway {
    /// Binds the port, on both IP versions when the system allows it.
    ///
    /// To be called from inside the runtime: the relay registers its
    /// endpoint on this socket.
    pub fn bind(listen: SocketAddr) -> io::Result<Self> {
        let runtime =
            quinn::default_runtime().ok_or_else(|| io::Error::other("no async runtime"))?;
        let socket = bind_socket(listen)?;
        let ipv6 = socket.local_addr()?.is_ipv6();
        let socket = runtime.wrap_udp_socket(socket)?;
        Ok(Self {
            inner: Arc::new(Gate { socket, ipv6 }),
        })
    }

    /// Where it listens: what the configuration said or, for a port left
    /// at nought, what the system gave.
    pub fn local_address(&self) -> io::Result<SocketAddr> {
        self.inner.socket.local_addr()
    }
}

impl AsyncUdpSocket for Doorway {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.inner.socket.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        self.inner.socket.try_send(transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            let count = match self.inner.socket.poll_recv(cx, bufs, meta) {
                Poll::Ready(Ok(count)) => count,
                other => return other,
            };
            let (kept, answers) = sifting::sift(
                bufs,
                meta,
                count,
                |from, datagram| {
                    probe::what_the_mirror_answers(datagram, from).map(|said| (from, said))
                },
                |_| None,
            );
            for (to, said) in answers {
                let _ = self.inner.socket.try_send(&Transmit {
                    destination: sifting::outward(to, self.inner.ipv6),
                    ecn: None,
                    contents: &said,
                    segment_size: None,
                    src_ip: None,
                });
            }
            if kept > 0 {
                return Poll::Ready(Ok(kept));
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.socket.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.socket.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.socket.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.socket.may_fragment()
    }
}

/// Any address of the same family, on any port: where a branch speaks
/// from.
fn anywhere(like: SocketAddr) -> SocketAddr {
    match like {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}

/// A relay of the barest kind, for the tests of this crate: it takes
/// every pass that does not start with nought, and hands every packet of
/// one end to the other.
///
/// Everything a real relay decides is left out on purpose, and lives in
/// the server. What is exercised here is the transport under it, which
/// is what this crate owns.
#[cfg(test)]
pub(crate) struct Bare {
    pub address: SocketAddr,
    pub fingerprint: Fingerprint,
    serving: tokio::task::JoinHandle<()>,
}

#[cfg(test)]
impl Drop for Bare {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

#[cfg(test)]
impl Bare {
    pub fn open() -> Self {
        let identity = Identity::generate().unwrap();
        let fingerprint = identity.fingerprint();
        let doorway = Doorway::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = doorway.local_address().unwrap();
        let endpoint =
            TunnelEndpoint::relay_on(&identity, Media::default(), Arc::new(doorway)).unwrap();
        let serving = tokio::spawn(async move {
            let both = Arc::new(tokio::sync::Mutex::new(Vec::<Connection>::new()));
            while let Ok(connection) = endpoint.accept().await {
                let both = both.clone();
                tokio::spawn(async move {
                    let presenting = Presenting::heard(&connection).await.unwrap();
                    if presenting.pass.first() == Some(&0) {
                        presenting.refused("this is not a pass").await.ok();
                        return;
                    }
                    presenting.taken().await.unwrap();
                    both.lock().await.push(connection.clone());
                    while let Ok(packet) = connection.read_datagram().await {
                        for other in both.lock().await.iter() {
                            if other.remote_address() != connection.remote_address() {
                                let _ = other.send_datagram(packet.clone());
                            }
                        }
                    }
                });
            }
        });
        Self {
            address,
            fingerprint,
            serving,
        }
    }

    pub fn wanted(&self, pass: &[u8]) -> Wanted {
        Wanted {
            address: self.address,
            fingerprint: self.fingerprint,
            pass: pass.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::congestion::MediaProfile;

    /// Past this, something that should have happened has not.
    const PATIENCE: Duration = Duration::from_secs(5);

    #[tokio::test(flavor = "multi_thread")]
    async fn two_branches_carry_a_whole_packet_of_the_tunnel_between_them() {
        let relay = Bare::open();
        let here = Identity::generate().unwrap();
        let there = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let first = Branch::open(
            &relay.wanted(b"pass"),
            &here,
            Sending::Pictures,
            profile,
            Marking::Ecn,
        )
        .await
        .unwrap();
        let second = Branch::open(
            &relay.wanted(b"pass"),
            &there,
            Sending::Pictures,
            profile,
            Marking::Ecn,
        )
        .await
        .unwrap();
        assert_eq!(first.address(), relay.address);

        // A whole packet of the tunnel, the only size that matters: that
        // is what goes through, or the relay is of no use.
        let packet = vec![7u8; usize::from(GUARANTEED_MTU)];
        assert!(first.send(&packet));
        let arrived = tokio::time::timeout(PATIENCE, second.arrived())
            .await
            .expect("nothing came through the relay")
            .unwrap();
        assert_eq!(&arrived[..], &packet[..]);
    }

    #[tokio::test]
    async fn what_the_branch_had_no_room_for_is_counted() {
        // A relayed road is two roads one after the other, and the first
        // has its own queue. When it overflows, the transport throws the
        // oldest out of it without a word: this counter is the only
        // place from which a road saturated here can be told apart from
        // a far computer gone silent, and both kill the session in the
        // same way.
        let relay = Bare::open();
        let profile = MediaProfile::default();
        let branch = Branch::open(
            &relay.wanted(b"pass"),
            &Identity::generate().unwrap(),
            Sending::Pictures,
            profile,
            Marking::Ecn,
        )
        .await
        .unwrap();

        // No wait in the loop: nothing goes out while it runs, so
        // the queue overflows.
        let packet = vec![7u8; usize::from(GUARANTEED_MTU)];
        for _ in 0..(crate::congestion::FASTEST.send_queue() / packet.len() * 4) {
            branch.send(&packet);
        }

        let carried = branch.carried();
        assert!(carried.sent > 0, "nothing was handed to the branch");
        assert!(
            carried.crowded > 0,
            "{} packets handed over and no lack of room counted",
            carried.sent
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pass_the_relay_refuses_says_why_rather_than_hanging() {
        let relay = Bare::open();
        let device = Identity::generate().unwrap();
        let refused = Branch::open(
            &relay.wanted(&[0]),
            &device,
            Sending::Pictures,
            MediaProfile::default(),
            Marking::Ecn,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&refused, RelayError::Refused(why) if why.contains("not a pass")),
            "{refused:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_relay_presenting_another_certificate_is_not_joined() {
        // That is what stops a session being diverted to a relay that
        // is not the one the server named.
        let relay = Bare::open();
        let device = Identity::generate().unwrap();
        let mut wanted = relay.wanted(b"pass");
        wanted.fingerprint = Identity::generate().unwrap().fingerprint();
        let refused = Branch::open(
            &wanted,
            &device,
            Sending::Pictures,
            MediaProfile::default(),
            Marking::Ecn,
        )
        .await
        .unwrap_err();
        assert!(matches!(refused, RelayError::Endpoint(_)), "{refused:?}");
    }

    #[tokio::test]
    async fn the_doorway_answers_the_mirror_on_the_relays_own_port() {
        let doorway = Doorway::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = doorway.local_address().unwrap();
        let identity = Identity::generate().unwrap();
        // Without an endpoint on it, nobody reads the socket: it is the
        // relay that keeps it running, mirror included.
        let _relay =
            TunnelEndpoint::relay_on(&identity, Media::default(), Arc::new(doorway)).unwrap();

        let asking = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let nonce = [4, 3, 2, 1, 4, 3, 2, 1];
        asking
            .send_to(&probe::who_am_i(nonce), address)
            .await
            .unwrap();
        let mut buf = [0u8; 1500];
        let (count, from) = tokio::time::timeout(PATIENCE, asking.recv_from(&mut buf))
            .await
            .expect("the mirror did not answer")
            .unwrap();
        assert_eq!(from, address);
        let Some(probe::Heard::SeenAs {
            nonce: answered,
            seen,
        }) = probe::heard(&buf[..count])
        else {
            panic!("not an answer from a mirror");
        };
        assert_eq!(answered, nonce);
        assert_eq!(seen, asking.local_addr().unwrap());
    }
}
