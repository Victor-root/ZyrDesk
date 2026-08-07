//! Transport ZyrDesk.
//!
//! Le tunnel qui relie les deux ordinateurs, et le calcul de ce qu'on
//! peut y faire passer d'un seul bloc.

pub mod congestion;
pub mod mtu;

pub use congestion::{ControleurMedia, ProfilMedia};
pub use mtu::{TaillePaquet, taille_paquet};
