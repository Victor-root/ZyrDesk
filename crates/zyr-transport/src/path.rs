//! A deliberately degraded path, to put the transport to the test.
//!
//! A laboratory network loses nothing. Yet the property the whole
//! network architecture rests on is precisely the one that only shows
//! under loss: an ordinary congestion control takes a loss for an order
//! to slow down and strangles the video, where the media controller has
//! to hold its rate.
//!
//! This socket wrapper drops a fraction of outgoing packets underneath
//! the transport, where a saturated link would lose them, and when asked
//! runs of them in a row, as interference does. The transport therefore
//! sees real losses, with its real detection machinery.
//!
//! It exists only to measure. Nothing in the product goes through it.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

/// Base the loss rate is expressed in.
pub const PER_THOUSAND: u64 = 1000;

/// Quality of the path underneath the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    /// The real path, as it is.
    Direct,
    /// Degraded path: the given fraction of outgoing packets is dropped,
    /// expressed per thousand, and now and then a run of them in a row.
    Degraded {
        loss_per_thousand: u16,
        lapses: Option<Lapses>,
    },
}

/// A moment of interference: a run of packets lost in a row.
///
/// Parity repairs a packet lost here and there, and cannot repair a
/// frame lost with all its shards. A run does that whatever the packets
/// are made of and however they are packed, where a loss drawn packet by
/// packet loses a frame or not by the way its shards happened to travel.
///
/// Counted in packets, like the loss, so that the same run replays the
/// same lapses. A lapse as long as its stretch is a total cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lapses {
    /// One lapse in this many packets.
    pub every: u32,
    /// Packets lost in a row at the end of each stretch: the first ones,
    /// which open the connection, are not lost.
    pub lasting: u32,
}

impl Lapses {
    fn covers(self, rank: u64) -> bool {
        let stretch = u64::from(self.every.max(1));
        rank % stretch >= stretch.saturating_sub(u64::from(self.lasting))
    }
}

/// Socket that loses a fraction of what it is handed.
#[derive(Debug)]
pub struct DegradedPath {
    inner: Arc<dyn AsyncUdpSocket>,
    loss_per_thousand: u64,
    lapses: Option<Lapses>,
    sent: AtomicU64,
}

impl DegradedPath {
    pub fn new(
        inner: Arc<dyn AsyncUdpSocket>,
        loss_per_thousand: u16,
        lapses: Option<Lapses>,
    ) -> Self {
        Self {
            inner,
            loss_per_thousand: u64::from(loss_per_thousand).min(PER_THOUSAND),
            lapses,
            sent: AtomicU64::new(0),
        }
    }

    /// Decides the fate of the next packet.
    ///
    /// The draw comes from a stirred counter rather than shared state:
    /// two tasks sending at once cannot tread on each other, and the same
    /// run replays the same losses.
    fn should_drop(&self) -> bool {
        let rank = self.sent.fetch_add(1, Ordering::Relaxed);
        self.lapses.is_some_and(|lapses| lapses.covers(rank))
            || stir(rank) % PER_THOUSAND < self.loss_per_thousand
    }
}

/// Stirs a counter so the losses do not fall in cadence.
///
/// A regular loss, exactly one packet in a hundred, resembles no network
/// at all and would let window computation errors slip through.
fn stir(rank: u64) -> u64 {
    let mut mixed = rank.wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

impl AsyncUdpSocket for DegradedPath {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.inner.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        if self.should_drop() {
            // The packet counts as gone: the network lost it, the sender
            // did not give up on it.
            return Ok(());
        }
        self.inner.try_send(transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        buffers: &mut [io::IoSliceMut<'_>],
        headers: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_recv(cx, buffers, headers)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    /// One packet per send, with no batching.
    ///
    /// Batching would put several packets in one send: dropping it would
    /// amount to losing a whole burst, and the requested loss rate would
    /// mean nothing any more.
    fn max_transmit_segments(&self) -> usize {
        1
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Socket that does nothing but count what it is handed.
    #[derive(Debug)]
    struct Counter(AtomicU64);

    impl AsyncUdpSocket for Counter {
        fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
            unimplemented!("the counter does not expect to be polled")
        }

        fn try_send(&self, _transmit: &Transmit) -> io::Result<()> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn poll_recv(
            &self,
            _cx: &mut Context,
            _buffers: &mut [io::IoSliceMut<'_>],
            _headers: &mut [RecvMeta],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok("127.0.0.1:0".parse().unwrap())
        }
    }

    /// Sends the given number of packets and reports what got through.
    fn send(loss_per_thousand: u16, lapses: Option<Lapses>, packets: u64) -> u64 {
        let arrived = Arc::new(Counter(AtomicU64::new(0)));
        let path = DegradedPath::new(arrived.clone(), loss_per_thousand, lapses);
        let payload = [0u8; 64];

        for _ in 0..packets {
            let transmit = Transmit {
                destination: "127.0.0.1:1".parse().unwrap(),
                ecn: None,
                contents: &payload,
                segment_size: None,
                src_ip: None,
            };
            path.try_send(&transmit).unwrap();
        }
        arrived.0.load(Ordering::Relaxed)
    }

    #[test]
    fn a_direct_path_loses_nothing() {
        assert_eq!(send(0, None, 10_000), 10_000);
    }

    #[test]
    fn the_requested_rate_is_honoured() {
        for per_thousand in [10u16, 20, 50] {
            let arrived = send(per_thousand, None, 100_000);
            let lost = 100_000 - arrived;
            let expected = per_thousand as u64 * 100;
            let gap = lost.abs_diff(expected);
            assert!(
                gap * 10 < expected,
                "{per_thousand} per thousand asked for, {lost} lost instead of {expected}"
            );
        }
    }

    #[test]
    fn a_fully_cut_path_lets_nothing_through() {
        assert_eq!(send(1000, None, 5_000), 0);
        // Past the base, the rate falls back to a total cut.
        assert_eq!(send(u16::MAX, None, 5_000), 0);
    }

    #[test]
    fn a_lapse_loses_a_run_of_packets_at_the_end_of_each_stretch() {
        let lapses = Lapses {
            every: 10,
            lasting: 3,
        };
        let path = DegradedPath::new(Arc::new(Counter(AtomicU64::new(0))), 0, Some(lapses));
        let lost: Vec<u64> = (0..30).filter(|_| path.should_drop()).collect();
        assert_eq!(lost, [7, 8, 9, 17, 18, 19, 27, 28, 29]);
    }

    #[test]
    fn a_lapse_as_long_as_its_stretch_is_a_total_cut() {
        let lapses = Lapses {
            every: 10,
            lasting: 10,
        };
        assert_eq!(send(0, Some(lapses), 1_000), 0);
    }

    #[test]
    fn the_lapses_come_on_top_of_the_loss() {
        let lapses = Lapses {
            every: 100,
            lasting: 5,
        };
        let lost = 100_000 - send(50, Some(lapses), 100_000);
        // Five in a hundred to the lapses, and a twentieth of what is left.
        let expected = 5_000 + 95_000 / 20;
        assert!(
            lost.abs_diff(expected) * 10 < expected,
            "{lost} lost instead of {expected}"
        );
    }

    #[test]
    fn the_losses_do_not_fall_in_cadence() {
        // One loss every hundred packets exactly would match any sending
        // rhythm and hide window computation errors.
        let path = DegradedPath::new(Arc::new(Counter(AtomicU64::new(0))), 100, None);
        let dropped: Vec<bool> = (0..2000).map(|_| path.should_drop()).collect();
        let gaps: Vec<usize> = dropped
            .iter()
            .enumerate()
            .filter(|(_, lost)| **lost)
            .map(|(rank, _)| rank)
            .collect::<Vec<_>>()
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect();

        assert!(gaps.len() > 100, "not enough losses to conclude anything");
        let distinct: std::collections::HashSet<_> = gaps.iter().collect();
        assert!(
            distinct.len() > 5,
            "the losses always fall at the same gap: {distinct:?}"
        );
    }
}
