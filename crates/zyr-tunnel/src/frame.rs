//! Naming the channel at the head of every datagram.
//!
//! One byte is enough: the packet size budget accounts for it, and every
//! byte taken here is a byte less for the video.

use crate::channel::{DatagramChannel, UnknownChannel};

/// Malformed datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    Empty,
    Channel(UnknownChannel),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Empty => write!(f, "datagramme vide"),
            FrameError::Channel(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// What a datagram off the tunnel turns out to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed<'a> {
    /// Real traffic for one of the engine's channels.
    Channel(DatagramChannel, &'a [u8]),
    /// Nothing: a datagram sent only to be acknowledged.
    Nudge,
}

/// Leading byte of a nudge. No engine channel is ever numbered this way,
/// `DatagramChannel` starting at 1, so it can never collide with one.
const NUDGE: u8 = 0;

/// Puts the channel in front of the payload.
pub fn encode(channel: DatagramChannel, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(channel.identifier());
    frame.extend_from_slice(payload);
    frame
}

/// A datagram carrying nothing, asked for nothing and answered the same
/// way: it exists only to be sent and acknowledged.
pub fn encode_nudge() -> Vec<u8> {
    vec![NUDGE]
}

/// Splits the channel from the payload, or recognises a nudge.
pub fn decode(frame: &[u8]) -> Result<Landed<'_>, FrameError> {
    let (head, payload) = frame.split_first().ok_or(FrameError::Empty)?;
    if *head == NUDGE {
        return Ok(Landed::Nudge);
    }
    let channel = DatagramChannel::from_identifier(*head).map_err(FrameError::Channel)?;
    Ok(Landed::Channel(channel, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_makes_the_round_trip_untouched() {
        for channel in DatagramChannel::ALL {
            let payload = b"some video packet or other";
            let frame = encode(channel, payload);
            assert_eq!(decode(&frame).unwrap(), Landed::Channel(channel, payload));
        }
    }

    #[test]
    fn the_overhead_matches_what_the_budget_takes_off() {
        // If the header grew without the budget following, the packets
        // would exceed the announced size and get fragmented.
        let payload = vec![0u8; 1300];
        let frame = encode(DatagramChannel::Video, &payload);
        let overhead = u16::try_from(frame.len() - payload.len()).unwrap();
        assert_eq!(overhead, zyr_transport::mtu::MUX_OVERHEAD);
    }

    #[test]
    fn an_empty_payload_stays_carriable() {
        let frame = encode(DatagramChannel::Control, &[]);
        assert_eq!(
            decode(&frame).unwrap(),
            Landed::Channel(DatagramChannel::Control, [].as_slice())
        );
    }

    #[test]
    fn a_nudge_is_told_apart_from_every_channel() {
        assert_eq!(decode(&encode_nudge()).unwrap(), Landed::Nudge);
    }

    #[test]
    fn malformed_frames_are_refused() {
        assert_eq!(decode(&[]), Err(FrameError::Empty));
        assert!(matches!(decode(&[4, 1, 2]), Err(FrameError::Channel(_))));
        assert!(matches!(decode(&[99]), Err(FrameError::Channel(_))));
    }
}
