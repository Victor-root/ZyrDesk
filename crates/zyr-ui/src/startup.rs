//! Whether ZyrDesk comes back on its own when Windows opens a session.
//!
//! The service answers before anybody has signed in; this window is what
//! says so, in the notification area. The two go together: a machine
//! reachable with nothing on screen to show it is exactly what nobody
//! should ship.
//!
//! Written where Windows itself looks, under the person's own account
//! and not the machine's, so it costs no administrator rights and
//! follows whoever asked for it rather than everybody on the computer.

// Tout ce qui est ici est demandé par l'accueil, que ce programme dessine
// lui-même, et ce qui dessine n'existe que sous Windows comme les
// fenêtres qu'il habille. Ailleurs, rien ne pose ces questions : le
// fichier reste compilé et vérifié, il n'est simplement appelé par
// personne.
#![cfg_attr(not(windows), allow(dead_code))]

/// Where Windows looks for what to start with a session.
#[cfg(windows)]
const WHERE: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Name of the entry, which is what the person reads in the task
/// manager's start-up tab.
#[cfg(windows)]
const ENTRY: &str = "ZyrDesk";

/// Decides whether ZyrDesk starts with the session.
#[cfg(windows)]
pub fn with_windows(on: bool) -> Result<(), String> {
    let program =
        std::env::current_exe().map_err(|e| format!("ce programme ne sait pas où il est : {e}"))?;
    if on {
        written(&format!("\"{}\"", program.display()))
    } else {
        erased()
    }
}

/// Outside Windows there is nothing to start with a session: the product
/// is a Windows one, and this exists so the rest stays compiled and
/// checked everywhere.
#[cfg(not(windows))]
pub fn with_windows(_on: bool) -> Result<(), String> {
    Err("le démarrage avec Windows n'existe que sous Windows".to_string())
}

#[cfg(windows)]
fn written(command: &str) -> Result<(), String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegOpenKeyExW, RegSetValueExW,
    };

    let path = wide(WHERE);
    let name = wide(ENTRY);
    let value = wide(command);

    // SAFETY: every pointer below is to a buffer that outlives the call,
    // and the key is closed on both ways out.
    unsafe {
        let mut key = std::ptr::null_mut();
        let opened = RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_SET_VALUE, &mut key);
        if opened != ERROR_SUCCESS {
            return Err(format!("le registre a refusé l'ouverture ({opened})"));
        }
        let written = RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast::<u8>(),
            // Bytes and not characters, the ending zero counted in.
            (value.len() * 2) as u32,
        );
        RegCloseKey(key);
        if written != ERROR_SUCCESS {
            return Err(format!("le registre a refusé l'écriture ({written})"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn erased() -> Result<(), String> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_SET_VALUE, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    };

    let path = wide(WHERE);
    let name = wide(ENTRY);

    // SAFETY: as above.
    unsafe {
        let mut key = std::ptr::null_mut();
        let opened = RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_SET_VALUE, &mut key);
        if opened != ERROR_SUCCESS {
            return Err(format!("le registre a refusé l'ouverture ({opened})"));
        }
        let erased = RegDeleteValueW(key, name.as_ptr());
        RegCloseKey(key);
        // Already absent is the state that was asked for, reached.
        if erased != ERROR_SUCCESS && erased != ERROR_FILE_NOT_FOUND {
            return Err(format!("le registre a refusé l'effacement ({erased})"));
        }
    }
    Ok(())
}

/// Text as Windows wants it: sixteen bits a character, ending on a zero.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
