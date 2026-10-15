//! What the far end may not say to the engine, taken out on the way.
//!
//! The tunnel carries the engine's stream without reading it. Whoever
//! assembles a host side may hand it a sieve, which every piece coming
//! from the far end goes through before the engine sees it: the service
//! does so for someone it lets watch this computer without touching it.
//! What a piece means is the sieve's business; the tunnel only hands
//! the pieces over, in order, and carries on what comes back.

use std::io;

use zyr_transport::Bytes;

/// Goes over the far end's stream towards the engine, piece by piece.
pub trait Sieve: Send {
    /// What passes of this piece, possibly nothing. An error ends the
    /// stream, and the session with it.
    fn sifted(&mut self, piece: Bytes) -> io::Result<Bytes>;
}
