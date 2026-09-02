//! Moving bytes between the engines and the tunnel.
//!
//! This module does not know which side it sits on. It moves bytes
//! between a local endpoint, which talks to an engine over loopback, and
//! the encrypted connection. Both ends of the tunnel use it under the
//! same rules; only the assembly differs.

use std::io;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use zyr_proto::net::EnginePorts;
use zyr_transport::{Connection, DatagramError, RecvStream, SendStream};

use crate::channel::{DatagramChannel, StreamChannel};
use crate::frame;

/// No UDP datagram can be larger than this.
///
/// The buffer is sized never to truncate: a truncation would pass for a
/// valid packet and silently corrupt the stream.
const BUFFER: usize = 65_535;

/// Buffers we ask the sockets that talk to the engine for.
///
/// The system default, often 64 KiB, is only about ten milliseconds of
/// video at a common rate: the pump being starved of CPU for the length
/// of one preemption is enough for the kernel to drop packets. It does
/// so silently, with neither the tunnel nor the transport able to count
/// it, which makes that loss particularly painful to diagnose. Four
/// mebibytes comfortably cover a scheduling hiccup.
const SOCKET_BUFFER: usize = 4 * 1024 * 1024;

/// Consecutive failures one channel puts up with before giving up.
///
/// A datagram lost to a passing system hiccup must not end a session,
/// and a socket that has genuinely broken must not spin forever.
const TOLERATED_FAILURES: u32 = 64;

/// Opens a UDP socket sized for a video stream.
///
/// The system may grant only part of what is asked, or refuse: it then
/// keeps its own value, which stays usable.
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
/// socket which is otherwise perfectly fine. The engine only opens its
/// media ports once the session negotiation is over, so the first
/// packets the tunnel relays necessarily land nowhere: without this, the
/// pump dies in the middle of the handshake and takes the session with
/// it. Every other system keeps those reports away from an unconnected
/// socket; this asks Windows to do the same.
#[cfg(windows)]
fn ignore_unreachable_reports(socket: &Socket) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{SIO_UDP_CONNRESET, SOCKET, WSAIoctl};

    let report: u32 = 0;
    let mut answered: u32 = 0;
    // Safe: the socket is ours and open, and the value read from lives
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

/// Tunnel counters, read by the measurement bench.
#[derive(Debug, Default)]
pub struct Counters {
    to_tunnel: AtomicU64,
    to_engine: AtomicU64,
    too_large: AtomicU64,
    crowded: AtomicU64,
    no_recipient: AtomicU64,
    unreadable: AtomicU64,
    refused: AtomicU64,
}

/// Snapshot of the counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reading {
    pub to_tunnel: u64,
    pub to_engine: u64,
    /// Packets refused for exceeding what the path accepts. Anything but
    /// zero means the packet size asked of the engine is too big for the
    /// path.
    pub too_large: u64,
    /// Packets handed to a send queue that had no room left for them.
    /// The transport took each of them by throwing an older one away,
    /// silently: this is the count of those holes in the picture.
    pub crowded: u64,
    /// Packets that arrived for a channel the local engine has not yet
    /// spoken on.
    pub no_recipient: u64,
    /// Datagrams whose header names no known channel.
    pub unreadable: u64,
    /// Packets the system refused to hand over or to send. A few at the
    /// start of a session are normal: the engine has not opened its
    /// media ports yet.
    pub refused: u64,
}

impl Counters {
    fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reading(&self) -> Reading {
        Reading {
            to_tunnel: self.to_tunnel.load(Ordering::Relaxed),
            to_engine: self.to_engine.load(Ordering::Relaxed),
            too_large: self.too_large.load(Ordering::Relaxed),
            crowded: self.crowded.load(Ordering::Relaxed),
            no_recipient: self.no_recipient.load(Ordering::Relaxed),
            unreadable: self.unreadable.load(Ordering::Relaxed),
            refused: self.refused.load(Ordering::Relaxed),
        }
    }
}

/// UDP end of one channel: the engine on one side, the tunnel on the other.
#[derive(Debug)]
pub struct EnginePort {
    socket: UdpSocket,
    /// Where to reach the engine.
    engine: Mutex<Option<SocketAddr>>,
    /// On the client side the engine picks its source port and may
    /// change it from one session to the next, so the address is read
    /// again on every packet. On the host side it is fixed by the
    /// engine's listening port and never moves.
    follows_the_source: bool,
}

impl EnginePort {
    /// Host-side end: the engine listens at a known address.
    pub fn towards_engine(engine: SocketAddr) -> io::Result<Self> {
        let socket = open_socket(SocketAddr::new(engine.ip(), 0))?;
        Ok(Self {
            socket,
            engine: Mutex::new(Some(engine)),
            follows_the_source: false,
        })
    }

    /// Client-side end: the engine comes to us, on the port it believes
    /// belongs to the remote host.
    pub fn from_engine(listen: SocketAddr) -> io::Result<Self> {
        let socket = open_socket(listen)?;
        Ok(Self {
            socket,
            engine: Mutex::new(None),
            follows_the_source: true,
        })
    }

    pub fn local_address(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    fn destination(&self) -> Option<SocketAddr> {
        *self.engine.lock().expect("engine address lock")
    }

    /// Waits for a packet from the engine.
    pub async fn receive(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let (read, source) = self.socket.recv_from(buffer).await?;
        if self.follows_the_source {
            *self.engine.lock().expect("engine address lock") = Some(source);
        }
        Ok(read)
    }

    /// Hands the engine what came out of the tunnel.
    ///
    /// Returns `false` when the engine has not spoken on this channel
    /// yet: there is then nobody to hand the packet to.
    pub async fn send(&self, payload: &[u8]) -> io::Result<bool> {
        let Some(destination) = self.destination() else {
            return Ok(false);
        };
        self.socket.send_to(payload, destination).await?;
        Ok(true)
    }
}

/// The three UDP ends of one side of the tunnel.
///
/// Built together so each channel necessarily lands on the right engine
/// port.
#[derive(Debug)]
pub struct DatagramPorts([EnginePort; DatagramChannel::ALL.len()]);

impl DatagramPorts {
    /// Host side: each channel talks to the matching engine port.
    pub fn towards_engine(engine: std::net::IpAddr, ports: EnginePorts) -> io::Result<Self> {
        Self::bring_up(ports, |port| {
            EnginePort::towards_engine(SocketAddr::new(engine, port))
        })
    }

    /// Client side: each channel listens where the engine believes the
    /// host to be.
    pub fn from_engine(listen: std::net::IpAddr, ports: EnginePorts) -> io::Result<Self> {
        Self::bring_up(ports, |port| {
            EnginePort::from_engine(SocketAddr::new(listen, port))
        })
    }

    fn bring_up(
        ports: EnginePorts,
        mut open: impl FnMut(u16) -> io::Result<EnginePort>,
    ) -> io::Result<Self> {
        let mut opened = Vec::with_capacity(DatagramChannel::ALL.len());
        for channel in DatagramChannel::ALL {
            opened.push(open(channel.port(ports))?);
        }
        Ok(Self(opened.try_into().expect("one port per known channel")))
    }

    pub fn port(&self, channel: DatagramChannel) -> &EnginePort {
        &self.0[channel.rank()]
    }
}

/// Announces the channel at the head of a reliable stream.
pub async fn announce(sending: &mut SendStream, channel: StreamChannel) -> io::Result<()> {
    sending.write_all(&[channel.identifier()]).await?;
    Ok(())
}

/// Reads the channel announcement at the head of a reliable stream.
pub async fn read_announcement(receiving: &mut RecvStream) -> io::Result<StreamChannel> {
    let mut head = [0u8; 1];
    receiving
        .read_exact(&mut head)
        .await
        .map_err(io::Error::other)?;
    StreamChannel::from_identifier(head[0]).map_err(io::Error::other)
}

/// Moves bytes between a local connection and a tunnel stream.
///
/// Each direction stops at its own end of stream without cutting the
/// other: an engine that has finished talking is still waiting for the
/// answer.
pub async fn relay_stream(
    mut local: TcpStream,
    mut sending: SendStream,
    mut receiving: RecvStream,
) -> io::Result<()> {
    let (mut local_read, mut local_write) = local.split();

    let upward = async {
        tokio::io::copy(&mut local_read, &mut sending).await?;
        sending.shutdown().await
    };
    let downward = async {
        tokio::io::copy(&mut receiving, &mut local_write).await?;
        local_write.shutdown().await
    };

    tokio::try_join!(upward, downward)?;
    Ok(())
}

/// Carries into the tunnel everything the engine sends on one channel.
pub async fn collect_datagrams(
    channel: DatagramChannel,
    port: &EnginePort,
    connection: &Connection,
    counters: &Counters,
) -> io::Result<()> {
    let mut buffer = vec![0u8; BUFFER];
    let mut failures = 0;
    loop {
        // A refused packet concerns that packet alone. Ending the pump
        // here would end the whole session, video and all, over one
        // datagram the system did not want.
        let read = match port.receive(&mut buffer).await {
            Ok(read) => {
                failures = 0;
                read
            }
            Err(e) => {
                Counters::bump(&counters.refused);
                failures += 1;
                if failures > TOLERATED_FAILURES {
                    return Err(e);
                }
                continue;
            }
        };
        let framed = frame::encode(channel, &buffer[..read]);
        // Asked before handing over rather than deduced afterwards: the
        // transport makes room by throwing the oldest away and says
        // nothing, so this is the only moment that loss can be counted.
        if connection.send_queue_room() < framed.len() {
            Counters::bump(&counters.crowded);
        }
        match connection.send_datagram(framed.into()) {
            Ok(()) => Counters::bump(&counters.to_tunnel),
            // The path narrowed since the packet size was asked of the
            // engine. Dropping beats fragmenting: the video protocol's
            // error correction exists for this.
            Err(DatagramError::TooLarge) => Counters::bump(&counters.too_large),
            Err(e) => return Err(io::Error::other(e)),
        }
    }
}

/// Hands the engines the datagrams that come out of the tunnel.
///
/// One reader for the three channels: a connection's datagrams arrive
/// through a single queue, and the header says whom to give them to.
pub async fn distribute_datagrams(
    connection: &Connection,
    ports: &DatagramPorts,
    counters: &Counters,
) -> io::Result<()> {
    let mut failures = 0;
    loop {
        let received = connection.read_datagram().await.map_err(io::Error::other)?;
        let Ok((channel, payload)) = frame::decode(&received) else {
            Counters::bump(&counters.unreadable);
            continue;
        };
        // The engine opens its media ports late: the first packets of a
        // session are handed to nobody, and on some systems that is
        // reported as an error. It concerns one packet, never the
        // session.
        match ports.port(channel).send(payload).await {
            Ok(true) => {
                failures = 0;
                Counters::bump(&counters.to_engine);
            }
            Ok(false) => Counters::bump(&counters.no_recipient),
            Err(e) => {
                Counters::bump(&counters.refused);
                failures += 1;
                if failures > TOLERATED_FAILURES {
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[tokio::test]
    async fn the_engine_sockets_can_absorb_a_burst() {
        // The system may trim what we ask for, but it must not stay at
        // an ordinary socket's default: a burst of frames would swamp
        // it, and packets lost there are counted nowhere.
        let socket = open_socket(local(0)).unwrap();
        let raw = Socket::from(socket.into_std().unwrap());
        assert!(
            raw.recv_buffer_size().unwrap() >= 128 * 1024,
            "receive buffer of {} bytes",
            raw.recv_buffer_size().unwrap()
        );
    }

    #[tokio::test]
    async fn a_host_side_port_targets_the_engine_without_waiting() {
        let engine = UdpSocket::bind(local(0)).await.unwrap();
        let address = engine.local_addr().unwrap();

        let port = EnginePort::towards_engine(address).unwrap();
        assert!(port.send(b"ping").await.unwrap());

        let mut received = [0u8; 16];
        let (read, _) = engine.recv_from(&mut received).await.unwrap();
        assert_eq!(&received[..read], b"ping");
    }

    #[tokio::test]
    async fn a_client_side_port_waits_for_the_engine_to_speak_first() {
        let port = EnginePort::from_engine(local(0)).unwrap();
        let address = port.local_address().unwrap();

        // Nothing has arrived yet: there is nobody to answer.
        assert!(!port.send(b"frame").await.unwrap());

        let engine = UdpSocket::bind(local(0)).await.unwrap();
        engine.send_to(b"ping", address).await.unwrap();

        let mut received = [0u8; 16];
        assert_eq!(port.receive(&mut received).await.unwrap(), 4);
        assert!(port.send(b"frame").await.unwrap());

        let (read, _) = engine.recv_from(&mut received).await.unwrap();
        assert_eq!(&received[..read], b"frame");
    }

    #[tokio::test]
    async fn a_client_side_port_follows_an_engine_that_changes_source() {
        let port = EnginePort::from_engine(local(0)).unwrap();
        let address = port.local_address().unwrap();
        let mut received = [0u8; 16];

        let first = UdpSocket::bind(local(0)).await.unwrap();
        first.send_to(b"a", address).await.unwrap();
        port.receive(&mut received).await.unwrap();

        // New engine session, new source port. The answers must follow,
        // or they leave towards a dead port.
        let second = UdpSocket::bind(local(0)).await.unwrap();
        second.send_to(b"b", address).await.unwrap();
        port.receive(&mut received).await.unwrap();

        port.send(b"frame").await.unwrap();
        let (read, _) = second.recv_from(&mut received).await.unwrap();
        assert_eq!(&received[..read], b"frame");
    }

    #[tokio::test]
    async fn each_channel_lands_on_the_expected_engine_port() {
        let ports = EnginePorts::new(42500).unwrap();
        let opened = DatagramPorts::from_engine([127, 0, 0, 1].into(), ports).unwrap();
        for channel in DatagramChannel::ALL {
            let bound = opened.port(channel).local_address().unwrap().port();
            assert_eq!(bound, channel.port(ports));
        }
    }
}
