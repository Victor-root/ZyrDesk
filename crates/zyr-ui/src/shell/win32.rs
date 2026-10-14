//! What every window of this program says to Windows the same way.

use windows_sys::Win32::Foundation::LPARAM;
use zyr_proto::fact::Fact;

/// Where a mouse message says the pointer is, in the window's real
/// pixels: two numbers in the two halves of one. Signed, since a pointer
/// held by a drag reads negative left of the window or above it.
pub fn pointer_in(with: LPARAM) -> (i32, i32) {
    (
        i32::from((with & 0xFFFF) as i16),
        i32::from(((with >> 16) & 0xFFFF) as i16),
    )
}

/// The languages the person reads Windows in, in their order of
/// preference, the way Windows names them: `fr-FR`, then `en-US`.
///
/// Nothing when Windows will not say, which leaves the words in English.
pub fn preferred_languages() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

    let mut count = 0u32;
    let mut length = 0u32;
    // SAFETY: asked first for the length alone, with no buffer to fill.
    let sized = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
        )
    };
    if sized == 0 {
        return Vec::new();
    }
    let mut names = vec![0u16; length as usize];
    // SAFETY: the buffer holds exactly the length Windows just gave.
    let read = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            names.as_mut_ptr(),
            &mut length,
        )
    };
    if read == 0 {
        return Vec::new();
    }
    // One name after the other, each ended by a zero, the list by two.
    names
        .split(|letter| *letter == 0)
        .filter(|name| !name.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Opens a web address in the person's browser, the one Windows opens
/// addresses with.
pub fn open_in_the_browser(address: &str) -> Result<(), Fact> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = zyr_win32::wide("open");
    let address = zyr_win32::wide(address);
    // SAFETY: two words that outlive the call, and nothing else asked.
    let answer = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            address.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Anything above thirty-two is a success: what is below is the
    // error, in the numbers of an older Windows.
    if answer as isize > 32 {
        Ok(())
    } else {
        Err(Fact::new("window.link_not_opened").with("code", answer as isize))
    }
}

/// Whether the person lets Windows animate what it shows: a person who
/// asked it not to is spared what moves in this program too.
pub fn lets_things_move() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };

    let mut lively: i32 = 1;
    // SAFETY: a question whose answer is a BOOL written into ours.
    let asked = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut lively as *mut i32).cast(),
            0,
        )
    };
    asked == 0 || lively != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_left_of_the_window_reads_negative() {
        let with = ((-3i32 as u16 as isize) << 16) | (-12i32 as u16 as isize);
        assert_eq!(pointer_in(with), (-12, -3));
        assert_eq!(pointer_in((40 << 16) | 25), (25, 40));
    }
}
