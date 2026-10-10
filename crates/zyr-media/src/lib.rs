//! Wire formats of the ZyrDesk engine.
//!
//! Everything the two halves of the engine say to each other, and nothing
//! that depends on an operating system: the packets that carry the picture
//! and the sound, the error correction that repairs them, the input events,
//! the messages of the control stream and of the local link, the picture
//! of a pointer that has no name, the cadence at which pictures leave, the
//! rate the road between the two bears, and the measures a session keeps.

pub mod audio;
pub mod clock;
pub mod codec;
pub mod control;
pub mod input;
pub mod pace;
pub mod pointer;
pub mod road;
pub mod service;
pub mod stats;
pub mod trace;
pub mod video;

#[cfg(test)]
mod testing;
mod wire;

pub use wire::WireError;

/// Version of the engine's wire formats, checked at the first message.
///
/// Version 3 carries the picture of a pointer that a program draws for
/// itself, which no name of the system's says; version 4, the ceiling the
/// road between the two computers calls for.
pub const MEDIA_VERSION: u16 = 4;
