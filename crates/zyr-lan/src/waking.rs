//! Waking a computer of this network that is switched off or asleep.
//!
//! Its card listens for one packet even then: six bytes of 255, then its
//! own hardware address sixteen times. Sent to everyone on the network,
//! since a computer that is off answers on no address: once on every
//! card of this computer, and once more to the whole network.

use std::io;
use std::net::{Ipv4Addr, UdpSocket};

/// The port the packet goes to: the one cards are made to listen on.
const PORT: u16 = 9;

/// The packet that wakes the card of that hardware address.
fn magic_packet(card: [u8; 6]) -> [u8; 102] {
    let mut packet = [0xFF; 102];
    for copy in packet[6..].as_chunks_mut::<6>().0 {
        *copy = card;
    }
    packet
}

/// Sends the packet that wakes that card, on every network this computer
/// is on. Done once it left by one of them at least.
pub fn wake(card: [u8; 6]) -> io::Result<()> {
    let packet = magic_packet(card);
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    let mut everywhere: Vec<Ipv4Addr> = zyr_proto::machine::addresses()
        .into_iter()
        .filter_map(|card| card.broadcast)
        .collect();
    everywhere.push(Ipv4Addr::BROADCAST);
    let mut refused = None;
    let mut sent = false;
    for to in everywhere {
        match socket.send_to(&packet, (to, PORT)) {
            Ok(_) => sent = true,
            Err(e) => refused = Some(e),
        }
    }
    match (sent, refused) {
        (true, _) => Ok(()),
        (false, Some(e)) => Err(e),
        (false, None) => Err(io::Error::other("no network to send it on")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packet_is_six_bytes_of_255_then_the_card_sixteen_times() {
        let card = [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E];
        let packet = magic_packet(card);
        assert_eq!(packet[..6], [0xFF; 6]);
        assert_eq!(packet.len(), 6 + 16 * 6);
        assert!(packet[6..].chunks(6).all(|copy| copy == card));
    }
}
