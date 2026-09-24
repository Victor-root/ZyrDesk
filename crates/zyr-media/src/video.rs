//! The picture on its way from host to client.
//!
//! Each encoded frame is cut into shards of equal size, each carried by
//! one datagram behind a 28-byte header, and followed by parity shards
//! computed with Reed-Solomon: any `data` shards among the `data +
//! parity` sent are enough to rebuild the frame, so a lost packet costs
//! nothing as long as no more than `parity` of them went missing.
//!
//! Header, little-endian:
//!
//! ```text
//! off size field
//! 0   1    version (1)
//! 1   1    flags: bit0 key frame, bit1 repeat of an unchanged screen
//! 2   2    stream (bumped each time the encoder is built again)
//! 4   4    frame (0, 1, 2... within a stream)
//! 8   2    index (of this shard, 0..data+parity)
//! 10  2    data (number of data shards, at least 1)
//! 12  2    parity (number of parity shards)
//! 14  2    shard (size of each shard in bytes, even, at least 2)
//! 16  4    size (of the encoded frame, in the last data shard)
//! 20  4    captured (host clock when the screen was captured, us, low 32 bits)
//! 24  2    host (capture to sent, in units of 10 us, saturating)
//! 26  1    codec
//! 27  1    reserved (0)
//! ```

mod assemble;
mod packetize;

pub use assemble::{Assembled, AssembledFrame, Assembler, AssemblyCounters, AssemblyLimits};
pub use packetize::{DEFAULT_FEC_PERCENT, OutgoingFrame, PacketizeError, Packetizer, Packets};

use crate::codec::VideoCodec;
use crate::wire::{Reader, WireError};

/// Size of the header in front of every video shard.
pub const VIDEO_HEADER: usize = 28;

/// Most shards a frame is cut into, data and parity together.
///
/// Any split within it is one the Reed-Solomon code supports.
pub const MAX_SHARDS: usize = 32768;

/// Largest shard a header can describe: its size is a u16, and even.
pub const MAX_SHARD_BYTES: usize = u16::MAX as usize - 1;

const VERSION: u8 = 1;
const KEY: u8 = 1;
const REPEAT: u8 = 2;

/// Where the shard index sits in the header, the one field that differs
/// between the packets of a frame.
const INDEX_AT: usize = 8;

/// What the header of one video datagram says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoHeader {
    pub key: bool,
    pub repeat: bool,
    pub stream: u16,
    pub frame: u32,
    pub index: u16,
    pub data: u16,
    pub parity: u16,
    pub shard: u16,
    pub size: u32,
    pub captured_us: u32,
    /// From capture to sending, carried in steps of 10 us and at most
    /// 655 350 us.
    pub host_latency_us: u32,
    pub codec: VideoCodec,
}

impl VideoHeader {
    pub fn write(&self, out: &mut [u8; VIDEO_HEADER]) {
        let flags = (u8::from(self.key) * KEY) | (u8::from(self.repeat) * REPEAT);
        let host = u16::try_from(self.host_latency_us / 10).unwrap_or(u16::MAX);
        out[0] = VERSION;
        out[1] = flags;
        out[2..4].copy_from_slice(&self.stream.to_le_bytes());
        out[4..8].copy_from_slice(&self.frame.to_le_bytes());
        out[INDEX_AT..INDEX_AT + 2].copy_from_slice(&self.index.to_le_bytes());
        out[10..12].copy_from_slice(&self.data.to_le_bytes());
        out[12..14].copy_from_slice(&self.parity.to_le_bytes());
        out[14..16].copy_from_slice(&self.shard.to_le_bytes());
        out[16..20].copy_from_slice(&self.size.to_le_bytes());
        out[20..24].copy_from_slice(&self.captured_us.to_le_bytes());
        out[24..26].copy_from_slice(&host.to_le_bytes());
        out[26] = self.codec.wire();
        out[27] = 0;
    }

    /// Splits a datagram into its header and its shard, checking that
    /// the header describes a frame that can exist and that exactly one
    /// shard follows it.
    pub fn read(datagram: &[u8]) -> Result<(VideoHeader, &[u8]), WireError> {
        let mut reader = Reader::new(datagram);
        let version = reader.u8()?;
        if version != VERSION {
            return Err(WireError::Version(version.into()));
        }
        let flags = reader.u8()?;
        let stream = reader.u16()?;
        let frame = reader.u32()?;
        let index = reader.u16()?;
        let data = reader.u16()?;
        let parity = reader.u16()?;
        let shard = reader.u16()?;
        let size = reader.u32()?;
        let captured_us = reader.u32()?;
        let host = reader.u16()?;
        let codec = VideoCodec::from_wire(reader.u8()?).ok_or(WireError::Invalid("codec"))?;
        let _reserved = reader.u8()?;

        let shards = usize::from(data) + usize::from(parity);
        if data == 0 || shards > MAX_SHARDS {
            return Err(WireError::Invalid("data"));
        }
        if usize::from(index) >= shards {
            return Err(WireError::Invalid("index"));
        }
        if shard < 2 || shard % 2 != 0 {
            return Err(WireError::Invalid("shard"));
        }
        let (whole, but_one) = (
            u64::from(data) * u64::from(shard),
            u64::from(data - 1) * u64::from(shard),
        );
        if u64::from(size) > whole || u64::from(size) <= but_one {
            return Err(WireError::Invalid("size"));
        }
        let payload = reader.rest();
        match payload.len().cmp(&usize::from(shard)) {
            std::cmp::Ordering::Less => return Err(WireError::Truncated),
            std::cmp::Ordering::Greater => return Err(WireError::TooLong),
            std::cmp::Ordering::Equal => {}
        }
        let header = VideoHeader {
            key: flags & KEY != 0,
            repeat: flags & REPEAT != 0,
            stream,
            frame,
            index,
            data,
            parity,
            shard,
            size,
            captured_us,
            host_latency_us: u32::from(host) * 10,
            codec,
        };
        Ok((header, payload))
    }

    /// Whether two packets belong to the same frame as the same frame:
    /// everything but the shard index has to agree.
    fn same_frame_as(&self, other: &VideoHeader) -> bool {
        VideoHeader {
            index: other.index,
            ..*self
        } == *other
    }
}

/// Serial-number order on stream ids: whether `a` came after `b`.
fn newer_stream(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Noise;

    fn header() -> VideoHeader {
        VideoHeader {
            key: true,
            repeat: false,
            stream: 7,
            frame: 123_456,
            index: 3,
            data: 4,
            parity: 1,
            shard: 1000,
            size: 3500,
            captured_us: 0xdead_beef,
            host_latency_us: 4_560,
            codec: VideoCodec::Hevc,
        }
    }

    fn datagram(header: &VideoHeader) -> Vec<u8> {
        let mut head = [0u8; VIDEO_HEADER];
        header.write(&mut head);
        let mut datagram = head.to_vec();
        datagram.resize(VIDEO_HEADER + usize::from(header.shard), 0xab);
        datagram
    }

    #[test]
    fn a_header_makes_the_round_trip() {
        for (key, repeat) in [(false, false), (true, false), (false, true), (true, true)] {
            let header = VideoHeader {
                key,
                repeat,
                ..header()
            };
            let datagram = datagram(&header);
            let (read, shard) = VideoHeader::read(&datagram).unwrap();
            assert_eq!(read, header);
            assert_eq!(shard.len(), 1000);
            assert!(shard.iter().all(|byte| *byte == 0xab));
        }
    }

    #[test]
    fn the_host_latency_is_carried_in_steps_of_ten_microseconds_and_saturates() {
        let mut header = header();
        header.host_latency_us = 12_345;
        let (read, _) = VideoHeader::read(&datagram(&header)).unwrap();
        assert_eq!(read.host_latency_us, 12_340);
        header.host_latency_us = u32::MAX;
        let (read, _) = VideoHeader::read(&datagram(&header)).unwrap();
        assert_eq!(read.host_latency_us, 655_350);
    }

    #[test]
    fn impossible_headers_are_refused() {
        let refused = |change: fn(&mut VideoHeader)| {
            let mut header = header();
            change(&mut header);
            VideoHeader::read(&datagram(&header)).unwrap_err()
        };
        assert_eq!(refused(|h| h.data = 0), WireError::Invalid("data"));
        assert_eq!(
            refused(|h| {
                h.data = 30_000;
                h.parity = 2_769;
                h.size = 29_999 * 1000 + 1;
            }),
            WireError::Invalid("data")
        );
        assert_eq!(refused(|h| h.index = 5), WireError::Invalid("index"));
        assert_eq!(refused(|h| h.shard = 999), WireError::Invalid("shard"));
        assert_eq!(refused(|h| h.shard = 0), WireError::Invalid("shard"));
        assert_eq!(refused(|h| h.size = 3000), WireError::Invalid("size"));
        assert_eq!(refused(|h| h.size = 4001), WireError::Invalid("size"));

        let mut wrong_codec = datagram(&header());
        wrong_codec[26] = 9;
        assert_eq!(
            VideoHeader::read(&wrong_codec).unwrap_err(),
            WireError::Invalid("codec")
        );
        let mut wrong_version = datagram(&header());
        wrong_version[0] = 2;
        assert_eq!(
            VideoHeader::read(&wrong_version).unwrap_err(),
            WireError::Version(2)
        );
    }

    #[test]
    fn a_shard_of_the_wrong_length_is_refused() {
        let mut datagram = datagram(&header());
        datagram.push(0);
        assert_eq!(
            VideoHeader::read(&datagram).unwrap_err(),
            WireError::TooLong
        );
        datagram.truncate(VIDEO_HEADER + 999);
        assert_eq!(
            VideoHeader::read(&datagram).unwrap_err(),
            WireError::Truncated
        );
        for len in 0..VIDEO_HEADER {
            assert_eq!(
                VideoHeader::read(&datagram[..len]).unwrap_err(),
                WireError::Truncated
            );
        }
    }

    #[test]
    fn garbage_never_panics() {
        let mut noise = Noise::new(0x0076_6964_656f);
        let valid = vec![datagram(&header())];
        for _ in 0..100_000 {
            let bytes = noise.garbage(&valid);
            let _ = VideoHeader::read(&bytes);
        }
    }

    #[test]
    fn streams_follow_serial_order_across_the_wrap() {
        assert!(newer_stream(1, 0));
        assert!(newer_stream(0, u16::MAX));
        assert!(!newer_stream(u16::MAX, 0));
        assert!(!newer_stream(5, 5));
    }
}
