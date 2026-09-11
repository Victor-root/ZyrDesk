//! What shape the pointer has on this computer.
//!
//! A desktop says what a click is about to do through the shape of the
//! pointer and through almost nothing else: an upright bar means the
//! click lands between two letters, a hand means something to follow, a
//! ring means wait. The computer watching draws its own pointer, so that
//! it answers the hand with no network in between; without this it would
//! answer with an arrow and nothing else, whatever is under it.
//!
//! One moment answers no shape at all, and it is the one moment this
//! computer draws its pointer into the picture itself: a window being
//! dragged. See `a_window_is_being_dragged`.
//!
//! Nothing here is done to the machine. It is a reading, and the whole
//! module exists because of where the reading has to happen. A service
//! sits in a session with no screen, no keyboard and no pointer, on a
//! window station carrying none of them, and the desktop that owns the
//! input belongs to another session entirely: it cannot be opened from
//! here, and no right makes it so. It is the same blindness that made
//! this computer answer that it had no screens, and it has the same
//! answer: this program is started again in the session that owns the
//! screen, and it reads from there.
//!
//! That helper writes one word to a file and the service reads it. A
//! file rather than anything cleverer, for the reason everything else
//! between these two programs is a file: it can be read with the eyes,
//! and it survives whoever wrote it.
//!
//! It only runs while somebody is asking. Its life is short and it is
//! started again for as long as the questions keep coming, so a service
//! that stops asking, or that stops altogether, leaves nothing behind for
//! more than a few seconds. A machine nobody is watching reads nothing.

// Outside Windows nothing calls this module: the service does not exist
// there. The shape of it stays compiled and tested everywhere, and the
// reading itself is the one part that cannot be.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::log::Log;
use zyr_proto::session::Pointer;

/// What this module's lines are filed under.
const TAG: &str = "pointer";

/// How often the helper reads the pointer.
///
/// About a drawn frame. The answer travels to another computer and is
/// drawn there, so reading faster than that machine can show it buys
/// nothing; reading much slower is a hand that reaches a text field and
/// waits to be told.
const READ_EVERY: Duration = Duration::from_millis(30);

/// How long a helper lives before it goes home of its own accord.
///
/// It is what stops one outliving the service that started it: nobody
/// terminates it, it simply ends. Short enough that a service which
/// crashes leaves nothing running for long, long enough that starting
/// them again is a few times a minute and not a few times a second.
const HELPER_LIVES: Duration = Duration::from_secs(10);

/// How late is too late to count on the one that is running.
///
/// Another is started before the last has ended, so the reading never
/// stops between two of them.
const START_ANOTHER_AFTER: Duration = Duration::from_secs(7);

/// How long the service goes on keeping a helper after the last question.
const AFTER_THE_LAST_QUESTION: Duration = Duration::from_secs(2);

/// Whether the thread that keeps a helper alive is running.
static KEEPING: AtomicBool = AtomicBool::new(false);

/// When the last question came, so the keeper knows when to stop.
static ASKED: Mutex<Option<Instant>> = Mutex::new(None);

/// The shape the pointer has right now.
///
/// Never blocks and never fails: what comes back is the last word the
/// helper wrote, which is at most one reading old, and the ordinary
/// arrow before the first one, which is corrected within the frame that
/// follows. A pointer that arrives right an instant late is worth far
/// more than an answer that holds up the channel it travels on.
pub fn shape(log: &Log) -> Pointer {
    let log = &log.about(TAG);
    *ASKED.lock().expect("dernière question") = Some(Instant::now());
    if !KEEPING.swap(true, Ordering::SeqCst) {
        keep_a_helper(log.clone());
    }
    written_shape()
}

/// What the helper last wrote, or the ordinary arrow.
fn written_shape() -> Pointer {
    std::fs::read_to_string(zyr_proto::paths::pointer_here())
        .ok()
        .and_then(|word| word.trim().parse().ok())
        .unwrap_or_default()
}

/// Whether the last question is far enough behind to stop.
fn nobody_is_asking() -> bool {
    ASKED
        .lock()
        .expect("dernière question")
        .is_none_or(|asked| asked.elapsed() > AFTER_THE_LAST_QUESTION)
}

/// Keeps a helper reading in the session that owns the screen, for as
/// long as anybody is asking.
///
/// A thread of its own because starting a program in another session
/// takes milliseconds, and the threads that answer the far computer are
/// shared with everything else this service does.
#[cfg(windows)]
fn keep_a_helper(log: Log) {
    let log = log.about(TAG);
    std::thread::spawn(move || {
        log.write("a session is asking what shape this computer's pointer has");
        let mut started: Option<Instant> = None;
        let mut refused = false;
        while !nobody_is_asking() {
            if started.is_none_or(|at| at.elapsed() > START_ANOTHER_AFTER) {
                match crate::session::start_reading_the_pointer() {
                    Ok(()) => {
                        if started.is_none() {
                            log.write("reading it from the session that owns the screen");
                        }
                        refused = false;
                        started = Some(Instant::now());
                    }
                    // Said once and not every second: a machine at its
                    // sign-in screen has no session to read from, and
                    // that is a state it can sit in for hours.
                    Err(e) => {
                        if !refused {
                            refused = true;
                            log.write(&format!("nothing can read the pointer here: {e}"));
                        }
                        started = None;
                    }
                }
            }
            std::thread::sleep(READ_EVERY);
        }
        log.write(&format!(
            "nobody is asking any more, the last shape read was {}",
            written_shape()
        ));
        // The word goes with the asking: the next session starts on the
        // ordinary pointer rather than on whatever shape this one was
        // left under.
        let _ = std::fs::remove_file(zyr_proto::paths::pointer_here());
        KEEPING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
fn keep_a_helper(_log: Log) {
    KEEPING.store(false, Ordering::SeqCst);
}

/// Reads the pointer of the desktop this program is standing on, and
/// writes it down, until its time is up.
///
/// This is the helper, and it only ever runs in the session that owns the
/// screen: started anywhere else it reads a desktop with no pointer on
/// it. It ends by itself so that nothing has to end it.
#[cfg(windows)]
pub fn follow_the_pointer_here() {
    let until = Instant::now() + HELPER_LIVES;
    let mut written = None;
    while Instant::now() < until {
        let shape = read_the_pointer();
        if written != Some(shape) {
            // Replaced whole and never written in place: the service
            // reads between two writes, and a word caught half written
            // would be a shape nobody named.
            let path = zyr_proto::paths::pointer_here();
            let beside = path.with_extension("new");
            if std::fs::write(&beside, format!("{shape}\n")).is_ok()
                && std::fs::rename(&beside, &path).is_ok()
            {
                written = Some(shape);
            }
        }
        std::thread::sleep(READ_EVERY);
    }
}

#[cfg(not(windows))]
pub fn follow_the_pointer_here() {}

/// The shape the pointer has on the desktop this program stands on.
#[cfg(windows)]
fn read_the_pointer() -> Pointer {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CURSOR_SHOWING, CURSORINFO, GetCursorInfo};

    if a_window_is_being_dragged() {
        return Pointer::Theirs;
    }
    let mut about = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the block is ours with its own size written in it as the
    // call requires.
    if unsafe { GetCursorInfo(&mut about) } == 0 {
        return Pointer::Arrow;
    }
    // Hidden is the ordinary pointer and not a shape of its own. A
    // machine hides it while somebody types and shows it again on the
    // first movement, and a session that answered « nothing » there would
    // blink the pointer out under a hand that had not moved.
    if about.flags & CURSOR_SHOWING == 0 {
        return Pointer::Arrow;
    }
    named_shape(about.hCursor)
}

/// Whether a window on this desktop is being dragged or resized right
/// now.
///
/// It is asked for one reason, and the answer is not a shape but the
/// absence of one. While that lasts, Windows stops letting the graphics
/// card carry the pointer and composes it with the window instead, so
/// that the two move together and neither is a frame behind the other.
/// The picture this computer sends is filmed after that composing, so
/// the pointer is already in it, and the switch that keeps the engine
/// from drawing one has nothing left to switch off. The session watching
/// would show two: its own, where the hand is, and this one, a round
/// trip behind, dragging the window.
///
/// So it is told to draw none for as long as this lasts, and what it
/// shows is the one Windows drew, moving with the window it drags.
///
/// The system says it plainly: a window being dragged or resized puts
/// the thread that owns it in a loop of its own, and that is what is
/// read here.
#[cfg(windows)]
fn a_window_is_being_dragged() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GUI_INMOVESIZE, GUITHREADINFO, GetGUIThreadInfo,
    };

    let mut about = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the block is ours with its own size written in it as the
    // call requires. Naming no thread means the one in front, which is
    // the only one that can be dragging anything.
    if unsafe { GetGUIThreadInfo(0, &mut about) } == 0 {
        return false;
    }
    about.flags & GUI_INMOVESIZE != 0
}

/// Which of the shapes this computer knows that pointer is.
///
/// Compared against the system's own, which are shared: a program asking
/// for the ordinary arrow is handed the very same pointer as every other
/// program that asked, so the two can be told apart by identity alone.
/// A pointer a program drew for itself matches none of them and comes
/// back as the arrow, which is what a system falls back to as well.
#[cfg(windows)]
fn named_shape(cursor: windows_sys::Win32::UI::WindowsAndMessaging::HCURSOR) -> Pointer {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL,
        IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_WAIT, LoadCursorW,
    };

    if cursor.is_null() {
        return Pointer::Arrow;
    }
    for (which, shape) in [
        (IDC_IBEAM, Pointer::Text),
        (IDC_HAND, Pointer::Hand),
        (IDC_WAIT, Pointer::Wait),
        (IDC_APPSTARTING, Pointer::WaitingArrow),
        (IDC_CROSS, Pointer::Cross),
        (IDC_SIZEWE, Pointer::SizeAcross),
        (IDC_SIZENS, Pointer::SizeDown),
        (IDC_SIZENWSE, Pointer::SizeFalling),
        (IDC_SIZENESW, Pointer::SizeRising),
        (IDC_SIZEALL, Pointer::SizeAll),
        (IDC_NO, Pointer::Refused),
        (IDC_ARROW, Pointer::Arrow),
    ] {
        // SAFETY: a shape of the system's own, asked for by the number
        // the system reserves for it. Nothing is loaded from a file and
        // nothing is ours to free: these are shared and outlive us.
        if unsafe { LoadCursorW(std::ptr::null_mut(), which) } == cursor {
            return shape;
        }
    }
    Pointer::Arrow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personne_ne_demande_tant_que_personne_n_a_demande() {
        // C'est ce qui décide qu'un ordinateur que personne ne regarde
        // ne lit rien du tout : sans question, plus aucun assistant
        // n'est relancé et le dernier s'éteint tout seul.
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
        // assistants et le curseur se figerait sur sa dernière forme le
        // temps qu'un autre démarre.
        assert!(
            START_ANOTHER_AFTER < HELPER_LIVES,
            "un assistant doit être relancé avant la fin du précédent"
        );
    }

    #[test]
    fn un_mot_absent_est_la_fleche_ordinaire() {
        // Le service lit ce fichier avant qu'aucun assistant n'ait eu le
        // temps d'écrire : ce moment-là doit être une flèche et non un
        // refus, sans quoi la première session n'aurait pas de curseur.
        let _ = std::fs::remove_file(zyr_proto::paths::pointer_here());
        assert_eq!(written_shape(), Pointer::Arrow);
    }
}
