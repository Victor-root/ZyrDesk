//! Tickets and passes: what the server says about one meeting.
//!
//! A ticket presents one device to another for one session: who goes
//! towards whom, under what right, for the next minute. Both services
//! receive it; the host admits the fingerprint it names, the client
//! learns whom to expect. A pass lets one device into the relay for one
//! session, and names the only other device its packets may reach.
//!
//! Neither replaces the tunnel's own authentication: each end still
//! presents its certificate and refuses any other fingerprint than the
//! one named. The server can present, it cannot impersonate.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zyr_transport::Fingerprint;

use crate::signing::{Forged, ServerPublicKey, Signed};

/// How long a ticket lives: the time it takes to open a session, and
/// not much more, so that one intercepted is worth little.
pub const TICKET_LIFE: Duration = Duration::from_secs(60);

/// How long a pass lives: the time to reach the relay, with room for a
/// retry.
pub const PASS_LIFE: Duration = Duration::from_secs(5 * 60);

/// How far apart two clocks may be before a ticket is refused for its
/// dates alone.
pub const CLOCK_SKEW: Duration = Duration::from_secs(5 * 60);

/// Shape of the tickets and passes of this crate.
const VERSION: u32 = 1;

/// Seconds since the epoch, which is how every date here is written.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// What kind of thing the server signed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Session,
    Relay,
}

/// Under what right one device goes towards another.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum Grant {
    /// Both devices belong to the same account.
    Owner,
    /// A contact shared that machine.
    Share { id: String },
}

/// One device presented to another, for one session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub v: u32,
    pub kind: Kind,
    pub session: String,
    /// The device that goes towards the other.
    #[serde(with = "crate::fingerprint")]
    pub from: Fingerprint,
    /// The device that is gone towards, and that admits the first.
    #[serde(with = "crate::fingerprint")]
    pub to: Fingerprint,
    pub issued: u64,
    pub expires: u64,
    pub grant: Grant,
    pub nonce: String,
}

impl Ticket {
    pub fn new(
        session: impl Into<String>,
        from: Fingerprint,
        to: Fingerprint,
        grant: Grant,
        issued: u64,
    ) -> Self {
        Self {
            v: VERSION,
            kind: Kind::Session,
            session: session.into(),
            from,
            to,
            issued,
            expires: issued + TICKET_LIFE.as_secs(),
            grant,
            nonce: nonce(),
        }
    }
}

/// One device let into the relay, for one session, towards one other.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Pass {
    pub v: u32,
    pub kind: Kind,
    pub session: String,
    /// The device that presents this pass.
    #[serde(with = "crate::fingerprint")]
    pub bearer: Fingerprint,
    /// The only device its packets may reach.
    #[serde(with = "crate::fingerprint")]
    pub peer: Fingerprint,
    pub issued: u64,
    pub expires: u64,
    pub nonce: String,
}

impl Pass {
    pub fn new(
        session: impl Into<String>,
        bearer: Fingerprint,
        peer: Fingerprint,
        issued: u64,
    ) -> Self {
        Self {
            v: VERSION,
            kind: Kind::Relay,
            session: session.into(),
            bearer,
            peer,
            issued,
            expires: issued + PASS_LIFE.as_secs(),
            nonce: nonce(),
        }
    }
}

/// Never the same twice: that is what tells a ticket from its replay.
fn nonce() -> String {
    zyr_proto::random::alphanumeric_string(32)
}

/// The part every signed thing carries, read before the rest.
#[derive(Deserialize)]
struct Head {
    v: u32,
    kind: Kind,
    issued: u64,
    expires: u64,
}

/// Why a ticket or a pass was not honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Forged(Forged),
    Version(u32),
    WrongKind,
    /// Dated after now, beyond what two honest clocks may differ by.
    NotYet,
    Expired,
    Replayed,
    /// Signed and valid, but about another device.
    NotForMe {
        named: Fingerprint,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Forged(e) => write!(f, "ticket contrefait : {e}"),
            Refusal::Version(v) => write!(f, "ticket d'une version inconnue ({v})"),
            Refusal::WrongKind => f.write_str("ce n'est pas un ticket de ce genre"),
            Refusal::NotYet => f.write_str(
                "ticket daté du futur : les horloges de cet ordinateur et du serveur divergent de \
                 plus de cinq minutes",
            ),
            Refusal::Expired => f.write_str("ticket expiré"),
            Refusal::Replayed => f.write_str("ticket déjà employé"),
            Refusal::NotForMe { named } => {
                write!(f, "ticket adressé à un autre appareil ({named})")
            }
        }
    }
}

impl std::error::Error for Refusal {}

/// Reads tickets and passes against the server's key, and remembers
/// which it has already honoured.
pub struct Verifier {
    key: ServerPublicKey,
    /// Nonces honoured, each with the moment it expires: past that, plus
    /// the clock tolerance, a replay is refused for its date anyway and
    /// the nonce can be forgotten.
    seen: Mutex<HashMap<String, u64>>,
}

impl Verifier {
    pub fn new(key: ServerPublicKey) -> Self {
        Self {
            key,
            seen: Mutex::new(HashMap::new()),
        }
    }

    pub fn key(&self) -> &ServerPublicKey {
        &self.key
    }

    /// The ticket, seen from the device it presents another to.
    pub fn ticket_for_host(
        &self,
        signed: &Signed,
        me: Fingerprint,
        now: u64,
    ) -> Result<Ticket, Refusal> {
        let ticket: Ticket = self.open(signed, Kind::Session, now)?;
        if ticket.to != me {
            return Err(Refusal::NotForMe { named: ticket.to });
        }
        self.first_time(&ticket.nonce, ticket.expires, now)?;
        Ok(ticket)
    }

    /// The ticket, seen from the device that goes towards the other.
    pub fn ticket_for_client(
        &self,
        signed: &Signed,
        me: Fingerprint,
        now: u64,
    ) -> Result<Ticket, Refusal> {
        let ticket: Ticket = self.open(signed, Kind::Session, now)?;
        if ticket.from != me {
            return Err(Refusal::NotForMe { named: ticket.from });
        }
        self.first_time(&ticket.nonce, ticket.expires, now)?;
        Ok(ticket)
    }

    /// The pass, seen from the relay, presented by the device whose
    /// certificate carries that fingerprint.
    pub fn pass(&self, signed: &Signed, bearer: Fingerprint, now: u64) -> Result<Pass, Refusal> {
        let pass: Pass = self.open(signed, Kind::Relay, now)?;
        if pass.bearer != bearer {
            return Err(Refusal::NotForMe { named: pass.bearer });
        }
        self.first_time(&pass.nonce, pass.expires, now)?;
        Ok(pass)
    }

    fn open<T: DeserializeOwned>(
        &self,
        signed: &Signed,
        kind: Kind,
        now: u64,
    ) -> Result<T, Refusal> {
        let bytes = signed.verified(&self.key).map_err(Refusal::Forged)?;
        let head: Head = serde_json::from_slice(&bytes)
            .map_err(|e| Refusal::Forged(Forged::Body(e.to_string())))?;
        if head.v != VERSION {
            return Err(Refusal::Version(head.v));
        }
        if head.kind != kind {
            return Err(Refusal::WrongKind);
        }
        let skew = CLOCK_SKEW.as_secs();
        if head.issued > now + skew {
            return Err(Refusal::NotYet);
        }
        if head.expires + skew < now {
            return Err(Refusal::Expired);
        }
        serde_json::from_slice(&bytes).map_err(|e| Refusal::Forged(Forged::Body(e.to_string())))
    }

    /// Honours that nonce once.
    fn first_time(&self, nonce: &str, expires: u64, now: u64) -> Result<(), Refusal> {
        let mut seen = self.seen.lock().expect("nonces vus");
        let skew = CLOCK_SKEW.as_secs();
        seen.retain(|_, until| *until + skew >= now);
        if seen.contains_key(nonce) {
            return Err(Refusal::Replayed);
        }
        seen.insert(nonce.to_string(), expires);
        Ok(())
    }

    /// How many nonces are still remembered.
    pub fn remembered(&self) -> usize {
        self.seen.lock().expect("nonces vus").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::ServerKey;
    use zyr_transport::Identity;

    fn two() -> (Fingerprint, Fingerprint) {
        (
            Identity::generate().unwrap().fingerprint(),
            Identity::generate().unwrap().fingerprint(),
        )
    }

    #[test]
    fn a_ticket_opens_for_both_ends() {
        let key = ServerKey::generate();
        let (client, host) = two();
        let ticket = Ticket::new("s1", client, host, Grant::Owner, 1_000);
        let signed = key.seal(&ticket).unwrap();

        let verifier = Verifier::new(key.public());
        assert_eq!(
            verifier.ticket_for_host(&signed, host, 1_010).unwrap(),
            ticket
        );
        // L'autre bout le lit aussi, et c'est un autre vérificateur : le
        // sien, avec sa propre mémoire des tickets vus.
        let other_end = Verifier::new(key.public());
        assert_eq!(
            other_end.ticket_for_client(&signed, client, 1_010).unwrap(),
            ticket
        );
        assert_eq!(ticket.expires - ticket.issued, TICKET_LIFE.as_secs());
    }

    #[test]
    fn a_ticket_for_another_device_is_refused() {
        // Un ticket valide n'ouvre que la machine qu'il nomme : l'hôte
        // n'admet pas au nom d'un autre, et le client ne part pas au nom
        // d'un autre.
        let key = ServerKey::generate();
        let (client, host) = two();
        let stranger = Identity::generate().unwrap().fingerprint();
        let signed = key
            .seal(&Ticket::new("s1", client, host, Grant::Owner, 1_000))
            .unwrap();
        let verifier = Verifier::new(key.public());
        assert_eq!(
            verifier
                .ticket_for_host(&signed, stranger, 1_000)
                .unwrap_err(),
            Refusal::NotForMe { named: host }
        );
        assert_eq!(
            verifier
                .ticket_for_client(&signed, stranger, 1_000)
                .unwrap_err(),
            Refusal::NotForMe { named: client }
        );
        // Et un ticket refusé pour ça n'a pas été « vu » : le bon appareil
        // peut encore s'en servir.
        assert!(verifier.ticket_for_host(&signed, host, 1_000).is_ok());
    }

    #[test]
    fn a_ticket_from_another_server_is_refused() {
        let key = ServerKey::generate();
        let impostor = ServerKey::generate();
        let (client, host) = two();
        let signed = impostor
            .seal(&Ticket::new("s1", client, host, Grant::Owner, 1_000))
            .unwrap();
        assert_eq!(
            Verifier::new(key.public())
                .ticket_for_host(&signed, host, 1_000)
                .unwrap_err(),
            Refusal::Forged(Forged::Signature)
        );
    }

    #[test]
    fn a_ticket_is_honoured_once() {
        let key = ServerKey::generate();
        let (client, host) = two();
        let signed = key
            .seal(&Ticket::new("s1", client, host, Grant::Owner, 1_000))
            .unwrap();
        let verifier = Verifier::new(key.public());
        assert!(verifier.ticket_for_host(&signed, host, 1_000).is_ok());
        assert_eq!(
            verifier.ticket_for_host(&signed, host, 1_001).unwrap_err(),
            Refusal::Replayed
        );
    }

    #[test]
    fn the_dates_are_read_with_the_clocks_in_mind() {
        let key = ServerKey::generate();
        let (client, host) = two();
        let signed = key
            .seal(&Ticket::new("s1", client, host, Grant::Owner, 10_000))
            .unwrap();
        let skew = CLOCK_SKEW.as_secs();
        let life = TICKET_LIFE.as_secs();

        // Un peu d'avance sur l'horloge du serveur passe ; beaucoup ne
        // passe pas, et la phrase dit que ce sont les horloges.
        let verifier = Verifier::new(key.public());
        assert!(
            verifier
                .ticket_for_host(&signed, host, 10_000 - skew)
                .is_ok()
        );
        assert_eq!(
            Verifier::new(key.public())
                .ticket_for_host(&signed, host, 10_000 - skew - 1)
                .unwrap_err(),
            Refusal::NotYet
        );
        // Expiré, avec la même tolérance.
        assert!(
            Verifier::new(key.public())
                .ticket_for_host(&signed, host, 10_000 + life + skew)
                .is_ok()
        );
        assert_eq!(
            Verifier::new(key.public())
                .ticket_for_host(&signed, host, 10_000 + life + skew + 1)
                .unwrap_err(),
            Refusal::Expired
        );
    }

    #[test]
    fn honoured_nonces_are_forgotten_once_a_replay_would_be_expired_anyway() {
        let key = ServerKey::generate();
        let (client, host) = two();
        let verifier = Verifier::new(key.public());
        for n in 0..5 {
            let signed = key
                .seal(&Ticket::new(
                    format!("s{n}"),
                    client,
                    host,
                    Grant::Owner,
                    1_000,
                ))
                .unwrap();
            assert!(verifier.ticket_for_host(&signed, host, 1_000).is_ok());
        }
        assert_eq!(verifier.remembered(), 5);
        let later = 1_000 + TICKET_LIFE.as_secs() + CLOCK_SKEW.as_secs() + 1;
        let fresh = key
            .seal(&Ticket::new("later", client, host, Grant::Owner, later))
            .unwrap();
        assert!(verifier.ticket_for_host(&fresh, host, later).is_ok());
        assert_eq!(verifier.remembered(), 1);
    }

    #[test]
    fn a_pass_is_not_a_ticket_and_names_its_bearer() {
        let key = ServerKey::generate();
        let (client, host) = two();
        let pass = Pass::new("s1", client, host, 1_000);
        let signed = key.seal(&pass).unwrap();
        let verifier = Verifier::new(key.public());
        assert_eq!(
            verifier.ticket_for_host(&signed, host, 1_000).unwrap_err(),
            Refusal::WrongKind
        );
        assert_eq!(
            verifier.pass(&signed, host, 1_000).unwrap_err(),
            Refusal::NotForMe { named: client }
        );
        assert_eq!(verifier.pass(&signed, client, 1_000).unwrap(), pass);
        assert_eq!(pass.expires - pass.issued, PASS_LIFE.as_secs());

        let ticket = key
            .seal(&Ticket::new("s1", client, host, Grant::Owner, 1_000))
            .unwrap();
        assert_eq!(
            verifier.pass(&ticket, client, 1_000).unwrap_err(),
            Refusal::WrongKind
        );
    }

    #[test]
    fn the_grant_reads_as_words() {
        let text = serde_json::to_string(&Grant::Share { id: "p7".into() }).unwrap();
        assert_eq!(text, r#"{"by":"share","id":"p7"}"#);
        assert_eq!(
            serde_json::to_string(&Grant::Owner).unwrap(),
            r#"{"by":"owner"}"#
        );
    }
}
