//! Sessions, from the interface: opening one, playing it, and finding
//! those already running.
//!
//! The whole road lives in `zyr-session`: opening the way, shared with
//! the command line, and playing the session to its end, which brings the
//! picture back when the session falls over on its own. What is here is
//! the shape it takes in a window, the stage that road is played on. It
//! runs away from the interface thread, and what happens on the way is
//! sent back as events rather than waited for, because opening a way to
//! another computer takes seconds, and the window keeps drawing all the
//! while. Then this window plays the session itself: its player draws
//! into a window of ours, and that thread follows what it says until the
//! session ends.
//!
//! Asking the service what it holds is what lets the home screen name the
//! sessions of this computer. The session this window plays keeps its
//! own way, which everything asked of the far computer goes through.

// Changing the far computer's screen and finding out which way a
// session travels are only asked for from the floating button's menu,
// which only exists on Windows, like the session itself.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use crate::app::App;
use zyr_control::{Answer, Request, WayId};
use zyr_player::{Ending, Event, Measures, Player, Surface};
use zyr_proto::fact::Fact;
use zyr_proto::log::Log;
use zyr_proto::session::{FarScreen, Preferred, SessionSettings, WantedScreen};
use zyr_session::{Opened, Stage, Step, Wanted};

use crate::desk::fingerprint_of;
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
    /// The real address the packets go to right now, whether it is the
    /// server's relay, and how long that road takes to come back, in
    /// milliseconds.
    pub via: String,
    pub relayed: bool,
    pub round_trip_ms: u64,
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
            via: session.via,
            relayed: session.relayed,
            round_trip_ms: session.round_trip_ms,
        }),
        _ => None,
    })
    .await
    .unwrap_or_default()
}

/// The way the session this window plays travels on, or nothing.
///
/// Kept from its opening rather than asked of the service: this window
/// opened it, and the list of every session this computer holds would
/// only have to be searched for the one already known here.
pub fn the_way() -> Option<WayId> {
    PLAYING
        .lock()
        .expect("played session")
        .as_ref()
        .map(|playing| playing.way)
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

/// What is answered to something asked of a session when none is under
/// way.
pub fn none_under_way() -> Fact {
    Fact::new("window.no_session")
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
    /// The way it travels on.
    way: WayId,
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
        .expect("played session")
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
        let mut playing = PLAYING.lock().expect("played session");
        let Some(playing) = playing.as_mut() else {
            return;
        };
        change(&mut playing.settings, &mut playing.steady);
        let wanted = zyr_session::player_wants(&playing.settings, playing.steady);
        playing.player.change(wanted);
        (wanted, playing.settings.codec)
    };
    note(&format!(
        "the player now asks for {}x{} at {} frames/s, {} Mb/s in {codec}, pointer drawn \
         over there: {}, still screen sent again: {}",
        wanted.width,
        wanted.height,
        wanted.fps,
        wanted.bitrate_kbps / 1000,
        if wanted.draw_pointer { "yes" } else { "no" },
        if wanted.steady { "yes" } else { "no" },
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
        .expect("played session")
        .as_ref()
        .is_some_and(|playing| playing.drive.send(Heard::Close).is_ok())
}

/// The codecs the far computer's engine said it can make, once it has
/// said.
pub fn what_the_far_computer_encodes() -> Option<zyr_player::CodecSet> {
    player().and_then(|player| player.encodable())
}

/// Presses Ctrl+Alt+Del on the far computer.
///
/// It goes nowhere near the picture, and could not. Windows keeps that
/// combination for itself at both ends of a session: this computer never
/// sees it, because its own Windows takes it before any program does, and
/// the far computer cannot be made to feel it by an engine, because the
/// way an engine types is exactly the way Windows refuses for this one.
///
/// So it travels on the product's own channel, from this service to the
/// one over there, which presses it on its own machine. That is why this
/// is handled here rather than among the keystrokes: it has no letter and
/// no place on a keyboard, and never will.
pub async fn press_ctrl_alt_del_over_there() -> Result<(), Fact> {
    let way = the_way().ok_or_else(none_under_way)?;
    service::ask(&Request::SecureAttention { way })
        .await
        .map(|_| ())
}

/// Puts the far computer's lock screen up.
///
/// What stands in for Windows+L, and it exists because that combination
/// itself cannot be made to travel. Windows handles it where no program
/// can see it, on purpose: it is one of the two gestures that hand a
/// machine back to whoever is sitting at it. Pressed here it locks this
/// computer whatever a session is doing, and there is no way to type it
/// over there either.
///
/// So it goes round the same way Ctrl+Alt+Del does, and for the same
/// reason: some things a session needs have no letter, no place on a
/// keyboard, and never will.
pub async fn lock_over_there() -> Result<(), Fact> {
    let way = the_way().ok_or_else(none_under_way)?;
    // Timed from here because here is where the picture is watched. The
    // far computer says what its own half cost, and the two together say
    // whether a picture that stands still for a second is standing still
    // on the road or on the machine.
    let asked_at = Instant::now();
    let answer = service::ask(&Request::LockScreen { way }).await;
    note(&format!(
        "far computer's lock screen: {} in {} ms",
        if answer.is_ok() { "done" } else { "refused" },
        asked_at.elapsed().as_millis()
    ));
    answer.map(|_| ())
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
    FAR_SCREEN.lock().expect("far screen").clone()
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
    *FAR_SCREEN.lock().expect("far screen") = id;
}

/// Watches that screen of the far computer from now on, or its main one
/// when nothing is named.
///
/// Asked the moment it is picked and never held back: that computer's
/// engine changes the screen it films where it stands, which costs it a
/// reinitialization of its capture and costs this end nothing at all.
/// The picture is on the other screen within the second, and the session
/// never stops.
pub async fn watch_the_far_screen(id: Option<String>) -> Result<(), Fact> {
    let way = the_way().ok_or_else(none_under_way)?;
    match crate::service::ask(&Request::FilmFarScreen {
        way,
        id: id.clone(),
    })
    .await?
    {
        Answer::Done => {}
        other => return Err(other.unexpected()),
    }
    ask_for_the_far_screen(id);
    note("far computer's screen changed without restarting anything");
    Ok(())
}

/// Moves to the next of the far computer's screens, and round to the
/// first after the last.
///
/// What the shortcut does. A far computer with one screen has nothing to
/// move to, and that is not a failure: it is the ordinary machine, and
/// the key is simply quiet on it.
pub async fn watch_the_next_far_screen() -> Result<(), Fact> {
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
    FAR_SCREENS.lock().expect("far screens").clone()
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
    *FAR_SCREENS.lock().expect("far screens") = screens.to_vec();
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
) -> Result<(), Fact> {
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
async fn become_that_size(app: &App, preferred: Preferred) -> Result<(), Fact> {
    let Some(way) = the_way() else {
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
    let (width, height) = match crate::service::ask(&Request::FarScreen { way, wanted }).await {
        Ok(Answer::Showing {
            size: Some((wide, high)),
        }) => (wide, high),
        Ok(Answer::Showing { size: None }) => (guessed.width, guessed.height),
        Ok(other) => return Err(other.unexpected()),
        Err(reason) => {
            note(&format!(
                "the far computer did not prepare its screen: {reason}"
            ));
            (guessed.width, guessed.height)
        }
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
) -> Result<(), Fact> {
    let peer = fingerprint_of(&fingerprint)?;

    // One at a time, held here and not merely on the screen. Taken
    // before anything moves, and given back by `finish`, which every
    // road out of `drive` ends at.
    if crate::floating::a_session_is_up(&app) || OPENING.swap(true, Ordering::SeqCst) {
        return Err(Fact::new("window.session_already_open"));
    }

    // Nobody has closed a session that has not begun. Put down here
    // rather than trusted: the opening asks it at every step, and a
    // « closed » left standing by whatever came before would let this one
    // go before its first question.
    Floating::closing(&app, false);

    // A session opens on the far computer's main screen, whatever screen
    // some earlier session was watching on some other machine. The choice
    // names one screen of one particular computer, so carrying it over
    // would be asking this one for a screen that is not its own.
    ask_for_the_far_screen(None);
    FAR_SCREENS.lock().expect("far screens").clear();

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

/// How long the player is given to say goodbye once the session is closed.
///
/// It says it to the service on this computer, which answers at once
/// whatever the far computer is doing: this only bounds a player that
/// has stopped answering, so that a closed session is back home within
/// the time it takes to notice, and never later.
const CLOSING_SHOWS: Duration = Duration::from_secs(3);

/// Opens the session and plays it in this window, from the first tunnel
/// to the last picture, bringing the picture back when the session falls
/// over on its own: `zyr_session::see_it_through` decides all of that,
/// and this window is where it is played.
fn drive(app: &App, wanted: Wanted, preferred: Preferred) {
    // Without the window's journal the player has nowhere to write, and
    // no far computer is worth disturbing for a session that cannot play.
    let Some(log) = crate::journal::the_log() else {
        return finish(app, Some(Fact::new("window.no_journal")));
    };
    let mut stage = OnScreen {
        app: app.clone(),
        preferred,
        log: log.clone(),
    };
    let trouble = zyr_session::see_it_through(wanted, &mut stage, &log);
    finish(app, trouble);
}

/// This window, as the stage a session is played on.
struct OnScreen {
    app: App,
    /// What was chosen, read again each time the way is opened again.
    preferred: Preferred,
    /// The window's journal, which the player writes in as well.
    log: Log,
}

impl Stage for OnScreen {
    fn step(&self, step: &Step) {
        if let Some(detail) = told(step) {
            crate::home::step(&self.app, &detail);
        }
    }

    fn still_wanted(&self) -> bool {
        !Floating::a_close_was_asked_for(&self.app)
    }

    fn play(&mut self, opened: Opened, shown: &mut dyn FnMut()) -> Result<Ending, Fact> {
        let showing = Showing::start(&self.app, opened, &self.preferred, &self.log)?;
        Ok(showing.until_it_ends(&self.app, shown))
    }

    fn coming_back(&self, attempt: u32) {
        crate::home::coming_back(&self.app, attempt);
    }

    fn asked_afresh(&mut self, wanted: &mut Wanted) {
        // What is kept when the service cannot be asked is what the
        // picture was already showing, never the ordinary settings: the
        // person asked for nothing to change.
        self.preferred =
            crate::app::block_on(crate::settings::what_was_chosen()).unwrap_or(self.preferred);
        (wanted.settings, wanted.far_magnification) = what_to_ask_for(&self.app, self.preferred);
        wanted.hush_the_far_speakers = self.preferred.mute_far_speakers;
        // And whether that computer is to grow a screen for this session
        // at all, which is the one thing the resolution decides over there.
        wanted.wants_a_screen_over_there = self.preferred.asked.wants_a_screen_over_there();
        // And which of that computer's screens to be served from, which is
        // what a picture coming back has to land on again.
        wanted.far_screen = the_far_screen();
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
    /// Makes the picture's window and starts the player in it, writing
    /// in that journal.
    fn start(app: &App, opened: Opened, preferred: &Preferred, log: &Log) -> Result<Showing, Fact> {
        let Opened {
            link,
            settings,
            way,
        } = opened;
        let window = crate::video::open(app)?;
        let (said, heard) = channel();
        let told = said.clone();
        let steady = preferred.steady_far_rate;
        let started = Player::start(
            &link,
            zyr_session::player_wants(&settings, steady),
            Surface::Window { hwnd: window },
            log.clone(),
            Box::new(move |event| {
                let _ = told.send(Heard::Player(event));
            }),
        );
        let player = match started {
            Ok(player) => player,
            Err(e) => {
                crate::video::close(app);
                return Err(e.fact());
            }
        };
        note(&format!("player plugged into {link}"));
        // The switches of this window start where the settings put them,
        // and the mouse where the session was opened with it.
        crate::floating::adopt(app, preferred);
        crate::system_keys::start_as(preferred.system_keys);
        crate::video::play_a_game(app, !settings.absolute_mouse);
        *PLAYING.lock().expect("played session") = Some(Playing {
            player: player.clone(),
            way: way.way(),
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
    /// it ended. `shown` is told the moment its first picture is on screen.
    fn until_it_ends(self, app: &App, shown: &mut dyn FnMut()) -> Ending {
        let Showing {
            player,
            heard,
            mut way,
        } = self;
        let mut on_screen = false;
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
                        "the player did not stop within {} s, the session is closed here",
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
                    note(&format!("stream in {} {width}x{height}", codec.name()));
                    crate::picture::takes_the_shape(app, (width, height));
                }
                Event::FirstPicture => {
                    shown();
                    on_screen = true;
                    // The button goes up with it, on the window's thread.
                    crate::video::show(app);
                    crate::home::put_the_opening_away(app);
                    // From here on the way is a session the service names,
                    // and closes should this program go without a word.
                    if let Err(reason) = way.hold() {
                        note(&format!(
                            "the service does not count this session among its own: {reason}"
                        ));
                    }
                }
                Event::Notice(fact) => {
                    note(&format!("the player says: {fact}"));
                    // Before the picture, the opening screen is what the
                    // person is reading.
                    if !on_screen {
                        crate::home::step(app, &fact);
                    }
                }
                Event::Ended(ending) => break ending,
            }
        };
        // The picture first, so that nothing hangs a button over it again
        // while the rest is put away.
        crate::video::close(app);
        *PLAYING.lock().expect("played session") = None;
        crate::system_keys::give_them_back();
        crate::floating::lower(app);
        crate::picture::shut_the_pointer_in(crate::picture::Cage::Free);
        drop(player);
        drop(way);
        ending
    }
}

/// Ends the session in progress, the person having closed the window on
/// it.
///
/// The same path the menu takes, and no second one.
pub fn end_it(app: &App) {
    let asked = app.clone();
    crate::app::spawn(async move {
        note("session ended by the window");
        if let Err(reason) = crate::floating::ask(&asked, crate::floating::Act::End).await {
            note(&format!("the session could not be ended: {reason}"));
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
fn told(step: &Step) -> Option<Fact> {
    match step {
        Step::Reached => Some(Fact::new("window.far_getting_ready")),
        Step::NoSoundCardHere => Some(Fact::new("window.no_sound_here")),
        Step::SpeakersLeftAlone { .. }
        | Step::ScreenLeftAlone { .. }
        | Step::FarScreenLeftAlone { .. }
        | Step::ScreenOverThere { .. } => None,
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
        note(&format!("{when}: no home window any more"));
        return;
    }
    fn say(what: bool) -> &'static str {
        if what { "yes" } else { "no" }
    }
    note(&format!(
        "{when}: home on screen={} whole screen={}",
        say(crate::main_window::on_screen()),
        say(crate::main_window::holds_the_screen()),
    ));
}

/// Puts everything back once a session is over, and says why when it
/// ended badly or never opened.
fn finish(app: &App, trouble: Option<Fact>) {
    how_the_window_stands("end of session, before");
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
    match trouble {
        None => crate::home::put_the_opening_away(app),
        Some(why) => crate::home::failed(app, &why),
    }
    how_the_window_stands("end of session, after");
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
            far_screen("one", true),
            far_screen("two", false),
            far_screen("three", false),
        ];
        for (watched, expected) in [("one", "two"), ("two", "three"), ("three", "one")] {
            assert_eq!(
                the_one_after(&screens, watched).map(|screen| screen.id.as_str()),
                Some(expected),
                "from « {watched} »"
            );
        }
        // A screen the list does not know is a list that has changed
        // under the session: the round starts again from the first.
        assert_eq!(
            the_one_after(&screens, "gone").map(|screen| screen.id.as_str()),
            Some("two")
        );
        // And a computer with a single screen has nowhere to go, which
        // is not a fault: the key simply stays silent.
        assert!(the_one_after(&screens[..1], "one").is_none());
        assert!(the_one_after(&[], "").is_none());
    }
}
