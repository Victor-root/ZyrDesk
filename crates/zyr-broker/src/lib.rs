//! What the service and the server say to each other.
//!
//! One crate, read by both sides, exactly as `zyr-control` is read by the
//! window and by the service: the messages of the live channel, the
//! bodies of the requests and answers, the tickets and passes the server
//! signs, and the proof a device gives of its own key. Nothing here
//! touches the network; everything here can be checked without one.
//!
//! JSON rather than the `verb key=value` lines of the internal channels:
//! this is an interface other programs may call, it reads at `curl`, and
//! its values are typed. Every message names the protocol version it
//! speaks, so that a device and a server installed at different dates
//! say so instead of misunderstanding each other.

pub mod code;
pub mod fingerprint;
pub mod live;
pub mod proof;
pub mod rest;
pub mod signing;
pub mod ticket;

pub use code::Code;
pub use signing::{Forged, ServerKey, ServerPublicKey, Signed};
pub use ticket::{Grant, Pass, Refusal, Ticket, Verifier, now};

/// Version of the dialect between a device and a server.
///
/// Raised whenever a message changes shape in a way an older half would
/// misread. A pair that does not speak the same version is refused at
/// the first exchange, with the version expected, so the person knows
/// which of the two to update.
pub const PROTOCOL: u32 = 1;
