//! Driving the client engine.
//!
//! The engine is steered through its command line and its portable
//! mode: none of its windows are used, and its state is kept apart for
//! each remote device. Our own build shows no window of its own before
//! the picture and says on its way out what became of the session; see
//! `patches/MANIFEST.md`.

pub mod command;
pub mod follow;
pub mod process;
pub mod state;

pub use process::{ClientEngine, EngineError, Pairing, Session, SessionOutcome};
pub use state::DeviceState;
