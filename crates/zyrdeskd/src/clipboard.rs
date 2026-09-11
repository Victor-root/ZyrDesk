//! What is on this computer's clipboard, and what it is being given.
//!
//! The same blindness as the pointer beside it, and the same answer. A
//! clipboard belongs to a window station; a service sits on one carrying
//! no screen, no desktop and no clipboard, and the station that has one
//! belongs to another session entirely. It cannot be opened from here,
//! and no right makes it so. So this program is started again in the
//! session that owns the screen, and reads and writes from there.
//!
//! That helper and this service pass two things through files, for the
//! reason everything between these two programs is a file: it can be read
//! with the eyes, and it survives whoever wrote it. What this computer
//! has, written by the helper and read here; and what it is to be given,
//! written here and picked up by the helper.
//!
//! Each of those is two files, a line that names and a file beside it
//! that holds. The service looks several times a second, and a picture
//! weighs a few hundred thousand bytes: what is looked at that often has
//! to be two words. The bytes are written first and the line after them,
//! so a line naming bytes names bytes that are already there, and a
//! reader that arrives between the two writes reads a line that does not
//! match and comes back a moment later.
//!
//! The helper only runs while somebody is asking, exactly like the
//! pointer's. A computer nobody is watching has its clipboard to itself.

// Outside Windows nothing calls this module: the service does not exist
// there. The shape of it stays compiled and tested everywhere, and the
// clipboard itself is the one part that cannot be.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::clipboard::{Clip, Head, Stamp};
use zyr_proto::log::Log;
use zyr_proto::paths;

/// How often the helper looks at the clipboard.
///
/// Slower than the pointer beside it by a good margin, and it is the
/// right way round: a shape is wanted within a drawn frame, while what
/// somebody copied is wanted before their hand reaches the other
/// keyboard. A fifth of a second, on a question that has to answer
/// within half of one.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// How long a helper lives before it goes home of its own accord.
///
/// It is what stops one outliving the service that started it: nobody
/// ends it, it simply ends.
const HELPER_LIVES: Duration = Duration::from_secs(10);

/// How late is too late to count on the one that is running.
///
/// Another is started before the last has ended, so the reading never
/// stops between two of them.
const START_ANOTHER_AFTER: Duration = Duration::from_secs(7);

/// How long the service goes on keeping a helper after the last question.
const AFTER_THE_LAST_QUESTION: Duration = Duration::from_secs(4);

/// How long something just given is waited on before it is given up as
/// not having landed.
///
/// A helper takes a moment to notice, put it on, and say so. For that
/// moment this computer says nothing about its own clipboard, rather
/// than say what was on it before and have the far computer push that
/// back. Past this, the helper is gone or refused, and saying nothing
/// for ever would be a clipboard that never shares again.
const SETTLES_WITHIN: Duration = Duration::from_secs(3);

/// Whether the thread that keeps a helper alive is running.
static KEEPING: AtomicBool = AtomicBool::new(false);

/// When the last question came, so the keeper knows when to stop.
static ASKED: Mutex<Option<Instant>> = Mutex::new(None);

/// What was last given to this computer's clipboard, and when, until the
/// helper says it is really there.
static GIVEN: Mutex<Option<(Stamp, Instant)>> = Mutex::new(None);

/// What was last found too heavy to cross, so it is said once and not at
/// every turn of every session.
static TOO_LARGE: Mutex<Option<Stamp>> = Mutex::new(None);

/// What this computer has on its clipboard.
///
/// Never blocks and never fails: what comes back is the last thing the
/// helper wrote, which is at most one look old.
///
/// Nothing means one of three things, and the far computer does the same
/// thing about all three, which is why they are one answer: this
/// clipboard is empty, no helper has read it yet, or something was just
/// put on it and has not landed. In none of those has this computer
/// anything to say, and saying nothing is never read as « empty yours ».
pub fn what_this_computer_has(log: &Log) -> Option<Clip> {
    *ASKED.lock().expect("dernière question") = Some(Instant::now());
    if !KEEPING.swap(true, Ordering::SeqCst) {
        keep_a_helper(log.clone());
    }
    let clip =
        written_clip(&paths::clipboard_here()).filter(|clip| !more_than_it_carries(clip, log));
    let mut given = GIVEN.lock().expect("ce qui vient d'être donné");
    match *given {
        // It landed: the helper read back the very thing that was put on,
        // and from here on it is simply what this computer has.
        Some((stamp, _)) if clip.as_ref().is_some_and(|clip| clip.stamp() == stamp) => {
            *given = None;
            clip
        }
        Some((_, when)) if when.elapsed() < SETTLES_WITHIN => None,
        // It did not land in the time a helper takes to notice, put it on
        // and say so. The order is taken away rather than left there: a
        // helper that will not take it would go on trying every fifth of
        // a second for as long as the session lasts, and neither the
        // clipboard nor the journal is any better for that.
        Some(_) => {
            *given = None;
            forget_the_pair(&paths::clipboard_wanted());
            log.write("clipboard: what came from the far computer never reached this clipboard");
            clip
        }
        None => clip,
    }
}

/// Puts that on this computer's clipboard, through the helper.
///
/// Written down and not waited for: whoever asks is answering the far
/// computer on a channel every other question of the session queues
/// behind, and a clipboard that lands a fifth of a second later lands
/// before any hand reaches for it.
pub fn give_it(clip: &Clip, log: &Log) -> Result<(), String> {
    write_the_pair(&paths::clipboard_wanted(), clip)
        .map_err(|e| format!("le presse-papiers n'a pas pu être posé : {e}"))?;
    *GIVEN.lock().expect("ce qui vient d'être donné") = Some((clip.stamp(), Instant::now()));
    log.write(&format!(
        "clipboard: {} is going on this computer's clipboard",
        clip.in_words()
    ));
    Ok(())
}

/// Whether that is more than a session carries, said once when it is.
///
/// The one rule about weight, and it lives here because here is the one
/// door a clip on this computer goes through on its way out, whether this
/// computer is the one watching or the one being watched. Past the
/// ceiling, what somebody copied would hold up the picture for seconds on
/// a link that has to carry both, to paste something they almost
/// certainly have on the other machine already.
///
/// Said once per thing and not at every turn: this is asked several times
/// a second, and a line a second for as long as a large picture sits on
/// somebody's clipboard is a journal with nothing else in it.
fn more_than_it_carries(clip: &Clip, log: &Log) -> bool {
    if !clip.too_large() {
        return false;
    }
    let mut said = TOO_LARGE.lock().expect("ce qui ne passe pas");
    if said.replace(clip.stamp()) != Some(clip.stamp()) {
        log.write(&format!(
            "clipboard: {} is more than a session carries, and stays on this computer",
            clip.in_words()
        ));
    }
    true
}

/// Whether the last question is far enough behind to stop.
fn nobody_is_asking() -> bool {
    ASKED
        .lock()
        .expect("dernière question")
        .is_none_or(|asked| asked.elapsed() > AFTER_THE_LAST_QUESTION)
}

/// Keeps a helper running in the session that owns the screen, for as
/// long as anybody is asking.
///
/// A thread of its own because starting a program in another session
/// takes milliseconds, and the threads that answer the far computer are
/// shared with everything else this service does.
#[cfg(windows)]
fn keep_a_helper(log: Log) {
    std::thread::spawn(move || {
        log.write("clipboard: a session is sharing this computer's clipboard");
        let mut started: Option<Instant> = None;
        let mut refused = false;
        while !nobody_is_asking() {
            if started.is_none_or(|at| at.elapsed() > START_ANOTHER_AFTER) {
                match crate::session::start_carrying_the_clipboard() {
                    Ok(()) => {
                        if started.is_none() {
                            log.write(
                                "clipboard: reading it from the session that owns the screen",
                            );
                        }
                        refused = false;
                        started = Some(Instant::now());
                    }
                    // Said once and not every turn: a machine at its
                    // sign-in screen has no session to read from, and
                    // that is a state it can sit in for hours.
                    Err(e) => {
                        if !refused {
                            refused = true;
                            log.write(&format!(
                                "clipboard: nothing can reach the clipboard here: {e}"
                            ));
                        }
                        started = None;
                    }
                }
            }
            std::thread::sleep(LOOK_EVERY);
        }
        log.write("clipboard: nobody is sharing it any more");
        // Both pairs go with the asking. What this computer had copied
        // is no more a session's business once the session has gone, and
        // the next one starts on what is really on the clipboard rather
        // than on what the last one left written down.
        forget_the_pair(&paths::clipboard_here());
        forget_the_pair(&paths::clipboard_wanted());
        *GIVEN.lock().expect("ce qui vient d'être donné") = None;
        KEEPING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
fn keep_a_helper(_log: Log) {
    KEEPING.store(false, Ordering::SeqCst);
}

/// Reads and writes this computer's clipboard, until its time is up.
///
/// This is the helper, and it only ever runs in the session that owns the
/// screen: started anywhere else it reads a window station with no
/// clipboard on it. It ends by itself so that nothing has to end it.
#[cfg(windows)]
pub fn carry_the_clipboard_here() {
    let until = Instant::now() + HELPER_LIVES;
    let mut counted: Option<u32> = None;
    let mut written: Option<Stamp> = None;
    // What went wrong last, so that a clipboard held by another program,
    // or a picture this machine's imaging will not take, is one line and
    // not five a second for as long as the session lasts.
    let mut complained: Option<String> = None;
    while Instant::now() < until {
        if let Some(wanted) = written_clip(&paths::clipboard_wanted()) {
            match zyr_clipboard::hold_this(&wanted) {
                // Read back on the very next turn, because putting
                // something on a clipboard is what changes it: the line
                // this helper then writes is what tells the service the
                // giving landed.
                Ok(missing) => {
                    forget_the_pair(&paths::clipboard_wanted());
                    for what in missing {
                        say_once(&mut complained, &format!("clipboard: {what}"));
                    }
                }
                // Left where it is, so the next turn tries again: a
                // clipboard held by another program for a moment is the
                // ordinary case and not a fault. The service takes the
                // order away when it has waited long enough, which is
                // what bounds this.
                Err(e) => say_once(
                    &mut complained,
                    &format!("clipboard: it would not take what it was given: {e}"),
                ),
            }
        }

        // The counter is the cheap half of this: reading a picture every
        // fifth of a second to discover it has not changed would cost a
        // few million bytes each time. Nought is a system that would not
        // say, and then the clipboard itself is read every turn, which is
        // what it costs to be right rather than fast.
        let counter = zyr_clipboard::times_it_changed();
        if counter != 0 && counted == Some(counter) {
            std::thread::sleep(LOOK_EVERY);
            continue;
        }
        counted = Some(counter);
        match zyr_clipboard::what_it_holds() {
            Ok(Some(clip)) if written != Some(clip.stamp()) => {
                match write_the_pair(&paths::clipboard_here(), &clip) {
                    Ok(()) => {
                        written = Some(clip.stamp());
                        complained = None;
                        // One line per thing copied, which is the right
                        // rate: a clipboard changes a few times an hour.
                        // Without it, a picture that never crossed and a
                        // clipboard nobody touched leave exactly the same
                        // trace, which is none.
                        said(&format!(
                            "clipboard: this computer now holds {}",
                            clip.in_words()
                        ));
                    }
                    Err(e) => say_once(
                        &mut complained,
                        &format!("clipboard: what is on it could not be written down: {e}"),
                    ),
                }
            }
            // Nothing this product carries. Said with what the clipboard
            // was really offering, because that is the one thing that
            // tells « nobody copied anything » from « somebody copied
            // something this product does not take », and the two look
            // identical from everywhere else.
            Ok(None) => say_once(
                &mut complained,
                &format!(
                    "clipboard: nothing on it that crosses ; it holds {}",
                    zyr_clipboard::what_is_offered()
                ),
            ),
            Ok(Some(_)) => {}
            Err(e) => say_once(
                &mut complained,
                &format!(
                    "clipboard: it would not be read: {e} ; it holds {}",
                    zyr_clipboard::what_is_offered()
                ),
            ),
        }
        std::thread::sleep(LOOK_EVERY);
    }
}

/// Writes that down unless it is word for word what was written last.
///
/// Everything in the loop above can go wrong at every turn, five times a
/// second, for as long as whatever is wrong lasts. What is worth reading
/// is that it went wrong and what it said, once.
#[cfg(windows)]
fn say_once(last: &mut Option<String>, what: &str) {
    if last.as_deref() == Some(what) {
        return;
    }
    *last = Some(what.to_string());
    said(what);
}

#[cfg(not(windows))]
pub fn carry_the_clipboard_here() {}

/// Writes into the service's own journal from the helper, which has no
/// journal of its own.
#[cfg(windows)]
fn said(what: &str) {
    if let Ok(log) = Log::open(&crate::service::log_path()) {
        log.write(what);
    }
}

/// Reads a clip from the pair of files that name it and hold it.
///
/// Nothing when the two do not agree, which is a reading taken between
/// the two writes and not a fault: the turn after has both halves.
fn written_clip(named: &std::path::Path) -> Option<Clip> {
    let head: Head = std::fs::read_to_string(named).ok()?.parse().ok()?;
    let bytes = std::fs::read(paths::beside(named)).ok()?;
    head.matches(&bytes).then(|| Clip::new(head.kind, bytes))
}

/// Writes a clip as that pair of files.
///
/// The bytes first and the line naming them after, which is the whole of
/// what makes the reading above safe: a line is never there before what
/// it names.
fn write_the_pair(named: &std::path::Path, clip: &Clip) -> std::io::Result<()> {
    zyr_proto::files::replace_bytes(&paths::beside(named), clip.bytes())?;
    zyr_proto::files::replace(named, &format!("{}\n", clip.named()))
}

/// Takes both files away, in the order that leaves nothing readable
/// behind: the line first, since it is the line that says there is
/// anything to read.
fn forget_the_pair(named: &std::path::Path) {
    let _ = std::fs::remove_file(named);
    let _ = std::fs::remove_file(paths::beside(named));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-clipboard-{}-{name}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn un_clip_ecrit_se_relit() {
        let folder = fresh_folder("aller-retour");
        let named = folder.join("clipboard-here.txt");
        let clip = Clip::picture(vec![0x89, b'P', b'N', b'G', 0, 255]);

        assert_eq!(written_clip(&named), None);
        write_the_pair(&named, &clip).unwrap();
        assert_eq!(written_clip(&named), Some(clip));

        forget_the_pair(&named);
        assert_eq!(written_clip(&named), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn une_ligne_qui_ne_correspond_pas_a_ses_octets_est_sautee() {
        // Le service lit entre les deux écritures : ce moment-là doit
        // être « rien à dire ce tour-ci » et jamais la moitié de deux
        // choses collée au presse-papiers d'en face.
        let folder = fresh_folder("entre-deux");
        let named = folder.join("clipboard-here.txt");
        write_the_pair(&named, &Clip::text("le nouveau")).unwrap();
        std::fs::write(paths::beside(&named), b"l'ancien").unwrap();

        assert_eq!(written_clip(&named), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn personne_ne_demande_tant_que_personne_n_a_demande() {
        // C'est ce qui décide qu'un ordinateur que personne ne regarde
        // garde son presse-papiers pour lui : sans question, plus aucun
        // assistant n'est relancé et le dernier s'éteint tout seul.
        *ASKED.lock().unwrap() = None;
        assert!(nobody_is_asking());
        *ASKED.lock().unwrap() = Some(Instant::now());
        assert!(!nobody_is_asking());
        *ASKED.lock().unwrap() = Instant::now().checked_sub(AFTER_THE_LAST_QUESTION * 2);
        assert!(nobody_is_asking());
    }

    #[test]
    fn un_assistant_est_relance_avant_que_le_precedent_ne_meure() {
        // Sans ce recouvrement, la lecture s'arrêterait entre deux
        // assistants et ce qu'on copie pendant ce temps-là ne partirait
        // jamais.
        assert!(
            START_ANOTHER_AFTER < HELPER_LIVES,
            "un assistant doit être relancé avant la fin du précédent"
        );
    }
}
