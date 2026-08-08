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
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_transport::{Identity, MediaProfile, TunnelEndpoint};
use zyr_tunnel::{Tunnel, greeting};

/// Past this, nothing is getting through.
const PATIENCE: Duration = Duration::from_secs(10);

/// Where the host engine listens, as on a real machine.
const ENGINE: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

async fn before_the_end<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, work)
        .await
        .expect("the tunnel let nothing through")
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

        let host = Tunnel::host(host_side.unwrap(), ENGINE, ports)
            .await
            .unwrap();

        // The real sequence, not a shortcut: the client learns the
        // host's engine ports before opening the local ones that stand
        // in for them. Nothing here is allowed to know them in advance.
        let client_connection = client_connection.unwrap();
        let greeting = greeting::ask(&client_connection).await.unwrap();
        let client = Tunnel::client(client_connection, client_side, greeting.engine)
            .await
            .unwrap();

        Self {
            _endpoints: (host_endpoint, client_endpoint),
            _host: host,
            client,
            client_side,
            ports: greeting.engine,
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
