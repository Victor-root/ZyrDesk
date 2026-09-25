//! What travels inside the tunnel, and how each kind is told apart.
//!
//! Two natures of traffic, kept apart exactly as they are. The engine's
//! control stream needs everything to arrive, in order: it takes one
//! reliable stream. The picture and the sound are worth nothing late:
//! they take datagrams, never retransmitted, which is what keeps a lost
//! packet from holding up every packet behind it.
//!
//! ZyrDesk's own questions take a reliable stream each, beside the
//! engine's.

use zyr_control::link::Channel;

/// Real-time streams, carried as unreliable datagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatagramChannel {
    Video,
    Audio,
}

/// Identifier of an unknown channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownChannel(pub u8);

impl std::fmt::Display for UnknownChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "canal inconnu : {}", self.0)
    }
}

impl std::error::Error for UnknownChannel {}

impl DatagramChannel {
    pub const ALL: [DatagramChannel; 2] = [DatagramChannel::Video, DatagramChannel::Audio];

    /// Leading byte that names the channel inside a datagram. The same
    /// number as the link's own channel, so a datagram changes carrier
    /// and never its name.
    pub fn identifier(self) -> u8 {
        match self {
            DatagramChannel::Video => 1,
            DatagramChannel::Audio => 3,
        }
    }

    pub fn from_identifier(byte: u8) -> Result<Self, UnknownChannel> {
        match byte {
            1 => Ok(DatagramChannel::Video),
            3 => Ok(DatagramChannel::Audio),
            other => Err(UnknownChannel(other)),
        }
    }

    /// The channel of the local link these datagrams travel on.
    pub fn on_the_link(self) -> Channel {
        match self {
            DatagramChannel::Video => Channel::Video,
            DatagramChannel::Audio => Channel::Audio,
        }
    }

    /// The datagram channel a frame of the local link belongs to, when
    /// it is one of them.
    pub fn off_the_link(channel: Channel) -> Option<Self> {
        match channel {
            Channel::Video => Some(DatagramChannel::Video),
            Channel::Audio => Some(DatagramChannel::Audio),
            Channel::Control | Channel::Service => None,
        }
    }
}

/// Reliable streams, carried as ordered streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamChannel {
    /// The engine's control stream, between the player and the engine:
    /// one per session, opened by the side watching.
    Engine,
    /// ZyrDesk's own channel: one question per stream.
    ZyrDesk,
}

impl StreamChannel {
    pub const ALL: [StreamChannel; 2] = [StreamChannel::Engine, StreamChannel::ZyrDesk];

    pub fn identifier(self) -> u8 {
        match self {
            StreamChannel::Engine => 1,
            StreamChannel::ZyrDesk => 4,
        }
    }

    pub fn from_identifier(byte: u8) -> Result<Self, UnknownChannel> {
        match byte {
            1 => Ok(StreamChannel::Engine),
            4 => Ok(StreamChannel::ZyrDesk),
            other => Err(UnknownChannel(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_datagram_identifiers_make_the_round_trip() {
        for channel in DatagramChannel::ALL {
            let id = channel.identifier();
            assert_eq!(DatagramChannel::from_identifier(id).unwrap(), channel);
        }
    }

    #[test]
    fn the_stream_identifiers_make_the_round_trip() {
        for channel in StreamChannel::ALL {
            let id = channel.identifier();
            assert_eq!(StreamChannel::from_identifier(id).unwrap(), channel);
        }
    }

    #[test]
    fn a_datagram_keeps_its_number_from_the_link_to_the_tunnel() {
        // The link and the tunnel number the picture and the sound the
        // same way: a datagram changing carrier must never change name.
        for channel in DatagramChannel::ALL {
            let on_the_link = channel.on_the_link();
            assert_eq!(on_the_link as u8, channel.identifier());
            assert_eq!(DatagramChannel::off_the_link(on_the_link), Some(channel));
        }
        assert_eq!(DatagramChannel::off_the_link(Channel::Control), None);
        assert_eq!(DatagramChannel::off_the_link(Channel::Service), None);
    }

    #[test]
    fn an_unknown_identifier_is_refused() {
        // Nought is the nudge, and two was the engines' control channel,
        // which now travels reliably.
        for byte in [0, 2, 4, 200] {
            assert_eq!(
                DatagramChannel::from_identifier(byte),
                Err(UnknownChannel(byte))
            );
        }
        for byte in [0, 2, 3, 9] {
            assert!(StreamChannel::from_identifier(byte).is_err(), "{byte}");
        }
    }
}
