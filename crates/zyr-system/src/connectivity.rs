//! Whether this computer reaches the Internet, only its own network, or
//! nothing, as Windows judges it.
//!
//! Windows' own answer and not one worked out here from the cards: a
//! machine always has some address on a virtual adapter or a VPN, and
//! one that has lost its Wi-Fi would still look connected to anybody
//! counting addresses. Windows tries the roads out and says what it
//! found, which is what its own icon by the clock shows.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::Networking::WinSock::{
    NL_NETWORK_CONNECTIVITY_HINT, NetworkConnectivityLevelHintConstrainedInternetAccess,
    NetworkConnectivityLevelHintInternetAccess, NetworkConnectivityLevelHintLocalAccess,
    NetworkConnectivityLevelHintNone,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows_sys::core::w;

/// How far this computer reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    /// The Internet, possibly behind a page asking to sign in first.
    Internet,
    /// Its own network, and nothing beyond.
    LocalOnly,
    /// No network at all.
    Offline,
}

/// The way Windows says it, from Windows 10 version 2004 on.
type Hint = unsafe extern "system" fn(*mut NL_NETWORK_CONNECTIVITY_HINT) -> WIN32_ERROR;

/// How far this computer reaches, or nothing when Windows cannot say:
/// an older Windows has no such answer, and one that is still judging
/// says it does not know yet.
pub fn connectivity() -> Option<Connectivity> {
    let ask = hint()?;
    // SAFETY: a structure of ours, all of whose fields are numbers.
    let mut said: NL_NETWORK_CONNECTIVITY_HINT = unsafe { std::mem::zeroed() };
    // SAFETY: the function Windows exports under that name, of the type
    // its header declares, given the structure above.
    if unsafe { ask(&mut said) } != ERROR_SUCCESS {
        return None;
    }
    let level = said.ConnectivityLevel;
    if level == NetworkConnectivityLevelHintInternetAccess
        || level == NetworkConnectivityLevelHintConstrainedInternetAccess
    {
        Some(Connectivity::Internet)
    } else if level == NetworkConnectivityLevelHintLocalAccess {
        Some(Connectivity::LocalOnly)
    } else if level == NetworkConnectivityLevelHintNone {
        Some(Connectivity::Offline)
    } else {
        None
    }
}

/// The function, looked for once, when Windows has it.
///
/// Looked for and not linked: a program linked to it would not start at
/// all on a Windows older than the function, for the sake of one line of
/// the home screen.
fn hint() -> Option<Hint> {
    static FOUND: OnceLock<Option<Hint>> = OnceLock::new();
    *FOUND.get_or_init(|| {
        // SAFETY: a library of the system's own, looked for in System32
        // alone. It is not freed: it stays loaded in every program that
        // has a network, and the function is kept for the program's life.
        let library = unsafe {
            LoadLibraryExW(
                w!("iphlpapi.dll"),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if library.is_null() {
            return None;
        }
        // SAFETY: a library loaded, and a terminated name.
        let found =
            unsafe { GetProcAddress(library, c"GetNetworkConnectivityHint".as_ptr().cast()) }?;
        // SAFETY: that function's address, whose type is the one its
        // header declares.
        Some(unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, Hint>(found) })
    })
}
