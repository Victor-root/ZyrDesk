//! Video packet size budget.
//!
//! A video packet that is too big gets fragmented along the way, and
//! fragmentation costs latency: a single lost fragment destroys the
//! whole packet. The size we ask the host engine for must therefore fit
//! in what the tunnel can carry in one piece.
//!
//! The computation starts from the size the transport actually reports
//! rather than from a guess at the QUIC overhead. Headers vary with the
//! length of the connection identifiers and the state of the path:
//! guessing them would mean redoing, less well, a computation the
//! transport already keeps up to date.

/// ZyrDesk header in front of every datagram: the channel identifier.
pub const MUX_OVERHEAD: u16 = 1;

/// Headers the engines' protocol adds to every video packet.
///
/// An estimate, pending a real measurement by packet capture. Milestone
/// M1's check V5 has to confirm it; any error here is paid in
/// fragmentation.
pub const ESTIMATED_ENGINE_HEADER: u16 = 28;

/// Margin kept while the real header size is unmeasured.
///
/// It shrinks to a few bytes once check V5 comes back.
pub const MARGIN: u16 = 32;

/// Ceiling: the value the client engine uses on a local network.
///
/// Going beyond brings nothing and moves closer to fragmentation.
pub const NOMINAL_SIZE: u16 = 1392;

/// Floor imposed by the client engine.
///
/// Below this it refuses the value. Its own value for a distant network
/// is 1024: staying above keeps it in "local" mode, meaning its remote
/// network detection stays off, since we are the ones handling the path.
pub const MINIMUM_SIZE: u16 = 1025;

/// The path cannot carry a usable video packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathTooNarrow {
    pub available_datagram: u16,
    pub required_datagram: u16,
}

impl std::fmt::Display for PathTooNarrow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "chemin trop étroit : {} octets utilisables, {} nécessaires",
            self.available_datagram, self.required_datagram
        )
    }
}

impl std::error::Error for PathTooNarrow {}

/// Packet size to ask the host engine for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketSize {
    pub bytes: u16,
    /// True when the path forced a size below the nominal value.
    ///
    /// It changes nothing functionally, but deserves a log line: the
    /// packet rate rises as the size falls.
    pub reduced_by_the_path: bool,
}

/// Total overhead taken off the datagram's usable room.
const TOTAL_OVERHEAD: u16 = MUX_OVERHEAD + ESTIMATED_ENGINE_HEADER + MARGIN;

/// Computes the packet size that fits in the announced datagram.
///
/// `usable_datagram` is the payload the transport accepts without
/// fragmenting, as it reports it for the current path.
pub fn packet_size(usable_datagram: u16) -> Result<PacketSize, PathTooNarrow> {
    let available = usable_datagram.saturating_sub(TOTAL_OVERHEAD);
    if available < MINIMUM_SIZE {
        return Err(PathTooNarrow {
            available_datagram: usable_datagram,
            required_datagram: MINIMUM_SIZE + TOTAL_OVERHEAD,
        });
    }
    Ok(PacketSize {
        bytes: available.min(NOMINAL_SIZE),
        reduced_by_the_path: available < NOMINAL_SIZE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_path_gives_the_nominal_size() {
        let size = packet_size(NOMINAL_SIZE + TOTAL_OVERHEAD).unwrap();
        assert_eq!(size.bytes, NOMINAL_SIZE);
        assert!(!size.reduced_by_the_path);

        // Wider still: the value stays capped.
        let size = packet_size(4000).unwrap();
        assert_eq!(size.bytes, NOMINAL_SIZE);
        assert!(!size.reduced_by_the_path);
    }

    #[test]
    fn an_ordinary_path_stays_comfortable() {
        // A common Ethernet path: the transport announces roughly 1400
        // usable bytes once its own overhead is taken off.
        let size = packet_size(1400).unwrap();
        assert!(size.bytes >= 1300, "only {} bytes", size.bytes);
        assert!(size.reduced_by_the_path);
    }

    #[test]
    fn a_narrow_path_shrinks_without_going_under_the_floor() {
        let just_enough = MINIMUM_SIZE + TOTAL_OVERHEAD;
        let size = packet_size(just_enough).unwrap();
        assert_eq!(size.bytes, MINIMUM_SIZE);
        assert!(size.reduced_by_the_path);
    }

    #[test]
    fn a_path_too_narrow_is_refused_rather_than_trimmed() {
        let too_tight = MINIMUM_SIZE + TOTAL_OVERHEAD - 1;
        let refusal = packet_size(too_tight).unwrap_err();
        assert_eq!(refusal.available_datagram, too_tight);
        assert!(refusal.required_datagram > too_tight);

        assert!(packet_size(0).is_err());
        assert!(packet_size(500).is_err());
    }

    #[test]
    fn the_size_returned_always_fits_the_datagram() {
        for datagram in (MINIMUM_SIZE + TOTAL_OVERHEAD)..=2000 {
            let size = packet_size(datagram).unwrap();
            let occupied = size.bytes + TOTAL_OVERHEAD;
            assert!(
                occupied <= datagram,
                "{occupied} bytes occupied for {datagram} available"
            );
            assert!(size.bytes >= MINIMUM_SIZE);
        }
    }
}
