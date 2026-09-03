//! The relay: packets between the two devices a pass names, and nothing
//! else.
//!
//! What arrives here is already encrypted with keys the two devices
//! alone hold. The relay reads the pass its own broker signed, learns
//! which two fingerprints belong to a session, and from then on hands
//! every packet of one to the other. It opens nothing, keeps nothing but
//! the count of the bytes it carried, and forgets that too once the
//! count is written down.
//!
//! Light on purpose. A relayed session costs one connection per device,
//! a small table, and a copy per packet; nothing here decodes, buffers
//! or waits. The transport under it, and the doorway that lets the
//! mirror share the port, live in `zyr_transport::relay`.
//!
//! The limits are the reason this can face the Internet at all: how many
//! new connections one address may open in a minute, how many sessions
//! may be relayed at once, and how fast any one of them may go. Past a
//! limit a packet is dropped, exactly as a network drops one, and the
//! session goes on at the rate that fits.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::task::JoinHandle;
use zyr_broker::signing::{ServerPublicKey, Signed};
use zyr_broker::ticket::Pass;
use zyr_broker::{Verifier, now};
use zyr_transport::relay::{Doorway, PASS_PATIENCE, Presenting};
use zyr_transport::{
    Bytes, Connection, EndpointError, Fingerprint, Identity, MediaProfile, TunnelEndpoint,
};

use crate::config;
use crate::journal;
use crate::limits::Limiter;
use crate::store::Store;

/// The relay's own certificate, in the keys folder.
const CERTIFICATE_FILE: &str = "relay.crt";
const KEY_FILE: &str = "relay.key";

/// What a device is told about the relay, so it can reach it.
#[derive(Debug, Clone)]
pub struct Offer {
    /// Host and port, as the devices type them.
    pub address: String,
    /// The fingerprint of the certificate it presents.
    pub fingerprint: Fingerprint,
}

/// The relay, carrying for as long as it is held.
pub struct Relay {
    address: SocketAddr,
    offer: Offer,
    carrying: Arc<Carrying>,
    serving: JoinHandle<()>,
}

impl Relay {
    /// Opens the relay on that doorway, with its own certificate.
    ///
    /// The doorway is the server's UDP port, mirror included: a device
    /// asks it where it is seen from and reaches the relay at the same
    /// place.
    pub fn open(
        doorway: &Doorway,
        keys_dir: &std::path::Path,
        address: String,
        limits: &config::Relay,
        key: ServerPublicKey,
        store: Arc<Store>,
    ) -> io::Result<Self> {
        let identity =
            Identity::load_or_create_at(&keys_dir.join(CERTIFICATE_FILE), &keys_dir.join(KEY_FILE))
                .map_err(io::Error::other)?;
        let offer = Offer {
            address,
            fingerprint: identity.fingerprint(),
        };
        let endpoint = TunnelEndpoint::relay_on(
            &identity,
            MediaProfile::default(),
            Arc::new(doorway.clone()),
        )
        .map_err(io::Error::other)?;
        let carrying = Arc::new(Carrying {
            most_sessions: limits.max_sessions as usize,
            bytes_per_second: f64::from(limits.max_kbps_per_session) * 1_000.0 / 8.0,
            knocks: Limiter::new(limits.connections_per_minute),
            verifier: Verifier::new(key),
            store,
            open: Mutex::new(HashMap::new()),
        });
        Ok(Self {
            address: doorway.local_address()?,
            offer,
            carrying: carrying.clone(),
            serving: tokio::spawn(serve(endpoint, carrying)),
        })
    }

    /// Where it listens.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// What the devices are told about it.
    pub fn offer(&self) -> Offer {
        self.offer.clone()
    }

    /// How many sessions it is carrying right now.
    pub fn sessions(&self) -> usize {
        self.carrying.open.lock().expect("sessions relayées").len()
    }

    pub fn stop(&self) {
        self.serving.abort();
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One relayed session, held by both of its ends.
struct Relayed {
    /// The two devices the passes named, and nobody else ever.
    between: [Fingerprint; 2],
    flow: Mutex<Flow>,
}

/// What crosses one session, and what it is allowed.
struct Flow {
    /// Where each of the two is, when it is there.
    ends: [Option<Connection>; 2],
    /// Bytes this session may still send at this instant, a bucket that
    /// refills at the rate allowed.
    allowance: f64,
    counted: Instant,
    carried: u64,
}

impl Relayed {
    fn new(between: [Fingerprint; 2], bytes_per_second: f64, now: Instant) -> Self {
        Self {
            between,
            flow: Mutex::new(Flow {
                ends: [None, None],
                allowance: bytes_per_second,
                counted: now,
                carried: 0,
            }),
        }
    }

    /// Which of the two that device is, or nothing when it is neither.
    fn side_of(&self, device: Fingerprint) -> Option<usize> {
        self.between.iter().position(|end| *end == device)
    }

    /// Puts that end in place, handing back the connection it replaces:
    /// a device whose connection to the relay broke comes back with the
    /// same pass, and the one before it has to be shown out.
    fn takes_its_place(&self, side: usize, connection: Connection) -> Option<Connection> {
        self.flow.lock().expect("session relayée").ends[side].replace(connection)
    }

    /// Takes that end away when it is still the one registered, and says
    /// whether anybody is left.
    fn leaves(&self, side: usize, connection: &Connection) -> bool {
        let mut flow = self.flow.lock().expect("session relayée");
        if flow.ends[side]
            .as_ref()
            .is_some_and(|held| held.stable_id() == connection.stable_id())
        {
            flow.ends[side] = None;
        }
        flow.ends.iter().all(Option::is_none)
    }

    /// Hands one packet to the other end, if there is one and if the
    /// session is within its rate.
    fn carry(&self, from: usize, packet: Bytes, bytes_per_second: f64, now: Instant) {
        let other = {
            let mut flow = self.flow.lock().expect("session relayée");
            let refill = now.duration_since(flow.counted).as_secs_f64() * bytes_per_second;
            flow.allowance = (flow.allowance + refill).min(bytes_per_second);
            flow.counted = now;
            let size = packet.len() as f64;
            // Past the rate this session is allowed, a packet is
            // dropped, which is what a network does too.
            if flow.allowance < size {
                return;
            }
            flow.allowance -= size;
            flow.carried += packet.len() as u64;
            match flow.ends[1 - from].clone() {
                Some(other) => other,
                // The other end is not here yet, or not any more: a
                // packet on a road nobody is at the end of.
                None => return,
            }
        };
        let _ = other.send_datagram(packet);
    }

    fn carried(&self) -> u64 {
        self.flow.lock().expect("session relayée").carried
    }
}

/// Everything the relay decides, and the sessions it is carrying.
struct Carrying {
    most_sessions: usize,
    bytes_per_second: f64,
    /// New connections one address may open in a minute.
    knocks: Limiter,
    verifier: Verifier,
    store: Arc<Store>,
    open: Mutex<HashMap<String, Arc<Relayed>>>,
}

/// Why a device was not let in, in the words it is told.
enum Turned {
    Unreadable,
    Refused(String),
    TooMany,
    /// A pass for a session already held between two other devices.
    NotOfThisSession,
}

impl std::fmt::Display for Turned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Turned::Unreadable => f.write_str("ce n'est pas un laissez-passer"),
            Turned::Refused(why) => f.write_str(why),
            Turned::TooMany => {
                f.write_str("ce relais porte déjà autant de sessions qu'il en accepte")
            }
            Turned::NotOfThisSession => {
                f.write_str("cette session est déjà tenue entre deux autres appareils")
            }
        }
    }
}

/// One device let into one session, on its side of it.
struct LetIn {
    session: String,
    relayed: Arc<Relayed>,
    side: usize,
}

impl Carrying {
    /// Reads the pass and puts the device on its side of the session.
    fn let_in(&self, presented: &[u8], device: Fingerprint) -> Result<LetIn, Turned> {
        let signed = Signed::from_bytes(presented).ok_or(Turned::Unreadable)?;
        let pass: Pass = self
            .verifier
            .pass(&signed, device, now())
            .map_err(|refusal| Turned::Refused(refusal.to_string()))?;
        let relayed = {
            let mut open = self.open.lock().expect("sessions relayées");
            match open.get(&pass.session) {
                Some(relayed) => relayed.clone(),
                None => {
                    if open.len() >= self.most_sessions {
                        return Err(Turned::TooMany);
                    }
                    let relayed = Arc::new(Relayed::new(
                        [pass.bearer, pass.peer],
                        self.bytes_per_second,
                        Instant::now(),
                    ));
                    open.insert(pass.session.clone(), relayed.clone());
                    relayed
                }
            }
        };
        let side = relayed.side_of(device).ok_or(Turned::NotOfThisSession)?;
        Ok(LetIn {
            session: pass.session,
            relayed,
            side,
        })
    }

    /// Takes that end away, and the session with it once both are gone.
    ///
    /// The bytes it carried are written down at that moment and never
    /// again: it is the one thing of a relayed session a server keeps,
    /// and it is a number, not a trace of anything that was said.
    async fn shown_out(&self, held: &LetIn, on: &Connection) {
        if !held.relayed.leaves(held.side, on) {
            return;
        }
        {
            let mut open = self.open.lock().expect("sessions relayées");
            match open.get(&held.session) {
                Some(known) if Arc::ptr_eq(known, &held.relayed) => open.remove(&held.session),
                // Replaced by a session of the same name, which only a
                // second pass from the broker could make: not ours to
                // take away.
                _ => return,
            };
        }
        let carried = held.relayed.carried();
        journal::say(format!(
            "session {}: relayed, {} kB carried",
            held.session,
            carried / 1_000
        ));
        let session = held.session.clone();
        let store = self.store.clone();
        let _ = tokio::task::spawn_blocking(move || store.session_relayed(&session, carried)).await;
    }
}

/// Takes in the devices that knock, one task each.
async fn serve(endpoint: TunnelEndpoint, carrying: Arc<Carrying>) {
    loop {
        let knocking = endpoint.accept_if(|from| {
            let allowed = carrying.knocks.allows(from.ip());
            if !allowed {
                journal::say(format!("relay: {} knocks too often", from.ip()));
            }
            allowed
        });
        match knocking.await {
            Ok(connection) => {
                tokio::spawn(attend(connection, carrying.clone()));
            }
            Err(EndpointError::Closed) => return,
            // One device turned away is not the end of the relay: it
            // must go on taking the next one in.
            Err(e) => journal::say(format!("relay: {e}")),
        }
    }
}

/// Serves one device from its pass to its last packet.
async fn attend(connection: Connection, carrying: Arc<Carrying>) {
    let from = connection.remote_address();
    let presenting = match tokio::time::timeout(PASS_PATIENCE, Presenting::heard(&connection)).await
    {
        Ok(Ok(presenting)) => presenting,
        // No pass within the deadline, or nothing readable: nothing
        // more is taken from this one.
        _ => {
            journal::say(format!("relay: {from} presented no pass"));
            return;
        }
    };
    let device = presenting.fingerprint;
    let held = match carrying.let_in(&presenting.pass, device) {
        Ok(held) => held,
        Err(turned) => {
            journal::say(format!("relay: {device} turned away, {turned}"));
            let _ = presenting.refused(&turned.to_string()).await;
            return;
        }
    };
    if presenting.taken().await.is_err() {
        return;
    }
    // A device whose connection to the relay broke comes back with the
    // same pass: the one before it is shown out rather than left holding
    // a side nobody is at.
    if let Some(before) = held.relayed.takes_its_place(held.side, connection.clone()) {
        before.close();
    }
    journal::say(format!(
        "relay: {device} is in for session {}, from {from}",
        held.session
    ));

    while let Ok(packet) = connection.read_datagram().await {
        held.relayed
            .carry(held.side, packet, carrying.bytes_per_second, Instant::now());
    }
    carrying.shown_out(&held, &connection).await;
}

/// The host the devices reach the relay at, and its UDP port.
pub fn address_of(public_host: &str, port: u16) -> String {
    match public_host.parse::<IpAddr>() {
        Ok(IpAddr::V6(ip)) => format!("[{ip}]:{port}"),
        _ => format!("{public_host}:{port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use zyr_broker::ServerKey;
    use zyr_transport::relay::{Branch, Wanted};

    /// Past this, something that should have happened has not.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// A relay with a store of its own, and the key its passes are
    /// signed with.
    struct Standing {
        relay: Relay,
        key: ServerKey,
        store: Arc<Store>,
        folder: std::path::PathBuf,
    }

    impl Drop for Standing {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    impl Standing {
        fn open(limits: config::Relay) -> Self {
            let folder = std::env::temp_dir().join(format!(
                "zyrdesk-server-relais-{}",
                zyr_proto::random::alphanumeric_string(8)
            ));
            std::fs::create_dir_all(&folder).unwrap();
            let store = Arc::new(Store::open(&folder.join("zyrdesk.db")).unwrap());
            let key = ServerKey::generate();
            let doorway = Doorway::bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let relay = Relay::open(
                &doorway,
                &folder,
                doorway.local_address().unwrap().to_string(),
                &limits,
                key.public(),
                store.clone(),
            )
            .unwrap();
            Self {
                relay,
                key,
                store,
                folder,
            }
        }

        /// A pass for that device, towards the other, sealed as the
        /// broker would.
        fn wanted(&self, session: &str, bearer: Fingerprint, peer: Fingerprint) -> Wanted {
            let pass = Pass::new(session, bearer, peer, now());
            Wanted {
                address: self.relay.address(),
                fingerprint: self.relay.offer().fingerprint,
                pass: self.key.seal(&pass).unwrap().to_bytes(),
            }
        }
    }

    fn limits() -> config::Relay {
        config::Relay {
            enabled: true,
            listen: "127.0.0.1:0".parse().unwrap(),
            max_sessions: 2,
            max_kbps_per_session: 60_000,
            connections_per_minute: 60,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_devices_a_pass_names_reach_each_other_and_the_bytes_are_counted() {
        let standing = Standing::open(limits());
        standing
            .store
            .session_started("s1", "d1", "d2", &zyr_broker::ticket::Grant::Owner, now())
            .unwrap();
        let here = Identity::generate().unwrap();
        let there = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let first = Branch::open(
            &standing.wanted("s1", here.fingerprint(), there.fingerprint()),
            &here,
            profile,
        )
        .await
        .unwrap();
        let second = Branch::open(
            &standing.wanted("s1", there.fingerprint(), here.fingerprint()),
            &there,
            profile,
        )
        .await
        .unwrap();

        let packet = vec![3u8; 1200];
        assert!(first.send(&packet));
        let arrived = tokio::time::timeout(PATIENCE, second.arrived())
            .await
            .expect("rien n'est passé par le relais")
            .unwrap();
        assert_eq!(&arrived[..], &packet[..]);
        assert_eq!(standing.relay.sessions(), 1);

        // Les deux bouts s'en vont : la session est oubliée, et ce
        // qu'elle a porté est écrit dans la base.
        drop(first);
        drop(second);
        let counted = tokio::time::timeout(PATIENCE, async {
            loop {
                let counted = standing.store.relayed().unwrap();
                if counted.sessions > 0 {
                    return counted;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("la session relayée n'a jamais été comptée");
        assert_eq!(
            counted,
            crate::store::Relayed {
                sessions: 1,
                bytes: 1_200
            }
        );
        assert_eq!(standing.relay.sessions(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_device_the_pass_does_not_name_is_turned_away() {
        // Le relais ne porte qu'entre les deux empreintes nommées : un
        // troisième appareil avec un laissez-passer de sa propre session
        // n'entre pas dans celle des autres.
        let standing = Standing::open(limits());
        let here = Identity::generate().unwrap();
        let there = Identity::generate().unwrap();
        let stranger = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let _first = Branch::open(
            &standing.wanted("s1", here.fingerprint(), there.fingerprint()),
            &here,
            profile,
        )
        .await
        .unwrap();
        let refused = Branch::open(
            &standing.wanted("s1", stranger.fingerprint(), here.fingerprint()),
            &stranger,
            profile,
        )
        .await
        .unwrap_err();
        assert!(refused.to_string().contains("deux autres"), "{refused}");

        // Et un laissez-passer signé par un autre serveur ne vaut rien.
        let impostor = ServerKey::generate();
        let pass = Pass::new("s2", stranger.fingerprint(), here.fingerprint(), now());
        let forged = Wanted {
            address: standing.relay.address(),
            fingerprint: standing.relay.offer().fingerprint,
            pass: impostor.seal(&pass).unwrap().to_bytes(),
        };
        let refused = Branch::open(&forged, &stranger, profile).await.unwrap_err();
        assert!(refused.to_string().contains("contrefait"), "{refused}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_relay_carries_no_more_sessions_than_it_accepts() {
        let mut limits = limits();
        limits.max_sessions = 1;
        let standing = Standing::open(limits);
        let here = Identity::generate().unwrap();
        let there = Identity::generate().unwrap();
        let profile = MediaProfile::default();

        let _first = Branch::open(
            &standing.wanted("s1", here.fingerprint(), there.fingerprint()),
            &here,
            profile,
        )
        .await
        .unwrap();
        let refused = Branch::open(
            &standing.wanted("s2", there.fingerprint(), here.fingerprint()),
            &there,
            profile,
        )
        .await
        .unwrap_err();
        assert!(
            refused.to_string().contains("autant de sessions"),
            "{refused}"
        );
    }

    #[test]
    fn a_session_over_its_rate_drops_what_does_not_fit() {
        // Le plafond de débit est un seau qui se remplit : une rafale
        // passe jusqu'à la seconde qu'il porte, le reste tombe, et une
        // seconde plus tard tout est revenu.
        let start = Instant::now();
        let (mine, theirs) = (
            Identity::generate().unwrap().fingerprint(),
            Identity::generate().unwrap().fingerprint(),
        );
        let per_second = 12_000.0;
        let relayed = Relayed::new([mine, theirs], per_second, start);
        let packet = Bytes::from(vec![0u8; 1_200]);
        for _ in 0..20 {
            relayed.carry(0, packet.clone(), per_second, start);
        }
        assert_eq!(relayed.carried(), 12_000);
        relayed.carry(
            0,
            packet.clone(),
            per_second,
            start + Duration::from_secs(1),
        );
        assert_eq!(relayed.carried(), 13_200);
    }

    #[test]
    fn the_address_the_devices_are_told_holds_an_ipv6_the_way_it_is_written() {
        assert_eq!(address_of("zyr.exemple.fr", 443), "zyr.exemple.fr:443");
        assert_eq!(address_of("192.168.1.40", 443), "192.168.1.40:443");
        assert_eq!(address_of("fd00::1", 443), "[fd00::1]:443");
    }
}
