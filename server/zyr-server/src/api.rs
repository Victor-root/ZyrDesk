//! What is asked and answered over HTTPS.
//!
//! One handler per path of `zyr_broker::rest::paths`. A refusal is a
//! status and a body with a code that does not change; the English
//! sentence beside it is a courtesy for whoever reads it at `curl`. The
//! store is only ever reached from a blocking thread, so a password
//! being hashed never holds the runtime.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequest, FromRequestParts, Path, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rustls::pki_types::CertificateDer;
use serde::de::DeserializeOwned;
use zyr_broker::proof::{Purpose, challenge_message};
use zyr_broker::rest::{
    Challenge, ContactInfo, ContactRequest, ContactStatus, DeviceInfo, Link, LinkAnswer, Login,
    LoginAnswer, Register, Rename, ServerInfo, ShareInfo, ShareRequest, paths,
};
use zyr_broker::{Code, PROTOCOL, now};
use zyr_transport::signed_by;

use crate::live::{self, Live};
use crate::store::{Account, Contact, Device, Fault, Share, Store};
use crate::{App, journal};

/// How long a challenge for attaching a device may be answered.
const CHALLENGE_LIFE: u64 = 60;

pub fn router(app: App) -> Router {
    Router::new()
        .route(paths::SERVER, get(server_info))
        .route(paths::ACCOUNTS, post(register))
        .route(paths::LOGIN, post(login))
        .route(paths::CHALLENGE, post(challenge))
        .route(paths::DEVICES, get(list_devices).post(link_device))
        .route(
            "/v1/devices/{id}",
            patch(rename_device).delete(revoke_device),
        )
        .route(paths::CONTACTS, get(list_contacts).post(ask_contact))
        .route("/v1/contacts/{id}/accept", post(accept_contact))
        .route("/v1/contacts/{id}/decline", post(decline_contact))
        .route("/v1/contacts/{id}", delete(remove_contact))
        .route(paths::SHARES, get(list_shares).post(give_share))
        .route("/v1/shares/{id}", delete(remove_share))
        .route(paths::LIVE, get(live::upgrade))
        .with_state(app)
}

/// Why a request was not served, as it goes back on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal(pub Code);

impl From<Code> for Refusal {
    fn from(code: Code) -> Self {
        Refusal(code)
    }
}

impl From<Fault> for Refusal {
    fn from(fault: Fault) -> Self {
        match fault {
            Fault::Refused(code) => Refusal(code),
            Fault::Broken(e) => {
                journal::say(format!("store failure: {e}"));
                Refusal(Code::Internal)
            }
        }
    }
}

fn status_of(code: Code) -> StatusCode {
    match code {
        Code::InvalidCredentials
        | Code::Unauthorized
        | Code::DeviceRevoked
        | Code::ProofInvalid => StatusCode::UNAUTHORIZED,
        Code::RegistrationClosed | Code::NoRight => StatusCode::FORBIDDEN,
        Code::NotFound | Code::DeviceUnknown => StatusCode::NOT_FOUND,
        Code::UsernameTaken | Code::ContactExists | Code::PeerOffline | Code::PeerNotHosting => {
            StatusCode::CONFLICT
        }
        Code::WeakPassword
        | Code::InvalidUsername
        | Code::InvitationInvalid
        | Code::ShareInvalid
        | Code::ContactSelf
        | Code::NotAContact
        | Code::ChallengeExpired
        | Code::BadRequest => StatusCode::BAD_REQUEST,
        Code::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        Code::UpgradeNeeded => StatusCode::UPGRADE_REQUIRED,
        Code::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// The courtesy sentence beside a code.
fn message(code: Code) -> &'static str {
    match code {
        Code::InvalidCredentials => "wrong username or password",
        Code::RegistrationClosed => "this server does not take new accounts",
        Code::InvitationInvalid => "an invitation code is required, and this one is not valid",
        Code::UsernameTaken => "this username is taken",
        Code::WeakPassword => "the password must be at least twelve characters",
        Code::InvalidUsername => {
            "a username is 3 to 32 letters, digits, dots, dashes or underscores"
        }
        Code::Unauthorized => "no valid token",
        Code::DeviceRevoked => "this device was revoked",
        Code::DeviceUnknown => "no such device on this account",
        Code::ProofInvalid => "the signature does not match the certificate",
        Code::ChallengeExpired => "unknown or expired challenge",
        Code::NotFound => "not found",
        Code::ContactExists => "a request already stands between these accounts",
        Code::NotAContact => "not a contact",
        Code::ContactSelf => "one cannot be one's own contact",
        Code::ShareInvalid => "the share names a device or a contact that does not fit",
        Code::PeerOffline => "that device is not connected",
        Code::PeerNotHosting => "that device does not accept remote access right now",
        Code::NoRight => "no right on that device",
        Code::UpgradeNeeded => "the server and the device speak different versions",
        Code::RateLimited => "too many attempts, wait",
        Code::BadRequest => "the request could not be read",
        Code::Internal => "the server failed, its journal says why",
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (
            status_of(self.0),
            Json(zyr_broker::rest::Error {
                error: self.0,
                message: message(self.0).to_string(),
            }),
        )
            .into_response()
    }
}

/// A JSON body, or a refusal that says so.
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = Refusal;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(body)) => Ok(Body(body)),
            Err(_) => Err(Refusal(Code::BadRequest)),
        }
    }
}

/// The bearer token in the `Authorization` header, if any.
pub fn bearer_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_string())
}

/// Whoever presents a token: an account, through one of its devices or
/// not.
///
/// A device attached to the account is the account: it may do every
/// gesture of it, because it may already open a session to every one
/// of its machines. The token of the account itself, short-lived, is
/// what attaches a device in the first place.
pub struct Actor {
    pub account: Account,
    pub device: Option<Device>,
}

impl FromRequestParts<App> for Actor {
    type Rejection = Refusal;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Self::Rejection> {
        let raw = bearer_of(&parts.headers).ok_or(Code::Unauthorized)?;
        let at = now();
        let actor = blocking(&app.store, move |store| {
            match store.bearer_of_token(&raw, at) {
                Ok(bearer) => Ok(Actor {
                    account: bearer.account,
                    device: Some(bearer.device),
                }),
                Err(Fault::Refused(Code::DeviceRevoked)) => Err(Code::DeviceRevoked.into()),
                Err(_) => Ok(Actor {
                    account: store.account_of_token(&raw, at)?,
                    device: None,
                }),
            }
        })
        .await?;
        Ok(actor)
    }
}

/// An account token alone: what attaches a new device.
pub struct AccountOnly(pub Account);

impl FromRequestParts<App> for AccountOnly {
    type Rejection = Refusal;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Self::Rejection> {
        let raw = bearer_of(&parts.headers).ok_or(Code::Unauthorized)?;
        let at = now();
        let account = blocking(&app.store, move |store| store.account_of_token(&raw, at)).await?;
        Ok(AccountOnly(account))
    }
}

/// Runs that on the blocking pool, where the store belongs.
pub async fn blocking<T: Send + 'static>(
    store: &Arc<Store>,
    work: impl FnOnce(&Store) -> Result<T, Fault> + Send + 'static,
) -> Result<T, Refusal> {
    let store = store.clone();
    match tokio::task::spawn_blocking(move || work(&store)).await {
        Ok(result) => result.map_err(Refusal::from),
        Err(e) => Err(Fault::Broken(e.to_string()).into()),
    }
}

/// Who is asking, for the limiter: the address of the connection, or the
/// one a reverse proxy on this machine says it forwarded.
fn client_ip(addr: SocketAddr, headers: &HeaderMap) -> IpAddr {
    if addr.ip().is_loopback()
        && let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|first| first.trim().parse::<IpAddr>().ok())
    {
        return forwarded;
    }
    addr.ip()
}

fn allowed(app: &App, addr: SocketAddr, headers: &HeaderMap) -> Result<(), Refusal> {
    if app.limiter.allows(client_ip(addr, headers)) {
        Ok(())
    } else {
        Err(Refusal(Code::RateLimited))
    }
}

pub fn server_info_of(app: &App) -> ServerInfo {
    ServerInfo {
        name: app.config.name.clone(),
        version: zyr_proto::PRODUCT_VERSION.to_string(),
        protocol: PROTOCOL,
        registration: app.config.registration.policy,
        // The relay comes with the next milestone; until then no server
        // has one to offer, whatever its configuration says.
        relay: false,
        signing_key: app.key.public(),
    }
}

/// A device as told to whoever may know of it.
pub fn device_info(live: &Live, device: &Device) -> DeviceInfo {
    let (online, access) = live.status_of(&device.id);
    DeviceInfo {
        id: device.id.clone(),
        name: device.name.clone(),
        fingerprint: device.fingerprint,
        online,
        access,
        last_seen: if online { None } else { device.last_seen },
    }
}

pub fn contact_info(
    store: &Store,
    live: &Live,
    me: &str,
    contact: &Contact,
) -> Result<ContactInfo, Fault> {
    let other = contact.other_than(me);
    let username = store
        .account_by_id(other)?
        .map(|account| account.username)
        .unwrap_or_default();
    Ok(ContactInfo {
        id: contact.id.clone(),
        username,
        status: if contact.accepted {
            ContactStatus::Accepted
        } else {
            ContactStatus::Pending
        },
        asked_by_me: contact.asker == me,
        online: live.account_online(other),
    })
}

pub fn share_info(store: &Store, live: &Live, share: &Share) -> Result<ShareInfo, Fault> {
    let device = store.device(&share.device)?.ok_or(Code::NotFound)?;
    let username = |id: &str| -> Result<String, Fault> {
        Ok(store
            .account_by_id(id)?
            .map(|account| account.username)
            .unwrap_or_default())
    };
    Ok(ShareInfo {
        id: share.id.clone(),
        device: device_info(live, &device),
        owner: username(&share.owner)?,
        with: username(&share.grantee)?,
        permissions: share.permissions.clone(),
        expires: share.expires,
        created: share.created,
    })
}

async fn server_info(State(app): State<App>) -> Json<ServerInfo> {
    Json(server_info_of(&app))
}

async fn register(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Body(register): Body<Register>,
) -> Result<Json<LoginAnswer>, Refusal> {
    allowed(&app, addr, &headers)?;
    let policy = app.config.registration.policy;
    let at = now();
    let answer = blocking(&app.store, move |store| {
        let account = store.create_account(
            &register.username,
            &register.password,
            register.email.as_deref(),
            register.invitation.as_deref(),
            policy,
            at,
        )?;
        let token = store.issue_account_token(&account, at)?;
        Ok(LoginAnswer {
            username: account.username,
            token: token.raw,
            expires: token.expires,
        })
    })
    .await?;
    journal::say(format!("account created: {}", answer.username));
    Ok(Json(answer))
}

async fn login(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Body(login): Body<Login>,
) -> Result<Json<LoginAnswer>, Refusal> {
    allowed(&app, addr, &headers)?;
    let at = now();
    let answer = blocking(&app.store, move |store| {
        let (account, token) = store.login(&login.username, &login.password, at)?;
        Ok(LoginAnswer {
            username: account.username,
            token: token.raw,
            expires: token.expires,
        })
    })
    .await?;
    Ok(Json(answer))
}

async fn challenge(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Challenge>, Refusal> {
    allowed(&app, addr, &headers)?;
    let at = now();
    let nonce = zyr_proto::random::alphanumeric_string(32);
    let expires = at + CHALLENGE_LIFE;
    let mut challenges = app.challenges.lock().expect("défis");
    challenges.retain(|_, until| *until > at);
    challenges.insert(nonce.clone(), expires);
    Ok(Json(Challenge { nonce, expires }))
}

/// Takes the challenge back, once, if it is still good.
fn take_challenge(app: &App, nonce: &str) -> Result<(), Refusal> {
    let mut challenges = app.challenges.lock().expect("défis");
    match challenges.remove(nonce) {
        Some(expires) if expires > now() => Ok(()),
        _ => Err(Refusal(Code::ChallengeExpired)),
    }
}

async fn link_device(
    State(app): State<App>,
    AccountOnly(account): AccountOnly,
    Body(link): Body<Link>,
) -> Result<Json<LinkAnswer>, Refusal> {
    take_challenge(&app, &link.nonce)?;
    let certificate = BASE64
        .decode(&link.certificate)
        .map_err(|_| Code::BadRequest)?;
    let signature = BASE64
        .decode(&link.signature)
        .map_err(|_| Code::BadRequest)?;
    let expected = challenge_message(&app.key.public(), &link.nonce, Purpose::Link);
    if !signed_by(
        &CertificateDer::from(certificate.as_slice()),
        &expected,
        &signature,
    ) {
        return Err(Refusal(Code::ProofInvalid));
    }
    let name = match link.name.trim() {
        "" => "Appareil".to_string(),
        named => named.to_string(),
    };
    let at = now();
    let account_id = account.id.clone();
    let (device, token) = blocking(&app.store, move |store| {
        store.link_device(&account_id, &certificate, &name, at)
    })
    .await?;
    journal::say(format!(
        "device attached: {} ({}) on account {}",
        device.name, device.fingerprint, account.username
    ));
    let info = device_info(&app.live, &device);
    app.live.notify_account(
        &account.id,
        zyr_broker::live::FromServer::DeviceAdded {
            device: info.clone(),
        },
    );
    Ok(Json(LinkAnswer {
        device: info,
        token: token.raw,
    }))
}

async fn list_devices(
    State(app): State<App>,
    actor: Actor,
) -> Result<Json<Vec<DeviceInfo>>, Refusal> {
    let live = app.live.clone();
    let account = actor.account.id;
    let devices = blocking(&app.store, move |store| {
        Ok(store
            .devices_of(&account)?
            .iter()
            .map(|device| device_info(&live, device))
            .collect())
    })
    .await?;
    Ok(Json(devices))
}

async fn rename_device(
    State(app): State<App>,
    actor: Actor,
    Path(id): Path<String>,
    Body(rename): Body<Rename>,
) -> Result<Json<DeviceInfo>, Refusal> {
    let name = rename.name.trim().to_string();
    if name.is_empty() {
        return Err(Refusal(Code::BadRequest));
    }
    let account = actor.account.id.clone();
    let device = blocking(&app.store, move |store| {
        store.rename_device(&account, &id, &name)
    })
    .await?;
    let info = device_info(&app.live, &device);
    app.live.renamed(&device, info.clone()).await;
    Ok(Json(info))
}

async fn revoke_device(
    State(app): State<App>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<DeviceInfo>, Refusal> {
    let at = now();
    let account = actor.account.id.clone();
    let (device, shares) = blocking(&app.store, move |store| {
        let shares = store.shares_of_device(&id, at)?;
        let device = store.revoke_device(&account, &id, at)?;
        Ok((device, shares))
    })
    .await?;
    journal::say(format!(
        "device revoked: {} ({}) from account {}",
        device.name, device.fingerprint, actor.account.username
    ));
    app.live.revoked(&device, &shares).await;
    Ok(Json(device_info(&app.live, &device)))
}

async fn list_contacts(
    State(app): State<App>,
    actor: Actor,
) -> Result<Json<Vec<ContactInfo>>, Refusal> {
    let live = app.live.clone();
    let me = actor.account.id;
    let contacts = blocking(&app.store, move |store| {
        store
            .contacts_of(&me)?
            .iter()
            .map(|contact| contact_info(store, &live, &me, contact))
            .collect()
    })
    .await?;
    Ok(Json(contacts))
}

async fn ask_contact(
    State(app): State<App>,
    actor: Actor,
    Body(request): Body<ContactRequest>,
) -> Result<Json<ContactInfo>, Refusal> {
    let live = app.live.clone();
    let me = actor.account.id.clone();
    let at = now();
    let (contact, mine, theirs) = blocking(&app.store, move |store| {
        let contact = store.ask_contact(&me, &request.username, at)?;
        let mine = contact_info(store, &live, &me, &contact)?;
        let theirs = contact_info(store, &live, &contact.asked, &contact)?;
        Ok((contact, mine, theirs))
    })
    .await?;
    app.live.notify_account(
        &contact.asked,
        zyr_broker::live::FromServer::ContactRequested { contact: theirs },
    );
    Ok(Json(mine))
}

async fn answer_contact(
    app: App,
    actor: Actor,
    id: String,
    accept: bool,
) -> Result<Json<ContactInfo>, Refusal> {
    let live = app.live.clone();
    let me = actor.account.id.clone();
    let at = now();
    let (contact, mine, theirs) = blocking(&app.store, move |store| {
        let contact = store.answer_contact(&me, &id, accept, at)?;
        let mine = contact_info(store, &live, &me, &contact)?;
        let theirs = contact_info(store, &live, &contact.asker, &contact)?;
        Ok((contact, mine, theirs))
    })
    .await?;
    let told = if accept {
        zyr_broker::live::FromServer::ContactAnswered { contact: theirs }
    } else {
        zyr_broker::live::FromServer::ContactRemoved {
            contact: contact.id.clone(),
        }
    };
    app.live.notify_account(&contact.asker, told);
    Ok(Json(mine))
}

async fn accept_contact(
    State(app): State<App>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<ContactInfo>, Refusal> {
    answer_contact(app, actor, id, true).await
}

async fn decline_contact(
    State(app): State<App>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<ContactInfo>, Refusal> {
    answer_contact(app, actor, id, false).await
}

async fn remove_contact(
    State(app): State<App>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, Refusal> {
    let me = actor.account.id.clone();
    let at = now();
    let (contact, shares) = blocking(&app.store, move |store| {
        let contact = store.remove_contact(&me, &id, at)?;
        // The shares between the two fell with the contact; whoever held
        // a session under one of them is told.
        let mut shares = store.shares_of(&contact.asker, at)?;
        shares.retain(|share| share.owner == contact.asked || share.grantee == contact.asked);
        Ok((contact, shares))
    })
    .await?;
    for who in [&contact.asker, &contact.asked] {
        app.live.notify_account(
            who,
            zyr_broker::live::FromServer::ContactRemoved {
                contact: contact.id.clone(),
            },
        );
    }
    for share in shares {
        app.live.share_removed(&share).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_shares(
    State(app): State<App>,
    actor: Actor,
) -> Result<Json<Vec<ShareInfo>>, Refusal> {
    let live = app.live.clone();
    let me = actor.account.id;
    let at = now();
    let shares = blocking(&app.store, move |store| {
        store
            .shares_of(&me, at)?
            .iter()
            .map(|share| share_info(store, &live, share))
            .collect()
    })
    .await?;
    Ok(Json(shares))
}

async fn give_share(
    State(app): State<App>,
    actor: Actor,
    Body(request): Body<ShareRequest>,
) -> Result<Json<ShareInfo>, Refusal> {
    if request.permissions.is_empty() {
        return Err(Refusal(Code::ShareInvalid));
    }
    let live = app.live.clone();
    let me = actor.account.id.clone();
    let at = now();
    let (share, info) = blocking(&app.store, move |store| {
        let share = store.give_share(
            &me,
            &request.device,
            &request.with,
            &request.permissions,
            request.expires,
            at,
        )?;
        let info = share_info(store, &live, &share)?;
        Ok((share, info))
    })
    .await?;
    journal::say(format!(
        "share given: device {} by {} with {}",
        info.device.name, info.owner, info.with
    ));
    app.live.share_given(&share, info.clone());
    Ok(Json(info))
}

async fn remove_share(
    State(app): State<App>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<StatusCode, Refusal> {
    let me = actor.account.id.clone();
    let at = now();
    let share = blocking(&app.store, move |store| store.remove_share(&me, &id, at)).await?;
    app.live.share_removed(&share).await;
    Ok(StatusCode::NO_CONTENT)
}
