//! Who sits at this window, and what their desktop looks like.
//!
//! The window greets the person by their name and shows this computer
//! with its own wallpaper, so both are read where the person is: in the
//! window's own process, which runs as them.

use std::path::PathBuf;
use std::ptr;

use windows_sys::Win32::Graphics::Gdi::{COLOR_DESKTOP, GetSysColor};
use windows_sys::Win32::NetworkManagement::NetManagement::{
    NetApiBufferFree, NetUserGetInfo, USER_INFO_2,
};
use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETDESKWALLPAPER, SystemParametersInfoW};
use zyr_win32::read_wide;

/// The name of whoever is signed in, as they would write it.
///
/// Their full name when Windows knows it: the one a company's directory
/// gives, or the one the account was set up with, which for an account
/// tied to Microsoft is the name of that account. Their sign-in name
/// otherwise, which is always there but is an identifier, not a name.
pub fn name_of_the_person() -> Option<String> {
    let said = |name: String| Some(name.trim().to_string()).filter(|name| !name.is_empty());
    directory_name()
        .and_then(said)
        .or_else(|| sign_in_name().and_then(|login| full_name(&login).and_then(said)))
        .or_else(|| sign_in_name().and_then(said))
}

/// The name a company's directory gives the person, on a computer that
/// belongs to one.
fn directory_name() -> Option<String> {
    let mut room = 0u32;
    // SAFETY: no buffer, which asks how much room the name needs.
    unsafe { GetUserNameExW(NameDisplay, ptr::null_mut(), &mut room) };
    if room == 0 {
        return None;
    }
    let mut spelled = vec![0u16; room as usize];
    // SAFETY: a buffer of exactly the room asked for, which the call
    // writes the name into and the length back.
    if !unsafe { GetUserNameExW(NameDisplay, spelled.as_mut_ptr(), &mut room) } {
        return None;
    }
    Some(read_wide(&spelled))
}

/// The name the person signs in with.
fn sign_in_name() -> Option<String> {
    let mut spelled = [0u16; 257];
    let mut room = spelled.len() as u32;
    // SAFETY: a buffer of ours, whose length is handed over and written
    // back as the length of what was put in it.
    if unsafe { GetUserNameW(spelled.as_mut_ptr(), &mut room) } == 0 {
        return None;
    }
    Some(read_wide(&spelled))
}

/// The full name written on this computer for that account.
fn full_name(login: &str) -> Option<String> {
    let login = zyr_win32::wide(login);
    let mut found: *mut u8 = ptr::null_mut();
    // SAFETY: this computer's own accounts, a name that outlives the
    // call, and a block the system allocates and that is freed once
    // below.
    unsafe {
        if NetUserGetInfo(ptr::null(), login.as_ptr(), 2, &mut found) != 0 || found.is_null() {
            return None;
        }
        let info = &*(found as *const USER_INFO_2);
        let name = crate::onscreen::wide_text(info.usri2_full_name);
        NetApiBufferFree(found.cast());
        Some(name)
    }
}

/// What the desktop of this computer shows: a picture when it has one,
/// and its colour, which is what shows around a picture that does not
/// fill the screen and instead of one when there is none.
pub struct Wallpaper {
    pub picture: Option<PathBuf>,
    /// Red, green and blue.
    pub colour: [u8; 3],
}

/// The wallpaper of whoever is signed in.
///
/// The picture Windows says it shows, or, when it names none or names a
/// file that is gone, the copy Windows keeps of the one on screen: a
/// slideshow or a picture of the day names nothing that lasts, but that
/// copy is always the one being shown.
pub fn wallpaper() -> Wallpaper {
    let mut spelled = [0u16; 1024];
    // SAFETY: a buffer of ours, of the size handed over, which the call
    // fills with a path ended by a nought.
    let said = unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            spelled.len() as u32,
            spelled.as_mut_ptr().cast(),
            0,
        )
    } != 0;
    let named = said
        .then(|| PathBuf::from(read_wide(&spelled)))
        .filter(|path| !path.as_os_str().is_empty() && path.is_file());
    let kept = std::env::var_os("APPDATA")
        .map(|roaming| PathBuf::from(roaming).join(r"Microsoft\Windows\Themes\TranscodedWallpaper"))
        .filter(|path| path.is_file());
    // SAFETY: no argument beyond the colour asked for.
    let colour = unsafe { GetSysColor(COLOR_DESKTOP) };
    Wallpaper {
        picture: named.or(kept),
        colour: [
            (colour & 0xFF) as u8,
            ((colour >> 8) & 0xFF) as u8,
            ((colour >> 16) & 0xFF) as u8,
        ],
    }
}
