//! Keeps the host engine running for as long as the service does.
//!
//! The supervisor strings four things together: choosing the session the
//! engine has to live in, preparing it, starting it, and deciding what
//! to do when it stops. The decision itself belongs to the neighbouring
//! module's policy; here we apply it, write down what happens, and hand
//! back the moment a stop is asked for.
//!
//! The session is not a detail. A service lives in a session with no
//! screen: the engine has to be pushed into the one carrying the
//! display, and that session changes whenever somebody signs in, signs
//! out or switches user. The supervisor watches it and starts the engine
//! over in the new one, because an engine left in a dead session shows
//! nothing at all.
//!
//! The engine is not reachable from the network: the tunnel the service
//! holds is the only way in, and it hands everything to the engine over
//! loopback. The engine's life and the tunnel's are tied together here.

// Outside Windows nothing calls this module: the service does not exist
// there. It stays compiled and tested everywhere, the logic having
// nothing platform-specific about it, but with no caller it would pass
// for dead code. The exception stops at platforms without a service: on
// Windows, genuinely dead code is still reported.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_control::Holdup;
use zyr_engine_host::api::EngineApi;
use zyr_engine_host::{Credentials, EngineRuntime, HostEngine, Launcher, SunshineConfig, ports};
use zyr_proto::log::Log;
use zyr_proto::paths;

use crate::account::Account;
use crate::control::{Answering, Desk};
use crate::gateway::{AtHand, Gateway};
use crate::machine::{Hosting, Machine};
use crate::preferences::Remembered;
use crate::restart::{self, Next, Policy};
use crate::ways::Ways;

/// Margin given to the engine to open its ports at start-up.
const START_DELAY: Duration = Duration::from_secs(30);

/// How often the supervisor takes back control to check the engine's
/// state, the session on screen and the stop order.
const WATCH_PERIOD: Duration = Duration::from_millis(500);

/// Pause before starting the engine over in a new session.
///
/// A user switch hands the screen over in several steps: waiting a
/// moment lets it settle rather than starting an engine in a session
/// that is already on its way out.
const SESSION_SETTLING: Duration = Duration::from_secs(1);

/// How often a missing or unusable engine is looked in on again.
///
/// It can be dropped onto the machine at any moment, so it is worth
/// checking; and doing so every second would be noise.
const ENGINE_WATCH: Duration = Duration::from_secs(5);

/// How often the engine is read on the subject of the host's screens.
///
/// Slower than the rest of the watch on purpose. The one thing being
/// looked for there is said at the end of a session and stays true until
/// something is done about it, so hearing it two seconds late costs
/// nothing, and reading a file the engine writes to all session long
/// twice a second costs the machine something for nothing.
const SCREEN_WATCH: Duration = Duration::from_secs(2);

/// Identifier of the session attached to the screen, when there is one.
#[cfg(windows)]
fn screen_session() -> Option<u32> {
    crate::session::session_on_screen()
}

/// Outside Windows there is no console session, and no service either.
/// The supervisor stays compiled and tested everywhere, its logic having
/// nothing platform-specific about it.
#[cfg(not(windows))]
fn screen_session() -> Option<u32> {
    Some(0)
}

/// How to start the engine in that session.
#[cfg(windows)]
fn launcher(session: u32) -> impl Launcher + 'static {
    crate::session::SessionLauncher::new(session)
}

#[cfg(not(windows))]
fn launcher(_session: u32) -> impl Launcher + 'static {
    zyr_engine_host::SameSession
}

#[cfg(windows)]
fn wake_to_be_named(log: &Log) -> bool {
    crate::screen::wake_to_be_named(log)
}

#[cfg(not(windows))]
fn wake_to_be_named(_log: &Log) -> bool {
    false
}

/// Puts the screen this computer grew for itself back to sleep.
///
/// Answers whether it really went: a refusal has to be tried again, and
/// the caller is the only one that knows when.
#[cfg(windows)]
fn put_the_grown_screen_away(log: &Log, still_nobody: &dyn Fn() -> bool) -> bool {
    crate::screen::back_to_sleep(log, still_nobody)
}

#[cfg(not(windows))]
fn put_the_grown_screen_away(_log: &Log, _still_nobody: &dyn Fn() -> bool) -> bool {
    true
}

/// Writes down what this computer's screens are doing, from the session
/// that owns them.
///
/// Nothing is moved: this is the errand that holds a desk for a session,
/// asked for nothing at all, which is how it doubles as the one way a
/// service ever learns what is plugged into its own machine.
#[cfg(windows)]
fn look_at_the_desk(log: &Log) {
    match crate::session::hold_the_desk_for(None) {
        Ok(took) => log.write(&format!(
            "this computer's screens were looked at from the session on screen ({took})"
        )),
        Err(e) => log.write(&format!("this computer could not look at its screens: {e}")),
    }
}

#[cfg(not(windows))]
fn look_at_the_desk(_log: &Log) {}

/// Puts this computer's desk back the way it was noted before a session
/// took it, from the session that owns the screen.
///
/// Answers whether it really went back: a refusal has to be tried again,
/// and the caller is the only one that knows when.
#[cfg(windows)]
fn put_the_desk_back(log: &Log) -> bool {
    // The desk first and the grown screen after it, and that order is the
    // whole of the safety. A session that borrowed the grown screen has
    // this computer's desktop on it: taking that screen away first leaves
    // Windows to decide where the desktop lands, and the arrangement put
    // back a moment later would be fighting whatever it decided. Put back
    // first, the desktop is already home on a screen its owner can see,
    // and the grown one goes away with nothing on it.
    //
    // The computer with nothing plugged into it never had a desk noted,
    // and there is nothing to put back before its grown screen goes.
    let back = if crate::screen::noted_before().is_empty() {
        true
    } else {
        the_desk_as_it_was(log)
    };
    if !screen_asleep() {
        put_the_grown_screen_away(log, &|| true);
    }
    back
}

/// Puts back what was noted, saying whether it really went back.
#[cfg(windows)]
fn the_desk_as_it_was(log: &Log) -> bool {
    match crate::session::give_the_desk_back() {
        Ok(took) => {
            log.write(&format!(
                "the desk was put back from the session on screen ({took})"
            ));
            // What became of it is written into this journal from over
            // there, since that is the only place it can be known. What
            // is known here is only that the errand ran, and the note
            // itself is what says whether there is still work to do.
            crate::screen::noted_before().is_empty()
        }
        Err(e) => {
            log.write(&format!("this computer's desk was not put back: {e}"));
            false
        }
    }
}

#[cfg(not(windows))]
fn put_the_desk_back(_log: &Log) -> bool {
    true
}

fn screen_asleep() -> bool {
    crate::screen::asleep()
}

/// Stop order, shared with whatever commands the service.
#[derive(Debug, Clone, Default)]
pub struct StopOrder(Arc<AtomicBool>);

impl StopOrder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for a stop. The supervisor hands back at its next check,
    /// having stopped the engine.
    pub fn ask_for_a_stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn stop_asked(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// How one life of the engine ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Life {
    /// It stopped, with this exit code.
    Stopped(Option<i32>),
    /// The screen moved to another session: the engine was stopped on
    /// purpose, and belongs over there now.
    SessionChanged,
    /// The name the engine knows this computer's virtual screen by was
    /// learned from the engine itself, which only says it as it starts.
    /// It was stopped on purpose so the next one is told to capture it.
    VirtualScreenLearned,
    /// How this computer serves was changed while it was running, and the
    /// engine could not be asked to serve that way where it stood: the way
    /// the screen is captured, which nothing changes in a running engine,
    /// or a floor an engine of an older build does not know how to be
    /// asked. It was stopped on purpose and the next one is told the new
    /// answer.
    ServingChanged,
    /// Which screen this computer is filmed on changed while the engine
    /// ran, its own having refused a size a session asked for. It reads
    /// that at its start and never again, so it was stopped on purpose.
    ScreenToFilmChanged,
    /// It said it could not put this computer's screens back the way it
    /// found them. It was stopped on purpose, which is what makes it try
    /// again.
    ScreenNotPutBack,
    /// Remote access was turned off while it ran.
    NoLongerWanted,
}

/// Why the supervisor handed back.
///
/// Nothing about the host engine ends the service. A computer whose
/// engine is missing, or will not stand, is still a computer that opens
/// sessions towards others: taking the whole service down would cost it
/// that, and the interface with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// A stop was asked for.
    Asked,
    /// Windows is shutting down.
    WindowsShutdown,
    /// There was nothing to run the service on.
    NoRuntime,
}

/// Runs until a stop is asked for.
pub fn run(order: &StopOrder, log: &Log) -> End {
    let exe = paths::host_engine_exe();

    // A screen some session picked belongs to that session and to nothing
    // else. A computer coming back up still serving the screen somebody
    // chose last week would be a computer rearranged by having been looked
    // at, with nobody there to notice: its main screen is the answer until
    // a session says otherwise.
    crate::screen::forget_the_screen_a_session_asked_for();

    // One runtime for the whole life of the service: the tunnel is
    // rebuilt with each engine, but rebuilding the threads underneath it
    // every time would be waste.
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            log.write(&format!("no runtime to carry the tunnel: {e}"));
            return End::NoRuntime;
        }
    };

    // The neighbourhood is announced for as long as the service runs,
    // not for as long as an engine does: a computer that only appeared
    // once its owner opened a window would be no use to anyone.
    let neighbourhood = match announce(log) {
        Ok(neighbourhood) => Some(neighbourhood),
        Err(e) => {
            log.write(&format!(
                "local network discovery unavailable, computers here will not find each other: {e}"
            ));
            None
        }
    };
    // What this computer holds lives as long as the service, not as long
    // as one engine: reaching another computer has nothing to do with
    // this one being reachable, and neither has anything to do with the
    // engine of the moment.
    //
    // What was asked for last time, honoured before anyone has said
    // anything this time.
    let remembered = Remembered::at(paths::preferences());
    let machine = Machine {
        hosting: Hosting::new(),
        ways: Ways::new(log.clone(), remembered.clone()),
        remembered,
        neighbours: neighbourhood
            .as_ref()
            .map(|n| n.found())
            .unwrap_or_default(),
        account: Account::at(paths::account(), log.clone()),
        door: crate::machine::Door::default(),
    };
    // The link to an account, when there is one, held from here on: it
    // proves this computer's key to the server, and keeps its channel
    // open on the runtime for as long as the service runs.
    match zyr_transport::Identity::load_or_create(&paths::identity_dir()) {
        Ok(identity) => machine.account.start(
            runtime.handle(),
            Arc::new(identity),
            machine.hosting.clone(),
            machine.remembered.clone(),
            machine.ways.clone(),
            machine.door.clone(),
        ),
        Err(e) => log.write(&format!("no identity to hold an account link with: {e}")),
    }
    // Not being able to answer the interface leaves this computer
    // reachable all the same, so it is worth saying loudly and carrying
    // on rather than giving up on remote access entirely.
    let _desk = match desk(runtime.handle(), machine.clone(), order.clone(), log) {
        Ok(desk) => Some(desk),
        Err(e) => {
            log.write(&format!(
                "control channel unavailable, the interface cannot drive this service: {e}"
            ));
            None
        }
    };
    runtime.spawn(machine.ways.clone().keep_tidy());

    let mut policy = Policy::new();
    let runtime_path = EngineRuntime::standard_path();
    let around = Around {
        exe: &exe,
        runtime_path: &runtime_path,
        runtime: runtime.handle(),
        machine: &machine,
        order,
        log,
    };
    let mut screenless = false;
    let mut refused = false;
    let mut engineless = false;
    let mut given_up = false;

    loop {
        if order.stop_asked() {
            return End::Asked;
        }

        // No engine running means nobody watching this computer, so its
        // speakers play. This is also what gives the sound back after a
        // session that ended badly, and what keeps trying until somebody
        // is signed in to give it back in.
        crate::speakers::keep_in_step(false, false, log);

        if !machine.remembered.remote_access() {
            // Remote access is off. The service stays up: it is still
            // what opens the ways out, answers the interface and
            // announces nothing on the network. Only being reachable
            // stops.
            if !refused {
                log.write("remote access is off, this computer cannot be reached");
                refused = true;
                machine.hosting.held_by(Holdup::Starting);
            }
            if !wait(SESSION_SETTLING, order) {
                return End::Asked;
            }
            continue;
        }
        if refused {
            log.write("remote access is on again");
            refused = false;
            // Turned off and on again is not a string of failures: the
            // engine deserves its first try back. It is also the one way
            // back from an engine this service has stopped insisting on.
            policy = Policy::new();
            given_up = false;
        }

        if !exe.is_file() {
            // The engine can be dropped in later, and everything this
            // computer needs to reach another one works without it. So
            // it is waited for rather than given up on.
            if !engineless {
                log.write(&format!("host engine not found: {}", exe.display()));
                engineless = true;
                machine.hosting.held_by(Holdup::EngineMissing);
            }
            if !wait(ENGINE_WATCH, order) {
                return End::Asked;
            }
            continue;
        }
        if engineless {
            log.write("host engine found");
            engineless = false;
            policy = Policy::new();
        }

        if given_up {
            // The engine will not stand. Trying forever would fill the
            // log and load the machine for nothing; the way back is the
            // remote access switch, read at the top of this loop.
            if !wait(ENGINE_WATCH, order) {
                return End::Asked;
            }
            continue;
        }

        let Some(session) = screen_session() else {
            // Between two sign-ins, no session owns the screen. An
            // engine started then would capture nothing, so we wait
            // instead of counting it as a failure.
            if !screenless {
                log.write("no session on screen, waiting for one");
                screenless = true;
            }
            if !wait(SESSION_SETTLING, order) {
                return End::Asked;
            }
            continue;
        };
        screenless = false;

        let start = Instant::now();
        let life = match one_engine_life(session, &around) {
            Ok(life) => life,
            Err(reason) => {
                log.write(&reason);
                // An engine that will not start is a failure like any
                // other: the policy decides whether insisting is worth
                // it.
                Life::Stopped(None)
            }
        };

        if order.stop_asked() {
            return End::Asked;
        }

        // Neither a session change nor a switch turned off is an
        // incident: the engine did what was asked of it, and the failure
        // count has no business moving.
        let Life::Stopped(stop) = life else {
            if life == Life::SessionChanged {
                log.write(&format!(
                    "the screen left session {session}, the engine starts over in the new one"
                ));
            }
            // Straight away rather than after the settling delay: the
            // engine was stopped on purpose the moment it had said what
            // was wanted of it, and nothing on the machine moved.
            if matches!(
                life,
                Life::VirtualScreenLearned | Life::ServingChanged | Life::ScreenToFilmChanged
            ) {
                continue;
            }
            if !wait(SESSION_SETTLING, order) {
                return End::Asked;
            }
            continue;
        };

        let lifetime = start.elapsed();
        match policy.after_stop(stop, lifetime) {
            Next::Finish => {
                log.write("Windows is shutting down, the engine is not restarted");
                return End::WindowsShutdown;
            }
            Next::GiveUp => {
                log.write(&format!(
                    "the engine fell {} times in a row without holding, this computer stays \
                     unreachable until remote access is turned off and on again",
                    policy.failures()
                ));
                machine.hosting.held_by(Holdup::EngineWontStand);
                given_up = true;
            }
            Next::Restart(delay) => {
                log.write(&format!(
                    "engine stopped after {} s, {}; another starts in {} s",
                    lifetime.as_secs(),
                    how_it_went(stop),
                    delay.as_secs()
                ));
                if !wait(delay, order) {
                    return End::Asked;
                }
            }
        }
    }
}

/// Opens the desk the interface and the command line talk to.
fn desk(
    runtime: &tokio::runtime::Handle,
    machine: Machine,
    order: StopOrder,
    log: &Log,
) -> Result<Desk, String> {
    let identity = zyr_transport::Identity::load_or_create(&paths::identity_dir())
        .map_err(|e| e.to_string())?;
    Desk::open(
        runtime,
        zyr_control::CHANNEL,
        Answering {
            fingerprint: identity.fingerprint(),
            machine,
            order,
            log: log.clone(),
        },
    )
    .map_err(|e| e.to_string())
}

/// Says this computer is here, for the other ZyrDesk on the network.
///
/// Nothing depends on this working: two computers whose owners know each
/// other's address get along without it. It only saves them the reading.
fn announce(log: &Log) -> Result<zyr_lan::Neighbourhood, String> {
    let identity = zyr_transport::Identity::load_or_create(&paths::identity_dir())
        .map_err(|e| e.to_string())?;
    let name = zyr_proto::machine::name();
    // What the network carries is written down as it arrives. Two
    // computers that never see each other is the one fault where
    // everything looks normal on both sides, and only this says whether
    // anything is being heard at all.
    let heard = log.clone();
    let neighbourhood =
        zyr_lan::Neighbourhood::open(&name, identity.fingerprint(), move |what| heard.write(what))
            .map_err(|e| e.to_string())?;
    log.write(&format!("announced on the local network as {name}"));
    say_where_this_computer_answers(log);
    Ok(neighbourhood)
}

/// Writes down where this computer answers, card by card.
///
/// The window's journal carries the same line, read live. It belongs
/// here as well so that the service's own trace is enough on its own:
/// two machines that never find each other are most often two machines
/// on two different networks, and a trace that does not say which
/// network turns that into an evening of questions.
fn say_where_this_computer_answers(log: &Log) {
    let answering = zyr_proto::machine::addresses();
    if answering.is_empty() {
        log.write("this computer has no address of its own on any network");
        return;
    }
    for address in answering {
        log.write(&format!("this computer answers at {address}"));
    }
}

/// Everything one engine's life is lived against: what does not change
/// from one engine to the next, gathered so it travels as one thing.
struct Around<'a> {
    exe: &'a std::path::Path,
    runtime_path: &'a std::path::Path,
    runtime: &'a tokio::runtime::Handle,
    machine: &'a Machine,
    order: &'a StopOrder,
    log: &'a Log,
}

/// Starts the engine in the given session and follows it until it stops.
///
/// Returns how its life ended, or why it could not live at all.
fn one_engine_life(session: u32, around: &Around<'_>) -> Result<Life, String> {
    let Around {
        exe,
        runtime_path,
        runtime,
        machine,
        order,
        log,
    } = around;
    let Some(ports) = ports::free_base() else {
        return Err("no port available in the range reserved for the engines".to_string());
    };

    // The engine binds to the local machine only: the tunnel below is
    // the sole way to it, and nothing on the network can knock on its
    // seven ports.
    // Read once, here, and compared against later: the engine is told
    // this at its start and never again, so a change while it runs is
    // only honoured by starting another one.
    let serving = machine.remembered.serving();
    let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir())
        .with_serving(serving);
    // What this computer's screens are doing, asked of the session that
    // owns them because a service cannot see one. Asked here rather than
    // when a session wants to know, so the answer is already in hand at
    // the one moment it decides something: whether this computer has a
    // screen of its own to film at all.
    look_at_the_desk(log);
    // Which screen to capture is read once, as the engine starts, so it
    // is decided here or not at all. A computer with a screen of its own
    // is aimed at its main screen, which is where the desktop is and
    // which a session is put at the size of.
    //
    // The screen this computer grows for itself is for the computer that
    // has none, a machine in a cupboard with nothing plugged into it, and
    // there it is the only thing there is to film. Woken only where it
    // has never been named, and put back below as soon as it has: the
    // engine names the screens it can see, and one that sleeps between
    // sessions is seen by nobody.
    //
    // Either name is absent the first time this computer ever runs an
    // engine, the name being the engine's own and the engine not having
    // said it yet; both are learned below and used from the next start
    // on, which costs that one start a restart.
    let named_this_start = wake_to_be_named(log);
    // Two computers are filmed on the screen they grow rather than on one
    // of their own: the one with nothing plugged in, which has no other,
    // and the one whose own screens draw nothing larger than themselves,
    // which cannot serve a session the size it asks for. Settled here
    // because the engine reads which screen to film at its start and
    // never again, and a session may only borrow that screen where the
    // engine is already looking at it.
    let on_its_own = crate::screen::showing_now().is_none();
    let films_the_grown_screen = on_its_own || crate::screen::the_main_screen_is_stuck();
    // Named on every computer, and not only on the one with nothing
    // plugged in. Left unnamed, the engine films whichever screen the
    // graphics card enumerates first, and it takes the first screen that
    // answers each time it has to start filming again: a screen being
    // resized answers nothing while the change lasts, so the very screen
    // a session had just put at its own size was the one the engine
    // walked away from, for the rest of that session.
    // And on an ordinary computer, the screen it is to be served from:
    // the one a session asked for, and its main one when none did. The
    // watch below reads that same answer again while the engine runs, so
    // a session asking for the screen beside this one is a start over and
    // not a wish nobody acts on.
    let aiming_at = match films_the_grown_screen {
        true => crate::screen::remembered(),
        false => crate::screen::the_screen_to_film(),
    };
    let config = match &aiming_at {
        Some(screen) => {
            log.write(&if films_the_grown_screen {
                format!(
                    "this computer is filmed on the screen it grew for itself ({screen}), {}",
                    if on_its_own {
                        "having none of its own"
                    } else {
                        "its own drawing nothing larger than themselves"
                    }
                )
            } else {
                format!("the engine is aimed at this computer's main screen ({screen})")
            });
            config.with_screen(screen)
        }
        None => config,
    };
    let engine_log = config.log_path();
    let credentials = Credentials::random();
    let mut engine = HostEngine::new(
        exe,
        config,
        credentials.clone(),
        paths::logs_dir().join("engine-console.log"),
    )
    .launched_by(launcher(session));

    engine.prepare().map_err(|e| e.to_string())?;
    engine.provision_credentials().map_err(|e| e.to_string())?;
    engine.start().map_err(|e| e.to_string())?;
    log.write(&format!(
        "engine started in session {session}, process {}, on base port {}",
        engine.process_id().unwrap_or_default(),
        ports.base()
    ));

    let api = EngineApi::new(ports, credentials.clone());
    if let Err(e) = api.wait_until_ready(START_DELAY, || !order.stop_asked()) {
        let _ = engine.stop();
        return Err(format!("the engine never finished starting: {e}"));
    }

    // Asked now and not later: the engine writes its list of screens as
    // it starts and never again, and what is being looked for in it is
    // the one name that lets the next start aim at the right screen.
    let learned = crate::screen::learn_from(
        &engine_log,
        crate::screen::AsStarted {
            aimed_at: aiming_at.as_deref(),
            films_the_grown_screen,
            asleep: !named_this_start && screen_asleep(),
        },
        log,
    );
    // Back to sleep the moment it has been named, whatever came of the
    // naming: awake past this point is a second screen on somebody's desk
    // with nobody watching it.
    if named_this_start {
        // Woken for this one start of the engine and for nothing else:
        // there is no session that could want it.
        put_the_grown_screen_away(log, &|| true);
    } else if !screen_asleep() {
        // Left awake by a run that never got to finish: the machine was
        // switched off, or the service fell over, with a session in
        // progress. It has to go, and it has to go **here** rather than
        // before the engine was started.
        //
        // Before, it collided with the engine head on. An engine starting
        // up tries to put back the arrangement of screens a session it
        // never finished had changed, and it retries that every time a
        // display device is added or removed. Taking our screen away a
        // second earlier was therefore both the thing that made the
        // arrangement it wants to restore impossible and the very event
        // that made it try again, and what it does when it fails is
        // switch every screen it can find back on. Somebody's screens
        // came out of it rearranged at every start of the service.
        //
        // Started first, the engine has said what it had to say, and
        // putting the screen away waits for the desktop to stop changing
        // before touching anything, which is what it has always done at
        // the end of a session.
        log.write("a screen was left awake by a run that did not finish, putting it back");
        put_the_grown_screen_away(log, &|| true);
    }
    // And the desk itself, for the same run that did not finish. What
    // says a session left one behind is the note it wrote before touching
    // anything, which outlives the service that wrote it: nothing else
    // could, the whole point of the note being to survive the machine
    // being switched off in the middle of a session.
    //
    // Here rather than before the engine started, for the reason just
    // above and doubled: rearranging a desktop while the engine is
    // arranging one is how two programs undo each other all evening.
    if !crate::screen::noted_before().is_empty() {
        log.write(
            "a desk was left the way a session left it by a run that did not finish, putting it \
             back",
        );
        put_the_desk_back(log);
    }
    if learned == crate::screen::Learned::StartAgain {
        let _ = engine.stop();
        return Ok(Life::VirtualScreenLearned);
    }

    let state = EngineRuntime {
        ports,
        credentials: credentials.clone(),
    };
    if let Err(e) = state.write(runtime_path) {
        let _ = engine.stop();
        return Err(format!("engine state not recorded: {e}"));
    }

    // Opened last: an engine that never answered has nothing to serve,
    // and dropped first at the end, since a tunnel leading to a stopped
    // engine only makes the other computer wait.
    // What the engine is filming, from the screen it was aimed at. Held
    // in one place and shared: the door moves it when a session asks for
    // another screen, and the watch below reads it to know whether the
    // engine is where it should be.
    let filming: crate::gateway::Filming = Arc::new(std::sync::Mutex::new(aiming_at.clone()));
    // And how the engine serves, from how it was started, held and shared
    // the same way: the door moves the floor a still screen is served at
    // where the engine stands, and the watch below reads it to know
    // whether the engine serves the way it should.
    let serving_now: crate::gateway::ServingNow = Arc::new(std::sync::Mutex::new(serving));
    let at_hand = AtHand {
        ports,
        credentials,
        films_the_grown_screen,
        engine_log: engine_log.clone(),
        filming: filming.clone(),
        serving: serving_now.clone(),
    };
    let gateway = match Gateway::open(runtime, at_hand, (*machine).clone(), log) {
        Ok(gateway) => gateway,
        Err(e) => {
            let _ = engine.stop();
            let _ = EngineRuntime::remove(runtime_path);
            return Err(format!("the tunnel could not be opened: {e}"));
        }
    };
    machine.hosting.open();
    log.write("remote access active");

    let life = {
        let mut watched = Watched {
            engine: &mut engine,
            api: &api,
            gateway: &gateway,
            // From here and not from the top of the file: what the engine
            // said about the screens at its own start is about the run
            // before this one, and that one has already been answered for.
            screens: crate::screen::Watching::from_here(&engine_log),
            heard: false,
            dealt_with: false,
        };
        wait_for_the_engine_to_stop(
            &mut watched,
            session,
            serving_now,
            Aimed {
                grown: films_the_grown_screen,
                at: filming,
            },
            &machine.remembered,
            order,
            log,
        )
    };
    machine.hosting.held_by(Holdup::Starting);
    drop(gateway);
    let _ = EngineRuntime::remove(runtime_path);
    Ok(life)
}

/// Which screen the engine is filming, starting with the one it was
/// aimed at.
///
/// The two travel together because they are one answer: a computer with
/// no screen of its own films the one it grows, and every other computer
/// films a screen it has, named. Read again while it runs, and a
/// difference is an engine that has to start over.
///
/// The name is shared with the door rather than copied: a session can
/// ask the engine to change screen where it stands, and what it is
/// filming then is what the door wrote there.
#[derive(Clone)]
struct Aimed {
    /// Whether it films the screen this computer grew for itself.
    grown: bool,
    /// The screen it is filming, under the engine's own name for it.
    at: crate::gateway::Filming,
}

/// One engine, and everything used to keep an eye on it while it lives.
///
/// Gathered so that watching it stays one function with a readable
/// signature: the engine itself, the two things that can be asked of it,
/// and what is remembered from one turn of the watch to the next.
struct Watched<'a> {
    engine: &'a mut HostEngine,
    api: &'a EngineApi,
    gateway: &'a Gateway,
    screens: crate::screen::Watching,
    /// Whether the engine has said it could not put the screens back.
    ///
    /// Remembered rather than acted on where it is read: the engine says
    /// it once and never again, and the moment it says it is not always
    /// the moment to answer.
    heard: bool,
    /// Whether it has been answered for during this engine's life.
    /// Answered once and once only: both answers below are things that
    /// must not be done twice in a row.
    dealt_with: bool,
}

/// Waits for the engine to stop, and stops it when it no longer has a
/// reason to run where it is.
fn wait_for_the_engine_to_stop(
    watched: &mut Watched<'_>,
    session: u32,
    serving: crate::gateway::ServingNow,
    aimed: Aimed,
    remembered: &Remembered,
    order: &StopOrder,
    log: &Log,
) -> Life {
    let mut last_look = Instant::now();
    // Apart from the one above: this one paces trying again after a
    // refusal, and the two would otherwise reset each other and leave a
    // refused screen unlooked at.
    let mut last_sleep_try = Instant::now() - SCREEN_WATCH;
    loop {
        if order.stop_asked() {
            log.write("stop asked for, the engine is being stopped");
            stop_and_say_how(watched.engine, log);
            return Life::Stopped(None);
        }

        if !remembered.remote_access() {
            log.write("remote access turned off, the engine is being stopped");
            stop_and_say_how(watched.engine, log);
            return Life::NoLongerWanted;
        }

        // Weighed against how the engine serves **now** and not against
        // how it was started: the door moves the floor a still screen is
        // served at where the engine stands, and moves this with it, so
        // the two agree and nothing starts over. What is left here is an
        // engine that could not be asked, and the way the screen is
        // captured, which nothing changes in a running engine.
        let served = *serving.lock().expect("façon de servir");
        if remembered.serving() != served {
            log.write("how this computer serves was changed, the engine starts over with it");
            stop_and_say_how(watched.engine, log);
            return Life::ServingChanged;
        }

        // A session has asked to be served from another of this computer's
        // screens, or to come back to its main one, and the door could not
        // ask the engine to change screen where it stands. Starting it
        // over is then the only way left, and the session that asked
        // knows: it is told that this computer is starting over, and it
        // waits and comes back rather than finding out from a way that
        // broke under it.
        //
        // The ordinary change never reaches this: the door moves the
        // engine and moves this with it, so the two agree and nothing
        // starts over. What is left here is an engine that could not be
        // asked, and a computer whose main screen has no name yet.
        //
        // Not held back until nobody is watching, unlike the case below.
        // The session that asked has no picture yet, it is what is waiting
        // on this, and holding the change until it closed would be holding
        // it for ever.
        let should_be = crate::screen::the_screen_to_film();
        let filming = aimed.at.lock().expect("écran filmé").clone();
        if !aimed.grown && should_be != filming {
            log.write(&format!(
                "a session asked this computer to be served from {}, and the engine was filming \
                 {}, so it starts over",
                should_be.as_deref().unwrap_or("its main screen"),
                filming
                    .as_deref()
                    .unwrap_or("whichever screen it found first")
            ));
            stop_and_say_how(watched.engine, log);
            return Life::ScreenToFilmChanged;
        }

        // A session has just found out that this computer's own screens
        // draw nothing larger than themselves, which changes the screen it
        // is filmed on. Read while nobody is watching and never during a
        // session: starting the engine over takes the tunnel with it, and
        // with the tunnel every session going through it.
        //
        // And never while a desk is still noted, which is the half that
        // was missed. Putting a desk back is paced a couple of seconds
        // slower than this, so the engine went first, and the desk came
        // home through the path meant for a run that did not finish: it
        // did come home, three seconds late and under a sentence that was
        // not true. Somebody's screens come first, the engine can wait.
        if !watched.gateway.a_session_is_open()
            && !aimed.grown
            && crate::screen::noted_before().is_empty()
            && crate::screen::the_main_screen_is_stuck()
        {
            log.write(
                "this computer's own screens draw nothing larger than themselves, so the engine \
                 starts over to film the screen it grew instead",
            );
            stop_and_say_how(watched.engine, log);
            return Life::ScreenToFilmChanged;
        }

        // The speakers follow whoever is watching: silent while a session
        // asked for it, playing again the moment nobody is watching. What
        // is asked comes from the far computer and never from a setting
        // here: the person taking control is the one who knows whether
        // this room should go quiet, and they are not in it to say so.
        // Asked at every turn and doing nothing at all when they already
        // are, so a refusal costs one line and is tried again in a
        // moment.
        crate::speakers::keep_in_step(
            watched.gateway.silence_was_asked_for(),
            watched.gateway.a_session_is_open(),
            log,
        );

        // And the virtual screen follows the same rule for the same
        // reason. A session that ends properly says so and this never
        // fires; this is the net under the ones that do not, which is
        // every session whose computer was closed, unplugged or crashed,
        // and without it such a session would leave a screen on this
        // machine's desk until somebody noticed.
        //
        // Tried again until it works, and that is the whole of the second
        // half. A refusal counted as done would leave somebody's screens
        // the way a stranger left them, with nothing ever looking at them
        // again, which is the one outcome this must never have.
        if watched.gateway.the_desk_is_held_for_nobody() && last_sleep_try.elapsed() >= SCREEN_WATCH
        {
            last_sleep_try = Instant::now();
            log.write(
                "nobody is watching this computer any more, its desk goes back the way it was",
            );
            if put_the_desk_back(log) {
                watched.gateway.the_desk_came_back();
            }
        }

        // The exit code is asked for first: it is the only thing that
        // tells a Windows shutdown from an incident, and a shutdown also
        // takes the session on screen away.
        match watched.engine.exit_seen() {
            Ok(Some(code)) => return Life::Stopped(code),
            Ok(None) => {}
            Err(e) => {
                log.write(&format!("cannot watch the engine: {e}"));
                return Life::Stopped(None);
            }
        }

        if screen_session() != Some(session) {
            stop_and_say_how(watched.engine, log);
            return Life::SessionChanged;
        }

        if last_look.elapsed() >= SCREEN_WATCH {
            last_look = Instant::now();
            if put_the_screens_back(watched, log) {
                return Life::ScreenNotPutBack;
            }
        }
        std::thread::sleep(WATCH_PERIOD);
    }
}

/// Answers for the host's screens when the engine says it cannot.
///
/// The engine changes them for the length of a session and puts them back
/// when it ends, which is the whole point of the arrangement: the far
/// computer is shown a desktop really drawn at the size it asked for,
/// rather than a small one blown up. Putting them back is the half that
/// can fail, and this is what happens then.
///
/// The engine is started over. That is not a trick played on it: going
/// and coming back are the two moments it puts the screens back of its
/// own accord, once on its way out with nothing standing in the way, and
/// then again on its way in for as long as it takes. Three more chances
/// where there were none, and the endless trying that was taking turns
/// with whatever else holds those screens stops the moment it goes.
///
/// If it says the same thing again with no session having been served in
/// between, then starting it over has been tried and has not worked:
/// something else on this computer holds the screens and means to keep
/// them. The engine is told to stop trying, so that at least the
/// monitors stop being switched about, and the journal says so plainly
/// rather than leaving somebody to work out why their machine clicks.
///
/// Neither answer is ever given while somebody is being served, and what
/// was heard before they arrived is forgotten rather than kept for later.
/// Both answers would cut the session that person is in the middle of:
/// one takes the engine away, the other makes it forget what it is meant
/// to be getting back to. And neither would be right anyway, because a
/// session in progress has moved those screens again and the engine will
/// try to put them back when it ends. Whoever quits their session and
/// reconnects straight away, which is the very thing a screen that came
/// back wrong makes people do, is left alone for it.
///
/// What the engine writes is read at every turn all the same, session or
/// no session. Skipping the reading would only pile the words up to be
/// read as one when the session ended, which is the same mistake with a
/// delay on it.
///
/// Answers whether the engine is to be started over.
fn put_the_screens_back(watched: &mut Watched<'_>, log: &Log) -> bool {
    if watched.dealt_with {
        return false;
    }
    watched.heard |= watched.screens.gave_up_on_the_screens();
    if watched.gateway.a_session_is_open() {
        watched.heard = false;
        return false;
    }
    if !watched.heard {
        return false;
    }
    watched.dealt_with = true;

    if watched.gateway.anyone_came_through() {
        log.write(
            "the engine could not put this computer's screens back the way it found them, so it \
             is started over: it puts them back as it goes, and goes on trying as it comes back",
        );
        stop_and_say_how(watched.engine, log);
        return true;
    }

    log.write(
        "the engine still cannot put this computer's screens back, and it has just been started \
         over for that: something else on this computer is holding them",
    );
    match watched.api.stop_trying_to_put_the_screens_back() {
        Ok(()) => log.write(
            "the engine is told to stop trying, so this computer stops switching its monitors \
             about; the screens stay as they are until somebody sets them",
        ),
        Err(e) => log.write(&format!("the engine would not be told to stop trying: {e}")),
    }
    false
}

/// How the engine's life ended, in words rather than in a number.
///
/// A bare number was what the journal carried, and it hid the one thing
/// worth seeing: `1073807364` reads as an incident to anybody, and it is
/// not one. It is a computer taking its engine with it as it goes, which
/// is the moment the host's screen is most likely to be left where a
/// session put it.
fn how_it_went(code: Option<i32>) -> String {
    match code {
        None => "interrupted".to_string(),
        Some(restart::TAKEN_WITH_ITS_SESSION) => {
            "taken away with the session it lived in, which is somebody signing out, somebody \
             switching user, or this computer going down"
                .to_string()
        }
        Some(restart::ENGINE_ASKED_TO_BE_LEFT) => "having asked to be left where it is".to_string(),
        Some(code) => format!("code {code}"),
    }
}

/// Stops the engine and writes down how it went.
///
/// Worth a line of its own every time. The engine puts the far
/// computer's screen back the size and the magnification it found it at
/// as it goes, and only as it goes: this line is what tells a screen
/// that came back wrong because the engine was taken from one that came
/// back wrong for some other reason.
fn stop_and_say_how(engine: &mut HostEngine, log: &Log) {
    match engine.stop() {
        Ok(Some(parting)) => log.write(&parting.to_string()),
        Ok(None) => {}
        Err(e) => log.write(&format!("the engine could not be stopped: {e}")),
    }
}

/// Waits the requested delay while staying alert to the stop order.
///
/// Returns `false` when a stop was asked for during the wait: a service
/// that sleeps a minute before answering is a service Windows kills.
fn wait(delay: Duration, order: &StopOrder) -> bool {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if order.stop_asked() {
            return false;
        }
        std::thread::sleep(WATCH_PERIOD.min(delay));
    }
    !order.stop_asked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stop_order_is_shared_between_two_hands() {
        let order = StopOrder::new();
        let copy = order.clone();
        assert!(!copy.stop_asked());
        order.ask_for_a_stop();
        assert!(copy.stop_asked());
    }

    #[test]
    fn the_wait_is_cut_short_by_the_order() {
        let order = StopOrder::new();
        let copy = order.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            copy.ask_for_a_stop();
        });

        let start = Instant::now();
        // Without this alertness, Windows would kill the service long
        // before this wait was over.
        assert!(!wait(Duration::from_secs(30), &order));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_zero_wait_goes_straight_through() {
        let order = StopOrder::new();
        assert!(wait(Duration::ZERO, &order));
    }

    #[test]
    fn without_an_engine_the_service_waits_for_one_instead_of_stopping() {
        // Un ordinateur sans moteur hôte reste un client à part entière.
        // Un service qui s'arrêterait là lui coûterait le tunnel, la
        // découverte du réseau et son interface, pour une moitié du
        // produit dont il n'a peut-être aucun usage.
        if paths::host_engine_exe().is_file() {
            return;
        }
        let folder = std::env::temp_dir().join(format!("zyrdeskd-{}-none", std::process::id()));
        let log = Log::open(&folder.join("service.log")).unwrap();

        let order = StopOrder::new();
        let asking = order.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            asking.ask_for_a_stop();
        });

        assert_eq!(run(&order, &log), End::Asked);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
