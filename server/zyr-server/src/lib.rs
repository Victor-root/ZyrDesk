//! The ZyrDesk server: accounts, presence, rendezvous, and a relay in
//! reserve.
//!
//! One binary, two roles. The broker keeps accounts, devices, contacts
//! and shares, says who is online, and presents two devices to each
//! other with a signed ticket; the relay, when it comes, carries packets
//! it cannot read. Nothing of a session passes through here in ordinary
//! use, and no key of a session is ever known here. Conception:
//! `docs/SERVER.md`.

pub mod api;
pub mod config;
pub mod journal;
pub mod keys;
pub mod limits;
pub mod live;
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
    let app: App = Arc::new(State {
        limiter: Limiter::new(config.limits.login_attempts_per_minute),
        live: Arc::new(live::Live::new(store.clone(), key.clone())),
        tls_fingerprint: tls.as_ref().and_then(Tls::fingerprint),
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
        "listening on {address}{}",
        if app.tls_fingerprint.is_some() {
            ", TLS"
        } else {
            ", in the clear behind a reverse proxy"
        }
    ));
    Ok(Running {
        address,
        app,
        handle,
        serving,
    })
}

impl Running {
    /// Lets the connections in progress finish, then returns.
    pub async fn stop(self) {
        self.handle.graceful_shutdown(Some(GRACE));
        let _ = self.serving.await;
        journal::say("stopped");
    }
}
