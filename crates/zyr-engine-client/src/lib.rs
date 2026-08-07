//! Supervision du moteur client (Moonlight officiel, non modifié à ce stade).
//!
//! Le moteur est piloté par sa ligne de commande et son mode portable :
//! aucune de ses fenêtres de gestion n'est utilisée, et son état est
//! cloisonné par appareil distant.

pub mod command;
pub mod process;
pub mod state;

pub use process::{ClientEngine, ErreurMoteur, IssueSession};
pub use state::DeviceState;
