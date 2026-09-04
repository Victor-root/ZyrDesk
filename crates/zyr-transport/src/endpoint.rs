//! Setting up the encrypted connection between two devices.
//!
//! Where the shape of a connection is decided: what each end presents,
//! whom it accepts, and what the path under it is allowed to do. Every
//! other module of the product knows nothing but `Connection` and the
//! two stream types below.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{AsyncUdpSocket, ClientConfig, Endpoint, ServerConfig, TransportConfig};
use rustls::pki_types::CertificateDer;

use crate::congestion::{FASTEST, Media};
use crate::identity::{AllowedPeers, AnyPeer, Fingerprint, Identity, PinnedPeer};
use crate::junction::Junction;
use crate::path::{DegradedPath, Path};

/// Payload carried around, without a copy when it changes hands.
pub use bytes::Bytes;

/// Sending half of a reliable stream.
pub type SendStream = quinn::SendStream;
/// Receiving half of a reliable stream.
pub type RecvStream = quinn::RecvStream;

/// Protocol announced during the handshake, so we answer nothing else.
const PROTOCOL: &[u8] = b"zyrdesk/1";

/// The same, for the connection a device holds towards a relay: another
/// conversation entirely, and never to be mistaken for a tunnel.
const RELAY_PROTOCOL: &[u8] = b"zyrdesk-relay/1";

/// Past this, the session counts as lost.
const MAXIMUM_IDLE: Duration = Duration::from_secs(30);

/// Keeps the mapping alive in the network equipment along the way.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// Receive queue, sized to absorb a burst of frames.
const RECEIVE_QUEUE: usize = 8 * 1024 * 1024;

/// Smallest packet QUIC requires every path to carry.
///
/// This is not a choice of ours. A connection whose path cannot carry
/// this cannot exist at all, and it is exactly what the transport falls
/// back to the moment it decides the path has stopped carrying anything
/// bigger. Whatever else happens to a path while a session runs, this
/// much of it holds.
pub const GUARANTEED_MTU: u16 = 1200;

/// Smallest packet the branch towards a relay is built on.
///
/// A relayed packet is a whole packet of the tunnel, `GUARANTEED_MTU`
/// bytes, inside a datagram of the outer connection: that outer path has
/// to carry those bytes plus its own envelope, some forty of them. The
/// floor of IPv6, which is what every ordinary network carries, leaves
/// exactly the room for it. A path that cannot even do that makes the
/// relay useless, and the branch says so rather than opening onto
/// packets that would all be refused.
const RELAY_MTU: u16 = 1280;

#[derive(Debug)]
pub enum EndpointError {
    Configuration(String),
    Network(std::io::Error),
    Connection(String),
    /// The far end hung up, which is how a session ends when somebody
    /// closes it. Told apart from the rest because it is not a fault:
    /// read as one, every ordinary end of every session was written down
    /// as « connexion impossible », and a journal that calls the normal
    /// case a failure is a journal nobody can read a real failure out of.
    Ended,
    Closed,
}

impl std::fmt::Display for EndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointError::Configuration(e) => write!(f, "configuration du transport : {e}"),
            EndpointError::Network(e) => write!(f, "erreur réseau : {e}"),
            EndpointError::Connection(e) => write!(f, "connexion impossible : {e}"),
            EndpointError::Ended => write!(f, "l'ordinateur d'en face a raccroché"),
            EndpointError::Closed => write!(f, "le point de connexion est fermé"),
        }
    }
}

/// What the end of a live connection means.
///
/// A peer that closes with no code and nothing to say is a session being
/// closed, and nothing else here can tell that from a session that
/// broke: both arrive as the same kind of error, at the same three
/// places, once the connection has been standing.
fn how_it_ended(e: quinn::ConnectionError) -> EndpointError {
    match &e {
        quinn::ConnectionError::ApplicationClosed(close)
            if close.error_code == quinn::VarInt::from_u32(0) && close.reason.is_empty() =>
        {
            EndpointError::Ended
        }
        _ => EndpointError::Connection(e.to_string()),
    }
}

impl std::error::Error for EndpointError {}

/// What the path under a connection is carrying right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carrying {
    /// Payload one datagram can carry at this instant.
    pub usable_datagram: u16,
    /// Times the transport decided the path had stopped carrying the
    /// packet size it had settled on, and fell back to the floor.
    ///
    /// Anything but zero on a session whose picture froze is the answer:
    /// the engine was told a size the path then stopped accepting, and
    /// it cannot be told another one until the next session.
    pub narrowings: u64,
    /// Datagrams that actually went out.
    pub sent: u64,
    /// Packets the transport itself saw lost on the path.
    pub lost: u64,
    /// What may be out on the wire unanswered at once. Nothing goes out
    /// beyond it, so a session losing packets with a full window is a
    /// session whose far end has stopped answering, and one losing them
    /// with room to spare is a path that really cannot take them.
    pub window: u64,
    pub round_trip: Duration,
}

/// A datagram could not be handed over.
#[derive(Debug)]
pub enum DatagramError {
    /// Bigger than the path accepts. To be dropped: fragmenting it would
    /// cost more than losing it.
    TooLarge,
    /// The connection is gone.
    Lost(String),
}

impl std::fmt::Display for DatagramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatagramError::TooLarge => write!(f, "datagramme trop gros pour le chemin"),
            DatagramError::Lost(e) => write!(f, "connexion perdue : {e}"),
        }
    }
}

impl std::error::Error for DatagramError {}

impl From<std::io::Error> for EndpointError {
    fn from(e: std::io::Error) -> Self {
        EndpointError::Network(e)
    }
}

/// Settings shared by both ends.
///
/// The window follows the session, since the controller works it out
/// afresh at every ask; the queue cannot, being settled here once and
/// for all, so it is sized on the fastest stream there is. `FASTEST`
/// says why.
fn transport(media: Media) -> TransportConfig {
    let mut config = TransportConfig::default();
    config.datagram_send_buffer_size(FASTEST.send_queue());
    config.congestion_controller_factory(Arc::new(media));
    config.datagram_receive_buffer_size(Some(RECEIVE_QUEUE));
    config.max_idle_timeout(Some(
        MAXIMUM_IDLE.try_into().expect("idle timeout representable"),
    ));
    config.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
    config
}

/// The same, on a path of its own: packet discovery finds what it
/// carries, which is the ordinary case of a tunnel opened at an address.
fn transport_discovering(media: Media) -> Arc<TransportConfig> {
    Arc::new(transport(media))
}

/// The same, on a junction: one packet size, and it never moves.
///
/// A junction changes the road under a connection without the connection
/// knowing, and two roads do not carry the same packet. Discovery would
/// find what the road of the moment carries, and the next road would
/// then be handed packets it cannot take: the transport would see them
/// vanish, decide the path had gone black, and fall back, all of it
/// invisibly and in the middle of a session. So every packet is the
/// smallest QUIC requires of any path at all, which is what every road
/// carries by definition, the relay's included.
///
/// It costs a few dozen bytes a packet on the reliable streams, which
/// carry nothing large. The video was already sized on this floor
/// (`guaranteed_usable_datagram`) and loses nothing.
fn transport_on_a_junction(media: Media) -> Arc<TransportConfig> {
    let mut config = transport(media);
    config.mtu_discovery_config(None);
    config.initial_mtu(GUARANTEED_MTU);
    Arc::new(config)
}

/// The same, towards a relay: a floor high enough for a whole packet of
/// the tunnel, and discovery above it.
fn transport_towards_a_relay(media: Media) -> Arc<TransportConfig> {
    let mut config = transport(media);
    config.min_mtu(RELAY_MTU);
    config.initial_mtu(RELAY_MTU);
    Arc::new(config)
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// What the end that waits presents, and whom it lets in.
fn server_config(
    identity: &Identity,
    verifier: Arc<dyn rustls::server::danger::ClientCertVerifier>,
    alpn: &[u8],
    transport: Arc<TransportConfig>,
) -> Result<ServerConfig, EndpointError> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| EndpointError::Configuration(e.to_string()))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![identity.certificate().clone()], identity.key())
        .map_err(|e| EndpointError::Configuration(e.to_string()))?;
    tls.alpn_protocols = vec![alpn.to_vec()];

    let quic =
        QuicServerConfig::try_from(tls).map_err(|e| EndpointError::Configuration(e.to_string()))?;
    let mut config = ServerConfig::with_crypto(Arc::new(quic));
    config.transport_config(transport);
    Ok(config)
}

/// What the end that goes presents, and whom it expects.
fn client_config(
    identity: &Identity,
    peer: Fingerprint,
    alpn: &[u8],
    transport: Arc<TransportConfig>,
) -> Result<ClientConfig, EndpointError> {
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| EndpointError::Configuration(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedPeer::new(peer)))
        .with_client_auth_cert(vec![identity.certificate().clone()], identity.key())
        .map_err(|e| EndpointError::Configuration(e.to_string()))?;
    tls.alpn_protocols = vec![alpn.to_vec()];

    let quic =
        QuicClientConfig::try_from(tls).map_err(|e| EndpointError::Configuration(e.to_string()))?;
    let mut config = ClientConfig::new(Arc::new(quic));
    config.transport_config(transport);
    Ok(config)
}

/// What a tunnel's waiting end presents, and the fingerprints it lets
/// in.
fn pinned_host(
    identity: &Identity,
    allowed: impl Into<AllowedPeers>,
    transport: Arc<TransportConfig>,
) -> Result<ServerConfig, EndpointError> {
    server_config(
        identity,
        Arc::new(PinnedPeer::new(allowed)),
        PROTOCOL,
        transport,
    )
}

/// Opens the endpoint on the requested path.
fn open(
    listen: SocketAddr,
    server: Option<ServerConfig>,
    path: Path,
) -> Result<Endpoint, EndpointError> {
    let Path::Degraded { loss_per_thousand } = path else {
        return Ok(match server {
            Some(config) => Endpoint::server(config, listen)?,
            None => Endpoint::client(listen)?,
        });
    };

    let runtime = quinn::default_runtime()
        .ok_or_else(|| EndpointError::Configuration("no async runtime".to_string()))?;
    let socket = runtime.wrap_udp_socket(std::net::UdpSocket::bind(listen)?)?;
    let degraded = Arc::new(DegradedPath::new(socket, loss_per_thousand));

    Ok(Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        server,
        degraded,
        runtime,
    )?)
}

/// Opens the endpoint on a socket somebody else holds: a junction, or
/// the doorway a server puts its relay on.
fn open_on(
    socket: Arc<dyn AsyncUdpSocket>,
    server: Option<ServerConfig>,
) -> Result<Endpoint, EndpointError> {
    let runtime = quinn::default_runtime()
        .ok_or_else(|| EndpointError::Configuration("no async runtime".to_string()))?;
    Ok(Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        server,
        socket,
        runtime,
    )?)
}

/// One end of the tunnel.
///
/// Cloneable, and cheaply: the thing underneath is a handle several
/// owners are meant to hold. What wants that is opening towards a
/// computer's several addresses at once, each attempt on its own task.
#[derive(Clone)]
pub struct TunnelEndpoint {
    endpoint: Endpoint,
}

impl TunnelEndpoint {
    /// The end that waits for the other device to connect.
    pub fn host(
        identity: &Identity,
        allowed: impl Into<AllowedPeers>,
        media: impl Into<Media>,
        listen: SocketAddr,
    ) -> Result<Self, EndpointError> {
        Self::host_on_path(identity, allowed, media, listen, Path::Direct)
    }

    /// The same, on a path whose quality is imposed.
    ///
    /// Reserved for the measurement bench: this is how congestion
    /// control is tested without a degraded network at hand.
    pub fn host_on_path(
        identity: &Identity,
        allowed: impl Into<AllowedPeers>,
        media: impl Into<Media>,
        listen: SocketAddr,
        path: Path,
    ) -> Result<Self, EndpointError> {
        let config = pinned_host(identity, allowed, transport_discovering(media.into()))?;
        Ok(Self {
            endpoint: open(listen, Some(config), path)?,
        })
    }

    /// The end that waits, on a junction: reached through a card by the
    /// computers the junction expects, and at its real address by the
    /// others.
    pub fn host_at(
        identity: &Identity,
        allowed: impl Into<AllowedPeers>,
        media: impl Into<Media>,
        junction: &Junction,
    ) -> Result<Self, EndpointError> {
        let config = pinned_host(identity, allowed, transport_on_a_junction(media.into()))?;
        Ok(Self {
            endpoint: open_on(Arc::new(junction.clone()), Some(config))?,
        })
    }

    /// The end a relay waits on: every device is let in, and what
    /// decides is the pass each of them presents afterwards.
    ///
    /// The socket is the doorway the mirror answers on, so one UDP port
    /// serves both.
    pub fn relay_on(
        identity: &Identity,
        media: impl Into<Media>,
        socket: Arc<dyn AsyncUdpSocket>,
    ) -> Result<Self, EndpointError> {
        let config = server_config(
            identity,
            Arc::new(AnyPeer::default()),
            RELAY_PROTOCOL,
            transport_towards_a_relay(media.into()),
        )?;
        Ok(Self {
            endpoint: open_on(socket, Some(config))?,
        })
    }

    /// The end that goes towards the other device.
    pub fn client(
        identity: &Identity,
        peer: Fingerprint,
        media: impl Into<Media>,
        listen: SocketAddr,
    ) -> Result<Self, EndpointError> {
        Self::client_on_path(identity, peer, media, listen, Path::Direct)
    }

    /// The same, on a path whose quality is imposed.
    pub fn client_on_path(
        identity: &Identity,
        peer: Fingerprint,
        media: impl Into<Media>,
        listen: SocketAddr,
        path: Path,
    ) -> Result<Self, EndpointError> {
        let config = client_config(
            identity,
            peer,
            PROTOCOL,
            transport_discovering(media.into()),
        )?;
        let mut endpoint = open(listen, None, path)?;
        endpoint.set_default_client_config(config);
        Ok(Self { endpoint })
    }

    /// The end that goes, on a junction: what it connects to is the
    /// card of the computer the junction expects.
    pub fn client_at(
        identity: &Identity,
        peer: Fingerprint,
        media: impl Into<Media>,
        junction: &Junction,
    ) -> Result<Self, EndpointError> {
        let config = client_config(
            identity,
            peer,
            PROTOCOL,
            transport_on_a_junction(media.into()),
        )?;
        let mut endpoint = open_on(Arc::new(junction.clone()), None)?;
        endpoint.set_default_client_config(config);
        Ok(Self { endpoint })
    }

    /// The end that goes towards a relay.
    ///
    /// The same machinery as a tunnel, under a protocol name of its own
    /// and on a packet floor of its own: what travels here is a whole
    /// packet of a tunnel, and it has to fit in one piece.
    pub fn towards_the_relay(
        identity: &Identity,
        relay: Fingerprint,
        media: impl Into<Media>,
        listen: SocketAddr,
    ) -> Result<Self, EndpointError> {
        let config = client_config(
            identity,
            relay,
            RELAY_PROTOCOL,
            transport_towards_a_relay(media.into()),
        )?;
        let mut endpoint = open(listen, None, Path::Direct)?;
        endpoint.set_default_client_config(config);
        Ok(Self { endpoint })
    }

    pub fn local_address(&self) -> Result<SocketAddr, EndpointError> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Goes to the remote device.
    ///
    /// The name we ask for is not checked: the fingerprint is what
    /// counts.
    ///
    /// The return only announces half of the authorisation, the host's
    /// by the client. The client's certificate leaves last and is only
    /// judged afterwards; a refused client therefore sees its connection
    /// succeed and then break straight away. A session must never be
    /// announced as established before the first successful exchange.
    pub async fn connect(&self, remote: SocketAddr) -> Result<Connection, EndpointError> {
        let connection = self
            .endpoint
            .connect(remote, "zyrdesk")
            .map_err(|e| EndpointError::Connection(e.to_string()))?
            .await
            .map_err(|e| EndpointError::Connection(e.to_string()))?;
        Ok(Connection::new(connection))
    }

    /// Waits for the remote device to connect, handshake included.
    pub async fn accept(&self) -> Result<Connection, EndpointError> {
        self.accept_knock(|_| true).await?.taken().await
    }

    /// Waits for the next knock, turning away whoever the caller will
    /// not have, and hands it over before the handshake.
    ///
    /// Two reasons, and both belong to a door anybody on the Internet
    /// may knock on. Refusing on the address alone costs nothing, where
    /// refusing after the handshake would already have cost a signature
    /// per knock. And handing the knock over unfinished lets the caller
    /// take the next one at once: a knock that goes quiet halfway
    /// through would otherwise hold up every other for as long as a
    /// connection takes to give up.
    pub async fn accept_knock(
        &self,
        allowed: impl Fn(SocketAddr) -> bool,
    ) -> Result<Knocking, EndpointError> {
        loop {
            let incoming = self.endpoint.accept().await.ok_or(EndpointError::Closed)?;
            if !allowed(incoming.remote_address()) {
                incoming.refuse();
                continue;
            }
            return Ok(Knocking(incoming));
        }
    }

    /// Waits for the connections in progress to end.
    pub async fn close(&self) {
        self.endpoint.close(0u32.into(), b"end of session");
        self.endpoint.wait_idle().await;
    }
}

/// A device that has knocked and is still shaking hands.
pub struct Knocking(quinn::Incoming);

impl Knocking {
    /// Where it knocked from, known before anything is agreed.
    pub fn from(&self) -> SocketAddr {
        self.0.remote_address()
    }

    /// Waits for the handshake to finish.
    ///
    /// A refusal names where it came from: nothing of it survives the
    /// failure, and without the address « somebody was turned away »
    /// reads exactly like « nobody came ».
    pub async fn taken(self) -> Result<Connection, EndpointError> {
        let from = self.from();
        let connection = self
            .0
            .await
            .map_err(|e| EndpointError::Connection(format!("{from} : {e}")))?;
        Ok(Connection::new(connection))
    }
}

/// Connection established with the remote device.
#[derive(Clone)]
pub struct Connection {
    inner: quinn::Connection,
}

impl Connection {
    fn new(inner: quinn::Connection) -> Self {
        Self { inner }
    }

    /// Where the other end of this connection is: a place on a network,
    /// or the card of a computer reached through a junction.
    ///
    /// Written the plain way, whatever the socket speaks: on one that
    /// speaks IPv6, the transport writes an IPv4 address in its mapped
    /// form, which nobody else does.
    pub fn remote_address(&self) -> SocketAddr {
        let remote = self.inner.remote_address();
        SocketAddr::new(remote.ip().to_canonical(), remote.port())
    }

    /// The fingerprint of the certificate the other end presented.
    ///
    /// Known to both ends of a tunnel in advance, and to nobody in
    /// advance at a relay, which is the one place this is read: a device
    /// arrives unannounced, and its certificate is what says whose pass
    /// it may present. TLS has already proven it holds that key.
    pub fn peer_fingerprint(&self) -> Option<Fingerprint> {
        let presented = self.inner.peer_identity()?;
        let chain = presented.downcast::<Vec<CertificateDer<'static>>>().ok()?;
        Some(Fingerprint::of_certificate(chain.first()?))
    }

    /// Payload one datagram can carry on the current path.
    ///
    /// It moves with path discovery: this is what decides the packet
    /// size we ask the engine for.
    pub fn usable_datagram(&self) -> Option<u16> {
        self.inner
            .max_datagram_size()
            .map(|size| u16::try_from(size).unwrap_or(u16::MAX))
    }

    /// Payload one datagram is certain to carry for the whole life of
    /// this connection.
    ///
    /// Not what the path offers at this instant: what it can never stop
    /// offering. The two are very different, and the difference is a
    /// session that freezes.
    ///
    /// The transport probes upwards for a bigger packet, and when those
    /// bigger packets start disappearing it decides the path has stopped
    /// carrying them and drops straight back to the smallest packet QUIC
    /// requires of any path at all. That happens a second or two into a
    /// session, on exactly the paths where it matters: a private tunnel
    /// carried inside another one, where the first probe gets through
    /// and nothing does once real video is flowing.
    ///
    /// The engine cannot follow. It is told a packet size once, as it
    /// starts, and keeps it for the whole session. Sized on the
    /// measurement of the moment, every video packet became too large to
    /// send the instant the transport dropped back, and every one of
    /// them was thrown away. The connection went on perfectly, the
    /// control channel with it, and the picture simply stopped: the
    /// worst shape a failure can take, because nothing anywhere says it
    /// happened.
    ///
    /// So the size is taken from the floor rather than the ceiling. What
    /// it costs is a few more packets for the same picture on a path
    /// that would have carried larger ones. What it buys is a session
    /// that cannot be killed by the path narrowing under it.
    pub fn guaranteed_usable_datagram(&self) -> Option<u16> {
        let usable = self.usable_datagram()?;
        // What the transport spends on its own headers. Measured rather
        // than worked out: it depends on the length of the connection
        // identifiers and on what the far end announced, and it does not
        // change with the size of the packet carrying it.
        let overhead = self.inner.stats().path.current_mtu.checked_sub(usable)?;
        GUARANTEED_MTU.checked_sub(overhead)
    }

    pub fn round_trip(&self) -> Duration {
        self.inner.rtt()
    }

    /// What the path is doing, read in one go.
    ///
    /// Together and not one call at a time, because these numbers are
    /// only worth anything beside each other: room left with nothing
    /// dropped is a healthy path, the same room with packets dropped is
    /// a session whose picture has stopped, and neither reads as
    /// anything on its own.
    pub fn carrying(&self) -> Carrying {
        let stats = self.inner.stats();
        Carrying {
            usable_datagram: self.usable_datagram().unwrap_or(0),
            narrowings: stats.path.black_holes_detected,
            sent: stats.frame_tx.datagram,
            lost: stats.path.lost_packets,
            window: stats.path.cwnd,
            round_trip: stats.path.rtt,
        }
    }

    /// Datagrams that actually went out on the network.
    ///
    /// To compare against the number of datagrams handed over: the
    /// difference is what the transport dropped from its send queue for
    /// want of room. It does so silently, sacrificing the oldest, which
    /// is the right call for video but is still a loss that has to be
    /// told apart from the network's.
    pub fn datagrams_sent(&self) -> u64 {
        self.inner.stats().frame_tx.datagram
    }

    /// Packets the transport itself observed as lost on the path.
    pub fn packets_lost(&self) -> u64 {
        self.inner.stats().path.lost_packets
    }

    /// Opens a reliable stream towards the peer.
    pub async fn open_stream(&self) -> Result<(SendStream, RecvStream), EndpointError> {
        self.inner.open_bi().await.map_err(how_it_ended)
    }

    /// Waits for a reliable stream the peer opens.
    pub async fn accept_stream(&self) -> Result<(SendStream, RecvStream), EndpointError> {
        self.inner.accept_bi().await.map_err(how_it_ended)
    }

    /// Room left in the queue of datagrams waiting to go out.
    ///
    /// Handing over more than this makes the transport throw the oldest
    /// away, which is the right call and still a loss: it is the only
    /// place where a packet the engine produced disappears without
    /// anything anywhere saying so.
    pub fn send_queue_room(&self) -> usize {
        self.inner.datagram_send_buffer_space()
    }

    /// Hands over a datagram, with no promise of delivery or order.
    ///
    /// When the send queue is full, the transport makes room by dropping
    /// the oldest: a stale frame is worth nothing.
    pub fn send_datagram(&self, payload: Bytes) -> Result<(), DatagramError> {
        self.inner.send_datagram(payload).map_err(|e| match e {
            quinn::SendDatagramError::TooLarge => DatagramError::TooLarge,
            other => DatagramError::Lost(other.to_string()),
        })
    }

    /// Waits for a datagram from the peer.
    pub async fn read_datagram(&self) -> Result<Bytes, EndpointError> {
        self.inner.read_datagram().await.map_err(how_it_ended)
    }

    /// Waits for the connection to break.
    pub async fn closed(&self) {
        self.inner.closed().await;
    }

    /// Shows the far end out, saying nothing.
    ///
    /// The ordinary end of a session, and the one the far end reads back
    /// as [`EndpointError::Ended`] rather than as a fault.
    pub fn close(&self) {
        self.inner.close(0u32.into(), b"");
    }

    /// A number that tells this connection from another, for as long as
    /// it lives: two connections of one device, one replacing the other,
    /// are otherwise indistinguishable.
    pub fn stable_id(&self) -> usize {
        self.inner.stable_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::congestion::MediaProfile;

    /// Past this, a connection that should have broken has not.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn local() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    /// Both ends and their connection, kept alive together.
    struct Pair {
        _host: TunnelEndpoint,
        _client: TunnelEndpoint,
        host_side: Connection,
        client_side: Connection,
    }

    /// Brings both ends up and puts them in touch.
    async fn pair() -> Pair {
        let host_identity = Identity::generate().unwrap();
        let client_identity = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let host = TunnelEndpoint::host(
            &host_identity,
            client_identity.fingerprint(),
            profile,
            local(),
        )
        .unwrap();
        let address = host.local_address().unwrap();
        let client = TunnelEndpoint::client(
            &client_identity,
            host_identity.fingerprint(),
            profile,
            local(),
        )
        .unwrap();

        let (host_side, client_side) = tokio::join!(host.accept(), client.connect(address));
        Pair {
            _host: host,
            _client: client,
            host_side: host_side.unwrap(),
            client_side: client_side.unwrap(),
        }
    }

    impl Pair {
        /// Sends a word on a reliable stream and reads it at the far end.
        ///
        /// What tells a connection still carries: a stream is answered
        /// for and sent again until it arrives, where a datagram is
        /// allowed to vanish on the way.
        async fn word_across(&self, word: &'static [u8]) -> Vec<u8> {
            let receiver = self.host_side.clone();
            let waiting = tokio::spawn(async move {
                let (_, mut reading) = receiver.accept_stream().await.unwrap();
                reading.read_to_end(64).await.unwrap()
            });

            let (mut writing, _) = self.client_side.open_stream().await.unwrap();
            writing.write_all(word).await.unwrap();
            writing.finish().unwrap();

            waiting.await.unwrap()
        }
    }

    #[tokio::test]
    async fn two_devices_that_know_each_other_connect() {
        let pair = pair().await;
        assert!(pair.client_side.usable_datagram().is_some());
        assert!(pair.host_side.usable_datagram().is_some());
    }

    #[tokio::test]
    async fn the_usable_datagram_allows_a_video_packet() {
        let pair = pair().await;
        let usable = pair.client_side.usable_datagram().unwrap();
        let size = crate::mtu::packet_size(usable).expect("a local path must allow a video packet");
        assert!(size.bytes >= crate::mtu::MINIMUM_SIZE);
    }

    #[tokio::test]
    async fn the_room_promised_is_never_more_than_the_room_of_the_moment() {
        // La taille de paquet demandée au moteur vaut pour toute la
        // session, et le chemin, lui, peut se rétrécir en cours de
        // route : le transport retombe alors au plancher garanti. Une
        // taille prise sur la mesure du moment ne passait plus du tout,
        // et l'image se figeait sans que rien ne le dise.
        let pair = pair().await;
        let now = pair.client_side.usable_datagram().unwrap();
        let promised = pair.client_side.guaranteed_usable_datagram().unwrap();
        assert!(promised <= now, "{promised} promis contre {now} mesurés");
        // Et il en reste assez pour un vrai paquet vidéo, sans quoi la
        // prudence ne servirait qu'à refuser les sessions.
        let size = crate::mtu::packet_size(promised).expect("le plancher doit rester utilisable");
        assert!(
            size.bytes >= crate::mtu::MINIMUM_SIZE,
            "{} octets de paquet pour un plancher de {} (promis {promised}, mesuré {now})",
            size.bytes,
            crate::mtu::MINIMUM_SIZE
        );
    }

    #[tokio::test]
    async fn a_datagram_crosses_the_tunnel() {
        let pair = pair().await;
        pair.client_side
            .send_datagram(Bytes::from_static(b"frame"))
            .unwrap();
        let received = pair.host_side.read_datagram().await.unwrap();
        assert_eq!(&received[..], b"frame");
    }

    #[tokio::test]
    async fn a_session_the_far_end_closes_reads_as_an_end_and_not_as_a_fault() {
        // Ce que voit l'ordinateur regardé quand la personne ferme sa
        // session : l'autre bout raccroche, sans code et sans un mot. Lu
        // comme une panne, chaque fin de session ordinaire s'écrivait
        // « connexion impossible » dans son journal, et une vraie panne
        // ne s'y distinguait plus de rien.
        let pair = pair().await;
        drop(pair.client_side);
        let ended = tokio::time::timeout(PATIENCE, pair.host_side.read_datagram()).await;
        assert!(matches!(ended, Ok(Err(EndpointError::Ended))), "{ended:?}");
    }

    #[tokio::test]
    async fn a_datagram_bigger_than_the_path_is_refused() {
        let pair = pair().await;
        let huge = Bytes::from(vec![0u8; 64 * 1024]);
        assert!(matches!(
            pair.client_side.send_datagram(huge),
            Err(DatagramError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn a_burst_bigger_than_the_send_queue_leaves_the_connection_alive() {
        // Une image clé part d'un bloc, et la pompe la pousse plus vite
        // que le transport ne la met sur le fil : la file d'envoi déborde
        // à chaque image clé, sur le meilleur des réseaux. Ce qui déborde
        // se jette, c'est voulu. Ce qui ne doit jamais arriver est que la
        // connexion elle-même y passe, et c'est ce que fait un transport
        // dont la comptabilité de file déraille : la session meurt sans
        // qu'une seule ligne de journal dise pourquoi.
        let pair = pair().await;
        let room = pair.client_side.guaranteed_usable_datagram().unwrap() as usize;
        let packet = Bytes::from(vec![0u8; room]);

        // Aucune attente dans la boucle : rien ne part tant qu'elle
        // tourne, donc la file déborde plusieurs fois.
        for _ in 0..(FASTEST.send_queue() / room * 8) {
            pair.client_side.send_datagram(packet.clone()).unwrap();
        }

        // Ce qui doit survivre est la connexion, pas les paquets. Un
        // datagramme envoyé après une rafale faite pour tout faire
        // déborder a toutes les raisons de se perdre, et le demander
        // serait demander au transport une promesse qu'il ne fait pas :
        // c'est un flux fiable qui dit si la connexion est encore là.
        let said = tokio::time::timeout(PATIENCE, pair.word_across(b"apres")).await;
        assert_eq!(
            said.expect("la connexion n'a pas survécu à la rafale"),
            b"apres"
        );
    }

    #[tokio::test]
    async fn a_reliable_stream_crosses_the_tunnel() {
        let pair = pair().await;
        assert_eq!(pair.word_across(b"negotiation").await, b"negotiation");
    }

    #[tokio::test]
    async fn an_unknown_device_is_refused() {
        let host_identity = Identity::generate().unwrap();
        let expected_identity = Identity::generate().unwrap();
        let intruder_identity = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        // The host expects one device and one only.
        let host = TunnelEndpoint::host(
            &host_identity,
            expected_identity.fingerprint(),
            profile,
            local(),
        )
        .unwrap();
        let address = host.local_address().unwrap();
        let intruder = TunnelEndpoint::client(
            &intruder_identity,
            host_identity.fingerprint(),
            profile,
            local(),
        )
        .unwrap();

        let (host_side, attempt) = tokio::join!(host.accept(), intruder.connect(address));
        assert!(host_side.is_err(), "the host accepted an unknown device");

        // The intruder may briefly have believed its connection was up:
        // it presents its certificate last and only learns of the refusal
        // when the connection breaks. Nothing will have travelled.
        if let Ok(connection) = attempt {
            let broken = tokio::time::timeout(PATIENCE, connection.closed()).await;
            assert!(broken.is_ok(), "the intruder kept its connection");
        }
    }

    #[tokio::test]
    async fn an_impersonated_host_is_refused() {
        let host_identity = Identity::generate().unwrap();
        let client_identity = Identity::generate().unwrap();
        let other_identity = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let host = TunnelEndpoint::host(
            &host_identity,
            client_identity.fingerprint(),
            profile,
            local(),
        )
        .unwrap();
        let address = host.local_address().unwrap();
        // The client expects a fingerprint the host cannot present.
        let client = TunnelEndpoint::client(
            &client_identity,
            other_identity.fingerprint(),
            profile,
            local(),
        )
        .unwrap();

        let (_, attempt) = tokio::join!(host.accept(), client.connect(address));
        assert!(attempt.is_err(), "the client accepted an impersonated host");
    }
}
