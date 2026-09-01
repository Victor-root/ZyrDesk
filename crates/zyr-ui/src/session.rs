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

// Relancer l'image et savoir par où elle passe ne se demande que depuis
// le menu du bouton flottant, qui n'existe que sous Windows comme la
// session elle-même.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use zyr_control::{Answer, Request};
use zyr_proto::session::{Asked, Codec, FarScreen, Preferred, SessionSettings};
use zyr_session::{Outcome, Step, Wanted};

use crate::service;

/// A session already under way, as the service describes it.
#[derive(PartialEq)]
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

/// The way the session in progress is held on, or nothing.
///
/// Asked of the service rather than remembered here, for the reason the
/// whole of this module is written that way: a session belongs to the
/// service and outlives this window, so a window opened in the middle of
/// one never saw it start and has nothing of its own to read.
///
/// The first, when there are several. Only one session at a time can be
/// opened from this window, so there is only ever one; a second would be
/// somebody else's, and its far computer is not the one on screen here.
pub async fn the_way_in_use() -> Option<zyr_control::WayId> {
    sessions()
        .await
        .first()
        .map(|session| zyr_control::WayId(session.way))
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

/// What the picture on screen was opened with.
///
/// Kept because it cannot be read back from anywhere. What a session asks
/// for is settled when its engine starts and told to it once, so anything
/// chosen afterwards is written down and nothing more until the picture
/// is opened again. This is what lets the session's own menu say which of
/// the two it is showing.
static SHOWN_AS: Mutex<Option<ToldOnce>> = Mutex::new(None);

/// Which of the far computer's screens this session is served from.
///
/// Empty is that computer's main screen, which is what every session
/// asks for until somebody says otherwise. Held here for the length of a
/// session and written into no settings file, deliberately: it names one
/// screen of one particular computer, and carrying it to the next
/// session would mean asking a different machine for a screen that is
/// not its own. A machine left showing the screen some earlier session
/// picked is a machine rearranged by having been looked at.
static FAR_SCREEN: Mutex<Option<String>> = Mutex::new(None);

/// The screens the far computer of the session in progress is showing
/// on, as it last named them.
///
/// Asked of that computer when the menu is opened and kept, so the menu
/// can say which screen is being watched without asking again at every
/// draw.
static FAR_SCREENS: Mutex<Vec<FarScreen>> = Mutex::new(Vec::new());

/// What the engine is told once, at its start, and never again.
///
/// The rest of what a person chooses is either asked of the engine while
/// it runs, by the keystrokes it answers to, or belongs to this side of
/// the picture entirely. Only these are worth opening the picture again
/// for, and only these are compared to know whether they are still what
/// is on screen.
///
/// The last two are told to the far computer's engine rather than to this
/// one's, and they are here for the same reason as the first three: its
/// engine reads them when it starts, so changing one means opening the
/// picture again, which is what asks that computer and starts its engine
/// over on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ToldOnce {
    asked: Asked,
    bitrate_kbps: u32,
    codec: Codec,
    steady_far_rate: bool,
    far_screen: Option<String>,
}

fn told_once(preferred: &Preferred) -> ToldOnce {
    ToldOnce {
        asked: preferred.asked,
        bitrate_kbps: preferred.bitrate_kbps,
        codec: preferred.codec,
        steady_far_rate: preferred.steady_far_rate,
        far_screen: the_far_screen(),
    }
}

/// Which of the far computer's screens the session is to be served from.
///
/// Empty is that computer's main screen. The two are one answer said two
/// ways, so picking the main screen by hand writes nothing here: left as
/// two answers, choosing the screen a session is already on would have
/// offered to open the picture again for a change that is not one.
pub fn the_far_screen() -> Option<String> {
    FAR_SCREEN.lock().expect("écran d'en face").clone()
}

/// The same, under the name the menu marks it by.
///
/// What was asked for, and the far computer's main screen when nothing
/// was: the menu has one line per screen and no line for « the main one »,
/// so the answer has to name a screen even when the choice was to name
/// none.
pub fn the_far_screen_named() -> String {
    the_far_screen()
        .or_else(|| {
            the_far_screens()
                .into_iter()
                .find(|screen| screen.main)
                .map(|screen| screen.id)
        })
        .unwrap_or_default()
}

/// Asks to be served from that screen, or from the far computer's main
/// one when nothing is named.
///
/// Written down and nothing more, exactly like the size, the rate and the
/// codec beside it: what a session is served from is settled when the far
/// engine starts, so the picture on screen goes on showing the screen it
/// was opened with and the menu offers to open it again.
pub fn ask_for_the_far_screen(id: Option<String>) {
    *FAR_SCREEN.lock().expect("écran d'en face") = id;
}

/// The far computer's screens, as it last named them.
pub fn the_far_screens() -> Vec<FarScreen> {
    FAR_SCREENS.lock().expect("écrans d'en face").clone()
}

/// Writes down what the far computer answered about its screens.
///
/// Kept so that a screen picked from the menu can be weighed against what
/// that computer actually offered: an identifier from anywhere else is a
/// window and a far computer that no longer agree, and honouring it would
/// hide that.
///
/// An empty answer is « it has not said » and never « it has none », so
/// it leaves what was known standing rather than emptying the line under
/// a menu somebody has open.
pub fn remember_the_far_screens(screens: &[FarScreen]) {
    if screens.is_empty() {
        return;
    }
    *FAR_SCREENS.lock().expect("écrans d'en face") = screens.to_vec();
}

/// Whether what is chosen now is not what the picture on screen shows.
///
/// False when nothing is on screen: there is then nothing to apply it to,
/// and the next session opens with it anyway.
pub fn waiting_to_be_applied(preferred: &Preferred) -> bool {
    match SHOWN_AS.lock().expect("réglages de l'image").as_ref() {
        Some(shown) => *shown != told_once(preferred),
        None => false,
    }
}

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

    // A session opens on the far computer's main screen, whatever screen
    // some earlier session was watching on some other machine. The choice
    // names one screen of one particular computer, so carrying it over
    // would be asking this one for a screen that is not its own.
    ask_for_the_far_screen(None);
    FAR_SCREENS.lock().expect("écrans d'en face").clear();

    let preferred = crate::settings::preferred().await;
    // The window takes the screen before anything else does, so the
    // opening is read on the same surface the picture will land on
    // rather than in a small window that grows under the eye.
    let _ = crate::picture::take_the_screen_for_a_session(
        &app,
        preferred.display_mode == zyr_proto::session::DisplayMode::Fullscreen,
    );

    let (settings, far_magnification) = what_to_ask_for(&app, preferred);
    let wanted = Wanted {
        host: host.trim().to_string(),
        peer: Some(peer),
        settings,
        pair_again: false,
        hush_the_far_speakers: preferred.mute_far_speakers,
        steady_far_rate: preferred.steady_far_rate,
        wants_a_screen_over_there: preferred.asked.wants_a_screen_over_there(),
        far_magnification,
        far_screen: None,
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
///
/// Two things come out of the one measurement, because they are one
/// measurement: what to ask the engine for, and how large the far screen
/// is asked to draw. Measuring twice would let the two disagree about the
/// screen they describe.
fn what_to_ask_for(app: &AppHandle, preferred: Preferred) -> (SessionSettings, u32) {
    let screen = crate::picture::the_screen_of_this_computer(app);
    let settings = preferred.settings(screen);
    crate::picture::tell_what_is_asked_for(screen, preferred.asked, &settings);
    (settings, preferred.asked.magnification(screen))
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
        let mut opening = Opening::begins();
        let running = match zyr_session::open(
            &wanted,
            &mut |step| {
                crate::journal::note(&written(&step));
                opening.reached(&step);
                // The floating button hangs on that process, and this window
                // is the only one that knows its number until the service
                // does; the way travels with it, so the session can be ended
                // during the seconds the service does not believe in it yet.
                if let Step::Showing { process, at } = &step {
                    crate::floating::expect(app, *process, &towards, at);
                    lay_the_picture_as_soon_as_it_opens(app.clone(), *process);
                }
                if let Some((detail, code)) = told(step) {
                    crate::accueil::etape(app, &detail, code);
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
        crate::journal::note(&opening.how_long_it_took());
        crate::accueil::range_l_ouverture(app);

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
            crate::accueil::relance(app);
            // What is kept when the service cannot be asked is what the
            // picture was already showing, never the ordinary settings:
            // the person asked for one thing to change, not for three
            // others to go back to what the product does by default.
            preferred = tauri::async_runtime::block_on(crate::settings::what_was_chosen())
                .unwrap_or(preferred);
            (wanted.settings, wanted.far_magnification) = what_to_ask_for(app, preferred);
            // The way is opened again with the picture, and what the far
            // computer was asked went with the old one: it has to be
            // asked afresh, and with what is chosen now.
            wanted.hush_the_far_speakers = preferred.mute_far_speakers;
            wanted.steady_far_rate = preferred.steady_far_rate;
            // And whether that computer is to grow a screen for this
            // session at all, which is the one thing the resolution
            // decides over there. Left as the first opening set it, a
            // session moved to « the host's own resolution » went on
            // waking a virtual screen on the far machine and asking it
            // for a size, which is exactly what that choice exists not
            // to do.
            wanted.wants_a_screen_over_there = preferred.asked.wants_a_screen_over_there();
            // And which of that computer's screens to be served from,
            // which is the whole reason a person opens the picture again
            // on a machine with two of them.
            wanted.far_screen = the_far_screen();
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
fn told(step: Step) -> Option<(String, Option<String>)> {
    Some(match step {
        Step::Reached { packet } => (format!("Tunnel établi, paquets de {packet} octets."), None),
        Step::Pairing { again: false } => (
            "Premier accès à cet ordinateur : les deux font connaissance. Rien à faire."
                .to_string(),
            None,
        ),
        Step::Pairing { again: true } => (
            "Cet ordinateur ne nous reconnaît plus : les deux font connaissance à nouveau. Rien \
             à faire."
                .to_string(),
            None,
        ),
        Step::PairingNeeded { pin } => (
            "Tapez ce code sur l'ordinateur que vous voulez contrôler :".to_string(),
            Some(pin),
        ),
        Step::Paired => ("Les deux ordinateurs se connaissent.".to_string(), None),
        Step::Starting => ("Démarrage de l'image…".to_string(), None),
        Step::Showing { .. } => ("L'image arrive…".to_string(), None),
        // The one of these the person is left waiting through, so it is
        // the one that goes on the opening screen: the far computer is
        // starting its engine over, and that is several seconds during
        // which nothing else would say anything at all.
        Step::FarScreenChanging => (
            "L'ordinateur distant change d'écran, il redémarre…".to_string(),
            None,
        ),
        Step::SpeakersLeftAlone { .. }
        | Step::RateLeftAlone { .. }
        | Step::ScreenLeftAlone { .. }
        | Step::FarScreenLeftAlone { .. }
        | Step::ScreenOverThere { .. } => return None,
    })
}

/// How long an opening took, in its parts.
///
/// One line at the end rather than four timestamps to subtract by hand.
/// Opening a session is the wait a person actually feels, it is made of
/// four very different things, and only one of them is ours to shorten:
/// guessing which one has already cost an evening, twice. Written where
/// the timestamps in this journal cannot answer, since they are cut to
/// the second and every part of this is smaller than that.
struct Opening {
    asked: std::time::Instant,
    reached: Option<std::time::Duration>,
    starting: Option<std::time::Duration>,
    showing: Option<std::time::Duration>,
}

impl Opening {
    fn begins() -> Self {
        Self {
            asked: std::time::Instant::now(),
            reached: None,
            starting: None,
            showing: None,
        }
    }

    /// Notes when a step was reached, for the three that mark a boundary.
    ///
    /// The questions put to the far computer say nothing when they are
    /// answered, only when they are refused, so they cannot be timed one
    /// by one from here. They all sit between the tunnel standing and the
    /// player starting, and that is how they are counted: together.
    fn reached(&mut self, step: &Step) {
        let so_far = self.asked.elapsed();
        match step {
            Step::Reached { .. } => self.reached = Some(so_far),
            Step::Starting => self.starting = Some(so_far),
            Step::Showing { .. } => self.showing = Some(so_far),
            _ => {}
        }
    }

    fn how_long_it_took(&self) -> String {
        let whole = self.asked.elapsed();
        let since =
            |from: Option<std::time::Duration>, to: Option<std::time::Duration>| match (from, to) {
                (Some(from), Some(to)) => format!("{} ms", to.saturating_sub(from).as_millis()),
                _ => "non mesuré".to_string(),
            };
        format!(
            "l'image est là après {} ms : {} pour joindre l'ordinateur distant, {} à lui demander \
             ce qu'il faut, {} à lancer le lecteur, {} avant sa première image",
            whole.as_millis(),
            since(Some(std::time::Duration::ZERO), self.reached),
            since(self.reached, self.starting),
            since(self.starting, self.showing),
            since(self.showing, Some(whole)),
        )
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
        Step::SpeakersLeftAlone { refused } => {
            format!("les enceintes de l'ordinateur distant restent allumées : {refused}")
        }
        Step::RateLeftAlone { refused } => {
            format!("l'ordinateur distant garde sa cadence d'écran immobile : {refused}")
        }
        Step::ScreenLeftAlone { refused } => {
            format!("l'ordinateur distant n'a pas réveillé son écran virtuel : {refused}")
        }
        Step::ScreenOverThere { wide, high } => format!(
            "l'ordinateur distant affiche {wide}x{high}, c'est ce qui est demandé au lecteur"
        ),
        Step::FarScreenChanging => {
            "l'ordinateur distant change d'écran, son moteur redémarre et la voie sera rouverte"
                .to_string()
        }
        Step::FarScreenLeftAlone { refused } => {
            format!("l'ordinateur distant garde l'écran qu'il filme : {refused}")
        }
    }
}

/// What state the home window is in, in words.
///
/// Written on both sides of the ending. A session that finishes must
/// leave that window exactly as it found it, and « sometimes it ends up
/// minimised » is the kind of report that cannot be chased without
/// knowing which of the two sides it was already on.
fn how_the_window_stands(_app: &AppHandle, when: &str) {
    if crate::fenetre::sienne() == 0 {
        crate::journal::note(&format!("{when} : plus de fenêtre d'accueil"));
        return;
    }
    fn say(what: bool) -> &'static str {
        if what { "oui" } else { "non" }
    }
    crate::journal::note(&format!(
        "{when} : accueil à l'écran={} plein écran={}",
        say(crate::fenetre::a_l_ecran()),
        say(crate::fenetre::tient_l_ecran()),
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
    if ok {
        crate::accueil::range_l_ouverture(app);
    } else {
        crate::accueil::echoue(app, &message);
    }
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
