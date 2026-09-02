//! Setting up the encrypted connection between two devices.
//!
//! This module is the only place in the product that names the transport
//! library. Everything else knows nothing but `Connection` and the two
//! stream types below. Changing transport, should the relay work
//! justify it, therefore touches this file and nothing more.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig};

use crate::congestion::MediaProfile;
use crate::identity::{AllowedPeers, Fingerprint, Identity, PinnedPeer};
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

/// Past this, the session counts as lost.
const MAXIMUM_IDLE: Duration = Duration::from_secs(30);

/// Keeps the mapping alive in the network equipment along the way.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// Deliberately short send queue.
///
/// Under congestion, dropping a stale frame beats keeping it: it would
/// arrive too late to be shown, and everything behind it would have
/// fallen behind. The video protocol's error correction exists to fill
/// those holes.
const SEND_QUEUE: usize = 128 * 1024;

/// Receive queue, sized to absorb a burst of frames.
const RECEIVE_QUEUE: usize = 8 * 1024 * 1024;

/// Smallest packet QUIC requires every path to carry.
///
/// This is not a choice of ours. A connection whose path cannot carry
/// this cannot exist at all, and it is exactly what the transport falls
/// back to the moment it decides the path has stopped carrying anything
/// bigger. Whatever else happens to a path while a session runs, this
/// much of it holds.
const GUARANTEED_MTU: u16 = 1200;

#[derive(Debug)]
pub enum EndpointError {
    Configuration(String),
    Network(std::io::Error),
    Connection(String),
    Closed,
}

impl std::fmt::Display for EndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointError::Configuration(e) => write!(f, "configuration du transport : {e}"),
            EndpointError::Network(e) => write!(f, "erreur réseau : {e}"),
            EndpointError::Connection(e) => write!(f, "connexion impossible : {e}"),
            EndpointError::Closed => write!(f, "le point de connexion est fermé"),
        }
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
/// Through a junction, the path under a connection can change without
/// the transport knowing, and a packet size found on one path is not
/// worth anything on the next: discovery stays off, and every packet
/// fits the floor every path is required to carry.
fn transport(profile: MediaProfile, one_path: bool) -> Arc<TransportConfig> {
    let mut config = TransportConfig::default();
    config.congestion_controller_factory(Arc::new(profile));
    config.datagram_send_buffer_size(SEND_QUEUE);
    config.datagram_receive_buffer_size(Some(RECEIVE_QUEUE));
    config.max_idle_timeout(Some(
        MAXIMUM_IDLE.try_into().expect("idle timeout representable"),
    ));
    config.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
    if !one_path {
        config.mtu_discovery_config(None);
    }
    Arc::new(config)
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// What the end that waits presents, and whom it lets in.
fn server_config(
    identity: &Identity,
    allowed: impl Into<AllowedPeers>,
    profile: MediaProfile,
    one_path: bool,
) -> Result<ServerConfig, EndpointError> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| EndpointError::Configuration(e.to_string()))?
        .with_client_cert_verifier(Arc::new(PinnedPeer::new(allowed)))
        .with_single_cert(vec![identity.certificate().clone()], identity.key())
        .map_err(|e| EndpointError::Configuration(e.to_string()))?;
    tls.alpn_protocols = vec![PROTOCOL.to_vec()];

    let quic =
        QuicServerConfig::try_from(tls).map_err(|e| EndpointError::Configuration(e.to_string()))?;
    let mut config = ServerConfig::with_crypto(Arc::new(quic));
    config.transport_config(transport(profile, one_path));
    Ok(config)
}

/// What the end that goes presents, and whom it expects.
fn client_config(
    identity: &Identity,
    peer: Fingerprint,
    profile: MediaProfile,
    one_path: bool,
) -> Result<ClientConfig, EndpointError> {
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| EndpointError::Configuration(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedPeer::new(peer)))
        .with_client_auth_cert(vec![identity.certificate().clone()], identity.key())
        .map_err(|e| EndpointError::Configuration(e.to_string()))?;
    tls.alpn_protocols = vec![PROTOCOL.to_vec()];

    let quic =
        QuicClientConfig::try_from(tls).map_err(|e| EndpointError::Configuration(e.to_string()))?;
    let mut config = ClientConfig::new(Arc::new(quic));
    config.transport_config(transport(profile, one_path));
    Ok(config)
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

/// Opens the endpoint on a junction's socket.
fn open_at(junction: &Junction, server: Option<ServerConfig>) -> Result<Endpoint, EndpointError> {
    let runtime = quinn::default_runtime()
        .ok_or_else(|| EndpointError::Configuration("no async runtime".to_string()))?;
    Ok(Endpoint::new_with_abstract_socket(
        quinn::EndpointConfig::default(),
        server,
        Arc::new(junction.clone()),
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
        profile: MediaProfile,
        listen: SocketAddr,
    ) -> Result<Self, EndpointError> {
        Self::host_on_path(identity, allowed, profile, listen, Path::Direct)
    }

    /// The same, on a path whose quality is imposed.
    ///
    /// Reserved for the measurement bench: this is how congestion
    /// control is tested without a degraded network at hand.
    pub fn host_on_path(
        identity: &Identity,
        allowed: impl Into<AllowedPeers>,
        profile: MediaProfile,
        listen: SocketAddr,
        path: Path,
    ) -> Result<Self, EndpointError> {
        let config = server_config(identity, allowed, profile, true)?;
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
        profile: MediaProfile,
        junction: &Junction,
    ) -> Result<Self, EndpointError> {
        let config = server_config(identity, allowed, profile, false)?;
        Ok(Self {
            endpoint: open_at(junction, Some(config))?,
        })
    }

    /// The end that goes towards the other device.
    pub fn client(
        identity: &Identity,
        peer: Fingerprint,
        profile: MediaProfile,
        listen: SocketAddr,
    ) -> Result<Self, EndpointError> {
        Self::client_on_path(identity, peer, profile, listen, Path::Direct)
    }

    /// The same, on a path whose quality is imposed.
    pub fn client_on_path(
        identity: &Identity,
        peer: Fingerprint,
        profile: MediaProfile,
        listen: SocketAddr,
        path: Path,
    ) -> Result<Self, EndpointError> {
        let config = client_config(identity, peer, profile, true)?;
        let mut endpoint = open(listen, None, path)?;
        endpoint.set_default_client_config(config);
        Ok(Self { endpoint })
    }

    /// The end that goes, on a junction: what it connects to is the
    /// card of the computer the junction expects.
    pub fn client_at(
        identity: &Identity,
        peer: Fingerprint,
        profile: MediaProfile,
        junction: &Junction,
    ) -> Result<Self, EndpointError> {
        let config = client_config(identity, peer, profile, false)?;
        let mut endpoint = open_at(junction, None)?;
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

    /// Waits for the remote device to connect.
    ///
    /// A refusal names where it came from. The address is read before the
    /// handshake is awaited, since nothing of it survives a failure, and
    /// without it « somebody was turned away » reads exactly like
    /// « nobody came », which are the two halves of every fault here.
    pub async fn accept(&self) -> Result<Connection, EndpointError> {
        let incoming = self.endpoint.accept().await.ok_or(EndpointError::Closed)?;
        let from = incoming.remote_address();
        let connection = incoming
            .await
            .map_err(|e| EndpointError::Connection(format!("{from} : {e}")))?;
        Ok(Connection::new(connection))
    }

    /// Waits for the connections in progress to end.
    pub async fn close(&self) {
        self.endpoint.close(0u32.into(), b"end of session");
        self.endpoint.wait_idle().await;
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
        self.inner
            .open_bi()
            .await
            .map_err(|e| EndpointError::Connection(e.to_string()))
    }

    /// Waits for a reliable stream the peer opens.
    pub async fn accept_stream(&self) -> Result<(SendStream, RecvStream), EndpointError> {
        self.inner
            .accept_bi()
            .await
            .map_err(|e| EndpointError::Connection(e.to_string()))
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
        self.inner
            .read_datagram()
            .await
            .map_err(|e| EndpointError::Connection(e.to_string()))
    }

    /// Waits for the connection to break.
    pub async fn closed(&self) {
        self.inner.closed().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn a_datagram_bigger_than_the_path_is_refused() {
        let pair = pair().await;
        let huge = Bytes::from(vec![0u8; 64 * 1024]);
        assert!(matches!(
            pair.client_side.send_datagram(huge),
            Err(DatagramError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn a_reliable_stream_crosses_the_tunnel() {
        let pair = pair().await;
        let receiver = pair.host_side.clone();
        let waiting = tokio::spawn(async move {
            let (_, mut reading) = receiver.accept_stream().await.unwrap();
            reading.read_to_end(64).await.unwrap()
        });

        let (mut writing, _) = pair.client_side.open_stream().await.unwrap();
        writing.write_all(b"negotiation").await.unwrap();
        writing.finish().unwrap();

        assert_eq!(waiting.await.unwrap(), b"negotiation");
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
