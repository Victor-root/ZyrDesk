//! What went through this side of the tunnel, second by second, for the
//! journal.
//!
//! A picture crosses a service on each computer on its way from the host
//! engine to the player: off a local link and into the connection on one,
//! out of the connection and onto a local link on the other. Neither is
//! meant to hold it for more than a few microseconds, and nothing said so
//! either way. Each second in which anything went through, one line says
//! what did, how long a datagram waited in this service, and what the
//! connection measured of the path meanwhile.
//!
//! Counted with atomics by the pumps as datagrams go by, and taken, then
//! set back to nought, once a second by a task of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use zyr_proto::log::Log;
use zyr_transport::{Connection, Traffic};

/// How often a line is written, while anything moves.
const EVERY: Duration = Duration::from_secs(1);

/// What one second saw go through.
#[derive(Debug)]
pub(crate) struct Flow {
    to_tunnel: AtomicU64,
    to_tunnel_bytes: AtomicU64,
    /// Least room left in the connection's queue of datagrams to send,
    /// seen as a datagram was handed over.
    least_room: AtomicU64,
    to_link: AtomicU64,
    to_link_bytes: AtomicU64,
    /// Between a datagram coming off the connection and its turn to be
    /// written onto the link.
    waited: Timing,
    /// Writing one datagram onto the link.
    writing: Timing,
    /// Most bytes waiting for the link at once.
    most_waiting: AtomicU64,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            to_tunnel: AtomicU64::new(0),
            to_tunnel_bytes: AtomicU64::new(0),
            least_room: AtomicU64::new(u64::MAX),
            to_link: AtomicU64::new(0),
            to_link_bytes: AtomicU64::new(0),
            waited: Timing::default(),
            writing: Timing::default(),
            most_waiting: AtomicU64::new(0),
        }
    }
}

/// Durations, as their sum, their count and their longest, in
/// microseconds.
#[derive(Debug, Default)]
struct Timing {
    sum: AtomicU64,
    count: AtomicU64,
    most: AtomicU64,
}

impl Timing {
    fn add(&self, took: Duration) {
        let us = u64::try_from(took.as_micros()).unwrap_or(u64::MAX);
        self.sum.fetch_add(us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.most.fetch_max(us, Ordering::Relaxed);
    }

    /// `mean/worst` in milliseconds, set back to nought.
    fn taken(&self) -> String {
        let sum = self.sum.swap(0, Ordering::Relaxed);
        let count = self.count.swap(0, Ordering::Relaxed);
        let most = self.most.swap(0, Ordering::Relaxed);
        match sum.checked_div(count) {
            Some(mean) => format!("{:.2}/{:.2}", mean as f64 / 1000.0, most as f64 / 1000.0),
            None => "-".to_string(),
        }
    }
}

impl Flow {
    /// A datagram off the link was handed to the connection, which had
    /// `room` bytes left in its queue.
    pub(crate) fn handed_to_the_tunnel(&self, bytes: usize, room: usize) {
        self.to_tunnel.fetch_add(1, Ordering::Relaxed);
        self.to_tunnel_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
        self.least_room.fetch_min(room as u64, Ordering::Relaxed);
    }

    /// `waiting` bytes wait for the link, a datagram just queued.
    pub(crate) fn queued(&self, waiting: usize) {
        self.most_waiting
            .fetch_max(waiting as u64, Ordering::Relaxed);
    }

    /// A datagram that waited that long for its turn was written onto
    /// the link, which took `writing`.
    pub(crate) fn onto_the_link(&self, bytes: usize, waited: Duration, writing: Duration) {
        self.to_link.fetch_add(1, Ordering::Relaxed);
        self.to_link_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
        self.waited.add(waited);
        self.writing.add(writing);
    }

    /// The line for the second just gone, or nothing when nothing went
    /// through it; the counts start again from nought either way.
    fn second(&self, path: &Traffic, before: &Traffic) -> Option<String> {
        let to_tunnel = self.to_tunnel.swap(0, Ordering::Relaxed);
        let to_tunnel_bytes = self.to_tunnel_bytes.swap(0, Ordering::Relaxed);
        let least_room = self.least_room.swap(u64::MAX, Ordering::Relaxed);
        let to_link = self.to_link.swap(0, Ordering::Relaxed);
        let to_link_bytes = self.to_link_bytes.swap(0, Ordering::Relaxed);
        let most_waiting = self.most_waiting.swap(0, Ordering::Relaxed);
        let waited = self.waited.taken();
        let writing = self.writing.taken();
        if to_tunnel == 0 && to_link == 0 {
            return None;
        }
        let sent = path.packets_sent.saturating_sub(before.packets_sent);
        let sends = path.sends.saturating_sub(before.sends);
        let received = path
            .packets_received
            .saturating_sub(before.packets_received);
        let receives = path.receives.saturating_sub(before.receives);
        let room = if least_room == u64::MAX {
            "-".to_string()
        } else {
            format!("{} KB", least_room / 1024)
        };
        Some(format!(
            "into the tunnel {to_tunnel} datagrams, {} KB, the queue to send left at least {room}; \
             onto the link {to_link} datagrams, {} KB, having waited here {waited} ms and been \
             written in {writing} ms (mean/worst), at most {} KB waiting; the path: round trip \
             {:.1} ms (least {:.1}), {sent} packets sent in {sends} sends, {received} received \
             in {receives} reads, {} lost",
            to_tunnel_bytes / 1024,
            to_link_bytes / 1024,
            most_waiting / 1024,
            path.round_trip.as_secs_f64() * 1000.0,
            path.least_round_trip.as_secs_f64() * 1000.0,
            path.lost.saturating_sub(before.lost),
        ))
    }
}

/// Writes a line for every second anything went through, for as long as
/// the tunnel stands.
pub(crate) async fn watch(connection: Connection, flow: Arc<Flow>, log: Log) {
    let mut before = connection.traffic();
    let mut ticks = tokio::time::interval(EVERY);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick is at once, before anything has had time to go by.
    ticks.tick().await;
    loop {
        ticks.tick().await;
        let path = connection.traffic();
        if let Some(line) = flow.second(&path, &before) {
            log.debug(|| line);
        }
        before = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traffic(packets: u64, lost: u64) -> Traffic {
        Traffic {
            packets_sent: packets,
            sends: packets / 4,
            packets_received: packets / 10,
            receives: packets / 10,
            lost,
            round_trip: Duration::from_micros(12_400),
            least_round_trip: Duration::from_micros(9_800),
        }
    }

    #[test]
    fn a_second_says_what_went_through_and_starts_again() {
        let flow = Flow::default();
        assert_eq!(flow.second(&traffic(0, 0), &traffic(0, 0)), None);

        flow.handed_to_the_tunnel(1200, 900 * 1024);
        flow.handed_to_the_tunnel(1200, 700 * 1024);
        flow.queued(3 * 1024);
        flow.onto_the_link(1100, Duration::from_micros(40), Duration::from_micros(10));
        flow.onto_the_link(1100, Duration::from_micros(120), Duration::from_micros(30));
        let line = flow.second(&traffic(1_400, 3), &traffic(1_000, 1)).unwrap();
        assert!(
            line.starts_with(
                "into the tunnel 2 datagrams, 2 KB, the queue to send left at least 700 KB; onto \
                 the link 2 datagrams, 2 KB, having waited here 0.08/0.12 ms and been written in \
                 0.02/0.03 ms (mean/worst), at most 3 KB waiting;"
            ),
            "{line}"
        );
        assert!(
            line.ends_with(
                "round trip 12.4 ms (least 9.8), 400 packets sent in 100 sends, 40 received in \
                 40 reads, 2 lost"
            ),
            "{line}"
        );
        // Nothing more went through: nothing to say.
        assert_eq!(flow.second(&traffic(1_400, 3), &traffic(1_400, 3)), None);
    }
}
