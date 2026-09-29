//! What the service asks of Windows itself.
//!
//! A service lives in a session with no screen, no desktop and nobody in
//! front of it. What it does to the computer somebody is sitting at goes
//! through here: finding the session that holds the screen and who is in
//! it, starting this very program there, as the engine of a session, for
//! a short errand whose exit code is its answer, or as a helper left to
//! read for a while; locking that screen from inside it; pressing
//! Ctrl+Alt+Del, which Windows takes from a service and from nothing
//! else; asking the Wi-Fi to put a session first; and reading the shape
//! the pointer has on the desktop this program stands on.
//!
//! This crate knows Windows and nothing about ZyrDesk: which errand is
//! which, and when one is worth running, is decided by whoever asks.
//! Outside Windows everything still compiles and says what is true
//! there, which is that no session holds a screen: the product is built
//! and tested on machines that are not the ones it runs on.

#[cfg(windows)]
mod attention;
#[cfg(not(windows))]
mod elsewhere;
#[cfg(windows)]
mod onscreen;
#[cfg(windows)]
mod pointer;
mod wifi;

use std::fmt;
use std::time::Duration;

#[cfg(windows)]
pub use attention::{forget_it, let_it_be_pressed, press};
#[cfg(not(windows))]
pub use elsewhere::{
    errand, errand_code, lock_this_desktop, pointer_shape, press, runs_in, session_on_screen,
    somebody_signed_in, start_a_helper, still_running, whoever_this_is,
};
#[cfg(windows)]
pub use onscreen::{
    Launch, SessionProcess, errand, errand_code, lock_this_desktop, runs_in, session_on_screen,
    somebody_signed_in, start_a_helper, start_in_session, whoever_this_is,
};
#[cfg(windows)]
pub use pointer::pointer_shape;
pub use wifi::{Favouring, favour_latency};
#[cfg(windows)]
pub use zyr_win32::still_running;

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
