//! The computer itself, a second at a time, while a session is open.
//!
//! A session going quiet leaves the same trace whether the fault is the
//! network, the far computer or this one, and every measurement the
//! service takes of the tunnel says what the tunnel did. What says what
//! this computer was doing at that second is written beside them: how
//! busy it was and with what, what the system counted of UDP and IP, and
//! what the network card that reaches the far computer carried and lost.
//! Held on both sides, since a picture is made on one computer and shown
//! on the other. What the computer is, which no second changes, is said
//! once before them.
//!
//! What is counted and how it is worded belongs to `zyr-system`, which
//! knows Windows; when to read, and for how long, belongs here. Elsewhere
//! there is nothing to read, and nothing is written.

use std::net::{IpAddr, SocketAddr};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use zyr_proto::log::Log;
use zyr_system::{Vitals, describe_this_computer};
use zyr_transport::{Junction, is_card};

/// What this module's lines are filed under.
const TAG: &str = "vitals";

/// How often the counters are read.
const EVERY: Duration = Duration::from_secs(1);

/// The counters written down for as long as this is held.
///
/// Handed to whatever owns a session, so that the reading ends exactly
/// when the session does, whichever way it ends.
pub struct Sampling {
    /// Dropping it is what tells the thread to stop.
    _stop: Sender<()>,
}

/// From now until what is returned is dropped, once a second, the
/// counters of this computer are written to the journal.
///
/// `peer` says where the session's packets go, each time it is asked: a
/// road can change during a session, and the card that carries it with it.
pub fn sample(log: &Log, peer: impl Fn() -> Option<IpAddr> + Send + 'static) -> Option<Sampling> {
    let log = log.about(TAG);
    let (stop, stopped) = mpsc::channel::<()>();
    thread::Builder::new()
        .name("zyrdeskd-vitals".to_string())
        .spawn(move || {
            // Started here, on the thread that reads: it is the one put
            // at the lowest priority.
            let Some(mut vitals) = Vitals::start() else {
                return;
            };
            if let Some(machine) = describe_this_computer() {
                log.write(&format!("this computer: {machine}"));
            }
            // Nothing is ever sent on `stopped`: it only goes away.
            vitals.second(peer());
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(EVERY) {
                if let Some(told) = vitals.second(peer()) {
                    log.debug(&told);
                }
            }
        })
        .ok()
        .map(|_| Sampling { _stop: stop })
}

/// [`sample`] for a session with the computer at `remote`, which is a
/// card when the server arranged the meeting: the road it really takes is
/// then the junction's to say.
pub fn sample_session(
    log: &Log,
    junction: Option<Junction>,
    remote: SocketAddr,
) -> Option<Sampling> {
    sample(log, move || the_road_now(junction.as_ref(), remote))
}

/// Where the packets of a session with `remote` go right now.
fn the_road_now(junction: Option<&Junction>, remote: SocketAddr) -> Option<IpAddr> {
    if !is_card(remote) {
        return Some(remote.ip());
    }
    junction?.road(remote).map(|road| road.through.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_computer_reached_at_its_own_address_is_reached_there() {
        let remote: SocketAddr = "192.168.1.5:47000".parse().unwrap();
        assert_eq!(the_road_now(None, remote), Some(remote.ip()));
    }

    #[test]
    fn a_card_with_no_junction_to_say_where_it_goes_goes_nowhere_known() {
        let card = zyr_transport::card_of(zyr_proto::fingerprint::Fingerprint::from([7; 32]));
        assert_eq!(the_road_now(None, card), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_counters_are_written_every_second_while_the_guard_is_held() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-vitals-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let path = folder.join("service.log");
        let log = Log::open(&path).expect("a journal");
        let sampling = sample(&log, || Some(IpAddr::from([127, 0, 0, 1])));
        assert!(sampling.is_some());
        thread::sleep(Duration::from_millis(2_600));
        drop(sampling);
        let written = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&folder);
        assert!(
            written.contains("[vitals] this computer: Windows "),
            "{written}"
        );
        assert!(written.contains("[vitals] over "), "{written}");
    }
}
