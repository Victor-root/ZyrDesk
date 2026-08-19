//! Launching the client engine.

use std::fmt;
use std::fs;
use std::io;
use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use zyr_proto::session::SessionSettings;

use crate::command;
use crate::state::DeviceState;

/// What the engine returns when the session itself went wrong (P-M5).
const SESSION_FAILED: i32 = 2;

/// What the engine returns when the other computer never answered (P-M5).
const UNREACHABLE: i32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    /// The engine stopped normally.
    Ended,
    /// The session started and then went wrong.
    Failed,
    /// The other computer was never reached.
    Unreachable,
    /// The engine stopped in a way it does not name, a crash among them.
    Unknown { code: Option<i32> },
}

#[derive(Debug)]
pub enum EngineError {
    ExecutableNotFound(PathBuf),
    Io(io::Error),
    PairingFailed {
        code: Option<i32>,
        output: String,
    },
    /// The far computer never accepted the code, nor refused it.
    PairingTimedOut(Duration),
    /// The host would not let go of what it was showing (P-M7).
    QuitFailed {
        code: Option<i32>,
        output: String,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::ExecutableNotFound(path) => {
                write!(f, "moteur client introuvable : {}", path.display())
            }
            EngineError::Io(e) => write!(f, "erreur système : {e}"),
            EngineError::PairingFailed { code, output } => {
                write!(f, "appairage échoué ({}){}", named(*code), after(output))
            }
            EngineError::PairingTimedOut(patience) => write!(
                f,
                "l'ordinateur distant n'a pas répondu à l'appairage en {} secondes",
                patience.as_secs()
            ),
            EngineError::QuitFailed { code, output } => write!(
                f,
                "fermeture refusée par l'ordinateur distant ({}){}",
                named(*code),
                after(output)
            ),
        }
    }
}

/// How a stop is named when the system gave no code for it.
fn named(code: Option<i32>) -> String {
    code.map(|c| c.to_string())
        .unwrap_or_else(|| "interrompu".to_string())
}

/// What the engine said, introduced, or nothing at all.
///
/// An engine that stopped without a word must not leave a message
/// trailing off after a colon.
fn after(output: &str) -> String {
    if output.is_empty() {
        String::new()
    } else {
        format!(" : {output}")
    }
}

impl std::error::Error for EngineError {}

impl From<io::Error> for EngineError {
    fn from(e: io::Error) -> Self {
        EngineError::Io(e)
    }
}

pub struct ClientEngine {
    exe: PathBuf,
    state: DeviceState,
    log: Option<PathBuf>,
}

impl ClientEngine {
    pub fn new(exe: impl Into<PathBuf>, state: DeviceState) -> Self {
        Self {
            exe: exe.into(),
            state,
            log: None,
        }
    }

    /// Collects everything the engine writes.
    ///
    /// Without this, its error messages only ever live in its own
    /// windows: a session that fails leaves nothing to work from.
    pub fn with_log(mut self, log: impl Into<PathBuf>) -> Self {
        self.log = Some(log.into());
        self
    }

    /// Opens the log in append mode, creating the folders it needs.
    fn open_log(&self) -> io::Result<Option<fs::File>> {
        let Some(path) = &self.log else {
            return Ok(None);
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map(Some)
    }

    pub fn state(&self) -> &DeviceState {
        &self.state
    }

    fn command(&self, arguments: &[String]) -> Result<Command, EngineError> {
        if !self.exe.is_file() {
            return Err(EngineError::ExecutableNotFound(self.exe.clone()));
        }
        let mut command = Command::new(&self.exe);
        // The working folder is what decides where the state lands.
        command.current_dir(self.state.folder()).args(arguments);
        Ok(command)
    }

    /// Starts pairing with a host, and hands the engine back running.
    ///
    /// The engine asks the far computer to pair and then waits, with no
    /// limit of its own, for it to accept a code. Nobody has given it
    /// one at this point: whoever starts this is the one who hands the
    /// code over, and only then waits for the outcome. Starting the
    /// engine first is not an ordering detail, it is the whole
    /// mechanism: the far engine refuses a code as long as nobody is
    /// asking it for one.
    pub fn start_pairing(&self, host: &str, pin: &str) -> Result<Pairing, EngineError> {
        self.state.prepare()?;
        let mut command = self.command(&command::pairing_arguments(host, pin))?;
        command.stdin(Stdio::null());

        // Collected as it is said rather than at the end: the engine is
        // still talking while we wait, and what it says about a refused
        // pairing lives nowhere else.
        let mut said_from = 0;
        if let Some(mut log) = self.open_log()? {
            let _ = writeln!(log, "--- pairing with {host} ---");
            said_from = log.metadata().map(|about| about.len()).unwrap_or(0);
            let errors = log.try_clone()?;
            command.stdout(Stdio::from(log)).stderr(Stdio::from(errors));
        }

        let engine = command.spawn()?;
        tie_to_this_program(&engine);
        Ok(Pairing {
            engine,
            log: self.log.clone(),
            said_from,
        })
    }

    /// Tells the host to close what it is showing.
    ///
    /// Nothing is streaming through this: it is one question asked over
    /// the same tunnel the session uses, so it only works while that
    /// tunnel is still standing.
    pub fn quit(&self, host: &str) -> Result<(), EngineError> {
        self.state.prepare()?;
        let output = self
            .command(&command::quit_arguments(host))?
            .stdin(Stdio::null())
            .output()?;

        if let Some(mut log) = self.open_log()? {
            let _ = writeln!(log, "--- closing the session on {host} ---");
            let _ = log.write_all(&output.stdout);
            let _ = log.write_all(&output.stderr);
        }

        if output.status.success() {
            return Ok(());
        }
        Err(EngineError::QuitFailed {
            code: output.status.code(),
            output: what_went_wrong(&output),
        })
    }

    /// Starts a session, and hands it back running.
    ///
    /// It is not waited on here: the session belongs to the engine
    /// process from that moment, and whoever asked for it may close
    /// without ending it.
    pub fn start_session(
        &self,
        host: &str,
        settings: &SessionSettings,
    ) -> Result<Session, EngineError> {
        self.state.prepare()?;
        let mut command = self.command(&command::session_arguments(host, settings))?;
        command.stdin(Stdio::null());
        if let Some(mut log) = self.open_log()? {
            let _ = writeln!(log, "--- session towards {host} ---");
            let errors = log.try_clone()?;
            command.stdout(Stdio::from(log)).stderr(Stdio::from(errors));
        }
        let engine = command.spawn()?;
        tie_to_this_program(&engine);
        Ok(Session { engine })
    }
}

/// Makes an engine die with the program that started it.
///
/// Windows does not end a child when its parent goes. An engine started
/// here therefore outlives whoever started it, and outlives it for good:
/// nothing else knows it exists, it holds no window once its session has
/// failed, and it sits in the task manager until the machine is
/// restarted. Closing the interface, updating it, or killing it all left
/// one behind, and they piled up.
///
/// A job object ties them together. Every engine is put in it, and the
/// system empties it the moment the last handle to it closes, which is
/// when this program ends, whatever the reason and even when nothing of
/// this program is left running to notice.
#[cfg(windows)]
fn tie_to_this_program(engine: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

    let Some(leash) = leash() else {
        return;
    };
    // SAFETY: the job is one this program made, and the handle belongs
    // to the process just started.
    unsafe { AssignProcessToJobObject(leash as _, engine.as_raw_handle() as _) };
}

/// The job every engine of this program is put in, made once.
///
/// Nothing closes it: it is meant to be closed by the system taking the
/// program's handles back, which is exactly what has to kill the
/// engines.
#[cfg(windows)]
fn leash() -> Option<isize> {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    static JOB: OnceLock<isize> = OnceLock::new();
    let job = *JOB.get_or_init(|| {
        // SAFETY: a job with no name and no particular rights.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return 0;
        }

        // SAFETY: the structure is plain data and is filled with zeroes
        // before the one field that matters is set.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the job is the one just made, and the structure and
        // its size are ours.
        unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw mut limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        job as isize
    });
    (job != 0).then_some(job)
}

#[cfg(not(windows))]
fn tie_to_this_program(_engine: &Child) {}

/// The last thing the engine said before giving up.
///
/// Its output starts with pages of graphics and translation notes and
/// ends with the reason. Keeping the first characters, which is what
/// this did at first, showed the person a wall of start-up noise and
/// threw away the one line they needed. The whole of it is in the log
/// either way; this is what fits in a message.
fn what_went_wrong(output: &std::process::Output) -> String {
    let mut said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if said.is_empty() {
        said = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    last_words(&said, TOLD)
}

/// How much of the engine's last words a message carries.
const TOLD: usize = 400;

/// The end of a text, cut on a line boundary.
fn last_words(said: &str, most: usize) -> String {
    let lines: Vec<&str> = said
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut room = most;
    for line in lines.iter().rev() {
        if line.len() + 1 > room && !kept.is_empty() {
            break;
        }
        room = room.saturating_sub(line.len() + 1);
        kept.push(line);
    }
    kept.reverse();
    let mut text = kept.join("\n");
    // A single line longer than the whole budget: keep its end, which is
    // where a reason sits, and not its beginning. Cut on a character and
    // never inside one: the engine speaks whatever language it was
    // started in, and an accent is two bytes.
    if text.len() > most {
        let from = (text.len() - most..=text.len())
            .find(|at| text.is_char_boundary(*at))
            .unwrap_or(text.len());
        text = text.split_off(from);
    }
    text
}

/// How often a pairing under way is looked in on.
const PAIRING_POLL: Duration = Duration::from_millis(100);

/// A pairing under way.
pub struct Pairing {
    engine: Child,
    /// Where everything the engine says is collected.
    log: Option<PathBuf>,
    /// Where in that log this pairing's own words begin. Without it the
    /// reason shown would be whatever the previous session left behind.
    said_from: u64,
}

impl Pairing {
    /// Waits for the two engines to have met.
    ///
    /// The wait is cut short rather than left to run forever: the engine
    /// puts no limit of its own on it, so a far computer that stops
    /// answering halfway would leave a session opening with nothing on
    /// screen and nothing to say.
    pub fn settled(mut self, patience: Duration) -> Result<(), EngineError> {
        let deadline = Instant::now() + patience;
        loop {
            match self.engine.try_wait()? {
                Some(status) if status.success() => return Ok(()),
                Some(status) => {
                    return Err(EngineError::PairingFailed {
                        code: status.code(),
                        output: self.what_was_said(),
                    });
                }
                None => {}
            }
            if Instant::now() >= deadline {
                // The engine is stopped on the way out, by `Drop`.
                return Err(EngineError::PairingTimedOut(patience));
            }
            std::thread::sleep(PAIRING_POLL);
        }
    }

    /// The last thing the engine said since this pairing started.
    fn what_was_said(&self) -> String {
        let Some(path) = &self.log else {
            return String::new();
        };
        let mut said = String::new();
        let read = fs::File::open(path).and_then(|mut file| {
            file.seek(io::SeekFrom::Start(self.said_from))?;
            file.read_to_string(&mut said)
        });
        if read.is_err() {
            return String::new();
        }
        last_words(said.trim(), TOLD)
    }
}

impl Drop for Pairing {
    /// A pairing nobody waited for does not get to outlive the program
    /// that started it.
    ///
    /// The engine puts no limit on how long it waits for a code, so a
    /// pairing dropped along the way would leave a process alive until
    /// the machine is restarted. That is a session opening on nothing,
    /// invisible, and impossible to guess at.
    ///
    /// A pairing that has already ended is dropped this way too: killing
    /// what has exited says so and does nothing, which is exactly what
    /// is wanted here.
    fn drop(&mut self) {
        let _ = self.engine.kill();
        let _ = self.engine.wait();
    }
}

/// A session under way.
pub struct Session {
    engine: Child,
}

impl Session {
    /// Number the system knows the engine by.
    ///
    /// This is what the service watches to know the session is over,
    /// whatever became of the program that started it.
    pub fn process_id(&self) -> u32 {
        self.engine.id()
    }

    /// Waits for the session to end.
    pub fn wait(&mut self) -> io::Result<SessionOutcome> {
        let status = self.engine.wait()?;
        Ok(outcome_of(status.code()))
    }

    /// Watches the session just long enough to know it has taken.
    ///
    /// `None` once it is still running at the end of the wait, which is
    /// what a session on its way to the screen looks like. Anything else
    /// is an engine that gave up before showing a thing, and what it gave
    /// up on is worth acting upon rather than reporting as a session that
    /// has ended.
    pub fn settled(&mut self, patience: Duration) -> io::Result<Option<SessionOutcome>> {
        let deadline = Instant::now() + patience;
        loop {
            if let Some(status) = self.engine.try_wait()? {
                return Ok(Some(outcome_of(status.code())));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(PAIRING_POLL);
        }
    }
}

/// What the engine's parting code means.
fn outcome_of(code: Option<i32>) -> SessionOutcome {
    match code {
        Some(0) => SessionOutcome::Ended,
        Some(SESSION_FAILED) => SessionOutcome::Failed,
        Some(UNREACHABLE) => SessionOutcome::Unreachable,
        code => SessionOutcome::Unknown { code },
    }
}

#[cfg(test)]
mod outcomes {
    use super::*;

    #[test]
    fn every_parting_code_the_engine_gives_is_named() {
        // Les codes viennent du correctif P-M5 posé sur le moteur : s'ils
        // changent d'un côté sans l'autre, une session ratée passerait
        // pour une session normale, et personne ne verrait rien.
        assert_eq!(outcome_of(Some(0)), SessionOutcome::Ended);
        assert_eq!(outcome_of(Some(SESSION_FAILED)), SessionOutcome::Failed);
        assert_eq!(outcome_of(Some(UNREACHABLE)), SessionOutcome::Unreachable);
        assert_eq!(
            outcome_of(Some(42)),
            SessionOutcome::Unknown { code: Some(42) }
        );
        // Tué par le système : aucun code, et ce n'est pas une fin
        // normale pour autant.
        assert_eq!(outcome_of(None), SessionOutcome::Unknown { code: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing_engine() -> ClientEngine {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-client-{}",
            zyr_proto::random::alphanumeric_string(12)
        ));
        ClientEngine::new("/nowhere/zyrdesk-session", DeviceState::in_folder(folder))
    }

    #[test]
    fn a_missing_engine_is_reported_before_anything_is_tried() {
        let engine = missing_engine();
        assert!(matches!(
            engine.start_pairing("127.0.0.1", "1234"),
            Err(EngineError::ExecutableNotFound(_))
        ));
        assert!(matches!(
            engine.start_session("127.0.0.1", &SessionSettings::default()),
            Err(EngineError::ExecutableNotFound(_))
        ));
        let _ = engine.state().forget();
    }

    #[test]
    fn what_the_engine_said_last_is_what_is_kept() {
        // Le moteur ouvre sur des pages de notes graphiques et finit par
        // la raison. Garder le début, ce qu'on faisait, montrait le bruit
        // de démarrage et jetait la seule ligne utile.
        let said = "\
00:00:00 - Qt Warning: SetProcessDpiAwarenessContext() failed
00:00:00 - SDL Info (0): Compiled with SDL 2.31.0
00:00:00 - Qt Info: Successfully loaded translation for \"fr_FR\"
PC-VICTOR is already paired";
        let kept = last_words(said, 400);
        assert!(kept.ends_with("is already paired"), "{kept}");

        // Serré, il ne reste que la fin, et jamais rien de plus long que
        // ce qui était demandé.
        let kept = last_words(said, 30);
        assert!(kept.contains("already paired"), "{kept}");
        assert!(kept.len() <= 30, "{} caractères : {kept}", kept.len());

        // Une seule ligne, plus longue que tout le budget : c'est sa fin
        // qui porte la raison.
        let kept = last_words(&format!("{}refusé", "x".repeat(500)), 20);
        assert!(kept.ends_with("refusé"), "{kept}");
        assert_eq!(kept.len(), 20);

        // Et la coupe tombe entre deux caractères, jamais au milieu
        // d'un : le moteur parle la langue dans laquelle il a démarré,
        // et couper un accent en deux ferait paniquer le programme au
        // pire moment, celui où il a une panne à raconter.
        for budget in 1..40 {
            let kept = last_words(&"é".repeat(60), budget);
            assert!(kept.len() <= budget, "{budget} : {kept}");
            assert!(kept.chars().all(|c| c == 'é'), "{budget} : {kept}");
        }
    }

    #[test]
    fn the_state_is_prepared_before_the_launch() {
        let engine = missing_engine();
        assert!(!engine.state().is_prepared());
        let _ = engine.start_pairing("127.0.0.1", "1234");
        assert!(engine.state().is_prepared());
        engine.state().forget().unwrap();
    }

    #[test]
    fn a_pairing_nobody_answers_is_cut_short_rather_than_waited_on() {
        // Le moteur n'impose aucune limite à cette attente-là : sans
        // celle-ci, une session s'ouvrirait indéfiniment sur rien.
        let pairing = Pairing {
            engine: patient_program().spawn().unwrap(),
            log: None,
            said_from: 0,
        };

        let started = Instant::now();
        let outcome = pairing.settled(Duration::from_millis(300));
        assert!(
            matches!(outcome, Err(EngineError::PairingTimedOut(_))),
            "{outcome:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// A program that does nothing and does not stop, standing in for an
    /// engine waiting on a computer that has gone quiet.
    fn patient_program() -> Command {
        let mut command = if cfg!(windows) {
            let mut ping = Command::new("ping");
            ping.args(["-n", "30", "127.0.0.1"]);
            ping
        } else {
            let mut sleep = Command::new("sleep");
            sleep.arg("30");
            sleep
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }
}
