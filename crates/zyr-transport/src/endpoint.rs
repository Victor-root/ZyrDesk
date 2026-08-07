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
use crate::identity::{Fingerprint, Identity, PinnedPeer};
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

/// Rhythm at which path discovery is read back.
const DISCOVERY_POLL: Duration = Duration::from_millis(50);

/// Identical readings in a row after which the path counts as settled.
const SETTLED_READINGS: u32 = 3;

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
fn transport(profile: MediaProfile) -> Arc<TransportConfig> {
    let mut config = TransportConfig::default();
    config.congestion_controller_factory(Arc::new(profile));
    config.datagram_send_buffer_size(SEND_QUEUE);
    config.datagram_receive_buffer_size(Some(RECEIVE_QUEUE));
    config.max_idle_timeout(Some(
        MAXIMUM_IDLE.try_into().expect("idle timeout representable"),
    ));
    config.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
    Arc::new(config)
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
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

/// One end of the tunnel.
pub struct TunnelEndpoint {
    endpoint: Endpoint,
}

impl TunnelEndpoint {
    /// The end that waits for the other device to connect.
    pub fn host(
        identity: &Identity,
        peer: Fingerprint,
        profile: MediaProfile,
        listen: SocketAddr,
    ) -> Result<Self, EndpointError> {
        Self::host_on_path(identity, peer, profile, listen, Path::Direct)
    }

    /// The same, on a path whose quality is imposed.
    ///
    /// Reserved for the measurement bench: this is how congestion
    /// control is tested without a degraded network at hand.
    pub fn host_on_path(
        identity: &Identity,
        peer: Fingerprint,
        profile: MediaProfile,
        listen: SocketAddr,
        path: Path,
    ) -> Result<Self, EndpointError> {
        let mut tls = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| EndpointError::Configuration(e.to_string()))?
            .with_client_cert_verifier(Arc::new(PinnedPeer::new(peer)))
            .with_single_cert(vec![identity.certificate().clone()], identity.key())
            .map_err(|e| EndpointError::Configuration(e.to_string()))?;
        tls.alpn_protocols = vec![PROTOCOL.to_vec()];

        let quic = QuicServerConfig::try_from(tls)
            .map_err(|e| EndpointError::Configuration(e.to_string()))?;
        let mut config = ServerConfig::with_crypto(Arc::new(quic));
        config.transport_config(transport(profile));

        Ok(Self {
            endpoint: open(listen, Some(config), path)?,
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
        let mut tls = rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| EndpointError::Configuration(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedPeer::new(peer)))
            .with_client_auth_cert(vec![identity.certificate().clone()], identity.key())
            .map_err(|e| EndpointError::Configuration(e.to_string()))?;
        tls.alpn_protocols = vec![PROTOCOL.to_vec()];

        let quic = QuicClientConfig::try_from(tls)
            .map_err(|e| EndpointError::Configuration(e.to_string()))?;
        let mut config = ClientConfig::new(Arc::new(quic));
        config.transport_config(transport(profile));

        let mut endpoint = open(listen, None, path)?;
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
    pub async fn accept(&self) -> Result<Connection, EndpointError> {
        let incoming = self.endpoint.accept().await.ok_or(EndpointError::Closed)?;
        let connection = incoming
            .await
            .map_err(|e| EndpointError::Connection(e.to_string()))?;
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

    /// Payload one datagram can carry on the current path.
    ///
    /// It moves with path discovery: this is what decides the packet
    /// size we ask the engine for.
    pub fn usable_datagram(&self) -> Option<u16> {
        self.inner
            .max_datagram_size()
            .map(|size| u16::try_from(size).unwrap_or(u16::MAX))
    }

    /// Waits for path discovery to settle, then reports the usable room
    /// it ends up with.
    ///
    /// The transport starts from a cautious size and probes upwards.
    /// Asking too early would give a needlessly small packet size, and
    /// the engine would keep it for the whole session: it cannot change
    /// it along the way.
    pub async fn settled_usable_datagram(&self, patience: Duration) -> Option<u16> {
        let deadline = std::time::Instant::now() + patience;
        let mut best = self.usable_datagram()?;
        let mut unchanged = 0;

        while unchanged < SETTLED_READINGS && std::time::Instant::now() < deadline {
            tokio::time::sleep(DISCOVERY_POLL).await;
            let current = self.usable_datagram()?;
            if current > best {
                best = current;
                unchanged = 0;
            } else {
                unchanged += 1;
            }
        }
        Some(best)
    }

    pub fn round_trip(&self) -> Duration {
        self.inner.rtt()
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
    async fn the_usable_room_never_shrinks_once_settled() {
        let pair = pair().await;
        let immediate = pair.client_side.usable_datagram().unwrap();
        let settled = pair
            .client_side
            .settled_usable_datagram(PATIENCE)
            .await
            .unwrap();
        assert!(
            settled >= immediate,
            "{settled} once settled against {immediate} straight away"
        );
        assert_eq!(settled, pair.client_side.usable_datagram().unwrap());
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
