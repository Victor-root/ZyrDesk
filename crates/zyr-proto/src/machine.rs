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
    /// What tells this card's network apart from the rest of the world.
    pub netmask: Ipv4Addr,
    /// The one address every computer of that network answers to at
    /// once, when the card has one.
    pub broadcast: Option<Ipv4Addr>,
}

impl Address {
    /// Every address of this card's network, this one left out.
    ///
    /// Empty for a network too wide to go through one by one: a card on a
    /// sixteen-bit network holds sixty-five thousand addresses, and
    /// knocking on all of them would be a nuisance rather than a search.
    pub fn neighbourhood(&self, most: u32) -> Vec<Ipv4Addr> {
        let mask = u32::from(self.netmask);
        let mine = u32::from(self.address);
        // Zero would mean the whole Internet, and a full mask a network
        // of one: neither is a neighbourhood.
        let held = (!mask).checked_add(1).unwrap_or(0);
        if held < 3 || held > most {
            return Vec::new();
        }
        let network = mine & mask;
        // The first address names the network and the last is the
        // broadcast: neither belongs to a computer.
        (1..held - 1)
            .map(|step| Ipv4Addr::from(network + step))
            .filter(|address| *address != self.address)
            .collect()
    }
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
                netmask: addr.netmask,
                broadcast: addr.broadcast,
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

/// The IPv6 addresses of this computer that the whole Internet routes.
///
/// Apart from the IPv4 ones above because they serve another purpose:
/// nothing is announced or called on them, they are only worth naming
/// to a far computer, since two of them reach each other without any
/// box to go through. Link-local and unique-local addresses are left
/// out, as are the ones that only stand for an IPv4 address.
pub fn global_ipv6_addresses() -> Vec<std::net::Ipv6Addr> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut answering: Vec<std::net::Ipv6Addr> = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V6(addr) => Some(addr.ip),
            if_addrs::IfAddr::V4(_) => None,
        })
        .filter(|ip| {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_multicast()
                && !ip.is_unicast_link_local()
                && !ip.is_unique_local()
                && ip.to_ipv4_mapped().is_none()
        })
        .collect();
    answering.sort();
    answering.dedup();
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
    fn a_neighbourhood_holds_every_address_but_this_one() {
        let card = Address {
            interface: "Ethernet".to_string(),
            address: "192.168.1.20".parse().unwrap(),
            netmask: "255.255.255.0".parse().unwrap(),
            broadcast: Some("192.168.1.255".parse().unwrap()),
        };
        let around = card.neighbourhood(256);
        // 254 machines possibles, moins la nôtre.
        assert_eq!(around.len(), 253);
        assert_eq!(around[0], "192.168.1.1".parse::<Ipv4Addr>().unwrap());
        assert_eq!(
            *around.last().unwrap(),
            "192.168.1.254".parse::<Ipv4Addr>().unwrap()
        );
        assert!(!around.contains(&card.address));
        // Ni l'adresse du réseau ni celle de diffusion : personne n'y
        // répond, et frapper à ces deux portes-là ne sert à rien.
        assert!(!around.contains(&"192.168.1.0".parse().unwrap()));
        assert!(!around.contains(&"192.168.1.255".parse().unwrap()));
    }

    #[test]
    fn a_network_too_wide_is_left_alone() {
        // Frapper à soixante-cinq mille portes toutes les trente secondes
        // serait une nuisance, pas une recherche.
        let wide = Address {
            interface: "Ethernet".to_string(),
            address: "10.0.0.5".parse().unwrap(),
            netmask: "255.255.0.0".parse().unwrap(),
            broadcast: None,
        };
        assert!(wide.neighbourhood(256).is_empty());

        // Et un masque qui ne laisse la place à personne non plus.
        let alone = Address {
            interface: "Tunnel".to_string(),
            address: "10.1.1.1".parse().unwrap(),
            netmask: "255.255.255.255".parse().unwrap(),
            broadcast: None,
        };
        assert!(alone.neighbourhood(256).is_empty());
    }

    #[test]
    fn nothing_readable_still_gives_something_to_display() {
        assert_eq!(readable(None), "Cet ordinateur");
        assert_eq!(readable(Some("   ".to_string())), "Cet ordinateur");
        assert_eq!(readable(Some("  PC-BUREAU\n".to_string())), "PC-BUREAU");
    }
}
