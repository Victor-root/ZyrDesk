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

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use zyr_control::{Answer, Request};
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
    /// The two computers have never met, and are being introduced.
    Pairing {
        /// They believed they knew each other, and the far one did not
        /// agree.
        again: bool,
    },
    /// The same, without a tunnel to carry the code. Only the diagnostic
    /// path gets here, and the window never opens one.
    PairingNeeded {
        pin: String,
    },
    Paired,
    Starting,
    /// The engine is running, as that process.
    Showing {
        process: u32,
    },
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
    /// Player showing it, so its window can be found among the others.
    pub process: u32,
    /// Where the tunnel puts that computer on this machine.
    pub at: String,
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
            process: session.process,
            at: session.at,
        }),
        _ => None,
    })
    .await
    .unwrap_or_default()
}

/// Set from the moment a session is asked for to the moment it is over.
///
/// The window's own screen refuses to ask for two sessions at once, but
/// a screen is not a guard: reloaded, it forgets, and two engines opened
/// on the same desktop cannot both be driven. This is the guard.
static OPENING: AtomicBool = AtomicBool::new(false);

/// Whether a session is being opened right now.
pub fn opening() -> bool {
    OPENING.load(Ordering::Relaxed)
}

#[tauri::command]
pub async fn connect(app: AppHandle, host: String, fingerprint: String) -> Result<(), String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;

    // One at a time, held here and not merely on the screen. Taken
    // before anything moves, and given back by `finish`, which every
    // road out of `drive` ends at.
    if crate::floating::a_session_is_up(&app) || OPENING.swap(true, Ordering::SeqCst) {
        return Err("une session est déjà en cours".to_string());
    }

    let preferred = crate::settings::preferred().await;
    // The window takes the screen before anything else does, so the
    // opening is read on the same surface the picture will land on
    // rather than in a small window that grows under the eye.
    let _ = crate::picture::take_the_screen(
        &app,
        preferred.display_mode == zyr_proto::session::DisplayMode::Fullscreen,
    );

    let wanted = Wanted {
        host: host.trim().to_string(),
        peer: Some(peer),
        // Read at the last moment rather than held: the settings screen
        // may have changed them since this window opened.
        settings: preferred.settings(),
        pair_again: false,
    };

    // On a thread of its own, and not one of the interface's: the
    // opening blocks for as long as a pairing takes, which is as long as
    // it takes someone to walk to another computer.
    std::thread::spawn(move || drive(&app, wanted));
    Ok(())
}

fn drive(app: &AppHandle, wanted: Wanted) {
    crate::journal::note(&format!("session demandée vers {}", wanted.host));
    let towards = wanted.host.clone();
    let running = match zyr_session::open(&wanted, &mut |step| {
        crate::journal::note(&written(&step));
        // The floating button hangs on that process, and this window is
        // the only one that knows its number until the service does; the
        // way travels with it, so the session can be ended during the
        // seconds the service does not believe in it yet.
        if let Step::Showing { process, at } = &step {
            crate::floating::expect(app, *process, &towards, at);
            lay_the_picture_as_soon_as_it_opens(app.clone(), *process);
        }
        say(app, told(step));
    }) {
        Ok(running) => running,
        Err(e) => {
            crate::journal::note(&format!(
                "session non ouverte : {}",
                e.to_string().replace('\n', " ")
            ));
            return finish(app, false, e.to_string());
        }
    };

    crate::journal::note(&format!(
        "session en cours, lecteur {}",
        running.process_id()
    ));
    say(app, Told::Live);

    // Waiting costs nothing here and buys the one thing the person wants
    // afterwards: whether the session ended by itself or fell over.
    let ended = running.wait();
    let on_purpose = crate::floating::Floating::was_closed_on_purpose(app);
    crate::journal::note(&match &ended {
        Ok(outcome) if on_purpose => {
            format!("session fermée volontairement, le lecteur a dit {outcome:?}")
        }
        Ok(outcome) => format!("session terminée : {outcome:?}"),
        Err(e) => format!("session terminée sur une erreur système : {e}"),
    });

    // Closing the far computer's desktop takes the stream away from the
    // engine, which stops the only way it knows how: on a failure. It is
    // still exactly what was asked for.
    if on_purpose {
        return finish(app, true, String::new());
    }

    match ended {
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

/// Ends the session in progress, the person having closed the window on
/// it.
///
/// The same path the menu takes, and no second one.
pub fn end_it(app: &AppHandle) {
    let asked = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::journal::note("session terminée par la croix de la fenêtre");
        if let Err(reason) = crate::floating::ask(&asked, crate::floating::Act::End).await {
            crate::journal::note(&format!(
                "la croix n'a pas pu terminer la session : {}",
                reason.replace('\n', " ")
            ));
        }
    });
}

/// How long the engine is given to open its window.
const WINDOW_TAKES: Duration = Duration::from_secs(20);

/// How often it is looked for while it does.
///
/// Once a millisecond, which is a lot to ask of a machine and is asked
/// for a few seconds at most. What is being raced is the engine settling
/// its own window: it creates it hidden and shows it once its size, its
/// place and its icon are done, and everything of ours has to happen
/// inside that. Losing that race costs an empty frame on screen, which is
/// the one thing this whole arrangement exists to avoid.
const WINDOW_STEP: Duration = Duration::from_millis(1);

/// Lays the picture in our window the moment the engine opens it.
///
/// The session watch would do it too, but it comes round once a second,
/// and that second is exactly what is seen: an ordinary window with a
/// title bar, at the size of the stream, in the middle of whichever
/// screen the system calls first, over the top of ours. Waited for at
/// the rhythm of a frame instead.
///
/// On a thread of its own and never on the spot: this is called from
/// inside the opening, and holding it there would hold back everything
/// the window is waiting to be told.
fn lay_the_picture_as_soon_as_it_opens(app: AppHandle, process: u32) {
    std::thread::spawn(move || {
        let until = std::time::Instant::now() + WINDOW_TAKES;
        while std::time::Instant::now() < until {
            if crate::picture::hold(&app, process) {
                return;
            }
            std::thread::sleep(WINDOW_STEP);
        }
    });
}

fn told(step: Step) -> Told {
    match step {
        Step::Reached { packet } => Told::Reached { packet },
        Step::Pairing { again } => Told::Pairing { again },
        Step::PairingNeeded { pin } => Told::PairingNeeded { pin },
        Step::Paired => Told::Paired,
        Step::Starting => Told::Starting,
        Step::Showing { process, .. } => Told::Showing { process },
    }
}

/// The same moment, in the journal.
///
/// The window shows it for as long as it is on screen and then draws
/// something else over it. Opening a session is where most of what can go
/// wrong goes wrong, and every step of it is worth having in writing
/// afterwards, when there is nothing left on screen to look at.
fn written(step: &Step) -> String {
    match step {
        Step::Reached { packet } => format!("tunnel ouvert, paquets de {packet} octets"),
        Step::Pairing { again: false } => "présentation des deux ordinateurs".to_string(),
        Step::Pairing { again: true } => {
            "l'ordinateur distant ne reconnaît plus celui-ci, nouvelle présentation".to_string()
        }
        Step::PairingNeeded { .. } => {
            "en attente du code à taper sur l'ordinateur distant".to_string()
        }
        Step::Paired => "les deux ordinateurs se connaissent".to_string(),
        Step::Starting => "démarrage du lecteur".to_string(),
        Step::Showing { process, .. } => format!("lecteur en marche, processus {process}"),
    }
}

/// An event the window may or may not still be there to hear. A closed
/// window is not a reason to stop driving the session.
fn say(app: &AppHandle, what: Told) {
    let _ = app.emit(STEP, what);
}

fn finish(app: &AppHandle, ok: bool, message: String) {
    OPENING.store(false, Ordering::SeqCst);
    crate::floating::expect_nothing(app);
    // Taken down here rather than left to the watch. The watch comes
    // round once a second and asks the service what it holds, and until
    // it does the button hangs over a picture that has gone. Whoever
    // drove the session knows it is over the instant it is.
    crate::picture::let_go(app);
    crate::floating::lower(app);
    // The screen goes back to the person: what took it was the session.
    let _ = crate::picture::take_the_screen(app, false);
    let _ = app.emit(ENDED, Finished { ok, message });
}
