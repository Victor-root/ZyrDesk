//! What sends, what echoes back, what keeps the clock.
//!
//! Packets leave in bursts, one per frame, the way a video encoder sends
//! them: that rhythm is what puts a path to the test, not a steady flow.
//! Each packet carries its own age, so the round trip reads itself on
//! return without the two computers having to agree on the time.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::measurement::{Outcome, RoundTrip};

/// Age of the packet, written at its head.
const TIMESTAMP: usize = size_of::<u64>();

/// Grace given to the last packets to come back.
const GRACE: Duration = Duration::from_millis(500);

/// No UDP datagram goes beyond this size.
const BUFFER: usize = 65_535;

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

/// Sends back everything that arrives, unchanged.
pub async fn echo(socket: UdpSocket) -> io::Result<()> {
    let mut buffer = vec![0u8; BUFFER];
    loop {
        let (read, source) = socket.recv_from(&mut buffer).await?;
        socket.send_to(&buffer[..read], source).await?;
    }
}

/// Sends at the requested cadence and times what comes back.
pub async fn probe(socket: UdpSocket, target: SocketAddr, cadence: Cadence) -> io::Result<Outcome> {
    if (cadence.size as usize) < TIMESTAMP {
        return Err(io::Error::other(format!(
            "a probe packet is at least {TIMESTAMP} bytes"
        )));
    }

    socket.connect(target).await?;
    let socket = Arc::new(socket);
    let start = Instant::now();

    let receiver = {
        let socket = socket.clone();
        tokio::spawn(async move { gather(&socket, start, cadence.duration).await })
    };

    let (sent, duration) = send(&socket, start, cadence).await?;
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
async fn send(socket: &UdpSocket, start: Instant, cadence: Cadence) -> io::Result<(u64, Duration)> {
    let mut packet = vec![0u8; cadence.size as usize];
    let mut rhythm = tokio::time::interval(cadence.interval());
    let per_frame = cadence.packets_per_frame();
    let mut sent = 0u64;

    while start.elapsed() < cadence.duration {
        rhythm.tick().await;
        for _ in 0..per_frame {
            let age = start.elapsed().as_nanos() as u64;
            packet[..TIMESTAMP].copy_from_slice(&age.to_le_bytes());
            socket.send(&packet).await?;
            sent += 1;
        }
    }

    Ok((sent, start.elapsed()))
}

/// Gathers the returns, up to the deadline plus the grace period.
async fn gather(socket: &UdpSocket, start: Instant, duration: Duration) -> Vec<RoundTrip> {
    let mut measurements = Vec::new();
    let mut buffer = vec![0u8; BUFFER];

    let _ = tokio::time::timeout(duration + GRACE, async {
        while let Ok(read) = socket.recv(&mut buffer).await {
            if let Some(round_trip) = time_it(&buffer[..read], start) {
                measurements.push(round_trip);
            }
        }
    })
    .await;

    measurements
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
    async fn a_full_probe_measures_what_comes_back() {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = echo(listener).await;
        });

        let mut cadence = cadence(1300, 10, 60);
        cadence.duration = Duration::from_millis(200);
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let outcome = probe(sender, target, cadence).await.unwrap();

        assert!(outcome.sent > 0);
        assert_eq!(outcome.lost(), 0, "nothing gets lost over loopback");
        assert!(outcome.rate() > 0.0);
    }

    #[tokio::test]
    async fn a_packet_too_short_to_carry_its_age_is_refused() {
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let target: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert!(probe(sender, target, cadence(4, 10, 60)).await.is_err());
    }
}
