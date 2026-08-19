//! What this computer is, asked of the service.
//!
//! Everything here is read, never held. When the service does not
//! answer, that is itself the state to show: the product is installed
//! but not running, which is something the person can act on, and this
//! is where they act on it.

use std::path::PathBuf;

use serde::Serialize;
use zyr_control::{Answer, Holdup, Request};
use zyr_proto::paths;

use crate::service;

/// A ZyrDesk the home screen shows.
#[derive(Serialize)]
pub struct Peer {
    pub name: String,
    pub fingerprint: String,
    pub address: String,
    /// Whether it is announcing itself right now. One written down by
    /// hand shows on a network that carries no announcement at all.
    pub seen: bool,
    /// Whether somebody wrote it down by hand. Only those can be taken
    /// off again.
    pub written: bool,
}

/// What the home screen shows about this computer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Standing {
    /// Name the person knows this machine by.
    pub name: String,
    /// What another computer pins to reach this one.
    pub fingerprint: String,
    /// Whether this computer can be reached right now.
    pub hosting: bool,
    /// What is in the way when it is not: « starting », « engineMissing »
    /// or « engineWontStand ».
    pub holdup: &'static str,
    /// Whether it is meant to be: the position of the switch.
    pub wanted: bool,
    /// Whether the ZyrDesk of this network are let in without anyone
    /// recognising them one by one.
    pub trusting: bool,
    /// Whether this computer answers from the moment it powers on,
    /// before anybody has signed in.
    pub at_boot: bool,
    /// Sessions this computer has open towards others.
    pub ways: usize,
    /// The build the service is running, which is not always this
    /// window's own.
    pub service_build: String,
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
            holdup: named(Holdup::Starting),
            wanted: false,
            trusting: false,
            at_boot: false,
            ways: 0,
            service_build: String::new(),
            unreachable: Some(reason),
        }
    }
}

/// How a holdup is named on the way to the window.
fn named(holdup: Holdup) -> &'static str {
    match holdup {
        Holdup::Starting => "starting",
        Holdup::EngineMissing => "engineMissing",
        Holdup::EngineWontStand => "engineWontStand",
    }
}

#[tauri::command]
pub async fn standing() -> Standing {
    match asked().await {
        Ok(standing) => standing,
        Err(reason) => Standing::without_the_service(reason),
    }
}

/// The product and the build this window was compiled from.
///
/// Kept under the person's eyes and shown beside the service's own: the
/// two are built together and installed together, and the day they
/// differ, that is the fault, whatever else it looks like.
#[tauri::command]
pub fn build() -> String {
    zyr_proto::version_line()
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
            address: peer.host,
            seen: peer.seen,
            written: peer.written,
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

/// Decides whether the ZyrDesk of this network are let in on sight.
#[tauri::command]
pub async fn set_trust(on: bool) -> Result<(), String> {
    match service::ask(&Request::SetTrust { on }).await? {
        Answer::Done => Ok(()),
        other => Err(service::unexpected(other)),
    }
}

/// Decides whether ZyrDesk comes back on its own with Windows.
///
/// Two things at once, deliberately, because they are one thing to the
/// person: the service starts with the machine, so it answers before
/// anybody has signed in, and this window starts with the session, so
/// the icon is there to say so. Turned off, nothing of this product runs
/// until somebody opens it.
#[tauri::command]
pub async fn set_at_boot(on: bool) -> Result<(), String> {
    match service::ask(&Request::SetAtBoot { on }).await? {
        Answer::Done => {}
        other => return Err(service::unexpected(other)),
    }
    crate::startup::with_windows(on)?;
    crate::journal::note(if on {
        "ZyrDesk will come back with Windows"
    } else {
        "ZyrDesk will not come back on its own"
    });
    Ok(())
}

/// Stops the service, and with it everything this computer was holding.
///
/// Asked of the service rather than of Windows: stopping a service the
/// ordinary way wants administrator rights, and a product that asked for
/// them every time somebody quit would be unusable.
#[tauri::command]
pub async fn stop_service() -> Result<(), String> {
    match service::ask(&Request::Stop).await? {
        Answer::Done => Ok(()),
        other => Err(service::unexpected(other)),
    }
}

/// Writes a computer down.
///
/// The way back when the network announces nothing. Everything else here
/// rests on two ZyrDesk hearing each other on the local network, and a
/// network that drops those announcements would otherwise leave the
/// product with no way in at all.
///
/// With an address, the computer also stays on the home screen: writing
/// it down once has to be enough, or the copying this replaces would
/// simply happen at every session instead of at the first.
#[tauri::command]
pub async fn authorize(
    fingerprint: String,
    host: Option<String>,
    name: Option<String>,
) -> Result<(), String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;
    let host = host
        .map(|host| host.trim().to_string())
        .filter(|host| !host.is_empty());
    let name = name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());

    match service::ask(&Request::Authorize { peer, host, name }).await? {
        Answer::Done => {
            crate::journal::note(&format!("{peer} written down"));
            Ok(())
        }
        other => Err(service::unexpected(other)),
    }
}

/// Takes a computer written down by hand off the home screen.
///
/// And out of the list of those allowed in, at the same time: one that
/// disappeared from the screen while still being able to reach this
/// computer would be a promise the product does not keep.
#[tauri::command]
pub async fn forget(fingerprint: String) -> Result<(), String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;
    match service::ask(&Request::Forget { peer }).await? {
        Answer::Done => {
            crate::journal::note(&format!("{peer} forgotten"));
            Ok(())
        }
        other => Err(service::unexpected(other)),
    }
}

/// Installs the ZyrDesk service and starts it.
///
/// The one moment the product asks Windows for administrator rights, and
/// the one prompt the person ever sees: a service is what makes this
/// computer reachable before anybody has signed in, and Windows lets no
/// program register one on its own.
#[tauri::command]
pub async fn start_service() -> Result<(), String> {
    let program = service_program()?;
    crate::journal::note(&format!("asking to set up {}", program.display()));

    // On a thread where waiting is allowed: this holds until the person
    // has answered the elevation prompt and the service has started.
    let outcome = tokio::task::spawn_blocking(move || set_up(&program))
        .await
        .map_err(|e| format!("la mise en service n'a pas pu être menée : {e}"))?;
    crate::journal::note(&match &outcome {
        Ok(()) => "service set up".to_string(),
        Err(reason) => format!("service not set up: {reason}"),
    });
    outcome
}

/// The service program, beside this one.
///
/// The two are built and shipped together, so this is where it is;
/// looking for it anywhere else would be guessing.
fn service_program() -> Result<PathBuf, String> {
    let here =
        std::env::current_exe().map_err(|e| format!("ce programme ne sait pas où il est : {e}"))?;
    let program = here.with_file_name(paths::executable_name("zyrdeskd"));
    if !program.is_file() {
        return Err(format!(
            "le service ZyrDesk est introuvable à côté de cette fenêtre :\n  {}",
            program.display()
        ));
    }
    Ok(program)
}

#[cfg(windows)]
fn set_up(program: &std::path::Path) -> Result<(), String> {
    crate::elevated::run(program, "setup")
}

/// Outside Windows there is no service to install: the product is a
/// Windows one, and this exists so the rest stays compiled and checked
/// everywhere.
#[cfg(not(windows))]
fn set_up(_program: &std::path::Path) -> Result<(), String> {
    Err("le service ZyrDesk n'existe que sous Windows".to_string())
}

async fn asked() -> Result<Standing, String> {
    match service::ask(&Request::Standing).await? {
        Answer::Standing(standing) => Ok(Standing {
            name: zyr_proto::machine::name(),
            fingerprint: standing.fingerprint.to_string(),
            hosting: standing.hosting,
            holdup: named(standing.holdup),
            wanted: standing.wanted,
            trusting: standing.trusting,
            at_boot: standing.at_boot,
            ways: standing.ways,
            service_build: standing.build,
            unreachable: None,
        }),
        other => Err(service::unexpected(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_that_does_not_answer_leaves_the_switches_down() {
        // Sinon la fenêtre montrerait un ordinateur joignable alors que
        // rien ne tourne pour le joindre.
        let standing = Standing::without_the_service("le service ne tourne pas".to_string());
        assert!(!standing.hosting);
        assert!(!standing.wanted);
        assert!(!standing.trusting);
        assert!(standing.unreachable.is_some());
        assert_eq!(standing.name, zyr_proto::machine::name());
    }

    #[test]
    fn every_holdup_has_a_name_the_window_knows() {
        for holdup in [
            Holdup::Starting,
            Holdup::EngineMissing,
            Holdup::EngineWontStand,
        ] {
            let name = named(holdup);
            assert!(!name.is_empty());
            assert!(!name.contains('-'), "{name}");
        }
    }
}
