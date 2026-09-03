//! The live channel: presence, changes to the account, and the
//! rendezvous of a session.
//!
//! One WebSocket per attached device. The server speaks first with a
//! challenge, the device proves its key in its first word, and from then
//! on the channel carries what is alive. Who is online is known here and
//! nowhere else: it is not a fact worth a database.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use tokio::sync::mpsc;
use zyr_broker::live::{FromDevice, FromServer, Peer, Relay};
use zyr_broker::proof::{Purpose, challenge_message};
use zyr_broker::rest::{Access, DeviceInfo, ShareInfo};
use zyr_broker::ticket::{Grant, Pass, Ticket};
use zyr_broker::{Code, PROTOCOL, ServerKey, now};
use zyr_transport::{Fingerprint, signed_by};

use crate::api::{self, Refusal, blocking, contact_info, device_info, share_info};
use crate::relay::Offer;
use crate::store::{Bearer, Device, Share, Store};
use crate::{App, journal};

/// How long the device gets to answer the challenge.
const HELLO_PATIENCE: Duration = Duration::from_secs(10);

/// A channel that has said nothing for this long is gone: the device
/// pings every thirty seconds.
const SILENCE: Duration = Duration::from_secs(90);

/// One device on its channel.
struct Online {
    account: String,
    access: Access,
    tx: mpsc::UnboundedSender<FromServer>,
    /// Which opening of the channel this is: a device that opens a second
    /// channel replaces the first, and the first must not take the
    /// second down as it leaves.
    opening: u64,
}

struct Session {
    from: String,
    to: String,
    grant: Grant,
}

pub struct Live {
    store: Arc<Store>,
    key: Arc<ServerKey>,
    /// The relay this server offers, when it has one.
    relay: Option<Offer>,
    online: Mutex<HashMap<String, Online>>,
    sessions: Mutex<HashMap<String, Session>>,
    openings: std::sync::atomic::AtomicU64,
}

impl Live {
    pub fn new(store: Arc<Store>, key: Arc<ServerKey>, relay: Option<Offer>) -> Self {
        Self {
            store,
            key,
            relay,
            online: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            openings: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Whether this server has a relay to offer.
    pub fn has_a_relay(&self) -> bool {
        self.relay.is_some()
    }

    /// The pass one device needs to reach the other through the relay,
    /// with where to present it.
    ///
    /// One pass each, and each names only its bearer and the one other
    /// device its packets may reach: the relay carries between those two
    /// fingerprints and no others.
    fn relay_for(
        &self,
        session: &str,
        bearer: Fingerprint,
        peer: Fingerprint,
        at: u64,
    ) -> Option<Relay> {
        let offer = self.relay.as_ref()?;
        let pass = match self.key.seal(&Pass::new(session, bearer, peer, at)) {
            Ok(sealed) => sealed,
            Err(e) => {
                journal::say(format!("relay pass could not be sealed: {e}"));
                return None;
            }
        };
        Some(Relay {
            address: offer.address.clone(),
            fingerprint: offer.fingerprint,
            pass,
        })
    }

    /// Whether that device is connected, and what it accepts.
    pub fn status_of(&self, device: &str) -> (bool, Access) {
        match self.online.lock().expect("en ligne").get(device) {
            Some(online) => (true, online.access),
            None => (false, Access::Off),
        }
    }

    /// Whether any device of that account is connected.
    pub fn account_online(&self, account: &str) -> bool {
        self.online
            .lock()
            .expect("en ligne")
            .values()
            .any(|online| online.account == account)
    }

    /// Says that to every connected device of the account.
    pub fn notify_account(&self, account: &str, told: FromServer) {
        for online in self.online.lock().expect("en ligne").values() {
            if online.account == account {
                let _ = online.tx.send(told.clone());
            }
        }
    }

    /// The same, leaving one device out: the one the news is about, when
    /// it is its own.
    fn notify_account_except(&self, account: &str, except: &str, told: FromServer) {
        for (id, online) in self.online.lock().expect("en ligne").iter() {
            if online.account == account && id != except {
                let _ = online.tx.send(told.clone());
            }
        }
    }

    pub fn notify_device(&self, device: &str, told: FromServer) {
        if let Some(online) = self.online.lock().expect("en ligne").get(device) {
            let _ = online.tx.send(told);
        }
    }

    /// The accounts that may know of that device: its own, and those it
    /// is shared with.
    async fn interested_in(&self, device: &Device) -> Vec<String> {
        let id = device.id.clone();
        let at = now();
        let mut accounts = vec![device.account.clone()];
        if let Ok(shares) =
            blocking(&self.store, move |store| store.shares_of_device(&id, at)).await
        {
            accounts.extend(shares.into_iter().map(|share| share.grantee));
        }
        accounts.dedup();
        accounts
    }

    async fn broadcast_presence(&self, device: &Device, online: bool, access: Access) {
        let told = FromServer::Presence {
            device: device.id.clone(),
            online,
            access,
        };
        for account in self.interested_in(device).await {
            self.notify_account_except(&account, &device.id, told.clone());
        }
    }

    pub async fn renamed(&self, device: &Device, info: DeviceInfo) {
        let told = FromServer::DeviceRenamed { device: info };
        for account in self.interested_in(device).await {
            self.notify_account(&account, told.clone());
        }
    }

    /// The device is no longer of its account: its channel is closed with
    /// the reason, its sessions end, and whoever knew of it is told.
    pub async fn revoked(&self, device: &Device, shares: &[Share]) {
        let taken = self.online.lock().expect("en ligne").remove(&device.id);
        if let Some(online) = taken {
            let _ = online.tx.send(FromServer::Bye {
                code: Code::DeviceRevoked,
            });
        }
        self.end_sessions_of(&device.id).await;
        self.notify_account(
            &device.account,
            FromServer::DeviceRevoked {
                device: device.id.clone(),
            },
        );
        for share in shares {
            self.notify_account(
                &share.grantee,
                FromServer::ShareRemoved {
                    share: share.id.clone(),
                },
            );
        }
    }

    pub fn share_given(&self, share: &Share, info: ShareInfo) {
        let told = FromServer::ShareGiven { share: info };
        self.notify_account(&share.owner, told.clone());
        self.notify_account(&share.grantee, told);
    }

    /// The share is gone: both accounts are told, and a session running
    /// under it is ended.
    pub async fn share_removed(&self, share: &Share) {
        let told = FromServer::ShareRemoved {
            share: share.id.clone(),
        };
        self.notify_account(&share.owner, told.clone());
        self.notify_account(&share.grantee, told);
        let under_it: Vec<String> = self
            .sessions
            .lock()
            .expect("sessions")
            .iter()
            .filter(|(_, session)| matches!(&session.grant, Grant::Share { id } if *id == share.id))
            .map(|(id, _)| id.clone())
            .collect();
        for session in under_it {
            self.end_session(&session, None).await;
        }
    }

    fn register(&self, bearer: &Bearer, tx: mpsc::UnboundedSender<FromServer>) -> u64 {
        let opening = self
            .openings
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.online.lock().expect("en ligne").insert(
            bearer.device.id.clone(),
            Online {
                account: bearer.account.id.clone(),
                access: Access::Off,
                tx,
                opening,
            },
        );
        opening
    }

    /// Takes the device off the list, unless another opening of its
    /// channel has taken its place.
    fn unregister(&self, device: &str, opening: u64) -> bool {
        let mut online = self.online.lock().expect("en ligne");
        match online.get(device) {
            Some(current) if current.opening == opening => {
                online.remove(device);
                true
            }
            _ => false,
        }
    }

    async fn end_sessions_of(&self, device: &str) {
        let involved: Vec<String> = self
            .sessions
            .lock()
            .expect("sessions")
            .iter()
            .filter(|(_, session)| session.from == device || session.to == device)
            .map(|(id, _)| id.clone())
            .collect();
        for session in involved {
            self.end_session(&session, Some(device)).await;
        }
    }

    /// Ends a session and tells whoever has not asked for it.
    async fn end_session(&self, id: &str, asked_by: Option<&str>) {
        let Some(session) = self.sessions.lock().expect("sessions").remove(id) else {
            return;
        };
        let told = FromServer::SessionEnd {
            session: id.to_string(),
        };
        for device in [&session.from, &session.to] {
            if Some(device.as_str()) != asked_by {
                self.notify_device(device, told.clone());
            }
        }
        let ended = id.to_string();
        let at = now();
        let _ = blocking(&self.store, move |store| store.session_ended(&ended, at)).await;
    }

    async fn welcome(&self, app: &App, bearer: &Bearer) -> Result<FromServer, Refusal> {
        let live = app.live.clone();
        let me = bearer.device.id.clone();
        let account = bearer.account.id.clone();
        let renew = bearer.renew;
        let device = bearer.device.clone();
        let at = now();
        let (devices, contacts, shares, token) = blocking(&self.store, move |store| {
            let devices = store
                .devices_of(&account)?
                .iter()
                .map(|device| device_info(&live, device))
                .collect();
            let contacts = store
                .contacts_of(&account)?
                .iter()
                .map(|contact| contact_info(store, &live, &account, contact))
                .collect::<Result<Vec<_>, _>>()?;
            let shares = store
                .shares_of(&account, at)?
                .iter()
                .map(|share| share_info(store, &live, share))
                .collect::<Result<Vec<_>, _>>()?;
            let token = if renew {
                Some(store.renew_device_token(&device, at)?.raw)
            } else {
                None
            };
            Ok((devices, contacts, shares, token))
        })
        .await?;
        Ok(FromServer::Welcome {
            server: api::server_info_of(app),
            me,
            devices,
            contacts,
            shares,
            token,
        })
    }

    /// Opens a session from the device on this channel towards that one.
    async fn open(&self, bearer: &Bearer, to: String) {
        let refused = |code: Code| FromServer::SessionRefused {
            to: to.clone(),
            code,
        };
        let device = bearer.device.clone();
        let target = to.clone();
        let at = now();
        let (target, grant) = match blocking(&self.store, move |store| {
            let (target, grant) = store.right_to(&device, &target, at)?;
            let owner = store
                .account_by_id(&target.account)?
                .map(|account| account.username)
                .unwrap_or_default();
            Ok((target, grant, owner))
        })
        .await
        {
            Ok((target, grant, owner)) => ((target, owner), grant),
            Err(Refusal(code)) => {
                self.notify_device(&bearer.device.id, refused(code));
                return;
            }
        };
        let (target, owner) = target;
        match self.status_of(&target.id) {
            (false, _) => {
                self.notify_device(&bearer.device.id, refused(Code::PeerOffline));
                return;
            }
            (true, Access::Ready) => {}
            (true, _) => {
                self.notify_device(&bearer.device.id, refused(Code::PeerNotHosting));
                return;
            }
        }
        let session = zyr_proto::random::alphanumeric_string(16);
        let ticket = Ticket::new(
            session.clone(),
            bearer.device.fingerprint,
            target.fingerprint,
            grant.clone(),
            at,
        );
        let sealed = match self.key.seal(&ticket) {
            Ok(sealed) => sealed,
            Err(e) => {
                journal::say(format!("ticket could not be sealed: {e}"));
                self.notify_device(&bearer.device.id, refused(Code::Internal));
                return;
            }
        };
        self.sessions.lock().expect("sessions").insert(
            session.clone(),
            Session {
                from: bearer.device.id.clone(),
                to: target.id.clone(),
                grant: grant.clone(),
            },
        );
        {
            let (session, from, to, grant) = (
                session.clone(),
                bearer.device.id.clone(),
                target.id.clone(),
                grant,
            );
            let _ = blocking(&self.store, move |store| {
                store.session_started(&session, &from, &to, &grant, at)
            })
            .await;
        }
        journal::say(format!(
            "session {session}: {} ({}) towards {} ({})",
            bearer.device.name, bearer.account.username, target.name, owner
        ));
        self.notify_device(
            &bearer.device.id,
            FromServer::SessionStart {
                session: session.clone(),
                ticket: sealed.clone(),
                peer: Peer {
                    device: target.id.clone(),
                    fingerprint: target.fingerprint,
                    name: target.name.clone(),
                    account: owner,
                },
                relay: self.relay_for(&session, bearer.device.fingerprint, target.fingerprint, at),
            },
        );
        self.notify_device(
            &target.id,
            FromServer::SessionStart {
                relay: self.relay_for(&session, target.fingerprint, bearer.device.fingerprint, at),
                session,
                ticket: sealed,
                peer: Peer {
                    device: bearer.device.id.clone(),
                    fingerprint: bearer.device.fingerprint,
                    name: bearer.device.name.clone(),
                    account: bearer.account.username.clone(),
                },
            },
        );
    }

    /// Passes candidates to the other device of the session, if the
    /// sender is one of its two.
    fn forward_candidates(&self, from: &str, session: &str, candidates: Vec<std::net::SocketAddr>) {
        let other = {
            let sessions = self.sessions.lock().expect("sessions");
            match sessions.get(session) {
                Some(s) if s.from == from => Some(s.to.clone()),
                Some(s) if s.to == from => Some(s.from.clone()),
                _ => None,
            }
        };
        if let Some(other) = other {
            self.notify_device(
                &other,
                FromServer::SessionCandidates {
                    session: session.to_string(),
                    candidates,
                },
            );
        }
    }

    async fn heard(&self, bearer: &Bearer, said: FromDevice) {
        match said {
            FromDevice::Hello { .. } => {}
            FromDevice::State { access } => {
                let changed = {
                    let mut online = self.online.lock().expect("en ligne");
                    match online.get_mut(&bearer.device.id) {
                        Some(me) if me.access != access => {
                            me.access = access;
                            true
                        }
                        _ => false,
                    }
                };
                if changed {
                    self.broadcast_presence(&bearer.device, true, access).await;
                }
            }
            FromDevice::SessionOpen { to } => self.open(bearer, to).await,
            FromDevice::SessionCandidates {
                session,
                candidates,
            } => self.forward_candidates(&bearer.device.id, &session, candidates),
            FromDevice::SessionEnd { session } => {
                let mine = self
                    .sessions
                    .lock()
                    .expect("sessions")
                    .get(&session)
                    .is_some_and(|s| s.from == bearer.device.id || s.to == bearer.device.id);
                if mine {
                    self.end_session(&session, Some(&bearer.device.id)).await;
                }
            }
        }
    }

    /// Serves one channel from its first word to its last.
    async fn attend(self: Arc<Self>, app: App, mut socket: WebSocket, bearer: Bearer) {
        let nonce = zyr_proto::random::alphanumeric_string(32);
        if send(
            &mut socket,
            &FromServer::Challenge {
                nonce: nonce.clone(),
            },
        )
        .await
        .is_err()
        {
            return;
        }
        let hello = tokio::time::timeout(HELLO_PATIENCE, socket.recv()).await;
        let Ok(Some(Ok(Message::Text(text)))) = hello else {
            return;
        };
        let Ok(FromDevice::Hello {
            protocol,
            build,
            signature,
        }) = serde_json::from_str::<FromDevice>(text.as_str())
        else {
            return;
        };
        if protocol != PROTOCOL {
            journal::say(format!(
                "device {} speaks protocol {protocol}, this server {PROTOCOL}",
                bearer.device.fingerprint
            ));
            let _ = send(
                &mut socket,
                &FromServer::Bye {
                    code: Code::UpgradeNeeded,
                },
            )
            .await;
            return;
        }
        let expected = challenge_message(&self.key.public(), &nonce, Purpose::Live);
        let proven = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            signature.as_bytes(),
        )
        .is_ok_and(|signature| {
            signed_by(
                &rustls::pki_types::CertificateDer::from(bearer.device.certificate.as_slice()),
                &expected,
                &signature,
            )
        });
        if !proven {
            journal::say(format!(
                "device {} failed its proof",
                bearer.device.fingerprint
            ));
            let _ = send(
                &mut socket,
                &FromServer::Bye {
                    code: Code::ProofInvalid,
                },
            )
            .await;
            return;
        }

        let welcome = match self.welcome(&app, &bearer).await {
            Ok(welcome) => welcome,
            Err(Refusal(code)) => {
                let _ = send(&mut socket, &FromServer::Bye { code }).await;
                return;
            }
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let opening = self.register(&bearer, tx);
        journal::say(format!(
            "device {} ({}) of {} is online, build {build}",
            bearer.device.name, bearer.device.fingerprint, bearer.account.username
        ));
        if send(&mut socket, &welcome).await.is_err() {
            self.unregister(&bearer.device.id, opening);
            return;
        }
        self.broadcast_presence(&bearer.device, true, Access::Off)
            .await;

        loop {
            tokio::select! {
                told = rx.recv() => match told {
                    Some(told) => {
                        let farewell = matches!(told, FromServer::Bye { .. });
                        if send(&mut socket, &told).await.is_err() || farewell {
                            break;
                        }
                    }
                    // Replaced by another opening of the same channel, or
                    // revoked: nothing more to say here.
                    None => break,
                },
                heard = tokio::time::timeout(SILENCE, socket.recv()) => match heard {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        match serde_json::from_str::<FromDevice>(text.as_str()) {
                            Ok(said) => self.heard(&bearer, said).await,
                            Err(e) => journal::say(format!(
                                "device {} said something unreadable: {e}",
                                bearer.device.fingerprint
                            )),
                        }
                    }
                    Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => break,
                    Ok(Some(Ok(_))) => {}
                },
            }
        }

        if self.unregister(&bearer.device.id, opening) {
            journal::say(format!(
                "device {} ({}) is offline",
                bearer.device.name, bearer.device.fingerprint
            ));
            self.end_sessions_of(&bearer.device.id).await;
            let id = bearer.device.id.clone();
            let at = now();
            let _ = blocking(&self.store, move |store| store.touch_device(&id, at)).await;
            self.broadcast_presence(&bearer.device, false, Access::Off)
                .await;
        }
    }
}

async fn send(socket: &mut WebSocket, told: &FromServer) -> Result<(), axum::Error> {
    let text = serde_json::to_string(told).expect("un message sérialisable");
    socket.send(Message::Text(text.into())).await
}

/// The handler: a device token, then the WebSocket.
pub async fn upgrade(
    ws: WebSocketUpgrade,
    State(app): State<App>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let raw = api::bearer_of(&headers).ok_or(Code::Unauthorized)?;
    let at = now();
    let bearer = blocking(&app.store, move |store| store.bearer_of_token(&raw, at)).await?;
    let live = app.live.clone();
    Ok(ws.on_upgrade(move |socket| live.attend(app, socket, bearer)))
}
