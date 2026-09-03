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
use std::time::Duration;

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

use crate::congestion::MediaProfile;
use crate::endpoint::{Bytes, Connection, EndpointError, GUARANTEED_MTU, TunnelEndpoint};
use crate::identity::{Fingerprint, Identity};
use crate::junction::bind_socket;
use crate::probe;
use crate::sifting;

/// How long a device gets to present its pass once it is connected.
pub const PASS_PATIENCE: Duration = Duration::from_secs(3);

/// The longest pass a device may present, and the longest refusal a
/// relay may write back.
const LONGEST_WORD: usize = 4096;

/// The relay's answer: the pass is taken, and packets may flow.
const TAKEN: u8 = 1;

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
            RelayError::Refused(why) => write!(f, "le relais a refusé le laissez-passer : {why}"),
            RelayError::Silent => f.write_str("le relais n'a rien répondu au laissez-passer"),
            RelayError::TooNarrow(room) => write!(
                f,
                "le chemin vers le relais ne porte que {room} octets par paquet, et il en faut \
                 {GUARANTEED_MTU} : ce réseau ne peut pas passer par un relais"
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
    pub async fn open(
        wanted: &Wanted,
        identity: &Identity,
        profile: MediaProfile,
    ) -> Result<Self, RelayError> {
        let endpoint = TunnelEndpoint::towards_the_relay(
            identity,
            wanted.fingerprint,
            profile,
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
            .ok_or_else(|| RelayError::Refused("aucun certificat présenté".to_string()))?;
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
        let runtime = quinn::default_runtime()
            .ok_or_else(|| io::Error::other("aucun exécuteur asynchrone"))?;
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
            TunnelEndpoint::relay_on(&identity, MediaProfile::default(), Arc::new(doorway))
                .unwrap();
        let serving = tokio::spawn(async move {
            let both = Arc::new(tokio::sync::Mutex::new(Vec::<Connection>::new()));
            while let Ok(connection) = endpoint.accept().await {
                let both = both.clone();
                tokio::spawn(async move {
                    let presenting = Presenting::heard(&connection).await.unwrap();
                    if presenting.pass.first() == Some(&0) {
                        presenting
                            .refused("ce n'est pas un laissez-passer")
                            .await
                            .ok();
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

    /// Past this, something that should have happened has not.
    const PATIENCE: Duration = Duration::from_secs(5);

    #[tokio::test(flavor = "multi_thread")]
    async fn two_branches_carry_a_whole_packet_of_the_tunnel_between_them() {
        let relay = Bare::open();
        let here = Identity::generate().unwrap();
        let there = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let first = Branch::open(&relay.wanted(b"laissez-passer"), &here, profile)
            .await
            .unwrap();
        let second = Branch::open(&relay.wanted(b"laissez-passer"), &there, profile)
            .await
            .unwrap();
        assert_eq!(first.address(), relay.address);

        // Un paquet entier du tunnel, la seule taille qui compte : c'est
        // ce qui passe, ou le relais ne sert à rien.
        let packet = vec![7u8; usize::from(GUARANTEED_MTU)];
        assert!(first.send(&packet));
        let arrived = tokio::time::timeout(PATIENCE, second.arrived())
            .await
            .expect("rien n'est arrivé par le relais")
            .unwrap();
        assert_eq!(&arrived[..], &packet[..]);
    }

    #[tokio::test]
    async fn what_the_branch_had_no_room_for_is_counted() {
        // Une route relayée est deux routes à la suite, et la première
        // a sa propre file. Quand elle déborde, le transport y jette le
        // plus ancien sans un mot : ce compteur est le seul endroit d'où
        // une route saturée ici se distingue d'un ordinateur d'en face
        // devenu muet, et les deux tuent la session de la même façon.
        let relay = Bare::open();
        let profile = MediaProfile::default();
        let branch = Branch::open(
            &relay.wanted(b"laissez-passer"),
            &Identity::generate().unwrap(),
            profile,
        )
        .await
        .unwrap();

        // Aucune attente dans la boucle : rien ne part tant qu'elle
        // tourne, donc la file déborde.
        let packet = vec![7u8; usize::from(GUARANTEED_MTU)];
        for _ in 0..(profile.send_queue() / packet.len() * 4) {
            branch.send(&packet);
        }

        let carried = branch.carried();
        assert!(carried.sent > 0, "rien n'a été confié à la branche");
        assert!(
            carried.crowded > 0,
            "{} paquets confiés et aucun manque de place compté",
            carried.sent
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pass_the_relay_refuses_says_why_rather_than_hanging() {
        let relay = Bare::open();
        let device = Identity::generate().unwrap();
        let refused = Branch::open(&relay.wanted(&[0]), &device, MediaProfile::default())
            .await
            .unwrap_err();
        assert!(
            matches!(&refused, RelayError::Refused(why) if why.contains("laissez-passer")),
            "{refused:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_relay_presenting_another_certificate_is_not_joined() {
        // C'est ce qui empêche de détourner une session vers un relais
        // qui n'est pas celui que le serveur a nommé.
        let relay = Bare::open();
        let device = Identity::generate().unwrap();
        let mut wanted = relay.wanted(b"laissez-passer");
        wanted.fingerprint = Identity::generate().unwrap().fingerprint();
        let refused = Branch::open(&wanted, &device, MediaProfile::default())
            .await
            .unwrap_err();
        assert!(matches!(refused, RelayError::Endpoint(_)), "{refused:?}");
    }

    #[tokio::test]
    async fn the_doorway_answers_the_mirror_on_the_relays_own_port() {
        let doorway = Doorway::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = doorway.local_address().unwrap();
        let identity = Identity::generate().unwrap();
        // Sans point d'accès dessus, personne ne lit la prise : c'est le
        // relais qui la fait tourner, miroir compris.
        let _relay =
            TunnelEndpoint::relay_on(&identity, MediaProfile::default(), Arc::new(doorway))
                .unwrap();

        let asking = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let nonce = [4, 3, 2, 1, 4, 3, 2, 1];
        asking
            .send_to(&probe::who_am_i(nonce), address)
            .await
            .unwrap();
        let mut buf = [0u8; 1500];
        let (count, from) = tokio::time::timeout(PATIENCE, asking.recv_from(&mut buf))
            .await
            .expect("le miroir n'a pas répondu")
            .unwrap();
        assert_eq!(from, address);
        let Some(probe::Heard::SeenAs {
            nonce: answered,
            seen,
        }) = probe::heard(&buf[..count])
        else {
            panic!("pas une réponse de miroir");
        };
        assert_eq!(answered, nonce);
        assert_eq!(seen, asking.local_addr().unwrap());
    }
}
