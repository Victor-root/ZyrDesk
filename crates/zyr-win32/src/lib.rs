//! Windows' own way of saying things, for every brick that talks to it.
//!
//! Several bricks speak to Windows directly, and each used to write the
//! same few lines for itself: a text put in the shape Windows reads, a
//! text read back out of a buffer Windows filled, a handle closed whatever
//! happens, the program a process was started from, whether it is still
//! running, and a refusal written with the system's own number. They are
//! written once, here. Nothing else lives here: this is a toolbox, and
//! what the product does with Windows is decided by the bricks using it.
//!
//! The refusals are the one part that is not Windows alone. A refusal is
//! an `io::Error` on every system, and saying it with its number wherever
//! it is said is what lets the logic reporting it be tried anywhere.

use std::io;

#[cfg(windows)]
use std::ffi::{OsStr, OsString};
#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};

/// An error, with the system's own number for it when it gave one: the
/// number is what names the fault in Microsoft's documentation.
pub fn with_its_code(e: &io::Error) -> String {
    match e.raw_os_error() {
        Some(code) => format!("{e} (0x{:08X})", code as u32),
        None => e.to_string(),
    }
}

/// What the system just refused, named by the call that refused it and
/// with its own number for the refusal.
///
/// One line of the journal then says which step broke and why, where the
/// bare message would leave a person with « Accès refusé » and a dozen
/// calls to choose from.
pub fn refusal_of(call: &str) -> io::Error {
    let refused = io::Error::last_os_error();
    io::Error::new(
        refused.kind(),
        format!("{call}: {}", with_its_code(&refused)),
    )
}

/// Text written for Windows, with the zero it looks for at its end.
#[cfg(windows)]
pub fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Text Windows wrote into a buffer: up to its first zero, or the whole
/// buffer when it has none.
#[cfg(windows)]
pub fn read_wide(from: &[u16]) -> String {
    let end = from
        .iter()
        .position(|&letter| letter == 0)
        .unwrap_or(from.len());
    String::from_utf16_lossy(&from[..end])
}

/// Handle closed for certain, whatever happens next.
///
/// Windows handles leak silently: one error in the middle of a run of
/// calls is enough to abandon one, and nothing reports it.
#[cfg(windows)]
#[derive(Debug)]
pub struct Handle(pub HANDLE);

#[cfg(windows)]
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: the handle is valid and closed only here.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// The file a running program was started from.
#[cfg(windows)]
pub fn image_of(process: u32) -> Option<PathBuf> {
    // SAFETY: a refused or finished process gives a null handle, which its
    // guard leaves alone; a real one is closed by it.
    let handle = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) });
    if handle.0.is_null() {
        return None;
    }
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    // SAFETY: the handle is live, and the slot is ours with its length in
    // letters given alongside it.
    let read = unsafe {
        QueryFullProcessImageNameW(handle.0, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length)
    };
    (read != 0).then(|| PathBuf::from(OsString::from_wide(&path[..length as usize])))
}

/// Whether that process is still running.
#[cfg(windows)]
pub fn still_running(process: u32) -> bool {
    // SAFETY: a refused or finished process gives a null handle, which is
    // the answer we are after and which its guard leaves alone; a real one
    // is closed by it.
    let handle = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) });
    if handle.0.is_null() {
        return false;
    }
    let mut code = 0u32;
    // SAFETY: the handle is live and the slot is ours.
    let asked = unsafe { GetExitCodeProcess(handle.0, &mut code) };
    // A handle can outlive the process it names: only the exit code says
    // which of the two we are looking at.
    asked != 0 && code == STILL_ACTIVE as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_carries_the_systems_own_number_as_microsoft_writes_it() {
        let denied = io::Error::from_raw_os_error(5);
        assert_eq!(with_its_code(&denied), format!("{denied} (0x00000005)"));
        // An HRESULT reads the way the documentation prints it, sign bit
        // and all, rather than as a negative number nobody can look up.
        let hresult = io::Error::from_raw_os_error(0x8007_0005_u32 as i32);
        assert!(
            with_its_code(&hresult).ends_with("(0x80070005)"),
            "{}",
            with_its_code(&hresult)
        );
        // A refusal the system did not number carries no number.
        assert_eq!(with_its_code(&io::Error::other("refused")), "refused");
    }

    #[cfg(windows)]
    #[test]
    fn a_wide_string_ends_with_a_zero_and_reads_back() {
        assert_eq!(wide("Aé"), [0x41, 0xe9, 0]);
        let encoded = wide("desktop");
        assert_eq!(encoded.last(), Some(&0));
        assert_eq!(read_wide(&encoded), "desktop");
        // A buffer Windows filled to the brim carries no zero at all.
        let brim: Vec<u16> = "full".encode_utf16().collect();
        assert_eq!(read_wide(&brim), "full");
    }

    #[cfg(windows)]
    #[test]
    fn this_very_process_is_running_and_was_started_from_its_program() {
        let us = std::process::id();
        assert!(still_running(us));
        assert_eq!(
            image_of(us).map(|image| image.to_string_lossy().to_lowercase()),
            std::env::current_exe()
                .ok()
                .map(|exe| exe.to_string_lossy().to_lowercase())
        );
    }
}
