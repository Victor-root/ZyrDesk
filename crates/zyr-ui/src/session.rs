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

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use zyr_control::{Answer, Request};
use zyr_proto::session::{Asked, Codec, Preferred, SessionSettings};
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
    /// The picture is in our window, and the session belongs to the
    /// service now.
    ///
    /// Not one of these two things and then the other: the opening screen
    /// comes down on this, so it has to wait for the later of them, and
    /// the later one is the picture.
    Live,
    /// The picture is being opened again, the person having changed what
    /// the session asks for.
    ///
    /// The steps that follow are the ones an opening always goes through,
    /// and the window shows them the same way. It only has to be told
    /// that one is starting, since nobody clicked anything to start it.
    Again,
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
    /// The way the service holds towards that computer.
    ///
    /// Carried because one thing a session can ask travels on the
    /// product's own channel rather than through the engines, and that
    /// channel is reached by naming the way: pressing Ctrl+Alt+Suppr
    /// over there.
    pub way: u64,
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
            way: session.way.0,
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

/// The player that was stopped so the picture could be opened again,
/// rather than because the session was over.
///
/// Read by whoever was waiting on that player, the instant it stops:
/// stopping it is the only way to change what it was told, and the two
/// reasons to stop it look exactly alike from the outside.
///
/// The player and not a plain yes: a second ask, landing on a player that
/// had already gone, would otherwise leave a yes behind that reopened the
/// session after the person closed it. Named this way it can only ever
/// reopen the picture it was written for.
static OPEN_AGAIN: AtomicU32 = AtomicU32::new(0);

/// The three numbers the picture on screen was opened with.
///
/// Kept because they cannot be read back from anywhere. What a session
/// asks for is settled when its engine starts and told to it once, so a
/// size, a rate or a codec chosen afterwards is written down and nothing
/// more until the picture is opened again. This is what lets the session's
/// own menu say which of the two it is showing.
static SHOWN_AS: Mutex<Option<(Asked, u32, Codec)>> = Mutex::new(None);

/// What the engine is told once, at its start, and never again.
///
/// The rest of what a person chooses is either asked of the engine while
/// it runs, by the keystrokes it answers to, or belongs to this side of
/// the picture entirely. Only these three are worth opening the picture
/// again for, and only these three are compared to know whether they are
/// still what is on screen.
fn told_once(preferred: &Preferred) -> (Asked, u32, Codec) {
    (preferred.asked, preferred.bitrate_kbps, preferred.codec)
}

/// Whether what is chosen now is not what the picture on screen shows.
///
/// False when nothing is on screen: there is then nothing to apply it to,
/// and the next session opens with it anyway.
pub fn waiting_to_be_applied(preferred: &Preferred) -> bool {
    match *SHOWN_AS.lock().expect("réglages de l'image") {
        Some(shown) => shown != told_once(preferred),
        None => false,
    }
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
    let _ = crate::picture::take_the_screen_for_a_session(
        &app,
        preferred.display_mode == zyr_proto::session::DisplayMode::Fullscreen,
    );

    let wanted = Wanted {
        host: host.trim().to_string(),
        peer: Some(peer),
        settings: what_to_ask_for(&app, preferred),
        pair_again: false,
        hush_the_far_speakers: preferred.mute_far_speakers,
    };

    // On a thread of its own, and not one of the interface's: the
    // opening blocks for as long as a pairing takes, which is as long as
    // it takes someone to walk to another computer.
    std::thread::spawn(move || drive(&app, wanted, preferred));
    Ok(())
}

/// What a session opened right now asks for, said in the journal on the
/// way past.
///
/// The screen is measured after the window has taken its place and never
/// before: the screen that counts is the one the picture will be shown
/// on, and that is only settled once the window has moved.
///
/// And the choices are read at the last moment rather than held: they can
/// have been changed from the settings screen since this window opened, or
/// from the session's own menu since the picture was last opened.
fn what_to_ask_for(app: &AppHandle, preferred: Preferred) -> SessionSettings {
    let screen = crate::picture::the_screen_of_this_computer(app);
    let settings = preferred.settings(screen);
    crate::picture::tell_what_is_asked_for(app, screen, preferred.asked, &settings);
    settings
}

/// Opens the session and holds it, from the first tunnel to the last
/// picture.
///
/// It opens more than once when the person changes what the session asks
/// for and applies it. The engine is told a size, a rate and a codec when
/// it starts and never again, so the only way to change one is to open the
/// picture again; everything around it stands, this thread and the
/// pairing included, and what it costs is the few seconds an opening
/// takes.
fn drive(app: &AppHandle, mut wanted: Wanted, mut preferred: Preferred) {
    crate::journal::note(&format!("session demandée vers {}", wanted.host));
    loop {
        *SHOWN_AS.lock().expect("réglages de l'image") = Some(told_once(&preferred));

        let towards = wanted.host.clone();
        let running = match zyr_session::open(
            &wanted,
            &mut |step| {
                crate::journal::note(&written(&step));
                // The floating button hangs on that process, and this window
                // is the only one that knows its number until the service
                // does; the way travels with it, so the session can be ended
                // during the seconds the service does not believe in it yet.
                if let Step::Showing { process, at } = &step {
                    crate::floating::expect(app, *process, &towards, at);
                    lay_the_picture_as_soon_as_it_opens(app.clone(), *process);
                }
                if let Some(told) = told(step) {
                    say(app, told);
                }
            },
            // The one thing this crate can answer and the opening cannot: a
            // player the person stopped and a player the far computer turned
            // away both look like an engine that lost its stream.
            &|| !crate::floating::Floating::a_close_was_asked_for(app),
        ) {
            Ok(running) => running,
            Err(e) => {
                crate::journal::note(&format!(
                    "session non ouverte : {}",
                    e.to_string().replace('\n', " ")
                ));
                return finish(app, false, e.to_string());
            }
        };

        let process = running.process_id();
        crate::journal::note(&format!("session en cours, lecteur {process}"));

        // Waited for here, and this is the whole of what the opening
        // screen is for: it covers the seconds between somebody asking
        // for a session and there being something to look at. Said the
        // moment the service took the session instead, it was said too
        // early whenever the two computers had to be introduced again,
        // because the wait that hid the difference the rest of the time
        // is skipped on that road. The person then watched their home
        // screen for four seconds, with a session card on it and no
        // picture, and the picture arrived with no announcement at all.
        //
        // Costs nothing where it was already right: by the time the
        // service holds an ordinary session, the picture has been in our
        // window for seconds.
        if !lay_the_picture_when_it_opens(app, process) {
            crate::journal::note(&format!(
                "le lecteur {process} n'a pas ouvert d'image en {} s, l'écran d'ouverture est \
                 retiré quand même",
                WINDOW_TAKES.as_secs()
            ));
        }
        say(app, Told::Live);

        // Waiting costs nothing here and buys the one thing the person
        // wants afterwards: whether the session ended by itself or fell
        // over.
        let ended = running.wait();

        // Asked for again rather than over. Read before anything else is:
        // a player stopped to be told something new looks exactly like one
        // that stopped for good, and only this tells the two apart.
        if OPEN_AGAIN.swap(0, Ordering::SeqCst) == process {
            crate::journal::note(&format!(
                "image relancée avec ce qui est choisi maintenant (le lecteur a dit {ended:?})"
            ));
            say(app, Told::Again);
            // What is kept when the service cannot be asked is what the
            // picture was already showing, never the ordinary settings:
            // the person asked for one thing to change, not for three
            // others to go back to what the product does by default.
            preferred = tauri::async_runtime::block_on(crate::settings::what_was_chosen())
                .unwrap_or(preferred);
            wanted.settings = what_to_ask_for(app, preferred);
            // The way is opened again with the picture, and what the far
            // computer was asked went with the old one: it has to be
            // asked afresh, and with what is chosen now.
            wanted.hush_the_far_speakers = preferred.mute_far_speakers;
            continue;
        }

        let on_purpose = crate::floating::Floating::was_closed_on_purpose(app);
        crate::journal::note(&match &ended {
            Ok(outcome) if on_purpose => {
                format!("session fermée volontairement, le lecteur a dit {outcome:?}")
            }
            Ok(outcome) => format!("session terminée : {outcome:?}"),
            Err(e) => format!("session terminée sur une erreur système : {e}"),
        });

        // Closing the far computer's desktop takes the stream away from
        // the engine, which stops the only way it knows how: on a
        // failure. It is still exactly what was asked for.
        if on_purpose {
            return finish(app, true, String::new());
        }

        return match ended {
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
        };
    }
}

/// Opens the picture again with what is chosen now, the session standing.
///
/// The one thing the session's own menu cannot do by asking the engine:
/// a size, a rate and a codec are handed to it when it starts and never
/// again. So the player is stopped and started, which the person sees as
/// the picture going away and coming back, and everything else stands.
///
/// Only a session this window is driving: the numbers to open it again
/// with live on that window's own thread, and a session opened elsewhere
/// has nobody here to hear this.
#[tauri::command]
pub async fn apply_session(app: AppHandle) -> Result<(), String> {
    if !opening() {
        return Err(
            "cette session n'a pas été ouverte depuis cette fenêtre.\n  \
                    Les réglages s'appliqueront à la prochaine."
                .to_string(),
        );
    }
    let process = crate::floating::player(&app)
        .await
        .ok_or("aucune session en cours")?;

    // Written down before the player is stopped, and never after: whoever
    // is waiting on that player wakes the instant it goes, and reads this
    // to know whether the session is over or beginning again.
    OPEN_AGAIN.store(process, Ordering::SeqCst);
    crate::journal::note(&format!(
        "réglages appliqués : le lecteur {process} est relancé"
    ));
    if !crate::floating::stop_the_player(process) {
        return Err("l'image n'a pas pu être relancée".to_string());
    }
    Ok(())
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
        lay_the_picture_when_it_opens(&app, process);
    });
}

/// The waiting itself, so that whoever needs the answer can have it.
///
/// Answers whether the picture ended up in our window. Called from two
/// places at once and none the worse for it: laying a picture already
/// laid does nothing, and the lock inside is there for exactly this.
fn lay_the_picture_when_it_opens(app: &AppHandle, process: u32) -> bool {
    let until = std::time::Instant::now() + WINDOW_TAKES;
    while std::time::Instant::now() < until {
        if crate::picture::hold(app, process) {
            return true;
        }
        std::thread::sleep(WINDOW_STEP);
    }
    false
}

/// The same moment, as the opening screen shows it, when it shows it at
/// all.
///
/// Not every step is worth a screen. A far computer that would not
/// silence its own speakers has nothing to do with what the person is
/// waiting for, and putting it there would replace « the picture is
/// coming » with a sentence about sound.
fn told(step: Step) -> Option<Told> {
    Some(match step {
        Step::Reached { packet } => Told::Reached { packet },
        Step::Pairing { again } => Told::Pairing { again },
        Step::PairingNeeded { pin } => Told::PairingNeeded { pin },
        Step::Paired => Told::Paired,
        Step::Starting => Told::Starting,
        Step::Showing { process, .. } => Told::Showing { process },
        Step::SpeakersLeftAlone { .. } => return None,
    })
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
        Step::SpeakersLeftAlone { refused } => {
            format!("les enceintes de l'ordinateur distant restent allumées : {refused}")
        }
    }
}

/// An event the window may or may not still be there to hear. A closed
/// window is not a reason to stop driving the session.
fn say(app: &AppHandle, what: Told) {
    let _ = app.emit(STEP, what);
}

/// What state the home window is in, in words.
///
/// Written on both sides of the ending. A session that finishes must
/// leave that window exactly as it found it, and « sometimes it ends up
/// minimised » is the kind of report that cannot be chased without
/// knowing which of the two sides it was already on.
fn how_the_window_stands(app: &AppHandle, when: &str) {
    use tauri::Manager as _;

    let Some(window) = app.get_webview_window(crate::HOME) else {
        crate::journal::note(&format!("{when} : plus de fenêtre d'accueil"));
        return;
    };
    fn say(what: tauri::Result<bool>) -> &'static str {
        match what {
            Ok(true) => "oui",
            Ok(false) => "non",
            Err(_) => "?",
        }
    }
    crate::journal::note(&format!(
        "{when} : accueil réduit={} visible={} plein écran={}",
        say(window.is_minimized()),
        say(window.is_visible()),
        say(window.is_fullscreen()),
    ));
}

fn finish(app: &AppHandle, ok: bool, message: String) {
    how_the_window_stands(app, "fin de session, avant");
    OPENING.store(false, Ordering::SeqCst);
    // A session that is over asks for nothing, and shows nothing. Both
    // are put down here rather than left standing: an « open it again »
    // that outlived its session has nothing left to name, and a picture
    // nobody is showing would have the session menu offering to apply
    // changes to it.
    OPEN_AGAIN.store(0, Ordering::SeqCst);
    *SHOWN_AS.lock().expect("réglages de l'image") = None;
    crate::floating::expect_nothing(app);
    // Taken down here rather than left to the watch. The watch comes
    // round once a second and asks the service what it holds, and until
    // it does the button hangs over a picture that has gone. Whoever
    // drove the session knows it is over the instant it is.
    crate::picture::let_go(app);
    crate::floating::lower(app);
    // The screen goes back to the person: what took it was the session.
    let _ = crate::picture::take_the_screen(app, false);
    // And so does the window. A session ends with something to say, an
    // error most of the time, and it is said on the home screen; behind
    // a taskbar button it is said to nobody. The window can be down
    // there for reasons of its own by then, put away by a hand during
    // the session or by the system, which takes a window covering the
    // whole screen down when the front leaves it.
    crate::show_home(app);
    let _ = app.emit(ENDED, Finished { ok, message });
    how_the_window_stands(app, "fin de session, après");
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::session::DisplayMode;

    #[test]
    fn only_what_the_engine_is_told_once_asks_for_the_picture_to_be_reopened() {
        // Relancer l'image coûte les quelques secondes d'une ouverture :
        // ça ne se fait que pour ce que le moteur apprend au démarrage et
        // jamais après. Le reste se demande au moteur en marche, ou ne le
        // regarde pas du tout, et le proposer serait faire payer ça pour
        // rien.
        let shown = Preferred::default();
        for other in [
            Preferred {
                display_mode: DisplayMode::Fullscreen,
                ..shown
            },
            Preferred {
                absolute_mouse: !shown.absolute_mouse,
                ..shown
            },
            Preferred {
                stats_overlay: !shown.stats_overlay,
                ..shown
            },
        ] {
            assert_eq!(told_once(&other), told_once(&shown));
        }

        // Les trois autres, elles, ne peuvent pas se changer autrement.
        for other in [
            Preferred {
                asked: Asked::Fixed(1280, 720),
                ..shown
            },
            Preferred {
                bitrate_kbps: shown.bitrate_kbps + 5_000,
                ..shown
            },
            Preferred {
                codec: Codec::H264,
                ..shown
            },
        ] {
            assert_ne!(told_once(&other), told_once(&shown));
        }
    }

    #[test]
    fn nothing_is_waiting_to_be_applied_when_nothing_is_on_screen() {
        // Hors session il n'y a pas d'image à relancer : la prochaine
        // s'ouvrira avec ce qui est choisi, sans que personne ait à le
        // demander.
        assert!(!waiting_to_be_applied(&Preferred::default()));
        assert!(!waiting_to_be_applied(&Preferred {
            asked: Asked::Fixed(1280, 720),
            ..Preferred::default()
        }));
    }
}
