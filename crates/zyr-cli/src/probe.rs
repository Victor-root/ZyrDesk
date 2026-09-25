//! What sends, what echoes back, what keeps the clock.
//!
//! Packets leave in bursts, one per frame, the way a video encoder sends
//! them: that rhythm is what puts a path to the test, not a steady flow.
//! Each packet carries its own age, so the round trip reads itself on
//! return without the two computers having to agree on the time.
//!
//! The same packets take two roads: a bare UDP socket, answered by an
//! echo on the other computer, and the player's end of a local link,
//! answered through the whole tunnel by a stand-in engine that sends
//! every picture it receives straight back.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use zyr_control::link::{Channel, Link, LinkReader, LinkWriter};

use crate::measurement::{Outcome, RoundTrip};

/// Age of the packet, written at its head.
const TIMESTAMP: usize = size_of::<u64>();

/// Grace given to the last packets to come back.
const GRACE: Duration = Duration::from_millis(500);

/// No UDP datagram goes beyond this size.
const BUFFER: usize = 65_535;

/// Buffers the echoes' sockets ask for.
///
/// The system default, often 64 KiB, is only about ten milliseconds of
/// video at a common rate: the echo being starved of CPU for the length
/// of one preemption is enough for the kernel to drop packets, silently,
/// and the bare road would then be measured worse than it is. Four
/// mebibytes comfortably cover a scheduling hiccup.
const SOCKET_BUFFER: usize = 4 * 1024 * 1024;

/// Sending rhythm, modelled on a video encoder's.
#[derive(Debug, Clone, Copy)]
pub struct Cadence {
    pub size: u16,
    pub rate_mbps: u64,
    pub frames_per_second: u32,
    pub duration: Duration,
}

impl Cadence {
    /// Packets to send per frame, at least one.
    pub fn packets_per_frame(&self) -> u32 {
        let per_second = self.rate_mbps * 1_000_000 / 8 / self.size.max(1) as u64;
        (per_second / self.frames_per_second.max(1) as u64).max(1) as u32
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(1) / self.frames_per_second.max(1)
    }
}

/// Opens a UDP socket sized for a video stream.
///
/// The system may grant only part of the buffers asked for, or refuse:
/// it then keeps its own, which stay usable.
///
/// To be called from a running async runtime: the socket has to register
/// with it to be watched.
pub fn open_socket(address: SocketAddr) -> io::Result<UdpSocket> {
    let domain = match address {
        SocketAddr::V4(_) => Domain::IPV4,
        SocketAddr::V6(_) => Domain::IPV6,
    };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    let _ = socket.set_recv_buffer_size(SOCKET_BUFFER);
    let _ = socket.set_send_buffer_size(SOCKET_BUFFER);
    socket.set_nonblocking(true)?;
    ignore_unreachable_reports(&socket)?;
    socket.bind(&address.into())?;
    UdpSocket::from_std(socket.into())
}

/// Stops Windows from failing a receive because of an earlier send.
///
/// Sending a datagram to a port nobody listens on draws an ICMP reply,
/// and Windows hands that back as an error on the *next* receive, on a
/// socket which is otherwise perfectly fine. The echo answers the last
/// packets of a measurement after the other bench has closed the socket
/// that sent them: without this, the echo would die there, and the next
/// measurement would find nobody answering. Every other system keeps
/// those reports away from an unconnected socket; this asks Windows to
/// do the same.
#[cfg(windows)]
fn ignore_unreachable_reports(socket: &Socket) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{SIO_UDP_CONNRESET, SOCKET, WSAIoctl};

    let report: u32 = 0;
    let mut answered: u32 = 0;
    // SAFETY: the socket is ours and open, and the value read from lives
    // until the call returns.
    let outcome = unsafe {
        WSAIoctl(
            socket.as_raw_socket() as SOCKET,
            SIO_UDP_CONNRESET,
            (&raw const report).cast(),
            size_of::<u32>() as u32,
            std::ptr::null_mut(),
            0,
            &mut answered,
            std::ptr::null_mut(),
            None,
        )
    };
    if outcome != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn ignore_unreachable_reports(_socket: &Socket) -> io::Result<()> {
    Ok(())
}

/// Sends back everything that arrives, unchanged.
pub async fn echo(socket: UdpSocket) -> io::Result<()> {
    let mut buffer = vec![0u8; BUFFER];
    loop {
        let (read, source) = socket.recv_from(&mut buffer).await?;
        socket.send_to(&buffer[..read], source).await?;
    }
}

/// Where the probe's packets go, and come back from.
pub enum Road {
    /// A socket, answered by [`echo`] at that address.
    Bare { socket: UdpSocket, echo: SocketAddr },
    /// The player's end of a local link: the packets leave as pictures,
    /// and come back as pictures once the tunnel has carried them to the
    /// other bench and back.
    Tunnel(Link),
}

/// Where the packets leave from.
enum Out {
    Bare(Arc<UdpSocket>),
    Tunnel(LinkWriter),
}

impl Out {
    async fn send(&mut self, packet: &[u8]) -> io::Result<()> {
        match self {
            Out::Bare(socket) => socket.send(packet).await.map(drop),
            Out::Tunnel(writer) => writer.send(Channel::Video, packet).await,
        }
    }
}

/// Where they come back.
enum Back {
    Bare(Arc<UdpSocket>),
    Tunnel(LinkReader),
}

impl Back {
    /// Waits for the next packet back and copies it into `buffer`: its
    /// length, or `None` once nothing more can come.
    async fn next(&mut self, buffer: &mut [u8]) -> Option<usize> {
        match self {
            Back::Bare(socket) => socket.recv(buffer).await.ok(),
            Back::Tunnel(reader) => loop {
                match reader.next().await {
                    Ok(Some((Channel::Video, packet))) => {
                        let length = packet.len().min(buffer.len());
                        buffer[..length].copy_from_slice(&packet[..length]);
                        return Some(length);
                    }
                    // What else the link says is not the probe's.
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => return None,
                }
            },
        }
    }
}

/// Sends at the requested cadence and times what comes back.
pub async fn probe(road: Road, cadence: Cadence) -> io::Result<Outcome> {
    if (cadence.size as usize) < TIMESTAMP {
        return Err(io::Error::other(format!(
            "a probe packet is at least {TIMESTAMP} bytes"
        )));
    }

    let (mut out, back) = match road {
        Road::Bare { socket, echo } => {
            socket.connect(echo).await?;
            let socket = Arc::new(socket);
            (Out::Bare(socket.clone()), Back::Bare(socket))
        }
        Road::Tunnel(link) => {
            let (reader, writer) = link.split();
            (Out::Tunnel(writer), Back::Tunnel(reader))
        }
    };
    let start = Instant::now();
    let receiver = tokio::spawn(gather(back, start, cadence.duration));

    let (sent, duration) = send(&mut out, start, cadence).await?;
    let measurements = receiver.await.map_err(io::Error::other)?;
    Ok(Outcome::from(
        measurements,
        sent,
        sent * cadence.size as u64,
        duration,
    ))
}

/// Sends until the time is up, and reports how many packets went out and
/// over how long.
async fn send(out: &mut Out, start: Instant, cadence: Cadence) -> io::Result<(u64, Duration)> {
    let mut packet = vec![0u8; cadence.size as usize];
    let mut rhythm = tokio::time::interval(cadence.interval());
    let per_frame = cadence.packets_per_frame();
    let mut sent = 0u64;

    while start.elapsed() < cadence.duration {
        rhythm.tick().await;
        for _ in 0..per_frame {
            let age = start.elapsed().as_nanos() as u64;
            packet[..TIMESTAMP].copy_from_slice(&age.to_le_bytes());
            out.send(&packet).await?;
            sent += 1;
        }
    }

    Ok((sent, start.elapsed()))
}

/// Gathers the returns, up to the deadline plus the grace period.
async fn gather(mut back: Back, start: Instant, duration: Duration) -> Vec<RoundTrip> {
    let mut measurements = Vec::new();
    let mut buffer = vec![0u8; BUFFER];

    let _ = tokio::time::timeout(duration + GRACE, async {
        while let Some(read) = back.next(&mut buffer).await {
            if let Some(round_trip) = time_it(&buffer[..read], start) {
                measurements.push(round_trip);
            }
        }
    })
    .await;

    measurements
}

/// Sends back every picture that arrives on the link, unchanged: the
/// stand-in engine at the far end of the tunnel.
pub async fn echo_pictures(link: Link) -> io::Result<()> {
    let (mut reader, mut writer) = link.split();
    while let Some((channel, packet)) = reader.next().await? {
        if channel == Channel::Video {
            writer.send(Channel::Video, &packet).await?;
        }
    }
    Ok(())
}

/// Reads back the age written in a packet and works out its round trip.
fn time_it(packet: &[u8], start: Instant) -> Option<RoundTrip> {
    let timestamp: [u8; TIMESTAMP] = packet.get(..TIMESTAMP)?.try_into().ok()?;
    let age = u64::from_le_bytes(timestamp);
    let now = start.elapsed().as_nanos() as u64;
    // A packet younger than its own departure makes no sense: it is
    // padding foreign to the probe.
    Some(RoundTrip(Duration::from_nanos(now.checked_sub(age)?)))
}

#[cfg(test)]
mod tests {
    use zyr_control::link::{self, Access, LinkListener};

    use super::*;

    fn cadence(size: u16, rate: u64, fps: u32) -> Cadence {
        Cadence {
            size,
            rate_mbps: rate,
            frames_per_second: fps,
            duration: Duration::from_secs(1),
        }
    }

    #[test]
    fn the_burst_per_frame_matches_the_target_rate() {
        // 50 Mb/s at 60 frames per second with 1300-byte packets:
        // 6.25 MB/s, so about 4807 packets, so 80 per frame.
        let cadence = cadence(1300, 50, 60);
        assert_eq!(cadence.packets_per_frame(), 80);
        assert_eq!(cadence.interval(), Duration::from_nanos(16_666_666));
    }

    #[test]
    fn a_tiny_cadence_still_sends_something() {
        // Without a floor, the bench would send nothing and measure
        // nothing.
        assert_eq!(cadence(1300, 1, 240).packets_per_frame(), 1);
        assert_eq!(cadence(1300, 0, 60).packets_per_frame(), 1);
    }

    #[test]
    fn a_degenerate_cadence_divides_by_no_zero() {
        // The values are bounded at input; these guards are here so a
        // zero from elsewhere never makes the bench panic.
        let degenerate = cadence(0, 50, 0);
        assert!(degenerate.packets_per_frame() >= 1);
        assert_eq!(degenerate.interval(), Duration::from_secs(1));
    }

    #[test]
    fn the_written_age_gives_the_round_trip() {
        let start = Instant::now();
        let mut packet = vec![0u8; 64];
        packet[..TIMESTAMP].copy_from_slice(&0u64.to_le_bytes());
        let measured = time_it(&packet, start).unwrap();
        assert!(measured.0 < Duration::from_millis(100));
    }

    #[test]
    fn a_packet_foreign_to_the_probe_is_ignored() {
        let start = Instant::now();
        assert!(time_it(&[1, 2, 3], start).is_none());

        // An age set in the future: this packet did not come from here.
        let mut packet = vec![0u8; 64];
        packet[..TIMESTAMP].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(time_it(&packet, start).is_none());
    }

    #[tokio::test]
    async fn the_echo_sends_back_what_it_receives() {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = echo(listener).await;
        });

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(b"packet", address).await.unwrap();
        let mut received = [0u8; 16];
        let (read, _) = sender.recv_from(&mut received).await.unwrap();
        assert_eq!(&received[..read], b"packet");
    }

    #[tokio::test]
    async fn the_stand_in_engine_sends_back_the_pictures_and_nothing_else() {
        let listener = LinkListener::create(Access::SystemOnly).unwrap();
        let name = listener.name().to_string();
        let (player, engine) = tokio::join!(link::connect(&name), listener.accept());
        tokio::spawn(echo_pictures(engine.unwrap()));

        let (mut reader, mut writer) = player.unwrap().split();
        writer.send(Channel::Control, b"hello").await.unwrap();
        writer.send(Channel::Video, b"picture").await.unwrap();
        let (channel, back) = reader.next().await.unwrap().unwrap();
        assert_eq!((channel, &back[..]), (Channel::Video, &b"picture"[..]));
    }

    #[tokio::test]
    async fn a_probe_through_a_link_measures_what_the_engine_sends_back() {
        let listener = LinkListener::create(Access::SystemOnly).unwrap();
        let name = listener.name().to_string();
        let (player, engine) = tokio::join!(link::connect(&name), listener.accept());
        tokio::spawn(echo_pictures(engine.unwrap()));

        let mut cadence = cadence(1160, 10, 60);
        cadence.duration = Duration::from_millis(200);
        let outcome = probe(Road::Tunnel(player.unwrap()), cadence).await.unwrap();

        assert!(outcome.sent > 0);
        assert_eq!(outcome.lost(), 0, "nothing gets lost on a local link");
    }

    #[tokio::test]
    async fn a_full_probe_measures_what_comes_back() {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = echo(listener).await;
        });

        let mut cadence = cadence(1300, 10, 60);
        cadence.duration = Duration::from_millis(200);
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let outcome = probe(
            Road::Bare {
                socket: sender,
                echo: target,
            },
            cadence,
        )
        .await
        .unwrap();

        assert!(outcome.sent > 0);
        assert_eq!(outcome.lost(), 0, "nothing gets lost over loopback");
        assert!(outcome.rate() > 0.0);
    }

    #[tokio::test]
    async fn a_packet_too_short_to_carry_its_age_is_refused() {
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let echo: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let road = Road::Bare {
            socket: sender,
            echo,
        };
        assert!(probe(road, cadence(4, 10, 60)).await.is_err());
    }
}
