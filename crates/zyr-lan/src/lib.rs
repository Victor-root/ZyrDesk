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

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use zyr_proto::net::TUNNEL_PORT;
use zyr_transport::Fingerprint;

/// The kind of service ZyrDesk answers to, in the shape mDNS expects.
///
/// The tunnel speaks UDP, and what is announced here is the tunnel: the
/// engines are never reachable from the network.
const SERVICE: &str = "_zyrdesk._udp.local.";

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

    fn note(&self, peer: Peer, now: Instant) {
        self.0
            .lock()
            .expect("found peers")
            .insert(peer.fingerprint, (peer, now));
    }

    fn forget(&self, name: &str) {
        // The goodbye carries the announced name, not the fingerprint,
        // so the entry is found by what it was announced under.
        self.0
            .lock()
            .expect("found peers")
            .retain(|_, (peer, _)| announced_as(&peer.fingerprint) != name);
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
    pub fn open(name: &str, fingerprint: Fingerprint) -> Result<Self, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        let instance = announced_as(&fingerprint);

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
            while let Ok(event) = listening.recv() {
                collect(event, &collecting, mine);
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

/// Turns what the network said into what we keep, or ignores it.
fn collect(event: ServiceEvent, found: &Found, mine: Fingerprint) {
    match event {
        ServiceEvent::ServiceResolved(info) => {
            let Some(peer) = read(&info) else {
                return;
            };
            // This computer hears its own announcement: showing it in
            // the list of others would be absurd.
            if peer.fingerprint == mine {
                return;
            }
            found.note(peer, Instant::now());
        }
        ServiceEvent::ServiceRemoved(_, name) => {
            found.forget(name.split('.').next().unwrap_or(&name));
        }
        _ => {}
    }
}

/// Reads a record, keeping only what is complete and makes sense.
///
/// Anything on the network can announce anything: a record missing a
/// field, or carrying something that is not a fingerprint, is dropped
/// rather than shown half-empty.
fn read(info: &mdns_sd::ResolvedService) -> Option<Peer> {
    let fingerprint: Fingerprint = info.get_property_val_str(CLE_EMPREINTE)?.parse().ok()?;
    let name = info.get_property_val_str(CLE_NOM)?.trim();
    if name.is_empty() {
        return None;
    }
    // Une machine annonce toutes ses adresses. La première d'entre
    // elles en version 4 est celle que le tunnel saura joindre.
    let address = info
        .addresses
        .iter()
        .map(|scoped| scoped.to_ip_addr())
        .find(IpAddr::is_ipv4)
        .or_else(|| {
            info.addresses
                .iter()
                .map(|scoped| scoped.to_ip_addr())
                .next()
        })?;
    Some(Peer {
        name: name.to_string(),
        fingerprint,
        address,
        port: info.port,
    })
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
    fn a_computer_is_announced_under_something_unique() {
        // Two machines called the same thing are common; two identities
        // the same are not.
        assert_ne!(announced_as(&fingerprint(1)), announced_as(&fingerprint(2)));
    }
}
