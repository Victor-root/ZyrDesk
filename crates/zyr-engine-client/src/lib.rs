//! Driving the client engine, which is the official Moonlight, still
//! unmodified at this stage.
//!
//! The engine is steered through its command line and its portable
//! mode: none of its management windows are used, and its state is kept
//! apart for each remote device.

pub mod command;
pub mod process;
pub mod state;

pub use process::{ClientEngine, EngineError, Session, SessionOutcome};
pub use state::DeviceState;
