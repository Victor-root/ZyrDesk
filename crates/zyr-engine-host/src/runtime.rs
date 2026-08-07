//! Description of the host engine currently running.
//!
//! Whoever holds the engine, console supervisor or Windows service, and
//! the pairing command are two separate processes: the first publishes
//! here what it takes to reach the engine, the second reads it back.
//!
//! The file holds the local API credentials in clear. It lives with the
//! rest of the product's data, which is acceptable while everything sits
//! in one working folder belonging to one person: reading it grants no
//! more than pairing a device with this engine. Once the interface talks
//! to the service over a named pipe, the credentials stay in memory and
//! this file loses its reason to exist.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use zyr_proto::net::{BasePortOutOfRange, EnginePorts};

use crate::credentials::Credentials;

const PORT_KEY: &str = "base_port";
const USER_KEY: &str = "user";
const PASSWORD_KEY: &str = "password";

#[derive(Debug)]
pub enum RuntimeError {
    Missing(PathBuf),
    Unreadable(io::Error),
    Malformed(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Missing(path) => write!(
                f,
                "aucun moteur hôte en cours d'exécution (fichier attendu : {})",
                path.display()
            ),
            RuntimeError::Unreadable(e) => write!(f, "état du moteur illisible : {e}"),
            RuntimeError::Malformed(detail) => write!(f, "état du moteur incohérent : {detail}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<BasePortOutOfRange> for RuntimeError {
    fn from(e: BasePortOutOfRange) -> Self {
        RuntimeError::Malformed(e.to_string())
    }
}

pub struct EngineRuntime {
    pub ports: EnginePorts,
    pub credentials: Credentials,
}

impl EngineRuntime {
    /// Standard location, among the product's data.
    pub fn standard_path() -> PathBuf {
        zyr_proto::paths::data_dir().join("host-runtime.conf")
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = format!(
            "{PORT_KEY}={}\n{USER_KEY}={}\n{PASSWORD_KEY}={}\n",
            self.ports.base(),
            self.credentials.user,
            self.credentials.password
        );
        fs::write(path, contents)
    }

    pub fn read(path: &Path) -> Result<Self, RuntimeError> {
        let contents = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(RuntimeError::Missing(path.to_path_buf()));
            }
            Err(e) => return Err(RuntimeError::Unreadable(e)),
        };

        let field = |key: &str| -> Result<String, RuntimeError> {
            contents
                .lines()
                .filter_map(|line| line.split_once('='))
                .find(|(name, _)| name.trim() == key)
                .map(|(_, value)| value.trim().to_string())
                .ok_or_else(|| RuntimeError::Malformed(format!("champ « {key} » absent")))
        };

        let base: u16 = field(PORT_KEY)?
            .parse()
            .map_err(|_| RuntimeError::Malformed(format!("champ « {PORT_KEY} » non numérique")))?;

        Ok(Self {
            ports: EnginePorts::new(base)?,
            credentials: Credentials {
                user: field(USER_KEY)?,
                password: field(PASSWORD_KEY)?,
            },
        })
    }

    pub fn remove(path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "zyrdesk-runtime-{}.conf",
            zyr_proto::random::alphanumeric_string(12)
        ))
    }

    #[test]
    fn writing_then_reading_keeps_everything() {
        let path = temporary_path();
        let runtime = EngineRuntime {
            ports: EnginePorts::new(42375).unwrap(),
            credentials: Credentials::random(),
        };
        runtime.write(&path).unwrap();

        let read_back = EngineRuntime::read(&path).unwrap();
        assert_eq!(read_back.ports, runtime.ports);
        assert_eq!(read_back.credentials, runtime.credentials);

        EngineRuntime::remove(&path).unwrap();
        EngineRuntime::remove(&path).unwrap();
    }

    #[test]
    fn an_engine_that_never_started_is_reported_plainly() {
        let path = temporary_path();
        assert!(matches!(
            EngineRuntime::read(&path),
            Err(RuntimeError::Missing(_))
        ));
    }

    #[test]
    fn inconsistent_files_are_rejected() {
        let cases = [
            "user=u\npassword=p\n",
            "base_port=abc\nuser=u\npassword=p\n",
            "base_port=80\nuser=u\npassword=p\n",
            "base_port=42375\npassword=p\n",
        ];
        for contents in cases {
            let path = temporary_path();
            fs::write(&path, contents).unwrap();
            assert!(
                matches!(EngineRuntime::read(&path), Err(RuntimeError::Malformed(_))),
                "{contents}"
            );
            EngineRuntime::remove(&path).unwrap();
        }
    }
}
