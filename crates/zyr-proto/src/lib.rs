//! Types et constantes partagés entre les composants ZyrDesk.

pub mod net;

/// Version du produit, unique pour tous les binaires du workspace.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");
