//! What Windows wants, and what the person wanted over it.
//!
//! Three choices: follow Windows, force light, force dark. Following is
//! the default, and it has to follow for real, including while the
//! window is open: everything else on the machine switches the moment
//! Windows does, and a product that does not is the odd one out.
//!
//! # Where the choice lives
//!
//! In a file beside the other things this machine remembers about
//! itself, read once when the program opens. It used to live in the web
//! view's own store, which went with the web view; and a store the
//! product cannot read from Rust would have left the window drawing
//! itself in one theme and its frame in another.
//!
//! # And Windows is asked directly
//!
//! The same value the toolkit itself reads, from the same place, and
//! watched rather than sampled: Windows raises a hand when it changes,
//! and the window is redrawn. Nothing here polls a registry on a timer
//! for an answer that changes twice a day.

// Tout ce qui est ici est demandé par l'accueil, que ce programme dessine
// lui-même, et ce qui dessine n'existe que sous Windows comme les
// fenêtres qu'il habille. Ailleurs, rien ne pose ces questions : le
// fichier reste compilé et vérifié, il n'est simplement appelé par
// personne.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::AppHandle;

/// The three answers, spelled as the file spells them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Choix {
    Systeme,
    Clair,
    Sombre,
}

impl Choix {
    /// The three, in the order the settings screen offers them.
    pub const ALL: [Choix; 3] = [Choix::Systeme, Choix::Clair, Choix::Sombre];

    /// What the file writes.
    fn name(self) -> &'static str {
        match self {
            Choix::Systeme => "systeme",
            Choix::Clair => "clair",
            Choix::Sombre => "sombre",
        }
    }

    /// What a person reads.
    pub fn word(self) -> &'static str {
        match self {
            Choix::Systeme => "Système",
            Choix::Clair => "Clair",
            Choix::Sombre => "Sombre",
        }
    }

    fn read(said: &str) -> Option<Choix> {
        Choix::ALL.into_iter().find(|choix| choix.name() == said)
    }

    fn rank(self) -> u8 {
        match self {
            Choix::Systeme => 0,
            Choix::Clair => 1,
            Choix::Sombre => 2,
        }
    }

    fn of(rank: u8) -> Choix {
        Choix::ALL
            .into_iter()
            .find(|choix| choix.rank() == rank)
            .unwrap_or(Choix::Systeme)
    }
}

/// What was chosen, read from the file once and held in a number from
/// then on: this is asked for on the thread that draws, where nothing
/// may touch a disk.
static CHOISI: AtomicU8 = AtomicU8::new(0);

/// And what Windows wants, read by the thread that watches it and by
/// nobody else.
///
/// One question to the registry per switch, and not one per picture
/// drawn: what draws asks this hundreds of times while a hand moves
/// across the window.
static WINDOWS_VEUT_CLAIR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Reads back what was chosen the last time somebody chose.
///
/// Once, when the program opens, and before the window is shown: a
/// window that opened in the wrong theme even for one beat would be seen
/// doing it.
pub fn what_was_chosen() {
    WINDOWS_VEUT_CLAIR.store(windows_wants_light(), Ordering::Relaxed);
    let path = zyr_proto::paths::chosen_theme();
    let Ok(written) = std::fs::read_to_string(&path) else {
        return;
    };
    let said = written
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("theme")?.trim().strip_prefix('='));
    if let Some(choix) = said.and_then(|said| Choix::read(said.trim())) {
        CHOISI.store(choix.rank(), Ordering::Relaxed);
    }
}

/// What was chosen: follow Windows unless somebody said otherwise.
pub fn chosen() -> Choix {
    Choix::of(CHOISI.load(Ordering::Relaxed))
}

/// Whether the interface is light right now.
///
/// What was chosen, or what Windows wants when nothing was. Everything
/// that draws asks this and nothing else: one answer for the whole
/// product, so no screen can hold an opinion of its own.
pub fn light() -> bool {
    match chosen() {
        Choix::Clair => true,
        Choix::Sombre => false,
        Choix::Systeme => WINDOWS_VEUT_CLAIR.load(Ordering::Relaxed),
    }
}

/// Takes a new choice, writes it down, and puts it on the window.
pub fn choose(choix: Choix) {
    CHOISI.store(choix.rank(), Ordering::Relaxed);
    let written = format!(
        "# Le thème de l'interface ZyrDesk : systeme, clair ou sombre.\n\
         # « systeme » suit ce que Windows demande.\n\
         # Écrit par ZyrDesk, peut se corriger à la main.\n\
         theme = {}\n",
        choix.name()
    );
    if let Err(e) = zyr_proto::files::replace(&zyr_proto::paths::chosen_theme(), &written) {
        crate::journal::note(&format!("thème non retenu : {e}"));
    }
    on_the_window();
}

/// Matches the window's frame to what the interface is.
///
/// The frame belongs to Windows and not to us, and it is the one part of
/// the window this program does not draw: without this, a light
/// interface would keep a dark title bar, which is exactly the kind of
/// seam a product is judged on.
///
/// The colour it comes to and not the choice that led there: the frame
/// is told what to be, and « follow Windows » is answered by this
/// program, which is the only one that knows both halves of the
/// question.
pub fn on_the_window() {
    crate::fenetre::habille(light());
}

/// Follows what Windows wants for as long as the program runs, and has
/// the window redrawn each time it changes.
///
/// On a thread of its own, asleep the whole time: Windows wakes it. The
/// alternative is asking the registry on a timer, which is a question
/// asked a thousand times for an answer that changes twice a day.
#[cfg(windows)]
pub fn watch(app: AppHandle) {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RegCloseKey,
        RegNotifyChangeKeyValue, RegOpenKeyExW,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};

    std::thread::spawn(move || {
        let name = wide(PERSONALIZE);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the name outlives the call, and the slot for the key it
        // hands back is ours.
        let opened =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, name.as_ptr(), 0, KEY_NOTIFY, &mut key) };
        if opened != 0 {
            note(&format!(
                "le thème de Windows ne sera pas suivi : sa clé ne s'ouvre pas (code {opened})"
            ));
            return;
        }
        // SAFETY: an unnamed event of ours, reset by hand between waits.
        let woken = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if woken.is_null() {
            // SAFETY: a key this thread opened, closed once.
            unsafe { RegCloseKey(key) };
            note("le thème de Windows ne sera pas suivi : pas de réveil à poser");
            return;
        }

        let mut said = windows_wants_light();
        loop {
            // SAFETY: the key and the event are both live, and the call
            // arms one notification which the wait below collects.
            let armed =
                unsafe { RegNotifyChangeKeyValue(key, 0, REG_NOTIFY_CHANGE_LAST_SET, woken, 1) };
            if armed != 0 {
                note(&format!(
                    "le thème de Windows n'est plus suivi : {armed} en posant la garde"
                ));
                break;
            }
            // SAFETY: the event is live and the wait has no deadline.
            if unsafe { WaitForSingleObject(woken, INFINITE) } != WAIT_OBJECT_0 {
                break;
            }
            // The key carries more than this one value: a change to any
            // of it wakes this thread, and only a change to the answer is
            // worth a word to anybody.
            let now = windows_wants_light();
            if now != said {
                said = now;
                WINDOWS_VEUT_CLAIR.store(now, Ordering::Relaxed);
                note(&format!(
                    "Windows demande maintenant une interface {}",
                    if now { "claire" } else { "sombre" }
                ));
                // Only when nobody has chosen: a window forced to a theme
                // does not follow, and redrawing it here would repaint the
                // same picture in the same colours.
                if chosen() == Choix::Systeme {
                    on_the_window();
                    crate::accueil::redraw(&app);
                }
            }
        }

        // SAFETY: both were made by this thread and are closed once.
        unsafe {
            CloseHandle(woken);
            RegCloseKey(key);
        }
    });
}

/// Elsewhere there is no Windows to follow, and no window to match.
#[cfg(not(windows))]
pub fn watch(_app: AppHandle) {}

/// Where Windows keeps what it wants applications to look like.
///
/// The same key and the same value the toolkit reads, deliberately: two
/// programs answering that question differently in the same window is
/// the seam this whole file exists to close. And it is the applications
/// setting, not the system one: Windows lets the two differ, and what a
/// window is asked to look like is the first.
#[cfg(windows)]
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
#[cfg(windows)]
const APPS_USE_LIGHT: &str = "AppsUseLightTheme";

/// Whether Windows is asking for a light interface.
///
/// Light when it says nothing, which is what Windows itself does with a
/// missing value.
#[cfg(windows)]
fn windows_wants_light() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let key = wide(PERSONALIZE);
    let value = wide(APPS_USE_LIGHT);
    let mut found = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: both names outlive the call, and the four bytes written
    // into are those of the number beside them.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut found).cast::<std::ffi::c_void>(),
            &mut size,
        )
    };
    code != 0 || found != 0
}

#[cfg(not(windows))]
fn windows_wants_light() -> bool {
    true
}

/// Zero-terminated string, the way Windows expects them.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

#[cfg(windows)]
fn note(what: &str) {
    crate::journal::note(what);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_is_written_and_read_back_as_itself() {
        // Le fichier est le seul endroit où le choix survit à la fenêtre :
        // un nom qui ne se relit pas est un choix perdu au redémarrage.
        for choix in Choix::ALL {
            assert_eq!(Choix::read(choix.name()), Some(choix));
            assert!(!choix.word().is_empty());
        }
        assert_eq!(Choix::read("bleu"), None);
    }

    #[test]
    fn a_choice_survives_the_number_it_is_held_as() {
        for choix in Choix::ALL {
            assert_eq!(Choix::of(choix.rank()), choix);
        }
        // Ce que dit un fichier abîmé : suivre Windows, comme au premier
        // démarrage.
        assert_eq!(Choix::of(9), Choix::Systeme);
    }
}
