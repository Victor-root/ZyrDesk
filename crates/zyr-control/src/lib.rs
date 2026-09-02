//! The service's control channel.
//!
//! The service owns what has to outlive the programs driving it: the
//! identity of this computer, the engine, and every end of tunnel. The
//! interface and the command line own nothing. They ask over this
//! channel, and what they asked for stays up once they are closed.
//!
//! That is what lets a session survive its window being shut, and what
//! lets the interface find the session again when it comes back.

pub mod client;
pub mod message;
pub mod pipe;

pub use client::{ControlError, Service};
pub use message::{
    Account, Answer, Attach, Device, Holdup, Malformed, OfAccount, PROTOCOL, Peer, Reached,
    Registering, Request, Session, Standing, WayId,
};
pub use pipe::{CHANNEL, Door};
