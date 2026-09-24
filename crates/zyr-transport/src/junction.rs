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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use quinn::udp::{EcnCodepoint, RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use ring::rand::SecureRandom;
use tokio::sync::{Notify, oneshot};

use crate::endpoint::Bytes;
use crate::identity::{Fingerprint, Identity};
use crate::marking::Marking;
use crate::probe::{self, Echo, Heard, Nonce, Probe};
use crate::relay::Branch;
use crate::sifting;

/// Where a line about a path goes.
pub type Say = Arc<dyn Fn(Aloud, &str) + Send + Sync>;

/// How loud a line is.
///
/// This crate is under everything and writes nothing itself: it hands
/// its lines to whoever is listening. But it is the only one that knows
/// which of them say what happened and which only count things, so it
/// says so, and the listener decides what a build keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aloud {
    /// What the transport says of itself: a road taken, a card given up
    /// on, a socket that refuses. Worth keeping in every build, since a
    /// session that will not open is diagnosed from exactly these.
    Says,
    /// What counts and measures. It drowns the rest and helps nobody
    /// whose session will not open, so it belongs to a build made for
    /// hunting and to no other.
    Hunts,
}

/// The port every card carries. Nothing listens there: a card is a name.
const CARD_PORT: u16 = 47000;

/// The first octet of every card: a block nobody routes and no card of
/// this machine ever carries.
const CARD_BLOCK: u8 = 240;

/// How often the junction looks over its paths.
const TICK: Duration = Duration::from_millis(100);

/// Round trip to a relay past which the packets are waiting in a queue
/// rather than travelling.
///
/// A relay is one hop of ordinary Internet away, a few dozen
/// milliseconds at worst. A quarter of a second to it is not distance,
/// it is a buffer filling up, and the equipment holding it is on the
/// line of whichever computer measures it.
const QUEUED: Duration = Duration::from_millis(250);

/// What the socket is asked to hold of what arrives while nothing is
/// reading it.
///
/// The system's own default is a few dozen kilobytes, which at the rate
/// a picture travels is about ten milliseconds: any moment this program
/// is not given the processor costs packets, and they are lost before
/// anything here can even count them. It is the one queue of the whole
/// road that nobody but the system holds, and it was left at whatever
/// the system felt like.
const ARRIVING_ROOM: usize = 8 * 1024 * 1024;

/// Silence on the socket worth writing down, while a session is open.
///
/// A tunnel that stands is answered several times a second: a whole
/// second with nothing at all handed over by the system is not a far
/// computer taking its time, it is a computer that has gone deaf.
const DEAF: Duration = Duration::from_secs(1);

/// A look-over this late means the computer itself stopped running.
///
/// Ten turns of the clock: a machine busy elsewhere misses a turn or two
/// and nobody is the wiser, and a whole second of not running is not
/// something a computer carrying a session does.
const LATE: Duration = Duration::from_secs(1);

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

/// How long a road already elected is trusted on its probes alone,
/// with nothing real proven yet.
///
/// An echo proves a road carries a light, regular datagram both ways;
/// it proves nothing of the heavier, far less regular traffic of a
/// real connection, and least of all while that connection is still
/// finding its way at all. A first connection races its candidate
/// roads under a patience of its own, fifteen seconds elsewhere in
/// this product: cutting a road out from under a handshake that only
/// needed a little longer trades a slow success for a certain
/// failure, since whatever replaces it starts that same race over
/// with nothing measured yet, and can be cut short in its turn before
/// it too has had the time to answer. Kept safely past that patience,
/// a road still unproven can only mean what this was written for: a
/// connection already open, answering every probe, and carrying
/// nothing real for far longer than establishing one ever should.
const PROVEN_WITHIN: Duration = Duration::from_secs(20);

/// Turns the packets a relay brought may win in a row before the socket
/// is read.
///
/// What a relay brought is already sorted and named, so it goes first;
/// but « first » applied to every turn is « only ». The transport asks
/// for one packet a turn on Windows, so a relay delivering steadily
/// would hold the direct socket shut, and the probes that would find a
/// direct road would never be read. One turn in eight is enough to keep
/// that door open and costs the relayed road nothing that can be felt.
const RELAY_TURNS: u64 = 8;

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
    /// Whether the last probe on this road actually left the machine.
    ///
    /// A probe the socket refused and a probe nobody answered leave the
    /// same trace here: neither is echoed. Only the second says anything
    /// about the road. Counting the first as a miss is how a road that
    /// was working got given up on this computer's own hiccup, three
    /// times an hour on the fifth of September.
    asked: bool,
    /// The last moment real traffic, not a probe of ours, was seen to
    /// come by this road: what an echo alone can never prove, since a
    /// road can answer every probe sent its way and carry nothing else.
    proven_at: Option<Instant>,
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
    /// Whether it has already said the branch is queueing on the way
    /// out. This one comes and goes, so both are worth a line.
    said_slow: bool,
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
    /// Buffers the transport handed over while no road was elected, and
    /// which were therefore swallowed rather than sent.
    ///
    /// The junction tells the transport a buffer left even when it had
    /// nowhere to put it, because saying otherwise would spin: the
    /// poller under it watches a socket that is always writable. Since
    /// nothing else can notice, this counts, and the journal says every
    /// one of them. The gravest thing this file does was the one thing
    /// it did in silence.
    swallowed: u64,
    /// How many of them the journal has already accounted for. A count
    /// and not a flag: a second black hole, later in the same session,
    /// was silent because the first had used up the one line allowed.
    said_swallowed: u64,
    next_number: u32,
    in_flight: Vec<InFlight>,
    /// The last moment real traffic crossed any road of this card, for
    /// as long as it has been expected.
    ///
    /// A road that is given up loses everything it had proven with it:
    /// a fresh one at the same address starts again with nothing
    /// measured. This is the one memory that survives that, because
    /// what it answers is not whether this particular road works but
    /// whether the two computers can still talk at all, and how long
    /// ago they last showed it. A card proven within `PROVEN_WITHIN`
    /// is not the card that constant was written for; one gone quiet
    /// for that long on every road it has tried since is exactly it,
    /// however far in its past the first proof was.
    proven_since: Option<Instant>,
    /// Whether the elected road has answered again since this was last
    /// asked, having gone quiet for at least one missed probe, or for
    /// long enough to be given up outright and reappear as a fresh one.
    ///
    /// Taken and cleared by the asking. A connection speaking to this
    /// card has no way of knowing its own retries have gone stale, a
    /// road switching under it being exactly what this transport is
    /// for; this is the one sign, short of that connection's own
    /// packets, that is worth telling it about.
    recovered: bool,
}

/// What one look-over of a card's roads found.
struct LookedOver {
    /// Roads to probe now: candidates whose turn it is, and the roads
    /// that answered and are due their next measurement.
    probe: Vec<Through>,
    /// Roads that were probed and said nothing, too many times running.
    given_up: Vec<Through>,
    /// Roads that have just missed their first echo: the moment trouble
    /// starts, which is six seconds before it is certain.
    missed: Vec<Through>,
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
            swallowed: 0,
            said_swallowed: 0,
            next_number: 1,
            in_flight: Vec::new(),
            proven_since: None,
            recovered: false,
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

    /// What the transport knows of the branch, when that road is the
    /// relay and there is one, for a journal line that names trouble.
    fn how_the_branch_stands(&self, through: Through) -> String {
        match (through, &self.relay) {
            (Through::Relay(_), Some(relaying)) => {
                format!(" ({})", relaying.branch.how_it_stands())
            }
            _ => String::new(),
        }
    }

    /// Every road of this card and where each one stands, on one line.
    ///
    /// What every other line here says is an outcome: a road taken, a
    /// road given up, a session that would not open. None of them says
    /// what the choice was made from, and a connection that fails once
    /// in two is a choice made from something this journal never wrote
    /// down. This is that something: each road, whether its last probe
    /// left this computer at all, whether it was answered, how long it
    /// took, how many probes it has missed, which of them is carrying
    /// the session, and whether real traffic has ever crossed the one
    /// that is.
    ///
    /// Written when it changes and never on a clock. The look-over runs
    /// ten times a second and would fill a journal in twelve seconds;
    /// what is worth reading is the moment a road moved, and between two
    /// movements there is nothing to say.
    ///
    /// `timed` is what tells the two uses apart, and it is the whole of
    /// why this is one function. The line a journal keeps carries how
    /// long each road takes; the line held to know whether anything
    /// moved must not, because a round trip changes at every probe and a
    /// session would then write this out once a second for as long as it
    /// lasts, which is the journal with nothing else left in it.
    fn how_the_roads_stand(&self, timed: bool) -> String {
        let mut said = String::new();
        for path in &self.paths {
            if !said.is_empty() {
                said.push_str(" ; ");
            }
            said.push_str(&self.named(path.through));
            if Some(path.through) == self.elected {
                said.push_str(" [en service]");
                if path.proven_at.is_none() {
                    said.push_str(", jamais traversée par du trafic réel");
                }
            }
            if timed {
                said.push_str(&format!(" {} ms", path.round_trip.as_millis()));
            }
            if path.misses > 0 {
                said.push_str(&format!(", {} sondes sans réponse", path.misses));
            }
            if !path.asked {
                said.push_str(", dernière sonde jamais partie d'ici");
            }
        }
        for candidate in &self.candidates {
            if self
                .paths
                .iter()
                .any(|path| path.through == candidate.through)
            {
                continue;
            }
            if !said.is_empty() {
                said.push_str(" ; ");
            }
            said.push_str(&self.named(candidate.through));
            said.push_str(if candidate.probed.is_some() {
                " sondée, sans réponse"
            } else {
                " jamais sondée"
            });
        }
        if said.is_empty() {
            said.push_str("aucune route connue");
        }
        said
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
    /// A road given up is never the road in use afterwards, as long as
    /// there is another: handing packets to a road that has stopped
    /// answering loses every one of them, where the election moving to a
    /// road that answers loses none.
    ///
    /// The last road is never given up, and that is the whole of the
    /// difference. Letting the election fall empty was meant to keep the
    /// packets and send them whole the moment a road answered; what it
    /// keeps is eight of them, a hundredth of a second of picture, and
    /// everything else is swallowed while the far computer is told
    /// nothing at all, probes and answers included. A road that has
    /// missed three probes is not a road proven dead, only one that has
    /// stopped saying it is alive: sending on it costs nothing, since the
    /// alternative sends nowhere, and it is what the far computer needs
    /// to hear to stop counting the silence.
    fn look_over(&mut self, now: Instant) -> LookedOver {
        let every = self.every(now);
        let mut due = Vec::new();
        let mut given_up = Vec::new();
        let mut missed = Vec::new();
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
        // What giving up may not take away: a road to send on. Counted
        // before, because a road removed is one road fewer for the road
        // after it in the same pass.
        let mut left = self.paths.len();
        self.paths.retain_mut(|path| {
            let keep_every = if Some(path.through) == elected {
                KEEP_EVERY
            } else {
                WARM_EVERY
            };
            if now.duration_since(path.probed) < keep_every {
                return true;
            }
            if !path.asked {
                // The last probe never left this computer. Its silence
                // is ours, and holding it against the road would give up
                // one that was carrying the session.
            } else if path.echoed < path.probed {
                path.misses += 1;
                if path.misses == 1 {
                    missed.push(path.through);
                }
            } else {
                path.misses = 0;
            }
            if path.misses >= MISSES_TO_DIE && left > 1 {
                left -= 1;
                given_up.push(path.through);
                return false;
            }
            path.probed = now;
            path.asked = true;
            due.push(path.through);
            true
        });
        self.in_flight
            .retain(|probe| now.duration_since(probe.at) < PROBE_LIFE);
        LookedOver {
            probe: due,
            given_up,
            missed,
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
        // The road quinn's own traffic actually rides, answering after
        // a gap: what the connection above this card has no other way
        // of learning, since the whole point of a card is that a road
        // can be given up and taken up again without it ever knowing.
        if Some(through) == self.elected {
            let was_quiet = self
                .paths
                .iter()
                .find(|path| path.through == through)
                .is_none_or(|path| path.misses > 0);
            self.recovered |= was_quiet;
        }
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
                    asked: true,
                    proven_at: None,
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

    /// Real traffic, not a probe of ours, was just seen to come by that
    /// road: the one proof an echo cannot forge on its own.
    fn proven(&mut self, through: Through, now: Instant) {
        if let Some(path) = self.paths.iter_mut().find(|path| path.through == through) {
            path.proven_at = Some(now);
        }
        self.proven_since = Some(now);
    }

    /// Whether the elected road has come back since this was last asked.
    fn take_recovery(&mut self) -> bool {
        std::mem::take(&mut self.recovered)
    }

    /// The road worth taking: the shortest direct one, and the relay
    /// only while no direct one answers.
    ///
    /// Among the roads that are still answering first, and among the
    /// others only when none is. A road keeps its last good measurement
    /// right up to the moment it is given up, so the sickest road in the
    /// list is often the shortest one in it, and choosing on length alone
    /// hands the session to the road that is dying. What makes a road
    /// worth taking is that something comes back on it.
    fn best(&self) -> Option<&Path> {
        self.best_among(|path| path.misses == 0)
            .or_else(|| self.best_among(|_| true))
    }

    /// The same rule, read over the roads worth considering.
    fn best_among(&self, worth: impl Fn(&Path) -> bool) -> Option<&Path> {
        self.paths
            .iter()
            .filter(|path| worth(path))
            .filter(|path| !path.through.relayed())
            .min_by_key(|path| path.round_trip)
            .or_else(|| self.paths.iter().find(|path| worth(path)))
    }

    /// The same rule again, a road set aside: the best of every other
    /// one, for a road that has already had its turn and is not to be
    /// its own replacement.
    fn best_other_than(&self, exclude: Through) -> Option<&Path> {
        self.best_among(|path| path.misses == 0 && path.through != exclude)
            .or_else(|| self.best_among(|path| path.through != exclude))
    }

    /// Chooses the road in use. Says what changed, if anything did.
    ///
    /// Between two direct roads, the shorter, but not for a difference
    /// nobody would feel: switching for a hair is how a session ends up
    /// swinging between two equals. Against the relay there is no such
    /// margin, in either direction: a direct road that answers takes the
    /// session at once, and a direct road that dies gives it back at
    /// once.
    ///
    /// And nothing at all while no road answers. When every road has
    /// missed its probe, `best` falls back to the roads that are dying,
    /// where a direct one outranks the relay however sick it is; the
    /// session then leaves the relay it had just taken, comes back to
    /// the road it had just left, and spends the blackout swinging
    /// between the two instead of waiting it out on one. Both ends of a
    /// road going quiet at once is what a link that comes and goes looks
    /// like, and it lasts seconds, not minutes: the road carrying the
    /// session keeps it until another road actually answers.
    ///
    /// Neither margin above is a road's forever: an echo proves a road
    /// answers, not that it carries anything, and a road held past
    /// `PROVEN_WITHIN` on that proof alone gives up its shelter to the
    /// best of the others, exactly as a road that stopped answering
    /// does. The other side of the same coin is a road real traffic has
    /// already crossed: it keeps its shelter against a challenger of its
    /// own kind that has not done the same, whatever the challenger
    /// measures, because a probe is the one thing that can make a dead
    /// road look exactly like a live one.
    ///
    /// That patience is longer, not gone, for a card that has already
    /// shown it works. A road given up for a burst of missed probes, a
    /// real outage of a few seconds among them, takes its shelter with
    /// it: the one answering in its place at the same address a moment
    /// later is a fresh road as far as this file knows, proof and all.
    /// Judging it from the instant it reappears would punish the very
    /// moment a session is recovering from a blackout for a card that
    /// has already shown it works, so it is judged instead from the
    /// last moment anything real crossed this card at all, on whatever
    /// road carried it: a card silent for less than `PROVEN_WITHIN`
    /// since then is still within the outage it is recovering from,
    /// however long its unproven road has been sitting elected. What
    /// this must not become is the shelter D168 wrote against, kept
    /// forever on the strength of a proof this old: a card that has
    /// carried nothing real for a whole `PROVEN_WITHIN` of its own,
    /// on every road it has tried in that time, has stopped
    /// recovering from anything and become the card this constant was
    /// written for, whatever it once proved.
    fn elect(&mut self, now: Instant) -> Option<Elected> {
        let current = self
            .elected
            .and_then(|through| self.paths.iter().find(|path| path.through == through));
        let established = self
            .proven_since
            .is_some_and(|proven| now.duration_since(proven) < PROVEN_WITHIN);
        let overdue = !established
            && current.is_some_and(|current| {
                current.proven_at.is_none() && now.duration_since(self.elected_at) >= PROVEN_WITHIN
            });
        let best = match current {
            Some(current) if overdue => self
                .best_other_than(current.through)
                .or_else(|| self.best()),
            _ => self.best(),
        }?;
        let chosen = match current {
            // Nothing answers anywhere, `best` included.
            Some(current) if best.misses > 0 => current.through,
            // The margin is there so a session does not swing between
            // two equals; it is not there to keep a road that has
            // stopped answering, which is not the equal of anything, nor
            // one that has had `PROVEN_WITHIN` to carry something real
            // and has not. A road real traffic has already crossed gets
            // the same margin against a challenger that has not,
            // whatever it measures: proof outranks a guess, however good
            // the guess.
            //
            // `misses < MISSES_TO_DIE` and not `== 0`: a road one probe
            // has failed to answer is not a road that stopped
            // answering, everywhere else in this file, and losing the
            // margin at the first miss is how a link that drops one
            // packet in twenty swings between two roads on every one of
            // them.
            Some(current)
                if !overdue
                    && current.misses < MISSES_TO_DIE
                    && current.through.relayed() == best.through.relayed()
                    && (current.round_trip <= best.round_trip + HYSTERESIS
                        || (current.proven_at.is_some() && best.proven_at.is_none())) =>
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
            self.swallowed += 1;
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
    /// Datagrams the socket has handed over, ours and the transport's
    /// alike, and how many of those were ours. The only place that says
    /// whether a computer gone deaf is still being given anything at
    /// all: everything else here measures what leaves.
    arrived: AtomicU64,
    ours: AtomicU64,
    /// Packets of ours the socket would not take, and probes that had
    /// no road to leave by.
    ///
    /// Sending one is best effort: a probe that does not go out is one
    /// that will be sent again. But a probe that never left this
    /// computer and a probe nobody answered read exactly the same from
    /// here, and it is always the second the journal has said: « stopped
    /// answering and is given up ». On a session that goes quiet, those
    /// two are the whole question, and this counter is the only thing
    /// that tells them apart.
    refused: AtomicU64,
    /// Turns of `poll_recv`, so the socket keeps one in `RELAY_TURNS`.
    turns: AtomicU64,
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
    ///
    /// `marking` says whether the packets leave with the transport's
    /// congestion mark on them, probes and echoes included: they are
    /// there to prove a road for the packets that follow, and a road is
    /// not proved by something plainer than those.
    pub fn bind(
        listen: SocketAddr,
        identity: Arc<Identity>,
        say: Say,
        marking: Marking,
    ) -> io::Result<Self> {
        let runtime = quinn::default_runtime()
            .ok_or_else(|| io::Error::other("aucun exécuteur asynchrone"))?;
        let socket = bind_socket(listen)?;
        let ipv6 = socket.local_addr()?.is_ipv6();
        let room = arriving_room(&socket);
        if room < ARRIVING_ROOM {
            say(
                Aloud::Says,
                &format!(
                    "the system holds {room} bytes of what arrives on this socket, not the \
                     {ARRIVING_ROOM} asked for: what comes in faster than this program is given \
                     the processor is lost before anything can count it"
                ),
            );
        }
        let socket = marking.applied(runtime.wrap_udp_socket(socket)?);
        let me = identity.fingerprint();
        let inner = Arc::new(Inner {
            socket,
            arrived: AtomicU64::new(0),
            ours: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            turns: AtomicU64::new(0),
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
                said_slow: false,
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

    /// Whether that card is still held for that session.
    ///
    /// For whoever keeps something alive alongside a session and has to
    /// know when to stop: a branch of relay held open, and nothing else
    /// so far. Told of the session and not of the card alone, for the
    /// reason D133 was written: a card outlives the session that took it,
    /// and answering « yes » for the one that follows would keep a branch
    /// open on a pass that belongs to nobody.
    pub fn still_expects(&self, card: SocketAddr, session: &str) -> bool {
        self.inner
            .table
            .lock()
            .expect("aiguilleur")
            .expected
            .get(&card)
            .is_some_and(|held| held.session == session)
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

    /// Whether the road to that card has just come back after going
    /// quiet, since this was last asked.
    ///
    /// Probing keeps going whether or not the tunnel has anything to
    /// send, so it notices a road working again before any traffic on
    /// it would: a connection speaking to this card has no other way of
    /// knowing its own retries have gone stale, a road switching under
    /// it being exactly what this transport is for.
    pub fn recovered(&self, card: SocketAddr) -> bool {
        let mut table = self.inner.table.lock().expect("aiguilleur");
        table
            .expected
            .get_mut(&card)
            .is_some_and(Expected::take_recovery)
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
    /// Whether what a relay brought is served before the socket, this
    /// turn.
    fn relay_goes_first(&self) -> bool {
        !self
            .turns
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(RELAY_TURNS)
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// An address as the socket wants it: mapped when it speaks IPv6.
    fn outward(&self, address: SocketAddr) -> SocketAddr {
        sifting::outward(address, self.ipv6)
    }

    /// Sends one datagram of ours, best effort: a probe that does not go
    /// out is a probe that will be sent again.
    fn send_to(&self, destination: SocketAddr, contents: &[u8]) -> bool {
        let went = self.socket.try_send(&Transmit {
            destination: self.outward(destination),
            // Marked as the transport marks its own, and taken off again
            // by the same socket when this computer was told not to
            // mark: a road is proved by the packet a session will put on
            // it, and a mark some middle box throws packets away for is
            // part of that packet. Unmarked, these went through a road
            // that dropped every packet of the session it had just been
            // given.
            ecn: Some(EcnCodepoint::Ect0),
            contents,
            segment_size: None,
            src_ip: None,
        });
        if went.is_err() {
            self.refused.fetch_add(1, Ordering::Relaxed);
        }
        went.is_ok()
    }

    /// Sends one datagram of ours by that road, whichever it is. Says
    /// whether it left this computer.
    fn send_by(&self, road: Through, contents: &[u8]) -> bool {
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
                match branch {
                    // A branch with no room for it is a packet that did
                    // not leave either, and so is a road whose branch
                    // has gone: both are counted where the journal can
                    // say them.
                    Some(branch) if branch.send(contents) => true,
                    _ => {
                        self.refused.fetch_add(1, Ordering::Relaxed);
                        false
                    }
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
        let mut probes: Vec<(SocketAddr, Through, Probe)> = Vec::new();
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
                for road in looked.missed {
                    said.push(format!(
                        "card {card}: {} did not answer a probe{}",
                        expected.named(road),
                        expected.how_the_branch_stands(road)
                    ));
                }
                for road in looked.given_up {
                    said.push(format!(
                        "card {card}: {} stopped answering and is given up{}",
                        expected.named(road),
                        expected.how_the_branch_stands(road)
                    ));
                }
                for road in looked.probe {
                    let number = expected.number(now);
                    probes.push((
                        *card,
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
                    // What the transport measures on its own, and what
                    // the junction's own probes cannot: a branch whose
                    // window is full sends nothing at all, probes
                    // included, so the road looks as short as ever right
                    // up to the moment it is given up for dead. This
                    // number keeps moving where that one stops.
                    let slow = relaying.branch.round_trip() > QUEUED;
                    if slow != relaying.said_slow {
                        relaying.said_slow = slow;
                        said.push(format!(
                            "card {card}: the branch to the relay at {} {}, {} ms to it",
                            relaying.branch.address(),
                            if slow {
                                "is waiting in a queue somewhere on the way out"
                            } else {
                                "is answering promptly again"
                            },
                            relaying.branch.round_trip().as_millis()
                        ));
                    }
                }
                if expected.swallowed > expected.said_swallowed {
                    said.push(format!(
                        "card {card}: no road is elected, so {} packet(s) the transport handed \
                         over went nowhere and it was told they had left",
                        expected.swallowed - expected.said_swallowed
                    ));
                    expected.said_swallowed = expected.swallowed;
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
            (self.say)(Aloud::Says, &line);
        }
        let mut never_left = Vec::new();
        for (card, road, probe) in probes {
            match probe::seal_probe(&self.identity, &probe) {
                Ok(bytes) if self.send_by(road, &bytes) => {}
                _ => never_left.push((card, road)),
            }
        }
        self.not_asked_after_all(&never_left);
        for (road, held) in flushed {
            self.send_held(road, &held);
        }
    }

    /// Takes back the probes that were decided and never left.
    ///
    /// Written down where the next look-over reads it, so a road is
    /// never given up on a silence this computer caused itself.
    fn not_asked_after_all(&self, never_left: &[(SocketAddr, Through)]) {
        if never_left.is_empty() {
            return;
        }
        let mut table = self.table.lock().expect("aiguilleur");
        for (card, road) in never_left {
            if let Some(expected) = table.expected.get_mut(card)
                && let Some(path) = expected.paths.iter_mut().find(|path| path.through == *road)
            {
                path.asked = false;
            }
        }
    }

    /// Whether anything at all is still arriving on the socket, while a
    /// computer is expected behind a card.
    ///
    /// Every other measurement here is of what leaves: what was handed
    /// over, what went out, how long the road took to answer. None of
    /// them can tell a far computer that stopped speaking from a socket
    /// that stopped being given anything, and the two are the same
    /// silence seen from here. This one counts what comes in, before it
    /// is looked at or sorted.
    fn still_hearing(
        &self,
        last: &mut (u64, Instant),
        said: &mut bool,
        now: Instant,
    ) -> Option<String> {
        let arrived = self.arrived.load(Ordering::Relaxed);
        if arrived != last.0 {
            *last = (arrived, now);
            if !*said {
                return None;
            }
            *said = false;
            return Some(format!(
                "the socket is being given packets again, {arrived} in all, {} of them ours",
                self.ours.load(Ordering::Relaxed)
            ));
        }
        let quiet = now.duration_since(last.1);
        if *said || quiet < DEAF || self.table.lock().expect("aiguilleur").expected.is_empty() {
            return None;
        }
        *said = true;
        Some(format!(
            "not one packet has been given to the socket for {} ms, though a session is open:              {arrived} arrived before that, {} of them ours",
            quiet.as_millis(),
            self.ours.load(Ordering::Relaxed)
        ))
    }

    /// Where every road of every card stands, for the cards where that
    /// has moved since the last look.
    ///
    /// The whole state and not the change: a line saying « this road
    /// missed one » is read next to nothing, where a line naming all of
    /// them says at once whether there was anything else to take. It
    /// costs one string per card per look-over, built and thrown away
    /// when it matches the last.
    fn roads_that_moved(&self, said: &mut HashMap<SocketAddr, String>) -> Vec<String> {
        let table = self.table.lock().expect("aiguilleur");
        let mut lines = Vec::new();
        for (card, expected) in &table.expected {
            let moved = expected.how_the_roads_stand(false);
            if said.get(card).is_some_and(|before| *before == moved) {
                continue;
            }
            lines.push(format!(
                "card {card}: {}",
                expected.how_the_roads_stand(true)
            ));
            said.insert(*card, moved);
        }
        // A card nobody expects any more is a session that ended, and
        // the next one towards the same computer starts from nothing.
        said.retain(|card, _| table.expected.contains_key(card));
        lines
    }

    /// Whether the socket is taking what this junction hands it.
    ///
    /// Said as it happens and never again for the same packets: what is
    /// worth reading is the moment this computer stopped being able to
    /// speak, next to the moment it stopped being spoken to.
    fn still_sending(&self, said: &mut u64) -> Option<String> {
        let refused = self.refused.load(Ordering::Relaxed);
        if refused == *said {
            return None;
        }
        let since = refused - *said;
        *said = refused;
        Some(format!(
            "{since} packet(s) of this computer's own did not leave it, {refused} in all: a road \
             given up after this was given up on a probe that never went out"
        ))
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
                    (self.say)(Aloud::Says, &line);
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
                // Reached here only for what is not a probe of ours: real
                // traffic, on its way to the transport. Its mere arrival
                // is what an echo alone cannot prove of the road it just
                // came by.
                let mut table = self.table.lock().expect("aiguilleur");
                let card = *table.by_real.get(&from)?;
                if let Some(expected) = table.expected.get_mut(&card) {
                    expected.proven(Through::Direct(from), Instant::now());
                }
                Some(self.outward(card))
            },
        );
        self.arrived.fetch_add(count as u64, Ordering::Relaxed);
        self.ours
            .fetch_add((count - kept) as u64, Ordering::Relaxed);
        for (road, bytes) in answers {
            self.send_by(road, &bytes);
        }
        kept
    }

    /// One packet a relay handed over, for the computer behind that
    /// card: answered here when it is ours, and queued for the transport
    /// otherwise.
    fn arrived_by_relay(&self, card: SocketAddr, packet: Bytes) {
        self.arrived.fetch_add(1, Ordering::Relaxed);
        if probe::is_ours(&packet) {
            self.ours.fetch_add(1, Ordering::Relaxed);
            if let Some((road, answer)) = self.heard(Through::Relay(card), &packet) {
                self.send_by(road, &answer);
            }
            return;
        }
        let waiting = {
            let mut table = self.table.lock().expect("aiguilleur");
            if let Some(expected) = table.expected.get_mut(&card) {
                expected.proven(Through::Relay(card), Instant::now());
            }
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
                (inner.say)(
                    Aloud::Says,
                    &format!(
                        "card {card}: the branch to the relay at {} is gone",
                        branch.address()
                    ),
                );
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
            // What a relay brought is already sorted and named, so it
            // goes first: nearly every turn, and not every one.
            // `RELAY_TURNS` says why the socket keeps a turn of its own.
            if self.inner.relay_goes_first() {
                let taken = self.inner.take_relayed(cx, bufs, meta);
                if taken > 0 {
                    return Poll::Ready(Ok(taken));
                }
            }
            let count = match self.inner.socket.poll_recv(cx, bufs, meta) {
                Poll::Ready(Ok(count)) => count,
                // Nothing on the socket, and its waker is registered:
                // what a relay brought is the other half of the answer,
                // and asking for it registers the other waker.
                Poll::Pending => {
                    let taken = self.inner.take_relayed(cx, bufs, meta);
                    if taken > 0 {
                        return Poll::Ready(Ok(taken));
                    }
                    return Poll::Pending;
                }
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
///
/// And says when it was not run on time. A computer that stops running
/// for a few seconds sends nothing at all meanwhile, roads and probes
/// included, and from the far end that is indistinguishable from a
/// network that stopped carrying: both leave a session that goes quiet
/// and dies of it. Only this end can tell them apart, and only if it
/// looks at its own clock.
async fn look_after(junction: Weak<Inner>) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut before = Instant::now();
    let mut last_arrival = (0u64, Instant::now());
    let mut said_deaf = false;
    let mut said_refused = 0u64;
    let mut said_roads: HashMap<SocketAddr, String> = HashMap::new();
    loop {
        tick.tick().await;
        let now = Instant::now();
        let Some(inner) = junction.upgrade() else {
            return;
        };
        let since = now.duration_since(before);
        before = now;
        if since > LATE {
            (inner.say)(
                Aloud::Says,
                &format!(
                    "this computer did not run for {} ms, and sent nothing at all meanwhile",
                    since.as_millis()
                ),
            );
        }
        // Counted and not witnessed: this pair says the socket went
        // quiet and spoke again, which it does a dozen times while two
        // computers are still finding each other. It is what a hunt
        // reads, and what drowns everything else.
        if let Some(line) = inner.still_hearing(&mut last_arrival, &mut said_deaf, now) {
            (inner.say)(Aloud::Hunts, &line);
        }
        if let Some(line) = inner.still_sending(&mut said_refused) {
            (inner.say)(Aloud::Says, &line);
        }
        inner.tick(now);
        // After the look-over and not before: what is worth reading is
        // where the roads stand once this turn has probed, answered and
        // elected, not where they stood a tenth of a second ago.
        for line in inner.roads_that_moved(&mut said_roads) {
            (inner.say)(Aloud::Says, &line);
        }
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
        make_room(&socket);
        return Ok(socket);
    }
    let socket = std::net::UdpSocket::bind(listen)?;
    socket.set_nonblocking(true)?;
    make_room(&socket);
    Ok(socket)
}

/// Asks the system to hold that much of what arrives before anything
/// reads it.
///
/// Best effort on purpose: a system that grants less is not a reason to
/// refuse a session, and what it really granted is read back and said.
fn make_room(socket: &std::net::UdpSocket) {
    let _ = socket2::SockRef::from(socket).set_recv_buffer_size(ARRIVING_ROOM);
}

/// What the system really holds for that socket.
pub fn arriving_room(socket: &std::net::UdpSocket) -> usize {
    socket2::SockRef::from(socket)
        .recv_buffer_size()
        .unwrap_or(0)
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
        Arc::new(|_, _| {})
    }

    fn local() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    fn junction(identity: &Arc<Identity>) -> Junction {
        Junction::bind(local(), identity.clone(), quiet(), Marking::Ecn).unwrap()
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
        // Each knows only the real address of the other, like a
        // candidate that came from the server.
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

        // The transport knows only the cards.
        assert_eq!(client_side.remote_address(), pair.host_card);
        assert_eq!(host_side.remote_address(), pair.client_card);
        // And the junction knows which way it really goes.
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

        // This real traffic proved the road on the host side, which
        // received it: a probe alone would never have done so.
        {
            let table = pair.host.inner.table.lock().expect("aiguilleur");
            let expected = table.expected.get(&pair.client_card).unwrap();
            let elected = expected.elected.unwrap();
            let path = expected
                .paths
                .iter()
                .find(|path| path.through == elected)
                .unwrap();
            assert!(
                path.proven_at.is_some(),
                "le trafic réel reçu n'a pas prouvé la route"
            );
        }

        // Each was seen by the other, at its real address.
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
        // The connection starts at once, before any address is known:
        // its packets wait.
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
        // The case the relay exists to carry: nothing connects
        // directly at the start, the session starts anyway, and the
        // direct road takes it over as soon as it is validated,
        // without a break.
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

        // Each end opens its branch: no address of the other is known,
        // and none will be before the switch.
        for (junction, card, identity) in [
            (&host, client_card, &host_identity),
            (&client, host_card, &client_identity),
        ] {
            let branch = crate::relay::Branch::open(
                &relay.wanted(b"laissez-passer"),
                identity,
                crate::congestion::Sending::Pictures,
                MediaProfile::default(),
                Marking::Ecn,
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

        // The relay is proven on the host side, which received this
        // real traffic.
        {
            let table = host.inner.table.lock().expect("aiguilleur");
            let expected = table.expected.get(&client_card).unwrap();
            let path = expected
                .paths
                .iter()
                .find(|path| path.through == Through::Relay(client_card))
                .unwrap();
            assert!(
                path.proven_at.is_some(),
                "le relais prouvé n'a pas été noté"
            );
        }

        // The direct road becomes possible: the switch is immediate,
        // and the same connection goes on, on the same card. A proven
        // road only protects against a stranger of its own kind; the
        // direct road takes no account of what the relay has just
        // proven.
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
        // It is the transport that reads the socket: without an endpoint
        // on it, nobody would hear anything.
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
        // The stranger passes itself off as the expected
        // client.
        let forged = probe::seal_probe(&stranger, &probe).unwrap();
        raw.send_to(&forged, door.local_address().unwrap())
            .await
            .unwrap();
        let mut buf = [0u8; 1500];
        let answered =
            tokio::time::timeout(Duration::from_millis(300), raw.recv_from(&mut buf)).await;
        assert!(answered.is_err(), "un écho est parti vers un inconnu");
        assert!(door.road(card).is_none());

        // The real client, for its part, gets its echo.
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

        // A silent mirror: nothing, without getting stuck.
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
        // A second path barely shorter is not worth a switch.
        let second = expected.number(now);
        assert!(expected.answered(b, second, Duration::from_millis(18), now));
        assert_eq!(moved(expected.elect(now)), None);
        // Clearly shorter, it is.
        let third = expected.number(now);
        assert!(expected.answered(b, third, Duration::from_millis(1), now));
        assert_eq!(moved(expected.elect(now)), Some((Some(a), b)));
        // An echo to an unknown number does not count.
        assert!(!expected.answered(a, 999, Duration::from_millis(1), now));
    }

    #[test]
    fn a_road_real_traffic_has_crossed_is_not_dropped_for_an_unproven_stranger() {
        // A probe proves only the probe: a stranger that answers
        // clearly faster, beyond any ordinary margin, must not take
        // the place of a road that real traffic has already crossed,
        // as long as it has proven nothing itself.
        let now = Instant::now();
        let mut expected = expecting(now);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(now);
        assert!(expected.answered(a, first, Duration::from_millis(20), now));
        assert_eq!(moved(expected.elect(now)), Some((None, a)));
        expected.proven(a, now);

        let second = expected.number(now);
        assert!(expected.answered(b, second, Duration::from_millis(1), now));
        assert_eq!(
            moved(expected.elect(now)),
            None,
            "une route prouvée a cédé la place à une inconnue plus rapide mais non prouvée"
        );

        // It proves itself in its turn: the ordinary rule takes over
        // again between two roads now equal in proof.
        expected.proven(b, now);
        assert_eq!(moved(expected.elect(now)), Some((Some(a), b)));
    }

    #[test]
    fn a_road_that_never_carries_anything_gives_way_once_overdue() {
        // A road answers every probe, always the best measured, and
        // yet nothing real has ever gone out on it. Past
        // PROVEN_WITHIN, another road that answers must be able to try
        // in its place.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(start);
        assert!(expected.answered(a, first, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));

        // b answers too, clearly slower: the ordinary margin leaves the
        // session on a, which has nevertheless never carried anything.
        let second = expected.number(start);
        assert!(expected.answered(b, second, Duration::from_millis(80), start));
        assert_eq!(moved(expected.elect(start)), None);

        // Time passes: a still answers the probes, faster than b, but
        // has never let a single real byte through. b gets its turn
        // despite its worse measurement.
        let later = start + PROVEN_WITHIN;
        assert_eq!(
            moved(expected.elect(later)),
            Some((Some(a), b)),
            "une route jamais prouvée a gardé la main indéfiniment"
        );

        // b proves itself: the ordinary margin applies again, this time
        // in its favour, even though a still measures shorter.
        expected.proven(b, later);
        assert_eq!(moved(expected.elect(later)), None);
    }

    #[test]
    fn a_card_recovering_from_a_real_outage_gets_its_road_a_fair_chance() {
        // On 7 September (D168): a road already proven for several
        // minutes is given up for dead during a real network outage
        // of a few seconds (three missed probes, six seconds), and
        // a road at the same address answers straight after, new to
        // the junction since the one that carried the proof
        // vanished with the old one. It too must have its twenty
        // seconds to prove itself, without being judged on the
        // spot.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let number = expected.number(start);
        assert!(expected.answered(a, number, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));
        expected.proven(a, start);

        // The outage: a given up for dead, b takes over for a
        // moment, as long as it lasts.
        let outage = start + Duration::from_secs(6);
        expected.paths.retain(|path| path.through != a);
        let number = expected.number(outage);
        assert!(expected.answered(b, number, Duration::from_millis(5), outage));
        assert_eq!(moved(expected.elect(outage)), Some((Some(a), b)));

        // a answers again, a moment later: a brand new road to the
        // junction, which has proven nothing by itself yet.
        let reborn = outage + Duration::from_millis(500);
        let number = expected.number(reborn);
        assert!(expected.answered(a, number, Duration::from_millis(1), reborn));
        assert_eq!(moved(expected.elect(reborn)), Some((Some(b), a)));

        // Just short of its own twenty seconds, still nothing
        // proven on this new road: yet it must not be penalised,
        // the card still being in the outage it is recovering
        // from.
        let still_fresh = reborn + PROVEN_WITHIN - Duration::from_millis(1);
        assert_eq!(
            moved(expected.elect(still_fresh)),
            None,
            "une route qui venait de reprendre après une vraie coupure a perdu sa place trop tôt"
        );
    }

    #[test]
    fn a_card_long_proven_still_gives_way_once_its_own_road_stays_silent() {
        // On 8 September: a card that had been carrying real
        // traffic for nineteen minutes loses its road, tries
        // another for the length of an outage of a few seconds,
        // then settles on a third that answers every probe without
        // ever carrying anything real again. Nothing ever called
        // that road into question again, the card having already
        // proven itself, and the session died thirty seconds later
        // for never having received anything (D173).
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let c = direct("10.0.0.3:47000");
        let number = expected.number(start);
        assert!(expected.answered(a, number, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));
        expected.proven(a, start);

        // The outage: a given up for dead, b takes over for a
        // moment.
        let outage = start + Duration::from_secs(6);
        expected.paths.retain(|path| path.through != a);
        let number = expected.number(outage);
        assert!(expected.answered(b, number, Duration::from_millis(5), outage));
        assert_eq!(moved(expected.elect(outage)), Some((Some(a), b)));

        // c answers in its turn, a little faster, and takes the
        // place of b: the road that will carry the card to the
        // end, without ever letting anything real through on it.
        let settled = outage + Duration::from_millis(500);
        let number = expected.number(settled);
        assert!(expected.answered(c, number, Duration::from_millis(1), settled));
        assert_eq!(moved(expected.elect(settled)), Some((Some(b), c)));

        // Less than twenty seconds after the last real proof, c
        // must not be called into question: the card is still in
        // its outage.
        let still_recovering = start + PROVEN_WITHIN - Duration::from_millis(1);
        assert_eq!(moved(expected.elect(still_recovering)), None);

        // Well past twenty seconds since the last real proof, and
        // since c went into service: another road that answers
        // must be able to try in its place, as for a card that had
        // never proven anything in its life.
        let long_after = settled + PROVEN_WITHIN + Duration::from_secs(1);
        assert_eq!(
            moved(expected.elect(long_after)),
            Some((Some(c), b)),
            "une route restée muette longtemps sur une carte pourtant éprouvée a gardé la main indéfiniment"
        );
    }

    #[test]
    fn a_single_missed_probe_does_not_cost_a_road_its_seat() {
        // A road that has missed a single probe has not stopped
        // answering: that takes three in a row, as everywhere else in
        // this file. Giving way at the first would switch the session
        // over on every packet lost on a link that loses one now and
        // then.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(start);
        assert!(expected.answered(a, first, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));

        let second = expected.number(start);
        assert!(expected.answered(b, second, Duration::from_millis(80), start));
        assert_eq!(moved(expected.elect(start)), None);

        // A first turn sends the probe again; with no echo by the
        // second, for want of a third, it counts as missed.
        expected.look_over(start + KEEP_EVERY);
        let now = start + KEEP_EVERY * 2;
        expected.look_over(now);
        let path = expected
            .paths
            .iter()
            .find(|path| path.through == a)
            .unwrap();
        assert_eq!(path.misses, 1);

        // b, much slower, must not take the place of a over this
        // single missed probe.
        assert_eq!(
            moved(expected.elect(now)),
            None,
            "une seule sonde manquée a coûté sa place à la route la plus rapide"
        );
    }

    #[test]
    fn a_road_that_misses_once_then_answers_signals_a_recovery() {
        // The signal a connection above this card waits for so as not
        // to sit out its own timeouts to the end: the elected road
        // went quiet for the length of a probe, then answered again.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(start);
        assert!(expected.answered(a, first, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));
        assert!(!expected.take_recovery(), "rien à récupérer à l'élection");

        let second = expected.number(start);
        assert!(expected.answered(b, second, Duration::from_millis(80), start));
        assert!(!expected.take_recovery(), "b n'est pas la route élue");

        // A missed probe round on a, as in the test above.
        expected.look_over(start + KEEP_EVERY);
        let now = start + KEEP_EVERY * 2;
        expected.look_over(now);
        assert_eq!(
            expected
                .paths
                .iter()
                .find(|path| path.through == a)
                .unwrap()
                .misses,
            1
        );

        // a answers again: the elected road comes back after a silence.
        let third = expected.number(now);
        assert!(expected.answered(a, third, Duration::from_millis(5), now));
        assert!(
            expected.take_recovery(),
            "le retour de la route élue n'a pas été vu"
        );
        // Taken once, cleared: asking again with nothing new says no.
        assert!(!expected.take_recovery());
    }

    #[test]
    fn a_road_that_never_missed_signals_no_recovery() {
        // The ordinary case, several times a second: nothing to pass on
        // to a connection that never has anything to learn.
        let now = Instant::now();
        let mut expected = expecting(now);
        let a = direct("10.0.0.1:47000");
        let first = expected.number(now);
        assert!(expected.answered(a, first, Duration::from_millis(5), now));
        assert_eq!(moved(expected.elect(now)), Some((None, a)));
        expected.take_recovery();

        let second = expected.number(now);
        assert!(expected.answered(a, second, Duration::from_millis(5), now));
        assert!(
            !expected.take_recovery(),
            "un écho ordinaire a été pris pour un retour"
        );
    }

    #[test]
    fn a_candidate_recovering_signals_nothing_while_it_is_not_elected() {
        // Only the road that really carries the connection matters:
        // another one that falters and comes back in the background
        // tells nobody anything.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(start);
        assert!(expected.answered(a, first, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));
        let second = expected.number(start);
        assert!(expected.answered(b, second, Duration::from_millis(80), start));
        expected.take_recovery();

        // b, not elected, misses a probe round at its own rhythm.
        expected.look_over(start + WARM_EVERY);
        let now = start + WARM_EVERY * 2;
        expected.look_over(now);
        assert_eq!(
            expected
                .paths
                .iter()
                .find(|path| path.through == b)
                .unwrap()
                .misses,
            1
        );

        let third = expected.number(now);
        assert!(expected.answered(b, third, Duration::from_millis(80), now));
        assert!(
            !expected.take_recovery(),
            "le retour d'une route qui ne porte pas la connexion a été signalé"
        );
    }

    #[test]
    fn an_elected_road_given_up_and_taken_up_again_signals_a_recovery() {
        // The case of 7 September
        // (`a_card_that_has_ever_carried_anything_stops_being_second_guessed`
        // above): the elected road is given up for good, then answers at
        // the same address, new to the junction. The silence was longer
        // than a single missed probe round, and the signal must carry
        // just as much.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let b = direct("10.0.0.2:47000");
        let first = expected.number(start);
        assert!(expected.answered(a, first, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, a)));
        let second = expected.number(start);
        assert!(expected.answered(b, second, Duration::from_millis(80), start));
        expected.take_recovery();

        let now = goes_quiet(&mut expected, start);
        assert!(
            expected.paths.iter().all(|path| path.through != a),
            "a aurait dû être abandonnée, b restant pour recevoir"
        );
        assert_eq!(
            expected.elected,
            Some(a),
            "toujours élue jusqu'au prochain elect()"
        );

        let third = expected.number(now);
        assert!(expected.answered(a, third, Duration::from_millis(5), now));
        assert!(
            expected.take_recovery(),
            "le retour d'une route élue abandonnée puis reprise n'a pas été vu"
        );
    }

    /// Silences the elected road until the junction draws the
    /// consequences, and returns what it decided.
    fn goes_quiet(expected: &mut Expected, from: Instant) -> Instant {
        let mut now = from;
        for _ in 0..=MISSES_TO_DIE {
            now += KEEP_EVERY;
            expected.look_over(now);
        }
        now
    }

    #[test]
    fn the_last_road_is_kept_rather_than_leaving_the_session_nowhere_to_send() {
        // On 4 September: the relay branch had been gone for half a
        // minute, only one direct road was left, it went quiet for eight
        // seconds, it was given up, and two seconds later it was
        // answering again. In between, no elected road at all: everything
        // the transport handed over vanished, probes and acknowledgements
        // included, and the far computer died of an absence. A road that
        // has missed three probes is not a road proven dead; sending on
        // it costs nothing, since the other choice sends nowhere.
        let start = Instant::now();
        let mut expected = expecting(start);
        let only = direct("10.0.0.1:47000");
        let number = expected.number(start);
        assert!(expected.answered(only, number, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, only)));

        let now = goes_quiet(&mut expected, start);
        assert_eq!(
            expected.elected,
            Some(only),
            "la session s'est retrouvée sans aucune route où envoyer"
        );
        // And it is still probed, without which its return would go
        // unnoticed.
        assert!(expected.look_over(now + KEEP_EVERY).probe.contains(&only));
    }

    #[test]
    fn a_road_that_answers_takes_the_session_from_one_that_has_stopped() {
        // The counterpart of the previous one: keeping the last road
        // must not let it overshadow a sound road. A road keeps its last
        // good measurement to the very end, so the sickest of the list
        // is often the shortest, and choosing on length alone gives the
        // session to the one that is dying.
        let start = Instant::now();
        let mut expected = expecting(start);
        let dying = direct("10.0.0.1:47000");
        let sound = direct("10.0.0.2:47000");
        let number = expected.number(start);
        assert!(expected.answered(dying, number, Duration::from_millis(5), start));
        assert_eq!(moved(expected.elect(start)), Some((None, dying)));

        // The second road answers, much longer: the margin leaves it
        // where it is as long as the first one is well.
        let number = expected.number(start);
        assert!(expected.answered(sound, number, Duration::from_millis(80), start));
        assert_eq!(moved(expected.elect(start)), None);

        // The first one goes quiet. The session moves to the second, and
        // only then does giving up make sense: one is left.
        let now = goes_quiet(&mut expected, start);
        assert_eq!(moved(expected.elect(now)), Some((Some(dying), sound)));
        assert_eq!(expected.elected, Some(sound));
        assert!(
            !expected.paths.iter().any(|path| path.through == dying),
            "une route abandonnée alors qu'il en restait une autre doit disparaître"
        );
    }

    #[test]
    fn a_direct_road_beats_the_relay_however_long_it_is() {
        // The rule of the product: the relay is only a fallback. A
        // validated direct path takes the session at once, even if it
        // measures ten times the relay, and gives it back to the relay
        // the moment it dies.
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

        // The direct road dies: the session goes back to the relay,
        // which stayed there all along.
        expected.paths.retain(|path| path.through != a);
        assert_eq!(moved(expected.elect(now)), Some((Some(a), relay)));
    }

    #[test]
    fn no_road_answering_leaves_the_session_where_it_is() {
        // On 4 September, a twenty-one minute session over 5G: both
        // computers stop hearing at the same time, the direct road misses
        // a probe, the session moves to the relay, the relay misses one
        // in its turn, and it goes straight back to the direct road it
        // had just left. The back and forth ate four of the ten seconds
        // the engine allowed back then, without either road carrying
        // anything at all.
        let start = Instant::now();
        let mut expected = expecting(start);
        let relay = Through::Relay(expected.card);
        let road = direct("10.0.0.1:47000");

        let number = expected.number(start);
        assert!(expected.answered(relay, number, Duration::from_millis(30), start));
        let number = expected.number(start);
        assert!(expected.answered(road, number, Duration::from_millis(40), start));
        assert_eq!(moved(expected.elect(start)), Some((None, road)));

        // One pass so that both roads are probed, and the relay alone
        // answers.
        let mut now = start + WARM_EVERY;
        expected.look_over(now);
        let number = expected.number(now);
        assert!(expected.answered(relay, number, Duration::from_millis(30), now));

        // The direct road missed its probe, the relay answered: the
        // session moves to the relay, which is indeed the rule.
        now += WARM_EVERY;
        expected.look_over(now);
        assert_eq!(moved(expected.elect(now)), Some((Some(road), relay)));

        // The relay misses its own and the direct road is still quiet.
        // Nothing answers anywhere any more: the session stays where
        // it is.
        now += WARM_EVERY;
        expected.look_over(now);
        assert!(expected.elect(now).is_none());
        assert_eq!(expected.elected, Some(relay));
    }

    #[test]
    fn the_relay_is_never_dropped_to_make_room_for_a_warm_path() {
        // Coming back to it must cost a line in a table, not a
        // connection.
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
        // A session lives for hours; its paths can all die for six
        // seconds, which is what a relay hiccup or a box dropping its
        // translation produces, and it must still be there when they come
        // back. The patience is therefore counted from the last path that
        // answered, and the rhythm of the probes starts again from zero
        // with it.
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        let number = expected.number(start);
        assert!(expected.answered(a, number, Duration::from_millis(5), start));

        // An hour into the session: the path has answered all along,
        // the last time just now, and the candidates that stay quiet
        // are only probed now and then.
        let late = start + Duration::from_secs(3600);
        let number = expected.number(late);
        assert!(expected.answered(a, number, Duration::from_millis(5), late));
        assert_eq!(expected.every(late), LATE_EVERY);

        // Then it dies.
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
        // And two minutes with nothing answering, then yes.
        assert!(
            (late + EXPECTATION_LIFE + Duration::from_secs(1)).duration_since(expected.answered_at)
                > EXPECTATION_LIFE
        );
    }

    #[test]
    fn a_probe_that_never_left_is_not_a_road_that_went_quiet() {
        // On 5 September, PC-SAV wrote twenty-three times that a packet
        // of its own had not gone out, and two seconds after each of
        // those lines, the road "did not answer a probe". The probe with
        // no answer was the probe that never left: the silence was ours
        // and the road paid for it.
        let start = Instant::now();
        let mut expected = expecting(start);
        let only = direct("10.0.0.1:47000");
        let number = expected.number(start);
        assert!(expected.answered(only, number, Duration::from_millis(5), start));

        let mut now = start;
        for _ in 0..=MISSES_TO_DIE {
            now += KEEP_EVERY;
            expected.look_over(now);
            // What the junction does when the socket has refused the
            // probe it had just decided on.
            for path in &mut expected.paths {
                path.asked = false;
            }
        }
        let path = expected
            .paths
            .iter()
            .find(|path| path.through == only)
            .expect("la route a été abandonnée sur nos propres refus");
        assert_eq!(
            path.misses, 0,
            "des sondes jamais parties ont été comptées contre la route"
        );
    }

    #[test]
    fn a_path_that_stops_echoing_is_given_up_after_three_misses() {
        let start = Instant::now();
        let mut expected = expecting(start);
        let a = direct("10.0.0.1:47000");
        // A second road, much longer, just so that giving up is allowed
        // at all: the last one never surrenders, and it is the test
        // above that says so.
        let b = direct("10.0.0.2:47000");
        let number = expected.number(start);
        expected.answered(a, number, Duration::from_millis(5), start);
        let number = expected.number(start);
        expected.answered(b, number, Duration::from_millis(90), start);
        expected.elect(start);
        // A probe every two seconds, three of them without an echo:
        // it is at the fourth that we know the path is dead.
        let mut now = start;
        for probe in 1..=MISSES_TO_DIE + 1 {
            now += KEEP_EVERY;
            let looked = expected.look_over(now);
            if probe <= MISSES_TO_DIE {
                assert!(looked.probe.contains(&a), "relance {probe}");
                assert!(looked.given_up.is_empty(), "abandonné à la relance {probe}");
            } else {
                assert!(
                    !looked.probe.contains(&a),
                    "le chemin mort a encore été sondé"
                );
                assert_eq!(looked.given_up, vec![a], "l'abandon n'est dit nulle part");
            }
        }
        assert!(!expected.paths.iter().any(|path| path.through == a));
        // And the election that follows in the same pass gives the
        // session to the road that is left. Without that, everything the
        // transport hands over afterwards goes down a dead path, without
        // a word and with no way back. It also says what is lost, which
        // an election emptied first could no longer name.
        assert_eq!(moved(expected.elect(now)), Some((Some(a), b)));
    }

    #[tokio::test]
    async fn a_session_that_ends_late_does_not_take_the_card_of_the_one_after_it() {
        // A card stands for one computer, so two sessions in a row
        // towards the same computer share it and the second takes it
        // from the first. The end of the first carried off the card of
        // the second: from then on everything the transport handed over
        // went in the bin without a word, and the far computer died of
        // an absence thirty seconds later.
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
        // An address is named when a session opens, or as the other end
        // finds some: waiting for the next turn of the clock would cost
        // a tenth of a second where it is felt, and would leave the
        // start of every session to the relay.
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
        // After the first seconds, every two seconds.
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
        // A relay carries one packet at a time: what the system would
        // have sent in one go must go out again as that many packets.
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
