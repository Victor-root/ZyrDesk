//! The ways out this computer holds.
//!
//! A way is one tunnel towards one remote computer, with the local
//! addresses standing in for its engine. The service holds it, not the
//! program that asked for it: that is what lets an interface be closed,
//! or crash, without taking the session down with it.
//!
//! A way that nobody uses is a leak. Each one is therefore tied to the
//! process it serves, and closes on its own once that process is gone.
//! One that is never tied to anything is closed too, after a short
//! grace period, since whoever asked for it never came back.
//!
//! Each way also remembers which computer it leads to. That is what an
//! interface opened in the middle of a session reads to name it: it was
//! not there when it started, and nothing else survived.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zyr_control::{Reached, Session, WayId};
use zyr_proto::net::{TUNNEL_PORT, device_loopback_addr};
use zyr_proto::paths;
use zyr_transport::{Fingerprint, Identity, MediaProfile, TunnelEndpoint, packet_size};
use zyr_tunnel::{Tunnel, greeting};

use crate::log::Log;

/// Where the tunnel leaves from: any interface, any port.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Time left to path discovery before the packet size is fixed.
///
/// The engine keeps that size for the whole session and cannot change it
/// along the way, so it is worth a moment's wait.
const PATH_DISCOVERY: Duration = Duration::from_secs(2);

/// How long a way may stay tied to nothing before it is closed.
///
/// It covers the moment between the way being opened and the player
/// being started with it. Whoever asked has that long to come back.
const GRACE: Duration = Duration::from_secs(30);

/// How often the ways are looked over.
const SWEEP: Duration = Duration::from_secs(2);

/// What one way out is made of. Dropping it closes it.
struct Open {
    _tunnel: Tunnel,
    _endpoint: TunnelEndpoint,
}

/// The computer a way leads to.
///
/// Kept because the service is the only thing that outlives everything:
/// an interface opened after the session started never knew where it
/// was going, and has to be told.
struct Towards {
    /// Address of the remote computer, as the person named it.
    host: String,
    peer: Fingerprint,
}

/// The process a way serves, and since when.
struct Serving {
    process: u32,
    since: Instant,
}

/// One way, and what it is for.
struct Kept<T> {
    thing: T,
    towards: Towards,
    /// Local address it took, so it can be given back.
    device: u16,
    /// Process it serves, once tied to one.
    user: Option<Serving>,
    opened: Instant,
}

/// The ways, their addresses, and who they are for.
///
/// Nothing here touches the network: it is the bookkeeping alone, so it
/// can be checked without two computers.
struct Register<T> {
    kept: HashMap<WayId, Kept<T>>,
    /// Addresses handed out, including those still being opened.
    taken: HashSet<u16>,
    next: u64,
}

impl<T> Register<T> {
    fn new() -> Self {
        Self {
            kept: HashMap::new(),
            taken: HashSet::new(),
            next: 1,
        }
    }

    /// Takes the lowest free local address.
    ///
    /// Lowest rather than next: the addresses are what the client engine
    /// remembers a computer by, and reusing them keeps its stored state
    /// from growing without end.
    fn reserve(&mut self) -> Option<u16> {
        let device = (0..u16::MAX).find(|index| !self.taken.contains(index))?;
        device_loopback_addr(device)?;
        self.taken.insert(device);
        Some(device)
    }

    /// Gives an address back without ever having used it.
    fn give_back(&mut self, device: u16) {
        self.taken.remove(&device);
    }

    /// Writes down a way that is now open.
    fn settle(&mut self, device: u16, towards: Towards, thing: T) -> WayId {
        let way = WayId(self.next);
        self.next += 1;
        self.kept.insert(
            way,
            Kept {
                thing,
                towards,
                device,
                user: None,
                opened: Instant::now(),
            },
        );
        way
    }

    /// Ties a way to the process using it.
    fn hold(&mut self, way: WayId, process: u32) -> bool {
        match self.kept.get_mut(&way) {
            Some(kept) => {
                kept.user = Some(Serving {
                    process,
                    since: Instant::now(),
                });
                true
            }
            None => false,
        }
    }

    /// Takes a way out of the register, handing back what has to be
    /// dropped. Dropping it here would mean closing a tunnel while
    /// holding the lock every other way is waiting on.
    fn release(&mut self, way: WayId) -> Option<T> {
        let kept = self.kept.remove(&way)?;
        self.taken.remove(&kept.device);
        Some(kept.thing)
    }

    /// Ways with nothing left to serve.
    fn finished(&self, alive: impl Fn(u32) -> bool, now: Instant) -> Vec<WayId> {
        self.kept
            .iter()
            .filter(|(_, kept)| match &kept.user {
                Some(serving) => !alive(serving.process),
                None => now.duration_since(kept.opened) > GRACE,
            })
            .map(|(way, _)| *way)
            .collect()
    }

    /// The sessions being served, oldest way first.
    ///
    /// A way nobody has claimed yet is left out: it is an attempt under
    /// way, watched by whoever started it, and gone on its own if they
    /// never come back. Announcing it as a session would put a picture
    /// on screen where there is none.
    fn held(&self, now: Instant) -> Vec<Session> {
        let mut sessions: Vec<Session> = self
            .kept
            .iter()
            .filter_map(|(way, kept)| {
                let serving = kept.user.as_ref()?;
                Some(Session {
                    way: *way,
                    towards: kept.towards.host.clone(),
                    peer: kept.towards.peer,
                    since: now.duration_since(serving.since),
                })
            })
            .collect();
        // The register is a map, and its order changes on its own. A
        // list that reshuffles between two questions would redraw the
        // interface for nothing.
        sessions.sort_by_key(|session| session.way);
        sessions
    }

    fn count(&self) -> usize {
        self.kept.len()
    }
}

/// The ways out, as the service holds them.
#[derive(Clone)]
pub struct Ways {
    register: Arc<Mutex<Register<Open>>>,
    alive: fn(u32) -> bool,
    log: Log,
}

impl Ways {
    pub fn new(log: Log) -> Self {
        Self {
            register: Arc::new(Mutex::new(Register::new())),
            alive: still_running,
            log,
        }
    }

    /// Opens a way to a remote computer, and keeps it.
    pub async fn open(
        &self,
        host: &str,
        peer: Fingerprint,
        media: MediaProfile,
    ) -> Result<Reached, String> {
        let remote = resolve(host)?;
        let identity =
            Identity::load_or_create(&paths::identity_dir()).map_err(|e| e.to_string())?;

        let device = self
            .register
            .lock()
            .expect("registre des voies")
            .reserve()
            .ok_or("plus d'adresse locale disponible pour une session de plus")?;

        match self.dig(remote, host, peer, media, device, &identity).await {
            Ok(reached) => Ok(reached),
            Err(e) => {
                self.register
                    .lock()
                    .expect("registre des voies")
                    .give_back(device);
                Err(e)
            }
        }
    }

    /// Everything between the address being taken and the way being
    /// written down. Kept apart so a failure anywhere gives the address
    /// back exactly once.
    async fn dig(
        &self,
        remote: SocketAddr,
        host: &str,
        peer: Fingerprint,
        media: MediaProfile,
        device: u16,
        identity: &Identity,
    ) -> Result<Reached, String> {
        let endpoint =
            TunnelEndpoint::client(identity, peer, media, SocketAddr::new(EVERY_INTERFACE, 0))
                .map_err(|e| e.to_string())?;

        let connection = endpoint
            .connect(remote)
            .await
            .map_err(|e| format!("{host} ne répond pas sur le port {TUNNEL_PORT} : {e}"))?;

        // The first real exchange, and the moment authorisation is
        // proven: a connection succeeds before the other computer has
        // judged our certificate, so nothing may be announced as
        // established until this answers.
        let greeting = greeting::ask(&connection).await.map_err(|e| {
            format!(
                "{host} a refusé cet ordinateur, ou son empreinte a changé.\n  \
                 Sur {host} : zyr-cli host authorize {}\n  Détail : {e}",
                identity.fingerprint()
            )
        })?;

        let usable = connection
            .settled_usable_datagram(PATH_DISCOVERY)
            .await
            .ok_or("le chemin n'annonce aucune taille de datagramme")?;
        let packet = packet_size(usable).map_err(|e| e.to_string())?;

        let address = IpAddr::V4(
            device_loopback_addr(device).ok_or("aucune adresse locale pour cet appareil")?,
        );
        let tunnel = Tunnel::client(connection, address, greeting.engine)
            .await
            .map_err(|e| format!("les ports locaux n'ont pas pu être ouverts : {e}"))?;

        let way = self.register.lock().expect("registre des voies").settle(
            device,
            Towards {
                host: host.to_string(),
                peer,
            },
            Open {
                _tunnel: tunnel,
                _endpoint: endpoint,
            },
        );
        self.log
            .write(&format!("way {way} open towards {host} on {address}"));

        Ok(Reached {
            way,
            address,
            engine: greeting.engine,
            packet: packet.bytes,
        })
    }

    pub fn hold(&self, way: WayId, process: u32) -> bool {
        let held = self
            .register
            .lock()
            .expect("registre des voies")
            .hold(way, process);
        if held {
            self.log
                .write(&format!("way {way} now serves process {process}"));
        }
        held
    }

    pub fn release(&self, way: WayId) -> bool {
        let closed = self
            .register
            .lock()
            .expect("registre des voies")
            .release(way);
        if closed.is_some() {
            self.log.write(&format!("way {way} closed"));
        }
        closed.is_some()
    }

    pub fn count(&self) -> usize {
        self.register.lock().expect("registre des voies").count()
    }

    /// The sessions this computer is holding towards others.
    pub fn held(&self) -> Vec<Session> {
        self.register
            .lock()
            .expect("registre des voies")
            .held(Instant::now())
    }

    /// Closes the ways with nothing left to serve, for as long as the
    /// service runs.
    pub async fn keep_tidy(self) {
        loop {
            tokio::time::sleep(SWEEP).await;
            let finished = self
                .register
                .lock()
                .expect("registre des voies")
                .finished(self.alive, Instant::now());
            for way in finished {
                self.log
                    .write(&format!("way {way} has nothing left to serve"));
                self.release(way);
            }
        }
    }
}

/// Where the tunnel has to knock. Only the port is ours to add.
fn resolve(host: &str) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    format!("{host}:{TUNNEL_PORT}")
        .to_socket_addrs()
        .map_err(|e| format!("adresse « {host} » introuvable : {e}"))?
        .next()
        .ok_or_else(|| format!("adresse « {host} » introuvable"))
}

/// Whether that process is still running.
#[cfg(windows)]
fn still_running(process: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: a refused or finished process gives a null handle, which
    // is the answer we are after; a real one is closed right below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) };
    if handle.is_null() {
        return false;
    }
    let mut code = 0u32;
    // SAFETY: the handle is live and the slot is ours.
    let asked =
        unsafe { windows_sys::Win32::System::Threading::GetExitCodeProcess(handle, &mut code) };
    // SAFETY: the handle came from the call above and is closed once.
    unsafe { CloseHandle(handle) };
    // A handle can outlive the process it names: only the exit code says
    // which of the two we are looking at.
    asked != 0 && code == STILL_ACTIVE as u32
}

/// Outside Windows the service does not exist, and nothing holds a way.
#[cfg(not(windows))]
fn still_running(_process: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register() -> Register<&'static str> {
        Register::new()
    }

    fn towards(host: &str) -> Towards {
        Towards {
            host: host.to_string(),
            peer: "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
                .parse()
                .unwrap(),
        }
    }

    #[test]
    fn each_way_takes_its_own_local_address() {
        let mut register = register();
        let first = register.reserve().unwrap();
        let second = register.reserve().unwrap();
        assert_ne!(first, second);
        assert_ne!(
            device_loopback_addr(first).unwrap(),
            device_loopback_addr(second).unwrap()
        );
    }

    #[test]
    fn a_closed_way_gives_its_address_back() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        // Taken again straight away would mean the address leaked with
        // every session, and the client engine remembering a new
        // computer each time.
        assert_eq!(register.release(way), Some("session"));
        assert_eq!(register.reserve(), Some(device));
    }

    #[test]
    fn an_abandoned_attempt_gives_its_address_back() {
        let mut register = register();
        let device = register.reserve().unwrap();
        register.give_back(device);
        assert_eq!(register.reserve(), Some(device));
    }

    #[test]
    fn releasing_a_way_twice_is_not_an_error() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        assert!(register.release(way).is_some());
        assert!(register.release(way).is_none());
    }

    #[test]
    fn a_way_whose_process_is_gone_is_finished() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        assert!(register.hold(way, 4242));

        let now = Instant::now();
        assert!(register.finished(|_| true, now).is_empty());
        assert_eq!(register.finished(|_| false, now), vec![way]);
    }

    #[test]
    fn a_way_nobody_ever_claimed_is_finished_after_the_grace_period() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");

        let now = Instant::now();
        assert!(register.finished(|_| true, now).is_empty());
        // Whoever asked for it never came back to say what it was for.
        assert_eq!(
            register.finished(|_| true, now + GRACE + Duration::from_secs(1)),
            vec![way]
        );
    }

    #[test]
    fn a_claimed_way_is_not_swept_for_being_old() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        register.hold(way, 4242);
        // A long session is the normal case, not an abandoned one.
        assert!(
            register
                .finished(|_| true, Instant::now() + GRACE * 100)
                .is_empty()
        );
    }

    #[test]
    fn holding_a_way_that_is_gone_says_so() {
        let mut register = register();
        assert!(!register.hold(WayId(7), 4242));
    }

    #[test]
    fn only_a_way_that_serves_a_process_counts_as_a_session() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");

        // Opened, not yet claimed: an attempt under way, not a picture
        // on screen.
        assert!(register.held(Instant::now()).is_empty());

        register.hold(way, 4242);
        let held = register.held(Instant::now());
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].way, way);
        assert_eq!(held[0].towards, "192.168.1.20");

        register.release(way);
        assert!(register.held(Instant::now()).is_empty());
    }

    #[test]
    fn a_session_says_how_long_the_picture_has_been_up() {
        let mut register = register();
        let device = register.reserve().unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        // A first pairing can take minutes between the way opening and
        // the player starting: the session is as old as the picture,
        // not as old as the tunnel.
        register.hold(way, 4242);

        let held = register.held(Instant::now() + Duration::from_secs(600));
        assert!(held[0].since >= Duration::from_secs(600), "{:?}", held[0]);
    }

    #[test]
    fn the_sessions_come_back_in_the_same_order_every_time() {
        let mut register = register();
        for _ in 0..8 {
            let device = register.reserve().unwrap();
            let way = register.settle(device, towards("192.168.1.20"), "session");
            register.hold(way, 4242);
        }
        // Ordered by the register itself, since what reads it redraws
        // only when the list actually changed.
        let ways: Vec<WayId> = register
            .held(Instant::now())
            .into_iter()
            .map(|session| session.way)
            .collect();
        let mut sorted = ways.clone();
        sorted.sort();
        assert_eq!(ways, sorted);
    }

    #[test]
    fn the_count_follows_what_is_open() {
        let mut register = register();
        assert_eq!(register.count(), 0);
        let device = register.reserve().unwrap();
        // An address taken is not yet a way: the count follows what is
        // actually open, which is what the interface shows.
        assert_eq!(register.count(), 0);
        let way = register.settle(device, towards("192.168.1.20"), "session");
        assert_eq!(register.count(), 1);
        register.release(way);
        assert_eq!(register.count(), 0);
    }
}
