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

// Outside Windows nothing calls this module: the service does not exist
// there. It stays compiled and tested everywhere, the logic having
// nothing platform-specific about it, but with no caller it would pass
// for dead code. The exception stops at platforms without a service: on
// Windows, genuinely dead code is still reported.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_engine_host::api::EngineApi;
use zyr_engine_host::{
    Credentials, EngineRuntime, HostEngine, Launcher, Listening, SunshineConfig, ports,
};
use zyr_proto::paths;

use crate::log::Log;
use crate::restart::{Next, Policy};

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
}

/// Why the supervisor handed back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// A stop was asked for.
    Asked,
    /// Windows is shutting down.
    WindowsShutdown,
    /// The engine will not stand, even after several restarts.
    EngineWontStand,
    /// There was nothing to start.
    NothingToStart,
}

/// Runs until a stop is asked for, or the engine gives up.
pub fn run(order: &StopOrder, log: &Log) -> End {
    let exe = paths::host_engine_exe();
    if !exe.is_file() {
        log.write(&format!("host engine not found: {}", exe.display()));
        return End::NothingToStart;
    }

    let mut policy = Policy::new();
    let runtime_path = EngineRuntime::standard_path();
    let mut screenless = false;

    loop {
        if order.stop_asked() {
            return End::Asked;
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
        let life = match one_engine_life(&exe, &runtime_path, session, order, log) {
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

        // A session change is not an incident: the engine did what was
        // asked of it, and the failure count has no business moving.
        let Life::Stopped(stop) = life else {
            log.write(&format!(
                "the screen left session {session}, the engine starts over in the new one"
            ));
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
                    "the engine fell {} times in a row without holding, giving up",
                    policy.failures()
                ));
                return End::EngineWontStand;
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

/// Starts the engine in the given session and follows it until it stops.
///
/// Returns how its life ended, or why it could not live at all.
fn one_engine_life(
    exe: &std::path::Path,
    runtime_path: &std::path::Path,
    session: u32,
    order: &StopOrder,
    log: &Log,
) -> Result<Life, String> {
    let Some(ports) = ports::free_base() else {
        return Err("no port available in the range reserved for the engines".to_string());
    };

    // The service does not carry a tunnel end yet, so the engine stays
    // reachable from the local network. It moves to strict loopback once
    // the service holds the tunnel.
    let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir())
        .with_listening(Listening::Network);
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

    let runtime = EngineRuntime { ports, credentials };
    if let Err(e) = runtime.write(runtime_path) {
        let _ = engine.stop();
        return Err(format!("engine state not recorded: {e}"));
    }
    log.write("remote access active");

    let life = wait_for_the_engine_to_stop(&mut engine, session, order, log);
    let _ = EngineRuntime::remove(runtime_path);
    Ok(life)
}

/// Waits for the engine to stop, and stops it when it no longer has a
/// reason to run where it is.
fn wait_for_the_engine_to_stop(
    engine: &mut HostEngine,
    session: u32,
    order: &StopOrder,
    log: &Log,
) -> Life {
    loop {
        if order.stop_asked() {
            log.write("stop asked for, the engine is being stopped");
            let _ = engine.stop();
            return Life::Stopped(None);
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
            let _ = engine.stop();
            return Life::SessionChanged;
        }
        std::thread::sleep(WATCH_PERIOD);
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
    fn without_an_engine_the_supervisor_says_so_instead_of_looping() {
        let folder = std::env::temp_dir().join(format!("zyrdeskd-{}-none", std::process::id()));
        let log = Log::open(&folder.join("service.log")).unwrap();
        // The engine is not installed on the test machine: that is
        // exactly the case the service has to report without insisting.
        if !paths::host_engine_exe().is_file() {
            assert_eq!(run(&StopOrder::new(), &log), End::NothingToStart);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }
}
