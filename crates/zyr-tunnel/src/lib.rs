//! Multiplexage des flux des moteurs dans une connexion unique.

pub mod canal;
pub mod trame;

pub use canal::{CanalDatagramme, CanalFlux};
