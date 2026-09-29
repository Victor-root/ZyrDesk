//! What this program is started for in the session that owns the
//! screen.
//!
//! The service cannot reach the desk of whoever sits at this computer, so
//! it starts this very program there, with a reserved argument naming
//! why: to be the engine of one session, to run one short errand whose
//! exit code is its answer (the speakers, the lock screen, the desk), or
//! to be a helper that reads for a while and ends by itself (the pointer,
//! the clipboard). Both ends of each argument live here, the one that
//! starts the program and the one that recognises what it was started
//! for, so that they cannot drift apart. How a program is started in that
//! session at all is `zyr_system`'s.

use std::io;
use std::time::Duration;

use zyr_proto::paths;
use zyr_proto::session::WantedScreen;
use zyr_system::{Errand, Launch, SessionProcess, Whose};

use crate::gateway::{Launched, Launcher};

/// What this module's lines are filed under.
const TAG: &str = "desk";

/// Reserved argument that turns this program into the engine of one
/// session, followed by the name of the link it serves it on.
///
/// An argument and not a command, like the one Windows starts the
/// service with: nobody types it, and it names a moment rather than
/// something a person can ask for.
pub const SERVE_ARGUMENT: &str = "--serve-a-session";

/// Where the engine's own console output goes, which is what it says
/// before its journal is open, and what a crash leaves behind.
const ENGINE_CONSOLE: &str = "engine-console.log";

/// The same for the speakers of this computer; see
/// `move_the_speakers`.
///
/// It carries which way they are to be moved, because both ways are the
/// same errand and one name for it is one name to keep in step.
pub const SPEAKERS_ARGUMENT: &str = "--set-the-speakers";
pub const SPEAKERS_QUIET: &str = "quiet";
pub const SPEAKERS_PLAYING: &str = "playing";

/// What that errand answers with.
///
/// Three answers and not two, because whoever asked has to know whether
/// it now owes the person their sound back. Muting speakers that were
/// already muted owes nothing, and giving that sound back at the end of
/// a session would be undoing something this product never did.
pub const SPEAKERS_MOVED: u32 = 0;
pub const SPEAKERS_REFUSED: u32 = 1;
pub const SPEAKERS_ALREADY: u32 = 2;

/// And the same for locking this computer's screen; see
/// `zyr_system::lock_this_desktop`.
///
/// Windows will only take that order from a program on the interactive
/// desktop, which a service is not, and there is no way round it: it is
/// what makes a lock screen worth trusting.
pub const LOCK_ARGUMENT: &str = "--lock-the-screen";

/// And the same for this computer's desk; see `do_this_to_the_desk`.
///
/// Two names and not one, because they are two errands with nothing in
/// common but the subject: one holds the desk for a session that is
/// starting, the other gives it back when that session has gone. The
/// first carries what the session wants, the second carries nothing at
/// all, what to put back having been written down when it was taken.
///
/// Here for the reason all of these are here. Everything Windows says
/// about the arrangement of screens is answered for the window station of
/// whoever asks, and a service sits on one with no screens at all: asked
/// from there, this computer has no screens, which is what it used to
/// answer a session that asked what it was showing.
pub const DESK_ARGUMENT: &str = "--hold-the-desk";
pub const DESK_BACK_ARGUMENT: &str = "--give-the-desk-back";

/// And a third, for the computer whose own screens cannot draw the size
/// a session asked for: the desktop moves onto the screen this computer
/// grew for itself, which the service has just woken at that size.
///
/// Its own errand and not part of the first, because the two happen
/// either side of something only the service can do. Starting a display
/// device is administrator work, so the service wakes the screen; putting
/// a desktop on it is window station work, so the session on screen does
/// that. One cannot wait for the other inside a single errand.
pub const DESK_GROWN_ARGUMENT: &str = "--take-the-grown-screen";

/// And a sixth, for the shape of this computer's pointer; see
/// `crate::pointer`.
///
/// The same blindness once more, and the plainest case of it: a pointer
/// belongs to a desktop, the desktop that owns the input belongs to the
/// session on screen, and the service's window station carries no
/// desktop at all. Asked from there, this computer has no pointer, which
/// is exactly what it answered a session that asked for the shape of it.
///
/// This one differs from the five above in one way: it does not do a
/// thing and come back, it reads for a while. It ends by itself after a
/// short life so that nothing has to end it, and the service starts
/// another for as long as somebody is asking.
pub const POINTER_ARGUMENT: &str = "--follow-the-pointer";

/// And a seventh, for this computer's clipboard; see
/// `crate::clipboard`.
///
/// The same blindness again: a clipboard belongs to a window station, and
/// the one a service sits on carries none at all. Asked from there, this
/// computer's clipboard is a clipboard nobody has ever copied anything
/// to, and anything written to it is written where nobody will paste.
///
/// Like the pointer above, it reads for a while rather than doing one
/// thing, and ends by itself. Unlike it, it writes too: what was copied
/// on the far computer is put on this one from here.
pub const CLIPBOARD_ARGUMENT: &str = "--carry-the-clipboard";

/// What the first of those carries when a session wants the desk noted
/// and nothing moved, which is what « keep your own screen » asks for.
///
/// A word and not an absent argument: an errand that names what it wants
/// and an errand that lost its argument on the way must not look alike.
const NOTHING_WANTED: &str = "none";

/// Starts the engine of each incoming session in the session attached
/// to the screen.
#[derive(Debug, Clone, Copy)]
pub struct ServingInSession {
    session: u32,
}

impl ServingInSession {
    pub fn new(session: u32) -> Self {
        Self { session }
    }
}

impl Launcher for ServingInSession {
    fn launch(&self, link: &str) -> io::Result<Box<dyn Launched>> {
        let ourselves = std::env::current_exe()?;
        let arguments = [SERVE_ARGUMENT.to_string(), link.to_string()];
        let console = paths::logs_dir().join(ENGINE_CONSOLE);
        let launch = Launch {
            exe: &ourselves,
            arguments: &arguments,
            working_dir: ourselves.parent(),
            console: &console,
            starting: "--- engine starting ---",
        };
        Ok(Box::new(zyr_system::start_in_session(
            &launch,
            self.session,
        )?))
    }
}

impl Launched for SessionProcess {
    fn process(&self) -> u32 {
        SessionProcess::process(self)
    }

    fn gone(&self) -> bool {
        SessionProcess::gone(self)
    }

    fn let_go(self: Box<Self>, within: Duration) -> io::Result<Option<u32>> {
        SessionProcess::let_go(*self, within)
    }
}

/// The link this program was started to serve a session on, if that is
/// what it was started for.
pub fn the_link_to_serve() -> Option<String> {
    the_link_named_in(std::env::args())
}

/// The same, over any list of arguments, so it can be read without
/// starting a program to hold them.
fn the_link_named_in(arguments: impl Iterator<Item = String>) -> Option<String> {
    let mut after = arguments.skip_while(|a| a != SERVE_ARGUMENT);
    after.next()?;
    after.next().filter(|link| !link.is_empty())
}

/// Moves this computer's speakers, and says whether they really moved.
///
/// From the session that owns the screen, like everything else here, and
/// for a reason of its own: which device the desktop plays to is a
/// question whose answer depends on who is signed in. Asked from the
/// service's own session, it would name a device nobody is listening to,
/// and the room would go on playing.
///
/// `true` means they were doing the opposite a moment ago and are now
/// doing what was asked, which is also « something is owed back ».
pub fn set_the_speakers(quiet: bool) -> io::Result<bool> {
    let way = if quiet {
        SPEAKERS_QUIET
    } else {
        SPEAKERS_PLAYING
    };
    let refused = "the speakers could not be reached from the session on screen";
    match zyr_system::errand_code(&[SPEAKERS_ARGUMENT.to_string(), way.to_string()], refused)? {
        (SPEAKERS_MOVED, _) => Ok(true),
        (SPEAKERS_ALREADY, _) => Ok(false),
        _ => Err(io::Error::other(refused)),
    }
}

/// Whether this program was started to move the speakers, and which way.
pub fn asked_about_the_speakers() -> Option<bool> {
    the_way_named_in(std::env::args())
}

/// The same, over any list of arguments, so it can be checked without
/// starting a program to hold them.
fn the_way_named_in(arguments: impl Iterator<Item = String>) -> Option<bool> {
    let mut after = arguments.skip_while(|a| a != SPEAKERS_ARGUMENT);
    after.next()?;
    match after.next()?.as_str() {
        SPEAKERS_QUIET => Some(true),
        SPEAKERS_PLAYING => Some(false),
        _ => None,
    }
}

/// Moves them, from inside the session that owns the screen.
///
/// This is the whole of what this program does when started with
/// `SPEAKERS_ARGUMENT`. What went wrong is written into the service's own
/// journal from here rather than carried back in the exit code: there is
/// more than one way for a computer to have no reachable sound, and a
/// number would tell nobody which of them happened.
pub fn move_the_speakers(quiet: bool) -> u32 {
    let said = |what: String| {
        if let Ok(log) = zyr_proto::log::Log::open(&crate::service::log_path()) {
            log.about(TAG).write(&what);
        }
    };
    let already = match zyr_sound::speakers_muted() {
        Ok(muted) => muted,
        Err(e) => {
            said(format!("speakers not read: {e}"));
            return SPEAKERS_REFUSED;
        }
    };
    if already == quiet {
        return SPEAKERS_ALREADY;
    }
    match zyr_sound::mute_speakers(quiet) {
        Ok(()) => SPEAKERS_MOVED,
        Err(e) => {
            said(format!("speakers not moved: {e}"));
            SPEAKERS_REFUSED
        }
    }
}

/// Locks this computer's screen, from the session that owns it.
///
/// The other half of Ctrl+Alt+Del, and the other way round. That one
/// goes through the service's own process, because Windows takes it from
/// a service and from nothing else; this one goes through a program on
/// the interactive desktop, because Windows takes it from there and from
/// nothing else. Both refusals protect the same thing: what a lock screen
/// is worth depends on nobody being able to put one up, or take one
/// down, from outside the desk it belongs to.
pub fn lock_the_screen() -> io::Result<Errand> {
    zyr_system::errand(
        &[LOCK_ARGUMENT.to_string()],
        "the screen could not be locked from the session that owns it",
    )
}

/// Notes this computer's desk and puts its main screen where a session
/// wants it, from the session that owns that screen.
///
/// Asked before the engine opens on it: what the engine captures is
/// pixels, and both the size and the magnification decide how many of
/// them a letter is made of. Changed afterwards they would land in the
/// middle of a picture somebody is already watching, and everything on
/// the desktop would jump.
///
/// Nothing asked for still runs, and is not a wasted errand: it is how
/// the service learns what this computer is showing, which it cannot see
/// for itself and used to answer « I cannot measure my own screen » to.
///
/// Never fails a session. What a session loses is a desk the size it
/// asked for, which is a session slightly wrong and not a session
/// missing, and the sentences saying why go into this computer's journal.
pub fn hold_the_desk_for(wanted: Option<WantedScreen>) -> io::Result<Errand> {
    let asked = match wanted {
        Some(screen) => screen.to_string(),
        None => NOTHING_WANTED.to_string(),
    };
    zyr_system::errand(
        &[DESK_ARGUMENT.to_string(), asked],
        "this computer's desk could not be set from the session that owns the screen",
    )
}

/// Moves this computer's desktop onto the screen it grew for itself, at
/// the size a session asked for, from the session that owns the screen.
///
/// Asked only after the service has woken that screen, and only where
/// this computer's own screens refused the size: everywhere else the
/// desktop stays where its owner left it.
pub fn take_the_grown_screen(wanted: WantedScreen) -> io::Result<Errand> {
    zyr_system::errand(
        &[DESK_GROWN_ARGUMENT.to_string(), wanted.to_string()],
        "this computer's desktop could not be moved onto the screen it grew for itself",
    )
}

/// Starts a helper in the session that owns the screen, to read the
/// shape of this computer's pointer.
pub fn start_reading_the_pointer() -> io::Result<()> {
    zyr_system::start_a_helper(&[POINTER_ARGUMENT.to_string()], Whose::TheService)
}

/// Whether this program was started to read the pointer.
pub fn asked_to_follow_the_pointer() -> bool {
    std::env::args().any(|argument| argument == POINTER_ARGUMENT)
}

/// Starts a helper in that same session, to read and write this
/// computer's clipboard.
///
/// The one helper started as the person and not as the service, and it
/// has to be. A clipboard is not simply a thing sitting on a window
/// station: text and pictures do sit there as plain blocks anybody with
/// the station can read, but files never do. What a program puts there
/// for files is a promise, an object living inside it, and reading or
/// paying that promise means one program calling into another. Windows
/// refuses that across accounts and across levels: the service is the
/// system, the Explorer is the person, and neither can reach into the
/// other. Under the service's account the object came back hollow going
/// one way, and what this computer offered was invisible going the other,
/// which is precisely the two halves that never worked.
pub fn start_carrying_the_clipboard() -> io::Result<()> {
    zyr_system::start_a_helper(&[CLIPBOARD_ARGUMENT.to_string()], Whose::ThePerson)
}

/// Whether this program was started to carry the clipboard.
pub fn asked_to_carry_the_clipboard() -> bool {
    std::env::args().any(|argument| argument == CLIPBOARD_ARGUMENT)
}

/// Puts the desk back the way it was noted, from the session that owns
/// the screen.
///
/// Asked when the last session goes, and asked again by the watch that
/// holds the engine for as long as a desk stays noted: a session whose
/// computer was closed, unplugged or crashed says nothing at all, and
/// that is exactly the session after which somebody's screens would stay
/// the way a stranger left them.
pub fn give_the_desk_back() -> io::Result<Errand> {
    zyr_system::errand(
        &[DESK_BACK_ARGUMENT.to_string()],
        "this computer's desk could not be put back from the session that owns the screen",
    )
}

/// What this program was started to do to the desk, if that is what it
/// was started for.
pub fn the_desk_asked_for() -> Option<Desk> {
    the_desk_named_in(std::env::args())
}

/// One errand about this computer's desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desk {
    /// Note it, and put the main screen where a session wants it. Nothing
    /// wanted notes it and moves nothing.
    Hold(Option<WantedScreen>),
    /// Put it back the way it was noted.
    Back,
    /// Move the desktop onto the screen this computer grew for itself,
    /// at that size, this computer's own screens having refused it.
    Borrow(WantedScreen),
}

/// The same, over any list of arguments, so it can be read without
/// starting a program to hold them.
fn the_desk_named_in(arguments: impl Iterator<Item = String>) -> Option<Desk> {
    let arguments: Vec<String> = arguments.collect();
    if arguments.iter().any(|a| a == DESK_BACK_ARGUMENT) {
        return Some(Desk::Back);
    }
    if let Some(asked) = after_the_word(&arguments, DESK_GROWN_ARGUMENT) {
        return asked.parse().ok().map(Desk::Borrow);
    }
    let asked = after_the_word(&arguments, DESK_ARGUMENT)?;
    if asked == NOTHING_WANTED {
        return Some(Desk::Hold(None));
    }
    // A size that will not read is not nothing asked for: it is an errand
    // that was meant to move a screen and cannot say where to. Answering
    // « note the desk and move nothing » to it would leave the session
    // watching a desk at the wrong size with nothing in any journal.
    asked.parse().ok().map(|screen| Desk::Hold(Some(screen)))
}

/// What follows that word among those arguments, when it is there and
/// something follows it.
fn after_the_word(arguments: &[String], word: &str) -> Option<String> {
    let at = arguments.iter().position(|argument| argument == word)?;
    arguments.get(at + 1).cloned()
}

/// Does it, from inside the session that owns the screen.
///
/// This is the whole of what this program does when started with either
/// desk argument. What happened is written into the service's own journal
/// from here rather than carried back in an exit code: there is more than
/// one way for a desk not to move, and a number would tell nobody which
/// of them happened.
pub fn do_this_to_the_desk(asked: Desk) {
    let said = match asked {
        Desk::Hold(wanted) => zyr_screen::desk::hold_the_desk_for(
            &paths::virtual_screen_dir(),
            wanted.map(|screen| (screen.wide, screen.high, screen.scale)),
        ),
        Desk::Back => zyr_screen::desk::give_the_desk_back(&paths::virtual_screen_dir()),
        Desk::Borrow(screen) => zyr_screen::desk::take_the_grown_screen_for(
            &paths::virtual_screen_dir(),
            (screen.wide, screen.high, screen.scale),
        ),
    };
    if let Ok(log) = zyr_proto::log::Log::open(&crate::service::log_path()) {
        let log = log.about(TAG);
        for line in said {
            log.write(&line);
        }
    }
}

/// Whether this program was started to lock the screen.
pub fn asked_to_lock_the_screen() -> bool {
    std::env::args().any(|a| a == LOCK_ARGUMENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_to_serve_is_read_from_the_arguments() {
        let said = |arguments: &[&str]| the_link_named_in(arguments.iter().map(|a| a.to_string()));
        let link = r"\\.\pipe\ZyrDesk-link-8fKq2Lr0aZ3x9Wm1";
        assert_eq!(
            said(&["zyrdeskd.exe", SERVE_ARGUMENT, link]),
            Some(link.to_string())
        );
        // Started for anything else, this program serves no session:
        // the ordinary commands must go on reaching clap untouched.
        assert_eq!(said(&["zyrdeskd.exe", "status"]), None);
        assert_eq!(said(&["zyrdeskd.exe"]), None);
        // And a link that is not named is no link at all.
        assert_eq!(said(&["zyrdeskd.exe", SERVE_ARGUMENT]), None);
        assert_eq!(said(&["zyrdeskd.exe", SERVE_ARGUMENT, ""]), None);
    }
}
