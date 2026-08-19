//! What this computer is called, and where it answers.
//!
//! The name a person recognises their machine by is the one Windows
//! shows everywhere else: in the network neighbourhood, in the system
//! settings, on another computer's screen. Inventing our own would mean
//! the same machine answering to two names.

use std::fmt;
use std::net::Ipv4Addr;

/// One place this computer answers at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// Name Windows gives the card, which is the name its owner reads in
    /// the system settings: « Ethernet », « Wi-Fi », or whatever a
    /// virtual adapter calls itself.
    pub interface: String,
    pub address: Ipv4Addr,
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.address, self.interface)
    }
}

/// Every address this computer answers at, card by card.
///
/// Written into the journal because a machine announcing itself on a
/// virtual adapter while the other one is on the real card looks, from
/// both sides, exactly like a machine that is switched off. Named cards
/// are what tells those two apart without anyone running anything.
///
/// Loopback is left out, being of no use to anybody else, and so is
/// version six: the tunnel is opened on version four.
pub fn addresses() -> Vec<Address> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut answering: Vec<Address> = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(addr) => Some(Address {
                interface: interface.name,
                address: addr.ip,
            }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    // Settled order: the same computer read twice must give the same
    // list, or two journals taken minutes apart cannot be compared.
    answering.sort_by(|a, b| {
        a.address
            .cmp(&b.address)
            .then(a.interface.cmp(&b.interface))
    });
    answering
}

/// Name of this computer, as its owner knows it.
///
/// Falls back to something readable rather than failing: a nameless
/// machine in a list would be worse than an approximate one.
pub fn name() -> String {
    readable(raw_name())
}

#[cfg(windows)]
fn raw_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(not(windows))]
fn raw_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
}

/// Keeps only what makes a name, and gives one when there is none.
fn readable(found: Option<String>) -> String {
    let cleaned = found.unwrap_or_default().trim().to_string();
    if cleaned.is_empty() {
        "Cet ordinateur".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_always_has_a_name_to_show() {
        assert!(!name().is_empty());
    }

    #[test]
    fn every_address_shown_is_one_another_computer_could_use() {
        let answering = addresses();
        for address in &answering {
            assert!(!address.interface.is_empty(), "{address}");
            assert!(!address.address.is_loopback(), "{address}");
            // Ce qui part dans le journal se lit d'un coup d'œil :
            // l'adresse, puis la carte entre parenthèses.
            let line = address.to_string();
            assert!(line.starts_with(&address.address.to_string()), "{line}");
            assert!(line.contains(&address.interface), "{line}");
        }
        assert!(
            answering
                .windows(2)
                .all(|two| two[0].address <= two[1].address),
            "{answering:?}"
        );
    }

    #[test]
    fn nothing_readable_still_gives_something_to_display() {
        assert_eq!(readable(None), "Cet ordinateur");
        assert_eq!(readable(Some("   ".to_string())), "Cet ordinateur");
        assert_eq!(readable(Some("  PC-BUREAU\n".to_string())), "PC-BUREAU");
    }
}
