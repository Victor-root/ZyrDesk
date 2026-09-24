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

    /// What stops the two tests that touch real ports from treading on
    /// each other.
    ///
    /// A port belongs not to the process but to the machine, and the test
    /// runner runs the tests of one binary in parallel. One of the two
    /// reserves and then releases the whole series to check that it is
    /// free, the other occupies one of them to check that it no longer
    /// is: started together, they grab the same number from each other
    /// and the one that comes second fails on "address already in use".
    static REAL_PORTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Each one's turn, without a test that has already failed making a
    /// second one fail for a reason that is not its own.
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
