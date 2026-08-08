//! What this computer is called.
//!
//! The name a person recognises their machine by is the one Windows
//! shows everywhere else: in the network neighbourhood, in the system
//! settings, on another computer's screen. Inventing our own would mean
//! the same machine answering to two names.

/// Name of this computer, as its owner knows it.
///
/// Falls back to something readable rather than failing: a nameless
/// machine in a list would be worse than an approximate one.
pub fn name() -> String {
    readable(raw_name())
}

#[cfg(windows)]
fn raw_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(not(windows))]
fn raw_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
}

/// Keeps only what makes a name, and gives one when there is none.
fn readable(found: Option<String>) -> String {
    let cleaned = found.unwrap_or_default().trim().to_string();
    if cleaned.is_empty() {
        "Cet ordinateur".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_always_has_a_name_to_show() {
        assert!(!name().is_empty());
    }

    #[test]
    fn nothing_readable_still_gives_something_to_display() {
        assert_eq!(readable(None), "Cet ordinateur");
        assert_eq!(readable(Some("   ".to_string())), "Cet ordinateur");
        assert_eq!(readable(Some("  PC-BUREAU\n".to_string())), "PC-BUREAU");
    }
}
