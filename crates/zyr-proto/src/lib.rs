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

/// Whether this computer is writing what is written only for a hunt.
///
/// Decided when the product starts, not when it is compiled. It was the
/// other way round at first, and that was wrong for a reason nothing in
/// the code could have shown: whoever builds this product builds it one
/// way, always the same way, and a line that only a second kind of build
/// ever writes is a line that is simply never there on the evening it is
/// wanted. A hunt then costs a rebuild of everything, a reinstall and a
/// lost afternoon, which is exactly when nobody has one.
///
/// So it is a file, put beside the journal it fills: present, this
/// computer writes the hunting lines; absent, it does not. Nothing to
/// rebuild, nothing to pass, and the same binary either way. Read once,
/// because the answer cannot change while the product runs and asking a
/// disk at every line would be its own kind of cost.
pub fn for_hunting() -> bool {
    use std::sync::OnceLock;

    static HUNTING: OnceLock<bool> = OnceLock::new();
    *HUNTING.get_or_init(|| paths::hunting().exists())
}

/// One line naming the product and the build behind it.
///
/// It says whether the hunting lines are being written, and it has to: a
/// journal holding none of them is either a computer that does not write
/// them or a moment when nothing happened, and those two read exactly
/// alike. Whoever is handed the journal must be able to tell which, or
/// they spend an evening looking for a line that was never going to be
/// there.
pub fn version_line() -> String {
    let hunting = if for_hunting() { ", à la chasse" } else { "" };
    format!("ZyrDesk {PRODUCT_VERSION} ({BUILD}{hunting})")
}
