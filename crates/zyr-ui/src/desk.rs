//! What this computer is, asked of the service.
//!
//! Everything here is read, never held. When the service does not
//! answer, that is itself the state to show: the product is installed
//! but not running, which is something the person can act on.

use serde::Serialize;
use zyr_control::{Answer, Request};

use crate::service;

/// A ZyrDesk found on the local network.
#[derive(Serialize)]
pub struct Peer {
    pub name: String,
    pub fingerprint: String,
    pub address: String,
}

/// What the home screen shows about this computer.
#[derive(Serialize)]
pub struct Standing {
    /// Name the person knows this machine by.
    pub name: String,
    /// What another computer pins to reach this one.
    pub fingerprint: String,
    /// Whether this computer can be reached right now.
    pub hosting: bool,
    /// Whether it is meant to be: the position of the switch.
    pub wanted: bool,
    /// Sessions this computer has open towards others.
    pub ways: usize,
    /// Set when the service could not be asked, in words meant to be
    /// shown as they are.
    pub unreachable: Option<String>,
}

impl Standing {
    /// What is still true when the service is not there.
    fn without_the_service(reason: String) -> Self {
        Self {
            name: zyr_proto::machine::name(),
            fingerprint: String::new(),
            hosting: false,
            wanted: false,
            ways: 0,
            unreachable: Some(reason),
        }
    }
}

#[tauri::command]
pub async fn standing() -> Standing {
    match asked().await {
        Ok(standing) => standing,
        Err(reason) => Standing::without_the_service(reason),
    }
}

/// The computers seen on the local network.
///
/// An empty list is an answer, not a failure: a network with nobody else
/// on it is the ordinary case on a first install. Only the service being
/// absent is worth saying out loud, and the home card already says it.
#[tauri::command]
pub async fn peers() -> Vec<Peer> {
    service::list(&Request::Peers, |answer| match answer {
        Answer::Peer(peer) => Some(Peer {
            name: peer.name,
            fingerprint: peer.fingerprint.to_string(),
            address: peer.address.to_string(),
        }),
        _ => None,
    })
    .await
    .unwrap_or_default()
}

/// Decides whether this computer accepts being controlled.
///
/// Answers with what to show if it could not be done: a switch that
/// moved without anything happening behind it would be a lie.
#[tauri::command]
pub async fn set_hosting(on: bool) -> Result<(), String> {
    match service::ask(&Request::SetHosting { on }).await? {
        Answer::Done => Ok(()),
        other => Err(service::unexpected(other)),
    }
}

async fn asked() -> Result<Standing, String> {
    match service::ask(&Request::Standing).await? {
        Answer::Standing(standing) => Ok(Standing {
            name: zyr_proto::machine::name(),
            fingerprint: standing.fingerprint.to_string(),
            hosting: standing.hosting,
            wanted: standing.wanted,
            ways: standing.ways,
            unreachable: None,
        }),
        other => Err(service::unexpected(other)),
    }
}
