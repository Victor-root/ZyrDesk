//! Types and constants shared by the ZyrDesk components.

pub mod net;
pub mod paths;
pub mod random;
pub mod session;

/// Product version, the same for every binary in the workspace.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");
