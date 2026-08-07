//! Keeps the host engine running for as long as the service does.
//!
//! The supervisor strings three things together: preparing the engine,
//! starting it, and deciding what to do when it stops. The decision
//! itself belongs to the neighbouring module's policy; here we apply it,
//! write down what happens, and hand back the moment a stop is asked
//! for.

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
use zyr_engine_host::{Credentials, EngineRuntime, HostEngine, Listening, SunshineConfig, ports};
use zyr_proto::paths;

use crate::log::Log;
use crate::restart::{Next, Policy};

/// Margin given to the engine to open its ports at start-up.
const START_DELAY: Duration = Duration::from_secs(30);

/// How often the supervisor takes back control to check the engine's
/// state and the stop order.
const WATCH_PERIOD: Duration = Duration::from_millis(500);

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

    loop {
        if order.stop_asked() {
            return End::Asked;
        }

        let start = Instant::now();
        let stop = match one_engine_life(&exe, &runtime_path, order, log) {
            Ok(code) => code,
            Err(reason) => {
                log.write(&reason);
                // An engine that will not start is a failure like any
                // other: the policy decides whether insisting is worth
                // it.
                None
            }
        };

        if order.stop_asked() {
            return End::Asked;
        }

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

/// Starts the engine and follows it until it stops.
///
/// Returns the exit code, or why it could not live at all.
fn one_engine_life(
    exe: &std::path::Path,
    runtime_path: &std::path::Path,
    order: &StopOrder,
    log: &Log,
) -> Result<Option<i32>, String> {
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
    );

    engine.prepare().map_err(|e| e.to_string())?;
    engine.provision_credentials().map_err(|e| e.to_string())?;
    engine.start().map_err(|e| e.to_string())?;
    log.write(&format!("engine started on base port {}", ports.base()));

    let api = EngineApi::new(ports, credentials.clone());
    if let Err(e) = api.wait_until_ready(START_DELAY) {
        let _ = engine.stop();
        return Err(format!("the engine never finished starting: {e}"));
    }

    let runtime = EngineRuntime { ports, credentials };
    if let Err(e) = runtime.write(runtime_path) {
        let _ = engine.stop();
        return Err(format!("engine state not recorded: {e}"));
    }
    log.write("remote access active");

    let code = wait_for_the_engine_to_stop(&mut engine, order, log);
    let _ = EngineRuntime::remove(runtime_path);
    Ok(code)
}

/// Waits for the engine to stop, or stops it when asked to.
fn wait_for_the_engine_to_stop(
    engine: &mut HostEngine,
    order: &StopOrder,
    log: &Log,
) -> Option<i32> {
    loop {
        if order.stop_asked() {
            log.write("stop asked for, the engine is being stopped");
            let _ = engine.stop();
            return None;
        }
        match engine.exit_seen() {
            Ok(Some(code)) => return code,
            Ok(None) => std::thread::sleep(WATCH_PERIOD),
            Err(e) => {
                log.write(&format!("cannot watch the engine: {e}"));
                return None;
            }
        }
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
