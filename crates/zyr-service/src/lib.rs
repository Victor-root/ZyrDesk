//! What the ZyrDesk service does, whatever runs it.
//!
//! The service is the part of the product that is always there on a
//! computer: it holds the door other computers come in through, the ways
//! this one reaches out on, the link to an account, and the desk the
//! window and the command line talk to. All of that is here, and none of
//! how Windows runs it. The program `zyrdeskd` answers Windows' service
//! manager, lays on this computer what the service needs, and wires this
//! to the engine it starts for each session: see [`Wiring`].
//!
//! The service also starts that very program again in the session that
//! owns the screen, for what cannot be done from its own. What each
//! start is for, and what it does over there, is here as well: the
//! program reads [`started_for`] and hands each purpose to whoever does
//! it.
//!
//! Nothing here asks Windows anything directly: that goes through the
//! platform bricks, which answer honestly on any other system. So all of
//! it is built and tried everywhere.

mod account;
mod clipboard;
mod control;
mod engine;
mod errands;
mod gateway;
mod incoming;
mod keeper;
mod known;
mod machine;
mod outside;
mod pointer;
mod preferences;
mod said;
mod screen;
mod speakers;
mod supervisor;
mod transfer;
mod ways;

pub use clipboard::carry_the_clipboard_here;
pub use errands::{Desk, StartedFor, do_this_to_the_desk, move_the_speakers, started_for};
pub use pointer::follow_the_pointer_here;
pub use supervisor::{End, StopOrder, Wiring, run};
