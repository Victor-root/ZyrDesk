//! The folders the person may have to look into, and what is in them.
//!
//! Three of them: where the product writes down what it has to say, and
//! where each engine is expected. The window names them and opens them
//! itself, so nobody is ever walked through a disk over the phone.
//!
//! Which folder is decided here and never by the window: a path arriving
//! from a page and handed to the system as it came would open anything
//! at all.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use zyr_proto::paths;

/// A folder the window is allowed to name.
#[derive(Clone, Copy)]
enum Which {
    Logs,
    HostEngine,
    ClientEngine,
}

impl Which {
    fn read(named: &str) -> Result<Self, String> {
        match named {
            "logs" => Ok(Which::Logs),
            "host-engine" => Ok(Which::HostEngine),
            "client-engine" => Ok(Which::ClientEngine),
            other => Err(format!("dossier inconnu : {other}")),
        }
    }

    fn path(self) -> PathBuf {
        match self {
            Which::Logs => paths::logs_dir(),
            Which::HostEngine => paths::host_engine_dir(),
            Which::ClientEngine => paths::client_engine_dir(),
        }
    }
}

/// What the engines look like on this machine.
///
/// The two halves are told apart on purpose: without the host engine
/// this computer cannot be controlled, without the client one it cannot
/// control anything, and neither costs the other.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Engines {
    pub host_here: bool,
    pub client_here: bool,
    pub host_folder: String,
    pub client_folder: String,
}

#[tauri::command]
pub fn engines() -> Engines {
    Engines {
        host_here: paths::host_engine_exe().is_file(),
        client_here: paths::client_engine_exe().is_file(),
        host_folder: paths::host_engine_dir().display().to_string(),
        client_folder: paths::client_engine_dir().display().to_string(),
    }
}

/// Where the product writes what it has to say.
#[tauri::command]
pub fn logs_folder() -> String {
    paths::logs_dir().display().to_string()
}

/// Opens one of them, so a problem can be looked at without anyone
/// having to be told where to click.
#[tauri::command]
pub fn open_folder(which: String) -> Result<(), String> {
    let folder = Which::read(&which)?.path();
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("le dossier n'a pas pu être créé : {e}"))?;
    shown(&folder).map_err(|e| format!("le dossier n'a pas pu être ouvert : {e}"))
}

#[cfg(windows)]
fn shown(folder: &Path) -> std::io::Result<()> {
    // The file explorer answers with a code of its own whatever happens,
    // so only the launch is worth checking.
    Command::new("explorer").arg(folder).spawn().map(|_| ())
}

#[cfg(not(windows))]
fn shown(folder: &Path) -> std::io::Result<()> {
    Command::new("xdg-open").arg(folder).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_folders_the_product_owns_can_be_named() {
        // Un chemin venu de la page et passé tel quel au système
        // ouvrirait n'importe quoi.
        for named in ["logs", "host-engine", "client-engine"] {
            assert!(Which::read(named).is_ok(), "{named}");
        }
        for named in ["C:\\Windows", "..", "", "identity"] {
            assert!(Which::read(named).is_err(), "{named}");
        }
    }

    #[test]
    fn each_named_folder_is_one_the_product_writes_in() {
        let root = paths::data_dir();
        for named in ["logs", "host-engine", "client-engine"] {
            let path = Which::read(named).unwrap().path();
            assert!(
                path.starts_with(&root),
                "{} hors du dossier",
                path.display()
            );
        }
    }
}
