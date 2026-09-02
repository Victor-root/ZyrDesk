//! The link of this computer to an account, held for as long as the
//! service runs.
//!
//! Without a link there is nothing here: no server is known, no
//! connection goes out, and whoever asks is told there is none. With
//! one, the live channel is kept open and what it says is kept for the
//! desk and the door; and two things go through it that nothing else
//! could carry. Whether this computer accepts remote access, which the
//! other computers of the account read as a green dot. And the meeting
//! of two computers that cannot see each other: the server presents them
//! to one another with a signed ticket, the one gone towards lets the
//! other in on the strength of it, and each says where it may be
//! reached.
//!
//! A computer of the account is reached under a road of its own,
//! `account:` and its identifier at the server, wherever the network
//! does not carry it: the desk reads that road and asks here for the
//! meeting before any tunnel is opened.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::JoinHandle;
use zyr_account::{
    AttachError, Credentials, Event, Link, Live, Registering, Rest, Snapshot, Start, Trust,
    Untrusted,
};
use zyr_broker::live::FromDevice;
use zyr_broker::rest::Access;
use zyr_broker::ticket::CLOCK_SKEW;
use zyr_broker::{Refusal, Verifier, now};
use zyr_control::{Holdup, WayId};
use zyr_proto::log::Log;
use zyr_proto::net::TUNNEL_PORT;
use zyr_transport::{Fingerprint, Identity};

use crate::machine::Hosting;
use crate::preferences::Remembered;
use crate::ways::Ways;

/// What the road to a computer of the account starts with.
const ROAD: &str = "account:";

/// How long the server gets to answer a session asked of it.
const RENDEZVOUS_PATIENCE: Duration = Duration::from_secs(10);

/// How long the far computer gets to say where it may be reached, once
/// the session is on.
const CANDIDATES_PATIENCE: Duration = Duration::from_secs(5);

/// How often what this computer says of itself is compared with what
/// the server was last told, and the ways opened by a meeting looked
/// over.
const TICK: Duration = Duration::from_secs(1);

/// The road to that device of the account, as a card carries it.
pub fn road_to(device: &str) -> String {
    format!("{ROAD}{device}")
}

/// The device a road names, when it is one of the account's.
pub fn device_of_road(host: &str) -> Option<&str> {
    host.strip_prefix(ROAD)
}

/// Why attaching did not happen.
#[derive(Debug)]
pub enum Attaching {
    /// The server presented a key nobody vouches for, and nothing was
    /// pinned: here it is, for the person to compare and confirm.
    Unpinned(Fingerprint),
    Refused(String),
}

/// Two computers presented to each other by the server.
#[derive(Debug)]
pub struct Rendezvous {
    pub session: String,
    pub peer: Fingerprint,
    /// What the far computer is called, for the journal.
    pub name: String,
    /// Where the far computer says it may be reached.
    pub candidates: Vec<SocketAddr>,
}

/// Where a session waits for its meeting: the other computer, and the
/// addresses it will name.
type Meeting = (Start, mpsc::UnboundedReceiver<Vec<SocketAddr>>);

/// The link, as the service holds it.
#[derive(Clone)]
pub struct Account(Arc<Inner>);

struct Inner {
    /// Where the link is kept on disk.
    path: PathBuf,
    log: Log,
    /// What only exists once the service runs.
    started: Mutex<Option<Started>>,
    /// The link and its channel, when there is one.
    held: Mutex<Option<Held>>,
    /// The computers a ticket let in, each until the ticket expires.
    admitted: Mutex<Vec<(Fingerprint, u64)>>,
    /// Woken whenever the admissions change, so the door reads them at
    /// once rather than at its next turn.
    changed: Notify,
    /// Sessions asked of the server and not yet matched, by the device
    /// gone towards.
    asked: Mutex<HashMap<String, oneshot::Sender<Result<Meeting, String>>>>,
    /// Sessions matched, waiting for where the far computer may be
    /// reached.
    expecting: Mutex<HashMap<String, mpsc::UnboundedSender<Vec<SocketAddr>>>>,
    /// The ways a meeting opened, and the session each stands for: the
    /// server is told when one closes.
    roads: Mutex<Vec<(WayId, String)>>,
}

struct Started {
    runtime: Handle,
    identity: Arc<Identity>,
    hosting: Hosting,
    remembered: Remembered,
    ways: Ways,
}

struct Held {
    link: Link,
    live: Live,
    verifier: Verifier,
    following: JoinHandle<()>,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.following.abort();
    }
}

impl Account {
    /// The link kept at that path, not yet read: nothing runs until
    /// `start`.
    pub fn at(path: PathBuf, log: Log) -> Self {
        Account(Arc::new(Inner {
            path,
            log,
            started: Mutex::new(None),
            held: Mutex::new(None),
            admitted: Mutex::new(Vec::new()),
            changed: Notify::new(),
            asked: Mutex::new(HashMap::new()),
            expecting: Mutex::new(HashMap::new()),
            roads: Mutex::new(Vec::new()),
        }))
    }

    /// Reads the link, and opens its channel when there is one.
    ///
    /// What this computer says of itself to the server is read from
    /// `hosting` and `remembered`; the ways opened by a meeting are
    /// watched in `ways`, so the server learns when one closes.
    pub fn start(
        &self,
        runtime: &Handle,
        identity: Arc<Identity>,
        hosting: Hosting,
        remembered: Remembered,
        ways: Ways,
    ) {
        let inner = &self.0;
        *inner.started.lock().expect("compte") = Some(Started {
            runtime: runtime.clone(),
            identity,
            hosting,
            remembered,
            ways,
        });
        match Link::read(&inner.path) {
            Ok(None) => inner
                .log
                .write("no account link: this computer knows no server"),
            Ok(Some(link)) => self.open(link),
            Err(e) => inner.log.write(&format!("account link left aside: {e}")),
        }
    }

    /// Opens the channel of that link, and follows it.
    fn open(&self, link: Link) {
        let inner = &self.0;
        let started = inner.started.lock().expect("compte");
        let Some(started) = started.as_ref() else {
            return;
        };
        let log = inner.log.clone();
        // The channel registers with the runtime as it opens, so it has
        // to be opened from inside it.
        let _guard = started.runtime.enter();
        // Held closed while the channel is opened and its follower
        // started: the follower reads what is held, and must not find
        // nothing there for the first thing the server says.
        let mut held = inner.held.lock().expect("lien de compte");
        let (live, events) = Live::open(
            link.clone(),
            started.identity.clone(),
            Arc::new(move |line: &str| log.write(line)),
        );
        let following = started.runtime.spawn(follow(self.clone(), events));
        inner.log.write(&format!(
            "account link with {} as {}, this computer is device {} there",
            link.server, link.username, link.device
        ));
        *held = Some(Held {
            verifier: Verifier::new(link.signing_key),
            link,
            live,
            following,
        });
    }

    /// The link, as it stands, or nothing.
    pub fn standing(&self) -> Option<zyr_control::Account> {
        let held = self.0.held.lock().expect("lien de compte");
        let held = held.as_ref()?;
        let snapshot = held.live.snapshot();
        Some(zyr_control::Account {
            server: held.link.server.clone(),
            name: snapshot
                .server
                .as_ref()
                .map_or_else(|| held.link.name.clone(), |server| server.name.clone()),
            username: held.link.username.clone(),
            device: held.link.device.clone(),
            connected: snapshot.connected,
            trouble: snapshot.trouble,
        })
    }

    /// What the server has said of the account, or nothing without a
    /// link.
    pub fn snapshot(&self) -> Option<Snapshot> {
        let held = self.0.held.lock().expect("lien de compte");
        held.as_ref().map(|held| held.live.snapshot())
    }

    /// The devices of the account, this computer marked among them.
    pub fn devices(&self) -> Vec<zyr_control::Device> {
        let held = self.0.held.lock().expect("lien de compte");
        let Some(held) = held.as_ref() else {
            return Vec::new();
        };
        held.live
            .snapshot()
            .devices
            .iter()
            .map(|device| zyr_control::Device {
                id: device.id.clone(),
                name: device.name.clone(),
                fingerprint: device.fingerprint,
                online: device.online,
                access: device.access,
                this: device.id == held.link.device,
                last_seen: device.last_seen,
            })
            .collect()
    }

    /// The computers a ticket let in, and still lets in.
    pub fn admitted(&self) -> Vec<Fingerprint> {
        let mut admitted = self.0.admitted.lock().expect("admis par ticket");
        let moment = now();
        // Read with the same tolerance the ticket was: two honest clocks
        // may differ, and the door must not close before the ticket does.
        admitted.retain(|(_, until)| *until + CLOCK_SKEW.as_secs() >= moment);
        admitted.iter().map(|(device, _)| *device).collect()
    }

    /// Waits for the admissions to change.
    pub async fn admissions_changed(&self) {
        self.0.changed.notified().await;
    }

    /// Attaches this computer to an account, and keeps the link.
    pub async fn attach(&self, asked: zyr_control::Attach) -> Result<(), Attaching> {
        let inner = &self.0;
        if inner.held.lock().expect("lien de compte").is_some() {
            return Err(Attaching::Refused(
                "cet ordinateur est déjà rattaché à un compte.\n  \
                 Détachez-le d'abord pour en changer."
                    .to_string(),
            ));
        }
        let identity = {
            let started = inner.started.lock().expect("compte");
            started.as_ref().map(|started| started.identity.clone())
        };
        let Some(identity) = identity else {
            return Err(Attaching::Refused(
                "le service n'est pas prêt à tenir un lien de compte".to_string(),
            ));
        };
        let trust = match asked.pin {
            Some(pin) => Trust::Pinned(pin),
            None => Trust::PublicOnly,
        };
        let credentials = Credentials {
            username: asked.username,
            password: asked.password,
            register: asked.register.map(|registering| Registering {
                email: registering.email,
                invitation: registering.invitation,
            }),
        };
        let name = match asked.name.trim() {
            "" => zyr_proto::machine::name(),
            named => named.to_string(),
        };
        let link =
            match zyr_account::attach(&asked.server, trust, &identity, &credentials, &name).await {
                Ok(link) => link,
                Err(AttachError::Untrusted(Untrusted::Unpinned { presented })) => {
                    inner.log.write(&format!(
                        "the server at {} presents a key nobody vouches for ({presented}), the \
                     person is asked",
                        asked.server
                    ));
                    return Err(Attaching::Unpinned(presented));
                }
                Err(e) => {
                    inner
                        .log
                        .write(&format!("attaching to {} refused: {e}", asked.server));
                    return Err(Attaching::Refused(e.to_string()));
                }
            };
        link.write(&inner.path).map_err(|e| {
            Attaching::Refused(format!("le lien de compte n'a pas pu être écrit : {e}"))
        })?;
        inner.log.write(&format!(
            "this computer is attached to {} as {} under the name « {name} »",
            link.server, link.username
        ));
        self.open(link);
        Ok(())
    }

    /// Takes this computer off its account.
    ///
    /// The link goes first, here, and the server is told last and only
    /// tried: a server that cannot be reached is no reason to stay
    /// attached, and the token dies with the file whatever it says.
    pub async fn detach(&self) -> Result<(), String> {
        let inner = &self.0;
        let (server, pin, token, device) = {
            let held = inner.held.lock().expect("lien de compte");
            let held = held
                .as_ref()
                .ok_or("cet ordinateur n'est rattaché à aucun compte")?;
            (
                held.link.server.clone(),
                held.link.pin,
                held.link.token.clone(),
                held.link.device.clone(),
            )
        };
        Link::remove(&inner.path)
            .map_err(|e| format!("le lien de compte n'a pas pu être effacé : {e}"))?;
        self.let_go();
        inner
            .log
            .write("this computer is detached from its account");
        let told = match Rest::new(&server, pin.map_or(Trust::PublicOnly, Trust::Pinned)) {
            Ok(rest) => rest
                .revoke_device(&token, &device)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        inner.log.write(&match told {
            Ok(()) => format!("and revoked at {server}"),
            Err(e) => format!("but {server} could not be told: {e}"),
        });
        Ok(())
    }

    /// Renames a device of the account, at the server.
    pub async fn rename(&self, device: &str, name: &str) -> Result<(), String> {
        let (rest, token, _) = self.door()?;
        rest.rename_device(&token, device, name)
            .await
            .map_err(|e| e.to_string())?;
        self.0.log.write(&format!(
            "device {device} of the account renamed « {name} »"
        ));
        Ok(())
    }

    /// Revokes a device of the account. This one is detached instead:
    /// the server would say so a moment later anyway, and the link is
    /// better gone at once.
    pub async fn revoke(&self, device: &str) -> Result<(), String> {
        let (rest, token, me) = self.door()?;
        if device == me {
            return self.detach().await;
        }
        rest.revoke_device(&token, device)
            .await
            .map_err(|e| e.to_string())?;
        self.0
            .log
            .write(&format!("device {device} revoked from the account"));
        Ok(())
    }

    /// The server's door, with this device's token and identifier.
    fn door(&self) -> Result<(Rest, String, String), String> {
        let held = self.0.held.lock().expect("lien de compte");
        let held = held
            .as_ref()
            .ok_or("cet ordinateur n'est rattaché à aucun compte")?;
        let trust = held.link.pin.map_or(Trust::PublicOnly, Trust::Pinned);
        let rest = Rest::new(&held.link.server, trust).map_err(|e| e.to_string())?;
        Ok((rest, held.link.token.clone(), held.link.device.clone()))
    }

    /// Asks the server to present this computer to that device of the
    /// account, and waits for the meeting.
    ///
    /// What comes back is whom to expect and where to knock. The far
    /// computer has let this one in by then, on the strength of the same
    /// ticket.
    pub async fn rendezvous(&self, device: &str) -> Result<Rendezvous, String> {
        let inner = &self.0;
        let (snapshot, me) = {
            let held = inner.held.lock().expect("lien de compte");
            let held = held
                .as_ref()
                .ok_or("cet ordinateur n'est rattaché à aucun compte")?;
            (held.live.snapshot(), held.link.device.clone())
        };
        if !snapshot.connected {
            return Err(format!(
                "le serveur du compte n'est pas joint en ce moment{}",
                snapshot
                    .trouble
                    .map_or_else(String::new, |why| format!(" : {why}"))
            ));
        }
        let target = snapshot
            .devices
            .iter()
            .chain(snapshot.shares.iter().map(|share| &share.device))
            .find(|known| known.id == device)
            .ok_or_else(|| format!("l'appareil {device} n'est pas dans le compte"))?;
        if target.id == me {
            return Err("c'est cet ordinateur-ci".to_string());
        }
        if !target.online {
            return Err(format!(
                "{} n'est pas connecté au serveur en ce moment",
                target.name
            ));
        }
        if target.access != Access::Ready {
            return Err(format!(
                "{} n'accepte pas l'accès distant en ce moment : {}",
                target.name,
                target.access.explanation()
            ));
        }

        let (matched, waiting) = oneshot::channel();
        inner
            .asked
            .lock()
            .expect("sessions demandées")
            .insert(device.to_string(), matched);
        {
            let held = inner.held.lock().expect("lien de compte");
            if let Some(held) = held.as_ref() {
                held.live.say(FromDevice::SessionOpen {
                    to: device.to_string(),
                });
            }
        }
        inner.log.write(&format!(
            "asking the server for a session towards {} ({device})",
            target.name
        ));
        let (start, mut candidates) = match tokio::time::timeout(RENDEZVOUS_PATIENCE, waiting).await
        {
            Ok(Ok(Ok(meeting))) => meeting,
            Ok(Ok(Err(refused))) => {
                return Err(format!(
                    "le serveur a refusé la session vers {} : {refused}",
                    target.name
                ));
            }
            Ok(Err(_)) => {
                return Err("le lien de compte s'est fermé pendant la demande".to_string());
            }
            Err(_) => {
                inner
                    .asked
                    .lock()
                    .expect("sessions demandées")
                    .remove(device);
                return Err(format!(
                    "le serveur n'a pas répondu à la demande de session vers {}",
                    target.name
                ));
            }
        };
        let named = match tokio::time::timeout(CANDIDATES_PATIENCE, candidates.recv()).await {
            Ok(Some(named)) => named,
            _ => Vec::new(),
        };
        inner
            .expecting
            .lock()
            .expect("candidats attendus")
            .remove(&start.session);
        inner.log.write(&format!(
            "session {} matched with {} ({}), which answers at {}",
            start.session,
            start.peer.name,
            start.peer.fingerprint,
            if named.is_empty() {
                "no address it named".to_string()
            } else {
                named
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        Ok(Rendezvous {
            session: start.session,
            peer: start.peer.fingerprint,
            name: start.peer.name,
            candidates: named,
        })
    }

    /// Notes that a meeting opened that way: the server is told when it
    /// closes.
    pub fn follow(&self, way: WayId, session: String) {
        self.0
            .roads
            .lock()
            .expect("voies du compte")
            .push((way, session));
    }

    /// Tells the server that session is over.
    pub fn ended(&self, session: &str) {
        let inner = &self.0;
        inner
            .roads
            .lock()
            .expect("voies du compte")
            .retain(|(_, road)| road != session);
        inner
            .expecting
            .lock()
            .expect("candidats attendus")
            .remove(session);
        let held = inner.held.lock().expect("lien de compte");
        if let Some(held) = held.as_ref() {
            held.live.say(FromDevice::SessionEnd {
                session: session.to_string(),
            });
        }
    }

    /// What the server said, as the follower hears it.
    fn heard(&self, event: Event) {
        let inner = &self.0;
        match event {
            Event::SessionStart(start) => self.matched(*start),
            Event::SessionCandidates {
                session,
                candidates,
            } => {
                let expecting = inner.expecting.lock().expect("candidats attendus");
                if let Some(waiting) = expecting.get(&session) {
                    let _ = waiting.send(candidates);
                }
            }
            Event::SessionEnd { session } => {
                inner
                    .expecting
                    .lock()
                    .expect("candidats attendus")
                    .remove(&session);
                inner
                    .roads
                    .lock()
                    .expect("voies du compte")
                    .retain(|(_, road)| *road != session);
                inner
                    .log
                    .write(&format!("the server says session {session} is over"));
            }
            Event::SessionRefused { to, code } => {
                if let Some(waiting) = inner.asked.lock().expect("sessions demandées").remove(&to)
                {
                    let _ = waiting.send(Err(code.explanation().to_string()));
                }
                inner.log.write(&format!(
                    "the server refused a session towards {to}: {code}"
                ));
            }
            Event::TokenRenewed(token) => {
                let mut held = inner.held.lock().expect("lien de compte");
                if let Some(held) = held.as_mut() {
                    held.link.token = token;
                    inner.log.write(&match held.link.write(&inner.path) {
                        Ok(()) => {
                            "the server renewed this device's token, the link is written again"
                                .to_string()
                        }
                        Err(e) => format!(
                            "the server renewed this device's token, and the link could not be \
                             written: {e}"
                        ),
                    });
                }
            }
            Event::Revoked => {
                inner
                    .log
                    .write("this device was revoked from the account, the link is forgotten");
                if let Err(e) = Link::remove(&inner.path) {
                    inner
                        .log
                        .write(&format!("the account link could not be erased: {e}"));
                }
                self.let_go();
            }
            Event::Untrusted(why) => inner
                .log
                .write(&format!("the server could not be believed: {why}")),
        }
    }

    /// A session the server matched: this computer is the one gone
    /// towards, and lets the other in; or the one going, and is told
    /// whom to expect.
    fn matched(&self, start: Start) {
        let inner = &self.0;
        let me = {
            let started = inner.started.lock().expect("compte");
            started
                .as_ref()
                .map(|started| started.identity.fingerprint())
        };
        let Some(me) = me else {
            return;
        };
        let held = inner.held.lock().expect("lien de compte");
        let Some(held) = held.as_ref() else {
            return;
        };
        let moment = now();
        match held.verifier.ticket_for_host(&start.ticket, me, moment) {
            Ok(ticket) => {
                if ticket.from != start.peer.fingerprint {
                    inner.log.write(&format!(
                        "session {}: the ticket names {} and the server names {}, so nobody is \
                         let in",
                        start.session, ticket.from, start.peer.fingerprint
                    ));
                    return;
                }
                self.admit(start.peer.fingerprint, ticket.expires);
                held.live.say(FromDevice::SessionCandidates {
                    session: start.session.clone(),
                    candidates: where_this_computer_answers(),
                });
                inner.log.write(&format!(
                    "{} ({}) is presented by the server for session {}, and may come in for as \
                     long as the ticket lives",
                    start.peer.name, start.peer.account, start.session
                ));
                return;
            }
            Err(Refusal::NotForMe { .. }) => {}
            Err(refusal) => {
                inner
                    .log
                    .write(&format!("a ticket from the server is refused: {refusal}"));
                return;
            }
        }
        match held.verifier.ticket_for_client(&start.ticket, me, moment) {
            Ok(ticket) if ticket.to == start.peer.fingerprint => {
                let (naming, named) = mpsc::unbounded_channel();
                // Where the far computer answers is expected before the
                // meeting is handed over: it says so the moment it has let
                // this one in, which is before anybody here has asked.
                inner
                    .expecting
                    .lock()
                    .expect("candidats attendus")
                    .insert(start.session.clone(), naming);
                match inner
                    .asked
                    .lock()
                    .expect("sessions demandées")
                    .remove(&start.peer.device)
                {
                    Some(waiting) => {
                        let _ = waiting.send(Ok((start, named)));
                    }
                    None => inner.log.write(&format!(
                        "session {} towards {} started without anybody here having asked",
                        start.session, start.peer.name
                    )),
                }
            }
            Ok(ticket) => inner.log.write(&format!(
                "session {}: the ticket goes towards {} and the server names {}",
                start.session, ticket.to, start.peer.fingerprint
            )),
            Err(refusal) => inner
                .log
                .write(&format!("a ticket from the server is refused: {refusal}")),
        }
    }

    /// Lets that computer in until then, and wakes the door.
    pub(crate) fn admit(&self, device: Fingerprint, until: u64) {
        let mut admitted = self.0.admitted.lock().expect("admis par ticket");
        admitted.retain(|(known, _)| *known != device);
        admitted.push((device, until));
        self.0.changed.notify_one();
    }

    /// Once a second: what this computer says of itself, and the ways a
    /// meeting opened.
    fn every_second(&self, told: &mut Option<Access>) {
        let inner = &self.0;
        let started = inner.started.lock().expect("compte");
        let Some(started) = started.as_ref() else {
            return;
        };
        let held = inner.held.lock().expect("lien de compte");
        let Some(held) = held.as_ref() else {
            return;
        };
        let access = access_now(&started.hosting, &started.remembered);
        if *told != Some(access) {
            held.live.set_access(access);
            *told = Some(access);
            inner.log.write(&format!(
                "the server is told this computer's remote access is {access:?}"
            ));
        }
        let over: Vec<String> = {
            let mut roads = inner.roads.lock().expect("voies du compte");
            let (open, closed): (Vec<_>, Vec<_>) = roads
                .drain(..)
                .partition(|(way, _)| started.ways.still_open(*way));
            *roads = open;
            closed.into_iter().map(|(_, session)| session).collect()
        };
        for session in over {
            held.live.say(FromDevice::SessionEnd {
                session: session.clone(),
            });
            inner.log.write(&format!(
                "its way closed, the server is told session {session} is over"
            ));
        }
    }

    /// Drops the link and everything that waited on it.
    fn let_go(&self) {
        let inner = &self.0;
        let gone = inner.held.lock().expect("lien de compte").take();
        drop(gone);
        inner.admitted.lock().expect("admis par ticket").clear();
        inner.changed.notify_one();
        inner.asked.lock().expect("sessions demandées").clear();
        inner.expecting.lock().expect("candidats attendus").clear();
        inner.roads.lock().expect("voies du compte").clear();
    }
}

/// Follows the channel for as long as it is open.
async fn follow(account: Account, mut events: mpsc::UnboundedReceiver<Event>) {
    let mut told = None;
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            heard = events.recv() => match heard {
                Some(event) => account.heard(event),
                None => return,
            },
            _ = tick.tick() => account.every_second(&mut told),
        }
    }
}

/// Whether this computer accepts remote access, as the server is told.
fn access_now(hosting: &Hosting, remembered: &Remembered) -> Access {
    if !remembered.remote_access() {
        return Access::Off;
    }
    match hosting.standing() {
        None => Access::Ready,
        Some(Holdup::Starting) => Access::Starting,
        Some(Holdup::EngineMissing) => Access::EngineMissing,
        Some(Holdup::EngineWontStand) => Access::EngineWontStand,
    }
}

/// Where this computer may be reached, card by card, for the far one to
/// try.
fn where_this_computer_answers() -> Vec<SocketAddr> {
    zyr_proto::machine::addresses()
        .into_iter()
        .map(|card| SocketAddr::from((card.address, TUNNEL_PORT)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use zyr_broker::rest::Registration;
    use zyr_control::Attach;
    use zyr_server::config::Config;
    use zyr_transport::identity::public_key_fingerprint;

    /// Past this, something that should have happened has not.
    const PATIENCE: Duration = Duration::from_secs(8);

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    #[test]
    fn the_road_to_a_computer_of_the_account_reads_back() {
        assert_eq!(road_to("d2"), "account:d2");
        assert_eq!(device_of_road("account:d2"), Some("d2"));
        assert_eq!(device_of_road("192.168.1.20"), None);
        assert_eq!(device_of_road("pc de victor.local"), None);
    }

    #[test]
    fn what_the_server_is_told_follows_the_switch_and_the_engine() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-account-{}-acces",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let hosting = Hosting::new();
        let remembered = Remembered::at(folder.join("preferences.conf"));
        // Voulu et en train de démarrer : pas encore prêt, et on le dit.
        assert_eq!(access_now(&hosting, &remembered), Access::Starting);
        hosting.open();
        assert_eq!(access_now(&hosting, &remembered), Access::Ready);
        hosting.held_by(Holdup::EngineMissing);
        assert_eq!(access_now(&hosting, &remembered), Access::EngineMissing);
        // Coupé exprès, quel que soit l'état du moteur : c'est ce qui se
        // lit en gris chez les autres.
        remembered.set_remote_access(false).unwrap();
        hosting.open();
        assert_eq!(access_now(&hosting, &remembered), Access::Off);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_ticket_lets_a_computer_in_for_as_long_as_it_lives() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-account-{}-admis",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let log = Log::open(&folder.join("service.log")).unwrap();
        let account = Account::at(folder.join("account.conf"), log);
        assert!(account.admitted().is_empty());

        let moment = now();
        account.admit(fingerprint(1), moment + 60);
        account.admit(
            fingerprint(2),
            moment.saturating_sub(CLOCK_SKEW.as_secs() + 1),
        );
        // Le premier est encore admis ; le second est expiré depuis plus
        // longtemps que deux horloges honnêtes ne divergent.
        assert_eq!(account.admitted(), vec![fingerprint(1)]);
        // Le même ordinateur présenté deux fois n'entre qu'une fois.
        account.admit(fingerprint(1), moment + 120);
        assert_eq!(account.admitted(), vec![fingerprint(1)]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A server on this machine, with a certificate nobody vouches for.
    struct Server {
        running: zyr_server::Running,
        folder: PathBuf,
        fingerprint: Fingerprint,
    }

    impl Server {
        async fn start() -> Self {
            let folder = std::env::temp_dir().join(format!(
                "zyrdeskd-account-{}-serveur",
                zyr_proto::random::alphanumeric_string(8)
            ));
            std::fs::create_dir_all(&folder).unwrap();
            let generated =
                rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
            let certificate = folder.join("server.crt");
            let key = folder.join("server.key");
            std::fs::write(&certificate, generated.cert.pem()).unwrap();
            std::fs::write(&key, generated.signing_key.serialize_pem()).unwrap();
            let fingerprint = public_key_fingerprint(generated.cert.der()).unwrap();
            let config = Config::parse(&format!(
                r#"
name = "Essai"
data_dir = "{}"

[api]
listen = "127.0.0.1:0"
tls_cert = "{}"
tls_key = "{}"
public_url = "https://essai.invalid"

[registration]
policy = "open"

[limits]
login_attempts_per_minute = 1000
"#,
                folder.display(),
                certificate.display(),
                key.display()
            ))
            .unwrap();
            let running = zyr_server::start(config).await.unwrap();
            Self {
                running,
                folder,
                fingerprint,
            }
        }

        fn address(&self) -> String {
            self.running.address.to_string()
        }

        async fn stop(self) {
            self.running.stop().await;
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    /// One computer of the account, with everything the service would
    /// hold for it.
    struct Computer {
        account: Account,
        identity: Arc<Identity>,
        hosting: Hosting,
        folder: PathBuf,
    }

    impl Computer {
        fn new(what: &str) -> Self {
            let folder = std::env::temp_dir().join(format!(
                "zyrdeskd-account-{}-{what}",
                zyr_proto::random::alphanumeric_string(8)
            ));
            let log = Log::open(&folder.join("service.log")).unwrap();
            let identity = Arc::new(Identity::generate().unwrap());
            let hosting = Hosting::new();
            let account = Account::at(folder.join("account.conf"), log.clone());
            account.start(
                &Handle::current(),
                identity.clone(),
                hosting.clone(),
                Remembered::at(folder.join("preferences.conf")),
                Ways::new(log),
            );
            Self {
                account,
                identity,
                hosting,
                folder,
            }
        }

        async fn attach(&self, server: &Server, name: &str, register: bool) {
            self.account
                .attach(Attach {
                    server: server.address(),
                    username: "victor".to_string(),
                    password: "douze caractères".to_string(),
                    register: register.then(zyr_control::Registering::default),
                    name: name.to_string(),
                    pin: Some(server.fingerprint),
                })
                .await
                .unwrap();
        }

        /// Waits until what the account says satisfies `what`.
        async fn until(&self, why: &str, what: impl Fn(&Account) -> bool) {
            let deadline = tokio::time::Instant::now() + PATIENCE;
            while !what(&self.account) {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "{why} : {:?}",
                    self.account.standing()
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    impl Drop for Computer {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_computers_of_one_account_see_each_other_and_are_presented() {
        let server = Server::start().await;

        // Un serveur que personne ne garantit est refusé tant que sa clé
        // n'est pas épinglée : c'est ce que la fenêtre montre pour
        // demander la comparaison.
        let pc = Computer::new("pc");
        let refused = pc
            .account
            .attach(Attach {
                server: server.address(),
                username: "victor".to_string(),
                password: "douze caractères".to_string(),
                register: Some(zyr_control::Registering::default()),
                name: "PC".to_string(),
                pin: None,
            })
            .await
            .unwrap_err();
        assert!(
            matches!(refused, Attaching::Unpinned(presented) if presented == server.fingerprint),
            "{refused:?}"
        );
        assert!(pc.account.standing().is_none());

        pc.attach(&server, "PC de Victor", true).await;
        let standing = pc.account.standing().unwrap();
        assert_eq!(standing.username, "victor");
        assert!(
            Link::read(&pc.folder.join("account.conf"))
                .unwrap()
                .is_some()
        );
        pc.until("le PC n'est jamais relié", |account| {
            account.standing().is_some_and(|it| it.connected)
        })
        .await;
        assert_eq!(
            pc.account
                .snapshot()
                .unwrap()
                .server
                .map(|s| s.registration),
            Some(Registration::Open)
        );

        // Le PC accepte l'accès distant ; le serveur l'apprend au tour
        // suivant, et le portable le voit prêt en arrivant.
        pc.hosting.open();
        let portable = Computer::new("portable");
        portable.attach(&server, "Portable", false).await;
        portable
            .until("le portable ne voit jamais le PC prêt", |account| {
                account.devices().iter().any(|device| {
                    device.name == "PC de Victor" && device.online && device.access == Access::Ready
                })
            })
            .await;
        let devices = portable.account.devices();
        assert_eq!(devices.len(), 2);
        assert!(
            devices
                .iter()
                .any(|device| device.this && device.name == "Portable")
        );
        let pc_device = devices
            .iter()
            .find(|device| device.name == "PC de Victor")
            .unwrap()
            .id
            .clone();

        // Le rendez-vous : le portable est présenté au PC, qui le laisse
        // entrer sur la foi du ticket et dit où il répond.
        let met = portable.account.rendezvous(&pc_device).await.unwrap();
        assert_eq!(met.peer, pc.identity.fingerprint());
        assert_eq!(met.name, "PC de Victor");
        assert_eq!(
            met.candidates,
            where_this_computer_answers(),
            "le PC dit où il répond, c'est-à-dire cette machine"
        );
        pc.until("le PC n'a jamais admis le portable", |account| {
            account.admitted() == vec![portable.identity.fingerprint()]
        })
        .await;
        portable.account.ended(&met.session);

        // Un appareil qui n'est pas prêt est refusé ici même, avec la
        // raison, sans déranger le serveur.
        pc.hosting.held_by(Holdup::EngineMissing);
        portable
            .until("le portable ne voit jamais le PC sans moteur", |account| {
                account
                    .devices()
                    .iter()
                    .any(|device| device.id == pc_device && device.access == Access::EngineMissing)
            })
            .await;
        let refused = portable.account.rendezvous(&pc_device).await.unwrap_err();
        assert!(refused.contains("moteur hôte absent"), "{refused}");

        // Renommé depuis le portable, le PC se voit sous son nouveau nom.
        portable
            .account
            .rename(&pc_device, "PC du salon")
            .await
            .unwrap();
        pc.until("le PC n'apprend jamais son nouveau nom", |account| {
            account
                .devices()
                .iter()
                .any(|device| device.this && device.name == "PC du salon")
        })
        .await;

        // Révoqué depuis le PC, le portable oublie son lien tout seul.
        let portable_device = portable.account.standing().unwrap().device;
        pc.account.revoke(&portable_device).await.unwrap();
        portable
            .until("le portable garde son lien", |account| {
                account.standing().is_none()
            })
            .await;
        assert!(
            Link::read(&portable.folder.join("account.conf"))
                .unwrap()
                .is_none()
        );
        assert!(portable.account.devices().is_empty());

        // Et le PC se détache lui-même : le fichier part, le serveur ne
        // le compte plus.
        pc.account.detach().await.unwrap();
        assert!(pc.account.standing().is_none());
        assert!(
            Link::read(&pc.folder.join("account.conf"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            pc.account.detach().await.unwrap_err(),
            "cet ordinateur n'est rattaché à aucun compte"
        );

        server.stop().await;
    }
}
