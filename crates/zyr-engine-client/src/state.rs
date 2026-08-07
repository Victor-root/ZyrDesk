//! Client engine state, kept apart for each remote device.
//!
//! The engine switches to portable mode as soon as a `portable.dat` file
//! sits in its working folder: all of its state, meaning settings,
//! identity and paired hosts, then stays in that folder instead of the
//! registry.
//!
//! One folder per remote device buys three things: no concurrent writes
//! between two simultaneous outgoing sessions, an identity that stays
//! stable over time for each relationship, and a reset of one
//! relationship that amounts to deleting a folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PORTABLE_MARKER: &str = "portable.dat";

/// Folder identifier derived from a host address.
///
/// While there is neither an account nor a device registry, the address
/// stands in for an identity. It is reduced to characters that are safe
/// in a folder name on every platform.
pub fn identifier_from_address(host: &str) -> String {
    let cleaned: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_ascii_lowercase();
    if cleaned.is_empty() {
        "device".to_string()
    } else {
        cleaned
    }
}

pub struct DeviceState {
    folder: PathBuf,
}

impl DeviceState {
    /// State of the device in the product's standard location.
    pub fn for_device(device_id: &str) -> Self {
        Self::in_folder(zyr_proto::paths::device_state_dir(device_id))
    }

    pub fn in_folder(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Creates the folder and drops the portable-mode marker in it.
    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.folder)?;
        let marker = self.folder.join(PORTABLE_MARKER);
        if !marker.exists() {
            fs::write(&marker, b"")?;
        }
        Ok(())
    }

    pub fn is_prepared(&self) -> bool {
        self.folder.join(PORTABLE_MARKER).is_file()
    }

    /// Settings files the engine writes under this folder.
    ///
    /// Where exactly they land depends on how the engine names its own
    /// tree, so the search is recursive rather than guessed.
    pub fn settings_files(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        collect_ini(&self.folder, &mut found);
        found.sort();
        found
    }

    /// True once the engine has recorded a paired host.
    ///
    /// Tells us whether a pairing is needed before the session.
    pub fn has_a_paired_host(&self) -> bool {
        self.settings_files().iter().any(|file| {
            fs::read_to_string(file)
                .map(|contents| contents.contains("hosts"))
                .unwrap_or(false)
        })
    }

    /// Erases everything this device relationship holds.
    pub fn forget(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.folder) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

fn collect_ini(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ini(&path, found);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
        {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_folder() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "zyrdesk-state-{}",
            zyr_proto::random::alphanumeric_string(12)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn preparing_drops_the_portable_mode_marker() {
        let base = temporary_folder();
        let state = DeviceState::in_folder(base.join("desk-pc"));
        assert!(!state.is_prepared());
        state.prepare().unwrap();
        assert!(state.is_prepared());
        // Preparing a second time must break nothing.
        state.prepare().unwrap();
        assert!(state.is_prepared());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_settings_are_found_whatever_the_folder_tree() {
        let base = temporary_folder();
        let state = DeviceState::in_folder(&base);
        state.prepare().unwrap();
        assert!(state.settings_files().is_empty());
        assert!(!state.has_a_paired_host());

        let nested = base.join("Some Vendor").join("Some Product");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("settings.ini"), "[General]\nhosts\\size=1\n").unwrap();

        assert_eq!(state.settings_files().len(), 1);
        assert!(state.has_a_paired_host());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn forgetting_erases_everything_and_stays_quiet_when_absent() {
        let base = temporary_folder();
        let state = DeviceState::in_folder(base.join("pc"));
        state.prepare().unwrap();
        assert!(state.folder().exists());
        state.forget().unwrap();
        assert!(!state.folder().exists());
        state.forget().unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn each_device_gets_its_own_state() {
        let first = DeviceState::for_device("desk-pc");
        let second = DeviceState::for_device("laptop");
        assert_ne!(first.folder(), second.folder());
    }

    #[test]
    fn addresses_become_safe_folder_names() {
        assert_eq!(identifier_from_address("192.168.1.10"), "192-168-1-10");
        assert_eq!(identifier_from_address("Desk-PC"), "desk-pc");
        assert_eq!(identifier_from_address("fe80::1%eth0"), "fe80--1-eth0");
        assert_eq!(identifier_from_address("..."), "device");
        assert_eq!(identifier_from_address(""), "device");
    }

    #[test]
    fn the_identifiers_hold_no_path_character() {
        for written in ["../escape", "a/b\\c", "C:\\Windows", "strange hôte"] {
            let id = identifier_from_address(written);
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{written} gives {id}"
            );
        }
    }
}
