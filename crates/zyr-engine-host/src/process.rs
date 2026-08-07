//! Lifecycle of the host engine process.
//!
//! The engine is launched as it comes, with no change to its code:
//! everything happens in the configuration file we produce and the
//! arguments we pass.
//!
//! Where the process actually lands is not decided here: a console
//! supervisor keeps it in its own session, the Windows service pushes it
//! into the session carrying the screen. That is what `Launcher` is for.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::SunshineConfig;
use crate::credentials::Credentials;
use crate::launch::{Launch, Launcher, Running, SameSession};

#[derive(Debug)]
pub enum EngineError {
    ExecutableNotFound(PathBuf),
    Io(io::Error),
    ProvisioningFailed { code: Option<i32>, output: String },
    AlreadyStarted,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::ExecutableNotFound(path) => {
                write!(f, "moteur hôte introuvable : {}", path.display())
            }
            EngineError::Io(e) => write!(f, "erreur système : {e}"),
            EngineError::ProvisioningFailed { code, output } => {
                let code = code.map(|c| c.to_string()).unwrap_or("interrompu".into());
                write!(
                    f,
                    "provisionnement des identifiants échoué ({code}) : {output}"
                )
            }
            EngineError::AlreadyStarted => write!(f, "le moteur est déjà démarré"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<io::Error> for EngineError {
    fn from(e: io::Error) -> Self {
        EngineError::Io(e)
    }
}

/// Arguments that start the engine.
pub fn start_arguments(config: &SunshineConfig) -> Vec<String> {
    vec![path_as_argument(&config.conf_path())]
}

/// Arguments that write the local API credentials.
///
/// The engine writes the credentials file named by the configuration,
/// then exits straight away.
pub fn provisioning_arguments(config: &SunshineConfig, credentials: &Credentials) -> Vec<String> {
    vec![
        path_as_argument(&config.conf_path()),
        "--creds".to_string(),
        credentials.user.clone(),
        credentials.password.clone(),
    ]
}

fn path_as_argument(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Folder to launch the engine from.
///
/// The engine resolves its resources (graphics shaders, images)
/// relative to the current folder and not to its own executable:
/// launching it from anywhere else makes it fail at start-up.
fn working_dir(exe: &Path) -> Option<&Path> {
    exe.parent().filter(|p| !p.as_os_str().is_empty())
}

pub struct HostEngine {
    exe: PathBuf,
    config: SunshineConfig,
    credentials: Credentials,
    log: PathBuf,
    launcher: Box<dyn Launcher>,
    process: Option<Box<dyn Running>>,
}

impl HostEngine {
    pub fn new(
        exe: impl Into<PathBuf>,
        config: SunshineConfig,
        credentials: Credentials,
        log: impl Into<PathBuf>,
    ) -> Self {
        Self {
            exe: exe.into(),
            config,
            credentials,
            log: log.into(),
            launcher: Box::new(SameSession),
            process: None,
        }
    }

    /// Starts the engine somewhere other than the current session.
    pub fn launched_by(mut self, launcher: impl Launcher + 'static) -> Self {
        self.launcher = Box::new(launcher);
        self
    }

    pub fn config(&self) -> &SunshineConfig {
        &self.config
    }

    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// Writes the configuration and creates the folders it expects.
    pub fn prepare(&self) -> Result<(), EngineError> {
        if !self.exe.is_file() {
            return Err(EngineError::ExecutableNotFound(self.exe.clone()));
        }
        for folder in self.config.required_dirs() {
            fs::create_dir_all(folder)?;
        }
        if let Some(parent) = self.log.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(self.config.conf_path(), self.config.render_conf())?;
        fs::write(self.config.apps_path(), self.config.render_apps())?;
        Ok(())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.exe);
        if let Some(folder) = working_dir(&self.exe) {
            command.current_dir(folder);
        }
        command
    }

    /// Writes the local API credentials into the engine's state.
    pub fn provision_credentials(&self) -> Result<(), EngineError> {
        let output = self
            .command()
            .args(provisioning_arguments(&self.config, &self.credentials))
            .stdin(Stdio::null())
            .output()?;
        if output.status.success() {
            return Ok(());
        }
        // Standard output may carry the credentials: only the error
        // stream is reported back, and truncated at that.
        let mut text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        text.truncate(500);
        Err(EngineError::ProvisioningFailed {
            code: output.status.code(),
            output: text,
        })
    }

    /// Starts the engine, with its output redirected into our log.
    pub fn start(&mut self) -> Result<(), EngineError> {
        if self.process.is_some() {
            return Err(EngineError::AlreadyStarted);
        }
        self.process = Some(self.launcher.launch(&Launch {
            exe: self.exe.clone(),
            arguments: start_arguments(&self.config),
            working_dir: working_dir(&self.exe).map(Path::to_path_buf),
            log: self.log.clone(),
        })?);
        Ok(())
    }

    /// Identifier of the engine's process, once started.
    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().map(|process| process.identifier())
    }

    /// Exit code, if the engine has stopped on its own.
    pub fn exit_seen(&mut self) -> Result<Option<Option<i32>>, EngineError> {
        match self.process.as_mut() {
            Some(process) => Ok(process.exit_seen()?),
            None => Ok(None),
        }
    }

    pub fn stop(&mut self) -> Result<(), EngineError> {
        if let Some(mut process) = self.process.take() {
            process.stop()?;
        }
        Ok(())
    }
}

impl Drop for HostEngine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use zyr_proto::net::EnginePorts;

    fn config() -> SunshineConfig {
        SunshineConfig::new(EnginePorts::new(42100).unwrap(), "/data/host", "/data/logs")
    }

    /// Launcher that starts nothing and only remembers what it was asked
    /// for.
    struct Noted(Arc<Mutex<Option<Launch>>>);

    struct Nothing;

    impl Running for Nothing {
        fn identifier(&self) -> u32 {
            0
        }
        fn exit_seen(&mut self) -> io::Result<Option<Option<i32>>> {
            Ok(None)
        }
        fn stop(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Launcher for Noted {
        fn launch(&self, launch: &Launch) -> io::Result<Box<dyn Running>> {
            *self.0.lock().unwrap() = Some(launch.clone());
            Ok(Box::new(Nothing))
        }
    }

    #[test]
    fn starting_passes_the_configuration_file_and_nothing_else() {
        let args = start_arguments(&config());
        assert_eq!(args.len(), 1);
        assert!(args[0].ends_with("engine.conf"));
    }

    #[test]
    fn provisioning_passes_the_configuration_then_the_credentials() {
        let credentials = Credentials {
            user: "u".to_string(),
            password: "p".to_string(),
        };
        let args = provisioning_arguments(&config(), &credentials);
        assert!(args[0].ends_with("engine.conf"));
        assert_eq!(&args[1..], ["--creds", "u", "p"]);
    }

    #[test]
    fn the_engine_is_started_by_the_launcher_it_was_given() {
        // Under the service, this launcher is what puts the engine in
        // the session carrying the screen: a start that went around it
        // would give a black picture and nothing to explain it.
        let seen = Arc::new(Mutex::new(None));
        let mut engine = HostEngine::new(
            "/nowhere/zyrdesk-host-engine",
            config(),
            Credentials::random(),
            "/data/logs/host.log",
        )
        .launched_by(Noted(Arc::clone(&seen)));

        engine.start().unwrap();
        let launch = seen.lock().unwrap().clone().expect("nothing was launched");
        assert_eq!(launch.exe, Path::new("/nowhere/zyrdesk-host-engine"));
        assert!(launch.arguments[0].ends_with("engine.conf"));
        assert_eq!(launch.working_dir.as_deref(), Some(Path::new("/nowhere")));

        // Starting twice would leave the first process unwatched.
        assert!(matches!(engine.start(), Err(EngineError::AlreadyStarted)));
    }

    #[test]
    fn preparing_refuses_a_missing_executable() {
        let engine = HostEngine::new(
            "/nowhere/zyrdesk-host-engine",
            config(),
            Credentials::random(),
            "/data/logs/host.log",
        );
        assert!(matches!(
            engine.prepare(),
            Err(EngineError::ExecutableNotFound(_))
        ));
    }

    #[test]
    fn preparing_writes_the_configuration_and_the_application_list() {
        let base = std::env::temp_dir().join(format!(
            "zyrdesk-test-{}",
            zyr_proto::random::alphanumeric_string(12)
        ));
        let fake_exe = base.join("engine");
        fs::create_dir_all(&base).unwrap();
        fs::write(&fake_exe, b"").unwrap();

        let config = SunshineConfig::new(
            EnginePorts::new(42100).unwrap(),
            base.join("host"),
            base.join("logs"),
        );
        let engine = HostEngine::new(
            &fake_exe,
            config.clone(),
            Credentials::random(),
            base.join("logs/host.log"),
        );
        engine.prepare().unwrap();

        let conf = fs::read_to_string(config.conf_path()).unwrap();
        assert!(conf.contains("bind_address = 127.0.0.1"));
        let apps = fs::read_to_string(config.apps_path()).unwrap();
        assert!(apps.contains("Desktop"));
        assert!(base.join("logs").is_dir());

        fs::remove_dir_all(&base).unwrap();
    }
}
