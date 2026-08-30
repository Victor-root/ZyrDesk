//! What Windows says its desktop is made of, asked once and asked right.
//!
//! Three things in this crate need Windows to describe the desktop: how
//! large a screen draws, whether a screen can carry a desktop bigger than
//! its own panel, and which screen answers to a given name. They all ask
//! the same question, and they all have to ask it the same way: told that
//! a desktop may differ in size from the panel it lands on. Asked without
//! that, Windows cannot describe a screen in that state at all and
//! answers that this computer has no screens, which once left a laptop
//! with a stretched desktop invisible to everything that looked for it.
//!
//! So the question lives here, once, and the three ask it through this.

use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QDC_VIRTUAL_MODE_AWARE, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;

/// The desktop as Windows describes it, panels and desktops told apart.
pub(crate) fn as_windows_has_it()
-> Option<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>)> {
    let asked = QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE;
    let mut paths = 0u32;
    let mut modes = 0u32;
    // SAFETY: both counts are ours, and nothing else is handed over.
    if unsafe { GetDisplayConfigBufferSizes(asked, &mut paths, &mut modes) } != ERROR_SUCCESS {
        return None;
    }
    let mut found: Vec<DISPLAYCONFIG_PATH_INFO> =
        vec![unsafe { std::mem::zeroed() }; paths as usize];
    let mut shapes: Vec<DISPLAYCONFIG_MODE_INFO> =
        vec![unsafe { std::mem::zeroed() }; modes as usize];
    // SAFETY: both lists are ours and hold what the counts above asked
    // for, and the counts go back in so the call can shorten them.
    if unsafe {
        QueryDisplayConfig(
            asked,
            &mut paths,
            found.as_mut_ptr(),
            &mut modes,
            shapes.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    found.truncate(paths as usize);
    shapes.truncate(modes as usize);
    Some((found, shapes))
}

/// The name Windows takes orders about that screen under, `\\.\DISPLAY1`
/// and its like.
///
/// The join between the two halves of Windows: the call that arranges
/// screens takes this name, the call that describes them takes a pair of
/// numbers, and only the second can be asked to say the first.
pub(crate) fn gdi_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut about: DISPLAYCONFIG_SOURCE_DEVICE_NAME = unsafe { std::mem::zeroed() };
    about.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
    about.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
    about.header.adapterId = path.sourceInfo.adapterId;
    about.header.id = path.sourceInfo.id;
    // SAFETY: the block is ours, opens on the header the call reads, and
    // carries its own size as that header requires.
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut about).cast()) } != ERROR_SUCCESS as i32 {
        return None;
    }
    Some(ending_at_its_nought(&about.viewGdiDeviceName))
}

/// What the screen on that path calls itself.
///
/// Out of the little block of identity every screen publishes, which is
/// the same name the host engine lists its screens by. It is the only
/// name of a screen that does not move with what else is plugged in.
fn how_it_introduces_itself(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut about: DISPLAYCONFIG_TARGET_DEVICE_NAME = unsafe { std::mem::zeroed() };
    about.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
    about.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
    about.header.adapterId = path.targetInfo.adapterId;
    about.header.id = path.targetInfo.id;
    // SAFETY: as above, on the other end of the same path.
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut about).cast()) } != ERROR_SUCCESS as i32 {
        return None;
    }
    Some(ending_at_its_nought(&about.monitorFriendlyDeviceName))
}

/// The screen this computer grew for itself, under the name Windows
/// takes orders about, once it is awake.
///
/// Found by what it says about itself and not by its place in the list,
/// which moves with everything else plugged in, nor by the driver's own
/// name, which names a device and not a screen. It is the same name the
/// host engine picks it out by, so the two always mean the same screen.
pub fn the_screen_the_driver_grew(driver: &dyn crate::driver::Driver) -> Option<String> {
    let (paths, _) = as_windows_has_it()?;
    paths.iter().find_map(|path| {
        driver
            .is_its_screen(&how_it_introduces_itself(path)?)
            .then(|| gdi_name(path))?
    })
}

fn ending_at_its_nought(letters: &[u16]) -> String {
    let end = letters
        .iter()
        .position(|letter| *letter == 0)
        .unwrap_or(letters.len());
    String::from_utf16_lossy(&letters[..end])
}
