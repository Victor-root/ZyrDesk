//! Types and constants shared by the ZyrDesk components.

pub mod clipboard;
pub mod files;
pub mod journal;
pub mod log;
pub mod machine;
pub mod net;
pub mod paths;
pub mod random;
pub mod session;
pub mod sifting;

/// Product version, the same for every binary in the workspace.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The code this binary was built from: commit and date.
///
/// Stamped in at build time. Every component opens its log with it, so a
/// fault is always read against the build that produced it rather than
/// against what anyone believes is installed.
pub const BUILD: &str = env!("ZYR_BUILD");

/// Whether this binary carries what is written only for a hunt.
///
/// Said out loud wherever the build is named, and it has to be: a
/// journal that holds no hunting lines is either a build that never
/// writes them or a moment when nothing happened, and those two read
/// exactly alike. Whoever is handed the journal has to be able to tell
/// which, or they spend an evening looking for a line that was never
/// going to be there.
pub const FOR_HUNTING: bool = cfg!(debug_assertions);

/// One line naming the product and the build behind it.
pub fn version_line() -> String {
    let hunting = if FOR_HUNTING {
        ", version de débogage"
    } else {
        ""
    };
    format!("ZyrDesk {PRODUCT_VERSION} ({BUILD}{hunting})")
}
