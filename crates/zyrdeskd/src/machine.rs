//! This computer, as everything answering for it has to see it.
//!
//! The service holds five things that outlive any one engine: whether
//! this computer can be reached, the ways out it has open, what its
//! owner asked for, who it sees on the network, and the account it is
//! attached to when it is. The desk answers the interface from them, the
//! door answers other computers from them, and neither owns any of it.
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

use zyr_account::Snapshot;
use zyr_broker::rest::DeviceInfo;
use zyr_control::{Holdup, OfAccount, PROTOCOL};
use zyr_lan::Found;
use zyr_proto::journal::Journal;
use zyr_proto::log::Log;
use zyr_proto::net::TUNNEL_PORT;
use zyr_proto::paths;
use zyr_transport::{Fingerprint, Junction, Media};

use crate::account::{self, Account};
use crate::known::{self, Known};
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

/// The junction the door stands on, for as long as the door is open,
/// and what the sessions coming through it ask to be served.
///
/// Held here because the door comes and goes with the engine, and the
/// account outlives both: when the server presents a computer, it is
/// the junction of the moment that has to expect it, and the branch of
/// relay opened for that computer carries the same session as the
/// tunnel, so it is sized on the same thing.
#[derive(Clone, Default)]
pub struct Door {
    junction: Arc<Mutex<Option<Junction>>>,
    media: Media,
}

impl Door {
    pub fn opened(&self, junction: Junction) {
        *self.junction.lock().expect("porte") = Some(junction);
    }

    pub fn closed(&self) {
        *self.junction.lock().expect("porte") = None;
    }

    /// The junction of the open door, or nothing while it is closed.
    pub fn junction(&self) -> Option<Junction> {
        self.junction.lock().expect("porte").clone()
    }

    /// What the tunnel and its branches of relay are being asked to
    /// carry. Handed out to be told, and to be read by the transport.
    pub fn media(&self) -> Media {
        self.media.clone()
    }
}

/// What this computer holds, for as long as the service runs.
///
/// None of it belongs to one engine: reaching another computer, being
/// reachable, what its owner asked for, who is on the network and the
/// account it is attached to all outlive any number of engines starting
/// and stopping.
#[derive(Clone)]
pub struct Machine {
    pub hosting: Hosting,
    pub ways: Ways,
    pub remembered: Remembered,
    pub neighbours: Found,
    pub account: Account,
    pub door: Door,
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
        journal.says("Compte", &self.account_line());
        journal.says("Sessions ouvertes", &self.ways.count().to_string());
        journal.says("Ordinateurs vus", &self.computers_seen(log));
        journal.gathered()
    }

    /// The link to an account, as it stands, in one line.
    ///
    /// A server that cannot be reached is named with the reason: two
    /// computers of one account that never see each other is a fault
    /// whose whole explanation is on this line, on one of the two.
    fn account_line(&self) -> String {
        let Some(account) = self.account.standing() else {
            return "aucun".to_string();
        };
        let state = if account.connected {
            "relié".to_string()
        } else {
            format!(
                "injoignable{}",
                account
                    .trouble
                    .map_or_else(String::new, |why| format!(" : {}", why.replace('\n', " ")))
            )
        };
        format!("{} sur {}, {state}", account.username, account.server)
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
    /// Those announcing themselves on the local network, then those
    /// written down by hand that are not announcing anything, then those
    /// the account names that neither carries. A computer on several
    /// lists is one card: what the network says of it is fresher than
    /// what was written down months ago, its address most of all, and
    /// the account's word goes on that card rather than on a second one.
    pub fn on_screen(&self, log: &Log) -> Vec<zyr_control::Peer> {
        let written = match known::read(&paths::known_computers()) {
            Ok(written) => written,
            Err(e) => {
                log.write(&format!("written-down computers unreadable: {e}"));
                Vec::new()
            }
        };
        merged(
            self.neighbours.peers(),
            written,
            self.account.snapshot().as_ref(),
        )
    }
}

/// The computers of the three lists, as one list.
fn merged(
    seen: Vec<zyr_lan::Peer>,
    written: Vec<Known>,
    account: Option<&Snapshot>,
) -> Vec<zyr_control::Peer> {
    let of_account = |fingerprint: Fingerprint| -> Option<OfAccount> {
        every_device_of(account?)
            .find(|(device, _)| device.fingerprint == fingerprint)
            .map(|(device, shared_by)| OfAccount {
                device: device.id.clone(),
                online: device.online,
                access: device.access,
                shared_by,
            })
    };

    let mut shown: Vec<zyr_control::Peer> = seen
        .into_iter()
        .map(|peer| zyr_control::Peer {
            written: written
                .iter()
                .any(|known| known.fingerprint == peer.fingerprint),
            account: of_account(peer.fingerprint),
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
            account: of_account(computer.fingerprint),
            name: computer.name,
            fingerprint: computer.fingerprint,
            host: computer.host,
            port: TUNNEL_PORT,
            seen: false,
            written: true,
        });
    }

    // Reached by its road at the server, since nothing here knows an
    // address for it: the desk asks the account for the meeting.
    for (device, shared_by) in account.into_iter().flat_map(every_device_of) {
        if shown
            .iter()
            .any(|peer| peer.fingerprint == device.fingerprint)
        {
            continue;
        }
        shown.push(zyr_control::Peer {
            name: device.name.clone(),
            fingerprint: device.fingerprint,
            host: account::road_to(&device.id),
            port: TUNNEL_PORT,
            seen: false,
            written: false,
            account: Some(OfAccount {
                device: device.id.clone(),
                online: device.online,
                access: device.access,
                shared_by,
            }),
        });
    }
    shown
}

/// Every computer the account names, this one left out: its own, then
/// those shared with it, each with the username of its owner.
fn every_device_of(snapshot: &Snapshot) -> impl Iterator<Item = (&DeviceInfo, Option<String>)> {
    snapshot
        .devices
        .iter()
        .filter(|device| Some(&device.id) != snapshot.me.as_ref())
        .map(|device| (device, None))
        .chain(
            snapshot
                .shares
                .iter()
                .map(|share| (&share.device, Some(share.owner.clone()))),
        )
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
            account: Account::at(folder.join("account.conf"), log.clone()),
            door: Door::default(),
        };
        (machine, log, folder)
    }

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    fn device(id: &str, seed: u8, name: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.to_string(),
            name: name.to_string(),
            fingerprint: fingerprint(seed),
            online: true,
            access: zyr_broker::rest::Access::Ready,
            last_seen: None,
        }
    }

    #[test]
    fn the_account_is_the_third_place_a_computer_comes_from() {
        let address: std::net::IpAddr = "192.168.1.20".parse().unwrap();
        let seen = vec![zyr_lan::Peer {
            name: "PC-BUREAU".to_string(),
            fingerprint: fingerprint(1),
            address,
            addresses: vec![address],
            port: TUNNEL_PORT,
        }];
        let snapshot = Snapshot {
            me: Some("d0".to_string()),
            devices: vec![
                device("d0", 9, "Moi"),
                device("d1", 1, "PC du bureau"),
                device("d2", 2, "Portable"),
            ],
            shares: vec![zyr_broker::rest::ShareInfo {
                id: "p1".to_string(),
                device: device("d7", 7, "PC de l'atelier"),
                owner: "ami".to_string(),
                with: "victor".to_string(),
                permissions: zyr_broker::rest::Permission::ALL.to_vec(),
                expires: None,
                created: 1,
            }],
            ..Snapshot::default()
        };

        let shown = merged(seen, Vec::new(), Some(&snapshot));
        // Le PC vu sur le réseau et rattaché au compte est une seule
        // carte : l'adresse du réseau, et le mot du compte dessus.
        assert_eq!(shown.len(), 3, "{shown:?}");
        assert_eq!(shown[0].name, "PC-BUREAU");
        assert_eq!(shown[0].host, "192.168.1.20");
        assert_eq!(
            shown[0].account.as_ref().map(|it| it.device.as_str()),
            Some("d1")
        );
        // Le portable, que rien d'autre ne porte, est joint par sa route
        // chez le serveur.
        assert_eq!(shown[1].name, "Portable");
        assert_eq!(shown[1].host, "account:d2");
        assert!(!shown[1].seen && !shown[1].written);
        // Et la machine partagée dit par qui.
        assert_eq!(
            shown[2]
                .account
                .as_ref()
                .and_then(|it| it.shared_by.clone()),
            Some("ami".to_string())
        );
        // Cet ordinateur-ci n'est pas une carte : on ne se connecte pas à
        // soi-même.
        assert!(shown.iter().all(|peer| peer.name != "Moi"));

        // Sans compte, rien ne change de ce qui existait.
        let shown = merged(Vec::new(), Vec::new(), None);
        assert!(shown.is_empty());
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
        // liste vide et une liste absente ne se lisent pas pareil. Le
        // compte pareil.
        assert!(text.contains("Ordinateurs vus  : aucun"), "{text}");
        assert!(text.contains("Compte"), "{text}");
        assert_eq!(machine.account_line(), "aucun");

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
