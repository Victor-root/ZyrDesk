//! Keeps this computer's door open for as long as the service runs.
//!
//! The supervisor strings three things together: choosing the session
//! the engines have to live in, opening the door while everything a
//! session needs is there, and closing it when that stops being so. Each
//! session that comes through the door brings its own engine up in the
//! session that owns the screen; the supervisor runs none.
//!
//! The session is not a detail. A service lives in a session with no
//! screen: an engine has to be pushed into the one carrying the display,
//! and that session changes whenever somebody signs in, signs out or
//! switches user. The supervisor watches it and opens the door again in
//! the new one, because an engine left in a dead session shows nothing
//! at all.

// Outside Windows nothing calls this module: the service does not exist
// there. It stays compiled and tested everywhere, the logic having
// nothing platform-specific about it, but with no caller it would pass
// for dead code. The exception stops at platforms without a service: on
// Windows, genuinely dead code is still reported.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_codec::Ffmpeg;
use zyr_control::Holdup;
use zyr_proto::log::Log;
use zyr_proto::paths;

use crate::account::Account;
use crate::control::{Answering, Desk};
use crate::gateway::{Gateway, Launcher};
use crate::machine::{Hosting, Machine};
use crate::preferences::Remembered;
use crate::ways::Ways;

/// What this module's lines are filed under.
///
/// The same word for the two files that hold the service together: what
/// Windows starts and what it runs are one thing to whoever is looking
/// for a line about either.
const TAG: &str = "service";

/// How often the supervisor takes back control to check the door, the
/// session on screen and the stop order.
const WATCH_PERIOD: Duration = Duration::from_millis(500);

/// Pause before opening the door again in a new session.
///
/// A user switch hands the screen over in several steps: waiting a
/// moment lets it settle rather than starting engines in a session that
/// is already on its way out.
const SESSION_SETTLING: Duration = Duration::from_secs(1);

/// How often FFmpeg is looked for.
///
/// It can be dropped onto the machine at any moment, so it is worth
/// checking; and doing so every second would be noise.
const ENGINE_WATCH: Duration = Duration::from_secs(5);

/// How often a desk nobody is watching any more is tried again.
///
/// Slower than the rest of the watch on purpose: putting a desk back is
/// an errand in another session, and one that was refused a moment ago
/// is refused again for a while.
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

/// How each session's engine is started in that session.
#[cfg(windows)]
fn launcher(session: u32) -> Arc<dyn Launcher> {
    Arc::new(crate::session::ServingInSession::new(session))
}

#[cfg(not(windows))]
fn launcher(_session: u32) -> Arc<dyn Launcher> {
    Arc::new(crate::gateway::NotHere)
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
    /// having closed the door.
    pub fn ask_for_a_stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn stop_asked(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Why the door was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closed {
    /// A stop was asked for.
    Asked,
    /// The screen moved to another session: the engines belong over
    /// there now.
    SessionChanged,
    /// Remote access was turned off.
    NoLongerWanted,
    /// How this computer speaks on the wire was changed. The door is
    /// opened on that once, so it opens again the new way.
    WireChanged,
    /// FFmpeg went missing: no engine could make a picture.
    FfmpegGone,
}

/// Why the supervisor handed back.
///
/// Nothing about the engine ends the service. A computer whose engine
/// cannot run is still a computer that opens sessions towards others:
/// taking the whole service down would cost it that, and the interface
/// with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// A stop was asked for.
    Asked,
    /// There was nothing to run the service on.
    NoRuntime,
}

/// Runs until a stop is asked for.
pub fn run(order: &StopOrder, log: &Log) -> End {
    let log = &log.about(TAG);

    // One runtime for the whole life of the service: the door is opened
    // and closed many times, but rebuilding the threads underneath it
    // every time would be waste.
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            log.write(&format!("no runtime to carry the tunnel: {e}"));
            return End::NoRuntime;
        }
    };

    // The neighbourhood is announced for as long as the service runs,
    // not for as long as the door is open: a computer that only appeared
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
    // as the door: reaching another computer has nothing to do with this
    // one being reachable.
    //
    // What was asked for last time, honoured before anyone has said
    // anything this time.
    let remembered = Remembered::at(paths::preferences());
    let machine = Machine {
        hosting: Hosting::new(),
        ways: Ways::new(log.clone(), remembered.clone()),
        incoming: crate::incoming::Incoming::default(),
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
    runtime.spawn(machine.ways.clone().keep_the_clipboards_in_step());
    runtime.spawn(machine.ways.clone().carry_the_pieces());

    let around = Around {
        runtime: runtime.handle(),
        machine: &machine,
        order,
        log,
    };
    let mut screenless = false;
    let mut refused = false;
    let mut ffmpegless = false;

    loop {
        if order.stop_asked() {
            return End::Asked;
        }

        // No door open means nobody watching this computer, so its
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
        }

        let ffmpeg = paths::ffmpeg_dir();
        let missing = missing_from(&ffmpeg);
        if !missing.is_empty() {
            // FFmpeg can be dropped in later, and everything this
            // computer needs to reach another one works without it. So
            // it is waited for rather than given up on.
            if !ffmpegless {
                log.write(&format!(
                    "FFmpeg is missing from {} ({}): no engine can make a picture, so this \
                     computer cannot be reached",
                    ffmpeg.display(),
                    missing.join(", ")
                ));
                ffmpegless = true;
                machine.hosting.held_by(Holdup::EngineMissing);
            }
            if !wait(ENGINE_WATCH, order) {
                return End::Asked;
            }
            continue;
        }
        if ffmpegless {
            log.write("FFmpeg found");
            ffmpegless = false;
        }

        let Some(session) = screen_session() else {
            // Between two sign-ins, no session owns the screen. An engine
            // started then would capture nothing, so the door waits.
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

        let closed = match one_door_life(session, &around) {
            Ok(closed) => closed,
            Err(reason) => {
                log.write(&reason);
                if !wait(ENGINE_WATCH, order) {
                    return End::Asked;
                }
                continue;
            }
        };
        match closed {
            Closed::Asked => return End::Asked,
            Closed::SessionChanged => {
                log.write(&format!(
                    "the screen left session {session}, the door opens again in the new one"
                ));
                if !wait(SESSION_SETTLING, order) {
                    return End::Asked;
                }
            }
            Closed::NoLongerWanted | Closed::WireChanged | Closed::FfmpegGone => {}
        }
    }
}

/// Which of FFmpeg's library files that folder does not hold, by name.
///
/// Looked for and not opened: opening them is the engine's, in the
/// session it runs in. What matters here is only whether there is
/// anything to open at all.
fn missing_from(folder: &Path) -> Vec<String> {
    Ffmpeg::missing_from(folder)
        .iter()
        .filter_map(|file| file.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
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
            log: log.about(crate::control::TAG),
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

/// Everything the door is opened against: what does not change from one
/// opening to the next, gathered so it travels as one thing.
struct Around<'a> {
    runtime: &'a tokio::runtime::Handle,
    machine: &'a Machine,
    order: &'a StopOrder,
    log: &'a Log,
}

/// Opens the door for engines living in that session, and holds it open
/// until it no longer should be.
///
/// Returns why it was closed, or why it could not be opened at all.
fn one_door_life(session: u32, around: &Around<'_>) -> Result<Closed, String> {
    let Around {
        runtime,
        machine,
        order,
        log,
    } = around;

    // What a run that never got to finish left behind: the machine was
    // switched off, or the service fell over, with a session in
    // progress. Put back before anybody can come in, the desk first and
    // the grown screen after it, which is the order everything here puts
    // them back in.
    if !crate::screen::noted_before().is_empty() {
        log.write(
            "a desk was left the way a session left it by a run that did not finish, putting it \
             back",
        );
        put_the_desk_back(log);
    } else if !screen_asleep() {
        log.write("a screen was left awake by a run that did not finish, putting it back");
        put_the_grown_screen_away(log, &|| true);
    }

    let wire = machine.remembered.wire();
    let gateway = Gateway::open(runtime, launcher(session), (*machine).clone(), log)
        .map_err(|e| format!("the tunnel could not be opened: {e}"))?;
    machine.hosting.open();
    log.write(&format!(
        "remote access active, each session's engine starting in session {session}"
    ));

    let closed = watch_the_door(&gateway, session, wire, machine, order, log);
    machine.hosting.held_by(match closed {
        Closed::FfmpegGone => Holdup::EngineMissing,
        _ => Holdup::Starting,
    });
    gateway.close();
    Ok(closed)
}

/// Holds the door open, and closes it the moment it no longer has a
/// reason to stand where it is.
fn watch_the_door(
    gateway: &Gateway,
    session: u32,
    wire: crate::preferences::Wire,
    machine: &Machine,
    order: &StopOrder,
    log: &Log,
) -> Closed {
    let mut last_look_for_ffmpeg = Instant::now();
    // Apart from the one above: this one paces trying again after a
    // refusal, and the two would otherwise reset each other.
    let mut last_sleep_try = Instant::now() - SCREEN_WATCH;
    loop {
        if order.stop_asked() {
            log.write("stop asked for, the door closes");
            return Closed::Asked;
        }

        if !machine.remembered.remote_access() {
            log.write("remote access turned off, the door closes");
            return Closed::NoLongerWanted;
        }

        // The door was opened on the wire as it was then, and cannot be
        // moved under a running session: it is opened again, which is
        // what a session opened towards this computer costs.
        if machine.remembered.wire() != wire {
            log.write("how this computer speaks on the wire was changed, the door reopens with it");
            return Closed::WireChanged;
        }

        if last_look_for_ffmpeg.elapsed() >= ENGINE_WATCH {
            last_look_for_ffmpeg = Instant::now();
            let missing = missing_from(&paths::ffmpeg_dir());
            if !missing.is_empty() {
                log.write(&format!(
                    "FFmpeg is no longer all there ({} missing), the door closes",
                    missing.join(", ")
                ));
                return Closed::FfmpegGone;
            }
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
            gateway.silence_was_asked_for(),
            gateway.a_session_is_open(),
            log,
        );

        // And the desk follows the same rule for the same reason. A
        // session that ends properly says so and this never fires; this
        // is the net under the ones that do not, which is every session
        // whose computer was closed, unplugged or crashed, and without it
        // such a session would leave a screen on this machine's desk
        // until somebody noticed.
        //
        // Tried again until it works, and that is the whole of the second
        // half. A refusal counted as done would leave somebody's screens
        // the way a stranger left them, with nothing ever looking at them
        // again, which is the one outcome this must never have.
        if gateway.the_desk_is_held_for_nobody() && last_sleep_try.elapsed() >= SCREEN_WATCH {
            last_sleep_try = Instant::now();
            log.write(
                "nobody is watching this computer any more, its desk goes back the way it was",
            );
            if put_the_desk_back(log) {
                gateway.the_desk_came_back();
            }
        }

        if screen_session() != Some(session) {
            return Closed::SessionChanged;
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
    fn without_ffmpeg_the_service_waits_for_it_instead_of_stopping() {
        // A computer that cannot serve a picture is still a client in its
        // own right. A service that stopped there would cost it the
        // tunnel, network discovery and its interface, for one half of
        // the product it may have no use for.
        if missing_from(&paths::ffmpeg_dir()).is_empty() {
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
