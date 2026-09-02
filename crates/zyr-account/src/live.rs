//! The live channel, from the device's side.
//!
//! One task holds the WebSocket for as long as the link exists: it
//! answers the challenge, keeps what the server says in a snapshot the
//! rest of the product reads, passes on what a session needs, and comes
//! back on its own when the connection breaks, with a wait that grows
//! and never gives up. Silence for a minute and a half is a break.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use zyr_broker::live::{FromDevice, FromServer, Peer, Relay};
use zyr_broker::proof::{Purpose, challenge_message};
use zyr_broker::rest::{Access, ContactInfo, DeviceInfo, ServerInfo, ShareInfo, paths};
use zyr_broker::{Code, PROTOCOL, Signed};
use zyr_transport::Identity;
use zyr_transport::trust::{Trust, Untrusted, client_config};

use crate::address::host_and_port;
use crate::link::Link;

/// How long the server gets to open with its challenge, and to welcome.
const PATIENCE: Duration = Duration::from_secs(10);

/// A ping goes out this often, so the silence below is never ours.
const HEARTBEAT: Duration = Duration::from_secs(30);

/// A server that has said nothing for this long is gone.
const SILENCE: Duration = Duration::from_secs(90);

/// The wait before coming back, at first and at most.
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MOST: Duration = Duration::from_secs(120);

/// What the rest of the product reads of the account.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// The channel is open and welcomed.
    pub connected: bool,
    pub server: Option<ServerInfo>,
    /// This device's identifier at the server.
    pub me: Option<String>,
    pub devices: Vec<DeviceInfo>,
    pub contacts: Vec<ContactInfo>,
    pub shares: Vec<ShareInfo>,
    /// Why the last attempt failed, while it stays disconnected.
    pub trouble: Option<String>,
}

impl Snapshot {
    /// Takes in what the server said.
    pub fn apply(&mut self, told: &FromServer) {
        match told {
            FromServer::Welcome {
                server,
                me,
                devices,
                contacts,
                shares,
                ..
            } => {
                self.connected = true;
                self.trouble = None;
                self.server = Some(server.clone());
                self.me = Some(me.clone());
                self.devices = devices.clone();
                self.contacts = contacts.clone();
                self.shares = shares.clone();
            }
            FromServer::Presence {
                device,
                online,
                access,
            } => {
                for known in self
                    .devices
                    .iter_mut()
                    .chain(self.shares.iter_mut().map(|share| &mut share.device))
                {
                    if known.id == *device {
                        known.online = *online;
                        known.access = *access;
                    }
                }
            }
            FromServer::DeviceAdded { device } | FromServer::DeviceRenamed { device } => {
                match self.devices.iter_mut().find(|known| known.id == device.id) {
                    Some(known) => *known = device.clone(),
                    None => self.devices.push(device.clone()),
                }
                for share in &mut self.shares {
                    if share.device.id == device.id {
                        share.device = device.clone();
                    }
                }
            }
            FromServer::DeviceRevoked { device } => {
                self.devices.retain(|known| known.id != *device);
                self.shares.retain(|share| share.device.id != *device);
            }
            FromServer::ContactRequested { contact } | FromServer::ContactAnswered { contact } => {
                match self
                    .contacts
                    .iter_mut()
                    .find(|known| known.id == contact.id)
                {
                    Some(known) => *known = contact.clone(),
                    None => self.contacts.push(contact.clone()),
                }
            }
            FromServer::ContactRemoved { contact } => {
                self.contacts.retain(|known| known.id != *contact);
            }
            FromServer::ShareGiven { share } => {
                match self.shares.iter_mut().find(|known| known.id == share.id) {
                    Some(known) => *known = share.clone(),
                    None => self.shares.push(share.clone()),
                }
            }
            FromServer::ShareRemoved { share } => {
                self.shares.retain(|known| known.id != *share);
            }
            FromServer::Challenge { .. }
            | FromServer::SessionStart { .. }
            | FromServer::SessionCandidates { .. }
            | FromServer::SessionEnd { .. }
            | FromServer::SessionRefused { .. }
            | FromServer::Bye { .. } => {}
        }
    }

    /// What the channel leaves behind as it breaks.
    fn disconnected(&mut self, why: String) {
        self.connected = false;
        self.trouble = Some(why);
    }
}

/// A session the server has just matched, seen from this side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub session: String,
    pub ticket: Signed,
    pub peer: Peer,
    pub relay: Option<Relay>,
}

/// What the channel passes on, for whoever drives sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    SessionStart(Box<Start>),
    SessionCandidates {
        session: String,
        candidates: Vec<std::net::SocketAddr>,
    },
    SessionEnd {
        session: String,
    },
    SessionRefused {
        to: String,
        code: Code,
    },
    /// The server handed a fresh token: the link is to be written again
    /// with it.
    TokenRenewed(String),
    /// This device is no longer of the account: the link is to be
    /// forgotten.
    Revoked,
    /// The server was not believed: the link is to be looked at.
    Untrusted(Untrusted),
}

enum Order {
    Say(FromDevice),
    Access(Access),
    Stop,
}

/// The channel, held by the service for as long as the link exists.
pub struct Live {
    snapshot: Arc<Mutex<Snapshot>>,
    orders: mpsc::UnboundedSender<Order>,
}

impl Live {
    /// Opens the channel and keeps it open.
    ///
    /// What the server says of the account lands in the snapshot; what a
    /// session needs comes out of the receiver.
    pub fn open(
        link: Link,
        identity: Arc<Identity>,
        log: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let (orders, taking_orders) = mpsc::unbounded_channel();
        let (events, taking_events) = mpsc::unbounded_channel();
        tokio::spawn(keep_open(
            link,
            identity,
            log,
            snapshot.clone(),
            taking_orders,
            events,
        ));
        (Self { snapshot, orders }, taking_events)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().expect("instantané").clone()
    }

    /// Tells the server whether this device accepts remote access.
    pub fn set_access(&self, access: Access) {
        let _ = self.orders.send(Order::Access(access));
    }

    pub fn say(&self, said: FromDevice) {
        let _ = self.orders.send(Order::Say(said));
    }

    /// Closes the channel for good.
    pub fn stop(&self) {
        let _ = self.orders.send(Order::Stop);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop();
    }
}

type Channel = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Why one opening of the channel ended.
enum Ended {
    /// Told to stop, or revoked: no coming back.
    ForGood,
    /// Broken, or refused for now: come back later.
    ForNow(String),
}

async fn keep_open(
    link: Link,
    identity: Arc<Identity>,
    log: Arc<dyn Fn(&str) + Send + Sync>,
    snapshot: Arc<Mutex<Snapshot>>,
    mut orders: mpsc::UnboundedReceiver<Order>,
    events: mpsc::UnboundedSender<Event>,
) {
    let mut access = Access::Off;
    let mut wait = RETRY_FIRST;
    loop {
        let ended = serve_once(
            &link,
            &identity,
            &log,
            &snapshot,
            &mut orders,
            &events,
            &mut access,
            &mut wait,
        )
        .await;
        let why = match ended {
            Ended::ForGood => break,
            Ended::ForNow(why) => why,
        };
        snapshot
            .lock()
            .expect("instantané")
            .disconnected(why.clone());
        log(&format!(
            "account channel down ({why}), back in {} s",
            wait.as_secs()
        ));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            order = orders.recv() => match order {
                Some(Order::Access(new)) => access = new,
                Some(Order::Say(_)) => {}
                Some(Order::Stop) | None => break,
            }
        }
        wait = (wait * 2).min(RETRY_MOST);
    }
    snapshot.lock().expect("instantané").connected = false;
}

#[allow(clippy::too_many_arguments)]
async fn serve_once(
    link: &Link,
    identity: &Identity,
    log: &Arc<dyn Fn(&str) + Send + Sync>,
    snapshot: &Arc<Mutex<Snapshot>>,
    orders: &mut mpsc::UnboundedReceiver<Order>,
    events: &mpsc::UnboundedSender<Event>,
    access: &mut Access,
    wait: &mut Duration,
) -> Ended {
    let mut channel = match connect(link).await {
        Ok(channel) => channel,
        Err(Failure::Untrusted(why)) => {
            let _ = events.send(Event::Untrusted(why.clone()));
            return Ended::ForNow(why.to_string());
        }
        Err(Failure::Broken(why)) => return Ended::ForNow(why),
    };
    let nonce = match next(&mut channel, PATIENCE).await {
        Ok(Some(FromServer::Challenge { nonce })) => nonce,
        Ok(Some(other)) => return Ended::ForNow(format!("le serveur a ouvert par {other:?}")),
        Ok(None) => return Ended::ForNow("le serveur a fermé sans un mot".to_string()),
        Err(why) => return Ended::ForNow(why),
    };
    let signature =
        match identity.sign(&challenge_message(&link.signing_key, &nonce, Purpose::Live)) {
            Ok(signature) => signature,
            Err(e) => return Ended::ForNow(e.to_string()),
        };
    if let Err(why) = say(
        &mut channel,
        &FromDevice::Hello {
            protocol: PROTOCOL,
            build: zyr_proto::BUILD.to_string(),
            signature: BASE64.encode(signature),
        },
    )
    .await
    {
        return Ended::ForNow(why);
    }
    match next(&mut channel, PATIENCE).await {
        Ok(Some(welcome @ FromServer::Welcome { .. })) => {
            if let FromServer::Welcome {
                token: Some(token), ..
            } = &welcome
            {
                let _ = events.send(Event::TokenRenewed(token.clone()));
            }
            snapshot.lock().expect("instantané").apply(&welcome);
        }
        Ok(Some(FromServer::Bye { code })) => {
            return match code {
                Code::DeviceRevoked => {
                    let _ = events.send(Event::Revoked);
                    Ended::ForGood
                }
                other => Ended::ForNow(other.explanation().to_string()),
            };
        }
        Ok(Some(other)) => return Ended::ForNow(format!("attendu un accueil, reçu {other:?}")),
        Ok(None) => return Ended::ForNow("le serveur a fermé avant l'accueil".to_string()),
        Err(why) => return Ended::ForNow(why),
    }
    log(&format!("account channel open with {}", link.server));
    *wait = RETRY_FIRST;
    if *access != Access::Off
        && let Err(why) = say(&mut channel, &FromDevice::State { access: *access }).await
    {
        return Ended::ForNow(why);
    }

    let mut heartbeat = tokio::time::interval(HEARTBEAT);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            order = orders.recv() => {
                let said = match order {
                    Some(Order::Say(said)) => said,
                    Some(Order::Access(new)) => {
                        *access = new;
                        FromDevice::State { access: new }
                    }
                    Some(Order::Stop) | None => {
                        let _ = channel.close(None).await;
                        return Ended::ForGood;
                    }
                };
                if let Err(why) = say(&mut channel, &said).await {
                    return Ended::ForNow(why);
                }
            }
            _ = heartbeat.tick() => {
                if channel.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return Ended::ForNow("le battement n'est pas parti".to_string());
                }
            }
            heard = next(&mut channel, SILENCE) => match heard {
                Ok(Some(told)) => {
                    snapshot.lock().expect("instantané").apply(&told);
                    match told {
                        FromServer::SessionStart { session, ticket, peer, relay } => {
                            let _ = events.send(Event::SessionStart(Box::new(Start {
                                session,
                                ticket,
                                peer,
                                relay,
                            })));
                        }
                        FromServer::SessionCandidates { session, candidates } => {
                            let _ = events.send(Event::SessionCandidates { session, candidates });
                        }
                        FromServer::SessionEnd { session } => {
                            let _ = events.send(Event::SessionEnd { session });
                        }
                        FromServer::SessionRefused { to, code } => {
                            let _ = events.send(Event::SessionRefused { to, code });
                        }
                        FromServer::Bye { code: Code::DeviceRevoked } => {
                            let _ = events.send(Event::Revoked);
                            return Ended::ForGood;
                        }
                        FromServer::Bye { code } => {
                            return Ended::ForNow(code.explanation().to_string());
                        }
                        _ => {}
                    }
                }
                Ok(None) => return Ended::ForNow("le serveur a fermé le canal".to_string()),
                Err(why) => return Ended::ForNow(why),
            },
        }
    }
}

/// Why the channel could not be opened.
enum Failure {
    Untrusted(Untrusted),
    Broken(String),
}

async fn connect(link: &Link) -> Result<Channel, Failure> {
    let (host, port) = host_and_port(&link.server).map_err(|e| Failure::Broken(e.to_string()))?;
    let stream = tokio::time::timeout(
        PATIENCE,
        TcpStream::connect((host.trim_matches(['[', ']']), port)),
    )
    .await
    .map_err(|_| Failure::Broken(format!("{host}:{port} ne répond pas")))?
    .map_err(|e| Failure::Broken(format!("{host}:{port} : {e}")))?;
    let trust = match link.pin {
        Some(pin) => Trust::Pinned(pin),
        None => Trust::PublicOnly,
    };
    let (config, verifier) = client_config(trust);
    let request = Request::builder()
        .uri(format!("wss://{host}:{port}{}", paths::LIVE))
        .header("Authorization", format!("Bearer {}", link.token))
        .header("Host", format!("{host}:{port}"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .map_err(|e| Failure::Broken(e.to_string()))?;
    match tokio_tungstenite::client_async_tls_with_config(
        request,
        stream,
        None,
        Some(Connector::Rustls(config)),
    )
    .await
    {
        Ok((channel, _)) => Ok(channel),
        Err(e) => Err(match verifier.why_refused() {
            Some(why) => Failure::Untrusted(why),
            None => Failure::Broken(e.to_string()),
        }),
    }
}

/// The next thing the server says, within that long; pings and pongs
/// count as saying something, so that a quiet server is not a gone one.
async fn next(channel: &mut Channel, within: Duration) -> Result<Option<FromServer>, String> {
    loop {
        let frame = tokio::time::timeout(within, channel.next())
            .await
            .map_err(|_| "le serveur ne dit plus rien".to_string())?;
        match frame {
            None | Some(Ok(Message::Close(_))) => return Ok(None),
            Some(Err(e)) => return Err(e.to_string()),
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(text.as_str())
                    .map(Some)
                    .map_err(|e| format!("le serveur a dit quelque chose d'illisible : {e}"));
            }
            Some(Ok(_)) => {}
        }
    }
}

async fn say(channel: &mut Channel, said: &FromDevice) -> Result<(), String> {
    let text = serde_json::to_string(said).map_err(|e| e.to_string())?;
    channel
        .send(Message::Text(text.into()))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_broker::rest::{ContactStatus, Permission, Registration};
    use zyr_transport::Identity;

    fn device(id: &str, online: bool) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: format!("PC {id}"),
            fingerprint: Identity::generate().unwrap().fingerprint(),
            online,
            access: if online { Access::Ready } else { Access::Off },
            last_seen: None,
        }
    }

    fn share(id: &str, device: DeviceInfo) -> ShareInfo {
        ShareInfo {
            id: id.into(),
            device,
            owner: "ami".into(),
            with: "victor".into(),
            permissions: Permission::ALL.to_vec(),
            expires: None,
            created: 1,
        }
    }

    #[test]
    fn the_snapshot_follows_what_the_server_says() {
        let mut snapshot = Snapshot::default();
        let server = ServerInfo {
            name: "Maison".into(),
            version: "0.1.0".into(),
            protocol: PROTOCOL,
            registration: Registration::Invitation,
            relay: false,
            udp_port: None,
            signing_key: zyr_broker::ServerKey::generate().public(),
        };
        snapshot.apply(&FromServer::Welcome {
            server: server.clone(),
            me: "d1".into(),
            devices: vec![device("d1", true), device("d2", false)],
            contacts: vec![],
            shares: vec![share("p1", device("d9", false))],
            token: None,
        });
        assert!(snapshot.connected);
        assert_eq!(snapshot.me.as_deref(), Some("d1"));
        assert_eq!(snapshot.server, Some(server));

        // La présence touche les appareils du compte et ceux des partages.
        snapshot.apply(&FromServer::Presence {
            device: "d2".into(),
            online: true,
            access: Access::Ready,
        });
        snapshot.apply(&FromServer::Presence {
            device: "d9".into(),
            online: true,
            access: Access::EngineMissing,
        });
        assert!(snapshot.devices[1].online);
        assert_eq!(snapshot.shares[0].device.access, Access::EngineMissing);

        // Ajouté, renommé, révoqué.
        snapshot.apply(&FromServer::DeviceAdded {
            device: device("d3", false),
        });
        assert_eq!(snapshot.devices.len(), 3);
        let mut renamed = device("d3", false);
        renamed.name = "Portable".into();
        snapshot.apply(&FromServer::DeviceRenamed {
            device: renamed.clone(),
        });
        assert_eq!(snapshot.devices[2], renamed);
        snapshot.apply(&FromServer::DeviceRevoked {
            device: "d3".into(),
        });
        assert_eq!(snapshot.devices.len(), 2);

        // Contacts et partages.
        let contact = ContactInfo {
            id: "c1".into(),
            username: "ami".into(),
            status: ContactStatus::Pending,
            asked_by_me: false,
            online: true,
        };
        snapshot.apply(&FromServer::ContactRequested {
            contact: contact.clone(),
        });
        snapshot.apply(&FromServer::ContactAnswered {
            contact: ContactInfo {
                status: ContactStatus::Accepted,
                ..contact
            },
        });
        assert_eq!(snapshot.contacts.len(), 1);
        assert_eq!(snapshot.contacts[0].status, ContactStatus::Accepted);
        snapshot.apply(&FromServer::ContactRemoved {
            contact: "c1".into(),
        });
        assert!(snapshot.contacts.is_empty());
        snapshot.apply(&FromServer::ShareRemoved { share: "p1".into() });
        assert!(snapshot.shares.is_empty());
        snapshot.apply(&FromServer::ShareGiven {
            share: share("p2", device("d8", true)),
        });
        assert_eq!(snapshot.shares.len(), 1);

        snapshot.disconnected("coupé".into());
        assert!(!snapshot.connected);
        assert_eq!(snapshot.trouble.as_deref(), Some("coupé"));
        // Les listes restent : c'est ce que l'accueil montre en gris.
        assert_eq!(snapshot.devices.len(), 2);
    }
}
