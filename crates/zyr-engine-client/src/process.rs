//! Launching the client engine.

use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

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
    PairingFailed { code: Option<i32>, output: String },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::ExecutableNotFound(path) => {
                write!(f, "moteur client introuvable : {}", path.display())
            }
            EngineError::Io(e) => write!(f, "erreur système : {e}"),
            EngineError::PairingFailed { code, output } => {
                let code = code.map(|c| c.to_string()).unwrap_or("interrompu".into());
                write!(f, "appairage échoué ({code}) : {output}")
            }
        }
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

    /// Pairs with a host, without asking anything.
    pub fn pair(&self, host: &str, pin: &str) -> Result<(), EngineError> {
        self.state.prepare()?;
        let output = self
            .command(&command::pairing_arguments(host, pin))?
            .stdin(Stdio::null())
            .output()?;

        // The output is recorded whatever the outcome: the engine
        // reports success even when the pairing did not go through.
        if let Some(mut log) = self.open_log()? {
            let _ = writeln!(log, "--- pairing with {host} ---");
            let _ = log.write_all(&output.stdout);
            let _ = log.write_all(&output.stderr);
        }

        if output.status.success() {
            return Ok(());
        }
        let mut text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if text.is_empty() {
            text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        text.truncate(500);
        Err(EngineError::PairingFailed {
            code: output.status.code(),
            output: text,
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
        Ok(Session {
            engine: command.spawn()?,
        })
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
        Ok(match status.code() {
            Some(0) => SessionOutcome::Ended,
            Some(SESSION_FAILED) => SessionOutcome::Failed,
            Some(UNREACHABLE) => SessionOutcome::Unreachable,
            code => SessionOutcome::Unknown { code },
        })
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
            engine.pair("127.0.0.1", "1234"),
            Err(EngineError::ExecutableNotFound(_))
        ));
        assert!(matches!(
            engine.start_session("127.0.0.1", &SessionSettings::default()),
            Err(EngineError::ExecutableNotFound(_))
        ));
        let _ = engine.state().forget();
    }

    #[test]
    fn the_state_is_prepared_before_the_launch() {
        let engine = missing_engine();
        assert!(!engine.state().is_prepared());
        let _ = engine.pair("127.0.0.1", "1234");
        assert!(engine.state().is_prepared());
        engine.state().forget().unwrap();
    }
}
