//! Host engine port layout, and the loopback address given to each
//! remote device on the client side.

use std::fmt;
use std::net::Ipv4Addr;

/// Range the host engine's base port is drawn from. Chosen so it never
/// collides with a standard Sunshine install, whose base port is 47989.
pub const ENGINE_BASE_PORT_MIN: u16 = 42000;
pub const ENGINE_BASE_PORT_MAX: u16 = 42999;

/// Ports of one host engine instance, derived from the base port by the
/// fixed offsets of the GameStream protocol that Sunshine implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnginePorts {
    base: u16,
}

/// Base port outside the allowed range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasePortOutOfRange(pub u16);

impl fmt::Display for BasePortOutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "port de base {} hors de la plage {}-{}",
            self.0, ENGINE_BASE_PORT_MIN, ENGINE_BASE_PORT_MAX
        )
    }
}

impl std::error::Error for BasePortOutOfRange {}

impl EnginePorts {
    pub fn new(base: u16) -> Result<Self, BasePortOutOfRange> {
        if (ENGINE_BASE_PORT_MIN..=ENGINE_BASE_PORT_MAX).contains(&base) {
            Ok(Self { base })
        } else {
            Err(BasePortOutOfRange(base))
        }
    }

    pub fn base(&self) -> u16 {
        self.base
    }

    /// TCP: GameStream pairing and control over HTTPS.
    pub fn https(&self) -> u16 {
        self.base - 5
    }

    /// TCP: GameStream discovery and start of pairing, over HTTP.
    pub fn http(&self) -> u16 {
        self.base
    }

    /// TCP: the engine's web interface and local API, locked to loopback.
    pub fn web_ui(&self) -> u16 {
        self.base + 1
    }

    /// UDP: video stream.
    pub fn video(&self) -> u16 {
        self.base + 9
    }

    /// UDP: real-time control channel, which carries the inputs.
    pub fn control(&self) -> u16 {
        self.base + 10
    }

    /// UDP: audio stream.
    pub fn audio(&self) -> u16 {
        self.base + 11
    }

    /// TCP: RTSP session negotiation.
    pub fn rtsp(&self) -> u16 {
        self.base + 21
    }

    /// TCP ports to reserve for one engine instance.
    pub fn tcp_ports(&self) -> [u16; 4] {
        [self.https(), self.http(), self.web_ui(), self.rtsp()]
    }

    /// UDP ports to reserve for one engine instance.
    pub fn udp_ports(&self) -> [u16; 3] {
        [self.video(), self.control(), self.audio()]
    }
}

/// Stable loopback address given to a remote device on the client side.
///
/// Each device gets its own address in 127.77.x.y, which Windows accepts
/// anywhere in 127.0.0.0/8 without configuration. It keeps the client
/// engine's stored state consistent over time and allows several
/// outgoing sessions at once. Final octets 0 and 255 are avoided.
/// Returns `None` past the capacity of the scheme, which is 65024
/// devices.
pub fn device_loopback_addr(device_index: u16) -> Option<Ipv4Addr> {
    let x = device_index / 254;
    let y = (device_index % 254) + 1;
    if x > 255 {
        return None;
    }
    Some(Ipv4Addr::new(127, 77, x as u8, y as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_are_derived_from_the_base_port() {
        let ports = EnginePorts::new(42000).unwrap();
        assert_eq!(ports.https(), 41995);
        assert_eq!(ports.http(), 42000);
        assert_eq!(ports.web_ui(), 42001);
        assert_eq!(ports.video(), 42009);
        assert_eq!(ports.control(), 42010);
        assert_eq!(ports.audio(), 42011);
        assert_eq!(ports.rtsp(), 42021);
        assert_eq!(ports.tcp_ports(), [41995, 42000, 42001, 42021]);
        assert_eq!(ports.udp_ports(), [42009, 42010, 42011]);
    }

    #[test]
    fn a_base_outside_the_range_is_refused() {
        assert!(EnginePorts::new(41999).is_err());
        assert!(EnginePorts::new(43000).is_err());
        assert!(EnginePorts::new(42999).is_ok());
    }

    #[test]
    fn loopback_addresses_are_stable_and_avoid_reserved_octets() {
        assert_eq!(device_loopback_addr(0), Some(Ipv4Addr::new(127, 77, 0, 1)));
        assert_eq!(
            device_loopback_addr(253),
            Some(Ipv4Addr::new(127, 77, 0, 254))
        );
        assert_eq!(
            device_loopback_addr(254),
            Some(Ipv4Addr::new(127, 77, 1, 1))
        );
        for index in 0..2000u16 {
            let octets = device_loopback_addr(index).unwrap().octets();
            assert_ne!(octets[3], 0);
            assert_ne!(octets[3], 255);
        }
        assert_eq!(device_loopback_addr(65024), None);
    }
}
