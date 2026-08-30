//! This computer, as everything answering for it has to see it.
//!
//! The service holds four things that outlive any one engine: whether
//! this computer can be reached, the ways out it has open, what its
//! owner asked for, and who it sees on the network. The desk answers the
//! interface from them, the door answers other computers from them, and
//! neither owns any of it.
//!
//! It is also where a journal is gathered. That page says two kinds of
//! thing: what any program on the machine can read for itself, which
//! `zyr_proto::journal` writes, and what only the service knows, which is
//! written here. Gathered in one place because it is asked for twice
//! over: by the person sitting at the machine, and by a computer reading
//! this one's journal from where it is rather than walking over.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::{Arc, Mutex};

use zyr_control::{Holdup, PROTOCOL};
use zyr_lan::Found;
use zyr_proto::journal::Journal;
use zyr_proto::log::Log;
use zyr_proto::paths;
use zyr_transport::Fingerprint;

use crate::known;
use crate::preferences::Remembered;
use crate::ways::Ways;

/// Whether this computer can be reached right now, and what is in the
/// way when it is not.
///
/// The supervisor opens it once the engine answers and the tunnel is
/// standing, and holds it back whenever something stops that from
/// happening. It is the one thing nothing else can work out on its own,
/// and the reason matters as much as the fact: an engine that is missing
/// and an engine that is starting look alike from a window, and only one
/// of the two is worth waiting for.
#[derive(Clone)]
pub struct Hosting(Arc<Mutex<Option<Holdup>>>);

impl Hosting {
    /// Not reachable yet, and nothing wrong with that.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Some(Holdup::Starting))))
    }

    /// The tunnel stands: this computer can be reached.
    pub fn open(&self) {
        *self.0.lock().expect("état de l'accès distant") = None;
    }

    /// It cannot, for this reason.
    pub fn held_by(&self, holdup: Holdup) {
        *self.0.lock().expect("état de l'accès distant") = Some(holdup);
    }

    /// What is in the way, or nothing at all.
    pub fn standing(&self) -> Option<Holdup> {
        *self.0.lock().expect("état de l'accès distant")
    }
}

impl Default for Hosting {
    fn default() -> Self {
        Self::new()
    }
}

/// What this computer holds, for as long as the service runs.
///
/// None of it belongs to one engine: reaching another computer, being
/// reachable, what its owner asked for and who is on the network all
/// outlive any number of engines starting and stopping.
#[derive(Clone)]
pub struct Machine {
    pub hosting: Hosting,
    pub ways: Ways,
    pub remembered: Remembered,
    pub neighbours: Found,
}

impl Machine {
    /// This computer's journal, heading and files.
    ///
    /// The same page whoever asked, which is the whole point of it being
    /// written here: a journal read from another computer that differed
    /// from the one read on the spot would be worth nothing to compare.
    pub fn journal(&self, fingerprint: Fingerprint, log: &Log) -> String {
        let mut journal = Journal::of_this_computer();
        journal.says(
            "Service",
            &format!("{}, dialecte {PROTOCOL}", zyr_proto::BUILD),
        );
        journal.says("Empreinte", &fingerprint.to_string());
        journal.says("Accès distant", &self.remote_access());
        journal.says(
            "Réseau local",
            if self.remembered.trust_local_network() {
                "ordinateurs de confiance"
            } else {
                "aucune confiance accordée"
            },
        );
        journal.says("Sessions ouvertes", &self.ways.count().to_string());
        journal.says("Ordinateurs vus", &self.computers_seen(log));
        journal.gathered()
    }

    /// What remote access amounts to right now, in the same words the
    /// home screen uses.
    fn remote_access(&self) -> String {
        if !self.remembered.remote_access() {
            return "désactivé".to_string();
        }
        match self.hosting.standing() {
            None => "activé, prêt à être contrôlé".to_string(),
            Some(Holdup::Starting) => "activé, démarrage en cours".to_string(),
            Some(Holdup::EngineMissing) => "activé, mais le moteur hôte est absent".to_string(),
            Some(Holdup::EngineWontStand) => "activé, mais le moteur hôte ne tient pas".to_string(),
        }
    }

    /// The computers this one shows, by name.
    ///
    /// The first thing anyone wonders when nothing appears on the home
    /// screen, and the list on screen does not say whether it is empty
    /// for want of a neighbour or for want of an answer.
    fn computers_seen(&self, log: &Log) -> String {
        let seen = self.on_screen(log);
        if seen.is_empty() {
            return "aucun".to_string();
        }
        seen.iter()
            .map(|peer| format!("{} ({})", peer.name, peer.host))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The computers the home screen shows.
    ///
    /// Those announcing themselves on the local network, and then those
    /// written down by hand that are not announcing anything. A computer
    /// on both lists is announced once: what the network says of it is
    /// fresher than what was written down months ago, its address most
    /// of all.
    pub fn on_screen(&self, log: &Log) -> Vec<zyr_control::Peer> {
        let written = match known::read(&paths::known_computers()) {
            Ok(written) => written,
            Err(e) => {
                log.write(&format!("written-down computers unreadable: {e}"));
                Vec::new()
            }
        };

        let mut shown: Vec<zyr_control::Peer> = self
            .neighbours
            .peers()
            .into_iter()
            .map(|peer| zyr_control::Peer {
                written: written
                    .iter()
                    .any(|known| known.fingerprint == peer.fingerprint),
                name: peer.name,
                fingerprint: peer.fingerprint,
                host: peer.address.to_string(),
                port: peer.port,
                seen: true,
            })
            .collect();

        for computer in written {
            if shown
                .iter()
                .any(|peer| peer.fingerprint == computer.fingerprint)
            {
                continue;
            }
            shown.push(zyr_control::Peer {
                name: computer.name,
                fingerprint: computer.fingerprint,
                host: computer.host,
                port: zyr_proto::net::TUNNEL_PORT,
                seen: false,
                written: true,
            });
        }
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(what: &str) -> (Machine, Log, std::path::PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-machine-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let log = Log::open(&folder.join("service.log")).unwrap();
        let machine = Machine {
            hosting: Hosting::new(),
            ways: Ways::new(log.clone()),
            remembered: Remembered::at(folder.join("preferences.conf")),
            neighbours: Found::new(),
        };
        (machine, log, folder)
    }

    #[test]
    fn what_stands_in_the_way_is_named_rather_than_left_to_be_guessed() {
        let (machine, _, folder) = machine("acces");

        // Un moteur absent et un moteur qui démarre se ressemblent trop
        // pour qu'un journal se contente de « pas prêt ».
        assert!(machine.remote_access().contains("démarrage"));
        machine.hosting.held_by(Holdup::EngineMissing);
        assert!(machine.remote_access().contains("absent"));
        machine.hosting.open();
        assert!(machine.remote_access().contains("prêt"));

        // Et un accès coupé exprès n'est pas une panne.
        machine.remembered.set_remote_access(false).unwrap();
        assert_eq!(machine.remote_access(), "désactivé");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_journal_says_what_only_the_service_knows() {
        let (machine, log, folder) = machine("journal");
        let fingerprint: Fingerprint =
            "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
                .parse()
                .unwrap();

        let text = machine.journal(fingerprint, &log);
        // Ce que la fenêtre ne peut pas lire seule, et qui est la moitié
        // de ce qu'on ouvre un journal pour savoir.
        assert!(text.contains(&fingerprint.to_string()), "{text}");
        assert!(text.contains(&format!("dialecte {PROTOCOL}")), "{text}");
        assert!(text.contains("Accès distant"), "{text}");
        // Sans voisin, la ligne le dit plutôt que de rester vide : une
        // liste vide et une liste absente ne se lisent pas pareil.
        assert!(text.contains("Ordinateurs vus  : aucun"), "{text}");

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
