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
    /// Which build the engines in place came from, when that is known.
    pub build: String,
}

#[tauri::command]
pub fn engines() -> Engines {
    Engines {
        host_here: paths::host_engine_exe().is_file(),
        client_here: paths::client_engine_exe().is_file(),
        host_folder: paths::host_engine_dir().display().to_string(),
        client_folder: paths::client_engine_dir().display().to_string(),
        build: which_build(),
    }
}

/// Which build produced the engines sitting on this machine.
///
/// Written by the script that fetches them. Without it, an engine that
/// is present says nothing about whether it is the one this code
/// expects, and the two drift apart in silence: the engines are the one
/// half of the product that a `git pull` does not carry.
fn which_build() -> String {
    match std::fs::read_to_string(paths::engines_dir().join("build.txt")) {
        Ok(text) => build_from(&text),
        // No file at all: engines put there by hand, which stays
        // perfectly valid and simply says nothing about where they came
        // from.
        Err(_) => String::new(),
    }
}

/// What that file says, kept apart from the disk so that what the script
/// writes and what the window reads can be checked against each other.
fn build_from(text: &str) -> String {
    let said = |key: &str| {
        text.lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| name.trim() == key)
            .map(|(_, value)| value.trim().to_string())
    };
    match (said("run"), said("date")) {
        (Some(run), Some(date)) => format!("compilation {run} du {date}"),
        (Some(run), None) => format!("compilation {run}"),
        _ => String::new(),
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
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
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
    fn the_engines_build_is_read_from_what_the_script_writes() {
        // Mot pour mot ce que packaging/engines/fetch-engines.ps1 écrit :
        // les deux doivent parler de la même chose, faute de quoi le
        // journal dirait « moteurs présents » sans jamais dire lesquels.
        let written = "# Moteurs ZyrDesk : d'où viennent ceux qui sont en place.\n\
             # Écrit par packaging/engines/fetch-engines.ps1, à ne pas corriger à la main.\n\
             run = 17392044\n\
             commit = a9f7db93c1\n\
             branche = develop\n\
             date = 2026-08-18T20:31:00Z\n";
        assert_eq!(
            build_from(written),
            "compilation 17392044 du 2026-08-18T20:31:00Z"
        );
    }

    #[test]
    fn engines_put_there_by_hand_say_nothing_rather_than_lie() {
        // Déposer les moteurs soi-même reste parfaitement valable : il
        // n'y a alors rien à dire de leur provenance, et surtout rien à
        // inventer.
        assert!(build_from("").is_empty());
        assert!(build_from("n'importe quoi").is_empty());
        assert!(build_from("# run = 1\n").is_empty());
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
