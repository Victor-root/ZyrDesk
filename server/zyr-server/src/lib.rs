//! The ZyrDesk server: accounts, presence, rendezvous, and a relay in
//! reserve.
//!
//! One binary, two roles. The broker keeps accounts, devices, contacts
//! and shares, says who is online, and presents two devices to each
//! other with a signed ticket; the relay carries packets it cannot read
//! between two devices no direct road joins. Nothing of a session passes
//! through here in ordinary use, and no key of a session is ever known
//! here. Conception: `docs/SERVER.md`.

pub mod api;
pub mod check;
pub mod config;
pub mod journal;
pub mod keys;
pub mod limits;
pub mod live;
pub mod mirror;
pub mod relay;
pub mod store;

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use tokio::task::JoinHandle;
use zyr_broker::ServerKey;
use zyr_transport::Fingerprint;

use crate::config::Config;
use crate::keys::Tls;
use crate::limits::Limiter;
use crate::store::Store;

/// What every handler reaches.
pub struct State {
    pub config: Config,
    pub store: Arc<Store>,
    pub key: Arc<ServerKey>,
    /// The public key of the API's own certificate, when it serves TLS
    /// itself: what a device pins when nobody else vouches for it.
    pub tls_fingerprint: Option<Fingerprint>,
    /// The UDP port the mirror answers on, when it could be opened.
    pub udp_port: Option<u16>,
    pub limiter: Limiter,
    pub live: Arc<live::Live>,
    /// Challenges handed out for attaching a device, each with the
    /// moment it expires.
    pub challenges: Mutex<HashMap<String, u64>>,
}

pub type App = Arc<State>;

#[derive(Debug)]
pub enum StartError {
    Store(store::Fault),
    Keys(keys::KeyError),
    Bind(SocketAddr, std::io::Error),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Store(e) => write!(f, "{e}"),
            StartError::Keys(e) => write!(f, "{e}"),
            StartError::Bind(listen, e) => write!(f, "écoute impossible sur {listen} : {e}"),
        }
    }
}

impl std::error::Error for StartError {}

/// The server, up and listening.
pub struct Running {
    /// Where the API answers, which is what the configuration said or,
    /// for a port left at nought, what the system gave.
    pub address: SocketAddr,
    pub app: App,
    handle: Handle<SocketAddr>,
    serving: JoinHandle<std::io::Result<()>>,
    udp: Option<Udp>,
}

impl Running {
    /// How many sessions the relay is carrying right now, and nought
    /// without a relay.
    ///
    /// For whoever tests a device against a whole server: a branch that
    /// outlives its session is only visible from here.
    pub fn sessions_relayed(&self) -> usize {
        match &self.udp {
            Some(Udp::Relay(relay)) => relay.sessions(),
            _ => 0,
        }
    }
}

/// What answers on the server's UDP port: the mirror alone, or the
/// relay, which answers the mirror on the same port.
enum Udp {
    Mirror(mirror::Mirror),
    Relay(relay::Relay),
}

impl Udp {
    fn address(&self) -> SocketAddr {
        match self {
            Udp::Mirror(mirror) => mirror.address(),
            Udp::Relay(relay) => relay.address(),
        }
    }

    fn offer(&self) -> Option<relay::Offer> {
        match self {
            Udp::Mirror(_) => None,
            Udp::Relay(relay) => Some(relay.offer()),
        }
    }

    fn stop(&self) {
        match self {
            Udp::Mirror(mirror) => mirror.stop(),
            Udp::Relay(relay) => relay.stop(),
        }
    }

    fn said(&self) -> String {
        match self {
            Udp::Mirror(mirror) => format!(", mirror on UDP {}", mirror.address()),
            Udp::Relay(relay) => format!(", mirror and relay on UDP {}", relay.address()),
        }
    }
}

/// How long the connections in progress get to finish at a stop.
const GRACE: Duration = Duration::from_secs(5);

/// Opens the store and the keys, and starts listening.
pub async fn start(config: Config) -> Result<Running, StartError> {
    let store = Arc::new(Store::open(&config.database()).map_err(StartError::Store)?);
    let key =
        Arc::new(keys::load_or_create_signing_key(&config.keys_dir()).map_err(StartError::Keys)?);
    let tls = config
        .api
        .tls()
        .map(|(certificate, key)| Tls::load(certificate, key))
        .transpose()
        .map_err(StartError::Keys)?;
    let listen = config.api.listen;
    // A UDP port that cannot be opened is not the end of the server: the
    // devices are told there is neither mirror nor relay, and reach each
    // other without.
    let udp = open_the_udp_port(&config, &key, &store);
    let app: App = Arc::new(State {
        limiter: Limiter::new(config.limits.login_attempts_per_minute),
        live: Arc::new(live::Live::new(
            store.clone(),
            key.clone(),
            udp.as_ref().and_then(Udp::offer),
        )),
        tls_fingerprint: tls.as_ref().and_then(Tls::fingerprint),
        udp_port: udp.as_ref().map(|udp| udp.address().port()),
        challenges: Mutex::new(HashMap::new()),
        config,
        store,
        key,
    });

    let router = api::router(app.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let handle: Handle<SocketAddr> = Handle::new();
    let serving = match tls {
        Some(tls) => {
            let config = RustlsConfig::from_config(tls.server_config().map_err(StartError::Keys)?);
            tokio::spawn(
                axum_server::bind_rustls(listen, config)
                    .handle(handle.clone())
                    .serve(router),
            )
        }
        None => tokio::spawn(
            axum_server::bind(listen)
                .handle(handle.clone())
                .serve(router),
        ),
    };
    let Some(address) = handle.listening().await else {
        let failure = match serving.await {
            Ok(Err(e)) => e,
            Ok(Ok(())) => std::io::Error::other("le serveur s'est arrêté avant d'écouter"),
            Err(e) => std::io::Error::other(e.to_string()),
        };
        return Err(StartError::Bind(listen, failure));
    };
    journal::say(format!(
        "listening on {address}{}{}",
        if app.tls_fingerprint.is_some() {
            ", TLS"
        } else {
            ", in the clear behind a reverse proxy"
        },
        udp.as_ref().map(Udp::said).unwrap_or_default()
    ));
    Ok(Running {
        address,
        app,
        handle,
        serving,
        udp,
    })
}

/// Opens the UDP port: the relay when the configuration wants one, the
/// mirror alone otherwise.
///
/// A relay whose certificate cannot be made falls back to the mirror
/// rather than taking the server down with it: the mirror is what makes
/// a direct road possible at all, and it is worth keeping whatever else
/// went wrong.
fn open_the_udp_port(config: &Config, key: &Arc<ServerKey>, store: &Arc<Store>) -> Option<Udp> {
    let listen = config.relay.listen;
    if !config.relay.enabled {
        return match mirror::Mirror::open(listen) {
            Ok(mirror) => Some(Udp::Mirror(mirror)),
            Err(e) => {
                journal::say(format!("no mirror: UDP {listen} could not be opened: {e}"));
                None
            }
        };
    }
    let doorway = match zyr_transport::Doorway::bind(listen) {
        Ok(doorway) => doorway,
        Err(e) => {
            journal::say(format!(
                "no mirror and no relay: UDP {listen} could not be opened: {e}"
            ));
            return None;
        }
    };
    let address = relay::address_of(&config.public_host(), doorway.local_address().ok()?.port());
    match relay::Relay::open(
        &doorway,
        &config.keys_dir(),
        address,
        &config.relay,
        key.public(),
        store.clone(),
    ) {
        Ok(relay) => Some(Udp::Relay(relay)),
        Err(e) => {
            journal::say(format!("no relay: {e}"));
            drop(doorway);
            mirror::Mirror::open(listen).ok().map(Udp::Mirror)
        }
    }
}

impl Running {
    /// Lets the connections in progress finish, then returns.
    pub async fn stop(self) {
        self.handle.graceful_shutdown(Some(GRACE));
        let _ = self.serving.await;
        if let Some(udp) = &self.udp {
            udp.stop();
        }
        journal::say("stopped");
    }
}
