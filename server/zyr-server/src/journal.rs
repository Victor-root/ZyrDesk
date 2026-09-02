//! What the server writes down.
//!
//! One line per event, on the standard output, which is journald on the
//! Debian it is made for: the timestamps are journald's, and there is no
//! file of ours to rotate. Never a secret, never a candidate address.

use std::fmt::Display;

pub fn say(line: impl Display) {
    println!("{line}");
}
