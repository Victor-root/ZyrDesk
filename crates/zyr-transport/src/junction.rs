//! The junction: the socket the transport speaks through, and the paths
//! that really carry its packets.
//!
//! The transport believes it speaks to a far computer at one address,
//! made up and stable, which is called its card here. Underneath, the
//! junction keeps the addresses that might reach that computer, probes
//! them with signed datagrams, keeps the ones that answer, and sends
//! every packet by the best of them. When the best one changes, the
//! transport sees nothing: same address, same connection, same keys.
//! Moving a session from one path to another is a line written in a
//! table.
//!
//! What the junction does not touch: a packet towards a real address
//! goes out as it is, and a packet from an address it knows nothing of
//! comes in as it is. Two computers on one network reach each other
//! exactly as they did before it existed.
//!
//! The datagrams of its own (`probe`) start with a byte no QUIC packet
//! starts with; they are sorted out before the transport sees anything,
//! and answered from here.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use quinn::udp::{EcnCodepoint, RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use ring::rand::SecureRandom;
use tokio::sync::{Notify, oneshot};

use crate::identity::{Fingerprint, Identity};
use crate::probe::{self, Echo, Heard, Nonce, Probe};

/// Where a line about a path goes.
pub type Say = Arc<dyn Fn(&str) + Send + Sync>;

/// The port every card carries. Nothing listens there: a card is a name.
const CARD_PORT: u16 = 47000;

/// The first octet of every card: a block nobody routes and no card of
/// this machine ever carries.
const CARD_BLOCK: u8 = 240;

/// How often the junction looks over its paths.
const TICK: Duration = Duration::from_millis(100);

/// The first seconds of a session: probes to every candidate, quickly,
/// which is what opens the boxes on both sides.
const EAGER: Duration = Duration::from_secs(5);
const EAGER_EVERY: Duration = Duration::from_millis(200);

/// The first minute: still looking for a path that has not answered.
const PATIENT: Duration = Duration::from_secs(65);
const PATIENT_EVERY: Duration = Duration::from_secs(2);

/// Afterwards: a look now and then.
const LATE_EVERY: Duration = Duration::from_secs(15);

/// A probe on the path in use, to measure it and to keep the boxes'
/// translations alive.
const KEEP_EVERY: Duration = Duration::from_secs(2);

/// A probe on the other paths that answered, to keep them warm.
const WARM_EVERY: Duration = Duration::from_secs(5);

/// Probes without an echo before a path is given up.
const MISSES_TO_DIE: u8 = 3;

/// Paths kept warm beside the one in use.
const WARM_PATHS: usize = 2;

/// A path has to be this much shorter to replace the one in use.
const HYSTERESIS: Duration = Duration::from_millis(3);

/// Packets kept for a computer no path reaches yet.
///
/// A handshake is a few packets; what is kept is enough for it to go
/// out whole the moment a path answers, rather than a second later when
/// the transport gives up waiting and sends it again.
const HELD: usize = 8;

/// A probe unanswered for this long is forgotten.
const PROBE_LIFE: Duration = Duration::from_secs(5);

/// An expected computer that never answered is forgotten after this.
const EXPECTATION_LIFE: Duration = Duration::from_secs(120);

/// How long the mirror gets to answer, each time it is asked.
const MIRROR_PATIENCE: Duration = Duration::from_secs(1);
const MIRROR_TRIES: usize = 3;

/// The card address of that computer: the transport reaches it there.
pub fn card_of(peer: Fingerprint) -> SocketAddr {
    let bytes = peer.as_bytes();
    SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(CARD_BLOCK, bytes[0], bytes[1], bytes[2])),
        CARD_PORT,
    )
}

/// Whether that address is a card rather than a place on a network.
pub fn is_card(address: SocketAddr) -> bool {
    match address.ip().to_canonical() {
        IpAddr::V4(ip) => ip.octets()[0] == CARD_BLOCK,
        IpAddr::V6(_) => false,
    }
}

/// What carries a session right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Road {
    /// The real address the packets go to.
    pub through: SocketAddr,
    pub round_trip: Duration,
}

/// An address that might reach the computer, and when it was last tried.
#[derive(Debug)]
struct Candidate {
    address: SocketAddr,
    probed: Option<Instant>,
}

/// An address that answered.
#[derive(Debug)]
struct Path {
    address: SocketAddr,
    round_trip: Duration,
    echoed: Instant,
    probed: Instant,
    misses: u8,
}

#[derive(Debug)]
struct InFlight {
    number: u32,
    at: Instant,
}

/// A packet kept until a path exists.
#[derive(Debug)]
struct Held {
    contents: Vec<u8>,
    segment_size: Option<usize>,
    ecn: Option<EcnCodepoint>,
    src_ip: Option<IpAddr>,
}

/// One far computer the transport may speak to through its card.
#[derive(Debug)]
struct Expected {
    peer: Fingerprint,
    session: String,
    since: Instant,
    candidates: Vec<Candidate>,
    paths: Vec<Path>,
    elected: Option<SocketAddr>,
    held: VecDeque<Held>,
    next_number: u32,
    in_flight: Vec<InFlight>,
}

impl Expected {
    fn new(peer: Fingerprint, session: &str, now: Instant) -> Self {
        Self {
            peer,
            session: session.to_string(),
            since: now,
            candidates: Vec::new(),
            paths: Vec::new(),
            elected: None,
            held: VecDeque::new(),
            next_number: 1,
            in_flight: Vec::new(),
        }
    }

    fn add_candidate(&mut self, address: SocketAddr) {
        if self.candidates.iter().any(|known| known.address == address) {
            return;
        }
        self.candidates.push(Candidate {
            address,
            probed: None,
        });
    }

    /// How often a candidate that has not answered is tried, at this
    /// point of the session.
    fn every(&self, now: Instant) -> Duration {
        let age = now.duration_since(self.since);
        if age < EAGER {
            EAGER_EVERY
        } else if age < PATIENT {
            PATIENT_EVERY
        } else {
            LATE_EVERY
        }
    }

    /// The addresses to probe now: candidates whose turn it is, and the
    /// paths to keep, the ones gone quiet for too long being dropped.
    fn due(&mut self, now: Instant) -> Vec<SocketAddr> {
        let every = self.every(now);
        let mut due = Vec::new();
        for candidate in &mut self.candidates {
            let answered = self
                .paths
                .iter()
                .any(|path| path.address == candidate.address);
            if answered {
                continue;
            }
            if candidate
                .probed
                .is_none_or(|at| now.duration_since(at) >= every)
            {
                candidate.probed = Some(now);
                due.push(candidate.address);
            }
        }
        let elected = self.elected;
        self.paths.retain_mut(|path| {
            let keep_every = if Some(path.address) == elected {
                KEEP_EVERY
            } else {
                WARM_EVERY
            };
            if now.duration_since(path.probed) < keep_every {
                return true;
            }
            if path.echoed < path.probed {
                path.misses += 1;
            } else {
                path.misses = 0;
            }
            if path.misses >= MISSES_TO_DIE {
                return false;
            }
            path.probed = now;
            due.push(path.address);
            true
        });
        self.in_flight
            .retain(|probe| now.duration_since(probe.at) < PROBE_LIFE);
        due
    }

    fn number(&mut self, now: Instant) -> u32 {
        let number = self.next_number;
        self.next_number = self.next_number.wrapping_add(1);
        self.in_flight.push(InFlight { number, at: now });
        number
    }

    /// An echo came back from there: the path answers, and this is how
    /// long it takes.
    fn answered(
        &mut self,
        from: SocketAddr,
        number: u32,
        round_trip: Duration,
        now: Instant,
    ) -> bool {
        let Some(at) = self
            .in_flight
            .iter()
            .position(|probe| probe.number == number)
        else {
            return false;
        };
        self.in_flight.swap_remove(at);
        match self.paths.iter_mut().find(|path| path.address == from) {
            Some(path) => {
                // Smoothed the way a transport does, so one slow echo
                // does not move a session off a good path.
                path.round_trip = (path.round_trip * 7 + round_trip) / 8;
                path.echoed = now;
                path.misses = 0;
            }
            None => {
                self.paths.push(Path {
                    address: from,
                    round_trip,
                    echoed: now,
                    probed: now,
                    misses: 0,
                });
                self.paths.sort_by_key(|path| path.round_trip);
                let elected = self.elected;
                let mut kept = 0;
                self.paths.retain(|path| {
                    if Some(path.address) == elected {
                        return true;
                    }
                    kept += 1;
                    kept <= WARM_PATHS
                });
            }
        }
        true
    }

    /// Chooses the path in use: the shortest, unless the one in use is
    /// nearly as short. Says what changed, if anything did.
    fn elect(&mut self) -> Option<(Option<SocketAddr>, SocketAddr)> {
        let best = self.paths.iter().min_by_key(|path| path.round_trip)?;
        let current = self
            .elected
            .and_then(|address| self.paths.iter().find(|path| path.address == address));
        let chosen = match current {
            Some(current) if current.round_trip <= best.round_trip + HYSTERESIS => current,
            _ => best,
        };
        if Some(chosen.address) == self.elected {
            return None;
        }
        let before = self.elected;
        self.elected = Some(chosen.address);
        Some((before, chosen.address))
    }

    fn round_trip_of(&self, address: SocketAddr) -> Option<Duration> {
        self.paths
            .iter()
            .find(|path| path.address == address)
            .map(|path| path.round_trip)
    }

    fn hold(&mut self, transmit: &Transmit<'_>) {
        if self.held.len() >= HELD {
            self.held.pop_front();
        }
        self.held.push_back(Held {
            contents: transmit.contents.to_vec(),
            segment_size: transmit.segment_size,
            ecn: transmit.ecn,
            src_ip: transmit.src_ip,
        });
    }
}

/// A question to the mirror, waiting for its answer.
#[derive(Debug)]
struct Asked {
    nonce: Nonce,
    answer: oneshot::Sender<SocketAddr>,
}

#[derive(Debug, Default)]
struct Table {
    expected: HashMap<SocketAddr, Expected>,
    /// The real addresses known to belong to a card.
    by_real: HashMap<SocketAddr, SocketAddr>,
    asked: Option<Asked>,
    /// This socket, as other computers and the mirror saw it.
    seen_as: Vec<SocketAddr>,
}

impl Table {
    fn expectation_of(
        &mut self,
        from: Fingerprint,
        session: &str,
    ) -> Option<(SocketAddr, &mut Expected)> {
        let card = card_of(from);
        let expected = self.expected.get_mut(&card)?;
        if expected.peer != from || expected.session != session {
            return None;
        }
        Some((card, expected))
    }

    fn note_seen(&mut self, seen: SocketAddr) -> bool {
        if self.seen_as.contains(&seen) {
            return false;
        }
        self.seen_as.push(seen);
        true
    }
}

struct Inner {
    socket: Arc<dyn AsyncUdpSocket>,
    /// Whether the socket speaks IPv6, in which case every IPv4 address
    /// is handed to it in its mapped form, as the transport does.
    ipv6: bool,
    identity: Arc<Identity>,
    me: Fingerprint,
    started: Instant,
    table: Mutex<Table>,
    seen_changed: Notify,
    say: Say,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Junction")
            .field("socket", &self.socket)
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}

/// The junction, as the transport and the service both hold it.
#[derive(Debug, Clone)]
pub struct Junction {
    inner: Arc<Inner>,
}

impl Junction {
    /// Opens the socket, on both IP versions when the system allows it,
    /// and starts looking after the paths.
    ///
    /// To be called from inside the runtime: the transport registers
    /// its socket with it, and the probing runs on it.
    pub fn bind(listen: SocketAddr, identity: Arc<Identity>, say: Say) -> io::Result<Self> {
        let runtime = quinn::default_runtime()
            .ok_or_else(|| io::Error::other("aucun exécuteur asynchrone"))?;
        let socket = bind_socket(listen)?;
        let ipv6 = socket.local_addr()?.is_ipv6();
        let socket = runtime.wrap_udp_socket(socket)?;
        let me = identity.fingerprint();
        let inner = Arc::new(Inner {
            socket,
            ipv6,
            identity,
            me,
            started: Instant::now(),
            table: Mutex::new(Table::default()),
            seen_changed: Notify::new(),
            say,
        });
        tokio::spawn(look_after(Arc::downgrade(&inner)));
        Ok(Self { inner })
    }

    pub fn local_address(&self) -> io::Result<SocketAddr> {
        self.inner.socket.local_addr()
    }

    /// Expects that computer for that session: the card the transport
    /// is to reach it at.
    pub fn expect(&self, peer: Fingerprint, session: &str) -> SocketAddr {
        let card = card_of(peer);
        let mut table = self.inner.table.lock().expect("aiguilleur");
        table
            .expected
            .insert(card, Expected::new(peer, session, Instant::now()));
        card
    }

    /// Addresses that might reach the computer behind that card.
    pub fn add_candidates(
        &self,
        card: SocketAddr,
        candidates: impl IntoIterator<Item = SocketAddr>,
    ) {
        let mut table = self.inner.table.lock().expect("aiguilleur");
        if let Some(expected) = table.expected.get_mut(&card) {
            for candidate in candidates {
                expected.add_candidate(candidate);
            }
        }
    }

    /// The computer behind that card is not expected any more.
    pub fn forget(&self, card: SocketAddr) {
        let mut table = self.inner.table.lock().expect("aiguilleur");
        if table.expected.remove(&card).is_some() {
            table.by_real.retain(|_, known| *known != card);
        }
    }

    /// What carries the session towards that card right now.
    pub fn road(&self, card: SocketAddr) -> Option<Road> {
        let table = self.inner.table.lock().expect("aiguilleur");
        let expected = table.expected.get(&card)?;
        let through = expected.elected?;
        Some(Road {
            through,
            round_trip: expected.round_trip_of(through)?,
        })
    }

    /// This socket as it was seen from elsewhere: by the mirror, and by
    /// every computer that echoed a probe.
    pub fn seen_as(&self) -> Vec<SocketAddr> {
        self.inner.table.lock().expect("aiguilleur").seen_as.clone()
    }

    /// Waits until this socket is seen from somewhere new.
    pub async fn seen_changed(&self) {
        self.inner.seen_changed.notified().await;
    }

    /// Asks the mirror at that address where this socket is seen from.
    ///
    /// From this very socket and no other: a box ties its translation
    /// to the socket that speaks, and only this one will carry the
    /// tunnel.
    pub async fn ask_the_mirror(&self, mirror: SocketAddr) -> Option<SocketAddr> {
        for _ in 0..MIRROR_TRIES {
            let mut nonce = Nonce::default();
            if ring::rand::SystemRandom::new().fill(&mut nonce).is_err() {
                return None;
            }
            let (answer, waiting) = oneshot::channel();
            self.inner.table.lock().expect("aiguilleur").asked = Some(Asked { nonce, answer });
            self.inner.send_to(mirror, &probe::who_am_i(nonce));
            if let Ok(Ok(seen)) = tokio::time::timeout(MIRROR_PATIENCE, waiting).await {
                return Some(seen);
            }
        }
        self.inner.table.lock().expect("aiguilleur").asked = None;
        None
    }
}

impl Inner {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// An address as the socket wants it: mapped when it speaks IPv6.
    fn outward(&self, address: SocketAddr) -> SocketAddr {
        match address {
            SocketAddr::V4(v4) if self.ipv6 => {
                SocketAddr::new(IpAddr::V6(v4.ip().to_ipv6_mapped()), v4.port())
            }
            other => other,
        }
    }

    /// Sends one datagram of ours, best effort: a probe that does not go
    /// out is a probe that will be sent again.
    fn send_to(&self, destination: SocketAddr, contents: &[u8]) {
        let _ = self.socket.try_send(&Transmit {
            destination: self.outward(destination),
            ecn: None,
            contents,
            segment_size: None,
            src_ip: None,
        });
    }

    /// The probes due now, decided under the lock and sent outside it.
    fn tick(&self, now: Instant) {
        let mut probes: Vec<(SocketAddr, Probe)> = Vec::new();
        let mut flushed: Vec<(SocketAddr, Held)> = Vec::new();
        {
            let mut table = self.table.lock().expect("aiguilleur");
            let mut gone = Vec::new();
            for (card, expected) in table.expected.iter_mut() {
                if expected.paths.is_empty()
                    && now.duration_since(expected.since) > EXPECTATION_LIFE
                {
                    gone.push(*card);
                    continue;
                }
                for address in expected.due(now) {
                    let number = expected.number(now);
                    probes.push((
                        address,
                        Probe {
                            session: expected.session.clone(),
                            from: self.me,
                            to: expected.peer,
                            number,
                            sent: self.now_ms(),
                        },
                    ));
                }
                if let Some((before, after)) = expected.elect() {
                    self.said_elected(*card, before, after, expected.round_trip_of(after));
                    if before.is_none() {
                        flushed.extend(expected.held.drain(..).map(|held| (after, held)));
                    }
                }
            }
            for card in gone {
                table.expected.remove(&card);
                table.by_real.retain(|_, known| *known != card);
                (self.say)(&format!("card {card}: nobody answered, forgotten"));
            }
        }
        for (address, probe) in probes {
            if let Ok(bytes) = probe::seal_probe(&self.identity, &probe) {
                self.send_to(address, &bytes);
            }
        }
        for (through, held) in flushed {
            self.send_held(through, &held);
        }
    }

    fn send_held(&self, through: SocketAddr, held: &Held) {
        let _ = self.socket.try_send(&Transmit {
            destination: self.outward(through),
            ecn: held.ecn,
            contents: &held.contents,
            segment_size: held.segment_size,
            src_ip: held.src_ip,
        });
    }

    fn said_elected(
        &self,
        card: SocketAddr,
        before: Option<SocketAddr>,
        after: SocketAddr,
        round_trip: Option<Duration>,
    ) {
        let round_trip = round_trip.map_or(0, |taken| taken.as_millis());
        (self.say)(&match before {
            None => format!("card {card}: reached through {after}, {round_trip} ms"),
            Some(before) => {
                format!("card {card}: now through {after}, {round_trip} ms, instead of {before}")
            }
        });
    }

    /// One datagram of ours, from that address. Says what to send back.
    fn heard(&self, from: SocketAddr, datagram: &[u8]) -> Option<(SocketAddr, Vec<u8>)> {
        let now = Instant::now();
        match probe::heard(datagram)? {
            Heard::Probe(sealed) => {
                let claimed = sealed.claims();
                if claimed.to != self.me {
                    return None;
                }
                let mut table = self.table.lock().expect("aiguilleur");
                let (card, expected) = table.expectation_of(claimed.from, &claimed.session)?;
                let probe = sealed.opened_by(expected.peer)?.clone();
                // Where it came from can reach this computer, so it is
                // worth probing back, and is the address to write to.
                expected.add_candidate(from);
                table.by_real.insert(from, card);
                drop(table);
                let echo = Echo {
                    probe: Probe {
                        session: probe.session,
                        from: self.me,
                        to: probe.from,
                        number: probe.number,
                        sent: probe.sent,
                    },
                    seen: from,
                };
                let bytes = probe::seal_echo(&self.identity, &echo).ok()?;
                Some((from, bytes))
            }
            Heard::Echo(sealed) => {
                let claimed = &sealed.claims().probe;
                if claimed.to != self.me {
                    return None;
                }
                let round_trip = Duration::from_millis(self.now_ms().saturating_sub(claimed.sent));
                let mut table = self.table.lock().expect("aiguilleur");
                let (card, expected) = table.expectation_of(claimed.from, &claimed.session)?;
                let echo = sealed.opened_by(expected.peer)?;
                let seen = echo.seen;
                if !expected.answered(from, echo.probe.number, round_trip, now) {
                    return None;
                }
                let mut flushed = Vec::new();
                if let Some((before, after)) = expected.elect() {
                    let round_trip = expected.round_trip_of(after);
                    if before.is_none() {
                        flushed.extend(expected.held.drain(..).map(|held| (after, held)));
                    }
                    self.said_elected(card, before, after, round_trip);
                }
                table.by_real.insert(from, card);
                let newly_seen = table.note_seen(seen);
                drop(table);
                for (through, held) in flushed {
                    self.send_held(through, &held);
                }
                if newly_seen {
                    self.seen_changed.notify_waiters();
                }
                None
            }
            Heard::SeenAs { nonce, seen } => {
                let mut table = self.table.lock().expect("aiguilleur");
                let asked = table.asked.take()?;
                if asked.nonce != nonce {
                    table.asked = Some(asked);
                    return None;
                }
                let newly_seen = table.note_seen(seen);
                drop(table);
                let _ = asked.answer.send(seen);
                if newly_seen {
                    self.seen_changed.notify_waiters();
                }
                None
            }
            // This computer is no mirror.
            Heard::WhoAmI(_) => None,
        }
    }

    /// Goes through what the socket received: answers what is ours and
    /// takes it out, and gives the transport its card for what comes
    /// from a computer it reaches through one. Says how many entries
    /// are left to hand over.
    fn sift(&self, bufs: &mut [io::IoSliceMut<'_>], meta: &mut [RecvMeta], count: usize) -> usize {
        let mut answers = Vec::new();
        let mut kept = 0;
        for read in 0..count {
            let from = meta[read].addr;
            let canonical = SocketAddr::new(from.ip().to_canonical(), from.port());
            let len = meta[read].len;
            let stride = meta[read].stride.clamp(1, len.max(1));
            let mut write_at = 0;
            let mut read_at = 0;
            while read_at < len {
                let segment = stride.min(len - read_at);
                let buf = &mut bufs[read][..len];
                if probe::is_ours(&buf[read_at..read_at + segment]) {
                    if let Some(answer) = self.heard(canonical, &buf[read_at..read_at + segment]) {
                        answers.push(answer);
                    }
                } else {
                    if write_at != read_at {
                        buf.copy_within(read_at..read_at + segment, write_at);
                    }
                    write_at += segment;
                }
                read_at += segment;
            }
            if write_at == 0 {
                continue;
            }
            let mut entry = meta[read];
            entry.len = write_at;
            if let Some(card) = self
                .table
                .lock()
                .expect("aiguilleur")
                .by_real
                .get(&canonical)
            {
                entry.addr = self.outward(*card);
            }
            if kept != read {
                let (before, after) = bufs.split_at_mut(read);
                before[kept][..write_at].copy_from_slice(&after[0][..write_at]);
            }
            meta[kept] = entry;
            kept += 1;
        }
        for (to, bytes) in answers {
            self.send_to(to, &bytes);
        }
        kept
    }
}

impl AsyncUdpSocket for Junction {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.inner.socket.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        let destination = SocketAddr::new(
            transmit.destination.ip().to_canonical(),
            transmit.destination.port(),
        );
        if !is_card(destination) {
            return self.inner.socket.try_send(transmit);
        }
        let through = {
            let mut table = self.inner.table.lock().expect("aiguilleur");
            let Some(expected) = table.expected.get_mut(&destination) else {
                // A card nobody is expected behind: the packet has
                // nowhere to go, and the transport will try again.
                return Ok(());
            };
            match expected.elected {
                Some(through) => through,
                None => {
                    expected.hold(transmit);
                    return Ok(());
                }
            }
        };
        self.inner.socket.try_send(&Transmit {
            destination: self.inner.outward(through),
            ecn: transmit.ecn,
            contents: transmit.contents,
            segment_size: transmit.segment_size,
            src_ip: transmit.src_ip,
        })
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
            let kept = self.inner.sift(bufs, meta, count);
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

/// Looks after the paths for as long as the junction exists.
async fn look_after(junction: Weak<Inner>) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let Some(inner) = junction.upgrade() else {
            return;
        };
        inner.tick(Instant::now());
    }
}

/// Binds the socket: on both IP versions when asked to listen on every
/// interface and the system allows it, on the one asked for otherwise.
fn bind_socket(listen: SocketAddr) -> io::Result<std::net::UdpSocket> {
    if listen.ip() == IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        && let Ok(socket) = bind_both(listen.port())
    {
        return Ok(socket);
    }
    let socket = std::net::UdpSocket::bind(listen)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

fn bind_both(port: u16) -> io::Result<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_only_v6(false)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port).into())?;
    Ok(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::congestion::MediaProfile;
    use crate::endpoint::{Bytes, TunnelEndpoint};

    /// Past this, something that should have happened has not.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn quiet() -> Say {
        Arc::new(|_| {})
    }

    fn local() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    fn junction(identity: &Arc<Identity>) -> Junction {
        Junction::bind(local(), identity.clone(), quiet()).unwrap()
    }

    /// Two computers, each expecting the other, and the transport on
    /// both.
    struct Pair {
        host: Junction,
        client: Junction,
        host_end: TunnelEndpoint,
        client_end: TunnelEndpoint,
        host_card: SocketAddr,
        client_card: SocketAddr,
    }

    fn pair() -> Pair {
        let host_identity = Arc::new(Identity::generate().unwrap());
        let client_identity = Arc::new(Identity::generate().unwrap());
        let host = junction(&host_identity);
        let client = junction(&client_identity);
        let client_card = host.expect(client_identity.fingerprint(), "s1");
        let host_card = client.expect(host_identity.fingerprint(), "s1");
        let host_end = TunnelEndpoint::host_at(
            &host_identity,
            client_identity.fingerprint(),
            MediaProfile::default(),
            &host,
        )
        .unwrap();
        let client_end = TunnelEndpoint::client_at(
            &client_identity,
            host_identity.fingerprint(),
            MediaProfile::default(),
            &client,
        )
        .unwrap();
        Pair {
            host,
            client,
            host_end,
            client_end,
            host_card,
            client_card,
        }
    }

    #[tokio::test]
    async fn the_card_of_a_computer_is_stable_and_never_a_real_place() {
        let peer = Identity::generate().unwrap().fingerprint();
        let card = card_of(peer);
        assert_eq!(card, card_of(peer));
        assert!(is_card(card));
        assert!(!is_card("192.168.1.4:47000".parse().unwrap()));
        assert!(!is_card("[::ffff:192.168.1.4]:47000".parse().unwrap()));
        assert!(is_card(SocketAddr::new(
            IpAddr::V6(match card.ip() {
                IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                IpAddr::V6(_) => unreachable!(),
            }),
            card.port()
        )));
    }

    #[tokio::test]
    async fn two_computers_reach_each_other_through_their_cards() {
        let pair = pair();
        // Chacun ne connaît de l'autre que son adresse réelle, comme un
        // candidat venu du serveur.
        pair.client
            .add_candidates(pair.host_card, [pair.host.local_address().unwrap()]);
        pair.host
            .add_candidates(pair.client_card, [pair.client.local_address().unwrap()]);

        let (accepted, connected) = tokio::time::timeout(
            PATIENCE,
            futures_join(
                pair.host_end.accept(),
                pair.client_end.connect(pair.host_card),
            ),
        )
        .await
        .expect("la connexion n'est pas venue");
        let host_side = accepted.unwrap();
        let client_side = connected.unwrap();

        // Le transport ne connaît que les cartes.
        assert_eq!(client_side.remote_address(), pair.host_card);
        assert_eq!(host_side.remote_address(), pair.client_card);
        // Et l'aiguilleur sait par où ça passe vraiment.
        let road = pair.client.road(pair.host_card).unwrap();
        assert_eq!(road.through, pair.host.local_address().unwrap());
        assert!(road.round_trip < Duration::from_secs(1));

        client_side
            .send_datagram(Bytes::from_static(b"frame"))
            .unwrap();
        let received = tokio::time::timeout(PATIENCE, host_side.read_datagram())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&received[..], b"frame");

        // Chacun a été vu par l'autre, à son adresse réelle.
        assert!(
            pair.client
                .seen_as()
                .contains(&pair.client.local_address().unwrap())
        );
        assert!(
            pair.host
                .seen_as()
                .contains(&pair.host.local_address().unwrap())
        );
    }

    #[tokio::test]
    async fn the_packets_of_a_connection_wait_for_a_path() {
        let pair = pair();
        pair.host
            .add_candidates(pair.client_card, [pair.client.local_address().unwrap()]);
        // La connexion part tout de suite, avant qu'aucune adresse ne
        // soit connue : ses paquets attendent.
        let connecting = tokio::spawn({
            let end = pair.client_end.clone();
            let card = pair.host_card;
            async move { end.connect(card).await }
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        pair.client
            .add_candidates(pair.host_card, [pair.host.local_address().unwrap()]);

        let accepted = tokio::time::timeout(PATIENCE, pair.host_end.accept())
            .await
            .expect("personne n'est venu")
            .unwrap();
        let connected = connecting.await.unwrap().unwrap();
        assert_eq!(connected.remote_address(), pair.host_card);
        assert_eq!(accepted.remote_address(), pair.client_card);
    }

    #[tokio::test]
    async fn a_computer_reached_by_its_address_passes_through_untouched() {
        let host_identity = Arc::new(Identity::generate().unwrap());
        let client_identity = Identity::generate().unwrap();
        let door = junction(&host_identity);
        let host_end = TunnelEndpoint::host_at(
            &host_identity,
            client_identity.fingerprint(),
            MediaProfile::default(),
            &door,
        )
        .unwrap();
        let client_end = TunnelEndpoint::client(
            &client_identity,
            host_identity.fingerprint(),
            MediaProfile::default(),
            local(),
        )
        .unwrap();
        let (accepted, connected) = tokio::time::timeout(
            PATIENCE,
            futures_join(
                host_end.accept(),
                client_end.connect(door.local_address().unwrap()),
            ),
        )
        .await
        .unwrap();
        let host_side = accepted.unwrap();
        let client_side = connected.unwrap();
        assert_eq!(
            host_side.remote_address(),
            client_end.local_address().unwrap()
        );
        client_side
            .send_datagram(Bytes::from_static(b"lan"))
            .unwrap();
        assert_eq!(&host_side.read_datagram().await.unwrap()[..], b"lan");
    }

    #[tokio::test]
    async fn a_probe_signed_by_a_stranger_gets_no_echo_and_no_place() {
        let host_identity = Arc::new(Identity::generate().unwrap());
        let client_identity = Identity::generate().unwrap();
        let stranger = Identity::generate().unwrap();
        let door = junction(&host_identity);
        let card = door.expect(client_identity.fingerprint(), "s1");
        // C'est le transport qui lit la prise : sans point d'accès dessus,
        // personne n'entendrait rien.
        let _end = TunnelEndpoint::host_at(
            &host_identity,
            client_identity.fingerprint(),
            MediaProfile::default(),
            &door,
        )
        .unwrap();

        let raw = tokio::net::UdpSocket::bind(local()).await.unwrap();
        let probe = Probe {
            session: "s1".into(),
            from: client_identity.fingerprint(),
            to: host_identity.fingerprint(),
            number: 1,
            sent: 0,
        };
        // L'inconnu se fait passer pour le client attendu.
        let forged = probe::seal_probe(&stranger, &probe).unwrap();
        raw.send_to(&forged, door.local_address().unwrap())
            .await
            .unwrap();
        let mut buf = [0u8; 1500];
        let answered =
            tokio::time::timeout(Duration::from_millis(300), raw.recv_from(&mut buf)).await;
        assert!(answered.is_err(), "un écho est parti vers un inconnu");
        assert!(door.road(card).is_none());

        // Le vrai client, lui, reçoit son écho.
        let genuine = probe::seal_probe(&client_identity, &probe).unwrap();
        raw.send_to(&genuine, door.local_address().unwrap())
            .await
            .unwrap();
        let (count, _) = tokio::time::timeout(PATIENCE, raw.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        let Some(Heard::Echo(sealed)) = probe::heard(&buf[..count]) else {
            panic!("pas un écho");
        };
        let echo = sealed.opened_by(host_identity.fingerprint()).unwrap();
        assert_eq!(echo.seen, raw.local_addr().unwrap());
        assert_eq!(echo.probe.number, 1);
    }

    #[tokio::test]
    async fn the_mirror_says_where_the_socket_is_seen_from() {
        let identity = Arc::new(Identity::generate().unwrap());
        let junction = junction(&identity);
        let _end = TunnelEndpoint::client_at(
            &identity,
            identity.fingerprint(),
            MediaProfile::default(),
            &junction,
        )
        .unwrap();
        let mirror = tokio::net::UdpSocket::bind(local()).await.unwrap();
        let mirror_address = mirror.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 1500];
            loop {
                let (count, from) = mirror.recv_from(&mut buf).await.unwrap();
                if let Some(Heard::WhoAmI(nonce)) = probe::heard(&buf[..count]) {
                    mirror
                        .send_to(&probe::seen_as(nonce, from), from)
                        .await
                        .unwrap();
                }
            }
        });
        let seen = tokio::time::timeout(PATIENCE, junction.ask_the_mirror(mirror_address))
            .await
            .unwrap();
        assert_eq!(seen, Some(junction.local_address().unwrap()));
        assert_eq!(junction.seen_as(), vec![junction.local_address().unwrap()]);

        // Un miroir muet : rien, sans rester bloqué.
        let silent = tokio::net::UdpSocket::bind(local()).await.unwrap();
        let seen = junction.ask_the_mirror(silent.local_addr().unwrap()).await;
        assert_eq!(seen, None);
    }

    #[test]
    fn the_shortest_path_is_elected_and_a_near_equal_does_not_replace_it() {
        let peer = Identity::generate().unwrap().fingerprint();
        let now = Instant::now();
        let mut expected = Expected::new(peer, "s1", now);
        let a: SocketAddr = "10.0.0.1:47000".parse().unwrap();
        let b: SocketAddr = "10.0.0.2:47000".parse().unwrap();
        let first = expected.number(now);
        assert!(expected.answered(a, first, Duration::from_millis(20), now));
        assert_eq!(expected.elect(), Some((None, a)));
        // Un second chemin à peine plus court ne vaut pas une bascule.
        let second = expected.number(now);
        assert!(expected.answered(b, second, Duration::from_millis(18), now));
        assert_eq!(expected.elect(), None);
        // Nettement plus court, si.
        let third = expected.number(now);
        assert!(expected.answered(b, third, Duration::from_millis(1), now));
        assert_eq!(expected.elect(), Some((Some(a), b)));
        // Un écho à un numéro inconnu ne compte pas.
        assert!(!expected.answered(a, 999, Duration::from_millis(1), now));
    }

    #[test]
    fn a_path_that_stops_echoing_is_given_up_after_three_misses() {
        let peer = Identity::generate().unwrap().fingerprint();
        let start = Instant::now();
        let mut expected = Expected::new(peer, "s1", start);
        let a: SocketAddr = "10.0.0.1:47000".parse().unwrap();
        let number = expected.number(start);
        expected.answered(a, number, Duration::from_millis(5), start);
        expected.elect();
        // Une sonde toutes les deux secondes, dont trois sans écho :
        // c'est à la quatrième qu'on sait que le chemin est mort.
        let mut now = start;
        for probe in 1..=MISSES_TO_DIE + 1 {
            now += KEEP_EVERY;
            let due = expected.due(now);
            if probe <= MISSES_TO_DIE {
                assert_eq!(due, vec![a], "relance {probe}");
            } else {
                assert!(due.is_empty(), "le chemin mort a encore été sondé");
            }
        }
        assert!(expected.paths.is_empty());
    }

    #[test]
    fn a_candidate_is_probed_quickly_at_first_then_less_often() {
        let peer = Identity::generate().unwrap().fingerprint();
        let start = Instant::now();
        let mut expected = Expected::new(peer, "s1", start);
        let a: SocketAddr = "10.0.0.1:47000".parse().unwrap();
        expected.add_candidate(a);
        expected.add_candidate(a);
        assert_eq!(expected.candidates.len(), 1);
        assert_eq!(expected.due(start), vec![a]);
        assert!(expected.due(start + Duration::from_millis(100)).is_empty());
        assert_eq!(expected.due(start + Duration::from_millis(200)), vec![a]);
        // Après les premières secondes, toutes les deux secondes.
        let later = start + EAGER + Duration::from_millis(500);
        assert_eq!(expected.due(later), vec![a]);
        assert!(expected.due(later + Duration::from_millis(500)).is_empty());
        assert_eq!(expected.due(later + PATIENT_EVERY), vec![a]);
    }

    #[test]
    fn a_socket_binds_on_every_interface_one_way_or_another() {
        let socket = bind_socket("0.0.0.0:0".parse().unwrap()).unwrap();
        let bound = socket.local_addr().unwrap();
        assert_ne!(bound.port(), 0);
    }

    async fn futures_join<A, B>(a: A, b: B) -> (A::Output, B::Output)
    where
        A: std::future::Future,
        B: std::future::Future,
    {
        tokio::join!(a, b)
    }
}
