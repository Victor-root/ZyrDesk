//! Putting frames back together on the client.
//!
//! A frame is whole as soon as all its data shards are in, with no
//! arithmetic at all, or failing that as soon as any `data` shards are
//! in, parity included, which Reed-Solomon turns back into the missing
//! data. Frames leave in order. One that cannot be completed is given
//! up, and said to be, once a newer frame has been arriving for
//! `reorder_grace` (a packet late by less than that still counts), or
//! once `max_pending_frames` newer frames are waiting behind it.
//!
//! Everything held is bounded by the limits, whatever arrives: at most
//! `max_pending_frames` frames of at most `max_frame_bytes` each, and
//! never more parity kept for a frame than it has data shards.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use reed_solomon_simd::ReedSolomonDecoder;

use super::{MAX_SHARD_BYTES, VideoHeader, newer_stream};
use crate::codec::VideoCodec;
use crate::wire::WireError;

/// How far ahead of the oldest frame still awaited a packet may be.
///
/// Frames are numbered one after the other, and the tunnel gives up
/// after 30 s of silence, which is fewer frames than this at any rate
/// the engine runs: a packet further ahead comes from a host numbering
/// its frames wrongly, and following it would mean declaring the whole
/// gap lost.
const FARTHEST_AHEAD: u32 = 1 << 15;

/// How many settled frames are remembered, to tell a packet that came
/// after its frame was given up (late) from parity its frame did not
/// need.
const REMEMBERED: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssemblyLimits {
    /// Largest frame accepted.
    pub max_frame_bytes: usize,
    /// Frames waiting at most, the oldest being given up to make room.
    /// At least one.
    pub max_pending_frames: usize,
    /// How long an incomplete frame is waited for once a newer one is
    /// arriving.
    pub reorder_grace: Duration,
}

impl Default for AssemblyLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 16 << 20,
            max_pending_frames: 32,
            reorder_grace: Duration::from_millis(3),
        }
    }
}

/// What became of the packets and frames so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssemblyCounters {
    /// Datagrams handed in, whatever became of them.
    pub packets: u64,
    /// Shards received a second time.
    pub duplicates: u64,
    /// Shards of a frame already given up, or of an older stream.
    pub late: u64,
    /// Shards of a frame already whole: parity it did not need.
    pub unneeded: u64,
    /// Datagrams that are not a video packet, or that contradict the
    /// rest of their frame.
    pub malformed: u64,
    /// Shards of a new frame refused because `poll` was not called since
    /// the last one, with `max_pending_frames` frames already waiting.
    pub overflow: u64,
    /// Frames delivered, repaired or not.
    pub frames_complete: u64,
    /// Among them, frames that needed parity.
    pub frames_recovered_by_fec: u64,
    /// Frames given up.
    pub frames_lost: u64,
    /// Frames of an older stream still waiting when a newer one began.
    pub frames_superseded: u64,
    /// Parity shards that stood in for missing data shards.
    pub parity_used: u64,
}

/// What `poll` hands out, in frame order within a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assembled {
    Frame(AssembledFrame),
    /// A frame that can no longer be completed.
    Lost {
        stream: u16,
        frame: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledFrame {
    pub stream: u16,
    pub frame: u32,
    pub key: bool,
    pub repeat: bool,
    pub codec: VideoCodec,
    /// Exactly the encoded frame. Handing it back with
    /// [`Assembler::recycle`] once decoded saves an allocation.
    pub data: Vec<u8>,
    pub captured_us: u32,
    pub host_latency_us: u32,
    pub first_packet: Instant,
    /// When the packet that made it whole arrived.
    pub last_packet: Instant,
    /// Whether parity had to stand in for missing data.
    pub repaired: bool,
}

/// Rebuilds frames from video datagrams.
///
/// Call [`push`](Self::push) for each datagram, then [`poll`](Self::poll)
/// until it says `None`; when no datagram comes, poll again at
/// [`next_deadline`](Self::next_deadline).
pub struct Assembler {
    limits: AssemblyLimits,
    stream: Option<u16>,
    /// The oldest frame neither delivered nor given up.
    next: u32,
    /// Frames with at least one shard in, in frame order, all at or
    /// after `next`.
    pending: VecDeque<Pending>,
    /// How many frames before `next` are remembered in `recent_lost`.
    settled: u32,
    /// Bit i: whether frame `next - 1 - i` was given up.
    recent_lost: u64,
    /// Kept so that its working space is reused.
    decoder: Option<ReedSolomonDecoder>,
    /// Buffers of settled frames, for the next ones.
    spare: Vec<Parts>,
    counters: AssemblyCounters,
}

impl Assembler {
    pub fn new(limits: AssemblyLimits) -> Self {
        Self {
            limits: AssemblyLimits {
                max_pending_frames: limits.max_pending_frames.max(1),
                ..limits
            },
            stream: None,
            next: 0,
            pending: VecDeque::new(),
            settled: 0,
            recent_lost: 0,
            decoder: None,
            spare: Vec::new(),
            counters: AssemblyCounters::default(),
        }
    }

    /// Takes one datagram in. An error means it was not a video packet
    /// that fits with the others, and it was counted as malformed.
    pub fn push(&mut self, datagram: &[u8], now: Instant) -> Result<(), WireError> {
        self.counters.packets += 1;
        let placed = self.place(datagram, now);
        if placed.is_err() {
            self.counters.malformed += 1;
        }
        placed
    }

    /// The next frame, whole or lost, if one is settled.
    pub fn poll(&mut self, now: Instant) -> Option<Assembled> {
        let stream = self.stream?;
        let (seen, whole) = match self.pending.front() {
            Some(first) if first.header.frame == self.next => (true, first.complete),
            _ => (false, false),
        };
        if whole {
            return self.deliver(stream);
        }
        let crowded = self.pending.len() > self.limits.max_pending_frames;
        let overdue = self
            .newer_since()
            .is_some_and(|since| now.saturating_duration_since(since) >= self.limits.reorder_grace);
        if !crowded && !overdue {
            return None;
        }
        if seen && let Some(given_up) = self.pending.pop_front() {
            self.keep_spare(given_up.parts);
        }
        self.counters.frames_lost += 1;
        let frame = self.next;
        self.settle(true);
        Some(Assembled::Lost { stream, frame })
    }

    /// When `poll` may give a frame up with no new datagram, if ever.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.newer_since()?.checked_add(self.limits.reorder_grace)
    }

    pub fn counters(&self) -> AssemblyCounters {
        self.counters
    }

    /// Takes back the buffer of a delivered frame, to fill with a later
    /// one.
    pub fn recycle(&mut self, buffer: Vec<u8>) {
        if buffer.capacity() > self.limits.max_frame_bytes + MAX_SHARD_BYTES {
            return;
        }
        if let Some(parts) = self.spare.iter_mut().find(|p| p.data.capacity() == 0) {
            parts.data = buffer;
        } else if self.spare.len() <= self.limits.max_pending_frames {
            self.spare.push(Parts {
                data: buffer,
                ..Parts::default()
            });
        }
    }

    fn place(&mut self, datagram: &[u8], now: Instant) -> Result<(), WireError> {
        let (header, shard) = VideoHeader::read(datagram)?;
        if header.size as usize > self.limits.max_frame_bytes {
            return Err(WireError::TooLong);
        }
        let fresh = match self.stream {
            Some(stream) if stream == header.stream => false,
            Some(stream) if !newer_stream(header.stream, stream) => {
                self.counters.late += 1;
                return Ok(());
            }
            _ => true,
        };
        // A new stream counts its frames from zero.
        let ahead = header.frame.wrapping_sub(if fresh { 0 } else { self.next });
        if !fresh && ahead > u32::MAX / 2 {
            self.behind(header.frame);
            return Ok(());
        }
        if ahead > FARTHEST_AHEAD {
            return Err(WireError::Invalid("frame"));
        }
        if fresh {
            self.begin(header.stream);
        }
        let at = match self
            .pending
            .iter()
            .rposition(|p| p.header.frame == header.frame)
        {
            Some(at) if self.pending[at].header.same_frame_as(&header) => at,
            Some(_) => return Err(WireError::Invalid("frame")),
            None if self.pending.len() > self.limits.max_pending_frames => {
                self.counters.overflow += 1;
                return Ok(());
            }
            None => self.open(header, now),
        };
        self.pending[at].take(
            usize::from(header.index),
            shard,
            now,
            &mut self.decoder,
            &mut self.counters,
        );
        Ok(())
    }

    /// Starts over with a newer stream, dropping what the older one left.
    fn begin(&mut self, stream: u16) {
        self.counters.frames_superseded += self.pending.len() as u64;
        while let Some(pending) = self.pending.pop_front() {
            self.keep_spare(pending.parts);
        }
        self.stream = Some(stream);
        self.next = 0;
        self.settled = 0;
        self.recent_lost = 0;
    }

    /// Counts a shard of a frame already settled.
    fn behind(&mut self, frame: u32) {
        let back = self.next.wrapping_sub(frame);
        if back <= self.settled && (self.recent_lost >> (back - 1)) & 1 == 0 {
            self.counters.unneeded += 1;
        } else {
            self.counters.late += 1;
        }
    }

    /// Makes room for a frame seen for the first time, in frame order.
    fn open(&mut self, header: VideoHeader, now: Instant) -> usize {
        let parts = self.spare.pop().unwrap_or_default();
        let ahead = header.frame.wrapping_sub(self.next);
        let at = self
            .pending
            .iter()
            .rposition(|p| p.header.frame.wrapping_sub(self.next) < ahead)
            .map_or(0, |before| before + 1);
        self.pending.insert(at, Pending::new(header, parts, now));
        at
    }

    fn deliver(&mut self, stream: u16) -> Option<Assembled> {
        let mut pending = self.pending.pop_front()?;
        let mut data = std::mem::take(&mut pending.parts.data);
        data.truncate(pending.header.size as usize);
        self.counters.frames_complete += 1;
        if pending.repaired {
            self.counters.frames_recovered_by_fec += 1;
        }
        self.settle(false);
        let header = pending.header;
        let frame = AssembledFrame {
            stream,
            frame: header.frame,
            key: header.key,
            repeat: header.repeat,
            codec: header.codec,
            data,
            captured_us: header.captured_us,
            host_latency_us: header.host_latency_us,
            first_packet: pending.first_packet,
            last_packet: pending.last_packet,
            repaired: pending.repaired,
        };
        self.keep_spare(pending.parts);
        Some(Assembled::Frame(frame))
    }

    fn settle(&mut self, lost: bool) {
        self.next = self.next.wrapping_add(1);
        self.recent_lost = (self.recent_lost << 1) | u64::from(lost);
        self.settled = (self.settled + 1).min(REMEMBERED);
    }

    /// When the first packet of a frame newer than `next` arrived.
    fn newer_since(&self) -> Option<Instant> {
        self.pending
            .iter()
            .filter(|p| p.header.frame != self.next)
            .map(|p| p.first_packet)
            .min()
    }

    fn keep_spare(&mut self, parts: Parts) {
        if self.spare.len() <= self.limits.max_pending_frames {
            self.spare.push(parts);
        }
    }

    /// Bytes held in buffers, waiting frames and spares together.
    #[cfg(test)]
    fn held_bytes(&self) -> usize {
        self.pending
            .iter()
            .map(|p| &p.parts)
            .chain(&self.spare)
            .map(Parts::held_bytes)
            .sum()
    }
}

/// The buffers of one frame, reused from frame to frame.
#[derive(Default)]
struct Parts {
    /// The data shards, end to end, each in its place.
    data: Vec<u8>,
    /// One bit per shard received, parity included.
    received: Vec<u64>,
    /// Parity shards in the order they arrived.
    parity: Vec<u8>,
    /// Their shard indices.
    parity_at: Vec<u16>,
}

impl Parts {
    #[cfg(test)]
    fn held_bytes(&self) -> usize {
        self.data.capacity()
            + self.parity.capacity()
            + self.received.capacity() * 8
            + self.parity_at.capacity() * 2
    }
}

/// A frame with at least one shard in.
struct Pending {
    /// As its first packet described it, which the others must repeat.
    header: VideoHeader,
    parts: Parts,
    data_count: usize,
    first_packet: Instant,
    last_packet: Instant,
    complete: bool,
    repaired: bool,
}

impl Pending {
    fn new(header: VideoHeader, mut parts: Parts, now: Instant) -> Self {
        let (data, parity, shard) = geometry(&header);
        parts.data.clear();
        parts.data.reserve_exact(data * shard);
        parts.data.resize(data * shard, 0);
        parts.received.clear();
        parts.received.resize((data + parity).div_ceil(64), 0);
        parts.parity.clear();
        parts.parity_at.clear();
        Self {
            header,
            parts,
            data_count: 0,
            first_packet: now,
            last_packet: now,
            complete: false,
            repaired: false,
        }
    }

    fn take(
        &mut self,
        index: usize,
        shard: &[u8],
        now: Instant,
        decoder: &mut Option<ReedSolomonDecoder>,
        counters: &mut AssemblyCounters,
    ) {
        let (word, bit) = (index / 64, 1u64 << (index % 64));
        if self.parts.received[word] & bit != 0 {
            counters.duplicates += 1;
            return;
        }
        self.parts.received[word] |= bit;
        let (data, parity, size) = geometry(&self.header);
        let enough = self.data_count + self.parts.parity_at.len() >= data;
        if self.complete || (index >= data && enough) {
            counters.unneeded += 1;
            return;
        }
        if index < data {
            self.parts.data[index * size..(index + 1) * size].copy_from_slice(shard);
            self.data_count += 1;
        } else {
            if self.parts.parity_at.is_empty() {
                self.parts.parity.reserve_exact(parity.min(data) * size);
            }
            self.parts.parity.extend_from_slice(shard);
            self.parts.parity_at.push(index as u16);
        }
        self.last_packet = now;
        if self.data_count == data {
            self.complete = true;
        } else if self.data_count + self.parts.parity_at.len() >= data {
            // The code accepts every split a valid header describes, so
            // this does not fail; if it ever did, the frame would stay
            // incomplete and be given up like any other.
            if let Ok(used) = self.repair(decoder) {
                self.complete = true;
                self.repaired = true;
                counters.parity_used += used as u64;
            }
        }
    }

    /// Rebuilds the missing data shards from the parity received.
    fn repair(
        &mut self,
        decoder: &mut Option<ReedSolomonDecoder>,
    ) -> Result<usize, reed_solomon_simd::Error> {
        let (data, parity, size) = geometry(&self.header);
        let reused = match decoder.take() {
            Some(mut reused) => {
                reused.reset(data, parity, size)?;
                reused
            }
            None => ReedSolomonDecoder::new(data, parity, size)?,
        };
        let decoder = decoder.insert(reused);
        for index in 0..data {
            if self.parts.received[index / 64] & (1 << (index % 64)) != 0 {
                decoder.add_original_shard(
                    index,
                    &self.parts.data[index * size..(index + 1) * size],
                )?;
            }
        }
        for (slot, index) in self.parts.parity_at.iter().enumerate() {
            let shard = &self.parts.parity[slot * size..(slot + 1) * size];
            decoder.add_recovery_shard(usize::from(*index) - data, shard)?;
        }
        let restored = decoder.decode()?;
        for (index, shard) in restored.restored_original_iter() {
            self.parts.data[index * size..(index + 1) * size].copy_from_slice(shard);
        }
        Ok(data - self.data_count)
    }
}

/// Data shards, parity shards, and shard size of a frame.
fn geometry(header: &VideoHeader) -> (usize, usize, usize) {
    (
        usize::from(header.data),
        usize::from(header.parity),
        usize::from(header.shard),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Noise;
    use crate::video::{MAX_SHARDS, OutgoingFrame, Packetizer, Packets, VIDEO_HEADER};

    const BUDGET: usize = 1161;
    const GRACE: Duration = Duration::from_millis(3);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    struct Host {
        packetizer: Packetizer,
        stream: u16,
        frame: u32,
    }

    impl Host {
        fn new(fec_percent: u8) -> Self {
            Self {
                packetizer: Packetizer::new(BUDGET, fec_percent),
                stream: 1,
                frame: 0,
            }
        }

        /// The datagrams of the next frame.
        fn send(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
            let mut packets = Packets::new();
            self.packetizer
                .packetize(
                    &OutgoingFrame {
                        data,
                        key: self.frame == 0,
                        repeat: false,
                        stream: self.stream,
                        frame: self.frame,
                        captured_us: self.frame.wrapping_mul(16_667),
                        host_latency_us: 2_500,
                        codec: VideoCodec::Hevc,
                    },
                    &mut packets,
                )
                .unwrap();
            self.frame = self.frame.wrapping_add(1);
            packets.iter().map(<[u8]>::to_vec).collect()
        }
    }

    fn drain(assembler: &mut Assembler, now: Instant) -> Vec<Assembled> {
        std::iter::from_fn(|| assembler.poll(now)).collect()
    }

    fn frame_of(assembled: &Assembled) -> &AssembledFrame {
        match assembled {
            Assembled::Frame(frame) => frame,
            Assembled::Lost { frame, .. } => panic!("frame {frame} lost"),
        }
    }

    fn parity_of(datagram: &[u8]) -> usize {
        usize::from(VideoHeader::read(datagram).unwrap().0.parity)
    }

    #[test]
    fn frames_arrive_whole_and_in_order() {
        let mut noise = Noise::new(1);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        let sizes = [1, 2, 1132, 1133, 1132 * 3, 50_000, 1 << 20, 5];
        for (n, size) in sizes.into_iter().enumerate() {
            let data = noise.bytes(size);
            let now = at + ms(n as u64 * 16);
            for datagram in host.send(&data) {
                assembler.push(&datagram, now).unwrap();
            }
            let out = drain(&mut assembler, now);
            assert_eq!(out.len(), 1, "size {size}");
            let frame = frame_of(&out[0]);
            assert_eq!(frame.data, data);
            assert_eq!(frame.frame, n as u32);
            assert_eq!(frame.stream, 1);
            assert_eq!(frame.key, n == 0);
            assert_eq!(frame.codec, VideoCodec::Hevc);
            assert_eq!(frame.host_latency_us, 2_500);
            assert_eq!(frame.captured_us, n as u32 * 16_667);
            assert!(!frame.repaired);
        }
        let counters = assembler.counters();
        assert_eq!(counters.frames_complete, sizes.len() as u64);
        assert_eq!(counters.frames_lost, 0);
        assert_eq!(counters.malformed, 0);
        // The parity came after the frame was whole.
        assert!(counters.unneeded > 0);
        assert_eq!(counters.late, 0);
    }

    #[test]
    fn any_loss_within_the_parity_is_repaired() {
        let mut noise = Noise::new(2);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        let mut data_lost = 0;
        for n in 0..300u64 {
            let size = 1 + noise.below(200_000);
            let data = noise.bytes(size);
            let mut datagrams = host.send(&data);
            let parity = parity_of(&datagrams[0]);
            let data_shards = datagrams.len() - parity;
            // Loses up to the parity, anywhere, and mixes up the rest.
            let lost = noise.below(parity + 1);
            for _ in 0..lost {
                let at = noise.below(datagrams.len());
                let gone = datagrams.remove(at);
                if (VideoHeader::read(&gone).unwrap().0.index as usize) < data_shards {
                    data_lost += 1;
                }
            }
            for i in (1..datagrams.len()).rev() {
                datagrams.swap(i, noise.below(i + 1));
            }
            let now = at + ms(n * 16);
            for datagram in &datagrams {
                assembler.push(datagram, now).unwrap();
            }
            let out = drain(&mut assembler, now);
            assert_eq!(out.len(), 1, "frame {n}");
            assert_eq!(frame_of(&out[0]).data, data, "frame {n}");
        }
        let counters = assembler.counters();
        assert_eq!(counters.frames_complete, 300);
        assert_eq!(counters.frames_lost, 0);
        assert!(data_lost > 0);
        assert!(counters.frames_recovered_by_fec > 0);
        assert!(counters.parity_used >= counters.frames_recovered_by_fec);
    }

    #[test]
    fn a_frame_losing_more_than_its_parity_is_lost_exactly_once() {
        let mut noise = Noise::new(3);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();

        let first = host.send(&noise.bytes(20_000));
        let parity = parity_of(&first[0]);
        let (sent, held_back) = first.split_at(first.len() - parity - 1);
        for datagram in sent {
            assembler.push(datagram, at).unwrap();
        }
        assert!(drain(&mut assembler, at).is_empty());
        assert_eq!(assembler.next_deadline(), None);

        let second_data = noise.bytes(3_000);
        let second_at = at + ms(16);
        for datagram in host.send(&second_data) {
            assembler.push(&datagram, second_at).unwrap();
        }
        assert!(drain(&mut assembler, second_at + GRACE - ms(1)).is_empty());
        assert_eq!(assembler.next_deadline(), Some(second_at + GRACE));

        let out = drain(&mut assembler, second_at + GRACE);
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0],
            Assembled::Lost {
                stream: 1,
                frame: 0
            }
        );
        assert_eq!(frame_of(&out[1]).data, second_data);

        // What was missing turns up after all: too late, and never a
        // second loss.
        for datagram in held_back {
            assembler.push(datagram, second_at + ms(5)).unwrap();
        }
        assert!(drain(&mut assembler, second_at + ms(100)).is_empty());
        let counters = assembler.counters();
        assert_eq!(counters.frames_lost, 1);
        assert_eq!(counters.late, held_back.len() as u64);
        assert_eq!(assembler.next_deadline(), None);
    }

    #[test]
    fn packets_reordered_within_the_grace_are_waited_for() {
        let mut noise = Noise::new(4);
        let mut host = Host::new(0);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        let (a, b) = (noise.bytes(10_000), noise.bytes(10_000));
        let (first, second) = (host.send(&a), host.send(&b));

        for datagram in &first[..first.len() - 1] {
            assembler.push(datagram, at).unwrap();
        }
        for datagram in &second {
            assembler.push(datagram, at + ms(1)).unwrap();
        }
        assert!(drain(&mut assembler, at + ms(2)).is_empty());
        assembler.push(first.last().unwrap(), at + ms(3)).unwrap();
        let out = drain(&mut assembler, at + ms(3));
        assert_eq!(out.len(), 2);
        assert_eq!(frame_of(&out[0]).data, a);
        assert_eq!(frame_of(&out[1]).data, b);
        assert_eq!(assembler.counters().frames_lost, 0);
    }

    #[test]
    fn duplicates_are_ignored_and_counted() {
        let mut noise = Noise::new(5);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        let data = noise.bytes(30_000);
        let datagrams = host.send(&data);
        let data_shards = datagrams.len() - parity_of(&datagrams[0]);
        for datagram in &datagrams[..data_shards - 1] {
            assembler.push(datagram, at).unwrap();
            assembler.push(datagram, at).unwrap();
        }
        assert!(drain(&mut assembler, at).is_empty());
        assembler.push(&datagrams[data_shards - 1], at).unwrap();
        let out = drain(&mut assembler, at);
        assert_eq!(out.len(), 1);
        assert_eq!(frame_of(&out[0]).data, data);
        for datagram in &datagrams {
            assembler.push(datagram, at).unwrap();
        }
        assert!(drain(&mut assembler, at + ms(100)).is_empty());
        let counters = assembler.counters();
        assert_eq!(counters.duplicates, data_shards as u64 - 1);
        assert_eq!(counters.unneeded, datagrams.len() as u64);
        assert_eq!(counters.frames_complete, 1);
    }

    #[test]
    fn a_newer_stream_drops_what_the_older_one_left() {
        let mut noise = Noise::new(6);
        let mut host = Host::new(0);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        host.send(&[1]);
        let old = host.send(&noise.bytes(5_000));
        for datagram in &old[1..] {
            assembler.push(datagram, at).unwrap();
        }

        host.stream = 2;
        host.frame = 0;
        let fresh = noise.bytes(4_000);
        for datagram in host.send(&fresh) {
            assembler.push(&datagram, at + ms(1)).unwrap();
        }
        let out = drain(&mut assembler, at + ms(50));
        assert_eq!(out.len(), 1);
        let frame = frame_of(&out[0]);
        assert_eq!((frame.stream, frame.frame), (2, 0));
        assert_eq!(frame.data, fresh);

        // What the old stream still sends is late, and changes nothing.
        assembler.push(&old[0], at + ms(2)).unwrap();
        assert!(drain(&mut assembler, at + ms(50)).is_empty());
        let counters = assembler.counters();
        assert_eq!(counters.frames_superseded, 1);
        assert_eq!(counters.late, 1);
        assert_eq!(counters.frames_lost, 0);
    }

    #[test]
    fn frame_numbers_wrap_around() {
        let mut noise = Noise::new(7);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        assembler.stream = Some(1);
        assembler.next = u32::MAX - 2;
        host.frame = u32::MAX - 2;
        let mut expected = Vec::new();
        for n in 0..6u64 {
            let data = noise.bytes(3_000);
            let datagrams = host.send(&data);
            let now = at + ms(n * 16);
            if n == 3 {
                // Frame u32::MAX + 1 = 0 goes missing entirely.
                continue;
            }
            for datagram in &datagrams {
                assembler.push(datagram, now).unwrap();
            }
            expected.push(data);
        }
        let out = drain(&mut assembler, at + ms(200));
        let numbers: Vec<(u32, bool)> = out
            .iter()
            .map(|a| match a {
                Assembled::Frame(frame) => (frame.frame, true),
                Assembled::Lost { frame, .. } => (*frame, false),
            })
            .collect();
        assert_eq!(
            numbers,
            vec![
                (u32::MAX - 2, true),
                (u32::MAX - 1, true),
                (u32::MAX, true),
                (0, false),
                (1, true),
                (2, true),
            ]
        );
    }

    #[test]
    fn frames_missing_entirely_are_each_reported() {
        let mut noise = Noise::new(8);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        for datagram in host.send(&noise.bytes(100)) {
            assembler.push(&datagram, at).unwrap();
        }
        host.send(&[1]);
        host.send(&[2]);
        for datagram in host.send(&noise.bytes(100)) {
            assembler.push(&datagram, at + ms(48)).unwrap();
        }
        let out = drain(&mut assembler, at + ms(48) + GRACE);
        assert_eq!(out.len(), 4);
        assert_eq!(frame_of(&out[0]).frame, 0);
        assert_eq!(
            out[1],
            Assembled::Lost {
                stream: 1,
                frame: 1
            }
        );
        assert_eq!(
            out[2],
            Assembled::Lost {
                stream: 1,
                frame: 2
            }
        );
        assert_eq!(frame_of(&out[3]).frame, 3);
    }

    #[test]
    fn too_many_newer_frames_give_the_oldest_up_at_once() {
        let mut noise = Noise::new(9);
        let mut host = Host::new(0);
        let limits = AssemblyLimits {
            max_pending_frames: 4,
            ..AssemblyLimits::default()
        };
        let mut assembler = Assembler::new(limits);
        let at = Instant::now();
        let first = host.send(&noise.bytes(5_000));
        assembler.push(&first[0], at).unwrap();
        for _ in 0..3 {
            for datagram in host.send(&noise.bytes(100)) {
                assembler.push(&datagram, at).unwrap();
                assert!(drain(&mut assembler, at).is_empty());
            }
        }
        // The fourth newer frame waiting pushes frame 0 out, with no
        // time passing at all.
        for datagram in host.send(&noise.bytes(100)) {
            assembler.push(&datagram, at).unwrap();
        }
        let out = drain(&mut assembler, at);
        assert_eq!(out.len(), 5);
        assert_eq!(
            out[0],
            Assembled::Lost {
                stream: 1,
                frame: 0
            }
        );
        for (n, assembled) in out[1..].iter().enumerate() {
            assert_eq!(frame_of(assembled).frame, n as u32 + 1);
        }
    }

    #[test]
    fn a_packet_that_contradicts_its_frame_is_refused() {
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits {
            max_frame_bytes: 10_000,
            ..AssemblyLimits::default()
        });
        let at = Instant::now();
        let datagrams = host.send(&[7u8; 5_000]);
        assembler.push(&datagrams[0], at).unwrap();

        let mut contradicting = datagrams[1].clone();
        contradicting[1] ^= 1;
        assert_eq!(
            assembler.push(&contradicting, at),
            Err(WireError::Invalid("frame"))
        );
        host.frame = 0;
        assert_eq!(
            assembler.push(&host.send(&[7u8; 10_001])[0], at),
            Err(WireError::TooLong)
        );
        host.frame = FARTHEST_AHEAD + 1;
        assert_eq!(
            assembler.push(&host.send(&[7u8; 100])[0], at),
            Err(WireError::Invalid("frame"))
        );
        assert_eq!(assembler.push(&[1, 2, 3], at), Err(WireError::Truncated));
        assert_eq!(assembler.counters().malformed, 4);
        assert_eq!(assembler.counters().packets, 5);
    }

    #[test]
    fn without_parity_any_missing_packet_loses_the_frame() {
        let mut noise = Noise::new(10);
        let mut host = Host::new(0);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        let first = host.send(&noise.bytes(5_000));
        assembler.push(&first[0], at).unwrap();
        for datagram in host.send(&[1]) {
            assembler.push(&datagram, at).unwrap();
        }
        let out = drain(&mut assembler, at + GRACE);
        assert_eq!(
            out[0],
            Assembled::Lost {
                stream: 1,
                frame: 0
            }
        );
        assert_eq!(frame_of(&out[1]).data, vec![1]);
    }

    #[test]
    fn a_recycled_buffer_carries_the_next_frame() {
        let mut noise = Noise::new(11);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        for datagram in host.send(&noise.bytes(40_000)) {
            assembler.push(&datagram, at).unwrap();
        }
        let first = drain(&mut assembler, at).remove(0);
        let Assembled::Frame(first) = first else {
            panic!("frame 0 lost");
        };
        let buffer = first.data.as_ptr();
        assembler.recycle(first.data);
        for datagram in host.send(&noise.bytes(30_000)) {
            assembler.push(&datagram, at).unwrap();
        }
        let second = drain(&mut assembler, at);
        assert_eq!(frame_of(&second[0]).data.as_ptr(), buffer);
    }

    #[test]
    fn garbage_never_panics() {
        let mut noise = Noise::new(12);
        let mut host = Host::new(20);
        let valid = host.send(&[9u8; 5_000]);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let at = Instant::now();
        for n in 0..50_000u64 {
            let bytes = noise.garbage(&valid);
            let now = at + Duration::from_micros(n * 10);
            let _ = assembler.push(&bytes, now);
            drain(&mut assembler, now);
        }
    }

    /// A host sending whatever headers it likes, all well formed: frames
    /// all over the window, streams changing, sizes up to the limit, and
    /// frames that never complete.
    #[test]
    fn memory_stays_bounded_whatever_arrives() {
        let mut noise = Noise::new(13);
        let limits = AssemblyLimits {
            max_frame_bytes: 64 << 10,
            max_pending_frames: 8,
            reorder_grace: GRACE,
        };
        let mut assembler = Assembler::new(limits);
        let per_frame = 2 * (limits.max_frame_bytes + MAX_SHARD_BYTES) + 8 * 1024 + 4 * 32_768;
        let bound = 2 * (limits.max_pending_frames + 1) * per_frame;
        let at = Instant::now();
        let mut stream = 0u16;
        for n in 0..20_000u64 {
            if noise.percent(1) {
                stream = stream.wrapping_add(1);
            }
            let shard = 2 * (1 + noise.below(4_000));
            let data = 1 + noise.below(limits.max_frame_bytes / shard);
            let parity = noise.below((3 * data).min(MAX_SHARDS - data) + 1);
            let size = (data - 1) * shard + 1 + noise.below(shard);
            let header = VideoHeader {
                key: noise.percent(10),
                repeat: false,
                stream: stream.wrapping_sub(u16::from(noise.percent(1))),
                frame: assembler.next.wrapping_add(noise.below(48) as u32),
                index: noise.below(data + parity) as u16,
                data: data as u16,
                parity: parity as u16,
                shard: shard as u16,
                size: size as u32,
                captured_us: 0,
                host_latency_us: 0,
                codec: VideoCodec::H264,
            };
            let mut head = [0u8; VIDEO_HEADER];
            header.write(&mut head);
            let mut datagram = head.to_vec();
            datagram.resize(VIDEO_HEADER + shard, 1);
            let now = at + Duration::from_micros(n * 100);
            let _ = assembler.push(&datagram, now);
            drain(&mut assembler, now);
            assert!(assembler.pending.len() <= limits.max_pending_frames);
            assert!(
                assembler.held_bytes() <= bound,
                "{}",
                assembler.held_bytes()
            );
        }
        let counters = assembler.counters();
        assert_eq!(counters.overflow, 0);
        assert!(counters.frames_lost > 0 && counters.frames_superseded > 0);
    }

    #[test]
    fn a_crowd_of_frames_without_polling_is_refused_not_kept() {
        let mut host = Host::new(0);
        let limits = AssemblyLimits {
            max_pending_frames: 2,
            ..AssemblyLimits::default()
        };
        let mut assembler = Assembler::new(limits);
        let at = Instant::now();
        for _ in 0..10 {
            let datagrams = host.send(&[1u8; 3_000]);
            assembler.push(&datagrams[0], at).unwrap();
        }
        assert_eq!(assembler.pending.len(), 3);
        assert_eq!(assembler.counters().overflow, 7);
    }

    /// Prints how fast frames go through packetizer and assembler, parity
    /// included, for a stream of 80 Mb/s at 60 frames a second.
    #[test]
    fn throughput_is_measured() {
        let mut noise = Noise::new(14);
        let mut host = Host::new(20);
        let mut assembler = Assembler::new(AssemblyLimits::default());
        let data = noise.bytes(80_000_000 / 8 / 60);
        let frames = 120u64;
        for (lose, label) in [(false, "whole"), (true, "one packet lost per frame")] {
            let started = Instant::now();
            let mut packets = Packets::new();
            for n in 0..frames {
                host.packetizer
                    .packetize(
                        &OutgoingFrame {
                            data: &data,
                            key: false,
                            repeat: false,
                            stream: host.stream,
                            frame: host.frame,
                            captured_us: 0,
                            host_latency_us: 0,
                            codec: VideoCodec::H264,
                        },
                        &mut packets,
                    )
                    .unwrap();
                host.frame += 1;
                let now = started + ms(n * 16);
                for (index, datagram) in packets.iter().enumerate() {
                    if !(lose && index == 3) {
                        assembler.push(datagram, now).unwrap();
                    }
                }
                let out = drain(&mut assembler, now);
                assert_eq!(out.len(), 1);
                if let Assembled::Frame(frame) = out.into_iter().next().unwrap() {
                    assembler.recycle(frame.data);
                }
            }
            let seconds = started.elapsed().as_secs_f64();
            let megabytes = (frames as usize * data.len()) as f64 / 1e6;
            println!("video, {label}: {:.0} MB/s", megabytes / seconds);
        }
        assert_eq!(assembler.counters().frames_lost, 0);
    }
}
