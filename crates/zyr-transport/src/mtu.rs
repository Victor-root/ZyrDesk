//! What one datagram of the engine may weigh.
//!
//! A datagram too big for the path is refused by the transport and lost,
//! and one sized on the measurement of the moment is lost the day the
//! path narrows under it: the room an engine is told of is what the
//! path can never stop carrying (see `guaranteed_usable_datagram`), less
//! the byte that names the channel in front of every datagram.

/// ZyrDesk header in front of every datagram: the channel identifier.
pub const MUX_OVERHEAD: u16 = 1;

/// What one datagram of the engine may carry on a path that guarantees
/// `usable` bytes, when that leaves anything at all.
pub fn datagram_budget(usable: u16) -> Option<u16> {
    usable
        .checked_sub(MUX_OVERHEAD)
        .filter(|budget| *budget > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_leaves_room_for_the_channel() {
        assert_eq!(datagram_budget(1162), Some(1161));
        assert_eq!(datagram_budget(MUX_OVERHEAD), None);
        assert_eq!(datagram_budget(0), None);
    }
}
