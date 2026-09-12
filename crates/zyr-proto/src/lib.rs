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
/// rebuild, nothing to pass, and the same binary either way.
///
/// A file rather than something each program holds for itself, because
/// there are several of them: the window throws the switch, and the
/// service, which is another program entirely and the one that writes
/// most of what is worth hunting, has to be turned on by the same click.
///
/// Looked at again now and then rather than once: a switch that took a
/// restart of everything to take effect would be the same chore over
/// again, differently spelled. Between two looks the answer is held, so
/// a line that is not written costs a glance at a clock and two atomics.
pub fn for_hunting() -> bool {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Instant;

    /// Ce qu'on attend avant de redemander au disque, en millisecondes.
    const LOOK_AGAIN: u64 = 2_000;

    static HUNTING: AtomicBool = AtomicBool::new(false);
    static LOOKED: AtomicU64 = AtomicU64::new(0);
    static SINCE: OnceLock<Instant> = OnceLock::new();

    let now = SINCE.get_or_init(Instant::now).elapsed().as_millis() as u64;
    let looked = LOOKED.load(Ordering::Relaxed);
    // Zéro veut dire « jamais regardé », d'où le plancher à un : sans lui
    // le premier instant du programme se relirait comme jamais.
    if looked == 0 || now.saturating_sub(looked) >= LOOK_AGAIN {
        LOOKED.store(now.max(1), Ordering::Relaxed);
        HUNTING.store(paths::hunting().exists(), Ordering::Relaxed);
    }
    HUNTING.load(Ordering::Relaxed)
}

/// Turns the hunting lines on for every program of this product on this
/// computer, or off.
///
/// Thrown from the journal, where somebody already is when they want it,
/// and taken up by the service within a couple of seconds without either
/// of them being restarted.
pub fn hunt(on: bool) -> std::io::Result<()> {
    let named = paths::hunting();
    if !on {
        return match std::fs::remove_file(&named) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(folder) = named.parent() {
        std::fs::create_dir_all(folder)?;
    }
    // Ce qu'il y a dedans ne regarde personne : c'est sa présence qui dit
    // tout. Une phrase quand même, pour qui le trouverait sans savoir.
    std::fs::write(
        &named,
        "Tant que ce fichier est là, ZyrDesk écrit aussi ce qu'il compte et mesure.\n",
    )
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
