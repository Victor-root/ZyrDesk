//! Driving the host engine, which is the official Sunshine, unmodified.
//!
//! The engine is steered through its official interfaces only:
//! configuration file, command line, and local REST API. The values
//! produced here apply the policy described in docs/engines/STRATEGY.md:
//! strict loopback binding, locked web interface, disabled tray icon,
//! and state paths under the ZyrDesk data folder.

pub mod api;
pub mod config;
pub mod credentials;
pub mod launch;
pub mod ports;
pub mod process;
pub mod runtime;

pub use config::{InnerEncryption, Listening, SunshineConfig};
pub use credentials::Credentials;
pub use launch::{Launch, Launcher, Parting, Running, SameSession};
pub use process::HostEngine;
pub use runtime::EngineRuntime;
