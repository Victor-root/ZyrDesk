//! Multiplexage des flux des moteurs dans une connexion unique.

pub mod canal;
pub mod pompe;
pub mod trame;
pub mod tunnel;

pub use canal::{CanalDatagramme, CanalFlux};
pub use pompe::{Releve, Statistiques};
pub use tunnel::Tunnel;
