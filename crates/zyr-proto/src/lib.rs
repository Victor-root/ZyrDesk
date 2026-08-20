//! Types and constants shared by the ZyrDesk components.

pub mod files;
pub mod log;
pub mod machine;
pub mod net;
pub mod paths;
pub mod random;
pub mod session;

/// Product version, the same for every binary in the workspace.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The code this binary was built from: commit and date.
///
/// Stamped in at build time. Every component opens its log with it, so a
/// fault is always read against the build that produced it rather than
/// against what anyone believes is installed.
pub const BUILD: &str = env!("ZYR_BUILD");

/// One line naming the product and the build behind it.
pub fn version_line() -> String {
    format!("ZyrDesk {PRODUCT_VERSION} ({BUILD})")
}
