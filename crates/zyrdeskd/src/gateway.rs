//! The tunnel end the service holds.
//!
//! This is the one door open on this computer. Everything a session
//! needs goes through it: the engine's seven ports are multiplexed into
//! a single encrypted connection. That is what lets the engine close
//! back onto the local machine, where nothing on the network can reach
//! it, and what leaves a single rule to write in a firewall.
//!
//! Who may come in is decided by fingerprint, from a list the service
//! reads again as it runs. Authorising one more computer must not mean
//! cutting the session in progress, and asking a small file every few
//! seconds costs nothing next to watching the filesystem on every
//! platform.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::task::{JoinHandle, JoinSet};
use zyr_proto::net::{EnginePorts, TUNNEL_PORT};
use zyr_proto::paths;
use zyr_transport::{
    AllowedPeers, EndpointError, Identity, MediaProfile, TunnelEndpoint, authorized,
};
use zyr_tunnel::Tunnel;

use crate::log::Log;

/// Every network interface: the computer is reachable from wherever the
/// other one is.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Where the engine listens, and the only place the tunnel hands it
/// anything.
const ENGINE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// How often the list of authorised devices is read again.
const AUTHORIZED_REFRESH: Duration = Duration::from_secs(5);

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
    pub fn open(runtime: &Handle, engine: EnginePorts, log: &Log) -> io::Result<Self> {
        // The transport registers with the runtime as it is built, so it
        // has to be built from inside it.
        let _guard = runtime.enter();

        let identity =
            Identity::load_or_create(&paths::identity_dir()).map_err(io::Error::other)?;
        let list = paths::authorized_devices();
        let allowed: AllowedPeers = authorized::read(&list)?.into_iter().collect();
        if allowed.is_empty() {
            log.write(
                "no device authorised yet, nothing can connect: \
                 run zyr-cli host authorize with the other computer's fingerprint",
            );
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

        Ok(Self {
            tasks: vec![
                runtime.spawn(keep_the_list_fresh(list, allowed, log.clone())),
                runtime.spawn(serve(endpoint, engine, log.clone())),
            ],
        })
    }
}

/// Takes in the devices that connect, one session each.
async fn serve(endpoint: TunnelEndpoint, engine: EnginePorts, log: Log) {
    let mut sessions = JoinSet::new();
    loop {
        match endpoint.accept().await {
            Ok(connection) => {
                let log = log.clone();
                sessions.spawn(async move { one_session(connection, engine, log).await });
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

async fn one_session(connection: zyr_transport::Connection, engine: EnginePorts, log: Log) {
    let mut tunnel = match Tunnel::host(connection, ENGINE, engine).await {
        Ok(tunnel) => tunnel,
        Err(e) => {
            log.write(&format!("session not opened: {e}"));
            return;
        }
    };
    log.write("session open");

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

/// Reads the authorised devices again, so a new one gets in without the
/// service being restarted.
async fn keep_the_list_fresh(list: PathBuf, allowed: AllowedPeers, log: Log) {
    let mut reported: Option<String> = None;
    loop {
        match authorized::read(&list) {
            Ok(devices) => {
                if reported.take().is_some() {
                    log.write("authorised devices readable again");
                }
                allowed.replace_with(devices);
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
