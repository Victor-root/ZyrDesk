//! What the person asked of this computer, kept across restarts.
//!
//! Remote access is a decision, not a state: turning it off has to
//! survive a reboot, or the machine would let itself be reached again
//! the next morning without anyone saying so. The same goes for what a
//! session looks like: a quality chosen once is not to be chosen again
//! at every launch. It is written to a plain file, in the same spirit as
//! the list of authorised devices: readable and correctable in a text
//! editor, holding no secret.
//!
//! A file that cannot be read is not an emergency. What matters is
//! choosing the safe answer when in doubt, and remote access being off
//! is the safe answer: a computer nobody can reach is an inconvenience,
//! a computer reachable against its owner's wishes is not.
//!
//! A line nobody understands is skipped rather than fatal. A file
//! written by a newer ZyrDesk, or mangled by hand, then loses one
//! setting instead of all of them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use zyr_proto::session::Preferred;

/// Keys, in the order they are written.
const REMOTE_ACCESS: &str = "remote_access";
const QUALITY: &str = "quality";
const CODEC: &str = "codec";
const DISPLAY: &str = "display";
const ABSOLUTE_MOUSE: &str = "absolute_mouse";
const STATS_OVERLAY: &str = "stats_overlay";

/// What the file says, when it says anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferences {
    /// Whether this computer accepts being controlled.
    pub remote_access: bool,
    /// What a session opened from this computer looks like.
    pub preferred: Preferred,
}

impl Default for Preferences {
    /// What a computer does before anyone has said otherwise.
    ///
    /// Reachable, because the product is installed to be reached: a
    /// first install that answered nothing would look broken.
    fn default() -> Self {
        Self {
            remote_access: true,
            preferred: Preferred::default(),
        }
    }
}

/// The preferences, and the only place they are written from.
///
/// Two settings sharing one file means neither may be saved alone: a
/// write that only knew about remote access would wipe the quality, and
/// the other way round. Everything therefore goes through here, which
/// reads the file once and writes it whole.
#[derive(Clone)]
pub struct Remembered {
    path: Arc<PathBuf>,
    now: Arc<Mutex<Preferences>>,
}

impl Remembered {
    /// Picks up what was asked for last time.
    pub fn at(path: PathBuf) -> Self {
        let now = from_disk(&path);
        Self {
            path: Arc::new(path),
            now: Arc::new(Mutex::new(now)),
        }
    }

    pub fn read(&self) -> Preferences {
        *self.now.lock().expect("réglages")
    }

    /// Whether this computer is meant to be reachable at all.
    ///
    /// What was asked for, which is not the same as what the engine has
    /// reached. Between the two sits the time it takes to start, and
    /// that gap is what the interface shows as « démarrage en cours »
    /// rather than pretending the switch lied.
    pub fn remote_access(&self) -> bool {
        self.read().remote_access
    }

    pub fn set_remote_access(&self, on: bool) -> io::Result<()> {
        self.change(|preferences| preferences.remote_access = on)
    }

    pub fn set_preferred(&self, preferred: Preferred) -> io::Result<()> {
        self.change(|preferences| preferences.preferred = preferred)
    }

    /// Writes the decision down before honouring it: a computer that
    /// obeyed but forgot would let itself be reached again tomorrow.
    fn change(&self, how: impl FnOnce(&mut Preferences)) -> io::Result<()> {
        let mut now = self.now.lock().expect("réglages");
        let mut asked = *now;
        how(&mut asked);
        onto_disk(&self.path, asked)?;
        *now = asked;
        Ok(())
    }
}

/// Reads the file, falling back to the defaults.
///
/// Named apart from `Remembered::read`, which hands back what is
/// already in hand: only this one touches the disk.
fn from_disk(path: &Path) -> Preferences {
    match fs::read_to_string(path) {
        Ok(text) => parsed(&text),
        // Never written yet, or unreadable. Either way there is nothing
        // to honour but the defaults, and refusing to start over a
        // preferences file would be absurd.
        Err(_) => Preferences::default(),
    }
}

fn onto_disk(path: &Path, preferences: Preferences) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, rendered(preferences))
}

fn rendered(preferences: Preferences) -> String {
    let preferred = preferences.preferred;
    format!(
        "# Réglages du service ZyrDesk.\n\
         # Ce fichier est écrit par le produit et peut se corriger à la main.\n\
         \n\
         # Cet ordinateur accepte d'être contrôlé à distance.\n\
         {REMOTE_ACCESS} = {}\n\
         \n\
         # Ce à quoi ressemble une session ouverte depuis cet ordinateur.\n\
         # Qualité : smooth, balanced ou detailed.\n\
         {QUALITY} = {}\n\
         # Codec : auto, H.264, HEVC ou AV1.\n\
         {CODEC} = {}\n\
         # Affichage : fullscreen, borderless ou windowed.\n\
         {DISPLAY} = {}\n\
         # Souris du bureau plutôt que souris de jeu.\n\
         {ABSOLUTE_MOUSE} = {}\n\
         # Statistiques affichées par-dessus l'image.\n\
         {STATS_OVERLAY} = {}\n",
        yes_no(preferences.remote_access),
        preferred.quality,
        preferred.codec,
        preferred.display_mode,
        yes_no(preferred.absolute_mouse),
        yes_no(preferred.stats_overlay),
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// Reads a yes or a no, keeping what was already there when the value
/// says neither.
fn told(value: &str, so_far: bool) -> bool {
    match value {
        "yes" | "oui" | "true" | "1" => true,
        "no" | "non" | "false" | "0" => false,
        _ => so_far,
    }
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
        let (key, value) = (key.trim(), value.trim());
        let preferred = &mut preferences.preferred;
        match key {
            // Anything other than a plain no is read as yes: a line
            // mangled by hand must not quietly make the computer
            // unreachable.
            REMOTE_ACCESS => {
                preferences.remote_access = !matches!(value, "no" | "non" | "false" | "0");
            }
            // The rest is a comfort setting: a value nobody understands
            // leaves the default in place, and the session still opens.
            QUALITY => preferred.quality = value.parse().unwrap_or_default(),
            CODEC => preferred.codec = value.parse().unwrap_or_default(),
            DISPLAY => preferred.display_mode = value.parse().unwrap_or_default(),
            ABSOLUTE_MOUSE => preferred.absolute_mouse = told(value, preferred.absolute_mouse),
            STATS_OVERLAY => preferred.stats_overlay = told(value, preferred.stats_overlay),
            _ => {}
        }
    }
    preferences
}

#[cfg(test)]
mod tests {
    use super::*;

    use zyr_proto::session::{Codec, DisplayMode, Quality};

    fn temporary_file(what: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-prefs-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        fs::create_dir_all(&folder).unwrap();
        folder.join("preferences.conf")
    }

    fn chosen() -> Preferences {
        Preferences {
            remote_access: false,
            preferred: Preferred {
                quality: Quality::Detailed,
                codec: Codec::Hevc,
                display_mode: DisplayMode::Borderless,
                absolute_mouse: false,
                stats_overlay: true,
            },
        }
    }

    #[test]
    fn a_computer_answers_before_anyone_has_said_otherwise() {
        // A first install that let nobody in would look broken.
        assert!(Preferences::default().remote_access);
    }

    #[test]
    fn what_was_asked_survives_a_restart() {
        let path = temporary_file("survives");
        for preferences in [chosen(), Preferences::default()] {
            onto_disk(&path, preferences).unwrap();
            assert_eq!(from_disk(&path), preferences);
        }
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn saving_one_setting_does_not_wipe_the_other() {
        // Les deux vivent dans le même fichier : une écriture qui ne
        // connaîtrait qu'un des deux effacerait l'autre en silence.
        let path = temporary_file("ensemble");
        let remembered = Remembered::at(path.clone());

        remembered.set_preferred(chosen().preferred).unwrap();
        remembered.set_remote_access(false).unwrap();

        assert_eq!(from_disk(&path), chosen());
        assert_eq!(remembered.read(), chosen());

        // Et dans l'autre sens.
        remembered.set_remote_access(true).unwrap();
        assert_eq!(from_disk(&path).preferred, chosen().preferred);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn what_is_kept_is_what_the_next_start_picks_up() {
        let path = temporary_file("relance");
        let remembered = Remembered::at(path.clone());
        remembered.set_preferred(chosen().preferred).unwrap();
        remembered
            .set_remote_access(chosen().remote_access)
            .unwrap();
        // Le service redémarre : rien en mémoire, tout sur le disque.
        assert_eq!(Remembered::at(path.clone()).read(), chosen());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_file_is_not_a_failure() {
        assert_eq!(
            from_disk(Path::new("/nowhere/preferences.conf")),
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
    fn one_unreadable_line_does_not_cost_the_others() {
        // Un fichier écrit par un ZyrDesk plus récent, ou corrigé de
        // travers, ne doit pas remettre tout le reste à zéro.
        let text = "quality = ultra\ncodec = HEVC\nun-verbe-inconnu = 3\nstats_overlay = yes\n";
        let read = parsed(text).preferred;
        assert_eq!(read.quality, Quality::default());
        assert_eq!(read.codec, Codec::Hevc);
        assert!(read.stats_overlay);
    }

    #[test]
    fn what_is_written_can_be_read_back() {
        // The file is meant to be opened by a person: it is checked here
        // that what they would read is what the product understands.
        let rendered = rendered(chosen());
        assert!(rendered.contains("remote_access = no"), "{rendered}");
        assert!(rendered.contains("quality = detailed"), "{rendered}");
        assert!(rendered.contains("codec = HEVC"), "{rendered}");
        assert_eq!(parsed(&rendered), chosen());
    }
}
