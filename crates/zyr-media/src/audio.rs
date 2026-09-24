//! The sound on its way from host to client.
//!
//! One datagram per Opus packet, 10 ms of 48 kHz stereo, behind an
//! 8-byte header, little-endian:
//!
//! ```text
//! off size field
//! 0   1    version (1)
//! 1   1    flags (0)
//! 2   2    sequence (one more per packet sent, wrapping)
//! 4   4    captured (host clock when the sound was captured, us, low 32 bits)
//! ```
//!
//! Sound has no parity: a missing packet is 10 ms that the decoder
//! conceals, which costs less than the delay waiting for a repair would.

use crate::wire::{Reader, WireError};

/// Size of the header in front of every Opus packet.
pub const AUDIO_HEADER: usize = 8;

/// Packets gathered before playing starts, when nothing else is decided.
pub const DEFAULT_DEPTH: usize = 2;

/// Packets held beyond the depth before the oldest are dropped to get
/// back to it: this much reordering is waited for, and never more delay.
const HEADROOM: usize = 4;

const VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHeader {
    pub sequence: u16,
    pub captured_us: u32,
}

/// Writes one audio datagram into `out`, replacing what it held.
pub fn write_audio(sequence: u16, captured_us: u32, opus: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(AUDIO_HEADER + opus.len());
    out.push(VERSION);
    out.push(0);
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&captured_us.to_le_bytes());
    out.extend_from_slice(opus);
}

/// Splits an audio datagram into its header and its Opus packet.
pub fn read_audio(datagram: &[u8]) -> Result<(AudioHeader, &[u8]), WireError> {
    let mut reader = Reader::new(datagram);
    let version = reader.u8()?;
    if version != VERSION {
        return Err(WireError::Version(version.into()));
    }
    let _flags = reader.u8()?;
    let sequence = reader.u16()?;
    let captured_us = reader.u32()?;
    let opus = reader.rest();
    // Silence is sent as nothing at all, never as an empty packet.
    if opus.is_empty() {
        return Err(WireError::Truncated);
    }
    Ok((
        AudioHeader {
            sequence,
            captured_us,
        },
        opus,
    ))
}

/// What the player gets when the sound card wants the next 10 ms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popped {
    Packet(Vec<u8>),
    /// That packet never came while later ones did: to be concealed.
    Missing,
    /// Nothing to play: the host is silent, or the buffer is filling up
    /// again after running dry.
    Empty,
}

/// What became of the packets so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JitterCounters {
    pub packets: u64,
    /// Packets that came after their turn to play.
    pub late: u64,
    pub duplicates: u64,
    /// Turns played with nothing, concealed.
    pub missing: u64,
    /// Packets dropped to bring a buffer grown too long back to its
    /// depth.
    pub dropped: u64,
    /// Times the buffer ran dry while playing.
    pub underruns: u64,
}

/// Puts audio packets back in order and evens out their arrival.
///
/// Playing starts once `depth` packets are in, and stops again whenever
/// the buffer runs dry, which is what a host falling silent looks like:
/// the sound that follows starts afresh, with no gap to conceal. No more
/// than `depth` plus a small headroom is ever held, the oldest being
/// dropped when more comes, so that the delay stays what was chosen.
#[derive(Debug, Clone)]
pub struct JitterBuffer {
    depth: usize,
    /// A ring: the packet due next sits at `head`.
    slots: Vec<Option<Vec<u8>>>,
    head: usize,
    /// The sequence due next, unknown until a packet arrives after a
    /// start or a silence.
    next: Option<u16>,
    /// Slots from `head` up to the furthest packet held.
    reach: usize,
    held: usize,
    playing: bool,
    counters: JitterCounters,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_DEPTH)
    }
}

impl JitterBuffer {
    /// A buffer gathering `depth` packets, at least one, before playing.
    pub fn new(depth: usize) -> Self {
        let depth = depth.max(1);
        Self {
            depth,
            slots: vec![None; depth + HEADROOM],
            head: 0,
            next: None,
            reach: 0,
            held: 0,
            playing: false,
            counters: JitterCounters::default(),
        }
    }

    pub fn push(&mut self, sequence: u16, packet: &[u8]) {
        self.counters.packets += 1;
        let Some(next) = self.next else {
            self.next = Some(sequence);
            self.store(0, packet);
            return;
        };
        let ahead = sequence.wrapping_sub(next);
        if ahead >= 0x8000 {
            self.behind(sequence, next, packet);
            return;
        }
        let mut ahead = usize::from(ahead);
        if ahead >= self.slots.len() {
            // Too far ahead to hold: keep the newest `depth` turns.
            let drop = ahead + 1 - self.depth;
            self.skip(drop);
            ahead -= drop;
        }
        self.store(ahead, packet);
    }

    /// The packet due now, or what stands in for it.
    pub fn pop(&mut self) -> Popped {
        if !self.playing {
            if self.held < self.depth {
                return Popped::Empty;
            }
            self.playing = true;
        }
        if self.held == 0 {
            self.playing = false;
            self.next = None;
            self.counters.underruns += 1;
            return Popped::Empty;
        }
        let slot = self.slots[self.head].take();
        self.advance(1);
        match slot {
            Some(packet) => {
                self.held -= 1;
                Popped::Packet(packet)
            }
            None => {
                self.counters.missing += 1;
                Popped::Missing
            }
        }
    }

    pub fn counters(&self) -> JitterCounters {
        self.counters
    }

    /// A packet older than the one due next: too late once playing, but
    /// still in time while filling up, if the buffer can make room for it
    /// in front.
    fn behind(&mut self, sequence: u16, next: u16, packet: &[u8]) {
        let back = usize::from(next.wrapping_sub(sequence));
        if self.playing || self.reach + back > self.slots.len() {
            self.counters.late += 1;
            return;
        }
        self.head = (self.head + self.slots.len() - back) % self.slots.len();
        self.next = Some(sequence);
        self.reach += back;
        self.store(0, packet);
    }

    fn store(&mut self, ahead: usize, packet: &[u8]) {
        let at = (self.head + ahead) % self.slots.len();
        let slot = &mut self.slots[at];
        if slot.is_some() {
            self.counters.duplicates += 1;
            return;
        }
        *slot = Some(packet.to_vec());
        self.held += 1;
        self.reach = self.reach.max(ahead + 1);
    }

    /// Gives up the next `turns` turns, dropping what they held.
    fn skip(&mut self, turns: usize) {
        for offset in 0..turns.min(self.slots.len()) {
            let at = (self.head + offset) % self.slots.len();
            if self.slots[at].take().is_some() {
                self.held -= 1;
                self.counters.dropped += 1;
            }
        }
        self.advance(turns);
    }

    fn advance(&mut self, turns: usize) {
        self.head = (self.head + turns) % self.slots.len();
        self.next = self.next.map(|next| next.wrapping_add(turns as u16));
        self.reach = self.reach.saturating_sub(turns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Noise;

    fn packet(sequence: u16) -> Vec<u8> {
        sequence.to_le_bytes().to_vec()
    }

    fn played(popped: Popped) -> Option<u16> {
        match popped {
            Popped::Packet(bytes) => Some(u16::from_le_bytes([bytes[0], bytes[1]])),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_datagram_makes_the_round_trip() {
        let mut out = vec![1, 2, 3];
        write_audio(0xbeef, 123_456_789, &[9, 8, 7], &mut out);
        assert_eq!(out.len(), AUDIO_HEADER + 3);
        let (header, opus) = read_audio(&out).unwrap();
        assert_eq!(
            header,
            AudioHeader {
                sequence: 0xbeef,
                captured_us: 123_456_789
            }
        );
        assert_eq!(opus, &[9, 8, 7]);
    }

    #[test]
    fn a_short_empty_or_foreign_datagram_is_refused() {
        let mut out = Vec::new();
        write_audio(1, 2, &[3], &mut out);
        for len in 0..out.len() {
            assert_eq!(read_audio(&out[..len]), Err(WireError::Truncated));
        }
        out[0] = 2;
        assert_eq!(read_audio(&out), Err(WireError::Version(2)));
    }

    #[test]
    fn garbage_never_panics() {
        let mut noise = Noise::new(20);
        let mut valid = Vec::new();
        write_audio(7, 8, &[1; 120], &mut valid);
        let valid = vec![valid];
        for _ in 0..100_000 {
            let _ = read_audio(&noise.garbage(&valid));
        }
    }

    #[test]
    fn playing_waits_for_the_depth_then_follows_the_order() {
        let mut buffer = JitterBuffer::new(2);
        assert_eq!(buffer.pop(), Popped::Empty);
        buffer.push(10, &packet(10));
        assert_eq!(buffer.pop(), Popped::Empty);
        buffer.push(11, &packet(11));
        assert_eq!(played(buffer.pop()), Some(10));
        buffer.push(12, &packet(12));
        assert_eq!(played(buffer.pop()), Some(11));
        assert_eq!(played(buffer.pop()), Some(12));
        assert_eq!(buffer.counters().underruns, 0);
    }

    #[test]
    fn reordered_packets_are_put_back_in_order() {
        let mut buffer = JitterBuffer::new(3);
        for sequence in [5, 3, 4, 7, 6] {
            buffer.push(sequence, &packet(sequence));
        }
        for sequence in 3..=7 {
            assert_eq!(played(buffer.pop()), Some(sequence));
        }
        let counters = buffer.counters();
        assert_eq!((counters.late, counters.missing), (0, 0));
    }

    #[test]
    fn a_gap_is_played_as_missing_and_a_late_packet_is_dropped() {
        let mut buffer = JitterBuffer::new(2);
        for sequence in [1, 2, 4] {
            buffer.push(sequence, &packet(sequence));
        }
        assert_eq!(played(buffer.pop()), Some(1));
        assert_eq!(played(buffer.pop()), Some(2));
        assert_eq!(buffer.pop(), Popped::Missing);
        buffer.push(3, &packet(3));
        assert_eq!(played(buffer.pop()), Some(4));
        let counters = buffer.counters();
        assert_eq!((counters.missing, counters.late), (1, 1));
    }

    #[test]
    fn duplicates_are_played_once() {
        let mut buffer = JitterBuffer::new(2);
        for sequence in [1, 1, 2, 2, 1] {
            buffer.push(sequence, &packet(sequence));
        }
        assert_eq!(played(buffer.pop()), Some(1));
        assert_eq!(played(buffer.pop()), Some(2));
        assert_eq!(buffer.counters().duplicates, 3);
    }

    #[test]
    fn running_dry_starts_over_without_a_gap() {
        let mut buffer = JitterBuffer::new(2);
        for sequence in [1, 2] {
            buffer.push(sequence, &packet(sequence));
        }
        assert_eq!(played(buffer.pop()), Some(1));
        assert_eq!(played(buffer.pop()), Some(2));
        assert_eq!(buffer.pop(), Popped::Empty);
        assert_eq!(buffer.counters().underruns, 1);
        // The host was silent for a while and counts on from elsewhere.
        buffer.push(500, &packet(500));
        assert_eq!(buffer.pop(), Popped::Empty);
        buffer.push(501, &packet(501));
        assert_eq!(played(buffer.pop()), Some(500));
        assert_eq!(buffer.counters().missing, 0);
    }

    #[test]
    fn a_buffer_grown_too_long_drops_back_to_its_depth() {
        let mut buffer = JitterBuffer::new(2);
        for sequence in 0..6 {
            buffer.push(sequence, &packet(sequence));
        }
        assert_eq!(played(buffer.pop()), Some(0));
        for sequence in 6..8 {
            buffer.push(sequence, &packet(sequence));
        }
        // 1 to 7 cannot all be held: the oldest go, the newest two stay.
        assert_eq!(played(buffer.pop()), Some(6));
        assert_eq!(played(buffer.pop()), Some(7));
        assert_eq!(buffer.counters().dropped, 5);
    }

    #[test]
    fn a_jump_far_ahead_keeps_only_what_follows_it() {
        let mut buffer = JitterBuffer::new(2);
        for sequence in [1, 2, 3] {
            buffer.push(sequence, &packet(sequence));
        }
        assert_eq!(played(buffer.pop()), Some(1));
        buffer.push(1_000, &packet(1_000));
        assert_eq!(buffer.pop(), Popped::Missing);
        assert_eq!(played(buffer.pop()), Some(1_000));
        assert_eq!(buffer.counters().dropped, 2);
    }

    #[test]
    fn sequences_wrap_around() {
        let mut buffer = JitterBuffer::new(2);
        for sequence in [u16::MAX - 1, u16::MAX, 0, 1] {
            buffer.push(sequence, &packet(sequence));
        }
        for sequence in [u16::MAX - 1, u16::MAX, 0, 1] {
            assert_eq!(played(buffer.pop()), Some(sequence));
        }
    }

    #[test]
    fn a_packet_older_than_the_first_is_taken_while_filling_up() {
        let mut buffer = JitterBuffer::new(2);
        buffer.push(11, &packet(11));
        buffer.push(10, &packet(10));
        assert_eq!(played(buffer.pop()), Some(10));
        assert_eq!(played(buffer.pop()), Some(11));
        assert_eq!(buffer.counters().late, 0);
    }

    /// A network losing, mixing up and doubling packets, and a sequence
    /// jumping ahead now and then: whatever is played since the buffer
    /// last ran dry is in order, each packet once.
    #[test]
    fn whatever_arrives_packets_leave_in_order_and_once() {
        let mut noise = Noise::new(22);
        let mut buffer = JitterBuffer::new(3);
        let mut sequence = 0u16;
        let mut on_the_way: Vec<u16> = Vec::new();
        let mut last_played: Option<u16> = None;
        let mut played = 0;
        for _ in 0..200_000 {
            match noise.below(200) {
                0 => sequence = sequence.wrapping_add(noise.below(40) as u16),
                1..=100 => {
                    if !noise.percent(5) {
                        on_the_way.push(sequence);
                    }
                    sequence = sequence.wrapping_add(1);
                    if on_the_way.len() > noise.below(4) {
                        let arrives = on_the_way.swap_remove(noise.below(on_the_way.len()));
                        buffer.push(arrives, &packet(arrives));
                        if noise.percent(3) {
                            buffer.push(arrives, &packet(arrives));
                        }
                    }
                }
                _ => match buffer.pop() {
                    Popped::Packet(bytes) => {
                        let now = u16::from_le_bytes([bytes[0], bytes[1]]);
                        if let Some(before) = last_played {
                            let after = now.wrapping_sub(before);
                            assert!(after != 0 && after < 0x8000, "{before} then {now}");
                        }
                        last_played = Some(now);
                        played += 1;
                    }
                    Popped::Missing => {}
                    Popped::Empty => last_played = None,
                },
            }
        }
        assert!(played > 40_000, "{played}");
    }

    #[test]
    fn whatever_arrives_the_buffer_stays_within_its_size() {
        let mut noise = Noise::new(21);
        let mut buffer = JitterBuffer::new(2);
        let mut sequence = 0u16;
        for _ in 0..100_000 {
            match noise.below(10) {
                0 => sequence = noise.next_u64() as u16,
                1..=6 => {
                    let jitter = noise.below(8) as u16;
                    buffer.push(sequence.wrapping_sub(jitter), &packet(sequence));
                    sequence = sequence.wrapping_add(1);
                }
                _ => {
                    buffer.pop();
                }
            }
            assert!(buffer.held <= buffer.slots.len());
            assert_eq!(
                buffer.held,
                buffer.slots.iter().filter(|slot| slot.is_some()).count()
            );
        }
    }
}
