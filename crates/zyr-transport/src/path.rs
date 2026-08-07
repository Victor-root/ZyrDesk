//! A deliberately degraded path, to put the transport to the test.
//!
//! A laboratory network loses nothing. Yet the property the whole
//! network architecture rests on is precisely the one that only shows
//! under loss: an ordinary congestion control takes a loss for an order
//! to slow down and strangles the video, where the media controller has
//! to hold its rate.
//!
//! This socket wrapper drops a fraction of outgoing packets underneath
//! the transport, where a saturated link would lose them. The transport
//! therefore sees real losses, with its real detection machinery.
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
    /// expressed per thousand.
    Degraded { loss_per_thousand: u16 },
}

/// Socket that loses a fraction of what it is handed.
#[derive(Debug)]
pub struct DegradedPath {
    inner: Arc<dyn AsyncUdpSocket>,
    loss_per_thousand: u64,
    sent: AtomicU64,
}

impl DegradedPath {
    pub fn new(inner: Arc<dyn AsyncUdpSocket>, loss_per_thousand: u16) -> Self {
        Self {
            inner,
            loss_per_thousand: u64::from(loss_per_thousand).min(PER_THOUSAND),
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
        stir(rank) % PER_THOUSAND < self.loss_per_thousand
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
    fn send(loss_per_thousand: u16, packets: u64) -> u64 {
        let arrived = Arc::new(Counter(AtomicU64::new(0)));
        let path = DegradedPath::new(arrived.clone(), loss_per_thousand);
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
        assert_eq!(send(0, 10_000), 10_000);
    }

    #[test]
    fn the_requested_rate_is_honoured() {
        for per_thousand in [10u16, 20, 50] {
            let arrived = send(per_thousand, 100_000);
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
        assert_eq!(send(1000, 5_000), 0);
        // Past the base, the rate falls back to a total cut.
        assert_eq!(send(u16::MAX, 5_000), 0);
    }

    #[test]
    fn the_losses_do_not_fall_in_cadence() {
        // One loss every hundred packets exactly would match any sending
        // rhythm and hide window computation errors.
        let path = DegradedPath::new(Arc::new(Counter(AtomicU64::new(0))), 100);
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
