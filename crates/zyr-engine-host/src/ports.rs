//! Picking a free port base for the host engine.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, UdpSocket};

use zyr_proto::net::{ENGINE_BASE_PORT_MAX, ENGINE_BASE_PORT_MIN, EnginePorts};

/// Gap between two bases we try.
///
/// One instance spreads its ports from the base minus 5 to the base plus
/// 21, which is 27 numbers: a shorter step would make two neighbouring
/// instances overlap.
const STEP: u16 = 32;

/// First base in the range whose derived ports are all free.
pub fn free_base() -> Option<EnginePorts> {
    (ENGINE_BASE_PORT_MIN..=ENGINE_BASE_PORT_MAX)
        .step_by(STEP as usize)
        .filter_map(|base| EnginePorts::new(base).ok())
        .find(ports_are_free)
}

/// True when the seven ports of this instance can all be reserved.
pub fn ports_are_free(ports: &EnginePorts) -> bool {
    let address = |port| SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    ports
        .tcp_ports()
        .iter()
        .all(|&port| TcpListener::bind(address(port)).is_ok())
        && ports
            .udp_ports()
            .iter()
            .all(|&port| UdpSocket::bind(address(port)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ce qui empêche les deux essais qui touchent à de vrais ports de
    /// se marcher dessus.
    ///
    /// Un port n'appartient pas au processus mais à la machine, et le
    /// lanceur d'essais fait tourner ceux d'un même binaire en parallèle.
    /// L'un des deux réserve puis relâche toute la série pour vérifier
    /// qu'elle est libre, l'autre en occupe un pour vérifier qu'elle ne
    /// l'est plus : lancés ensemble, ils se prennent le même numéro et
    /// celui qui arrive second échoue sur « adresse déjà utilisée ».
    static REAL_PORTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Le tour de chacun, sans qu'un essai déjà en échec en fasse échouer
    /// un second pour une raison qui n'est pas la sienne.
    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        REAL_PORTS.lock().unwrap_or_else(|held| held.into_inner())
    }

    #[test]
    fn a_base_is_found_on_an_ordinary_machine() {
        let _turn = one_at_a_time();
        assert!(free_base().is_some());
    }

    #[test]
    fn a_busy_base_is_skipped() {
        let _turn = one_at_a_time();
        let ports = EnginePorts::new(ENGINE_BASE_PORT_MIN).unwrap();
        let _squatter =
            TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, ports.http())).unwrap();
        assert!(!ports_are_free(&ports));
        let found = free_base().expect("another base must be found");
        assert_ne!(found.base(), ports.base());
    }

    #[test]
    fn the_bases_we_try_never_overlap() {
        let first = EnginePorts::new(ENGINE_BASE_PORT_MIN).unwrap();
        let next = EnginePorts::new(ENGINE_BASE_PORT_MIN + STEP).unwrap();
        assert!(
            first.rtsp() < next.https(),
            "instance at {}: ports up to {}, the next one starts at {}",
            first.base(),
            first.rtsp(),
            next.https()
        );
    }

    #[test]
    fn the_range_offers_enough_bases() {
        let bases = (ENGINE_BASE_PORT_MIN..=ENGINE_BASE_PORT_MAX)
            .step_by(STEP as usize)
            .count();
        assert!(bases >= 20, "only {bases} candidate bases");
    }
}
