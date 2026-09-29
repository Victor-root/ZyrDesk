//! What Windows starts when the person signs in.
//!
//! Written where Windows itself looks, under the person's own account
//! and not the machine's, so it costs no administrator rights and
//! follows whoever asked for it rather than everybody on the computer.

use std::io;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegSetValueExW,
};

/// Where Windows looks for what to start with a session.
const WHERE: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Writes the command Windows runs under that name when the person signs
/// in, or with no command, takes the name away.
///
/// The name is what the person reads in the task manager's start-up tab.
pub fn start_at_sign_in(entry: &str, command: Option<&str>) -> io::Result<()> {
    let path = zyr_win32::wide(WHERE);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: a path of ours, and a key of ours to receive, closed on
    // every way out below.
    let opened =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_SET_VALUE, &mut key) };
    if opened != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "the registry refused to open ({opened})"
        )));
    }
    let done = match command {
        Some(command) => written(key, entry, command),
        None => erased(key, entry),
    };
    // SAFETY: the key opened above, closed once.
    unsafe { RegCloseKey(key) };
    done
}

fn written(key: HKEY, entry: &str, command: &str) -> io::Result<()> {
    let name = zyr_win32::wide(entry);
    let value = zyr_win32::wide(command);
    // SAFETY: a key open for writing, and texts that outlive the call.
    let written = unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast::<u8>(),
            // Bytes and not characters, the ending zero counted in.
            (value.len() * 2) as u32,
        )
    };
    if written != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "the registry refused to write ({written})"
        )));
    }
    Ok(())
}

fn erased(key: HKEY, entry: &str) -> io::Result<()> {
    let name = zyr_win32::wide(entry);
    // SAFETY: as above.
    let erased = unsafe { RegDeleteValueW(key, name.as_ptr()) };
    // Already absent is the state that was asked for, reached.
    if erased != ERROR_SUCCESS && erased != ERROR_FILE_NOT_FOUND {
        return Err(io::Error::other(format!(
            "the registry refused to erase ({erased})"
        )));
    }
    Ok(())
}
