//! The datagrams waiting to be written onto the local link.
//!
//! Reading datagrams off the connection must never wait on whoever is at
//! the other end of the link: a player that stops reading for a moment
//! would otherwise stop the connection's own reading, and everything
//! behind it. So what comes off the connection is queued, and when the
//! queue is full the oldest goes first: a picture that is late is worth
//! less than the one after it, and the player asks for a key frame when
//! it finds a hole.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use zyr_transport::Bytes;

use crate::channel::DatagramChannel;

/// Most bytes waiting at once, each datagram weighing what [`weight`]
/// says.
///
/// Well over a key frame at the highest rate offered, so a burst never
/// loses a piece of itself while the link is merely busy; the link drains
/// a gigabyte a second, so what this holds is a few milliseconds of it.
pub const ROOM: usize = 4 * 1024 * 1024;

/// What one datagram weighs while it waits: its payload, and its place
/// in the queue. The place counts too, or datagrams carrying nothing
/// would weigh nothing and pile up without end.
fn weight(payload: &Bytes) -> usize {
    payload.len() + std::mem::size_of::<(DatagramChannel, Bytes)>()
}

/// Datagrams bound for the link, oldest first.
#[derive(Debug, Default)]
pub struct DatagramQueue {
    waiting: Mutex<Waiting>,
    arrived: Notify,
}

#[derive(Debug, Default)]
struct Waiting {
    /// Each with when it was queued.
    datagrams: VecDeque<(DatagramChannel, Bytes, Instant)>,
    bytes: usize,
}

impl DatagramQueue {
    /// Queues a datagram, throwing the oldest away while there is no room
    /// for it. Answers how many were thrown away, and how many bytes are
    /// left waiting.
    pub fn push(&self, channel: DatagramChannel, payload: Bytes) -> (u64, usize) {
        let mut dropped = 0;
        let left;
        {
            let mut waiting = self.waiting.lock().expect("datagrammes en attente");
            waiting.bytes += weight(&payload);
            waiting
                .datagrams
                .push_back((channel, payload, Instant::now()));
            while waiting.bytes > ROOM {
                let Some((_, oldest, _)) = waiting.datagrams.pop_front() else {
                    break;
                };
                waiting.bytes -= weight(&oldest);
                dropped += 1;
            }
            left = waiting.bytes;
        }
        self.arrived.notify_one();
        (dropped, left)
    }

    /// Waits for the oldest datagram, and takes it, with how long it
    /// waited.
    ///
    /// Safe to abandon while waiting: nothing is taken until it is handed
    /// back.
    pub async fn pop(&self) -> (DatagramChannel, Bytes, Duration) {
        loop {
            let arrived = self.arrived.notified();
            {
                let mut waiting = self.waiting.lock().expect("datagrammes en attente");
                if let Some((channel, payload, queued)) = waiting.datagrams.pop_front() {
                    waiting.bytes -= weight(&payload);
                    return (channel, payload, queued.elapsed());
                }
            }
            arrived.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datagram(size: usize, mark: u8) -> Bytes {
        Bytes::from(vec![mark; size])
    }

    #[tokio::test]
    async fn datagrams_come_out_in_the_order_they_went_in() {
        let queue = DatagramQueue::default();
        for mark in 0..10 {
            let channel = if mark % 2 == 0 {
                DatagramChannel::Video
            } else {
                DatagramChannel::Audio
            };
            assert_eq!(queue.push(channel, datagram(100, mark)).0, 0);
        }
        for mark in 0..10 {
            let (channel, payload, _) = queue.pop().await;
            assert_eq!(payload[0], mark);
            assert_eq!(channel == DatagramChannel::Video, mark % 2 == 0);
        }
    }

    #[tokio::test]
    async fn a_full_queue_throws_the_oldest_away() {
        let queue = DatagramQueue::default();
        let size = 1200;
        let fits = ROOM / weight(&datagram(size, 0));
        for turn in 0..fits + 5 {
            let (dropped, _) =
                queue.push(DatagramChannel::Video, datagram(size, (turn % 251) as u8));
            assert_eq!(dropped, u64::from(turn >= fits), "turn {turn}");
        }
        // What is left starts five datagrams in: the newest are kept.
        let (_, first, _) = queue.pop().await;
        assert_eq!(first[0], 5);
    }

    #[tokio::test]
    async fn datagrams_carrying_nothing_still_fill_the_queue() {
        // A far end sending empty datagrams to a player that has stopped
        // reading: they are thrown away like any other, rather than
        // piling up for as long as the player is gone.
        let queue = DatagramQueue::default();
        let fits = ROOM / weight(&Bytes::new());
        for _ in 0..fits {
            assert_eq!(queue.push(DatagramChannel::Audio, Bytes::new()).0, 0);
        }
        assert_eq!(queue.push(DatagramChannel::Audio, Bytes::new()).0, 1);
    }

    #[tokio::test]
    async fn a_datagram_larger_than_the_room_empties_the_queue_of_itself() {
        let queue = DatagramQueue::default();
        queue.push(DatagramChannel::Audio, datagram(10, 1));
        assert_eq!(
            queue.push(DatagramChannel::Video, datagram(ROOM + 1, 2)).0,
            2
        );
        let waited = tokio::time::timeout(std::time::Duration::from_millis(20), queue.pop()).await;
        assert!(waited.is_err(), "nothing should be left waiting");
    }

    #[tokio::test]
    async fn a_pop_waits_for_what_arrives_later() {
        let queue = std::sync::Arc::new(DatagramQueue::default());
        let pushing = queue.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            pushing.push(DatagramChannel::Audio, datagram(8, 7));
        });
        let (channel, payload, waited) = queue.pop().await;
        assert_eq!(channel, DatagramChannel::Audio);
        assert_eq!(payload[0], 7);
        // Popped the moment it came: it hardly waited.
        assert!(waited < std::time::Duration::from_millis(20), "{waited:?}");
    }
}
