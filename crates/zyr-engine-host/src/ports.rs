//! Choix d'une base de ports libre pour le moteur hôte.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, UdpSocket};

use zyr_proto::net::{ENGINE_BASE_PORT_MAX, ENGINE_BASE_PORT_MIN, EnginePorts};

/// Écart entre deux bases essayées.
///
/// Les ports d'une instance s'étalent de la base moins 5 à la base plus
/// 21, soit 27 numéros : un pas plus court ferait se chevaucher deux
/// instances voisines.
const PAS: u16 = 32;

/// Première base de la plage dont tous les ports dérivés sont libres.
pub fn base_libre() -> Option<EnginePorts> {
    (ENGINE_BASE_PORT_MIN..=ENGINE_BASE_PORT_MAX)
        .step_by(PAS as usize)
        .filter_map(|base| EnginePorts::new(base).ok())
        .find(ports_libres)
}

/// Vrai si les sept ports de cette instance peuvent être réservés.
pub fn ports_libres(ports: &EnginePorts) -> bool {
    let adresse = |port| SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    ports
        .tcp_ports()
        .iter()
        .all(|&p| TcpListener::bind(adresse(p)).is_ok())
        && ports
            .udp_ports()
            .iter()
            .all(|&p| UdpSocket::bind(adresse(p)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_base_est_trouvee_sur_une_machine_ordinaire() {
        assert!(base_libre().is_some());
    }

    #[test]
    fn une_base_occupee_est_ecartee() {
        let ports = EnginePorts::new(ENGINE_BASE_PORT_MIN).unwrap();
        let _occupant =
            TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, ports.http())).unwrap();
        assert!(!ports_libres(&ports));
        let trouvee = base_libre().expect("une autre base doit être trouvée");
        assert_ne!(trouvee.base(), ports.base());
    }

    #[test]
    fn les_bases_essayees_ne_se_chevauchent_pas() {
        let premiere = EnginePorts::new(ENGINE_BASE_PORT_MIN).unwrap();
        let suivante = EnginePorts::new(ENGINE_BASE_PORT_MIN + PAS).unwrap();
        assert!(
            premiere.rtsp() < suivante.https(),
            "instance à {} : ports jusqu'à {}, la suivante commence à {}",
            premiere.base(),
            premiere.rtsp(),
            suivante.https()
        );
    }

    #[test]
    fn la_plage_offre_assez_de_bases() {
        let bases = (ENGINE_BASE_PORT_MIN..=ENGINE_BASE_PORT_MAX)
            .step_by(PAS as usize)
            .count();
        assert!(bases >= 20, "seulement {bases} bases candidates");
    }
}
