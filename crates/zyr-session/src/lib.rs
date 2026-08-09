//! Opening a session on a remote computer, end to end.
//!
//! Three things have to happen in order, and none of them belongs to
//! whoever asked: the service opens a way and hands back a local address
//! standing in for the remote computer, the engine pairs with it if the
//! two have never met, and the engine is started on that address. The
//! service is then told which process the way serves, so it closes on
//! its own whatever becomes of the caller.
//!
//! This lives apart from the command line and the interface because both
//! do exactly the same thing here, and the difference between them is
//! only how they say it: one prints, the other draws.
//!
//! Progress is reported as it happens rather than returned at the end.
//! Pairing is the reason: it shows a code and then waits for someone to
//! type it on the other computer, so the code has to reach the person
//! before the waiting starts.

use std::fmt;
use std::io;
use std::path::PathBuf;

use zyr_control::{Answer, Request, Service, WayId};
use zyr_engine_client::state::identifier_from_address;
use zyr_engine_client::{ClientEngine, DeviceState, EngineError, Session, SessionOutcome};
use zyr_proto::paths;
use zyr_proto::random;
use zyr_proto::session::SessionSettings;
use zyr_transport::{Fingerprint, MediaProfile};

// Handed back by `Running::wait`, so callers do not have to reach past
// this crate to the engine driver to name what they were given.
pub use zyr_engine_client::SessionOutcome as Outcome;

/// What is being asked for.
pub struct Wanted {
    /// Address of the remote computer, as the person wrote it.
    pub host: String,
    /// Fingerprint the remote computer is recognised by.
    ///
    /// Without one there is no tunnel: the engine is pointed straight at
    /// the address, which is the diagnostic path and never how a session
    /// is opened for real.
    pub peer: Option<Fingerprint>,
    pub settings: SessionSettings,
    /// Pairs again even if the two computers already know each other.
    pub pair_again: bool,
}

/// What is happening, as it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The way is open, and the path takes packets of that size.
    Reached { packet: u16 },
    /// The two computers have never met: this code has to be typed on
    /// the other one before anything else can happen.
    PairingNeeded { pin: String },
    /// It was typed, and accepted.
    Paired,
    /// The engine is starting.
    Starting,
}

#[derive(Debug)]
pub enum Error {
    /// The engine is not on this machine.
    EngineMissing(PathBuf),
    /// The service could not be asked, or refused.
    Service(String),
    /// The engine refused the pairing, or could not be run.
    Pairing(EngineError),
    /// The engine could not be started.
    Engine(EngineError),
    /// The far computer would not let go of what it was showing.
    Closing(EngineError),
    /// The device's own state could not be reset.
    State(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EngineMissing(path) => {
                write!(f, "moteur client introuvable : {}", path.display())
            }
            Error::Service(reason) => f.write_str(reason),
            Error::Pairing(e) => write!(f, "appairage refusé : {e}"),
            Error::Engine(e) => write!(f, "démarrage de la session : {e}"),
            Error::Closing(e) => write!(f, "fermeture sur l'ordinateur distant : {e}"),
            Error::State(e) => write!(f, "réinitialisation de l'appairage : {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// A session under way, and the way that serves it.
///
/// Dropping this changes nothing: the service was told which process to
/// watch, and closes the way when that process is gone. Whoever wants to
/// know how the session ended waits for it; whoever does not, walks away.
pub struct Running {
    session: Session,
    driving: Option<Driving>,
    /// Where everything the engine says was collected.
    log: PathBuf,
}

impl Running {
    /// Number the system knows the engine by.
    pub fn process_id(&self) -> u32 {
        self.session.process_id()
    }

    pub fn log(&self) -> &std::path::Path {
        &self.log
    }

    /// Waits for the session to end, and gives the way back at once
    /// rather than leaving the service to notice.
    pub fn wait(mut self) -> io::Result<SessionOutcome> {
        let outcome = self.session.wait();
        if let Some(driving) = &mut self.driving {
            driving.let_go();
        }
        outcome
    }
}

/// Tells the far computer to close what it was showing.
///
/// Leaving a session and closing it are two different things, and both
/// are worth having. Leaving keeps the far computer's desktop open and
/// waiting, so coming back takes no time at all; closing hands it back,
/// which is what to do when one is done for the day.
///
/// `host` is the computer as the person named it, which is what its
/// stored pairing is filed under. `at` is where the tunnel puts it on
/// this machine, which is the only address the engine can reach it at,
/// and it only exists while that tunnel stands.
pub fn close_on_the_far_computer(host: &str, at: &str) -> Result<(), Error> {
    let exe = paths::client_engine_exe();
    if !exe.is_file() {
        return Err(Error::EngineMissing(exe));
    }
    let state = DeviceState::for_device(&identifier_from_address(host));
    ClientEngine::new(&exe, state)
        .with_log(paths::logs_dir().join("session.log"))
        .quit(at)
        .map_err(Error::Closing)
}

/// Opens a session, reporting what happens as it happens.
pub fn open(wanted: &Wanted, told: &mut dyn FnMut(Step)) -> Result<Running, Error> {
    let exe = paths::client_engine_exe();
    if !exe.is_file() {
        return Err(Error::EngineMissing(exe));
    }

    let mut settings = wanted.settings;

    // The way stands before the engine is told anything: what the engine
    // is handed is a local address that only exists once it is open.
    let mut driving = match wanted.peer {
        Some(peer) => {
            let driving =
                Driving::towards(&wanted.host, peer, &settings).map_err(Error::Service)?;
            told(Step::Reached {
                packet: driving.packet,
            });
            Some(driving)
        }
        None => None,
    };
    let target = match &driving {
        Some(driving) => {
            settings.packet_size = Some(u32::from(driving.packet));
            driving.target.clone()
        }
        None => wanted.host.clone(),
    };

    let state = DeviceState::for_device(&identifier_from_address(&wanted.host));
    if wanted.pair_again {
        state.forget().map_err(Error::State)?;
    }

    let already_known = state.has_a_paired_host();
    let log = paths::logs_dir().join("session.log");
    let engine = ClientEngine::new(&exe, state).with_log(&log);

    if !already_known {
        let pin = random::pairing_pin();
        told(Step::PairingNeeded { pin: pin.clone() });
        engine.pair(&target, &pin).map_err(Error::Pairing)?;
        told(Step::Paired);
    }

    told(Step::Starting);
    let session = engine
        .start_session(&target, &settings)
        .map_err(Error::Engine)?;

    // From here the session belongs to the engine and to the service.
    // Whoever asked for it may go.
    if let Some(driving) = &mut driving {
        driving.hold(session.process_id());
    }

    Ok(Running {
        session,
        driving,
        log,
    })
}

/// The service, and the way it holds for this session.
struct Driving {
    runtime: tokio::runtime::Runtime,
    service: Service,
    way: WayId,
    /// Address the client engine is given, standing in for the remote
    /// computer.
    target: String,
    /// Packet size the path allows, imposed on the engine.
    packet: u16,
}

impl Driving {
    /// Asks the service for a way to that computer.
    fn towards(host: &str, peer: Fingerprint, settings: &SessionSettings) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;

        let mut service = runtime
            .block_on(Service::join())
            .map_err(|e| e.to_string())?;
        // The window the transport keeps open follows the session that
        // was actually asked for, not a nominal one.
        let request = Request::Reach {
            host: host.to_string(),
            peer,
            media: MediaProfile {
                bits_per_second: u64::from(settings.bitrate_kbps) * 1000,
                frames_per_second: settings.fps,
            },
        };

        let reached = match runtime
            .block_on(service.ask(&request))
            .map_err(|e| e.to_string())?
        {
            Answer::Reached(reached) => reached,
            Answer::Refused(reason) => return Err(reason),
            other => return Err(format!("réponse inattendue du service : {other}")),
        };

        Ok(Self {
            runtime,
            service,
            way: reached.way,
            target: format!("{}:{}", reached.address, reached.engine.http()),
            packet: reached.packet,
        })
    }

    /// Tells the service which process the way now serves, so it closes
    /// on its own whatever becomes of whoever asked.
    fn hold(&mut self, process: u32) {
        let request = Request::Hold {
            way: self.way,
            process,
        };
        if let Err(e) = self.runtime.block_on(self.service.ask(&request)) {
            eprintln!("Avertissement : le service n'a pas pris la session en charge ({e}).");
            eprintln!("  Elle se fermera avec le programme qui l'a lancée.");
        }
    }

    /// Gives the way back at the end of the session. The service would
    /// close it on its own; saying so frees the address at once.
    fn let_go(&mut self) {
        let request = Request::Release { way: self.way };
        let _ = self.runtime.block_on(self.service.ask(&request));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted() -> Wanted {
        Wanted {
            host: "192.168.1.20".to_string(),
            peer: None,
            settings: SessionSettings::default(),
            pair_again: false,
        }
    }

    #[test]
    fn a_missing_engine_is_reported_before_anything_is_attempted() {
        // Nothing else can be checked without two computers; what
        // matters here is that the check comes first, since everything
        // after it opens a tunnel or writes to disk.
        if paths::client_engine_exe().is_file() {
            return;
        }
        let mut steps = Vec::new();
        let outcome = open(&wanted(), &mut |step| steps.push(step));
        assert!(matches!(outcome, Err(Error::EngineMissing(_))));
        assert!(steps.is_empty(), "{steps:?}");
    }

    #[test]
    fn every_failure_says_something_a_person_can_act_on() {
        let messages = [
            Error::EngineMissing(PathBuf::from("/nowhere/zyrdesk-session")).to_string(),
            Error::Service("192.168.1.20 ne répond pas".to_string()).to_string(),
        ];
        for message in messages {
            assert!(!message.is_empty());
            assert!(!message.starts_with("Error"), "{message}");
        }
    }
}
