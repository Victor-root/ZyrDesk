//! The whole tunnel, between two fake engines.
//!
//! The real engines are not needed to check what matters here: that a
//! byte dropped in on one side comes out identical on the other, on the
//! right port, in both directions, without either end having to know a
//! tunnel exists.
//!
//! Both sides run in the same process, on two distinct loopback
//! addresses: the host engine on 127.0.0.1, the client-side listeners on
//! the address dedicated to the device. That is exactly the addressing
//! scheme planned for real.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_transport::{Identity, MediaProfile, TunnelEndpoint};
use zyr_tunnel::{Answers, Tunnel, aside};

/// Past this, nothing is getting through.
const PATIENCE: Duration = Duration::from_secs(10);

/// Where the host engine listens, as on a real machine.
const ENGINE: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

async fn before_the_end<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, work)
        .await
        .expect("the tunnel let nothing through")
}

/// Code the fake engine refuses, standing in for an engine that has
/// nobody waiting on one.
const REFUSED_PIN: &str = "9999";

/// The host engine as the tunnel sees it: its ports, and a pairing code
/// written down instead of handed to anything.
struct FakeEngine {
    ports: EnginePorts,
    handed: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    /// Times Ctrl+Alt+Suppr was asked for. Counted rather than done:
    /// nothing here has a Windows to press it on.
    attended: Arc<AtomicU32>,
    /// Whether the far computer was asked to go quiet. Written down for
    /// the same reason: nothing here has speakers to silence.
    hushed: Arc<AtomicBool>,
}

impl Answers for FakeEngine {
    fn engine(&self) -> EnginePorts {
        self.ports
    }

    fn hand_over_the_code(&self, pin: &str, name: &str) -> Result<(), String> {
        if pin == REFUSED_PIN {
            return Err("le moteur n'attend aucun code".to_string());
        }
        self.handed
            .lock()
            .unwrap()
            .push((pin.to_string(), name.to_string()));
        Ok(())
    }

    fn secure_attention(&self) -> Result<(), String> {
        self.attended.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn hush_the_speakers(&self, quiet: bool) -> Result<(), String> {
        self.hushed.store(quiet, Ordering::Relaxed);
        Ok(())
    }
}

/// The tunnel brought up on both sides, kept alive for the test.
///
/// Everything is dropped together at the end: the pumps stop with it.
struct Bench {
    _endpoints: (TunnelEndpoint, TunnelEndpoint),
    _host: Tunnel,
    client: Tunnel,
    /// Address the client engine believes the host to be at.
    client_side: IpAddr,
    ports: EnginePorts,
    /// The way, still open, to speak to the far ZyrDesk rather than to
    /// its engine.
    connection: zyr_transport::Connection,
    /// What the host engine was handed.
    handed: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    /// Times the far ZyrDesk was asked to press Ctrl+Alt+Suppr.
    attended: Arc<AtomicU32>,
    /// Whether the far ZyrDesk was asked to silence its speakers.
    hushed: Arc<AtomicBool>,
}

impl Bench {
    async fn bring_up(base: u16, device: u16) -> Self {
        let ports = EnginePorts::new(base).unwrap();
        let client_side = IpAddr::V4(device_loopback_addr(device).unwrap());

        let host_identity = Identity::generate().unwrap();
        let client_identity = Identity::generate().unwrap();
        let profile = MediaProfile::default();
        let ephemeral = SocketAddr::new(ENGINE, 0);

        let host_endpoint = TunnelEndpoint::host(
            &host_identity,
            client_identity.fingerprint(),
            profile,
            ephemeral,
        )
        .unwrap();
        let meeting_point = host_endpoint.local_address().unwrap();
        let client_endpoint = TunnelEndpoint::client(
            &client_identity,
            host_identity.fingerprint(),
            profile,
            ephemeral,
        )
        .unwrap();

        let (host_side, client_connection) = tokio::join!(
            host_endpoint.accept(),
            client_endpoint.connect(meeting_point)
        );

        let handed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let attended = Arc::new(AtomicU32::new(0));
        let hushed = Arc::new(AtomicBool::new(false));
        let host = Tunnel::host(
            host_side.unwrap(),
            ENGINE,
            Arc::new(FakeEngine {
                ports,
                handed: handed.clone(),
                attended: attended.clone(),
                hushed: hushed.clone(),
            }),
        )
        .await
        .unwrap();

        // The real sequence, not a shortcut: the client learns the
        // host's engine ports before opening the local ones that stand
        // in for them. Nothing here is allowed to know them in advance.
        let client_connection = client_connection.unwrap();
        let engine = aside::ask_the_ports(&client_connection).await.unwrap();
        let client = Tunnel::client(client_connection.clone(), client_side, engine)
            .await
            .unwrap();

        Self {
            _endpoints: (host_endpoint, client_endpoint),
            _host: host,
            client,
            client_side,
            ports: engine,
            connection: client_connection,
            handed,
            attended,
            hushed,
        }
    }

    /// Address of an engine port, as the client engine sees it.
    fn as_the_client_sees(&self, port: u16) -> SocketAddr {
        SocketAddr::new(self.client_side, port)
    }
}

/// Fake host engine that echoes back whatever is written to it, in TCP.
async fn tcp_engine(port: u16) {
    let listener = TcpListener::bind(SocketAddr::new(ENGINE, port))
        .await
        .unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut reading, mut writing) = stream.split();
                let _ = tokio::io::copy(&mut reading, &mut writing).await;
                let _ = writing.shutdown().await;
            });
        }
    });
}

/// Fake host engine that answers in UDP, naming the port it was reached
/// on: that is how we check no channel crosses another.
async fn udp_engine(port: u16) {
    let socket = UdpSocket::bind(SocketAddr::new(ENGINE, port))
        .await
        .unwrap();
    tokio::spawn(async move {
        let mut buffer = [0u8; 2048];
        while let Ok((read, source)) = socket.recv_from(&mut buffer).await {
            let answer = format!("{port}:{}", String::from_utf8_lossy(&buffer[..read]));
            let _ = socket.send_to(answer.as_bytes(), source).await;
        }
    });
}

#[tokio::test]
async fn the_client_learns_the_host_engine_ports_from_the_host() {
    // The base port is picked by the host when its engine starts. A
    // client that guessed it would open its stand-in ports on the wrong
    // numbers, and the session would go nowhere with nothing to explain
    // it.
    let bench = Bench::bring_up(42700, 5).await;
    assert_eq!(bench.ports.base(), 42700);
}

#[tokio::test]
async fn the_pairing_code_travels_through_the_tunnel() {
    // C'est ce qui remplace un code affiché sur un écran et tapé sur
    // l'autre. Le tunnel a déjà reconnu les deux ordinateurs à leur
    // empreinte avant de s'ouvrir : le code ne prouve rien de plus, et
    // personne n'a plus à se lever.
    let bench = Bench::bring_up(42850, 8).await;

    before_the_end(aside::ask_to_pair(
        &bench.connection,
        "0429",
        "PC de Victor",
    ))
    .await
    .unwrap();

    let handed = bench.handed.lock().unwrap().clone();
    assert_eq!(
        handed,
        vec![("0429".to_string(), "PC de Victor".to_string())]
    );
}

#[tokio::test]
async fn ctrl_alt_suppr_travels_on_the_product_s_own_channel() {
    // Windows garde cette combinaison pour lui aux deux bouts : celui qui
    // regarde ne la voit jamais, et celui qui est regardé ne peut pas la
    // recevoir d'un moteur. Elle traverse donc entre les deux moitiés de
    // ZyrDesk, et aucun moteur n'en sait rien.
    let bench = Bench::bring_up(42500, 7).await;

    before_the_end(aside::ask_for_the_secure_attention(&bench.connection))
        .await
        .unwrap();

    assert_eq!(bench.attended.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn couper_le_son_de_l_hote_se_demande_depuis_le_client() {
    // C'est celui qui prend la main qui sait si la pièce d'en face doit
    // se taire, et il n'est pas dedans pour aller le dire. La demande
    // traverse donc entre les deux moitiés de ZyrDesk, comme le reste de
    // ce qui n'appartient à aucun moteur.
    let bench = Bench::bring_up(42950, 10).await;

    before_the_end(aside::ask_to_hush(&bench.connection, true))
        .await
        .unwrap();
    assert!(bench.hushed.load(Ordering::Relaxed));

    // Et dans l'autre sens, parce qu'une session peut finir sans que la
    // machine d'en face s'en aperçoive autrement.
    before_the_end(aside::ask_to_hush(&bench.connection, false))
        .await
        .unwrap();
    assert!(!bench.hushed.load(Ordering::Relaxed));
}

#[tokio::test]
async fn an_engine_that_refuses_the_code_says_so_rather_than_going_quiet() {
    // Sinon l'ordinateur qui se connecte attendrait sur un moteur qui
    // n'attend rien, sans rien à montrer.
    let bench = Bench::bring_up(42900, 9).await;

    let refusal = before_the_end(aside::ask_to_pair(&bench.connection, REFUSED_PIN, "PC"))
        .await
        .unwrap_err();
    assert!(
        refusal.to_string().contains("n'attend aucun code"),
        "{refusal}"
    );

    // Et la voie tient toujours : un appairage raté n'emporte pas la
    // session avec lui.
    let ports = before_the_end(aside::ask_the_ports(&bench.connection))
        .await
        .unwrap();
    assert_eq!(ports.base(), 42900);
}

#[tokio::test]
async fn a_reliable_stream_crosses_the_tunnel_both_ways() {
    let bench = Bench::bring_up(42100, 0).await;
    tcp_engine(bench.ports.http()).await;

    let mut stream = before_the_end(TcpStream::connect(
        bench.as_the_client_sees(bench.ports.http()),
    ))
    .await
    .unwrap();
    stream.write_all(b"pairing").await.unwrap();
    stream.shutdown().await.unwrap();

    let mut received = Vec::new();
    before_the_end(stream.read_to_end(&mut received))
        .await
        .unwrap();
    assert_eq!(received, b"pairing");
}

#[tokio::test]
async fn a_datagram_crosses_the_tunnel_both_ways() {
    let bench = Bench::bring_up(42200, 1).await;
    udp_engine(bench.ports.video()).await;

    let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
        .await
        .unwrap();
    client
        .send_to(b"ping", bench.as_the_client_sees(bench.ports.video()))
        .await
        .unwrap();

    let mut received = [0u8; 64];
    let (read, _) = before_the_end(client.recv_from(&mut received))
        .await
        .unwrap();
    assert_eq!(
        &received[..read],
        format!("{}:ping", bench.ports.video()).as_bytes()
    );
}

#[tokio::test]
async fn each_channel_lands_on_its_own_engine_port() {
    let bench = Bench::bring_up(42300, 2).await;
    for port in bench.ports.udp_ports() {
        udp_engine(port).await;
    }

    // The three channels share one datagram queue: if the header were
    // misread, the video would land in the audio.
    for port in bench.ports.udp_ports() {
        let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
            .await
            .unwrap();
        client
            .send_to(b"ping", bench.as_the_client_sees(port))
            .await
            .unwrap();

        let mut received = [0u8; 64];
        let (read, _) = before_the_end(client.recv_from(&mut received))
            .await
            .unwrap();
        assert_eq!(&received[..read], format!("{port}:ping").as_bytes());
    }
}

#[tokio::test]
async fn the_engine_web_interface_stays_out_of_the_tunnel() {
    let bench = Bench::bring_up(42400, 3).await;
    tcp_engine(bench.ports.web_ui()).await;

    // It does run on the host side, but nothing listens for it on the
    // client side: it is reachable only from the machine hosting it.
    assert!(
        TcpStream::connect(bench.as_the_client_sees(bench.ports.web_ui()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn packets_sent_before_the_engine_listens_do_not_end_the_session() {
    // The engine opens its media ports only once the negotiation is
    // over, so everything the tunnel relays until then lands nowhere.
    // That must cost those packets and nothing else: ending the pump
    // there would break the negotiation still under way on the reliable
    // streams, and the session would fail with no visible cause.
    let bench = Bench::bring_up(42800, 6).await;
    let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
        .await
        .unwrap();

    for _ in 0..20 {
        client
            .send_to(b"early", bench.as_the_client_sees(bench.ports.video()))
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The engine shows up late, and the session carries on.
    udp_engine(bench.ports.video()).await;
    client
        .send_to(b"ping", bench.as_the_client_sees(bench.ports.video()))
        .await
        .unwrap();

    let mut received = [0u8; 64];
    let (read, _) = before_the_end(client.recv_from(&mut received))
        .await
        .unwrap();
    assert_eq!(
        &received[..read],
        format!("{}:ping", bench.ports.video()).as_bytes()
    );
}

#[tokio::test]
async fn the_counters_follow_what_travels() {
    let bench = Bench::bring_up(42600, 4).await;
    udp_engine(bench.ports.audio()).await;
    assert_eq!(bench.client.reading(), zyr_tunnel::Reading::default());

    let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
        .await
        .unwrap();
    client
        .send_to(b"ping", bench.as_the_client_sees(bench.ports.audio()))
        .await
        .unwrap();
    let mut received = [0u8; 64];
    before_the_end(client.recv_from(&mut received))
        .await
        .unwrap();

    let reading = bench.client.reading();
    assert_eq!(reading.to_tunnel, 1);
    assert_eq!(reading.to_engine, 1);
    assert_eq!(reading.too_large, 0);
    assert_eq!(reading.unreadable, 0);
}
