//! What the person asked of this computer, kept across restarts.
//!
//! Remote access is a decision, not a state: turning it off has to
//! survive a reboot, or the machine would let itself be reached again
//! the next morning without anyone saying so. It is written to a plain
//! file, in the same spirit as the list of authorised devices: readable
//! and correctable in a text editor, holding no secret.
//!
//! A file that cannot be read is not an emergency. What matters is
//! choosing the safe answer when in doubt, and remote access being off
//! is the safe answer: a computer nobody can reach is an inconvenience,
//! a computer reachable against its owner's wishes is not.

use std::fs;
use std::io;
use std::path::Path;

/// Key remote access is written under.
const REMOTE_ACCESS: &str = "remote_access";

/// What the file says, when it says anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferences {
    pub remote_access: bool,
}

impl Default for Preferences {
    /// What a computer does before anyone has said otherwise.
    ///
    /// On, because the product is installed to be reached: a first
    /// install that answered nothing would look broken.
    fn default() -> Self {
        Self {
            remote_access: true,
        }
    }
}

/// Reads the preferences, falling back to the defaults.
pub fn read(path: &Path) -> Preferences {
    match fs::read_to_string(path) {
        Ok(text) => parsed(&text),
        // Never written yet, or unreadable. Either way there is nothing
        // to honour but the defaults, and refusing to start over a
        // preferences file would be absurd.
        Err(_) => Preferences::default(),
    }
}

pub fn write(path: &Path, preferences: Preferences) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, rendered(preferences))
}

fn rendered(preferences: Preferences) -> String {
    format!(
        "# Réglages du service ZyrDesk.\n\
         # Ce fichier est écrit par le produit et peut se corriger à la main.\n\
         \n\
         # Cet ordinateur accepte d'être contrôlé à distance.\n\
         {REMOTE_ACCESS} = {}\n",
        yes_no(preferences.remote_access)
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn parsed(text: &str) -> Preferences {
    let mut preferences = Preferences::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == REMOTE_ACCESS {
            // Anything other than a plain no is read as yes: a line
            // mangled by hand must not quietly make the computer
            // unreachable.
            preferences.remote_access = !matches!(value.trim(), "no" | "non" | "false" | "0");
        }
    }
    preferences
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_file(what: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-prefs-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        fs::create_dir_all(&folder).unwrap();
        folder.join("preferences.conf")
    }

    #[test]
    fn a_computer_answers_before_anyone_has_said_otherwise() {
        // A first install that let nobody in would look broken.
        assert!(Preferences::default().remote_access);
    }

    #[test]
    fn what_was_asked_survives_a_restart() {
        let path = temporary_file("survives");
        for asked in [false, true] {
            write(
                &path,
                Preferences {
                    remote_access: asked,
                },
            )
            .unwrap();
            assert_eq!(read(&path).remote_access, asked);
        }
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_file_is_not_a_failure() {
        assert_eq!(
            read(Path::new("/nowhere/preferences.conf")),
            Preferences::default()
        );
    }

    #[test]
    fn a_file_mangled_by_hand_leaves_the_computer_reachable() {
        // The other way round, a typo would take the machine off the
        // network and look exactly like a network fault.
        for text in [
            "",
            "n'importe quoi",
            "remote_access",
            "remote_access = peut-être",
        ] {
            assert!(parsed(text).remote_access, "sur « {text} »");
        }
    }

    #[test]
    fn only_a_plain_no_turns_it_off() {
        for text in [
            "remote_access = no",
            "remote_access=non",
            "  remote_access = 0  ",
        ] {
            assert!(!parsed(text).remote_access, "sur « {text} »");
        }
    }

    #[test]
    fn what_is_written_can_be_read_back() {
        // The file is meant to be opened by a person: it is checked here
        // that what they would read is what the product understands.
        let rendered = rendered(Preferences {
            remote_access: false,
        });
        assert!(rendered.contains("remote_access = no"), "{rendered}");
        assert!(!parsed(&rendered).remote_access);
    }
}
