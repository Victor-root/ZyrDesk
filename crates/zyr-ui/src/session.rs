//! Sessions, from the interface: opening one, playing it, and finding
//! those already running.
//!
//! The whole opening sequence lives in `zyr-session`, shared with the
//! command line: it opens the way and asks the far computer what the
//! session wants of it. What is here is the shape it takes in a window.
//! It runs away from the interface thread, and what happens on the way is
//! sent back as events rather than waited for, because opening a way to
//! another computer takes seconds, and the window keeps drawing all the
//! while. Then this window plays the session itself: its player draws
//! into a window of ours, and this thread follows what it says until the
//! session ends.
//!
//! Asking the service what it holds is what lets the rest of the window
//! name the session, and reach the far computer through its way.

// Changing the far computer's screen and finding out which way a
// session travels are only asked for from the floating button's menu,
// which only exists on Windows, like the session itself.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use crate::app::App;
use zyr_control::{Answer, Request};
use zyr_player::{Ending, Event, Measures, Player, Surface};
use zyr_proto::session::{FarScreen, Preferred, SessionSettings, WantedScreen};
use zyr_session::{Opened, Step, Wanted};

use crate::floating::Floating;
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
    /// The program playing it.
    pub process: u32,
    /// The link its player is connected to.
    pub at: String,
    /// The real address the packets go to right now, and how long that
    /// road takes to come back, in milliseconds.
    pub via: String,
    pub round_trip_ms: u64,
    /// The way the service holds towards that computer.
    ///
    /// Carried because some things a session asks travel on the
    /// product's own channel rather than through the player, and that
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
/// Asked of the service rather than remembered here: the way belongs to
/// the service, which lists it once the session's first picture is up.
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
/// a screen is not a guard: reloaded, it forgets, and two players drawing
/// into the same window cannot both be driven. This is the guard.
static OPENING: AtomicBool = AtomicBool::new(false);

/// Whether a session is being opened or played right now.
pub fn opening() -> bool {
    OPENING.load(Ordering::Relaxed)
}

/// What the thread driving a session hears while its picture plays: what
/// its player says, and the person closing the session.
enum Heard {
    Player(Event),
    Close,
}

/// The session this window plays.
struct Playing {
    player: Player,
    /// What the player was last asked for, in the session's own terms:
    /// every change made while it plays starts from here, and the player
    /// is told the whole of it each time.
    settings: SessionSettings,
    /// Whether the far computer sends a still screen again at the full
    /// rate.
    steady: bool,
    /// Where the person closing the session is told to whoever drives it.
    drive: Sender<Heard>,
}

static PLAYING: Mutex<Option<Playing>> = Mutex::new(None);

/// The player of the session this window plays, if one plays.
pub fn player() -> Option<Player> {
    PLAYING
        .lock()
        .expect("session jouée")
        .as_ref()
        .map(|playing| playing.player.clone())
}

/// What the session on screen costs right now, or nothing measured.
pub fn measures() -> Measures {
    player().map(|player| player.measures()).unwrap_or_default()
}

/// Asks the player for something else, where the session stands: a size,
/// a rate, a codec, a cadence, or the far pointer drawn or not.
///
/// Nothing at all when no session plays: the next one opens with what
/// was written down, and that is the whole of what a choice made outside
/// a session means.
pub fn ask_the_player(change: impl FnOnce(&mut SessionSettings, &mut bool)) {
    let (wanted, codec) = {
        let mut playing = PLAYING.lock().expect("session jouée");
        let Some(playing) = playing.as_mut() else {
            return;
        };
        change(&mut playing.settings, &mut playing.steady);
        let wanted = zyr_session::player_wants(&playing.settings, playing.steady);
        playing.player.change(wanted);
        (wanted, playing.settings.codec)
    };
    note(&format!(
        "le lecteur demande maintenant {}x{} à {} images/s, {} Mb/s en {codec}, curseur \
         dessiné là-bas : {}, écran immobile renvoyé : {}",
        wanted.width,
        wanted.height,
        wanted.fps,
        wanted.bitrate_kbps / 1000,
        if wanted.draw_pointer { "oui" } else { "non" },
        if wanted.steady { "oui" } else { "non" },
    ));
}

/// Closes the session playing in this window, and says whether one was
/// playing.
///
/// Told to the thread that drives it, which asks the player to say
/// goodbye and waits for it, a moment and no more.
pub fn close() -> bool {
    PLAYING
        .lock()
        .expect("session jouée")
        .as_ref()
        .is_some_and(|playing| playing.drive.send(Heard::Close).is_ok())
}

/// The codecs the far computer's engine said it can make, once it has
/// said.
pub fn what_the_far_computer_encodes() -> Option<zyr_player::CodecSet> {
    player().and_then(|player| player.encodable())
}

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
/// ways, so picking the main screen by hand writes nothing here.
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
/// What the menu marks, and what a picture that comes back asks for.
/// Moved only once that computer has answered: a mark on a screen nobody
/// is filming would be the one thing in this menu that lies.
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

/// One of the settings a session takes where it stands, once it has been
/// written down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changed {
    /// How much picture is asked for.
    Size,
    /// How much rate carries it.
    Rate,
    Codec,
    /// Whether the far computer resends a still screen at full rate.
    SteadyFarRate,
}

/// Gives the session in progress what was just chosen, where it stands.
///
/// Every one of them goes to the player, which asks the far engine for it
/// on the stream it already has: the rate is taken by the encoder where it
/// is when it can, a size, a codec or a cadence make the encoder and the
/// decoder over again in the same window. Nothing is ever opened again.
///
/// The size is asked of the far computer first, as at the opening: the
/// screen of that size over there, or its own, and what it answers it will
/// be showing is what the player asks for.
pub async fn take_where_it_stands(
    app: App,
    changed: Changed,
    preferred: Preferred,
) -> Result<(), String> {
    if player().is_none() {
        return Ok(());
    }
    match changed {
        Changed::Rate => ask_the_player(|settings, _| {
            settings.bitrate_kbps = preferred.bitrate_kbps;
        }),
        Changed::Codec => ask_the_player(|settings, _| settings.codec = preferred.codec),
        Changed::SteadyFarRate => ask_the_player(|_, steady| *steady = preferred.steady_far_rate),
        Changed::Size => return become_that_size(&app, preferred).await,
    }
    Ok(())
}

/// Gives the session the size chosen now: the far computer's screen
/// first, then the player. Our own window takes the new shape when the
/// player says the new stream has it.
async fn become_that_size(app: &App, preferred: Preferred) -> Result<(), String> {
    let Some(way) = the_way_in_use().await else {
        return Ok(());
    };
    let (guessed, magnification) = what_to_ask_for(app, preferred);
    let wanted = preferred
        .asked
        .wants_a_screen_over_there()
        .then_some(WantedScreen {
            wide: guessed.width,
            high: guessed.height,
            scale: magnification,
        });
    // What that computer says it will be showing wins over what this end
    // guessed, exactly as at the opening. A refusal costs the sharpness of
    // the picture and nothing else, and the session goes on.
    let (width, height) = match crate::service::ask(&Request::FarScreen { way, wanted }).await? {
        Answer::Showing {
            size: Some((wide, high)),
        } => (wide, high),
        Answer::Showing { size: None } => (guessed.width, guessed.height),
        Answer::Refused(reason) => {
            note(&format!(
                "l'ordinateur distant n'a pas préparé son écran : {reason}"
            ));
            (guessed.width, guessed.height)
        }
        other => return Err(crate::service::unexpected(other)),
    };
    ask_the_player(|settings, _| {
        settings.width = width;
        settings.height = height;
    });
    Ok(())
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
    // here rather than trusted: the opening asks it at every step, and a
    // « closed » left standing by whatever came before would let this one
    // go before its first question.
    Floating::was_closed_on_purpose(&app);

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
    // opening blocks for as long as the far computer takes to answer, and
    // the session is followed from there until it ends.
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
/// measurement: what to ask the far computer for, and how large its
/// screen is asked to draw. Measuring twice would let the two disagree
/// about the screen they describe.
fn what_to_ask_for(app: &App, preferred: Preferred) -> (SessionSettings, u32) {
    let screen = crate::picture::the_screen_of_this_computer(app);
    let settings = preferred.settings(screen);
    crate::picture::tell_what_is_asked_for(screen, preferred.asked, &settings);
    (settings, preferred.asked.magnification(screen))
}

/// How many times in a row the picture is brought back before the person
/// is told instead.
///
/// A session that falls over, comes back and falls over again within the
/// minute is not a network that hiccups: it is one that cannot carry a
/// session at all just now, and bringing the picture back forever would
/// hide that behind a screen that never settles.
const COMES_BACK_IN_A_ROW: u32 = 5;

/// How many of those may fail to open at all, one after the other.
///
/// Its own count, and a much shorter one, because the two failures cost
/// wildly different amounts of time. A picture that came back and fell
/// over again was answered in seconds; an opening that finds nobody
/// takes fifteen seconds twice over, the service asking a second time on
/// its own (D171). The far computer being off is exactly what this looks
/// like, and telling the person that after a minute is honest where
/// telling them after three would be a product that hangs.
const OPENINGS_MISSED_IN_A_ROW: u32 = 2;

/// A session that stood this long before falling over is a fresh
/// accident and not the same one over again, so the count starts over.
const HELD_LONG_ENOUGH: Duration = Duration::from_secs(60);

/// The pause before the picture is asked for again.
///
/// The far computer has its own tidying to do once its client vanishes:
/// it puts the desk back the way it found it, and lets go of the engine
/// that served the session. It also learns that the client is gone by its
/// own patience running out, which can leave it half a minute behind this
/// end. Waiting a moment costs the person nothing they can feel and
/// spares one try landing on a computer that is still holding the session
/// that just fell over.
const BEFORE_COMING_BACK: Duration = Duration::from_secs(3);

/// How often the pause looks up to see whether it is still wanted.
const WHILE_WAITING: Duration = Duration::from_millis(50);

/// How long the player is given to say goodbye once the session is closed.
///
/// It says it to the service on this computer, which answers at once
/// whatever the far computer is doing: this only bounds a player that
/// has stopped answering, so that a closed session is back home within
/// the time it takes to notice, and never later.
const CLOSING_SHOWS: Duration = Duration::from_secs(3);

/// The picture brought back after a session fell over on its own.
///
/// A road between two homes goes quiet for a few seconds now and then,
/// and thirty seconds of it end a session by design (D138). What that
/// would cost is the whole session: the picture gone, the person back on
/// the home screen with something to click and a far computer to wait
/// for again. Making that half-minute unlosable is a race that cannot be
/// won: it takes one outage a little too long, or one unlucky cascade,
/// and the session is over.
///
/// This is the other answer, and it is the one the products people
/// compare this to give: falling over stops costing the session. The
/// picture comes back by the road the person walked by hand, and it comes
/// back on its own.
struct ComingBack {
    /// How many times in a row, with nothing between them long enough to
    /// call the next one a fresh accident.
    in_a_row: u32,
    /// How many of those did not manage to open, in a row.
    missed: u32,
}

impl ComingBack {
    fn none() -> Self {
        Self {
            in_a_row: 0,
            missed: 0,
        }
    }

    /// The picture is up again: whatever it took to get here is spent,
    /// and the next opening that fails is the first of its own row.
    fn opened(&mut self) {
        self.missed = 0;
    }

    /// Which try the person is being shown, counting both roads: a
    /// picture that fell over again and an opening that found nobody are
    /// one wait as they see it.
    fn try_number(&self) -> u32 {
        self.in_a_row + self.missed
    }

    /// Whether the picture is worth bringing back, the session having
    /// ended without anybody asking for it.
    ///
    /// Only what a session falling over looks like: the link lost, or the
    /// player saying it could not go on. A far computer that ended it
    /// cleanly has decided so, and that is not an accident to undo.
    fn after(&mut self, ended: &Ending, held: Duration) -> bool {
        if !matches!(ended, Ending::LinkLost | Ending::EngineFailed(_)) {
            return false;
        }
        if held >= HELD_LONG_ENOUGH {
            self.in_a_row = 0;
        }
        self.once_more()
    }

    /// The same, an opening having failed rather than a picture having
    /// fallen over.
    ///
    /// Only while the picture was already coming back: a session that
    /// never opened in the first place is the person's own try, answered
    /// where they can see it, and the service has already asked twice by
    /// then (D171).
    fn again(&mut self) -> bool {
        if !self.tried() || self.missed >= OPENINGS_MISSED_IN_A_ROW {
            return false;
        }
        self.missed += 1;
        true
    }

    fn once_more(&mut self) -> bool {
        if self.in_a_row >= COMES_BACK_IN_A_ROW {
            return false;
        }
        self.in_a_row += 1;
        true
    }

    /// Whether the picture has been brought back at all.
    fn tried(&self) -> bool {
        self.in_a_row > 0
    }

    fn how_it_went(&self) -> String {
        format!(
            "l'image a été reprise {} fois de suite sans que la session tienne",
            self.in_a_row
        )
    }
}

/// Says what is happening, and waits out the moment the far computer
/// needs.
///
/// Answers whether to go on: a person who closes the window during the
/// pause is heard at once, and there is nothing left to come back to.
///
/// What is deliberately not put down here is the screen and the window.
/// It is one session as the person sees it, and handing the screen back
/// to take it again a second later is exactly the flicker this road
/// exists to spare them.
fn hold_on_before_coming_back(app: &App, coming_back: &ComingBack) -> bool {
    crate::home::coming_back(app, coming_back.try_number());
    waited_out(app, BEFORE_COMING_BACK)
}

/// Waits that long, unless the person closes the window meanwhile.
///
/// Answers whether the wait ran its course. Looked up from rather than
/// slept through: a cross pressed during it would otherwise be answered
/// three seconds later, by a picture coming back that nobody wants.
fn waited_out(app: &App, how_long: Duration) -> bool {
    let until = Instant::now() + how_long;
    while Instant::now() < until {
        if Floating::a_close_was_asked_for(app) {
            return false;
        }
        std::thread::sleep(WHILE_WAITING);
    }
    true
}

/// What the far computer is asked for when the way is opened again.
///
/// Everything it was told went with the old way, so all of it is asked
/// afresh, and with what is chosen now rather than what was chosen when
/// the session opened.
fn asked_afresh(app: &App, wanted: &mut Wanted, preferred: &mut Preferred) {
    // What is kept when the service cannot be asked is what the picture
    // was already showing, never the ordinary settings: the person asked
    // for nothing to change.
    *preferred = crate::app::block_on(crate::settings::what_was_chosen()).unwrap_or(*preferred);
    (wanted.settings, wanted.far_magnification) = what_to_ask_for(app, *preferred);
    wanted.hush_the_far_speakers = preferred.mute_far_speakers;
    // And whether that computer is to grow a screen for this session at
    // all, which is the one thing the resolution decides over there.
    wanted.wants_a_screen_over_there = preferred.asked.wants_a_screen_over_there();
    // And which of that computer's screens to be served from, which is
    // what a picture coming back has to land on again.
    wanted.far_screen = the_far_screen();
}

/// Opens the session and plays it, from the first tunnel to the last
/// picture.
///
/// It opens more than once when the session falls over on its own, which
/// is what a road that goes quiet for too long comes to: the picture is
/// brought back the same way, and `ComingBack` says how long that is worth
/// trying.
fn drive(app: &App, mut wanted: Wanted, mut preferred: Preferred) {
    note(&format!("session demandée vers {}", wanted.host));
    let mut coming_back = ComingBack::none();
    loop {
        let mut opening = Opening::begins();
        let started = match zyr_session::open(
            &wanted,
            &mut |step| {
                note(&written(&step));
                opening.reached(&step);
                if let Some(detail) = told(step) {
                    crate::home::step(app, &detail);
                }
            },
            &|| !Floating::a_close_was_asked_for(app),
        ) {
            Ok(opened) => {
                opening.opened();
                Showing::start(app, opened, &preferred)
            }
            // Let go of by the person, from the cross of the window or
            // the shortcut: there is nothing to show them about it, they
            // are the one who asked. The flag is taken here rather than
            // left standing, since no session will end to take it.
            Err(zyr_session::Error::Abandoned) => {
                Floating::was_closed_on_purpose(app);
                note("ouverture abandonnée : la session a été fermée avant l'image");
                return finish(app, true, String::new());
            }
            Err(e) => Err(e.to_string()),
        };
        let showing = match started {
            Ok(showing) => showing,
            Err(reason) => match failed_to_open(app, reason, &mut coming_back) {
                Then::Again => {
                    asked_afresh(app, &mut wanted, &mut preferred);
                    continue;
                }
                Then::Over => return,
            },
        };

        let (ended, shown) = showing.until_it_ends(app, &mut opening, &mut coming_back);

        let on_purpose = Floating::was_closed_on_purpose(app);
        note(&if on_purpose {
            format!("session fermée volontairement, le lecteur a dit {ended:?}")
        } else {
            format!("session terminée : {ended:?}")
        });
        if on_purpose {
            return finish(app, true, String::new());
        }

        // A player that ended before its first picture is an opening that
        // failed, and is answered as one: it did not fall over, it never
        // stood.
        let Some(held) = shown else {
            let Some(reason) = before_the_picture(&ended) else {
                return finish(app, true, String::new());
            };
            match failed_to_open(app, reason, &mut coming_back) {
                Then::Again => {
                    asked_afresh(app, &mut wanted, &mut preferred);
                    continue;
                }
                Then::Over => return,
            }
        };

        // Nobody asked for this one, so the picture comes back rather
        // than the session ending under the person. The window keeps the
        // screen and this thread keeps everything it knows, so what they
        // see is a picture that goes and returns.
        if coming_back.after(&ended, held) {
            note(&format!(
                "la session est tombée toute seule, l'image est reprise ({} sur {})",
                coming_back.in_a_row, COMES_BACK_IN_A_ROW
            ));
            if !hold_on_before_coming_back(app, &coming_back) {
                return closed_during_the_pause(app);
            }
            asked_afresh(app, &mut wanted, &mut preferred);
            continue;
        }

        return match ended {
            Ending::Asked | Ending::HostLeft => finish(app, true, String::new()),
            _ if coming_back.tried() => finish(
                app,
                false,
                format!(
                    "La session n'a pas tenu : {}.\n  \
                     Le réseau entre les deux ordinateurs ne la porte pas en ce moment.",
                    coming_back.how_it_went()
                ),
            ),
            Ending::LinkLost => finish(
                app,
                false,
                "La connexion avec l'ordinateur distant a été perdue.".into(),
            ),
            Ending::EngineFailed(reason) => finish(app, false, reason),
        };
    }
}

/// What comes after an opening that failed.
enum Then {
    /// The picture is on its way back: the way is asked for again.
    Again,
    /// The session is over, and the home screen says why.
    Over,
}

/// Answers an opening that failed.
///
/// A picture on its way back that did not open is one of its tries and
/// not the end of them: the far computer can still be holding the session
/// that just fell over, which it learns of by its own patience running
/// out. Any other opening that fails is told where the person sees it.
fn failed_to_open(app: &App, reason: String, coming_back: &mut ComingBack) -> Then {
    note(&format!(
        "session non ouverte : {}",
        reason.replace('\n', " ")
    ));
    if coming_back.again() {
        if !hold_on_before_coming_back(app, coming_back) {
            closed_during_the_pause(app);
            return Then::Over;
        }
        return Then::Again;
    }
    if coming_back.tried() {
        finish(
            app,
            false,
            format!("{reason}\n  {}", coming_back.how_it_went()),
        );
    } else {
        finish(app, false, reason);
    }
    Then::Over
}

/// What a session that ended before its first picture has to say, or
/// nothing when it was closed from here.
fn before_the_picture(ended: &Ending) -> Option<String> {
    match ended {
        Ending::Asked => None,
        Ending::HostLeft => {
            Some("L'ordinateur distant a fermé la session avant la première image.".to_string())
        }
        Ending::LinkLost => Some(
            "La connexion avec l'ordinateur distant a été perdue avant la première image."
                .to_string(),
        ),
        Ending::EngineFailed(reason) => Some(reason.clone()),
    }
}

/// A picture playing in this window: the way it travels on, its player,
/// and what the player says.
struct Showing {
    player: Player,
    heard: Receiver<Heard>,
    /// The way, given back to the service when this is let go of.
    way: zyr_session::Driving,
}

impl Showing {
    /// Makes the picture's window and starts the player in it.
    fn start(app: &App, opened: Opened, preferred: &Preferred) -> Result<Showing, String> {
        let Opened {
            link,
            settings,
            way,
        } = opened;
        let log = crate::journal::the_log()
            .ok_or("le journal de la fenêtre ne s'ouvre pas, le lecteur n'a pas où écrire")?;
        let window = crate::video::open(app)?;
        let (said, heard) = channel();
        let told = said.clone();
        let steady = preferred.steady_far_rate;
        let started = Player::start(
            &link,
            zyr_session::player_wants(&settings, steady),
            Surface::Window { hwnd: window },
            log,
            Box::new(move |event| {
                let _ = told.send(Heard::Player(event));
            }),
        );
        let player = match started {
            Ok(player) => player,
            Err(e) => {
                crate::video::close(app);
                return Err(e.to_string());
            }
        };
        note(&format!("lecteur branché sur {link}"));
        // The switches of this window start where the settings put them,
        // and the mouse where the session was opened with it.
        crate::floating::adopt(app, preferred);
        crate::system_keys::start_as(preferred.system_keys);
        crate::video::play_a_game(app, !settings.absolute_mouse);
        *PLAYING.lock().expect("session jouée") = Some(Playing {
            player: player.clone(),
            settings,
            steady,
            drive: said,
        });
        // Closed while the player was starting: it says goodbye at once.
        if Floating::a_close_was_asked_for(app) {
            close();
        }
        Ok(Showing { player, heard, way })
    }

    /// Follows what the player says until the session ends, and says how
    /// it ended and how long its picture stood, if it ever showed one.
    fn until_it_ends(
        self,
        app: &App,
        opening: &mut Opening,
        coming_back: &mut ComingBack,
    ) -> (Ending, Option<Duration>) {
        let Showing {
            player,
            heard,
            mut way,
        } = self;
        let mut shown_at: Option<Instant> = None;
        let mut closing: Option<Instant> = None;
        let ending = loop {
            let next = match closing {
                None => heard.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(until) => heard.recv_timeout(until.saturating_duration_since(Instant::now())),
            };
            let said = match next {
                Ok(Heard::Player(said)) => said,
                Ok(Heard::Close) => {
                    if closing.is_none() {
                        player.stop();
                        closing = Some(Instant::now() + CLOSING_SHOWS);
                    }
                    continue;
                }
                // The player did not say goodbye in time: the session is
                // over on this side all the same.
                Err(RecvTimeoutError::Timeout) => {
                    note(&format!(
                        "le lecteur ne s'est pas arrêté en {} s, la session est fermée ici",
                        CLOSING_SHOWS.as_secs()
                    ));
                    break Ending::Asked;
                }
                Err(RecvTimeoutError::Disconnected) => break Ending::Asked,
            };
            match said {
                Event::Streaming {
                    codec,
                    width,
                    height,
                } => {
                    note(&format!("flux en {} {width}x{height}", codec.name()));
                    crate::picture::takes_the_shape(app, (width, height));
                }
                Event::FirstPicture => {
                    opening.shown();
                    shown_at = Some(Instant::now());
                    crate::video::show(app);
                    crate::floating::keep_up_with_the_picture(app);
                    crate::home::put_the_opening_away(app);
                    note(&opening.how_long_it_took());
                    if coming_back.tried() {
                        // Worth its own line, and worth reading tomorrow:
                        // this is the whole of what a session that used
                        // to die looks like now.
                        note(&format!(
                            "l'image est revenue après {} reprise(s), la session continue",
                            coming_back.try_number()
                        ));
                    }
                    coming_back.opened();
                    // From here on the way is a session the service names,
                    // and closes should this program go without a word.
                    if let Err(reason) = way.hold() {
                        note(&format!(
                            "le service ne compte pas cette session parmi les siennes : {reason}"
                        ));
                    }
                }
                Event::Notice(text) => {
                    note(&format!("le lecteur dit : {text}"));
                    // Before the picture, the opening screen is what the
                    // person is reading.
                    if shown_at.is_none() {
                        crate::home::step(app, &text);
                    }
                }
                Event::Ended(ending) => break ending,
            }
        };
        // The picture first, so that nothing hangs a button over it again
        // while the rest is put away.
        crate::video::close(app);
        *PLAYING.lock().expect("session jouée") = None;
        crate::system_keys::give_them_back();
        crate::floating::lower(app);
        crate::picture::shut_the_pointer_in(crate::picture::Cage::Free);
        drop(player);
        drop(way);
        (ending, shown_at.map(|at| at.elapsed()))
    }
}

/// Ends the session in progress, the person having closed the window on
/// it.
///
/// The same path the menu takes, and no second one.
pub fn end_it(app: &App) {
    let asked = app.clone();
    crate::app::spawn(async move {
        note("session terminée par la fenêtre");
        if let Err(reason) = crate::floating::ask(&asked, crate::floating::Act::End).await {
            note(&format!(
                "la session n'a pas pu être terminée : {}",
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
/// Opening a session is the wait a person actually feels, and only some
/// of it is ours to shorten: guessing which part has already cost an
/// evening, twice. Written where the timestamps in this journal cannot
/// answer, since they are cut to the second and every part of this is
/// smaller than that.
struct Opening {
    asked: Instant,
    reached: Option<Duration>,
    opened: Option<Duration>,
    shown: Option<Duration>,
}

impl Opening {
    fn begins() -> Self {
        Self {
            asked: Instant::now(),
            reached: None,
            opened: None,
            shown: None,
        }
    }

    /// Notes when the way stood.
    ///
    /// The questions put to the far computer afterwards say nothing when
    /// they are answered, only when they are refused, so they cannot be
    /// timed one by one from here. They all sit between the tunnel
    /// standing and the way handed over, and that is how they are
    /// counted: together.
    fn reached(&mut self, step: &Step) {
        if *step == Step::Reached {
            self.reached = Some(self.asked.elapsed());
        }
    }

    /// Notes when the way was handed over, everything asked.
    fn opened(&mut self) {
        self.opened = Some(self.asked.elapsed());
    }

    /// Notes when the first picture was on screen, which is the only
    /// moment the person is waiting for.
    fn shown(&mut self) {
        self.shown = Some(self.asked.elapsed());
    }

    fn how_long_it_took(&self) -> String {
        let since = |from: Option<Duration>, to: Option<Duration>| match (from, to) {
            (Some(from), Some(to)) => format!("{} ms", to.saturating_sub(from).as_millis()),
            _ => "non mesuré".to_string(),
        };
        format!(
            "image à l'écran {} après la demande : {} pour joindre l'ordinateur distant, {} à lui \
             demander ce qu'il faut, {} du lecteur jusqu'à la première image",
            since(Some(Duration::ZERO), self.shown),
            since(Some(Duration::ZERO), self.reached),
            since(self.reached, self.opened),
            since(self.opened, self.shown),
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
fn how_the_window_stands(when: &str) {
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

/// The person closed the window while the picture was on its way back.
///
/// The flag is taken here rather than left standing, since no session
/// will end to take it: nothing was playing when they pressed the cross.
fn closed_during_the_pause(app: &App) {
    Floating::was_closed_on_purpose(app);
    note("reprise abandonnée : la session a été fermée pendant l'attente");
    finish(app, true, String::new());
}

fn finish(app: &App, ok: bool, message: String) {
    how_the_window_stands("fin de session, avant");
    OPENING.store(false, Ordering::SeqCst);
    // Taken down here rather than left to the watch. The watch comes
    // round once a second, and until it does the button hangs over a
    // picture that has gone. Whoever drove the session knows it is over
    // the instant it is.
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
    how_the_window_stands("fin de session, après");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session that fell over the instant it opened.
    fn fell_over() -> Ending {
        Ending::LinkLost
    }

    #[test]
    fn a_picture_that_falls_over_comes_back_a_bounded_number_of_times() {
        let mut coming_back = ComingBack::none();
        assert!(!coming_back.tried());
        // It comes back, as long as the session does not stand long
        // enough in between for it to be a new accident.
        for attempt in 1..=COMES_BACK_IN_A_ROW {
            assert!(
                coming_back.after(&fell_over(), Duration::from_secs(2)),
                "reprise {attempt}"
            );
            assert_eq!(coming_back.in_a_row, attempt);
        }
        // Past that count, the person is told rather than left
        // watching a screen that never settles.
        assert!(!coming_back.after(&fell_over(), Duration::from_secs(2)));
        assert!(coming_back.tried());
    }

    #[test]
    fn a_session_that_stood_long_enough_starts_the_count_over() {
        let mut coming_back = ComingBack::none();
        for _ in 0..COMES_BACK_IN_A_ROW {
            assert!(coming_back.after(&fell_over(), Duration::from_secs(2)));
        }
        assert!(!coming_back.after(&fell_over(), Duration::from_secs(2)));
        // A session that stood for a minute and then falls over is a
        // new failure, and not the same one starting again.
        assert!(coming_back.after(&fell_over(), HELD_LONG_ENOUGH));
        assert_eq!(coming_back.in_a_row, 1);
    }

    #[test]
    fn a_session_the_far_computer_ended_is_not_brought_back() {
        let mut coming_back = ComingBack::none();
        // Hanging up is a decision of the far computer, not an accident
        // to undo, and a session closed here was closed on purpose.
        assert!(!coming_back.after(&Ending::HostLeft, Duration::from_secs(2)));
        assert!(!coming_back.after(&Ending::Asked, Duration::from_secs(2)));
        assert!(!coming_back.tried());
        // A player that could not go on, yes: from where the person
        // sits, it is the same thing as a session falling over.
        assert!(coming_back.after(
            &Ending::EngineFailed("la carte graphique a disparu".to_string()),
            Duration::from_secs(2)
        ));
    }

    #[test]
    fn an_opening_that_finds_nobody_is_only_retried_while_coming_back() {
        let mut coming_back = ComingBack::none();
        // The first attempt is the person's: they clicked, and the
        // failure is told where they see it.
        assert!(!coming_back.again());

        assert!(coming_back.after(&fell_over(), Duration::from_secs(2)));
        // A comeback that does not open is one of its tries, and they are
        // counted apart because they cost half a minute each.
        for attempt in 1..=OPENINGS_MISSED_IN_A_ROW {
            assert!(coming_back.again(), "essai manqué {attempt}");
        }
        assert!(!coming_back.again());

        // With the picture back, what it took to get there has been
        // spent.
        coming_back.opened();
        assert!(coming_back.again());
    }

    #[test]
    fn a_session_that_never_showed_a_picture_says_why_unless_closed_here() {
        // Closed from here is nothing to tell; everything else is.
        assert_eq!(before_the_picture(&Ending::Asked), None);
        for ended in [
            Ending::HostLeft,
            Ending::LinkLost,
            Ending::EngineFailed("aucun encodeur ne sait faire cette image".to_string()),
        ] {
            let said = before_the_picture(&ended).unwrap();
            assert!(!said.is_empty(), "{ended:?}");
        }
        assert_eq!(
            before_the_picture(&Ending::EngineFailed("la carte a disparu".to_string())),
            Some("la carte a disparu".to_string())
        );
    }

    #[test]
    fn an_opening_says_how_long_each_part_took() {
        let mut opening = Opening::begins();
        // Nothing reached: every part says it was not measured rather
        // than a figure of nought.
        assert!(opening.how_long_it_took().contains("non mesuré"));
        opening.reached(&Step::Reached);
        opening.opened();
        opening.shown();
        let said = opening.how_long_it_took();
        assert!(!said.contains("non mesuré"), "{said}");
        assert!(said.contains("jusqu'à la première image"), "{said}");
    }

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
