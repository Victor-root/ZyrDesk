//! The speakers of this computer, quiet while somebody watches it from
//! afar.
//!
//! A computer being controlled goes on playing out loud into a room
//! nobody is in, while the same sound is also travelling down the
//! session. Whoever asked for that in the settings gets the room
//! silenced instead, and the session keeps its sound: what the host
//! engine records is the mix Windows hands to the sound card, copied
//! before the card applies its own mute.
//!
//! The engines' own answer to this is a second sound card that no cable
//! leads to, published by somebody else and installed behind the
//! person's back. Nothing of the sort is needed here, and nothing of the
//! sort is installed.
//!
//! # What is owed
//!
//! Two states are kept apart on purpose. What this service last **asked**
//! of the speakers is not the same as what it **owes** them: speakers the
//! person had already muted are left exactly as they are, and giving
//! that sound back at the end of a session would be undoing something
//! this product never did.
//!
//! What is owed outlives the service, in one small file. A machine
//! switched off in the middle of a session, or a service that fell over,
//! would otherwise stay silent for ever with nothing anywhere saying
//! why: Windows remembers a muted device across a restart. The file is
//! read when the service starts and the sound is given back on the first
//! turn of the watch.

use std::sync::atomic::{AtomicBool, Ordering};

use zyr_proto::log::Log;
use zyr_proto::paths;

/// Whether the last thing this service asked of the speakers was to be
/// quiet.
static ASKED: AtomicBool = AtomicBool::new(false);

/// Whether the sound is owed back, which is to say whether this service
/// is what silenced them.
static OWED: AtomicBool = AtomicBool::new(false);

/// Whether somebody was watching this computer at the last look.
///
/// Kept for the journal and for nothing else. Without it, a computer
/// whose speakers stay on says nothing at all about why: the setting
/// being off, the session never being counted and the mute being refused
/// all read alike, which is to say they do not read.
static WATCHED: AtomicBool = AtomicBool::new(false);

/// Brings the speakers into line with what is going on, and does nothing
/// at all when they already are.
///
/// Written as « let it be so » rather than « do it now », and called at
/// every turn of the watch: a refusal then costs nothing but one line,
/// since the next turn tries again. Which is what makes the sound come
/// back on a machine that was left silent, even when nobody is signed in
/// yet at the moment the service starts.
pub fn keep_in_step(wanted: bool, a_session_is_open: bool, log: &Log) {
    // One line per session, whatever is decided, and this is the whole
    // reason both facts are handed over separately rather than already
    // multiplied together: what is worth reading is not that the
    // speakers did nothing, it is why.
    if WATCHED.swap(a_session_is_open, Ordering::Relaxed) != a_session_is_open {
        log.write(&format!(
            "somebody {} watching this computer, and its speakers are {}",
            if a_session_is_open {
                "is now"
            } else {
                "is no longer"
            },
            if wanted {
                "to be silent while they do"
            } else {
                "left alone, nobody having asked for that"
            }
        ));
    }

    let quiet = wanted && a_session_is_open;
    if quiet == ASKED.load(Ordering::Relaxed) {
        return;
    }
    // Giving the sound back is only ours to do when taking it was.
    if !quiet && !OWED.load(Ordering::Relaxed) {
        ASKED.store(false, Ordering::Relaxed);
        return;
    }
    match moved(quiet) {
        Ok(really) => {
            ASKED.store(quiet, Ordering::Relaxed);
            what_is_owed(quiet && really, log);
            log.write(if quiet {
                "the speakers of this computer are silent while it is being watched"
            } else {
                "the speakers of this computer play again"
            });
        }
        // Said and not insisted on here: the next turn of the watch comes
        // round in a moment and asks again.
        Err(e) => log.write(&format!("the speakers would not move: {e}")),
    }
}

/// Picks up a silence a previous run of this service left behind.
///
/// Nothing is given back here. What this does is remember that something
/// is owed, so the first turn of the watch gives it back the ordinary
/// way: at the moment the service starts there may be nobody signed in
/// at all, and the speakers can only be reached from the session that
/// owns the screen.
///
/// Only the service asks this, and a service is a Windows thing.
#[cfg(windows)]
pub fn pick_up_where_it_was_left(log: &Log) {
    if !paths::hushed_speakers().exists() {
        return;
    }
    log.write("this computer was left silent by a session that did not end properly");
    ASKED.store(true, Ordering::Relaxed);
    OWED.store(true, Ordering::Relaxed);
    WATCHED.store(true, Ordering::Relaxed);
}

/// Writes down whether the sound is owed, so a service that never comes
/// back still leaves the answer behind.
fn what_is_owed(owed: bool, log: &Log) {
    OWED.store(owed, Ordering::Relaxed);
    let path = paths::hushed_speakers();
    let outcome = if owed {
        zyr_proto::files::replace(
            &path,
            "# ZyrDesk a coupé les enceintes de cet ordinateur pour la durée\n\
             # d'une session, et doit les rallumer à la fin. Ce fichier est\n\
             # ce qui s'en souvient si le service ne va pas jusqu'au bout.\n\
             # Il disparaît tout seul dès que le son est rendu.\n",
        )
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    };
    if let Err(e) = outcome {
        log.write(&format!(
            "what the speakers are owed is not written down: {e}"
        ));
    }
}

/// Moves them, and says whether they were really doing the opposite.
///
/// From the session that owns the screen and never from the service's
/// own: which device the desktop plays to depends on who is signed in,
/// and a service asking the question in its own session names a device
/// nobody is listening to.
#[cfg(windows)]
fn moved(quiet: bool) -> std::io::Result<bool> {
    crate::session::set_the_speakers(quiet)
}

/// Outside Windows there is no service and no session on a screen. The
/// rest of this file stays compiled and tested everywhere, having
/// nothing platform-specific about it.
#[cfg(not(windows))]
fn moved(_quiet: bool) -> std::io::Result<bool> {
    Err(std::io::Error::other(
        "cet ordinateur n'a pas d'enceintes à couper ainsi",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ce_qui_est_du_n_est_pas_ce_qui_a_ete_demande() {
        // Des enceintes déjà muettes avant la session sont laissées
        // telles quelles : les rallumer à la fin défairait un geste que
        // ce produit n'a pas fait.
        ASKED.store(true, Ordering::Relaxed);
        OWED.store(false, Ordering::Relaxed);
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-speakers-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let log = Log::open(&folder.join("service.log")).unwrap();

        // Rien n'est demandé au système : la fonction s'arrête avant.
        // Hors de Windows, toucher aux enceintes échoue toujours, donc
        // « demandé » retombé veut dire qu'on n'y a pas touché.
        keep_in_step(false, false, &log);
        assert!(!ASKED.load(Ordering::Relaxed));
        assert!(!OWED.load(Ordering::Relaxed));

        std::fs::remove_dir_all(&folder).ok();
    }
}
