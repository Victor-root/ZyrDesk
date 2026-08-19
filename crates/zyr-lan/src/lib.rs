//! Announcing this computer on the local network, and finding the others.
//!
//! Two computers on the same network should find each other without
//! anyone reading an address out loud. That is all this does: it says
//! « ZyrDesk is here, on this address, and this is its fingerprint »,
//! and it collects what the others say.
//!
//! What it deliberately does not do is decide anything. A computer found
//! here is a computer that exists, not a computer that is allowed: the
//! fingerprint still has to be recognised before a session opens. The
//! announcement carries no secret, only what is already public to
//! anyone who can reach the machine.
//!
//! It lives in the service rather than in the interface: the service is
//! what runs when nobody has opened a window, and a computer that only
//! appears once its owner opens the interface would be useless.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mdns_sd::{DaemonEvent, ServiceDaemon, ServiceEvent, ServiceInfo};
use zyr_proto::net::TUNNEL_PORT;
use zyr_transport::Fingerprint;

/// The kind of service ZyrDesk answers to, in the shape mDNS expects.
///
/// The tunnel speaks UDP, and what is announced here is the tunnel: the
/// engines are never reachable from the network.
const SERVICE: &str = "_zyrdesk._udp.local.";

/// Port mDNS itself uses, which nobody gets to choose.
///
/// Named here because the firewall has to let it through: a computer
/// that cannot hear the announcements never finds anyone, and that looks
/// exactly like a product that does not work.
pub const PORT: u16 = 5353;

/// Key the fingerprint travels under.
const CLE_EMPREINTE: &str = "fp";

/// Key the machine's name travels under.
const CLE_NOM: &str = "nom";

/// How long a computer stays listed after it was last heard from.
///
/// mDNS says goodbye when a machine leaves properly, but a machine
/// unplugged, asleep or crashed says nothing at all. Without this, the
/// list would only ever grow.
const OUBLI: Duration = Duration::from_secs(90);

/// A ZyrDesk found on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// Name its owner knows it by.
    pub name: String,
    /// What has to be recognised before anything opens.
    pub fingerprint: Fingerprint,
    /// Where to reach it.
    pub address: IpAddr,
    pub port: u16,
}

/// Everything found so far, forgotten when it stops answering.
#[derive(Clone, Default)]
pub struct Found(Arc<Mutex<HashMap<Fingerprint, (Peer, Instant)>>>);

impl Found {
    pub fn new() -> Self {
        Self::default()
    }

    /// Computers heard from recently enough to be believed.
    pub fn peers(&self) -> Vec<Peer> {
        self.seen_at(Instant::now())
    }

    fn seen_at(&self, now: Instant) -> Vec<Peer> {
        let mut found = self.0.lock().expect("found peers");
        found.retain(|_, (_, seen)| now.duration_since(*seen) < OUBLI);
        let mut peers: Vec<Peer> = found.values().map(|(peer, _)| peer.clone()).collect();
        // Rangés par nom : une carte qui change de place à chaque
        // rafraîchissement est insupportable à l'usage, et rien dans
        // l'ordre d'arrivée ne veut dire quoi que ce soit.
        peers.sort_by(|a, b| a.name.cmp(&b.name).then(a.address.cmp(&b.address)));
        peers
    }

    /// Writes a computer down. `true` when it was not already there.
    ///
    /// A machine re-announces itself for as long as it runs, so only the
    /// first time is worth telling anyone about.
    fn note(&self, peer: Peer, now: Instant) -> bool {
        self.0
            .lock()
            .expect("found peers")
            .insert(peer.fingerprint, (peer, now))
            .is_none()
    }

    /// Takes a computer off the list, and hands back the name it was
    /// known by when there was one to take off.
    fn forget(&self, name: &str) -> Option<String> {
        // The goodbye carries the announced name, not the fingerprint,
        // so the entry is found by what it was announced under.
        let mut found = self.0.lock().expect("found peers");
        let gone = found
            .values()
            .find(|(peer, _)| announced_as(&peer.fingerprint) == name)
            .map(|(peer, _)| peer.name.clone())?;
        found.retain(|_, (peer, _)| announced_as(&peer.fingerprint) != name);
        Some(gone)
    }
}

/// Name this computer is announced under.
///
/// The fingerprint rather than the machine name: two computers may well
/// be called the same thing, and mDNS needs the name to be unique. What
/// a person reads travels in the record's fields instead.
fn announced_as(fingerprint: &Fingerprint) -> String {
    fingerprint.to_string()
}

/// The announcement and the search, for as long as this is held.
///
/// Dropping it takes this computer off the network properly, so the
/// others stop showing it at once instead of waiting to forget it.
pub struct Neighbourhood {
    daemon: ServiceDaemon,
    announced: String,
    found: Found,
}

impl Neighbourhood {
    /// Says this computer is here, and starts listening for others.
    ///
    /// `noticed` is told what the network carries, as it arrives. Without
    /// it, a computer that never appears looks exactly like a computer
    /// that is switched off, and there is nothing to tell the two apart
    /// from the outside.
    pub fn open(
        name: &str,
        fingerprint: Fingerprint,
        noticed: impl Fn(&str) + Send + Sync + 'static,
    ) -> Result<Self, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        let instance = announced_as(&fingerprint);
        let noticed = Arc::new(noticed);

        // What the announcement itself is doing, before anything is
        // heard back. A computer can announce itself perfectly and be
        // announcing it on a card nobody else is on, and from both ends
        // that looks exactly like a computer switched off.
        let watching = daemon.monitor()?;
        let telling = noticed.clone();
        std::thread::spawn(move || watch(&watching, telling.as_ref()));

        let mut fields = std::collections::HashMap::new();
        fields.insert(CLE_EMPREINTE.to_string(), fingerprint.to_string());
        fields.insert(CLE_NOM.to_string(), name.to_string());

        // The addresses are left to the library: it knows the interfaces
        // this machine actually has, and keeps up when one appears.
        let announcement = ServiceInfo::new(
            SERVICE,
            &instance,
            &format!("{instance}.local."),
            (),
            TUNNEL_PORT,
            fields,
        )?
        .enable_addr_auto();
        daemon.register(announcement)?;

        let found = Found::new();
        let listening = daemon.browse(SERVICE)?;
        let collecting = found.clone();
        let mine = fingerprint;
        std::thread::spawn(move || {
            // What has already been mentioned, so that a machine
            // re-announcing itself every minute does not fill the
            // journal with the same line. One entry per computer on the
            // network, which is a handful at worst.
            let mut mentioned = std::collections::HashSet::new();
            while let Ok(event) = listening.recv() {
                collect(event, &collecting, mine, &mut mentioned, noticed.as_ref());
            }
        });

        Ok(Self {
            daemon,
            announced: instance,
            found,
        })
    }

    /// What has been found so far.
    pub fn found(&self) -> Found {
        self.found.clone()
    }
}

impl Drop for Neighbourhood {
    fn drop(&mut self) {
        let _ = self
            .daemon
            .unregister(&format!("{}.{SERVICE}", self.announced));
        let _ = self.daemon.shutdown();
    }
}

/// Reports what the announcement itself is doing.
///
/// The other half of the diagnosis. The lines below say which addresses
/// this computer is announcing on and which cards the announcements
/// leave by, so a machine announcing itself only on a virtual adapter or
/// a VPN says so instead of looking switched off from every side.
///
/// An announcement goes out again for as long as the service runs, and
/// what is worth reading is the list of cards, not how many times each
/// was used: each is said once and then left alone. Addresses appearing
/// and disappearing are rare enough to be said every time.
fn watch(watching: &mdns_sd::Receiver<DaemonEvent>, noticed: &dyn Fn(&str)) {
    let mut already = std::collections::HashSet::new();
    while let Ok(event) = watching.recv() {
        let (line, every_time) = match event {
            DaemonEvent::IpAdd(address) => (format!("announcing on {address}"), true),
            DaemonEvent::IpDel(address) => (format!("{address} is no longer answering"), true),
            DaemonEvent::Announce(_, whence) => (format!("announcement sent from {whence}"), false),
            DaemonEvent::Respond(whence) => {
                (format!("a question was answered from {whence}"), false)
            }
            DaemonEvent::Error(e) => (format!("the local network refused something: {e}"), false),
            _ => continue,
        };
        if every_time || already.insert(line.clone()) {
            noticed(&line);
        }
    }
}

/// Turns what the network said into what we keep, or ignores it.
///
/// What is worth telling apart, and why each stage is reported: hearing
/// an announcement proves the network carries them at all, and reading
/// one proves it carries ours. A computer that never appears is one of
/// two very different faults, and only these lines say which.
fn collect(
    event: ServiceEvent,
    found: &Found,
    mine: Fingerprint,
    mentioned: &mut std::collections::HashSet<String>,
    noticed: &dyn Fn(&str),
) {
    match event {
        ServiceEvent::ServiceFound(_, fullname) => {
            let instance = instance_of(&fullname).to_string();
            if instance != announced_as(&mine) && mentioned.insert(instance.clone()) {
                noticed(&format!("an announcement was heard from {instance}"));
            }
        }
        ServiceEvent::ServiceResolved(info) => {
            let Some((peer, announced)) = read(&info) else {
                noticed("an announcement arrived incomplete and was left aside");
                return;
            };
            // This computer hears its own announcement: showing it in
            // the list of others would be absurd.
            if peer.fingerprint == mine {
                return;
            }
            let named = named(&peer, &announced);
            if found.note(peer, Instant::now()) {
                noticed(&format!("found {named} on the local network"));
            }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            let instance = instance_of(&fullname);
            mentioned.remove(instance);
            if let Some(gone) = found.forget(instance) {
                noticed(&format!("{gone} left the local network"));
            }
        }
        _ => {}
    }
}

/// The instance part of a full mDNS name, which is what a computer is
/// announced under.
fn instance_of(fullname: &str) -> &str {
    fullname.split('.').next().unwrap_or(fullname)
}

/// Reads a record, keeping only what is complete and makes sense.
///
/// Anything on the network can announce anything: a record missing a
/// field, or carrying something that is not a fingerprint, is dropped
/// rather than shown half-empty.
///
/// Hands back the computer and every address it announced: the second is
/// worth writing down even though only the first is used, since a
/// machine reached at the wrong one of its addresses is otherwise a
/// silent failure with nothing to look at.
fn read(info: &mdns_sd::ResolvedService) -> Option<(Peer, Vec<IpAddr>)> {
    let fingerprint: Fingerprint = info.get_property_val_str(CLE_EMPREINTE)?.parse().ok()?;
    let name = info.get_property_val_str(CLE_NOM)?.trim();
    if name.is_empty() {
        return None;
    }
    let announced = in_order(
        info.addresses
            .iter()
            .map(|scoped| scoped.to_ip_addr())
            .collect(),
    );
    let peer = Peer {
        name: name.to_string(),
        fingerprint,
        address: *announced.first()?,
        port: info.port,
    };
    Some((peer, announced))
}

/// Puts the addresses a computer announced into a settled order.
///
/// A machine with a second card, a virtual adapter or a VPN announces
/// every address it has, and they arrive as a set, in no order at all:
/// taking « the first » would reach the same computer somewhere else
/// from one time to the next. Version four leads, being what the tunnel
/// is opened on.
fn in_order(mut announced: Vec<IpAddr>) -> Vec<IpAddr> {
    announced.sort_by_key(|address| (!address.is_ipv4(), *address));
    announced
}

/// How a computer just found is named in the journal.
///
/// Every address it announced, and not only the one that will be used:
/// the day a computer is reached at the wrong one of its addresses, this
/// line is what says so.
fn named(peer: &Peer, announced: &[IpAddr]) -> String {
    let elsewhere: Vec<String> = announced.iter().skip(1).map(ToString::to_string).collect();
    if elsewhere.is_empty() {
        return format!("{} at {}", peer.name, peer.address);
    }
    format!(
        "{} at {}, also announced at {}",
        peer.name,
        peer.address,
        elsewhere.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        // Écrite puis relue, comme elle voyage sur le réseau : c'est le
        // seul chemin par lequel une empreinte entre dans ce module.
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    fn peer(seed: u8, name: &str) -> Peer {
        Peer {
            name: name.to_string(),
            fingerprint: fingerprint(seed),
            address: "192.168.1.20".parse().unwrap(),
            port: TUNNEL_PORT,
        }
    }

    #[test]
    fn a_computer_heard_from_is_listed() {
        let found = Found::new();
        found.note(peer(1, "PC-BUREAU"), Instant::now());
        assert_eq!(found.peers().len(), 1);
        assert_eq!(found.peers()[0].name, "PC-BUREAU");
    }

    #[test]
    fn only_the_first_sight_of_a_computer_is_worth_reporting() {
        // Une machine se réannonce tant qu'elle tourne : sans ça, le
        // journal se remplirait de la même ligne toutes les minutes.
        let found = Found::new();
        assert!(found.note(peer(1, "PC-BUREAU"), Instant::now()));
        assert!(!found.note(peer(1, "PC-BUREAU"), Instant::now()));
    }

    #[test]
    fn a_departure_names_who_left_and_stays_quiet_otherwise() {
        let found = Found::new();
        found.note(peer(1, "PC-BUREAU"), Instant::now());

        assert_eq!(
            found.forget(&announced_as(&fingerprint(1))).as_deref(),
            Some("PC-BUREAU")
        );
        // Un départ annoncé deux fois, ou celui d'une machine qu'on n'a
        // jamais vue, ne doit rien raconter du tout.
        assert!(found.forget(&announced_as(&fingerprint(1))).is_none());
        assert!(found.forget("une-machine-inconnue").is_none());
    }

    #[test]
    fn a_full_mdns_name_gives_back_what_the_computer_is_announced_under() {
        let announced = announced_as(&fingerprint(1));
        assert_eq!(instance_of(&format!("{announced}.{SERVICE}")), announced);
        assert_eq!(instance_of("sans-point"), "sans-point");
    }

    #[test]
    fn the_same_computer_twice_is_one_computer() {
        // A machine re-announces itself regularly, and answers every
        // question asked of the network: without the fingerprint as the
        // key, the list would fill up with copies.
        let found = Found::new();
        found.note(peer(1, "PC-BUREAU"), Instant::now());
        found.note(peer(1, "PC-BUREAU"), Instant::now());
        assert_eq!(found.peers().len(), 1);
    }

    #[test]
    fn a_renamed_computer_keeps_its_place() {
        let found = Found::new();
        found.note(peer(1, "PC-BUREAU"), Instant::now());
        found.note(peer(1, "PC-SALON"), Instant::now());
        assert_eq!(found.peers().len(), 1);
        assert_eq!(found.peers()[0].name, "PC-SALON");
    }

    #[test]
    fn a_computer_that_stops_answering_is_forgotten() {
        // Unplugged, asleep or crashed, it says nothing on its way out.
        let found = Found::new();
        let now = Instant::now();
        found.note(peer(1, "PC-BUREAU"), now - OUBLI - Duration::from_secs(1));
        found.note(peer(2, "PC-SALON"), now);
        let still_there = found.seen_at(now);
        assert_eq!(still_there.len(), 1);
        assert_eq!(still_there[0].name, "PC-SALON");
    }

    #[test]
    fn a_goodbye_takes_the_computer_off_the_list() {
        let found = Found::new();
        found.note(peer(1, "PC-BUREAU"), Instant::now());
        found.forget(&announced_as(&fingerprint(1)));
        assert!(found.peers().is_empty());
    }

    #[test]
    fn a_computer_with_several_cards_is_always_reached_at_the_same_one() {
        // Une machine à deux cartes annonce ses deux adresses, et elles
        // arrivent en vrac : sans ordre arrêté, on la joindrait tantôt
        // d'un côté tantôt de l'autre, et un essai sur deux échouerait
        // sans que rien ne l'explique.
        let un: IpAddr = "192.168.1.20".parse().unwrap();
        let deux: IpAddr = "192.168.2.20".parse().unwrap();
        let six: IpAddr = "fe80::1".parse().unwrap();
        assert_eq!(in_order(vec![deux, un]), vec![un, deux]);
        assert_eq!(in_order(vec![six, deux, un]), vec![un, deux, six]);
        // La version quatre d'abord : c'est là-dessus que le tunnel est
        // ouvert.
        assert_eq!(in_order(vec![six, deux]), vec![deux, six]);
    }

    #[test]
    fn a_computer_found_says_where_else_it_answers() {
        let un: IpAddr = "192.168.1.20".parse().unwrap();
        let deux: IpAddr = "192.168.2.20".parse().unwrap();
        let found = peer(1, "PC-BUREAU");
        assert_eq!(named(&found, &[un]), "PC-BUREAU at 192.168.1.20");
        assert_eq!(
            named(&found, &[un, deux]),
            "PC-BUREAU at 192.168.1.20, also announced at 192.168.2.20"
        );
    }

    #[test]
    fn a_computer_is_announced_under_something_unique() {
        // Two machines called the same thing are common; two identities
        // the same are not.
        assert_ne!(announced_as(&fingerprint(1)), announced_as(&fingerprint(2)));
    }
}
