//! Les flux des moteurs, et leur repérage dans le tunnel.
//!
//! Les moteurs échangent sur sept ports : quatre en TCP, trois en UDP.
//! Le tunnel les fait tous passer dans une seule connexion, ce qui n'en
//! laisse qu'un à ouvrir dans un pare-feu et un seul chemin à établir.
//!
//! La distinction entre les deux natures de flux est conservée telle
//! quelle. Les flux TCP portent la négociation et l'appairage : ils
//! exigent que tout arrive, dans l'ordre, et empruntent donc des flux
//! fiables. Les flux UDP portent la vidéo, l'audio et les entrées : une
//! image en retard ne vaut rien, ils empruntent donc des datagrammes,
//! jamais retransmis. Les faire passer par un flux fiable ajouterait des
//! retransmissions et un blocage en file, exactement ce que le protocole
//! des moteurs évite depuis toujours.

use zyr_proto::net::EnginePorts;

/// Flux temps réel, transportés en datagrammes non fiables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanalDatagramme {
    Video,
    /// Entrées clavier et souris, et retours d'état de la session.
    Controle,
    Audio,
}

/// Identifiant d'un canal inconnu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanalInconnu(pub u8);

impl std::fmt::Display for CanalInconnu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "canal inconnu : {}", self.0)
    }
}

impl std::error::Error for CanalInconnu {}

impl CanalDatagramme {
    pub const TOUS: [CanalDatagramme; 3] = [
        CanalDatagramme::Video,
        CanalDatagramme::Controle,
        CanalDatagramme::Audio,
    ];

    /// Octet de tête identifiant le canal dans un datagramme.
    pub fn identifiant(self) -> u8 {
        match self {
            CanalDatagramme::Video => 1,
            CanalDatagramme::Controle => 2,
            CanalDatagramme::Audio => 3,
        }
    }

    /// Place du canal dans `TOUS`, pour ranger une chose par canal.
    pub fn rang(self) -> usize {
        match self {
            CanalDatagramme::Video => 0,
            CanalDatagramme::Controle => 1,
            CanalDatagramme::Audio => 2,
        }
    }

    pub fn depuis_identifiant(octet: u8) -> Result<Self, CanalInconnu> {
        match octet {
            1 => Ok(CanalDatagramme::Video),
            2 => Ok(CanalDatagramme::Controle),
            3 => Ok(CanalDatagramme::Audio),
            autre => Err(CanalInconnu(autre)),
        }
    }

    /// Port du moteur correspondant à ce canal.
    pub fn port(self, ports: EnginePorts) -> u16 {
        match self {
            CanalDatagramme::Video => ports.video(),
            CanalDatagramme::Controle => ports.control(),
            CanalDatagramme::Audio => ports.audio(),
        }
    }

    pub fn depuis_port(port: u16, ports: EnginePorts) -> Option<Self> {
        Self::TOUS.into_iter().find(|c| c.port(ports) == port)
    }
}

/// Flux fiables, transportés en flux ordonnés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanalFlux {
    /// Découverte et amorce d'appairage, en clair côté moteur.
    MoteurHttp,
    /// Appairage et contrôle de session, chiffré côté moteur.
    MoteurHttps,
    /// Négociation de session.
    Rtsp,
    /// Canal propre à ZyrDesk : versions, code d'appairage,
    /// presse-papiers, statistiques.
    ZyrDesk,
}

impl CanalFlux {
    pub const TOUS: [CanalFlux; 4] = [
        CanalFlux::MoteurHttp,
        CanalFlux::MoteurHttps,
        CanalFlux::Rtsp,
        CanalFlux::ZyrDesk,
    ];

    pub fn identifiant(self) -> u8 {
        match self {
            CanalFlux::MoteurHttp => 1,
            CanalFlux::MoteurHttps => 2,
            CanalFlux::Rtsp => 3,
            CanalFlux::ZyrDesk => 4,
        }
    }

    pub fn depuis_identifiant(octet: u8) -> Result<Self, CanalInconnu> {
        match octet {
            1 => Ok(CanalFlux::MoteurHttp),
            2 => Ok(CanalFlux::MoteurHttps),
            3 => Ok(CanalFlux::Rtsp),
            4 => Ok(CanalFlux::ZyrDesk),
            autre => Err(CanalInconnu(autre)),
        }
    }

    /// Port du moteur, sauf pour le canal propre à ZyrDesk.
    pub fn port(self, ports: EnginePorts) -> Option<u16> {
        match self {
            CanalFlux::MoteurHttp => Some(ports.http()),
            CanalFlux::MoteurHttps => Some(ports.https()),
            CanalFlux::Rtsp => Some(ports.rtsp()),
            CanalFlux::ZyrDesk => None,
        }
    }

    pub fn depuis_port(port: u16, ports: EnginePorts) -> Option<Self> {
        Self::TOUS.into_iter().find(|c| c.port(ports) == Some(port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports() -> EnginePorts {
        EnginePorts::new(42000).unwrap()
    }

    #[test]
    fn les_identifiants_de_datagramme_font_l_aller_retour() {
        for canal in CanalDatagramme::TOUS {
            let id = canal.identifiant();
            assert_eq!(CanalDatagramme::depuis_identifiant(id).unwrap(), canal);
        }
    }

    #[test]
    fn les_identifiants_de_flux_font_l_aller_retour() {
        for canal in CanalFlux::TOUS {
            let id = canal.identifiant();
            assert_eq!(CanalFlux::depuis_identifiant(id).unwrap(), canal);
        }
    }

    #[test]
    fn aucun_identifiant_n_est_partage() {
        let mut vus: Vec<u8> = CanalDatagramme::TOUS
            .iter()
            .map(|c| c.identifiant())
            .collect();
        vus.sort_unstable();
        vus.dedup();
        assert_eq!(vus.len(), CanalDatagramme::TOUS.len());

        let mut vus: Vec<u8> = CanalFlux::TOUS.iter().map(|c| c.identifiant()).collect();
        vus.sort_unstable();
        vus.dedup();
        assert_eq!(vus.len(), CanalFlux::TOUS.len());
    }

    #[test]
    fn le_rang_designe_bien_le_canal() {
        // Sans quoi ce qui est rangé par canal se retrouverait mélangé.
        for canal in CanalDatagramme::TOUS {
            assert_eq!(CanalDatagramme::TOUS[canal.rang()], canal);
        }
    }

    #[test]
    fn un_identifiant_inconnu_est_refuse() {
        assert_eq!(CanalDatagramme::depuis_identifiant(0), Err(CanalInconnu(0)));
        assert!(CanalDatagramme::depuis_identifiant(200).is_err());
        assert!(CanalFlux::depuis_identifiant(0).is_err());
        assert!(CanalFlux::depuis_identifiant(9).is_err());
    }

    #[test]
    fn les_canaux_visent_les_ports_attendus_du_moteur() {
        let p = ports();
        assert_eq!(CanalDatagramme::Video.port(p), p.video());
        assert_eq!(CanalDatagramme::Controle.port(p), p.control());
        assert_eq!(CanalDatagramme::Audio.port(p), p.audio());
        assert_eq!(CanalFlux::MoteurHttp.port(p), Some(p.http()));
        assert_eq!(CanalFlux::MoteurHttps.port(p), Some(p.https()));
        assert_eq!(CanalFlux::Rtsp.port(p), Some(p.rtsp()));
        assert_eq!(CanalFlux::ZyrDesk.port(p), None);
    }

    #[test]
    fn tous_les_ports_du_moteur_sont_couverts() {
        let p = ports();
        for port in p.udp_ports() {
            assert!(
                CanalDatagramme::depuis_port(port, p).is_some(),
                "port UDP {port} sans canal"
            );
        }
        // L'interface web du moteur n'est délibérément pas transportée :
        // elle reste inaccessible depuis l'autre ordinateur.
        for port in p.tcp_ports().into_iter().filter(|&x| x != p.web_ui()) {
            assert!(
                CanalFlux::depuis_port(port, p).is_some(),
                "port TCP {port} sans canal"
            );
        }
        assert!(CanalFlux::depuis_port(p.web_ui(), p).is_none());
    }

    #[test]
    fn un_port_etranger_n_est_rattache_a_aucun_canal() {
        let p = ports();
        assert!(CanalDatagramme::depuis_port(80, p).is_none());
        assert!(CanalFlux::depuis_port(80, p).is_none());
    }
}
