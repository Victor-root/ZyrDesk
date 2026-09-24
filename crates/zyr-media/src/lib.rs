//! Wire formats of the ZyrDesk engine.
//!
//! Everything the two halves of the engine say to each other, and nothing
//! that depends on an operating system: the packets that carry the picture
//! and the sound, the error correction that repairs them, the input events,
//! the messages of the control stream and of the local link, the cadence
//! at which pictures leave, and the measures a session keeps.

pub mod audio;
pub mod clock;
pub mod codec;
pub mod control;
pub mod input;
pub mod pace;
pub mod service;
pub mod stats;
pub mod video;

#[cfg(test)]
mod testing;
mod wire;

pub use wire::WireError;

/// Version of the engine's wire formats, checked at the first message.
pub const MEDIA_VERSION: u16 = 1;
