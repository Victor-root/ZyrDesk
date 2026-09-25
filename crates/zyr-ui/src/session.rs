//! Sessions, from the interface: opening one, and finding those already
//! running.
//!
//! The whole opening sequence lives in `zyr-session`, shared with the
//! command line. What is here is the shape it takes in a window: it runs
//! away from the interface thread, and what happens on the way is sent
//! back as events rather than waited for, because opening a way to
//! another computer takes seconds, and the window keeps drawing all the
//! while.
//!
//! A window is not where a session lives, though. Asking the service
//! what it holds is what lets a window opened afterwards, or reopened
//! after a crash, show the session instead of an empty home screen.

// Changing the far computer's screen and finding out which way a
// session travels are only asked for from the floating button's menu,
// which only exists on Windows, like the session itself.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::App;
use zyr_control::{Answer, Request};
use zyr_proto::session::{FarScreen, Preferred, SessionSettings};
use zyr_session::{Step, Wanted};

use crate::service;

/// What this module files its journal lines under.
const TAG: &str = "session";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

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
    /// The real address the packets go to right now, and how long that
    /// road takes to come back, in milliseconds.
    pub via: String,
    pub round_trip_ms: u64,
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
            via: session.via,
            round_trip_ms: session.round_trip_ms,
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

/// What a session this window is not driving is told.
///
/// Its player lives in another program, and nothing here can reach it.
const NOT_FROM_HERE: &str = "cette session n'a pas été ouverte depuis cette fenêtre.\n  \
                             Les réglages s'appliqueront à la prochaine.";

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

/// Writes down which of the far computer's screens is being watched.
///
/// What the menu marks, and what a picture opened again asks for. Moved
/// only once that computer has answered: a mark on a screen nobody is
/// filming would be the one thing in this menu that lies.
pub fn ask_for_the_far_screen(id: Option<String>) {
    *FAR_SCREEN.lock().expect("écran d'en face") = id;
}

/// Watches that screen of the far computer from now on, or its main one
/// when nothing is named.
///
/// Asked the moment it is picked and never held back: that computer's
/// engine changes the screen it films where it stands, which costs it a
/// reinitialization of its capture and costs this end nothing at all.
/// The picture is on the other screen within the second, and the session
/// never stops.
pub async fn watch_the_far_screen(id: Option<String>) -> Result<(), String> {
    let way = the_way_in_use()
        .await
        .ok_or("aucune session en cours".to_string())?;
    match crate::service::ask(&Request::FilmFarScreen {
        way,
        id: id.clone(),
    })
    .await?
    {
        Answer::Done => {}
        Answer::Refused(reason) => return Err(reason),
        other => return Err(crate::service::unexpected(other)),
    }
    ask_for_the_far_screen(id);
    note("écran de l'ordinateur distant changé sans rien relancer");
    Ok(())
}

/// Moves to the next of the far computer's screens, and round to the
/// first after the last.
///
/// What the shortcut does. A far computer with one screen has nothing to
/// move to, and that is not a failure: it is the ordinary machine, and
/// the key is simply quiet on it.
pub async fn watch_the_next_far_screen() -> Result<(), String> {
    // Asked of the far computer when this end has never asked: the list
    // is filled when the menu is opened, and a key that only worked
    // after somebody had opened the menu once would be a key that
    // sometimes does nothing.
    if the_far_screens().is_empty() {
        crate::settings::the_far_computers_screens().await;
    }
    let screens = the_far_screens();
    let Some(next) = the_one_after(&screens, &the_far_screen_named()) else {
        return Ok(());
    };
    // Its main screen is asked for by naming no screen at all, exactly
    // as the menu does: the two are one answer said two ways, and a
    // session that names it would be told « you have it » by a computer
    // that answers the same thing to nobody naming anything.
    watch_the_far_screen((!next.main).then(|| next.id.clone())).await
}

/// The screen after that one in the list, and round to the first after
/// the last.
///
/// Nothing at all when there is nowhere to go, which is a far computer
/// with one screen and a far computer that has not said.
fn the_one_after<'a>(screens: &'a [FarScreen], watched: &str) -> Option<&'a FarScreen> {
    if screens.len() < 2 {
        return None;
    }
    // A screen this list does not know starts the round at the first,
    // which is what a list that changed under a session leaves behind.
    let at = screens
        .iter()
        .position(|screen| screen.id == watched)
        .unwrap_or(0);
    screens.get((at + 1) % screens.len())
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

/// Gives the session in progress what was just chosen, where it stands.
///
/// Nothing at all when no session is on screen: the next one opens with
/// what was written down, and that is the whole of what a choice made
/// outside a session means. This window plays no session of its own, so
/// one in progress belongs to another program, and is told so.
pub async fn take_where_it_stands() -> Result<(), String> {
    match the_way_in_use().await {
        Some(_) => Err(NOT_FROM_HERE.to_string()),
        None => Ok(()),
    }
}

/// Opens a session towards that computer.
///
/// `only_here` keeps it on this local network: the address and nothing
/// else, with nothing asked of any server. Decided on the home screen,
/// where the two ways of reaching a computer are two things to click.
pub async fn connect(
    app: App,
    host: String,
    fingerprint: String,
    only_here: bool,
) -> Result<(), String> {
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

    // Nobody has closed a session that has not begun. Read and put down
    // here rather than trusted: the opening asks it at every step now,
    // and a « closed » left standing by whatever came before would let
    // this one go before its first question.
    crate::floating::Floating::was_closed_on_purpose(&app);

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
        peer,
        settings,
        hush_the_far_speakers: preferred.mute_far_speakers,
        wants_a_screen_over_there: preferred.asked.wants_a_screen_over_there(),
        far_magnification,
        far_screen: None,
        only_here,
    };

    // On a thread of its own, and not one of the interface's: the
    // opening blocks for as long as the far computer takes to answer.
    std::thread::spawn(move || drive(&app, wanted));
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
fn what_to_ask_for(app: &App, preferred: Preferred) -> (SessionSettings, u32) {
    let screen = crate::picture::the_screen_of_this_computer(app);
    let settings = preferred.settings(screen);
    crate::picture::tell_what_is_asked_for(screen, preferred.asked, &settings);
    (settings, preferred.asked.magnification(screen))
}

/// What the opening says once the way stands: the picture it opened for
/// has nowhere to go yet.
const NOT_WIRED: &str = "le nouveau moteur n'est pas encore branché sur la fenêtre";

/// Opens the way to the session, and says what came of it.
///
/// The picture itself is not played here: the way is given back as soon
/// as it stands, and the person is told why there is nothing to watch.
fn drive(app: &App, wanted: Wanted) {
    note(&format!("session demandée vers {}", wanted.host));
    let mut opening = Opening::begins();
    let opened = match zyr_session::open(
        &wanted,
        &mut |step| {
            note(&written(&step));
            opening.reached(&step);
            if let Some(detail) = told(step) {
                crate::home::step(app, &detail);
            }
        },
        &|| !crate::floating::Floating::a_close_was_asked_for(app),
    ) {
        Ok(opened) => opened,
        // Let go of by the person, from the cross of the window or the
        // shortcut: there is nothing to show them about it, they are the
        // one who asked. The flag is taken here rather than left
        // standing, since no session will end to take it.
        Err(zyr_session::Error::Abandoned) => {
            crate::floating::Floating::was_closed_on_purpose(app);
            note("ouverture abandonnée : la session a été fermée avant l'image");
            return finish(app, true, String::new());
        }
        Err(e) => {
            note(&format!(
                "session non ouverte : {}",
                e.to_string().replace('\n', " ")
            ));
            return finish(app, false, e.to_string());
        }
    };
    note(&opening.how_long_it_took());
    note(&format!(
        "voie ouverte vers {}, lien {} ; {NOT_WIRED}, la voie est rendue",
        wanted.host, opened.link
    ));
    drop(opened);
    finish(app, false, NOT_WIRED.to_string())
}

/// Ends the session in progress, the person having closed the window on
/// it.
///
/// The same path the menu takes, and no second one.
pub fn end_it(app: &App) {
    let asked = app.clone();
    crate::app::spawn(async move {
        note("session terminée par la croix de la fenêtre");
        if let Err(reason) = crate::floating::ask(&asked, crate::floating::Act::End).await {
            note(&format!(
                "la croix n'a pas pu terminer la session : {}",
                reason.replace('\n', " ")
            ));
        }
    });
}

/// The same moment, as the opening screen shows it, when it shows it at
/// all.
///
/// Not every step is worth a screen. A far computer that would not
/// silence its own speakers has nothing to do with what the person is
/// waiting for, and putting it there would replace « the picture is
/// coming » with a sentence about sound.
fn told(step: Step) -> Option<String> {
    Some(match step {
        Step::Reached => "Tunnel établi, l'ordinateur distant se prépare…".to_string(),
        Step::NoSoundCardHere => {
            "Cet ordinateur n'a pas de sortie audio : la session sera muette.".to_string()
        }
        Step::SpeakersLeftAlone { .. }
        | Step::ScreenLeftAlone { .. }
        | Step::FarScreenLeftAlone { .. }
        | Step::ScreenOverThere { .. } => return None,
    })
}

/// How long an opening took, in its parts.
///
/// One line at the end rather than timestamps to subtract by hand.
/// Opening a session is the wait a person actually feels, and only one
/// part of it is ours to shorten: guessing which one has already cost an
/// evening, twice. Written where the timestamps in this journal cannot
/// answer, since they are cut to the second and every part of this is
/// smaller than that.
struct Opening {
    asked: std::time::Instant,
    reached: Option<std::time::Duration>,
}

impl Opening {
    fn begins() -> Self {
        Self {
            asked: std::time::Instant::now(),
            reached: None,
        }
    }

    /// Notes when the way stood.
    ///
    /// The questions put to the far computer afterwards say nothing when
    /// they are answered, only when they are refused, so they cannot be
    /// timed one by one from here. They all sit between the tunnel
    /// standing and the end of the opening, and that is how they are
    /// counted: together.
    fn reached(&mut self, step: &Step) {
        if *step == Step::Reached {
            self.reached = Some(self.asked.elapsed());
        }
    }

    fn how_long_it_took(&self) -> String {
        let whole = self.asked.elapsed();
        match self.reached {
            Some(reached) => format!(
                "ouverture faite en {} ms : {} ms pour joindre l'ordinateur distant, {} ms à lui \
                 demander ce qu'il faut",
                whole.as_millis(),
                reached.as_millis(),
                whole.saturating_sub(reached).as_millis(),
            ),
            None => format!("ouverture faite en {} ms", whole.as_millis()),
        }
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
        Step::Reached => "tunnel ouvert".to_string(),
        Step::NoSoundCardHere => "cet ordinateur n'a pas de sortie audio".to_string(),
        Step::SpeakersLeftAlone { refused } => {
            format!("les enceintes de l'ordinateur distant restent allumées : {refused}")
        }
        Step::ScreenLeftAlone { refused } => {
            format!("l'ordinateur distant n'a pas réveillé son écran virtuel : {refused}")
        }
        Step::ScreenOverThere { wide, high } => format!(
            "l'ordinateur distant affiche {wide}x{high}, c'est ce qui est demandé au lecteur"
        ),
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
fn how_the_window_stands(_app: &App, when: &str) {
    if crate::main_window::handle() == 0 {
        note(&format!("{when} : plus de fenêtre d'accueil"));
        return;
    }
    fn say(what: bool) -> &'static str {
        if what { "oui" } else { "non" }
    }
    note(&format!(
        "{when} : accueil à l'écran={} plein écran={}",
        say(crate::main_window::on_screen()),
        say(crate::main_window::holds_the_screen()),
    ));
}

fn finish(app: &App, ok: bool, message: String) {
    how_the_window_stands(app, "fin de session, avant");
    OPENING.store(false, Ordering::SeqCst);
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
        crate::home::put_the_opening_away(app);
    } else {
        crate::home::failed(app, &message);
    }
    how_the_window_stands(app, "fin de session, après");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn far_screen(id: &str, main: bool) -> FarScreen {
        FarScreen {
            id: id.to_string(),
            main,
            wide: 3840,
            high: 2160,
            name: id.to_string(),
        }
    }

    #[test]
    fn the_shortcut_goes_round_the_far_computers_screens() {
        let screens = [
            far_screen("un", true),
            far_screen("deux", false),
            far_screen("trois", false),
        ];
        for (watched, expected) in [("un", "deux"), ("deux", "trois"), ("trois", "un")] {
            assert_eq!(
                the_one_after(&screens, watched).map(|screen| screen.id.as_str()),
                Some(expected),
                "depuis « {watched} »"
            );
        }
        // A screen the list does not know is a list that has changed
        // under the session: the round starts again from the first.
        assert_eq!(
            the_one_after(&screens, "parti").map(|screen| screen.id.as_str()),
            Some("deux")
        );
        // And a computer with a single screen has nowhere to go, which
        // is not a fault: the key simply stays silent.
        assert!(the_one_after(&screens[..1], "un").is_none());
        assert!(the_one_after(&[], "").is_none());
    }
}
