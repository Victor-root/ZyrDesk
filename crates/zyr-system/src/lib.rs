//! What ZyrDesk asks of Windows itself.
//!
//! A service lives in a session with no screen, no desktop and nobody in
//! front of it. What it does to the computer somebody is sitting at goes
//! through here: finding the session that holds the screen and who is in
//! it, starting this very program there, as the engine of a session, for
//! a short errand whose exit code is its answer, or as a helper left to
//! read for a while; locking that screen from inside it; pressing
//! Ctrl+Alt+Del, which Windows takes from a service and from nothing
//! else; asking the Wi-Fi to put a session first; reading the shape
//! the pointer has on the desktop this program stands on; counting
//! how busy the computer is and what its network has carried and lost;
//! and saying what the computer is.
//!
//! And what gets a program running at all: with no console flashing up,
//! with administrator rights asked of the person, or when the person
//! signs in.
//!
//! This crate knows Windows and nothing about ZyrDesk: which errand is
//! which, and when one is worth running, is decided by whoever asks.
//! Outside Windows everything still compiles and says what is true
//! there, which is that no session holds a screen: the product is built
//! and tested on machines that are not the ones it runs on.

#[cfg(windows)]
mod attention;
#[cfg(any(windows, test))]
mod counted;
#[cfg(windows)]
mod elevated;
#[cfg(not(windows))]
mod elsewhere;
#[cfg(windows)]
mod inspection;
#[cfg(any(windows, test))]
mod machine;
#[cfg(windows)]
mod onscreen;
#[cfg(windows)]
mod pointer;
#[cfg(windows)]
mod reading;
#[cfg(windows)]
mod sign_in;
mod wifi;

use std::fmt;
use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

#[cfg(windows)]
pub use attention::{forget_it, let_it_be_pressed, press};
#[cfg(windows)]
pub use elevated::run_as_administrator;
#[cfg(not(windows))]
pub use elsewhere::{
    SessionProcess, Vitals, describe_this_computer, errand, errand_code, lock_this_desktop,
    pointer_shape, press, run_as_administrator, runs_in, session_on_screen, somebody_signed_in,
    start_a_helper, start_at_sign_in, start_in_session, still_running, whoever_this_is,
};
#[cfg(windows)]
pub use inspection::describe_this_computer;
#[cfg(windows)]
pub use onscreen::{
    SessionProcess, errand, errand_code, lock_this_desktop, runs_in, session_on_screen,
    somebody_signed_in, start_a_helper, start_in_session, whoever_this_is,
};
#[cfg(windows)]
pub use pointer::pointer_shape;
#[cfg(windows)]
pub use reading::Vitals;
#[cfg(windows)]
pub use sign_in::start_at_sign_in;
pub use wifi::{Favouring, favour_latency};
#[cfg(windows)]
pub use zyr_win32::still_running;

/// A program to start in another session.
pub struct Launch<'a> {
    pub exe: &'a Path,
    pub arguments: &'a [String],
    pub working_dir: Option<&'a Path>,
    /// Where what it writes on its console goes.
    pub console: &'a Path,
    /// The line that marks, in that file, where this run begins.
    pub starting: &'a str,
}

/// How long an errand took, in its two halves.
///
/// Split, and not added together, because the two costs have nothing to
/// do with each other and only one of them is ever worth working on.
/// Getting a program running in another Windows session is Windows'
/// price, paid every time and roughly the same; what the program then
/// takes to answer is the errand itself. A single number cannot say
/// which of the two a session is waiting on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Errand {
    /// Time spent getting the program running over there.
    pub started: Duration,
    /// Time it then took to do what it went for and answer.
    pub answered: Duration,
}

impl fmt::Display for Errand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ms starting a program in the session on screen, {} ms waiting for it",
            self.started.as_millis(),
            self.answered.as_millis()
        )
    }
}

/// Whose a helper is, which decides what the desk lets it touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    /// The service's own account, moved onto that session's screen: enough
    /// to read a desk, change a screen, tap an engine on the shoulder.
    TheService,
    /// The person signed in at that screen, for a helper that does not
    /// merely look at the desk but acts on it in somebody's name: two
    /// programs on one desk may only speak to each other when they are
    /// the same person at the same level.
    ThePerson,
}

/// Why a program could not be run with administrator rights, or could not
/// be seen through.
#[derive(Debug)]
pub enum Refusal {
    /// The person turned the prompt down.
    Declined,
    /// Windows did not start it, and why.
    NotStarted(io::Error),
    /// Windows started it and handed back nothing to watch it by.
    Unwatched,
    /// Waiting for it to finish failed.
    NotWaited,
    /// It finished without Windows saying how.
    NoExitCode,
}

/// Runs a program and waits for what it says, with no console window
/// flashing up on the screen of whoever asked.
pub fn run_unseen(program: &Path, arguments: &[&str]) -> io::Result<Output> {
    let mut command = Command::new(program);
    command.args(arguments);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        /// What keeps the console window from being made at all.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.output()
}
