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

use crate::control::{Answering, Desk, Hosting};
use crate::gateway::{AtHand, Gateway};
use crate::preferences::Remembered;
use crate::restart::{Next, Policy};
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
    /// How this computer serves was changed while it was running. The
    /// engine reads that once, at its own start, so it was stopped on
    /// purpose and the next one is told the new answer.
    ServingChanged,
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

    // The desk and the ways out live as long as the service, not as
    // long as one engine: reaching another computer has nothing to do
    // with this one being reachable.
    let ways = Ways::new(log.clone());
    let hosting = Hosting::new();
    // What was asked for last time, honoured before anyone has said
    // anything this time.
    let remembered = Remembered::at(paths::preferences());

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
    let neighbours = neighbourhood
        .as_ref()
        .map(|n| n.found())
        .unwrap_or_default();
    let around_here = neighbours.clone();
    // Not being able to answer the interface leaves this computer
    // reachable all the same, so it is worth saying loudly and carrying
    // on rather than giving up on remote access entirely.
    let _desk = match desk(
        runtime.handle(),
        ways.clone(),
        hosting.clone(),
        remembered.clone(),
        neighbours,
        order.clone(),
        log,
    ) {
        Ok(desk) => Some(desk),
        Err(e) => {
            log.write(&format!(
                "control channel unavailable, the interface cannot drive this service: {e}"
            ));
            None
        }
    };
    runtime.spawn(ways.keep_tidy());

    let mut policy = Policy::new();
    let runtime_path = EngineRuntime::standard_path();
    let around = Around {
        exe: &exe,
        runtime_path: &runtime_path,
        runtime: runtime.handle(),
        hosting: &hosting,
        remembered: &remembered,
        neighbours: &around_here,
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

        if !remembered.remote_access() {
            // Remote access is off. The service stays up: it is still
            // what opens the ways out, answers the interface and
            // announces nothing on the network. Only being reachable
            // stops.
            if !refused {
                log.write("remote access is off, this computer cannot be reached");
                refused = true;
                hosting.held_by(Holdup::Starting);
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
                hosting.held_by(Holdup::EngineMissing);
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
            if matches!(life, Life::VirtualScreenLearned | Life::ServingChanged) {
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
                hosting.held_by(Holdup::EngineWontStand);
                given_up = true;
            }
            Next::Restart(delay) => {
                let code = stop
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "interrupted".to_string());
                log.write(&format!(
                    "engine stopped (code {code}) after {} s, restarting in {} s",
                    lifetime.as_secs(),
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
#[allow(clippy::too_many_arguments)]
fn desk(
    runtime: &tokio::runtime::Handle,
    ways: Ways,
    hosting: Hosting,
    remembered: Remembered,
    neighbours: zyr_lan::Found,
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
            ways,
            hosting,
            remembered,
            neighbours,
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
    hosting: &'a Hosting,
    remembered: &'a Remembered,
    neighbours: &'a zyr_lan::Found,
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
        hosting,
        remembered,
        neighbours,
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
    let serving = remembered.serving();
    let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir())
        .with_serving(serving);
    // Which screen to capture is read once, as the engine starts, so it
    // is decided here or not at all. Absent the first time this computer
    // ever runs, since the name is the engine's own and the engine has
    // not said it yet; learned below and used from the next start on.
    let aiming_at = crate::screen::remembered();
    let config = match &aiming_at {
        Some(screen) => config.with_screen_of_its_own(screen),
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
    // the one name that lets the next start aim at the virtual screen.
    if crate::screen::learn_from(&engine_log, aiming_at.as_deref(), log)
        == crate::screen::Learned::StartAgain
    {
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
    let at_hand = AtHand { ports, credentials };
    let gateway = match Gateway::open(
        runtime,
        at_hand,
        (*neighbours).clone(),
        (*remembered).clone(),
        log,
    ) {
        Ok(gateway) => gateway,
        Err(e) => {
            let _ = engine.stop();
            let _ = EngineRuntime::remove(runtime_path);
            return Err(format!("the tunnel could not be opened: {e}"));
        }
    };
    hosting.open();
    log.write("remote access active");

    let life = wait_for_the_engine_to_stop(&mut engine, session, serving, remembered, order, log);
    hosting.held_by(Holdup::Starting);
    drop(gateway);
    let _ = EngineRuntime::remove(runtime_path);
    Ok(life)
}

/// Waits for the engine to stop, and stops it when it no longer has a
/// reason to run where it is.
fn wait_for_the_engine_to_stop(
    engine: &mut HostEngine,
    session: u32,
    serving: zyr_proto::session::Serving,
    remembered: &Remembered,
    order: &StopOrder,
    log: &Log,
) -> Life {
    loop {
        if order.stop_asked() {
            log.write("stop asked for, the engine is being stopped");
            stop_and_say_how(engine, log);
            return Life::Stopped(None);
        }

        if !remembered.remote_access() {
            log.write("remote access turned off, the engine is being stopped");
            stop_and_say_how(engine, log);
            return Life::NoLongerWanted;
        }

        if remembered.serving() != serving {
            log.write("how this computer serves was changed, the engine starts over with it");
            stop_and_say_how(engine, log);
            return Life::ServingChanged;
        }

        // The exit code is asked for first: it is the only thing that
        // tells a Windows shutdown from an incident, and a shutdown also
        // takes the session on screen away.
        match engine.exit_seen() {
            Ok(Some(code)) => return Life::Stopped(code),
            Ok(None) => {}
            Err(e) => {
                log.write(&format!("cannot watch the engine: {e}"));
                return Life::Stopped(None);
            }
        }

        if screen_session() != Some(session) {
            stop_and_say_how(engine, log);
            return Life::SessionChanged;
        }
        std::thread::sleep(WATCH_PERIOD);
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
