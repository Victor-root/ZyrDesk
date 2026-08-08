//! Sessions, from the interface: opening one, and finding those already
//! running.
//!
//! The whole opening sequence lives in `zyr-session`, shared with the
//! command line. What is here is the shape it takes in a window: it runs
//! away from the interface thread, and what happens on the way is sent
//! back as events rather than waited for, because pairing shows a code
//! and then waits for someone to walk to the other computer.
//!
//! A window is not where a session lives, though. Asking the service
//! what it holds is what lets a window opened afterwards, or reopened
//! after a crash, show the session instead of an empty home screen.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use zyr_control::{Answer, Request};
use zyr_proto::session::SessionSettings;
use zyr_session::{Outcome, Step, Wanted};

use crate::service;

/// Names the interface listens on. One per moment worth drawing.
const STEP: &str = "session-step";
const ENDED: &str = "session-ended";

/// A moment of the opening, on its way to the window.
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Told {
    /// The tunnel stands.
    Reached {
        packet: u16,
    },
    /// This code has to be typed on the other computer.
    PairingNeeded {
        pin: String,
    },
    Paired,
    Starting,
    /// The picture is up and the session belongs to the service now.
    Live,
}

/// How a session finished, or failed to start.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Finished {
    /// True when nothing went wrong.
    ok: bool,
    /// What to show the person. Empty when there is nothing to say.
    message: String,
}

/// A session already under way, as the service describes it.
#[derive(Serialize)]
pub struct Ongoing {
    /// Remote computer, as it was named when the session was asked for.
    pub towards: String,
    /// What matches it to a computer on screen.
    pub fingerprint: String,
    /// How long the picture has been up, in seconds.
    pub since: u64,
}

/// The sessions this computer is holding.
///
/// An empty list is the ordinary answer, and so is the one given when
/// the service cannot be reached: the home card already says out loud
/// that it is not running, and saying it twice would only add noise.
#[tauri::command]
pub async fn sessions() -> Vec<Ongoing> {
    service::list(&Request::Sessions, |answer| match answer {
        Answer::Session(session) => Some(Ongoing {
            towards: session.towards,
            fingerprint: session.peer.to_string(),
            since: session.since.as_secs(),
        }),
        _ => None,
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub fn connect(app: AppHandle, host: String, fingerprint: String) -> Result<(), String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;

    let wanted = Wanted {
        host: host.trim().to_string(),
        peer: Some(peer),
        settings: SessionSettings::default(),
        pair_again: false,
    };

    // On a thread of its own, and not one of the interface's: the
    // opening blocks for as long as a pairing takes, which is as long as
    // it takes someone to walk to another computer.
    std::thread::spawn(move || drive(&app, wanted));
    Ok(())
}

fn drive(app: &AppHandle, wanted: Wanted) {
    let running = match zyr_session::open(&wanted, &mut |step| say(app, told(step))) {
        Ok(running) => running,
        Err(e) => return finish(app, false, e.to_string()),
    };

    say(app, Told::Live);

    // Waiting costs nothing here and buys the one thing the person wants
    // afterwards: whether the session ended by itself or fell over.
    match running.wait() {
        Ok(Outcome::Ended) => finish(app, true, String::new()),
        Ok(Outcome::Failed) => finish(
            app,
            false,
            "La session s'est arrêtée sur une erreur.".into(),
        ),
        Ok(Outcome::Unreachable) => {
            finish(app, false, "L'ordinateur distant n'a pas répondu.".into())
        }
        Ok(Outcome::Unknown { .. }) => finish(
            app,
            false,
            "Le lecteur s'est arrêté sans dire pourquoi.".into(),
        ),
        Err(e) => finish(app, false, e.to_string()),
    }
}

fn told(step: Step) -> Told {
    match step {
        Step::Reached { packet } => Told::Reached { packet },
        Step::PairingNeeded { pin } => Told::PairingNeeded { pin },
        Step::Paired => Told::Paired,
        Step::Starting => Told::Starting,
    }
}

/// An event the window may or may not still be there to hear. A closed
/// window is not a reason to stop driving the session.
fn say(app: &AppHandle, what: Told) {
    let _ = app.emit(STEP, what);
}

fn finish(app: &AppHandle, ok: bool, message: String) {
    let _ = app.emit(ENDED, Finished { ok, message });
}
