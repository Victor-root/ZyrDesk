//! Lifecycle of the host engine process.
//!
//! The engine is launched as it comes, with no change to its code:
//! everything happens in the configuration file we produce and the
//! arguments we pass.
//!
//! The supervisor still runs in the foreground here, in the user's own
//! console: a keyboard interrupt therefore reaches the engine too, and
//! it shuts down through its own mechanism. The Windows service replaces
//! that with an explicitly commanded stop.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::config::SunshineConfig;
use crate::credentials::Credentials;

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
    process: Option<Child>,
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
            process: None,
        }
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
        let log = fs::File::create(&self.log)?;
        let log_for_errors = log.try_clone()?;
        let child = self
            .command()
            .args(start_arguments(&self.config))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_for_errors))
            .spawn()?;
        self.process = Some(child);
        Ok(())
    }

    /// Exit code, if the engine has stopped on its own.
    pub fn exit_seen(&mut self) -> Result<Option<Option<i32>>, EngineError> {
        match self.process.as_mut() {
            Some(child) => Ok(child.try_wait()?.map(|status| status.code())),
            None => Ok(None),
        }
    }

    pub fn stop(&mut self) -> Result<(), EngineError> {
        if let Some(mut child) = self.process.take() {
            child.kill()?;
            child.wait()?;
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
    use zyr_proto::net::EnginePorts;

    fn config() -> SunshineConfig {
        SunshineConfig::new(EnginePorts::new(42100).unwrap(), "/data/host", "/data/logs")
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
