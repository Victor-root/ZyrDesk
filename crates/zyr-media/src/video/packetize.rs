//! Cutting a frame into datagrams, with the parity that repairs them.
//!
//! The shard size follows the frame: a frame that fits in one datagram
//! goes out in one datagram of its own size, and a large one is spread
//! evenly over as few full datagrams as it needs, so that no packet is
//! sent mostly empty.

use std::fmt;

use reed_solomon_simd::ReedSolomonEncoder;

use super::{INDEX_AT, MAX_SHARD_BYTES, MAX_SHARDS, VIDEO_HEADER, VideoHeader};
use crate::codec::VideoCodec;

/// Parity added to each frame when nothing else is decided, in percent
/// of its data shards.
pub const DEFAULT_FEC_PERCENT: u8 = 20;

/// One encoded frame, as the encoder hands it over.
#[derive(Debug, Clone, Copy)]
pub struct OutgoingFrame<'a> {
    pub data: &'a [u8],
    pub key: bool,
    pub repeat: bool,
    pub stream: u16,
    pub frame: u32,
    pub captured_us: u32,
    pub host_latency_us: u32,
    pub codec: VideoCodec,
}

/// Why a frame could not be cut into datagrams.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketizeError {
    /// An encoder handed over nothing.
    Empty,
    /// More than the header can describe with datagrams of this budget.
    TooLarge { size: usize, most: usize },
    /// The parity could not be computed.
    Correction(reed_solomon_simd::Error),
}

impl fmt::Display for PacketizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketizeError::Empty => write!(f, "image vide"),
            PacketizeError::TooLarge { size, most } => {
                write!(f, "image trop grosse : {size} octets pour {most} au plus")
            }
            PacketizeError::Correction(e) => write!(f, "correction d'erreurs impossible : {e}"),
        }
    }
}

impl std::error::Error for PacketizeError {}

impl From<reed_solomon_simd::Error> for PacketizeError {
    fn from(e: reed_solomon_simd::Error) -> Self {
        PacketizeError::Correction(e)
    }
}

/// The datagrams of one frame, all of the same length, data shards
/// first and parity after.
///
/// They sit end to end in a single buffer, reused from one frame to the
/// next: once it has grown to the largest frame, cutting a frame
/// allocates nothing, and handing a frame to another thread moves one
/// allocation.
#[derive(Debug, Clone, Default)]
pub struct Packets {
    bytes: Vec<u8>,
    each: usize,
    count: usize,
}

impl Packets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Each datagram, in the order they are best sent.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &[u8]> + DoubleEndedIterator {
        (0..self.count).map(move |at| &self.bytes[at * self.each..(at + 1) * self.each])
    }
}

/// Cuts frames into datagrams for one path.
pub struct Packetizer {
    largest_shard: usize,
    fec_percent: u8,
    /// Kept from frame to frame so that its working space is reused.
    encoder: Option<ReedSolomonEncoder>,
}

impl Packetizer {
    /// A packetizer for datagrams of at most `datagram_budget` bytes,
    /// adding `fec_percent` of parity.
    ///
    /// A budget with no room for a shard after the header leaves every
    /// frame too large, which is what it is.
    pub fn new(datagram_budget: usize, fec_percent: u8) -> Self {
        let room = datagram_budget
            .saturating_sub(VIDEO_HEADER)
            .min(MAX_SHARD_BYTES);
        Self {
            largest_shard: room - room % 2,
            fec_percent,
            encoder: None,
        }
    }

    /// Cuts one frame into `out`, replacing what it held.
    pub fn packetize(
        &mut self,
        frame: &OutgoingFrame<'_>,
        out: &mut Packets,
    ) -> Result<(), PacketizeError> {
        out.bytes.clear();
        out.count = 0;
        let size = frame.data.len();
        if size == 0 {
            return Err(PacketizeError::Empty);
        }
        let most = self.largest_shard * MAX_SHARDS;
        if size > most {
            return Err(PacketizeError::TooLarge { size, most });
        }
        let data = size.div_ceil(self.largest_shard);
        let shard = size.div_ceil(data).next_multiple_of(2);
        let parity = self.parity_for(data);
        let each = VIDEO_HEADER + shard;

        // Every count fits its field: at most MAX_SHARDS shards of at most
        // MAX_SHARD_BYTES, which bounds the size as well.
        let mut head = [0u8; VIDEO_HEADER];
        VideoHeader {
            key: frame.key,
            repeat: frame.repeat,
            stream: frame.stream,
            frame: frame.frame,
            index: 0,
            data: data as u16,
            parity: parity as u16,
            shard: shard as u16,
            size: size as u32,
            captured_us: frame.captured_us,
            host_latency_us: frame.host_latency_us,
            codec: frame.codec,
        }
        .write(&mut head);

        out.bytes.reserve((data + parity) * each);
        for (index, piece) in frame.data.chunks(shard).enumerate() {
            put_packet(&mut out.bytes, &mut head, index, piece);
        }
        // Pads the last data shard.
        out.bytes.resize(data * each, 0);

        if parity > 0 {
            let encoder = match self.encoder.take() {
                Some(mut encoder) => {
                    encoder.reset(data, parity, shard)?;
                    encoder
                }
                None => ReedSolomonEncoder::new(data, parity, shard)?,
            };
            let encoder = self.encoder.insert(encoder);
            for index in 0..data {
                let at = index * each + VIDEO_HEADER;
                encoder.add_original_shard(&out.bytes[at..at + shard])?;
            }
            let parities = encoder.encode()?;
            for (offset, piece) in parities.recovery_iter().enumerate() {
                put_packet(&mut out.bytes, &mut head, data + offset, piece);
            }
        }
        out.each = each;
        out.count = data + parity;
        Ok(())
    }

    /// Parity shards for a frame of `data` shards: the percentage asked,
    /// rounded up, at least one when there is any, and never more than
    /// the header can number.
    fn parity_for(&self, data: usize) -> usize {
        if self.fec_percent == 0 {
            return 0;
        }
        let asked = (data * usize::from(self.fec_percent)).div_ceil(100).max(1);
        asked.min(MAX_SHARDS - data)
    }
}

fn put_packet(bytes: &mut Vec<u8>, head: &mut [u8; VIDEO_HEADER], index: usize, shard: &[u8]) {
    head[INDEX_AT..INDEX_AT + 2].copy_from_slice(&(index as u16).to_le_bytes());
    bytes.extend_from_slice(head);
    bytes.extend_from_slice(shard);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the tunnel carries today after its channel byte.
    const BUDGET: usize = 1161;
    const SHARD: usize = 1132;

    fn frame(data: &[u8]) -> OutgoingFrame<'_> {
        OutgoingFrame {
            data,
            key: false,
            repeat: false,
            stream: 1,
            frame: 9,
            captured_us: 42,
            host_latency_us: 1_000,
            codec: VideoCodec::H264,
        }
    }

    /// Cuts a frame and says how: (data, parity, shard) as each header
    /// has it, after checking every header agrees and is numbered in
    /// order.
    fn cut(packetizer: &mut Packetizer, data: &[u8]) -> (usize, usize, usize, Packets) {
        let mut packets = Packets::new();
        packetizer.packetize(&frame(data), &mut packets).unwrap();
        let headers: Vec<VideoHeader> = packets
            .iter()
            .map(|p| VideoHeader::read(p).unwrap().0)
            .collect();
        let first = headers[0];
        for (index, header) in headers.iter().enumerate() {
            assert_eq!(usize::from(header.index), index);
            assert_eq!(
                VideoHeader {
                    index: 0,
                    ..*header
                },
                first
            );
        }
        assert_eq!(first.size as usize, data.len());
        let (k, m, s) = (
            usize::from(first.data),
            usize::from(first.parity),
            usize::from(first.shard),
        );
        assert_eq!(packets.len(), k + m);
        (k, m, s, packets)
    }

    #[test]
    fn a_one_byte_frame_takes_one_small_packet_and_its_parity() {
        let mut packetizer = Packetizer::new(BUDGET, DEFAULT_FEC_PERCENT);
        let (k, m, s, packets) = cut(&mut packetizer, &[7]);
        assert_eq!((k, m, s), (1, 1, 2));
        let first = packets.iter().next().unwrap();
        assert_eq!(first.len(), VIDEO_HEADER + 2);
        assert_eq!(&first[VIDEO_HEADER..], &[7, 0]);
    }

    #[test]
    fn exact_multiples_fill_every_shard() {
        let mut packetizer = Packetizer::new(BUDGET, DEFAULT_FEC_PERCENT);
        let data: Vec<u8> = (0..SHARD * 3).map(|i| i as u8).collect();
        let (k, m, s, packets) = cut(&mut packetizer, &data);
        assert_eq!((k, m, s), (3, 1, SHARD));
        for (index, packet) in packets.iter().take(k).enumerate() {
            assert_eq!(
                &packet[VIDEO_HEADER..],
                &data[index * SHARD..(index + 1) * SHARD]
            );
        }

        // One byte more spreads evenly over one more packet.
        let data = vec![1u8; SHARD * 3 + 1];
        let (k, m, s, _) = cut(&mut packetizer, &data);
        assert_eq!((k, m, s), (4, 1, 850));
    }

    #[test]
    fn shards_are_even_and_never_larger_than_the_budget() {
        let mut packetizer = Packetizer::new(BUDGET, DEFAULT_FEC_PERCENT);
        for size in [2, 3, 1131, 1132, 1133, 2265, 5000, 99_999] {
            let (k, _, s, packets) = cut(&mut packetizer, &vec![5u8; size]);
            assert_eq!(s % 2, 0, "{size}");
            assert!(s <= SHARD, "{size}");
            assert!((k - 1) * s < size && size <= k * s, "{size}");
            assert!(packets.iter().all(|p| p.len() <= BUDGET), "{size}");
        }
    }

    #[test]
    fn a_large_key_frame_is_cut_with_its_share_of_parity() {
        let mut packetizer = Packetizer::new(BUDGET, DEFAULT_FEC_PERCENT);
        let data = vec![3u8; 1 << 20];
        let (k, m, s, _) = cut(&mut packetizer, &data);
        assert_eq!((k, m, s), (927, 186, SHARD));

        let data = vec![3u8; 4 << 20];
        let (k, m, _, _) = cut(&mut packetizer, &data);
        assert_eq!(k, (4usize << 20).div_ceil(SHARD));
        assert_eq!(m, (k * 20).div_ceil(100));
    }

    #[test]
    fn the_parity_follows_the_percentage_asked() {
        let data = vec![1u8; SHARD * 10];
        let parity = |percent| cut(&mut Packetizer::new(BUDGET, percent), &data).1;
        assert_eq!(parity(0), 0);
        assert_eq!(parity(1), 1);
        assert_eq!(parity(20), 2);
        assert_eq!(parity(21), 3);
        assert_eq!(parity(100), 10);
        assert_eq!(parity(255), 26);
    }

    #[test]
    fn the_shard_count_never_exceeds_what_the_header_numbers() {
        // A budget of one 2-byte shard keeps these frames small.
        let mut packetizer = Packetizer::new(VIDEO_HEADER + 2, DEFAULT_FEC_PERCENT);
        let (k, m, _, _) = cut(&mut packetizer, &vec![1u8; 60_000]);
        assert_eq!((k, m), (30_000, 2_768));
        let (k, m, _, _) = cut(&mut packetizer, &vec![1u8; 65_536]);
        assert_eq!((k, m), (32_768, 0));
        let mut packets = Packets::new();
        assert_eq!(
            packetizer.packetize(&frame(&vec![1u8; 65_537]), &mut packets),
            Err(PacketizeError::TooLarge {
                size: 65_537,
                most: 65_536
            })
        );
        assert!(packets.is_empty());
    }

    #[test]
    fn nothing_or_no_room_is_refused() {
        let mut packets = Packets::new();
        assert_eq!(
            Packetizer::new(BUDGET, 20).packetize(&frame(&[]), &mut packets),
            Err(PacketizeError::Empty)
        );
        assert_eq!(
            Packetizer::new(VIDEO_HEADER + 1, 20).packetize(&frame(&[1]), &mut packets),
            Err(PacketizeError::TooLarge { size: 1, most: 0 })
        );
    }

    #[test]
    fn a_huge_budget_stays_within_what_a_header_describes() {
        let mut packetizer = Packetizer::new(1 << 20, 0);
        let (k, _, s, _) = cut(&mut packetizer, &vec![1u8; 200_000]);
        assert_eq!((k, s), (4, 50_000));
        let (_, _, s, _) = cut(&mut packetizer, &vec![1u8; 70_000]);
        assert_eq!(s, 35_000);
        assert!(packetizer.largest_shard <= MAX_SHARD_BYTES);
    }

    #[test]
    fn the_buffer_is_reused_from_frame_to_frame() {
        let mut packetizer = Packetizer::new(BUDGET, DEFAULT_FEC_PERCENT);
        let mut packets = Packets::new();
        let data = vec![9u8; 100_000];
        packetizer.packetize(&frame(&data), &mut packets).unwrap();
        let (at, capacity) = (packets.bytes.as_ptr(), packets.bytes.capacity());
        packetizer
            .packetize(&frame(&data[..50_000]), &mut packets)
            .unwrap();
        packetizer.packetize(&frame(&data), &mut packets).unwrap();
        assert_eq!(packets.bytes.as_ptr(), at);
        assert_eq!(packets.bytes.capacity(), capacity);
    }
}
