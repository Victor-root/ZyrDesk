//! What shape the pointer has on this computer.
//!
//! A desktop says what a click is about to do through the shape of the
//! pointer and through almost nothing else: an upright bar means the
//! click lands between two letters, a hand means something to follow, a
//! ring means wait. The computer watching draws its own pointer, so that
//! it answers the hand with no network in between; without this it would
//! answer with an arrow and nothing else, whatever is under it.
//!
//! Nothing here is done to the machine. It is a reading, and the whole
//! module exists because of where the reading has to happen: a service
//! sits in a session with no screen, no keyboard and no pointer, and
//! asking it there answers about a desktop nobody is looking at. The
//! answer lives on the desktop that owns the input, which changes under
//! a machine being locked or asking for a password, and a thread has to
//! be standing on it to read anything at all.
//!
//! So one thread stands there, and it only stands there while somebody
//! is asking: it starts on the first question and goes home a couple of
//! seconds after the last. A machine nobody is watching reads nothing.

// Outside Windows nothing calls this module: the service does not exist
// there. The shape of it stays compiled and tested everywhere, and the
// reading itself is the one part that cannot be.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::log::Log;
use zyr_proto::session::Pointer;

/// How often the shape is read while somebody is asking for it.
///
/// About a drawn frame. The answer travels to another computer and is
/// drawn there, so reading faster than that machine can show it buys
/// nothing; reading much slower is a hand that reaches a text field and
/// waits to be told.
const READ_EVERY: Duration = Duration::from_millis(30);

/// How long the reader stays on the desktop after the last question.
///
/// Long enough to cover the gap between two questions of one session,
/// including a picture opened again in the middle of it, and short
/// enough that a machine nobody is watching is back to reading nothing
/// almost at once.
const AFTER_THE_LAST_QUESTION: Duration = Duration::from_secs(2);

/// The shape last read, as its position in `Pointer::ALL`.
///
/// A number rather than the shape itself because it is written by the
/// reading thread and read by whoever answers the far computer, and
/// those are never the same thread.
static SHAPE: AtomicU8 = AtomicU8::new(0);

/// Whether the reading thread is standing on the desktop right now.
static READING: AtomicBool = AtomicBool::new(false);

/// When the last question came, so the thread knows when to go home.
static ASKED: Mutex<Option<Instant>> = Mutex::new(None);

/// The shape the pointer has right now, and starts the reading if it had
/// stopped.
///
/// Never blocks and never fails: what comes back is the last shape read,
/// which is at most one reading old, and the ordinary arrow for the
/// first question of a session, which is corrected within the frame that
/// follows. A pointer that arrives right an instant late is worth far
/// more than an answer that holds up the channel it travels on.
pub fn shape(log: &Log) -> Pointer {
    *ASKED.lock().expect("dernière question") = Some(Instant::now());
    if !READING.swap(true, Ordering::SeqCst) {
        start_reading(log.clone());
    }
    Pointer::ALL
        .get(SHAPE.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
}

/// Whether the last question is far enough behind to stop.
fn nobody_is_asking() -> bool {
    ASKED
        .lock()
        .expect("dernière question")
        .is_none_or(|asked| asked.elapsed() > AFTER_THE_LAST_QUESTION)
}

/// Writes down what was read, for whoever answers next.
fn read_as(shape: Pointer) {
    let at = Pointer::ALL
        .iter()
        .position(|known| *known == shape)
        .unwrap_or(0);
    SHAPE.store(at as u8, Ordering::Relaxed);
}

/// Stands on the desktop that owns the input and reads the pointer, for
/// as long as anybody is asking.
///
/// A thread of its own and not a moment borrowed from another: standing
/// on a desktop is done to a thread and stays done to it, and the
/// threads that answer the far computer are shared with everything else
/// this service does.
#[cfg(windows)]
fn start_reading(log: Log) {
    std::thread::spawn(move || {
        log.write("pointer: a session is asking what shape this computer's pointer has");
        let mut standing = Standing::nowhere(log.clone());
        let mut seen = Seen::default();
        while !nobody_is_asking() {
            standing.follow_the_input();
            let shape = standing.read();
            seen.saw(shape);
            read_as(shape);
            std::thread::sleep(READ_EVERY);
        }
        log.write(&format!("pointer: nobody is asking any more, {seen}"));
        // Put down before the desktop is let go of, so a question
        // arriving in between starts a thread rather than finding this
        // one on its way out.
        READING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
fn start_reading(_log: Log) {
    READING.store(false, Ordering::SeqCst);
}

/// What the reading saw while it lasted, for the journal.
///
/// Two lines a session, and they answer the two questions a pointer that
/// stays an arrow raises: was anything read at all, and was anything but
/// the arrow ever under it. A line per reading would be thirty a second
/// and would answer neither.
#[derive(Default)]
struct Seen {
    readings: u64,
    shapes: Vec<Pointer>,
}

impl Seen {
    fn saw(&mut self, shape: Pointer) {
        self.readings += 1;
        if !self.shapes.contains(&shape) {
            self.shapes.push(shape);
        }
    }
}

impl std::fmt::Display for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} readings, shapes seen:", self.readings)?;
        if self.shapes.is_empty() {
            return f.write_str(" none at all");
        }
        for shape in &self.shapes {
            write!(f, " {shape}")?;
        }
        Ok(())
    }
}

/// The desktop this thread is standing on, and its name.
///
/// The name is what says whether it is still the right one: opening the
/// desktop that owns the input hands back a new handle every time, so
/// two handles say nothing about whether they are the same desk.
#[cfg(windows)]
struct Standing {
    desk: windows_sys::Win32::System::StationsAndDesktops::HDESK,
    named: String,
    log: Log,
    /// Whether the last try to stand somewhere was refused, so that a
    /// refusal is written once and not thirty times a second.
    turned_away: bool,
}

#[cfg(windows)]
impl Standing {
    fn nowhere(log: Log) -> Self {
        Self {
            desk: std::ptr::null_mut(),
            named: String::new(),
            log,
            turned_away: false,
        }
    }

    /// Says a refusal once, and says the recovery once too.
    fn turned_away(&mut self, why: &str) {
        if !self.turned_away {
            self.turned_away = true;
            self.log
                .write(&format!("pointer: nowhere to read the pointer, {why}"));
        }
    }

    /// Moves to the desktop that owns the screen and the keyboard, when
    /// it is not the one this thread is already on.
    ///
    /// It changes under an ordinary machine: `Winlogon` while it is
    /// locked or asking for a password, `Default` while somebody works.
    /// A thread left on the desk it opened first goes on answering about
    /// that one, which is a pointer belonging to a screen nobody is
    /// looking at.
    fn follow_the_input(&mut self) {
        use windows_sys::Win32::Foundation::GENERIC_READ;
        use windows_sys::Win32::System::StationsAndDesktops::{
            CloseDesktop, OpenInputDesktop, SetThreadDesktop,
        };

        // SAFETY: nothing of ours is handed over, and a refusal answers
        // null. Refused is ordinary and not a fault: the desk is being
        // handed from one to the other, and there is nothing to open in
        // between.
        let opened = unsafe { OpenInputDesktop(0, 0, GENERIC_READ) };
        if opened.is_null() {
            self.turned_away("the desktop with the input would not open");
            return;
        }
        let named = name_of(opened);
        if named == self.named && !self.desk.is_null() {
            // SAFETY: a desktop this function opened a line above and
            // this thread never stood on, closed exactly once.
            unsafe { CloseDesktop(opened) };
            return;
        }
        // SAFETY: a desktop just opened, given to this thread alone,
        // which holds no window and no hook and so may leave the one it
        // was on.
        if unsafe { SetThreadDesktop(opened) } == 0 {
            self.turned_away("this thread was not allowed to stand on it");
            // SAFETY: refused, so nothing stands on it and it is ours to
            // close.
            unsafe { CloseDesktop(opened) };
            return;
        }
        self.turned_away = false;
        self.log.write(&format!(
            "pointer: now reading on desktop {named}{}",
            if self.named.is_empty() {
                String::new()
            } else {
                format!(", was on {}", self.named)
            }
        ));
        let left = std::mem::replace(&mut self.desk, opened);
        self.named = named;
        if !left.is_null() {
            // SAFETY: the desktop this thread has just left, closed
            // exactly once, and never while it was standing on it.
            unsafe { CloseDesktop(left) };
        }
    }

    /// The shape the pointer has on the desk this thread is standing on.
    fn read(&self) -> Pointer {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CURSOR_SHOWING, CURSORINFO, GetCursorInfo,
        };

        if self.desk.is_null() {
            return Pointer::Arrow;
        }
        let mut about = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: the block is ours with its own size written in it as
        // the call requires, read on the desk this thread stands on.
        if unsafe { GetCursorInfo(&mut about) } == 0 {
            return Pointer::Arrow;
        }
        // Hidden is the ordinary pointer and not a shape of its own. A
        // machine hides it while somebody types and shows it again on the
        // first movement, and a session that answered « nothing » there
        // would blink the pointer out under a hand that had not moved.
        if about.flags & CURSOR_SHOWING == 0 {
            return Pointer::Arrow;
        }
        named_shape(about.hCursor)
    }
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

/// The name of that desktop, or nothing when it cannot be read.
#[cfg(windows)]
fn name_of(desk: windows_sys::Win32::System::StationsAndDesktops::HDESK) -> String {
    use windows_sys::Win32::System::StationsAndDesktops::{GetUserObjectInformationW, UOI_NAME};

    let mut name = [0u16; 64];
    let mut needed = 0u32;
    // SAFETY: a desktop the caller holds open, and a slot of ours with
    // its length in bytes given alongside as the call expects.
    let read = unsafe {
        GetUserObjectInformationW(
            desk,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            (name.len() * size_of::<u16>()) as u32,
            &mut needed,
        )
    };
    if read == 0 {
        return String::new();
    }
    let end = name.iter().position(|letter| *letter == 0).unwrap_or(0);
    String::from_utf16_lossy(&name[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_forme_se_range_et_se_relit_sans_perte() {
        // Elle traverse deux fils par un nombre : celui qui lit se tient
        // sur un bureau, celui qui répond est partagé avec tout le
        // reste du service. Une forme qui ne reviendrait pas identique
        // donnerait un sablier pour une barre de texte.
        for shape in Pointer::ALL {
            read_as(shape);
            assert_eq!(
                Pointer::ALL[SHAPE.load(Ordering::Relaxed) as usize],
                shape,
                "sur « {shape} »"
            );
        }
    }

    #[test]
    fn personne_ne_demande_tant_que_personne_n_a_demande() {
        // C'est ce qui décide qu'un ordinateur que personne ne regarde
        // ne lit rien du tout : sans question, le fil rentre chez lui.
        *ASKED.lock().unwrap() = None;
        assert!(nobody_is_asking());
        *ASKED.lock().unwrap() = Some(Instant::now());
        assert!(!nobody_is_asking());
        *ASKED.lock().unwrap() = Instant::now().checked_sub(AFTER_THE_LAST_QUESTION * 2);
        assert!(nobody_is_asking());
    }
}
