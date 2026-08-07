//! Schéma de ports du moteur hôte et adressage loopback des appareils distants.

use std::fmt;
use std::net::Ipv4Addr;

/// Plage dans laquelle le port de base du moteur hôte est tiré.
/// Choisie pour ne jamais entrer en collision avec une installation
/// Sunshine standard (port de base 47989).
pub const ENGINE_BASE_PORT_MIN: u16 = 42000;
pub const ENGINE_BASE_PORT_MAX: u16 = 42999;

/// Ports d'une instance du moteur hôte, dérivés du port de base selon
/// les décalages fixes du protocole GameStream implémentés par Sunshine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnginePorts {
    base: u16,
}

/// Port de base hors de la plage autorisée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasePortHorsPlage(pub u16);

impl fmt::Display for BasePortHorsPlage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "port de base {} hors de la plage {}-{}",
            self.0, ENGINE_BASE_PORT_MIN, ENGINE_BASE_PORT_MAX
        )
    }
}

impl std::error::Error for BasePortHorsPlage {}

impl EnginePorts {
    pub fn new(base: u16) -> Result<Self, BasePortHorsPlage> {
        if (ENGINE_BASE_PORT_MIN..=ENGINE_BASE_PORT_MAX).contains(&base) {
            Ok(Self { base })
        } else {
            Err(BasePortHorsPlage(base))
        }
    }

    pub fn base(&self) -> u16 {
        self.base
    }

    /// TCP : appairage et contrôle HTTPS GameStream.
    pub fn https(&self) -> u16 {
        self.base - 5
    }

    /// TCP : découverte et amorce d'appairage HTTP GameStream.
    pub fn http(&self) -> u16 {
        self.base
    }

    /// TCP : interface web et API locale de Sunshine (verrouillée loopback).
    pub fn web_ui(&self) -> u16 {
        self.base + 1
    }

    /// UDP : flux vidéo.
    pub fn video(&self) -> u16 {
        self.base + 9
    }

    /// UDP : canal de contrôle temps réel (entrées).
    pub fn control(&self) -> u16 {
        self.base + 10
    }

    /// UDP : flux audio.
    pub fn audio(&self) -> u16 {
        self.base + 11
    }

    /// TCP : négociation de session RTSP.
    pub fn rtsp(&self) -> u16 {
        self.base + 21
    }

    /// Ports TCP à réserver pour une instance du moteur.
    pub fn tcp_ports(&self) -> [u16; 4] {
        [self.https(), self.http(), self.web_ui(), self.rtsp()]
    }

    /// Ports UDP à réserver pour une instance du moteur.
    pub fn udp_ports(&self) -> [u16; 3] {
        [self.video(), self.control(), self.audio()]
    }
}

/// Adresse loopback stable attribuée à un appareil distant côté client.
///
/// Chaque appareil reçoit une adresse dédiée en 127.77.x.y (Windows accepte
/// tout 127.0.0.0/8 sans configuration), ce qui garde l'état du moteur client
/// cohérent dans le temps et permet plusieurs sessions sortantes simultanées.
/// Les octets finaux 0 et 255 sont évités. Retourne `None` au-delà de la
/// capacité du schéma (65 024 appareils).
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
    fn ports_derives_du_port_de_base() {
        let p = EnginePorts::new(42000).unwrap();
        assert_eq!(p.https(), 41995);
        assert_eq!(p.http(), 42000);
        assert_eq!(p.web_ui(), 42001);
        assert_eq!(p.video(), 42009);
        assert_eq!(p.control(), 42010);
        assert_eq!(p.audio(), 42011);
        assert_eq!(p.rtsp(), 42021);
        assert_eq!(p.tcp_ports(), [41995, 42000, 42001, 42021]);
        assert_eq!(p.udp_ports(), [42009, 42010, 42011]);
    }

    #[test]
    fn base_hors_plage_refusee() {
        assert!(EnginePorts::new(41999).is_err());
        assert!(EnginePorts::new(43000).is_err());
        assert!(EnginePorts::new(42999).is_ok());
    }

    #[test]
    fn adresses_loopback_stables_et_sans_octets_reserves() {
        assert_eq!(device_loopback_addr(0), Some(Ipv4Addr::new(127, 77, 0, 1)));
        assert_eq!(
            device_loopback_addr(253),
            Some(Ipv4Addr::new(127, 77, 0, 254))
        );
        assert_eq!(
            device_loopback_addr(254),
            Some(Ipv4Addr::new(127, 77, 1, 1))
        );
        for i in 0..2000u16 {
            let addr = device_loopback_addr(i).unwrap();
            let octets = addr.octets();
            assert_ne!(octets[3], 0);
            assert_ne!(octets[3], 255);
        }
        assert_eq!(device_loopback_addr(65024), None);
    }
}
