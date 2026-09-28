//! What every window of this program says to Windows the same way.

use windows_sys::Win32::Foundation::LPARAM;

/// Text as Windows reads it: sixteen bits a character, ended by the zero
/// it looks for.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Where a mouse message says the pointer is, in the window's real
/// pixels: two numbers in the two halves of one. Signed, since a pointer
/// held by a drag reads negative left of the window or above it.
pub fn pointer_in(with: LPARAM) -> (i32, i32) {
    (
        i32::from((with & 0xFFFF) as i16),
        i32::from(((with >> 16) & 0xFFFF) as i16),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_ends_on_the_zero_windows_looks_for() {
        assert_eq!(wide("Aé"), [0x41, 0xe9, 0]);
    }

    #[test]
    fn a_pointer_left_of_the_window_reads_negative() {
        let with = ((-3i32 as u16 as isize) << 16) | (-12i32 as u16 as isize);
        assert_eq!(pointer_in(with), (-12, -3));
        assert_eq!(pointer_in((40 << 16) | 25), (25, 40));
    }
}
