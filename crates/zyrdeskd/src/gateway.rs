//! The tunnel end the service holds.
//!
//! This is the one door open on this computer. Everything a session
//! needs goes through it: the engine's seven ports are multiplexed into
//! a single encrypted connection. That is what lets the engine close
//! back onto the local machine, where nothing on the network can reach
//! it, and what leaves a single rule to write in a firewall.
//!
//! Who may come in is decided by fingerprint. Two things put a
//! fingerprint on that list: it was written down, or its owner announced
//! itself on this local network while this computer was trusting it.
//! The list is read again as the service runs, so one more computer
//! appearing on the network does not mean cutting the session in
//! progress, and asking a small file every few seconds costs nothing
//! next to watching the filesystem on every platform.
//!
//! The door also answers for the engine on ZyrDesk's own channel: the
//! far computer hands over the code its engine is waiting for, and it is
//! passed on here. That is the whole of what replaced a code shown on
//! one screen and typed on the other.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::runtime::Handle;
use tokio::task::{JoinHandle, JoinSet};
use zyr_engine_host::Credentials;
use zyr_engine_host::api::EngineApi;
use zyr_lan::Found;
use zyr_proto::log::Log;
use zyr_proto::net::{EnginePorts, TUNNEL_PORT};
use zyr_proto::paths;
use zyr_transport::{
    AllowedPeers, EndpointError, Fingerprint, Identity, MediaProfile, TunnelEndpoint, authorized,
};
use zyr_tunnel::{Answers, Tunnel};

use crate::preferences::Remembered;

/// Every network interface: the computer is reachable from wherever the
/// other one is.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Where the engine listens, and the only place the tunnel hands it
/// anything.
const ENGINE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// How often the list of authorised devices is worked out again.
const AUTHORIZED_REFRESH: Duration = Duration::from_secs(5);

/// How long a pairing code is offered to the local engine.
///
/// The far computer starts its own engine and then hands the code over,
/// so the two arrive within a hair of each other and in no fixed order.
/// The engine refuses a code as long as nobody is asking it for one, so
/// it is offered again until somebody is.
const PAIRING_PATIENCE: Duration = Duration::from_secs(10);

/// Pause between two offers.
const PAIRING_RETRY: Duration = Duration::from_millis(200);

/// The local engine, as the tunnel has to see it.
pub struct AtHand {
    pub ports: EnginePorts,
    pub credentials: Credentials,
}

/// The local engine, and the one thing a far computer may ask of it.
struct Attending {
    ports: EnginePorts,
    api: Arc<EngineApi>,
    log: Log,
}

impl Answers for Attending {
    fn engine(&self) -> EnginePorts {
        self.ports
    }

    /// Offers the far computer's code to the engine, and keeps offering
    /// it after answering.
    ///
    /// The engine only takes a code while a client is asking it for one,
    /// and reports success either way (`patches/MANIFEST.md`). The far
    /// engine was started before the code was sent, but started is not
    /// yet asking: a code offered in that gap is swallowed with a
    /// straight face, and stopping there left the real request, arriving
    /// a moment later, waiting for a code nobody would offer again. So
    /// the first successful offer answers the caller, and the offering
    /// goes on quietly for the rest of the patience: offering a code
    /// nobody is waiting for does nothing, which is exactly why it is
    /// safe to insist.
    fn hand_over_the_code(&self, pin: &str, name: &str) -> Result<(), String> {
        let deadline = Instant::now() + PAIRING_PATIENCE;
        loop {
            let refused = match self.api.submit_pin(pin, name) {
                Ok(()) => break,
                Err(e) => e.to_string(),
            };
            if Instant::now() >= deadline {
                self.log
                    .write(&format!("pairing refused to {name}: {refused}"));
                return Err(refused);
            }
            std::thread::sleep(PAIRING_RETRY);
        }
        self.log
            .write(&format!("pairing code offered to the engine for {name}"));

        let api = self.api.clone();
        let pin = pin.to_string();
        let name = name.to_string();
        std::thread::spawn(move || {
            while Instant::now() < deadline {
                std::thread::sleep(PAIRING_RETRY);
                let _ = api.submit_pin(&pin, &name);
            }
        });
        Ok(())
    }
}

/// The open door, and the sessions coming through it.
///
/// Dropping it closes everything: the tunnel has no reason to outlive
/// the engine it serves.
#[derive(Debug)]
pub struct Gateway {
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Gateway {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Gateway {
    /// Opens the tunnel and serves whoever is authorised.
    pub fn open(
        runtime: &Handle,
        engine: AtHand,
        neighbours: Found,
        remembered: Remembered,
        log: &Log,
    ) -> io::Result<Self> {
        // The transport registers with the runtime as it is built, so it
        // has to be built from inside it.
        let _guard = runtime.enter();

        let identity =
            Identity::load_or_create(&paths::identity_dir()).map_err(io::Error::other)?;
        let list = paths::authorized_devices();
        let starting = let_in(authorized::read(&list)?, &neighbours, &remembered);
        let allowed: AllowedPeers = starting.iter().copied().collect();
        if starting.is_empty() {
            log.write(
                "nobody can reach this computer yet: no device written down, \
                 and no other ZyrDesk seen on the local network",
            );
        }
        // Nommés un par un. Une session refusée et une liste vide se
        // ressemblent trop pour qu'on se contente d'un nombre.
        for device in &starting {
            log.write(&format!("{device} may come in"));
        }

        let endpoint = TunnelEndpoint::host(
            &identity,
            allowed.clone(),
            MediaProfile::default(),
            SocketAddr::new(EVERY_INTERFACE, TUNNEL_PORT),
        )
        .map_err(io::Error::other)?;

        log.write(&format!(
            "tunnel open on port {TUNNEL_PORT}, fingerprint of this computer {}",
            identity.fingerprint()
        ));

        let attending: Arc<dyn Answers> = Arc::new(Attending {
            ports: engine.ports,
            api: Arc::new(EngineApi::new(engine.ports, engine.credentials)),
            log: log.clone(),
        });

        Ok(Self {
            tasks: vec![
                runtime.spawn(keep_the_list_fresh(
                    list,
                    allowed,
                    starting,
                    neighbours,
                    remembered,
                    log.clone(),
                )),
                runtime.spawn(serve(endpoint, attending, log.clone())),
            ],
        })
    }
}

/// Takes in the devices that connect, one session each.
async fn serve(endpoint: TunnelEndpoint, attending: Arc<dyn Answers>, log: Log) {
    let mut sessions = JoinSet::new();
    loop {
        match endpoint.accept().await {
            Ok(connection) => {
                let log = log.clone();
                let attending = attending.clone();
                sessions.spawn(async move { one_session(connection, attending, log).await });
                while sessions.try_join_next().is_some() {}
            }
            // A refused device is not the end of the door: it must not
            // stop this computer from taking in the next one, which is
            // otherwise a denial of service anyone could trigger.
            Err(EndpointError::Closed) => {
                log.write("the tunnel is closed, no longer taking anyone in");
                return;
            }
            Err(e) => log.write(&format!("connection refused: {e}")),
        }
    }
}

async fn one_session(connection: zyr_transport::Connection, attending: Arc<dyn Answers>, log: Log) {
    let from = connection.remote_address();
    let mut tunnel = match Tunnel::host(connection, ENGINE, attending).await {
        Ok(tunnel) => tunnel,
        Err(e) => {
            log.write(&format!("session from {from} not opened: {e}"));
            return;
        }
    };
    log.write(&format!("session open with {from}"));

    let outcome = tunnel.wait().await;
    let reading = tunnel.reading();
    match outcome {
        Ok(()) => log.write(&format!(
            "session ended, {} packets to the engine, {} to the tunnel",
            reading.to_engine, reading.to_tunnel
        )),
        Err(e) => log.write(&format!("session ended: {e}")),
    }
}

/// Works the list of authorised devices out again, so a computer that
/// has just appeared gets in without the service being restarted.
///
/// Every change is written down, and only the changes: the list is
/// worked out afresh every few seconds, and saying so each time would
/// bury everything else. What matters is the moment a computer starts or
/// stops being let in, which is exactly what a refused session needs
/// explaining.
async fn keep_the_list_fresh(
    list: PathBuf,
    allowed: AllowedPeers,
    starting: Vec<Fingerprint>,
    neighbours: Found,
    remembered: Remembered,
    log: Log,
) {
    let mut reported: Option<String> = None;
    let mut known = starting;
    loop {
        match authorized::read(&list) {
            Ok(written) => {
                if reported.take().is_some() {
                    log.write("authorised devices readable again");
                }
                let now = let_in(written, &neighbours, &remembered);
                for said in apart(&known, &now) {
                    log.write(&said);
                }
                known = now.clone();
                allowed.replace_with(now);
            }
            // What was already allowed stays allowed: a file being
            // rewritten must not cut the session in progress.
            Err(e) => {
                let message = e.to_string();
                if reported.as_deref() != Some(message.as_str()) {
                    log.write(&format!("authorised devices unreadable: {message}"));
                    reported = Some(message);
                }
            }
        }
        tokio::time::sleep(AUTHORIZED_REFRESH).await;
    }
}

/// What changed between two states of the list, in words.
///
/// Nothing when nothing moved, which is the ordinary case a few times a
/// minute for as long as the service runs.
fn apart(before: &[Fingerprint], now: &[Fingerprint]) -> Vec<String> {
    let mut said = Vec::new();
    for device in now {
        if !before.contains(device) {
            said.push(format!("{device} may now come in"));
        }
    }
    for device in before {
        if !now.contains(device) {
            said.push(format!("{device} may no longer come in"));
        }
    }
    said
}

/// Everyone this computer lets in.
///
/// The devices written down, plus the ZyrDesk announcing themselves on
/// this local network when it is trusted. That trust is what spares
/// anyone carrying a fingerprint from one computer to the other, and it
/// covers exactly what the network already carries: a machine that can
/// speak on it. Nothing arriving from outside it is ever let in this
/// way, and the day sessions cross the Internet an account takes over.
fn let_in(
    written: Vec<Fingerprint>,
    neighbours: &Found,
    remembered: &Remembered,
) -> Vec<Fingerprint> {
    if !remembered.trust_local_network() {
        return written;
    }
    let seen = neighbours
        .peers()
        .into_iter()
        .map(|peer| peer.fingerprint)
        .collect();
    joined(written, seen)
}

/// Two lists of fingerprints as one, without repeats.
///
/// The same computer is very often on both: written down once, and
/// announcing itself ever since.
fn joined(written: Vec<Fingerprint>, seen: Vec<Fingerprint>) -> Vec<Fingerprint> {
    let mut devices = written;
    for device in seen {
        if !devices.contains(&device) {
            devices.push(device);
        }
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    fn remembered(what: &str) -> (Remembered, PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-gateway-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let path = folder.join("preferences.conf");
        (Remembered::at(path), folder)
    }

    #[test]
    fn a_neighbour_is_let_in_without_anyone_writing_it_down() {
        // C'est tout l'intérêt du réseau local : deux ZyrDesk allumés
        // sur le même réseau se joignent sans rien recopier.
        let devices = joined(vec![fingerprint(1)], vec![fingerprint(2)]);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);
    }

    #[test]
    fn a_device_both_written_down_and_seen_is_one_device() {
        // Sinon la même empreinte entrerait deux fois dans la liste que
        // le transport consulte à chaque connexion.
        let devices = joined(vec![fingerprint(1)], vec![fingerprint(1), fingerprint(2)]);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);
    }

    #[test]
    fn only_what_changed_in_the_list_is_worth_a_line() {
        // La liste est refaite toutes les cinq secondes : le journal ne
        // doit porter que les moments où elle bouge, sinon il n'y aura
        // plus rien d'autre à y lire.
        let un = fingerprint(1);
        let deux = fingerprint(2);
        assert!(apart(&[un, deux], &[un, deux]).is_empty());
        assert!(apart(&[], &[]).is_empty());

        let arrive = apart(&[un], &[un, deux]);
        assert_eq!(arrive.len(), 1);
        assert!(arrive[0].starts_with(&deux.to_string()), "{arrive:?}");
        assert!(arrive[0].contains("may now come in"), "{arrive:?}");

        let part = apart(&[un, deux], &[un]);
        assert_eq!(part.len(), 1);
        assert!(part[0].contains("may no longer come in"), "{part:?}");
    }

    #[test]
    fn trust_turned_off_leaves_only_what_was_written_down() {
        let (remembered, folder) = remembered("sans-confiance");
        assert!(remembered.trust_local_network());
        remembered.set_trust_local_network(false).unwrap();

        // Rien de ce que le réseau annonce ne doit plus entrer : c'est
        // le seul effet attendu de cet interrupteur.
        let devices = let_in(vec![fingerprint(1)], &Found::new(), &remembered);
        assert_eq!(devices, vec![fingerprint(1)]);

        let _ = std::fs::remove_dir_all(&folder);
    }
}
