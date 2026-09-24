//! The service's control channel.
//!
//! The service owns what has to outlive the programs driving it: the
//! identity of this computer, the engine, and every end of tunnel. The
//! interface and the command line own nothing. They ask over this
//! channel, and what they asked for stays up once they are closed.
//!
//! That is what lets a session survive its window being shut, and what
//! lets the interface find the session again when it comes back.
//!
//! The engines talk to the service over a channel of their own, the
//! local link, made for one session and closed with it.

pub mod client;
pub mod link;
pub mod message;
pub mod pipe;
#[cfg(windows)]
mod windows_pipe;

pub use client::{ControlError, Service};
pub use message::{
    Account, Answer, Attach, Device, Holdup, Malformed, OfAccount, PROTOCOL, Peer, Reached,
    Registering, Request, Session, Standing, Watching, WayId,
};
pub use pipe::{CHANNEL, Door};
