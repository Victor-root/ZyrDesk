//! The folders the person may have to look into, and what is in them.
//!
//! Two of them: where the product writes down what it has to say, and
//! where FFmpeg is expected. The window names them and opens them
//! itself, so nobody is ever walked through a disk over the phone.
//!
//! Which folder is decided here and never elsewhere: a path arriving
//! from anywhere else and handed to the system as it came would open
//! anything at all.

// Everything here is asked for by the home window, which this program
// draws itself, and what draws only exists on Windows, like the windows
// it dresses. Elsewhere, nothing asks these questions: the file stays
// compiled and checked, it is simply called by nobody.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::Command;

use zyr_proto::paths;

/// A folder the window is allowed to name.
#[derive(Clone, Copy)]
enum Which {
    Logs,
    Ffmpeg,
}

impl Which {
    fn read(named: &str) -> Result<Self, String> {
        match named {
            "logs" => Ok(Which::Logs),
            "ffmpeg" => Ok(Which::Ffmpeg),
            other => Err(format!("dossier inconnu : {other}")),
        }
    }

    fn path(self) -> PathBuf {
        match self {
            Which::Logs => paths::logs_dir(),
            Which::Ffmpeg => paths::ffmpeg_dir(),
        }
    }
}

/// Whether FFmpeg is all there, where the player and the host engine load
/// it from.
///
/// Both halves of a session need it, one to decode and the other to
/// encode: without it this computer can neither be controlled nor
/// control another.
pub fn ffmpeg_here() -> bool {
    zyr_codec::Ffmpeg::missing_from(&paths::ffmpeg_dir()).is_empty()
}

/// Where the product writes what it has to say.
pub fn logs_folder() -> String {
    paths::logs_dir().display().to_string()
}

/// Opens one of them, so a problem can be looked at without anyone
/// having to be told where to click.
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
        // A path coming from the page and handed to the system
        // as it is would open anything at all.
        for named in ["logs", "ffmpeg"] {
            assert!(Which::read(named).is_ok(), "{named}");
        }
        for named in ["C:\\Windows", "..", "", "identity"] {
            assert!(Which::read(named).is_err(), "{named}");
        }
    }

    #[test]
    fn each_named_folder_is_one_of_the_product_s_own() {
        let logs = Which::read("logs").unwrap().path();
        assert!(
            logs.starts_with(paths::data_dir()),
            "{} hors du dossier",
            logs.display()
        );
        assert_eq!(Which::read("ffmpeg").unwrap().path(), paths::ffmpeg_dir());
    }
}
