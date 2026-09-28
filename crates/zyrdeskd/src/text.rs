//! Text as Windows reads it: sixteen bits a letter, ending on a zero.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// Text written for Windows, with the zero it looks for at its end.
pub(crate) fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}
