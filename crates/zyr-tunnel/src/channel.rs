//! The engines' streams, and how they are told apart inside the tunnel.
//!
//! The engines talk over seven ports: four in TCP, three in UDP. The
//! tunnel carries them all inside a single connection, which leaves one
//! port to open in a firewall and one path to establish.
//!
//! The distinction between the two natures of traffic is kept exactly as
//! it is. The TCP streams carry negotiation and pairing: they need
//! everything to arrive, in order, so they take reliable streams. The
//! UDP streams carry video, audio and inputs: a late frame is worth
//! nothing, so they take datagrams, never retransmitted. Putting them
//! through a reliable stream would add retransmissions and head-of-line
//! blocking, exactly what the engines' protocol has always avoided.

use zyr_proto::net::EnginePorts;

/// Real-time streams, carried as unreliable datagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatagramChannel {
    Video,
    /// Keyboard and mouse inputs, and session state feedback.
    Control,
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
    pub const ALL: [DatagramChannel; 3] = [
        DatagramChannel::Video,
        DatagramChannel::Control,
        DatagramChannel::Audio,
    ];

    /// Leading byte that names the channel inside a datagram.
    pub fn identifier(self) -> u8 {
        match self {
            DatagramChannel::Video => 1,
            DatagramChannel::Control => 2,
            DatagramChannel::Audio => 3,
        }
    }

    /// Place of the channel in `ALL`, to hold one thing per channel.
    pub fn rank(self) -> usize {
        match self {
            DatagramChannel::Video => 0,
            DatagramChannel::Control => 1,
            DatagramChannel::Audio => 2,
        }
    }

    pub fn from_identifier(byte: u8) -> Result<Self, UnknownChannel> {
        match byte {
            1 => Ok(DatagramChannel::Video),
            2 => Ok(DatagramChannel::Control),
            3 => Ok(DatagramChannel::Audio),
            other => Err(UnknownChannel(other)),
        }
    }

    /// Engine port this channel corresponds to.
    pub fn port(self, ports: EnginePorts) -> u16 {
        match self {
            DatagramChannel::Video => ports.video(),
            DatagramChannel::Control => ports.control(),
            DatagramChannel::Audio => ports.audio(),
        }
    }

    pub fn from_port(port: u16, ports: EnginePorts) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.port(ports) == port)
    }
}

/// Reliable streams, carried as ordered streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamChannel {
    /// Discovery and start of pairing, in the clear on the engine side.
    EngineHttp,
    /// Pairing and session control, encrypted on the engine side.
    EngineHttps,
    /// Session negotiation.
    Rtsp,
    /// ZyrDesk's own channel: versions, pairing code, clipboard,
    /// statistics.
    ZyrDesk,
}

impl StreamChannel {
    pub const ALL: [StreamChannel; 4] = [
        StreamChannel::EngineHttp,
        StreamChannel::EngineHttps,
        StreamChannel::Rtsp,
        StreamChannel::ZyrDesk,
    ];

    pub fn identifier(self) -> u8 {
        match self {
            StreamChannel::EngineHttp => 1,
            StreamChannel::EngineHttps => 2,
            StreamChannel::Rtsp => 3,
            StreamChannel::ZyrDesk => 4,
        }
    }

    pub fn from_identifier(byte: u8) -> Result<Self, UnknownChannel> {
        match byte {
            1 => Ok(StreamChannel::EngineHttp),
            2 => Ok(StreamChannel::EngineHttps),
            3 => Ok(StreamChannel::Rtsp),
            4 => Ok(StreamChannel::ZyrDesk),
            other => Err(UnknownChannel(other)),
        }
    }

    /// Engine port, except for ZyrDesk's own channel.
    pub fn port(self, ports: EnginePorts) -> Option<u16> {
        match self {
            StreamChannel::EngineHttp => Some(ports.http()),
            StreamChannel::EngineHttps => Some(ports.https()),
            StreamChannel::Rtsp => Some(ports.rtsp()),
            StreamChannel::ZyrDesk => None,
        }
    }

    pub fn from_port(port: u16, ports: EnginePorts) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.port(ports) == Some(port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports() -> EnginePorts {
        EnginePorts::new(42000).unwrap()
    }

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
    fn no_identifier_is_shared() {
        let mut seen: Vec<u8> = DatagramChannel::ALL
            .iter()
            .map(|c| c.identifier())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), DatagramChannel::ALL.len());

        let mut seen: Vec<u8> = StreamChannel::ALL.iter().map(|c| c.identifier()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), StreamChannel::ALL.len());
    }

    #[test]
    fn the_rank_names_the_right_channel() {
        // Without this, whatever is held per channel would get mixed up.
        for channel in DatagramChannel::ALL {
            assert_eq!(DatagramChannel::ALL[channel.rank()], channel);
        }
    }

    #[test]
    fn an_unknown_identifier_is_refused() {
        assert_eq!(DatagramChannel::from_identifier(0), Err(UnknownChannel(0)));
        assert!(DatagramChannel::from_identifier(200).is_err());
        assert!(StreamChannel::from_identifier(0).is_err());
        assert!(StreamChannel::from_identifier(9).is_err());
    }

    #[test]
    fn the_channels_target_the_expected_engine_ports() {
        let ports = ports();
        assert_eq!(DatagramChannel::Video.port(ports), ports.video());
        assert_eq!(DatagramChannel::Control.port(ports), ports.control());
        assert_eq!(DatagramChannel::Audio.port(ports), ports.audio());
        assert_eq!(StreamChannel::EngineHttp.port(ports), Some(ports.http()));
        assert_eq!(StreamChannel::EngineHttps.port(ports), Some(ports.https()));
        assert_eq!(StreamChannel::Rtsp.port(ports), Some(ports.rtsp()));
        assert_eq!(StreamChannel::ZyrDesk.port(ports), None);
    }

    #[test]
    fn every_engine_port_is_covered() {
        let ports = ports();
        for port in ports.udp_ports() {
            assert!(
                DatagramChannel::from_port(port, ports).is_some(),
                "UDP port {port} with no channel"
            );
        }
        // The engine's web interface is deliberately not carried: it
        // stays out of reach from the other computer.
        for port in ports
            .tcp_ports()
            .into_iter()
            .filter(|&p| p != ports.web_ui())
        {
            assert!(
                StreamChannel::from_port(port, ports).is_some(),
                "TCP port {port} with no channel"
            );
        }
        assert!(StreamChannel::from_port(ports.web_ui(), ports).is_none());
    }

    #[test]
    fn a_foreign_port_belongs_to_no_channel() {
        let ports = ports();
        assert!(DatagramChannel::from_port(80, ports).is_none());
        assert!(StreamChannel::from_port(80, ports).is_none());
    }
}
