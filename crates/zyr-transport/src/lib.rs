//! Transport ZyrDesk.
//!
//! Le tunnel qui relie les deux ordinateurs, et le calcul de ce qu'on
//! peut y faire passer d'un seul bloc.

pub mod chemin;
pub mod congestion;
pub mod identite;
pub mod mtu;
pub mod point;

pub use chemin::Chemin;
pub use congestion::{ControleurMedia, ProfilMedia};
pub use identite::{Empreinte, Identite};
pub use mtu::{TaillePaquet, taille_paquet};
pub use point::{
    Bytes, Connexion, ErreurDatagramme, ErreurPoint, FluxEnvoi, FluxReception, PointTerminal,
};
