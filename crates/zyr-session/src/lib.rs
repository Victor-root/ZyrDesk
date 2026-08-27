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
//! Progress is reported as it happens rather than returned at the end:
//! opening a session takes seconds, and a window with nothing to say for
//! all of them looks stuck.
//!
//! Pairing happens here too, and nobody is asked for anything. The
//! engines demand that a code shown on one computer be typed on the
//! other; the tunnel already recognised both computers by fingerprint
//! before it opened, so the code goes through it. The order is the whole
//! mechanism: the far engine refuses a code as long as nobody is asking
//! it for one, so ours is started and left waiting first.
//!
//! And what this computer remembers of a pairing is only a note it wrote
//! to itself: the far one decides, and can have forgotten. A session that
//! stops before showing anything is therefore taken as that, and the two
//! are introduced again rather than the person being told it failed.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

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
    /// Whether the far computer's speakers fall silent for the length of
    /// the session.
    ///
    /// Asked from here because the choice belongs here: whoever takes
    /// control of a machine in another room is the one who knows that the
    /// room should go quiet, and a setting on that machine would have to
    /// be walked over to, which is the one thing remote control exists to
    /// spare. It travels on the product's own channel, never through an
    /// engine, and the far computer gives its sound back when the way
    /// closes, whatever became of this end.
    pub hush_the_far_speakers: bool,
    /// Whether the far computer is asked to resend a still screen at
    /// full rate while this session watches it.
    ///
    /// Asked here and nowhere else, for the same reason as the speakers:
    /// what it costs is paid over there, and the only person who can tell
    /// whether the picture feels smooth is the one looking at it. Its
    /// engine reads it when it starts, so it is asked before the session
    /// opens and never inside one.
    pub steady_far_rate: bool,
}

/// What is happening, as it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The way is open, and the path takes packets of that size.
    Reached { packet: u16 },
    /// The two computers have never met, and are being introduced.
    /// Nothing is asked of anyone.
    ///
    /// `again` when they believed they already knew each other and the
    /// far one turned out not to agree, which is what a pairing forgotten
    /// on the other side looks like from here.
    Pairing { again: bool },
    /// The same, without a tunnel to carry the code: it has to be typed
    /// on the other computer. Only the diagnostic path ever gets here.
    PairingNeeded { pin: String },
    /// They know each other now.
    Paired,
    /// The engine is starting.
    Starting,
    /// The engine is running, and this is the process it runs as.
    ///
    /// Said as soon as it is known and long before the session is
    /// believed: whoever asked is the only one who knows this number
    /// until the service is told, and the floating button hangs on that
    /// process. Waiting for the session to be believed would put the
    /// button up several seconds after the picture.
    ///
    /// `at` is where the engine reaches the far computer on this
    /// machine. Carried for the same reason as the process: until the
    /// service believes the session, whoever asked is the only one who
    /// can end it, and ending is asked at that address.
    Showing { process: u32, at: String },
    /// The far computer would not silence its own speakers, and the
    /// session goes on regardless.
    ///
    /// Worth saying and never worth failing over: a far computer that
    /// cannot go quiet, because nobody is signed in on it or because
    /// Windows would not have it, still has a perfectly good session to
    /// give.
    SpeakersLeftAlone { refused: String },
    /// The far computer would not change the rate it serves a still
    /// screen at, and the session goes on regardless.
    ///
    /// Worth saying and never worth failing over either: what it costs is
    /// the smoothness of a pointer over a desktop where nothing else is
    /// moving, which is a session slightly less pleasant and not a
    /// session missing.
    RateLeftAlone { refused: String },
    /// The far computer would not wake its virtual screen, and the
    /// session goes on regardless.
    ///
    /// What it costs is the sharpness of a picture larger than that
    /// computer's own screen: without the virtual screen it serves what
    /// its own screen can draw and this end stretches the rest, which is
    /// what every session did before that screen existed.
    ScreenLeftAlone { refused: String },
}

/// How long the engines are given to meet, the code having travelled on
/// its own.
///
/// Generous: what is being waited on is two engines exchanging
/// certificates over a tunnel, not a person.
const PAIRING_PATIENCE: Duration = Duration::from_secs(30);

/// The same, when somebody has to walk to the other computer.
const PAIRING_BY_HAND: Duration = Duration::from_secs(180);

/// How long a session is watched before it is believed.
///
/// Long enough that an engine turned away at the door has stopped. That
/// is not always quick: the engine reaches the far computer over plain
/// text, is refused the encrypted channel that says the two have met,
/// and then takes about five seconds to call that computer offline and
/// give up. Three seconds, which is what this was, ran out first: the
/// session was called live, the engine died just after, and the person
/// read that the far computer had not answered instead of the two being
/// introduced again.
///
/// The cost of waiting longer is that the floating button, which the
/// service only knows about once this is over, arrives a few seconds
/// after the picture. That is the right way round: a button that is late
/// is a nuisance, a session declared live and dead in the same breath is
/// a fault.
///
/// It is only ever waited when the pairing was skipped, so a first
/// session never pays it.
const SESSION_TAKES: Duration = Duration::from_secs(6);

/// How long that watch waits before looking up to ask whether the session
/// is still wanted.
///
/// Short enough that a click to close is felt almost at once, long enough
/// that the wait is not a spin.
const WATCH_STEP: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum Error {
    /// The engine is not on this machine.
    EngineMissing(PathBuf),
    /// The service could not be asked, or refused.
    Service(String),
    /// The engine refused the pairing, or could not be run.
    Pairing(EngineError),
    /// The far computer would not take the code its engine was waiting
    /// for.
    Handover(String),
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
            Error::Handover(reason) => f.write_str(reason),
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
    /// The way the service holds for this session, kept to be let go of.
    ///
    /// Never read, and named so. It is here for exactly as long as the
    /// session lasts, and gives the way back the moment this is dropped,
    /// whichever road the caller took to get there.
    _driving: Option<Driving>,
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

    /// Waits for the session to end.
    ///
    /// The way goes back on its own a line later, when this is dropped.
    pub fn wait(mut self) -> io::Result<SessionOutcome> {
        self.session.wait()
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
///
/// `still_wanted` is asked while the opening is being watched, and only
/// then. Between the picture appearing and this returning there are a
/// few seconds during which the player stopping is read as the far
/// computer having turned this one away; a person who closes the session
/// in those seconds stops the player just the same, and only whoever
/// took that click can tell the two apart. Answered « no », the watch
/// simply stops.
pub fn open(
    wanted: &Wanted,
    told: &mut dyn FnMut(Step),
    still_wanted: &dyn Fn() -> bool,
) -> Result<Running, Error> {
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

    // Asked as soon as the way stands, before the engine is started: a
    // session that never opens has still said it, and the far computer
    // gives its sound back when the way closes either way.
    //
    // A refusal is written down and never fatal. A far computer that
    // cannot silence its own speakers, because nobody is signed in on it
    // or because Windows would not have it, is a far computer that still
    // has a perfectly good session to give.
    if let Some(driving) = &mut driving
        && let Err(refused) = driving.hush_the_far_speakers(wanted.hush_the_far_speakers)
    {
        told(Step::SpeakersLeftAlone { refused });
    }

    // And the same for the rate that computer serves a still screen at,
    // asked before its engine starts because that is the only moment its
    // engine reads it. A refusal costs the smoothness of a pointer and
    // nothing else, so it is written down and the session goes on.
    if let Some(driving) = &mut driving
        && let Err(refused) = driving.serve_steady_over_there(wanted.steady_far_rate)
    {
        told(Step::RateLeftAlone { refused });
    }

    // And the virtual screen over there, asked for the size this session
    // is about to ask its engine for. Before that engine is started,
    // because it can only capture a screen that is already there, and
    // asked at all because that screen sleeps between sessions: a machine
    // nobody is looking at has the screens its owner plugged in and no
    // others.
    //
    // A refusal costs the sharpness of a picture larger than the far
    // machine's own screen and nothing else, so it is written down and
    // the session goes on: that is what every session did before this
    // screen existed.
    if let Some(driving) = &mut driving
        && let Err(refused) = driving.far_screen(Some((settings.width, settings.height)))
    {
        told(Step::ScreenLeftAlone { refused });
    }

    let state = DeviceState::for_device(&identifier_from_address(&wanted.host));
    if wanted.pair_again {
        state.forget().map_err(Error::State)?;
    }

    let already_known = state.has_a_paired_host();
    let log = paths::logs_dir().join("session.log");
    let engine = ClientEngine::new(&exe, state).with_log(&log);

    if !already_known {
        introduce(&engine, &target, driving.as_mut(), false, told)?;
        told(Step::Paired);
    }

    told(Step::Starting);
    let mut session = engine
        .start_session(&target, &settings)
        .map_err(Error::Engine)?;
    told(Step::Showing {
        process: session.process_id(),
        at: target.clone(),
    });

    // What this computer remembers of a pairing is a note it wrote to
    // itself, and the far computer is the only one that decides. It can
    // have been reinstalled, reset, or simply have forgotten, and the
    // engine then turns the session away in under a second, into a log
    // nobody reads. Watched here rather than believed: the two are
    // introduced again, and the session opens.
    //
    // Only when the pairing was skipped. Having just been introduced and
    // still being turned away is another fault entirely, and doing it
    // twice would not make it any better.
    if already_known && gave_up_at_once(&mut session, still_wanted)? {
        introduce(&engine, &target, driving.as_mut(), true, told)?;
        told(Step::Paired);
        told(Step::Starting);
        session = engine
            .start_session(&target, &settings)
            .map_err(Error::Engine)?;
        told(Step::Showing {
            process: session.process_id(),
            at: target.clone(),
        });
    }

    // From here the session belongs to the engine and to the service.
    // Whoever asked for it may go.
    if let Some(driving) = &mut driving {
        driving.hold(session.process_id());
    }

    Ok(Running {
        session,
        _driving: driving,
        log,
    })
}

/// Introduces two engines that have never met.
///
/// Ours is started first and left waiting, because the far one refuses a
/// code as long as nobody is asking it for one. The code then goes
/// through the tunnel, which recognised both computers before it opened,
/// and only then is the outcome waited for.
fn introduce(
    engine: &ClientEngine,
    target: &str,
    driving: Option<&mut Driving>,
    again: bool,
    told: &mut dyn FnMut(Step),
) -> Result<(), Error> {
    let pin = random::pairing_pin();

    let Some(driving) = driving else {
        // No tunnel, so no channel to carry the code: the diagnostic
        // path, and the only place anybody still types one.
        told(Step::PairingNeeded { pin: pin.clone() });
        return engine
            .start_pairing(target, &pin)
            .map_err(Error::Pairing)?
            .settled(PAIRING_BY_HAND)
            .map_err(Error::Pairing);
    };

    told(Step::Pairing { again });
    let pairing = engine.start_pairing(target, &pin).map_err(Error::Pairing)?;
    driving.hand_over_the_code(&pin).map_err(Error::Handover)?;
    pairing.settled(PAIRING_PATIENCE).map_err(Error::Pairing)
}

/// Whether the engine stopped before showing anything.
///
/// A session that has taken is still running when this returns, and one
/// the far engine turned away is long gone. Ending straight away of its
/// own accord is not a failure and is left alone: somebody closed it.
///
/// Which the exit code does not say. Closing a session hands the far
/// computer its desktop back, that computer takes the stream away, and
/// the engine stops the only way it knows how, on a failure: exactly what
/// a computer that no longer knows this one looks like. So the caller is
/// asked, in small steps rather than once at the end, and the watch drops
/// the moment the session stops being wanted. Without it, closing a
/// session during these few seconds had the two computers introduced
/// again over a session the person had just left, and the far engine,
/// asked for a pairing nobody was waiting for, refused it.
fn gave_up_at_once(session: &mut Session, still_wanted: &dyn Fn() -> bool) -> Result<bool, Error> {
    let deadline = Instant::now() + SESSION_TAKES;
    while Instant::now() < deadline {
        if !still_wanted() {
            return Ok(false);
        }
        let stopped = session
            .settled(WATCH_STEP)
            .map_err(|e| Error::Engine(EngineError::Io(e)))?;
        if stopped.is_some() {
            return Ok(worth_introducing_again(stopped, still_wanted()));
        }
    }
    Ok(false)
}

/// Whether what the engine stopped on is worth introducing the two
/// computers again.
///
/// Still running is a session that has taken. Ending of its own accord is
/// somebody who closed it, and pairing over that would reopen a session
/// they had just left.
///
/// And a session no longer wanted is never worth it, whatever the engine
/// stopped on. Closing a session hands the far computer its desktop back,
/// that computer takes the stream away, and the engine stops on a
/// failure: from here that is indistinguishable from a computer that no
/// longer knows this one. Only the caller knows, so the caller is asked.
fn worth_introducing_again(stopped: Option<SessionOutcome>, still_wanted: bool) -> bool {
    still_wanted
        && matches!(
            stopped,
            Some(
                SessionOutcome::Failed
                    | SessionOutcome::Unreachable
                    | SessionOutcome::Unknown { .. }
            )
        )
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

    /// Asks the far computer to wake its virtual screen for a picture
    /// that size, or, with no size, to put it back to sleep.
    fn far_screen(&mut self, size: Option<(u32, u32)>) -> Result<(), String> {
        self.asked(&Request::FarScreen {
            way: self.way,
            size,
        })
    }

    /// Asks the far computer to resend a still screen at full rate, or
    /// to stop doing it.
    fn serve_steady_over_there(&mut self, rate: bool) -> Result<(), String> {
        self.asked(&Request::SteadyFar {
            way: self.way,
            rate,
        })
    }

    /// Asks the far computer to silence its speakers, or to let them
    /// play again.
    fn hush_the_far_speakers(&mut self, quiet: bool) -> Result<(), String> {
        self.asked(&Request::Hush {
            way: self.way,
            quiet,
        })
    }

    /// One ask of the service that is either done or refused, and nothing
    /// else. Three of them have exactly this shape.
    fn asked(&mut self, request: &Request) -> Result<(), String> {
        match self
            .runtime
            .block_on(self.service.ask(request))
            .map_err(|e| e.to_string())?
        {
            Answer::Done => Ok(()),
            Answer::Refused(reason) => Err(reason),
            other => Err(format!("réponse inattendue du service : {other}")),
        }
    }

    /// Hands the far computer the code its engine is waiting for.
    ///
    /// The service does the sending: it is the one holding the way, and
    /// the way is the only thing that already knows both computers.
    fn hand_over_the_code(&mut self, pin: &str) -> Result<(), String> {
        let request = Request::Pair {
            way: self.way,
            pin: pin.to_string(),
        };
        match self
            .runtime
            .block_on(self.service.ask(&request))
            .map_err(|e| e.to_string())?
        {
            Answer::Done => Ok(()),
            Answer::Refused(reason) => Err(reason),
            other => Err(format!("réponse inattendue du service : {other}")),
        }
    }

    /// Tells the service which process the way now serves, so it closes
    /// on its own whatever becomes of whoever asked.
    fn hold(&mut self, process: u32) {
        let request = Request::Hold {
            way: self.way,
            process,
        };
        // A refusal counts as much as a channel that broke: either way
        // nothing watches the session, and saying so is the only thing
        // that keeps that from being discovered at the next restart.
        let unwatched = match self.runtime.block_on(self.service.ask(&request)) {
            Ok(Answer::Done) => return,
            Ok(other) => other.to_string(),
            Err(e) => e.to_string(),
        };
        eprintln!("Avertissement : le service n'a pas pris la session en charge ({unwatched}).");
        eprintln!("  Elle se fermera avec le programme qui l'a lancée.");
    }

    /// Gives the way back at the end of the session. The service would
    /// close it on its own; saying so frees the address at once.
    fn let_go(&mut self) {
        let request = Request::Release { way: self.way };
        let _ = self.runtime.block_on(self.service.ask(&request));
    }
}

/// The way goes back whatever happens to whoever asked for it.
///
/// A guard and not a line at the end of the road that works. Every road
/// out of `open` after the way stands used to leave it standing: an
/// engine that would not start, a pairing refused, a session watched and
/// found wanting. The service closes a way when the process it was told
/// to watch goes, and it is told that at the very end of `open`, so a way
/// abandoned before then was a way nobody would ever close. One of them
/// stayed open for the rest of the evening after a pairing was refused,
/// with the window showing « Sessions ouvertes: 1 » over no session at
/// all.
///
/// Releasing a way twice is not an error, which is what makes this safe
/// beside anything else that might already have said it.
impl Drop for Driving {
    fn drop(&mut self) {
        self.let_go();
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
            hush_the_far_speakers: false,
            steady_far_rate: true,
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
        let outcome = open(&wanted(), &mut |step| steps.push(step), &|| true);
        assert!(matches!(outcome, Err(Error::EngineMissing(_))));
        assert!(steps.is_empty(), "{steps:?}");
    }

    #[test]
    fn a_session_that_never_took_is_worth_a_second_introduction() {
        // Ce que cet ordinateur retient d'un appairage n'est qu'une note
        // qu'il s'est écrite à lui-même : la machine d'en face peut avoir
        // été réinstallée, remise à zéro, ou simplement avoir oublié. Le
        // moteur repart alors en moins d'une seconde, et c'est le seul
        // signe qu'on en ait.
        assert!(worth_introducing_again(Some(Outcome::Failed), true));
        assert!(worth_introducing_again(Some(Outcome::Unreachable), true));
        assert!(worth_introducing_again(
            Some(Outcome::Unknown { code: Some(9) }),
            true
        ));

        // Toujours en cours : la session a pris, on n'y touche pas.
        assert!(!worth_introducing_again(None, true));
        // Terminée toute seule : quelqu'un l'a fermée. Réappairer
        // rouvrirait une session qu'on vient de quitter.
        assert!(!worth_introducing_again(Some(Outcome::Ended), true));
    }

    #[test]
    fn une_session_que_l_on_ferme_ne_relance_aucun_appairage() {
        // Fermer une session rend son bureau à l'ordinateur d'en face,
        // qui reprend le flux, et le moteur s'arrête de la seule façon
        // qu'il connaisse : sur un échec. Vu d'ici, c'est exactement un
        // ordinateur qui ne nous reconnaît plus. Sans la question posée
        // à l'appelant, fermer pendant les secondes qui suivent
        // l'ouverture faisait repartir un appairage par-dessus une
        // session qu'on venait de quitter, et le moteur d'en face, à qui
        // personne ne demandait de code, le refusait.
        for arret in [
            Some(Outcome::Failed),
            Some(Outcome::Unreachable),
            Some(Outcome::Unknown { code: Some(9) }),
            Some(Outcome::Ended),
            None,
        ] {
            assert!(
                !worth_introducing_again(arret.clone(), false),
                "sur {arret:?}"
            );
        }
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
