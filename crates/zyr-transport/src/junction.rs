//! The junction: the socket the transport speaks through, and the roads
//! that really carry its packets.
//!
//! The transport believes it speaks to a far computer at one address,
//! made up and stable, which is called its card here. Underneath, the
//! junction keeps the roads that might reach that computer, probes them
//! with signed datagrams, keeps the ones that answer, and sends every
//! packet by the best of them. When the best one changes, the transport
//! sees nothing: same address, same connection, same keys. Moving a
//! session from one road to another is a line written in a table.
//!
//! A road is an address of the far computer, or the relay branch held
//! for it, and the two are elected under one rule: a direct road that
//! answers always wins, whatever the relay measures. The relay carries
//! the session while no direct road is validated, and hands it over the
//! moment one is, without the session knowing.
//!
//! What the junction does not touch: a packet towards a real address
//! goes out as it is, and a packet from an address it knows nothing of
//! comes in as it is. Two computers on one network reach each other
//! exactly as they did before it existed.
//!
//! The datagrams of its own (`probe`) start with a byte no QUIC packet
//! starts with; they are sorted out before the transport sees anything,
//! and answered from here, by the road they came from.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use quinn::udp::{EcnCodepoint, RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use ring::rand::SecureRandom;
use tokio::sync::{Notify, oneshot};

use crate::endpoint::Bytes;
use crate::identity::{Fingerprint, Identity};
use crate::probe::{self, Echo, Heard, Nonce, Probe};
use crate::relay::Branch;
use crate::sifting;

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

/// A probe on the road in use, to measure it and to keep the boxes'
/// translations alive.
const KEEP_EVERY: Duration = Duration::from_secs(2);

/// A probe on the other roads that answered, to keep them warm.
const WARM_EVERY: Duration = Duration::from_secs(5);

/// Probes without an echo before a road is given up.
const MISSES_TO_DIE: u8 = 3;

/// Roads kept warm beside the one in use.
///
/// The relay is never one of them: it is kept for as long as it stands,
/// so that coming back to it costs nothing at all.
const WARM_PATHS: usize = 2;

/// A direct road has to be this much shorter to replace another.
const HYSTERESIS: Duration = Duration::from_millis(3);

/// Packets a relay brought, waiting to be handed to the transport.
///
/// Small on purpose: what the transport has not taken by then is already
/// late, and the oldest is the one worth losing, exactly as in the send
/// queue.
const RELAYED_WAITING: usize = 256;

/// Packets kept for a computer no path reaches yet.
///
/// A handshake is a few packets; what is kept is enough for it to go
/// out whole the moment a path answers, rather than a second later when
/// the transport gives up waiting and sends it again.
const HELD: usize = 8;

/// A probe unanswered for this long is forgotten.
const PROBE_LIFE: Duration = Duration::from_secs(5);

/// A computer that has not answered by any road for this long is
/// forgotten, and every packet the transport hands over for it is
/// dropped from then on.
///
/// Counted from the last road that answered, and not from the moment
/// the computer was expected. A session lives for hours; its roads can
/// all die for six seconds, which is what a relay hiccup or a box
/// dropping its translation looks like, and it must be alive when they
/// come back. Counted from the start, every session older than two
/// minutes was thrown away the instant its last road died, and nothing
/// could bring it back.
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

/// A road towards one far computer.
///
/// Self-contained: a value of this says on its own where a packet goes,
/// which is what lets one type serve the election, the probes and the
/// answers alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Through {
    /// Straight to that address.
    Direct(SocketAddr),
    /// Through the relay branch held for the computer behind that card,
    /// which hands the packet on to it.
    Relay(SocketAddr),
}

impl Through {
    fn relayed(self) -> bool {
        matches!(self, Through::Relay(_))
    }
}

/// What carries a session right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Road {
    /// The real address the packets go to: the far computer itself, or
    /// the relay that hands them on to it.
    pub through: SocketAddr,
    /// Whether that address is a relay rather than the far computer.
    pub relayed: bool,
    pub round_trip: Duration,
}

/// A road that might reach the computer, and when it was last tried.
#[derive(Debug)]
struct Candidate {
    through: Through,
    probed: Option<Instant>,
}

/// A road just taken, and the one it replaces.
#[derive(Debug, Clone, Copy)]
struct Elected {
    before: Option<Through>,
    after: Through,
    /// How long the one before carried the session, which is what says
    /// « by the relay, then direct after 340 ms ».
    carried: Duration,
}

/// A road that answered.
#[derive(Debug)]
struct Path {
    through: Through,
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

/// The relay branch held for one far computer, and the task reading it.
struct Relaying {
    branch: Branch,
    reading: tokio::task::JoinHandle<()>,
    /// Whether the journal has already said this branch is refusing
    /// packets. A branch that refuses does not start again on its own,
    /// so the moment is the news and the count is not.
    said_crowded: bool,
}

impl Drop for Relaying {
    fn drop(&mut self) {
        // The task holds a branch of its own, so the connection would
        // outlive the expectation without this.
        self.reading.abort();
    }
}

impl fmt::Debug for Relaying {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Relaying")
            .field("branch", &self.branch)
            .finish_non_exhaustive()
    }
}

/// One far computer the transport may speak to through its card.
#[derive(Debug)]
struct Expected {
    card: SocketAddr,
    peer: Fingerprint,
    session: String,
    since: Instant,
    /// The last moment a road answered, or the start when none ever
    /// has: what the patience below is counted from.
    answered_at: Instant,
    candidates: Vec<Candidate>,
    paths: Vec<Path>,
    elected: Option<Through>,
    /// When the road in use was elected, so a journal can say how long
    /// the relay carried a session before the direct road took over.
    elected_at: Instant,
    relay: Option<Relaying>,
    held: VecDeque<Held>,
    next_number: u32,
    in_flight: Vec<InFlight>,
}

/// What one look-over of a card's roads found.
struct LookedOver {
    /// Roads to probe now: candidates whose turn it is, and the roads
    /// that answered and are due their next measurement.
    probe: Vec<Through>,
    /// Roads that were probed and said nothing, too many times running.
    given_up: Vec<Through>,
}

impl Expected {
    fn new(card: SocketAddr, peer: Fingerprint, session: &str, now: Instant) -> Self {
        Self {
            card,
            peer,
            session: session.to_string(),
            since: now,
            answered_at: now,
            candidates: Vec::new(),
            paths: Vec::new(),
            elected: None,
            elected_at: now,
            relay: None,
            held: VecDeque::new(),
            next_number: 1,
            in_flight: Vec::new(),
        }
    }

    /// Notes a road worth trying, and says whether it is a new one.
    fn add_candidate(&mut self, address: SocketAddr) -> bool {
        self.add_road(Through::Direct(address))
    }

    fn add_road(&mut self, through: Through) -> bool {
        if self.candidates.iter().any(|known| known.through == through) {
            return false;
        }
        self.candidates.push(Candidate {
            through,
            probed: None,
        });
        true
    }

    /// Notes that this road has just been probed, so the next look-over
    /// waits its turn rather than probing it again.
    fn probed(&mut self, through: Through, now: Instant) {
        if let Some(candidate) = self
            .candidates
            .iter_mut()
            .find(|known| known.through == through)
        {
            candidate.probed = Some(now);
        }
    }

    /// How that road reads in the journal.
    fn named(&self, through: Through) -> String {
        match through {
            Through::Direct(address) => address.to_string(),
            Through::Relay(_) => match &self.relay {
                Some(relaying) => format!("the relay at {}", relaying.branch.address()),
                None => "the relay".to_string(),
            },
        }
    }

    /// How often a road that is not answering is tried, at this point.
    ///
    /// Quick at first, then less and less: the first seconds are what
    /// open the boxes on both sides, and a road that has said nothing
    /// for a minute is not about to. A session that has lost every road
    /// starts that clock again, because the seconds after the last road
    /// dies are worth exactly what the seconds after a session opens are
    /// worth: the far computer is there, and nothing reaches it.
    fn every(&self, now: Instant) -> Duration {
        let from = if self.paths.is_empty() {
            self.answered_at
        } else {
            self.since
        };
        let age = now.duration_since(from);
        if age < EAGER {
            EAGER_EVERY
        } else if age < PATIENT {
            PATIENT_EVERY
        } else {
            LATE_EVERY
        }
    }

    /// Looks the roads over: which are due a probe, and which have gone
    /// quiet long enough to be given up.
    ///
    /// A road given up is never the road in use afterwards. Handing
    /// packets to a road that has stopped answering loses every one of
    /// them without a word, where letting the election fall empty keeps
    /// the last few and sends them whole the moment a road answers.
    fn look_over(&mut self, now: Instant) -> LookedOver {
        let every = self.every(now);
        let mut due = Vec::new();
        let mut given_up = Vec::new();
        for candidate in &mut self.candidates {
            let answered = self
                .paths
                .iter()
                .any(|path| path.through == candidate.through);
            if answered {
                continue;
            }
            if candidate
                .probed
                .is_none_or(|at| now.duration_since(at) >= every)
            {
                candidate.probed = Some(now);
                due.push(candidate.through);
            }
        }
        let elected = self.elected;
        self.paths.retain_mut(|path| {
            let keep_every = if Some(path.through) == elected {
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
                given_up.push(path.through);
                return false;
            }
            path.probed = now;
            due.push(path.through);
            true
        });
        if self
            .elected
            .is_some_and(|through| given_up.contains(&through))
        {
            self.elected = None;
        }
        self.in_flight
            .retain(|probe| now.duration_since(probe.at) < PROBE_LIFE);
        LookedOver {
            probe: due,
            given_up,
        }
    }

    fn number(&mut self, now: Instant) -> u32 {
        let number = self.next_number;
        self.next_number = self.next_number.wrapping_add(1);
        self.in_flight.push(InFlight { number, at: now });
        number
    }

    /// An echo came back that way: the road answers, and this is how
    /// long it takes.
    fn answered(
        &mut self,
        through: Through,
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
        self.answered_at = now;
        match self.paths.iter_mut().find(|path| path.through == through) {
            Some(path) => {
                // Smoothed the way a transport does, so one slow echo
                // does not move a session off a good road.
                path.round_trip = (path.round_trip * 7 + round_trip) / 8;
                path.echoed = now;
                path.misses = 0;
            }
            None => {
                self.paths.push(Path {
                    through,
                    round_trip,
                    echoed: now,
                    probed: now,
                    misses: 0,
                });
                self.paths.sort_by_key(|path| path.round_trip);
                let elected = self.elected;
                let mut kept = 0;
                self.paths.retain(|path| {
                    // The relay is kept whatever it measures: coming
                    // back to it must not cost a new connection.
                    if Some(path.through) == elected || path.through.relayed() {
                        return true;
                    }
                    kept += 1;
                    kept <= WARM_PATHS
                });
            }
        }
        true
    }

    /// The road worth taking: the shortest direct one, and the relay
    /// only while no direct one answers.
    fn best(&self) -> Option<&Path> {
        self.paths
            .iter()
            .filter(|path| !path.through.relayed())
            .min_by_key(|path| path.round_trip)
            .or_else(|| self.paths.first())
    }

    /// Chooses the road in use. Says what changed, if anything did.
    ///
    /// Between two direct roads, the shorter, but not for a difference
    /// nobody would feel: switching for a hair is how a session ends up
    /// swinging between two equals. Against the relay there is no such
    /// margin, in either direction: a direct road that answers takes the
    /// session at once, and a direct road that dies gives it back at
    /// once.
    fn elect(&mut self, now: Instant) -> Option<Elected> {
        let best = self.best()?;
        let current = self
            .elected
            .and_then(|through| self.paths.iter().find(|path| path.through == through));
        let chosen = match current {
            Some(current)
                if current.through.relayed() == best.through.relayed()
                    && current.round_trip <= best.round_trip + HYSTERESIS =>
            {
                current.through
            }
            _ => best.through,
        };
        if Some(chosen) == self.elected {
            return None;
        }
        let taken = Elected {
            before: self.elected,
            after: chosen,
            carried: now.duration_since(self.elected_at),
        };
        self.elected = Some(chosen);
        self.elected_at = now;
        Some(taken)
    }

    fn round_trip_of(&self, through: Through) -> Option<Duration> {
        self.paths
            .iter()
            .find(|path| path.through == through)
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
    /// What a relay brought, waiting to be handed to the transport as
    /// coming from a card.
    relayed: VecDeque<(SocketAddr, Bytes)>,
    /// Whoever is waiting on the socket, to be woken when a relay brings
    /// something: nothing on the socket itself would wake it.
    waiting: Option<Waker>,
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
            .insert(card, Expected::new(card, peer, session, Instant::now()));
        card
    }

    /// Holds that relay branch as one more road towards the computer
    /// behind that card.
    ///
    /// From then on it is a road like any other: probed, measured, and
    /// elected only while no direct road answers. What it carries is
    /// read on a task of its own, which ends with the expectation.
    pub fn relay_through(&self, card: SocketAddr, branch: Branch) {
        let reading = tokio::spawn(read_the_relay(
            Arc::downgrade(&self.inner),
            card,
            branch.clone(),
        ));
        {
            let mut table = self.inner.table.lock().expect("aiguilleur");
            let Some(expected) = table.expected.get_mut(&card) else {
                reading.abort();
                return;
            };
            expected.relay = Some(Relaying {
                branch,
                reading,
                said_crowded: false,
            });
            expected.add_road(Through::Relay(card));
        }
        self.inner.probe_now(card, &[Through::Relay(card)]);
    }

    /// Addresses that might reach the computer behind that card.
    ///
    /// Each new one is probed on the spot: an address is named at the
    /// opening of a session, or as the far computer finds one, and
    /// waiting for the next look-over would cost up to a tenth of a
    /// second at exactly the moment a tenth of a second is felt.
    pub fn add_candidates(
        &self,
        card: SocketAddr,
        candidates: impl IntoIterator<Item = SocketAddr>,
    ) {
        let fresh: Vec<Through> = {
            let mut table = self.inner.table.lock().expect("aiguilleur");
            let Some(expected) = table.expected.get_mut(&card) else {
                return;
            };
            candidates
                .into_iter()
                .filter(|candidate| expected.add_candidate(*candidate))
                .map(Through::Direct)
                .collect()
        };
        self.inner.probe_now(card, &fresh);
    }

    /// The computer behind that card is not expected any more, for that
    /// session.
    ///
    /// One card stands for one computer, so two sessions in a row
    /// towards the same computer share it, and the second takes it from
    /// the first. A session ending late would otherwise take the card
    /// out from under the session that replaced it: everything the
    /// transport hands over goes nowhere from then on, in silence, and
    /// the far computer dies of an absence half a minute later. Only
    /// the session the card is held for can give it back.
    pub fn forget(&self, card: SocketAddr, session: &str) {
        let mut table = self.inner.table.lock().expect("aiguilleur");
        if table
            .expected
            .get(&card)
            .is_some_and(|held| held.session == session)
        {
            table.expected.remove(&card);
            table.by_real.retain(|_, known| *known != card);
        }
    }

    /// What carries the session towards that card right now.
    pub fn road(&self, card: SocketAddr) -> Option<Road> {
        let table = self.inner.table.lock().expect("aiguilleur");
        let expected = table.expected.get(&card)?;
        let elected = expected.elected?;
        let round_trip = expected.round_trip_of(elected)?;
        Some(match elected {
            Through::Direct(through) => Road {
                through,
                relayed: false,
                round_trip,
            },
            Through::Relay(_) => Road {
                through: expected.relay.as_ref()?.branch.address(),
                relayed: true,
                round_trip,
            },
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
        sifting::outward(address, self.ipv6)
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

    /// Sends one datagram of ours by that road, whichever it is.
    fn send_by(&self, road: Through, contents: &[u8]) {
        match road {
            Through::Direct(address) => self.send_to(address, contents),
            Through::Relay(card) => {
                let branch = self
                    .table
                    .lock()
                    .expect("aiguilleur")
                    .expected
                    .get(&card)
                    .and_then(|expected| expected.relay.as_ref())
                    .map(|relaying| relaying.branch.clone());
                if let Some(branch) = branch {
                    branch.send(contents);
                }
            }
        }
    }

    /// Probes those roads at once, rather than at the next look-over.
    fn probe_now(&self, card: SocketAddr, roads: &[Through]) {
        let mut probes = Vec::new();
        {
            let mut table = self.table.lock().expect("aiguilleur");
            let Some(expected) = table.expected.get_mut(&card) else {
                return;
            };
            let now = Instant::now();
            for road in roads {
                expected.probed(*road, now);
                let number = expected.number(now);
                probes.push((
                    *road,
                    Probe {
                        session: expected.session.clone(),
                        from: self.me,
                        to: expected.peer,
                        number,
                        sent: self.now_ms(),
                    },
                ));
            }
        }
        for (road, probe) in probes {
            if let Ok(bytes) = probe::seal_probe(&self.identity, &probe) {
                self.send_by(road, &bytes);
            }
        }
    }

    /// The probes due now, decided under the lock and sent outside it.
    fn tick(&self, now: Instant) {
        let mut probes: Vec<(Through, Probe)> = Vec::new();
        let mut flushed: Vec<(Through, Held)> = Vec::new();
        let mut said = Vec::new();
        {
            let mut table = self.table.lock().expect("aiguilleur");
            let mut gone = Vec::new();
            for (card, expected) in table.expected.iter_mut() {
                if expected.paths.is_empty()
                    && now.duration_since(expected.answered_at) > EXPECTATION_LIFE
                {
                    gone.push(*card);
                    continue;
                }
                let looked = expected.look_over(now);
                for road in looked.given_up {
                    said.push(format!(
                        "card {card}: {} stopped answering and is given up",
                        expected.named(road)
                    ));
                }
                for road in looked.probe {
                    let number = expected.number(now);
                    probes.push((
                        road,
                        Probe {
                            session: expected.session.clone(),
                            from: self.me,
                            to: expected.peer,
                            number,
                            sent: self.now_ms(),
                        },
                    ));
                }
                if let Some(relaying) = expected.relay.as_mut() {
                    let carried = relaying.branch.carried();
                    if carried.crowded > 0 && !relaying.said_crowded {
                        relaying.said_crowded = true;
                        said.push(format!(
                            "card {card}: the branch to the relay at {} is not taking packets as \
                             fast as the transport hands them over, so the oldest are thrown \
                             away, {} of {} so far",
                            relaying.branch.address(),
                            carried.crowded,
                            carried.sent + carried.crowded
                        ));
                    }
                }
                if let Some(taken) = expected.elect(now) {
                    said.push(said_elected(expected, taken));
                    if taken.before.is_none() {
                        flushed.extend(expected.held.drain(..).map(|held| (taken.after, held)));
                    }
                }
            }
            for card in gone {
                table.expected.remove(&card);
                table.by_real.retain(|_, known| *known != card);
                said.push(format!("card {card}: nobody answered, forgotten"));
            }
        }
        for line in said {
            (self.say)(&line);
        }
        for (road, probe) in probes {
            if let Ok(bytes) = probe::seal_probe(&self.identity, &probe) {
                self.send_by(road, &bytes);
            }
        }
        for (road, held) in flushed {
            self.send_held(road, &held);
        }
    }

    fn send_held(&self, road: Through, held: &Held) {
        match road {
            Through::Direct(through) => {
                let _ = self.socket.try_send(&Transmit {
                    destination: self.outward(through),
                    ecn: held.ecn,
                    contents: &held.contents,
                    segment_size: held.segment_size,
                    src_ip: held.src_ip,
                });
            }
            Through::Relay(_) => {
                for packet in packets(&held.contents, held.segment_size) {
                    self.send_by(road, packet);
                }
            }
        }
    }

    /// One datagram of ours, come by that road. Says what to send back,
    /// and by which road.
    fn heard(&self, came_by: Through, datagram: &[u8]) -> Option<(Through, Vec<u8>)> {
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
                // Nothing of the sort through a relay: what comes out of
                // one is nobody's address.
                if let Through::Direct(from) = came_by {
                    expected.add_candidate(from);
                    table.by_real.insert(from, card);
                }
                drop(table);
                let echo = Echo {
                    probe: Probe {
                        session: probe.session,
                        from: self.me,
                        to: probe.from,
                        number: probe.number,
                        sent: probe.sent,
                    },
                    // Where the probe was seen from, which a relay does
                    // not say: the sender's own card goes there instead,
                    // and the far end ignores it for exactly that reason.
                    seen: match came_by {
                        Through::Direct(from) => from,
                        Through::Relay(_) => card_of(probe.from),
                    },
                };
                let bytes = probe::seal_echo(&self.identity, &echo).ok()?;
                Some((came_by, bytes))
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
                if !expected.answered(came_by, echo.probe.number, round_trip, now) {
                    return None;
                }
                let mut flushed = Vec::new();
                let mut said = None;
                if let Some(taken) = expected.elect(now) {
                    if taken.before.is_none() {
                        flushed.extend(expected.held.drain(..).map(|held| (taken.after, held)));
                    }
                    said = Some(said_elected(expected, taken));
                }
                // An address is worth writing down only when the echo
                // really came from one.
                let newly_seen = match came_by {
                    Through::Direct(from) => {
                        table.by_real.insert(from, card);
                        table.note_seen(seen)
                    }
                    Through::Relay(_) => false,
                };
                drop(table);
                if let Some(line) = said {
                    (self.say)(&line);
                }
                for (road, held) in flushed {
                    self.send_held(road, &held);
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
        let (kept, answers) = sifting::sift(
            bufs,
            meta,
            count,
            |from, datagram| self.heard(Through::Direct(from), datagram),
            |from| {
                let card = *self.table.lock().expect("aiguilleur").by_real.get(&from)?;
                Some(self.outward(card))
            },
        );
        for (road, bytes) in answers {
            self.send_by(road, &bytes);
        }
        kept
    }

    /// One packet a relay handed over, for the computer behind that
    /// card: answered here when it is ours, and queued for the transport
    /// otherwise.
    fn arrived_by_relay(&self, card: SocketAddr, packet: Bytes) {
        if probe::is_ours(&packet) {
            if let Some((road, answer)) = self.heard(Through::Relay(card), &packet) {
                self.send_by(road, &answer);
            }
            return;
        }
        let waiting = {
            let mut table = self.table.lock().expect("aiguilleur");
            if table.relayed.len() >= RELAYED_WAITING {
                table.relayed.pop_front();
            }
            table.relayed.push_back((card, packet));
            table.waiting.take()
        };
        if let Some(waiting) = waiting {
            waiting.wake();
        }
    }

    /// Hands the transport what the relays brought, as coming from the
    /// cards. Says how many entries were filled; nought registers to be
    /// woken by the next packet, which no socket would wake anybody for.
    fn take_relayed(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> usize {
        let mut table = self.table.lock().expect("aiguilleur");
        let room = bufs.len().min(meta.len());
        let mut taken = 0;
        while taken < room {
            let Some((card, packet)) = table.relayed.pop_front() else {
                break;
            };
            // Bigger than what the transport offers to read into: it
            // cannot be handed over whole, so it is a loss like any
            // other on a road.
            if packet.len() > bufs[taken].len() {
                continue;
            }
            bufs[taken][..packet.len()].copy_from_slice(&packet);
            meta[taken] = RecvMeta {
                addr: self.outward(card),
                len: packet.len(),
                stride: packet.len(),
                ecn: None,
                dst_ip: None,
            };
            taken += 1;
        }
        if taken == 0 {
            table.waiting = Some(cx.waker().clone());
        }
        taken
    }
}

/// The line a change of road is worth in the journal.
fn said_elected(expected: &Expected, taken: Elected) -> String {
    let card = expected.card;
    let round_trip = expected
        .round_trip_of(taken.after)
        .map_or(0, |measured| measured.as_millis());
    let named = expected.named(taken.after);
    match taken.before {
        None => format!("card {card}: reached through {named}, {round_trip} ms"),
        Some(before) => format!(
            "card {card}: now through {named}, {round_trip} ms, instead of {}, which carried it \
             for {} ms",
            expected.named(before),
            taken.carried.as_millis()
        ),
    }
}

/// A buffer the transport handed over, as the packets it holds: one, or
/// several of one size when the system was asked to send them together.
fn packets(contents: &[u8], segment_size: Option<usize>) -> impl Iterator<Item = &[u8]> {
    contents.chunks(segment_size.unwrap_or(contents.len()).max(1))
}

/// Reads one relay branch for as long as the junction expects it.
///
/// The task is dropped with the expectation, so reaching the end of it
/// means the branch itself broke, and that is worth a line: a road that
/// stops carrying without a word is the shape every silent failure of
/// this product has taken.
async fn read_the_relay(junction: Weak<Inner>, card: SocketAddr, branch: Branch) {
    loop {
        let Some(packet) = branch.arrived().await else {
            if let Some(inner) = junction.upgrade() {
                (inner.say)(&format!(
                    "card {card}: the branch to the relay at {} is gone",
                    branch.address()
                ));
            }
            return;
        };
        let Some(inner) = junction.upgrade() else {
            return;
        };
        inner.arrived_by_relay(card, packet);
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
        let road = {
            let mut table = self.inner.table.lock().expect("aiguilleur");
            let Some(expected) = table.expected.get_mut(&destination) else {
                // A card nobody is expected behind: the packet has
                // nowhere to go, and the transport will try again.
                return Ok(());
            };
            match expected.elected {
                Some(road) => road,
                None => {
                    expected.hold(transmit);
                    return Ok(());
                }
            }
        };
        match road {
            Through::Direct(through) => self.inner.socket.try_send(&Transmit {
                destination: self.inner.outward(through),
                ecn: transmit.ecn,
                contents: transmit.contents,
                segment_size: transmit.segment_size,
                src_ip: transmit.src_ip,
            }),
            // A relay carries one packet at a time: what the system
            // would have sent as one buffer goes over as the packets it
            // holds.
            Through::Relay(_) => {
                for packet in packets(transmit.contents, transmit.segment_size) {
                    self.inner.send_by(road, packet);
                }
                Ok(())
            }
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            // What a relay brought is already sorted and named: it goes
            // first, and nothing on the socket waits behind it.
            let taken = self.inner.take_relayed(cx, bufs, meta);
            if taken > 0 {
                return Poll::Ready(Ok(taken));
            }
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
///
/// Public because everything of this product that binds a UDP port binds
/// it this way: the junction, the doorway a server puts its relay on,
/// and the mirror of a server that has no relay.
pub fn bind_socket(listen: SocketAddr) -> io::Result<std::net::UdpSocket> {
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

    #[tokio::test(flavor = "multi_thread")]
    async fn a_session_leaves_by_the_relay_and_goes_direct_without_breaking() {
        // Le cas que le relais existe pour porter : rien ne se joint
        // directement au départ, la session part quand même, et le
        // direct la reprend dès qu'il est validé, sans coupure.
        let relay = crate::relay::Bare::open();
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

        // Chaque bout ouvre sa branche : aucune adresse de l'autre n'est
        // connue, et aucune ne le sera avant la bascule.
        for (junction, card, identity) in [
            (&host, client_card, &host_identity),
            (&client, host_card, &client_identity),
        ] {
            let branch = crate::relay::Branch::open(
                &relay.wanted(b"laissez-passer"),
                identity,
                MediaProfile::default(),
            )
            .await
            .unwrap();
            junction.relay_through(card, branch);
        }

        let (accepted, connected) = tokio::time::timeout(
            PATIENCE,
            futures_join(host_end.accept(), client_end.connect(host_card)),
        )
        .await
        .expect("la session n'est jamais partie par le relais");
        let host_side = accepted.unwrap();
        let client_side = connected.unwrap();
        let road = client.road(host_card).unwrap();
        assert!(road.relayed, "{road:?}");
        assert_eq!(road.through, relay.address);

        client_side
            .send_datagram(Bytes::from_static(b"par le relais"))
            .unwrap();
        let received = tokio::time::timeout(PATIENCE, host_side.read_datagram())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&received[..], b"par le relais");

        // Le direct devient possible : la bascule est immédiate, et la
        // même connexion continue, sur la même carte.
        client.add_candidates(host_card, [host.local_address().unwrap()]);
        host.add_candidates(client_card, [client.local_address().unwrap()]);
        let direct = tokio::time::timeout(PATIENCE, async {
            loop {
                if let Some(road) = client.road(host_card)
                    && !road.relayed
                {
                    return road;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("le direct n'a jamais repris la session");
        assert_eq!(direct.through, host.local_address().unwrap());
        assert_eq!(client_side.remote_address(), host_card);

        client_side
            .send_datagram(Bytes::from_static(b"en direct"))
            .unwrap();
        let received = tokio::time::timeout(PATIENCE, host_side.read_datagram())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&received[..], b"en direct");
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

    /// An expectation with nothing in it, for the bookkeeping tests.
    fn expecting(now: Instant) -> Expected {
        let peer = Identity::generate().unwrap().fingerprint();
        Expected::new(card_of(peer), peer, "s1", now)
    }

    fn direct(address: &str) -> Through {
        Through::Direct(address.parse().unwrap())
    }

    /// What was elected, as a test reads it: the road before and after.
    fn moved(taken: Option<Elected>) -> Option<(Option<Through>, Through)> {
        taken.map(|taken| (taken.before, taken.after))
    }

    #[test]
    fn the_shortest_path_is_elected_and_a_near_equal_does_not_replace_it() {
        let now = Instant::now();
        let mut expected = expecting(now);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(now);
        assert!(expected.answered(a, first, Duration::from_millis(20), now));
        assert_eq!(moved(expected.elect(now)), Some((None, a)));
        // Un second chemin à peine plus court ne vaut pas une bascule.
        let second = expected.number(now);
        assert!(expected.answered(b, second, Duration::from_millis(18), now));
        assert_eq!(moved(expected.elect(now)), None);
        // Nettement plus court, si.
        let third = expected.number(now);
        assert!(expected.answered(b, third, Duration::from_millis(1), now));
        assert_eq!(moved(expected.elect(now)), Some((Some(a), b)));
        // Un écho à un numéro inconnu ne compte pas.
        assert!(!expected.answered(a, 999, Duration::from_millis(1), now));
    }

    #[test]
    fn a_direct_road_beats_the_relay_however_long_it_is() {
        // La règle du produit : le relais n'est qu'un secours. Un chemin
        // direct validé prend la session tout de suite, même s'il mesure
        // dix fois le relais, et la rend au relais dès qu'il meurt.
        let now = Instant::now();
        let mut expected = expecting(now);
        let relay = Through::Relay(expected.card);
        let a = direct("10.0.0.1:47000");

        let number = expected.number(now);
        assert!(expected.answered(relay, number, Duration::from_millis(5), now));
        assert_eq!(moved(expected.elect(now)), Some((None, relay)));

        let number = expected.number(now);
        assert!(expected.answered(a, number, Duration::from_millis(200), now));
        assert_eq!(moved(expected.elect(now)), Some((Some(relay), a)));
        assert!(expected.elect(now).is_none());

        // Le direct meurt : la session revient au relais, qui est resté
        // là tout du long.
        expected.paths.retain(|path| path.through != a);
        assert_eq!(moved(expected.elect(now)), Some((Some(a), relay)));
    }

    #[test]
    fn the_relay_is_never_dropped_to_make_room_for_a_warm_path() {
        // Y revenir doit coûter une ligne de table, pas une connexion.
        let now = Instant::now();
        let mut expected = expecting(now);
        let relay = Through::Relay(expected.card);
        let number = expected.number(now);
        expected.answered(relay, number, Duration::from_millis(90), now);
        for step in 0..(WARM_PATHS + 3) {
            let number = expected.number(now);
            let road = direct(&format!("10.0.0.{}:47000", step + 1));
            expected.answered(road, number, Duration::from_millis(step as u64 + 1), now);
        }
        assert!(
            expected.paths.iter().any(|path| path.through == relay),
            "le relais a été jeté"
        );
    }

    #[test]
    fn a_session_that_loses_every_road_is_kept_and_probed_as_at_its_opening() {
        // Une session vit des heures ; ses chemins peuvent tous mourir
        // six secondes, ce qu'un hoquet de relais ou une box qui lâche sa
        // traduction donnent, et elle doit être encore là quand ils
        // reviennent. La patience se compte donc du dernier chemin qui a
        // répondu, et le rythme des sondes repart de zéro avec elle.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let number = expected.number(start);
        assert!(expected.answered(a, number, Duration::from_millis(5), start));

        // Une heure de session : le chemin a répondu tout du long, la
        // dernière fois à l'instant, et les candidats qui se taisent ne
        // sont plus sondés que de loin en loin.
        let late = start + Duration::from_secs(3600);
        let number = expected.number(late);
        assert!(expected.answered(a, number, Duration::from_millis(5), late));
        assert_eq!(expected.every(late), LATE_EVERY);

        // Puis il meurt.
        expected.paths.clear();
        assert_eq!(
            expected.every(late),
            EAGER_EVERY,
            "un chemin qui meurt doit être cherché comme à l'ouverture"
        );
        assert!(
            late.duration_since(expected.answered_at) < EXPECTATION_LIFE,
            "la session a été jetée dès la mort de son chemin"
        );
        // Et deux minutes sans que rien ne réponde, alors oui.
        assert!(
            (late + EXPECTATION_LIFE + Duration::from_secs(1)).duration_since(expected.answered_at)
                > EXPECTATION_LIFE
        );
    }

    #[test]
    fn a_path_that_stops_echoing_is_given_up_after_three_misses() {
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let number = expected.number(start);
        expected.answered(a, number, Duration::from_millis(5), start);
        expected.elect(start);
        // Une sonde toutes les deux secondes, dont trois sans écho :
        // c'est à la quatrième qu'on sait que le chemin est mort.
        let mut now = start;
        for probe in 1..=MISSES_TO_DIE + 1 {
            now += KEEP_EVERY;
            let looked = expected.look_over(now);
            if probe <= MISSES_TO_DIE {
                assert_eq!(looked.probe, vec![a], "relance {probe}");
                assert!(looked.given_up.is_empty(), "abandonné à la relance {probe}");
            } else {
                assert!(looked.probe.is_empty(), "le chemin mort a encore été sondé");
                assert_eq!(looked.given_up, vec![a], "l'abandon n'est dit nulle part");
            }
        }
        assert!(expected.paths.is_empty());
        // Et la route abandonnée n'est plus celle qu'on emprunte. Sans
        // ça, tout ce que le transport confie ensuite part dans un
        // chemin mort, sans un mot et sans retour possible ; là, c'est
        // gardé pour la première route qui répond.
        assert_eq!(expected.elected, None);
    }

    #[tokio::test]
    async fn a_session_that_ends_late_does_not_take_the_card_of_the_one_after_it() {
        // Une carte vaut pour un ordinateur, donc deux sessions de suite
        // vers le même ordinateur la partagent et la seconde la prend à
        // la première. La fin de la première emportait la carte de la
        // seconde : à partir de là tout ce que le transport confiait
        // partait à la poubelle sans un mot, et l'ordinateur d'en face
        // mourait d'une absence trente secondes plus tard.
        let identity = Arc::new(Identity::generate().unwrap());
        let junction = junction(&identity);
        let peer = Identity::generate().unwrap().fingerprint();
        let card = junction.expect(peer, "s1");
        assert_eq!(
            junction.expect(peer, "s2"),
            card,
            "une carte par ordinateur"
        );

        junction.forget(card, "s1");
        let session_of = |card| {
            let table = junction.inner.table.lock().expect("aiguilleur");
            table.expected.get(&card).map(|held| held.session.clone())
        };
        assert_eq!(session_of(card).as_deref(), Some("s2"));

        junction.forget(card, "s2");
        assert_eq!(session_of(card), None);
    }

    #[tokio::test]
    async fn an_address_named_is_probed_at_once_and_not_again_at_the_look_over() {
        // Une adresse est nommée à l'ouverture d'une session, ou au fur
        // et à mesure que l'autre en trouve : attendre le tour d'horloge
        // suivant coûterait un dixième de seconde là où il se sent, et
        // laisserait au relais le début de chaque session.
        let identity = Arc::new(Identity::generate().unwrap());
        let junction = junction(&identity);
        let card = junction.expect(Identity::generate().unwrap().fingerprint(), "s1");
        junction.add_candidates(card, ["10.0.0.1:47000".parse().unwrap()]);

        let table = junction.inner.table.lock().expect("aiguilleur");
        let expected = table.expected.get(&card).expect("l'attente a disparu");
        assert_eq!(expected.in_flight.len(), 1, "aucune sonde n'est partie");
        assert!(expected.candidates[0].probed.is_some());
    }

    #[test]
    fn a_road_probed_on_the_spot_waits_its_turn_at_the_next_look_over() {
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let Through::Direct(address) = a else {
            unreachable!()
        };
        assert!(expected.add_candidate(address));
        assert!(!expected.add_candidate(address), "deux fois la même");
        expected.probed(a, start);
        assert!(expected.look_over(start).probe.is_empty());
        assert_eq!(expected.look_over(start + EAGER_EVERY).probe, vec![a]);
    }

    #[test]
    fn a_candidate_is_probed_quickly_at_first_then_less_often() {
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let Through::Direct(address) = a else {
            unreachable!()
        };
        expected.add_candidate(address);
        expected.add_candidate(address);
        assert_eq!(expected.candidates.len(), 1);
        assert_eq!(expected.look_over(start).probe, vec![a]);
        assert!(
            expected
                .look_over(start + Duration::from_millis(100))
                .probe
                .is_empty()
        );
        assert_eq!(
            expected.look_over(start + Duration::from_millis(200)).probe,
            vec![a]
        );
        // Après les premières secondes, toutes les deux secondes.
        let later = start + EAGER + Duration::from_millis(500);
        assert_eq!(expected.look_over(later).probe, vec![a]);
        assert!(
            expected
                .look_over(later + Duration::from_millis(500))
                .probe
                .is_empty()
        );
        assert_eq!(expected.look_over(later + PATIENT_EVERY).probe, vec![a]);
    }

    #[test]
    fn a_buffer_the_system_would_have_sent_in_one_go_is_split_for_a_relay() {
        // Un relais porte un paquet à la fois : ce que le système aurait
        // envoyé d'un bloc doit repartir en autant de paquets.
        let contents = vec![0u8; 2500];
        let split: Vec<usize> = packets(&contents, Some(1200))
            .map(|packet| packet.len())
            .collect();
        assert_eq!(split, vec![1200, 1200, 100]);
        assert_eq!(packets(&contents, None).count(), 1);
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
