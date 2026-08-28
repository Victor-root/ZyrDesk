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
use zyr_proto::log::Log;
use zyr_proto::net::{TUNNEL_PORT, device_loopback_addr};
use zyr_proto::paths;
use zyr_transport::{Connection, Fingerprint, Identity, MediaProfile, TunnelEndpoint, packet_size};
use zyr_tunnel::{Tunnel, aside};

/// Where the tunnel leaves from: any interface, any port.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Time left to path discovery before the packet size is fixed.
///
/// The engine keeps that size for the whole session and cannot change it
/// along the way, so it is worth a moment's wait.
const PATH_DISCOVERY: Duration = Duration::from_secs(2);

/// How long a way may stay tied to nothing before it is closed.
///
/// It covers everything between the way being opened and the player
/// being handed over: at worst a session that starts, is turned away
/// because the far computer forgot the pairing, waits six seconds to be
/// sure, pairs again with thirty seconds of patience, and starts a
/// second time before it is believed. Cutting the way anywhere along
/// that road closes the tunnel under a session that was going to make
/// it; whoever really abandoned an attempt costs a closed loopback for
/// two minutes, which nothing else is waiting on.
const GRACE: Duration = Duration::from_secs(120);

/// How often the ways are looked over.
const SWEEP: Duration = Duration::from_secs(2);

/// What one way out is made of. Dropping it closes it.
struct Open {
    _tunnel: Tunnel,
    _endpoint: TunnelEndpoint,
    /// The way itself, kept open to speak to the far ZyrDesk rather than
    /// to its engine: the pairing code travels this way.
    connection: Connection,
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
    /// Where the client engine finds that computer on this machine: the
    /// local address the way stands on, and the engine's own port.
    at: String,
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

    /// Takes the local address that stands for that computer.
    ///
    /// Drawn from the computer's fingerprint, so the same computer lands
    /// on the same address session after session, whatever order the
    /// sessions were opened in. The engine's stored pairing names the
    /// address it paired with and nothing else: an address that moved
    /// with connection order made the engine treat a known computer as a
    /// stranger and pair with it again, silently, every time the order
    /// changed.
    ///
    /// When the drawn address is taken, the next free one is used: two
    /// computers landing on the same number is rare, and only costs that
    /// stability while both are open at once.
    fn reserve(&mut self, peer: Fingerprint) -> Option<u16> {
        let slots = u32::from(u16::MAX);
        let bytes = peer.as_bytes();
        let drawn = u32::from(u16::from_be_bytes([bytes[0], bytes[1]]));
        let device = (0..slots)
            .map(|step| ((drawn + step) % slots) as u16)
            .find(|index| !self.taken.contains(index) && device_loopback_addr(*index).is_some())?;
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

    /// What a way stands on, for as long as it stands.
    fn thing(&self, way: WayId) -> Option<&T> {
        self.kept.get(&way).map(|kept| &kept.thing)
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
                    process: serving.process,
                    at: kept.towards.at.clone(),
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
        also: &[IpAddr],
    ) -> Result<Reached, String> {
        let asked = resolve(host)?;
        let candidates = every_way_there(asked, also);
        // Written down before anything is tried. What the person sees of
        // a failure is a sentence in a window they will have closed by
        // the time anyone looks; the trace is what remains, and it is
        // worth as much as the attempt itself.
        self.log.write(&match candidates.len() {
            1 => format!("opening a way to {asked}, expecting {peer}"),
            count => format!(
                "opening a way to {peer}, racing {count} addresses: {}",
                candidates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });

        let identity =
            Identity::load_or_create(&paths::identity_dir()).map_err(|e| e.to_string())?;

        let device = self
            .register
            .lock()
            .expect("registre des voies")
            .reserve(peer)
            .ok_or("plus d'adresse locale disponible pour une session de plus")?;

        match self
            .dig(&candidates, host, peer, media, device, &identity)
            .await
        {
            Ok(reached) => Ok(reached),
            Err(e) => {
                self.register
                    .lock()
                    .expect("registre des voies")
                    .give_back(device);
                // Sur une ligne : un refus est écrit pour être lu à
                // l'écran, sur plusieurs lignes, et le journal en compte
                // une par événement.
                self.log
                    .write(&format!("no way to {asked}: {}", e.replace('\n', " ")));
                Err(e)
            }
        }
    }

    /// Opens towards every address at once and keeps whichever answers
    /// first, dropping the rest.
    ///
    /// A computer on the same desk often has several addresses, and they
    /// are not worth the same at all: one is the cable between the two
    /// machines, another belongs to a virtual adapter or a VPN that wraps
    /// the traffic up and sends it somewhere far away before bringing it
    /// back. Sixty milliseconds of latency between two computers on one
    /// desk is what the second kind costs, and no session survives that
    /// pleasantly.
    ///
    /// Nothing here can tell them apart by looking: an address is four
    /// numbers, and which of them leads through a tunnel is not written
    /// anywhere. So they are all tried at once and the fastest to answer
    /// wins, which is the same answer arrived at by measuring instead of
    /// guessing. The losers are dropped the moment there is a winner.
    async fn race(
        &self,
        endpoint: &TunnelEndpoint,
        candidates: &[SocketAddr],
    ) -> Result<(Connection, u128, SocketAddr), String> {
        let started = Instant::now();
        let mut running = tokio::task::JoinSet::new();
        for address in candidates {
            let towards = endpoint.clone();
            let address = *address;
            running.spawn(async move { (address, towards.connect(address).await) });
        }

        let mut refused = Vec::new();
        while let Some(finished) = running.join_next().await {
            let Ok((address, outcome)) = finished else {
                continue;
            };
            match outcome {
                Ok(connection) => {
                    return Ok((connection, started.elapsed().as_millis(), address));
                }
                Err(e) => refused.push(format!("{address} : {e}")),
            }
        }
        Err(refused.join(" ; "))
    }

    /// Everything between the address being taken and the way being
    /// written down. Kept apart so a failure anywhere gives the address
    /// back exactly once.
    async fn dig(
        &self,
        candidates: &[SocketAddr],
        host: &str,
        peer: Fingerprint,
        media: MediaProfile,
        device: u16,
        identity: &Identity,
    ) -> Result<Reached, String> {
        let endpoint =
            TunnelEndpoint::client(identity, peer, media, SocketAddr::new(EVERY_INTERFACE, 0))
                .map_err(|e| e.to_string())?;

        let (connection, took, through) = self
            .race(&endpoint, candidates)
            .await
            .map_err(|e| format!("{host} ne répond pas sur le port {TUNNEL_PORT} : {e}"))?;
        if candidates.len() > 1 {
            self.log
                .write(&format!("{through} answered first, after {took} ms"));
        }

        // The first real exchange, and the moment authorisation is
        // proven: a connection succeeds before the other computer has
        // judged our certificate, so nothing may be announced as
        // established until this answers.
        let engine = aside::ask_the_ports(&connection).await.map_err(|e| {
            format!(
                "{host} a refusé cet ordinateur, ou son empreinte a changé.\n  \
                 Sur {host}, vérifiez que l'accès distant est actif et que\n  \
                 la confiance au réseau local l'est aussi.\n  Détail : {e}"
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
        let tunnel = Tunnel::client(connection.clone(), address, engine)
            .await
            .map_err(|e| format!("les ports locaux n'ont pas pu être ouverts : {e}"))?;

        let way = self.register.lock().expect("registre des voies").settle(
            device,
            Towards {
                host: host.to_string(),
                peer,
                at: format!("{address}:{}", engine.http()),
            },
            Open {
                _tunnel: tunnel,
                _endpoint: endpoint,
                connection,
            },
        );
        self.log
            .write(&format!("way {way} open towards {host} on {address}"));

        Ok(Reached {
            way,
            address,
            engine,
            packet: packet.bytes,
        })
    }

    /// Hands the far computer the code its engine is waiting for.
    ///
    /// The name it files this computer under is this computer's own: it
    /// is the only side that knows it, and nobody should have to type it
    /// in.
    pub async fn hand_over_the_code(&self, way: WayId, pin: &str) -> Result<(), String> {
        // Taken out from under the lock before waiting on the network:
        // every other way is queueing behind that lock.
        let connection = {
            let register = self.register.lock().expect("registre des voies");
            register.thing(way).map(|open| open.connection.clone())
        };
        let Some(connection) = connection else {
            return Err(format!("la voie {way} n'existe plus"));
        };

        let name = zyr_proto::machine::name();
        aside::ask_to_pair(&connection, pin, &name)
            .await
            .map_err(|e| format!("l'ordinateur distant a refusé l'appairage : {e}"))?;
        self.log
            .write(&format!("way {way} handed its pairing code over"));
        Ok(())
    }

    /// Asks the far computer to press Ctrl+Alt+Suppr on itself.
    ///
    /// The same shape as the pairing above, and for the same reason: the
    /// connection comes out from under the lock before anything waits on
    /// the network, because every other way is queueing behind it.
    pub async fn ask_for_the_secure_attention(&self, way: WayId) -> Result<(), String> {
        let connection = {
            let register = self.register.lock().expect("registre des voies");
            register.thing(way).map(|open| open.connection.clone())
        };
        let Some(connection) = connection else {
            return Err(format!("la voie {way} n'existe plus"));
        };

        aside::ask_for_the_secure_attention(&connection)
            .await
            .map_err(|e| format!("l'ordinateur distant n'a pas pressé Ctrl+Alt+Suppr : {e}"))?;
        self.log
            .write(&format!("way {way} asked for Ctrl+Alt+Suppr"));
        Ok(())
    }

    /// Asks the far computer to silence its own speakers for the length
    /// of the session, or to let them play again.
    ///
    /// The same shape as the two above, and for the same reason: the
    /// connection comes out from under the lock before anything waits on
    /// the network.
    pub async fn ask_to_hush(&self, way: WayId, quiet: bool) -> Result<(), String> {
        let connection = {
            let register = self.register.lock().expect("registre des voies");
            register.thing(way).map(|open| open.connection.clone())
        };
        let Some(connection) = connection else {
            return Err(format!("la voie {way} n'existe plus"));
        };

        aside::ask_to_hush(&connection, quiet)
            .await
            .map_err(|e| format!("les enceintes de l'ordinateur distant n'ont pas bougé : {e}"))?;
        self.log.write(&format!(
            "way {way} asked the far computer's speakers to {}",
            if quiet { "be silent" } else { "play again" }
        ));
        Ok(())
    }

    /// Asks the far computer to resend a still screen at full rate, or
    /// to stop doing it.
    ///
    /// Asked at the opening of every session, and almost never changing
    /// anything: the far computer does nothing at all when it is already
    /// serving the way it was asked to.
    pub async fn ask_to_serve_steady(&self, way: WayId, rate: bool) -> Result<(), String> {
        let connection = {
            let register = self.register.lock().expect("registre des voies");
            register.thing(way).map(|open| open.connection.clone())
        };
        let Some(connection) = connection else {
            return Err(format!("la voie {way} n'existe plus"));
        };

        aside::ask_to_serve_steady(&connection, rate)
            .await
            .map_err(|e| format!("l'ordinateur distant n'a pas réglé sa cadence : {e}"))?;
        self.log.write(&format!(
            "way {way} asked the far computer to {} resending a still screen",
            if rate { "start" } else { "stop" }
        ));
        Ok(())
    }

    /// Asks the far computer to wake its virtual screen for a picture
    /// that size, or, with no size, to put it back to sleep.
    ///
    /// Asked at the opening of a session and answered before the picture
    /// is opened: the far engine has to find that screen, and it only
    /// finds one that is already there. Asked again with nothing at the
    /// end, so the screen does not outlive the session that wanted it.
    pub async fn ask_for_a_screen(
        &self,
        way: WayId,
        size: Option<(u32, u32)>,
    ) -> Result<Option<(u32, u32)>, String> {
        let connection = {
            let register = self.register.lock().expect("registre des voies");
            register.thing(way).map(|open| open.connection.clone())
        };
        let Some(connection) = connection else {
            return Err(format!("la voie {way} n'existe plus"));
        };

        let showing = aside::ask_for_a_screen(&connection, size)
            .await
            .map_err(|e| format!("l'ordinateur distant n'a pas préparé son écran : {e}"))?;
        self.log.write(&match size {
            Some((wide, high)) => format!(
                "way {way} asked the far computer to wake its virtual screen for {wide}x{high}"
            ),
            None => format!("way {way} asked the far computer to keep its own screen"),
        });
        if let Some((wide, high)) = showing {
            self.log.write(&format!(
                "way {way}: the far computer is showing {wide}x{high}"
            ));
        }
        Ok(showing)
    }

    /// Asks the far computer to put its lock screen up.
    ///
    /// The same shape again. This is what stands in for Windows+L, which
    /// cannot travel: Windows keeps that one where no program can reach
    /// it, at both ends of a session.
    pub async fn ask_to_lock(&self, way: WayId) -> Result<(), String> {
        let connection = {
            let register = self.register.lock().expect("registre des voies");
            register.thing(way).map(|open| open.connection.clone())
        };
        let Some(connection) = connection else {
            return Err(format!("la voie {way} n'existe plus"));
        };

        aside::ask_to_lock(&connection)
            .await
            .map_err(|e| format!("l'ordinateur distant ne s'est pas verrouillé : {e}"))?;
        self.log
            .write(&format!("way {way} asked the far computer to lock itself"));
        Ok(())
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
/// Every way there worth trying, the one that was asked for first.
///
/// First because it is the one somebody named, or the one the product
/// wrote down, and a race whose entrants all answer should be won by the
/// expected one. The others come from what that computer answered on: a
/// machine with two cards answers on both, and only trying tells which
/// one is the cable and which is a detour.
fn every_way_there(asked: SocketAddr, also: &[IpAddr]) -> Vec<SocketAddr> {
    let mut ways = vec![asked];
    for address in also {
        let candidate = SocketAddr::new(*address, TUNNEL_PORT);
        if !ways.contains(&candidate) {
            ways.push(candidate);
        }
    }
    ways
}

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

    fn peer() -> Fingerprint {
        "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
            .parse()
            .unwrap()
    }

    fn other_peer() -> Fingerprint {
        "f145a3b2c89cf2370829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d79"
            .parse()
            .unwrap()
    }

    fn towards(host: &str) -> Towards {
        Towards {
            host: host.to_string(),
            peer: peer(),
            at: "127.77.0.1:47989".to_string(),
        }
    }

    #[test]
    fn each_way_takes_its_own_local_address() {
        let mut register = register();
        let first = register.reserve(peer()).unwrap();
        let second = register.reserve(peer()).unwrap();
        assert_ne!(first, second);
        assert_ne!(
            device_loopback_addr(first).unwrap(),
            device_loopback_addr(second).unwrap()
        );
    }

    #[test]
    fn a_computer_keeps_the_same_local_address_from_one_session_to_the_next() {
        // Le moteur retient l'appairage sous l'adresse qu'il a composée :
        // une adresse qui bougerait avec l'ordre des connexions lui
        // ferait prendre un ordinateur connu pour un inconnu, et
        // réappairer en silence.
        let mut register = register();
        let first = register.reserve(peer()).unwrap();
        register.give_back(first);

        // Un autre ordinateur ouvre une session entre-temps : il prend
        // sa propre adresse, pas celle du premier.
        let second = register.reserve(other_peer()).unwrap();
        assert_ne!(first, second);

        // Et le premier retrouve la sienne.
        assert_eq!(register.reserve(peer()), Some(first));
    }

    #[test]
    fn a_closed_way_gives_its_address_back() {
        let mut register = register();
        let device = register.reserve(peer()).unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        // Taken again straight away would mean the address leaked with
        // every session, and the client engine remembering a new
        // computer each time.
        assert_eq!(register.release(way), Some("session"));
        assert_eq!(register.reserve(peer()), Some(device));
    }

    #[test]
    fn an_abandoned_attempt_gives_its_address_back() {
        let mut register = register();
        let device = register.reserve(peer()).unwrap();
        register.give_back(device);
        assert_eq!(register.reserve(peer()), Some(device));
    }

    #[test]
    fn releasing_a_way_twice_is_not_an_error() {
        let mut register = register();
        let device = register.reserve(peer()).unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        assert!(register.release(way).is_some());
        assert!(register.release(way).is_none());
    }

    #[test]
    fn a_way_whose_process_is_gone_is_finished() {
        let mut register = register();
        let device = register.reserve(peer()).unwrap();
        let way = register.settle(device, towards("192.168.1.20"), "session");
        assert!(register.hold(way, 4242));

        let now = Instant::now();
        assert!(register.finished(|_| true, now).is_empty());
        assert_eq!(register.finished(|_| false, now), vec![way]);
    }

    #[test]
    fn a_way_nobody_ever_claimed_is_finished_after_the_grace_period() {
        let mut register = register();
        let device = register.reserve(peer()).unwrap();
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
        let device = register.reserve(peer()).unwrap();
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
        let device = register.reserve(peer()).unwrap();
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
        let device = register.reserve(peer()).unwrap();
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
            let device = register.reserve(peer()).unwrap();
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
        let device = register.reserve(peer()).unwrap();
        // An address taken is not yet a way: the count follows what is
        // actually open, which is what the interface shows.
        assert_eq!(register.count(), 0);
        let way = register.settle(device, towards("192.168.1.20"), "session");
        assert_eq!(register.count(), 1);
        register.release(way);
        assert_eq!(register.count(), 0);
    }
}
