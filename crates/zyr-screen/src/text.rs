//! Text as Windows reads it and writes it: sixteen bits a letter,
//! ending on a zero.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// Text written for Windows, with the zero it looks for at its end.
pub(crate) fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Text Windows wrote into a buffer: up to its first zero, or the whole
/// buffer when it has none.
pub(crate) fn read_wide(from: &[u16]) -> String {
    let end = from
        .iter()
        .position(|&letter| letter == 0)
        .unwrap_or(from.len());
    String::from_utf16_lossy(&from[..end])
}
