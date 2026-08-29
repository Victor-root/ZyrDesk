//! A desktop larger than the panel it is drawn on.
//!
//! A laptop panel draws 1920x1200 and nothing else. Asked for a 4K
//! desktop it can only say no, and a session watching it from a 4K screen
//! then gets 1920x1200 blown up: the right shape, none of the detail.
//!
//! Windows can do better than that, and this is the half of it that the
//! old call cannot reach. The older interface has one size per screen and
//! that size is the signal sent to the panel, so it stops where the panel
//! stops. The newer one keeps the two apart: the **desktop** has a size,
//! the **panel** has a size, and the graphics card shrinks the first into
//! the second. Set that way, a 1920x1200 laptop really does draw a
//! 3840x2160 desktop, and what its owner sees on the panel is that
//! desktop made small, letterboxed where the shapes differ.
//!
//! That is what the reference product does, and it is what a picture of
//! its display settings showed: the laptop's own built-in panel, its own
//! brightness slider, and 3840 x 2160 in the resolution box.
//!
//! # Why this is the second thing tried and not the first
//!
//! Because it is the rarer half. A screen that offers the size asked for
//! is set through the old call, which is simple, well trodden and already
//! carries this product's sessions. This is for the screen that does not
//! offer it, where the choice is between a stretched desktop and a blurred
//! picture.
//!
//! # Where this runs
//!
//! In the session that owns the screen, like everything else about the
//! arrangement of screens: a service is on a window station with no
//! screens on it, and from there there is no desktop to resize.

use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_DEVICE_INFO_SET_SUPPORT_VIRTUAL_RESOLUTION, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
    DISPLAYCONFIG_MODE_INFO_TYPE_TARGET, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SCALING_ASPECTRATIOCENTEREDMAX, DISPLAYCONFIG_SCALING_IDENTITY,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_SUPPORT_VIRTUAL_RESOLUTION,
    DisplayConfigGetDeviceInfo, DisplayConfigSetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QDC_VIRTUAL_MODE_AWARE, QueryDisplayConfig, SDC_ALLOW_CHANGES,
    SDC_APPLY, SDC_USE_SUPPLIED_DISPLAY_CONFIG, SDC_VIRTUAL_MODE_AWARE, SetDisplayConfig,
};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;

/// Puts that screen's desktop at that size, whatever its panel draws,
/// and says what happened.
///
/// Says nothing at all when there was nothing to do, which is the answer
/// on every screen that was never stretched: this is asked of each of
/// them when a desk is put back, and a line each would bury the ones
/// that matter.
///
/// Never fails a session. A computer that will not take it serves the
/// size it has and the picture is stretched at the other end, which is
/// what happened before any of this existed.
pub fn put_at(screen: &str, wide: u32, high: u32) -> Option<String> {
    let Some((mut paths, mut modes)) = the_desktop_as_it_is() else {
        return Some(format!(
            "Windows would not describe its desktop, so {screen} was left alone"
        ));
    };
    let Some(path) = the_path_of(&paths, screen) else {
        return Some(format!("{screen} is not among the screens of this desktop"));
    };
    // Windows switches this off per screen, and a screen with it off
    // refuses every desktop larger than its panel. It is what the box
    // saying « the resolution you chose is not supported » is made of, and
    // turning it on is the whole difference between this working and not.
    let allowed = allow_more_than_the_panel(&paths[path]);
    // SAFETY: reading the union as the word it also is, which is how
    // both halves of the index are reached at once.
    let word = unsafe { paths[path].sourceInfo.Anonymous.modeInfoIdx };
    let source = match mode_at(word, &modes) {
        Some(source) if modes[source].infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE => source,
        _ => return Some(format!("{screen} has no desktop of its own to resize")),
    };
    // SAFETY: the union is read as what `infoType` just said it holds.
    let before = unsafe { modes[source].Anonymous.sourceMode };
    // Both halves have to be right, and the second is the one that was
    // missed. A desktop back at the size of its panel but still laid out
    // to be shrunk into it is one Windows goes on calling stretched, and
    // a stretched desktop cannot even be described to whatever reads a
    // screen next unless that reader was told to expect one.
    let wanted_scaling = if fills_the_panel(&paths[path], &modes, wide, high) {
        DISPLAYCONFIG_SCALING_IDENTITY
    } else {
        DISPLAYCONFIG_SCALING_ASPECTRATIOCENTEREDMAX
    };
    if (before.width, before.height) == (wide, high)
        && paths[path].targetInfo.scaling == wanted_scaling
    {
        return None;
    }
    modes[source].Anonymous.sourceMode.width = wide;
    modes[source].Anonymous.sourceMode.height = high;
    // Centred and keeping its shape rather than stretched to fill: a
    // desktop of one shape squeezed into a panel of another is everything
    // on it subtly the wrong shape, which is worse to sit in front of than
    // a black band. The panel keeps its own size either way.
    paths[path].targetInfo.scaling = wanted_scaling;
    // And the third block, which is the one that was missing. Told that a
    // desktop may differ from its panel, a path carries not two halves but
    // three: the desktop's own size, the panel's, and this, which says
    // where the desktop lands and how much of it is shown. Changing the
    // first without this one describes a desktop of one size laid out for
    // another, and Windows answers that the request itself is malformed
    // rather than that the size is impossible, which is exactly what it
    // answered.
    //
    // The whole desktop, shown whole: this product asks for a bigger desk,
    // never for a corner of one.
    if let Some(image) = the_desktop_image_of(&paths[path], &modes) {
        let whole = windows_sys::Win32::Foundation::RECTL {
            left: 0,
            top: 0,
            right: wide as i32,
            bottom: high as i32,
        };
        // Written as what this mode's `infoType` says it holds, which was
        // checked when it was found.
        modes[image].Anonymous.desktopImageInfo.PathSourceSize.x = wide as i32;
        modes[image].Anonymous.desktopImageInfo.PathSourceSize.y = high as i32;
        modes[image].Anonymous.desktopImageInfo.DesktopImageRegion = whole;
        modes[image].Anonymous.desktopImageInfo.DesktopImageClip = whole;
    }
    // SAFETY: both lists are ours, and their lengths are the ones being
    // handed over. Told to apply what is supplied rather than to work
    // something out, and told that a desktop may differ in size from the
    // panel it lands on, which is the whole point.
    let answer = unsafe {
        SetDisplayConfig(
            paths.len() as u32,
            paths.as_ptr(),
            modes.len() as u32,
            modes.as_ptr(),
            SDC_APPLY
                | SDC_USE_SUPPLIED_DISPLAY_CONFIG
                | SDC_ALLOW_CHANGES
                | SDC_VIRTUAL_MODE_AWARE,
        )
    };
    Some(if answer == ERROR_SUCCESS as i32 {
        format!(
            "{screen} draws a {wide}x{high} desktop{}{}",
            if wanted_scaling == DISPLAYCONFIG_SCALING_IDENTITY {
                ""
            } else {
                ", shrunk into its own panel"
            },
            if allowed {
                " (its panel had to be told to take desktops larger than itself)"
            } else {
                ""
            }
        )
    } else {
        format!(
            "Windows would not give {screen} a {wide}x{high} desktop: {}",
            why(answer)
        )
    })
}

/// The desktop as Windows describes it, told that a desktop may be
/// larger than the panel it is drawn on.
///
/// Asked that way from the start and not only when writing: the lists
/// that come back are the ones handed straight back, and a list read
/// without that flag describes a world where the two sizes are the same.
fn the_desktop_as_it_is() -> Option<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>)> {
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

/// Which of those paths carries the screen Windows takes orders about
/// under that name.
fn the_path_of(paths: &[DISPLAYCONFIG_PATH_INFO], screen: &str) -> Option<usize> {
    paths.iter().position(|path| {
        let mut about: DISPLAYCONFIG_SOURCE_DEVICE_NAME = unsafe { std::mem::zeroed() };
        about.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        about.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        about.header.adapterId = path.sourceInfo.adapterId;
        about.header.id = path.sourceInfo.id;
        // SAFETY: the block is ours, opens on the header the call reads,
        // and carries its own size as that header requires.
        if unsafe { DisplayConfigGetDeviceInfo((&raw mut about).cast()) } != ERROR_SUCCESS as i32 {
            return false;
        }
        let end = about
            .viewGdiDeviceName
            .iter()
            .position(|letter| *letter == 0)
            .unwrap_or(0);
        String::from_utf16_lossy(&about.viewGdiDeviceName[..end]) == screen
    })
}

/// Lets that screen take a desktop larger than its own panel, and says
/// whether it had to be told.
///
/// Windows keeps this per screen and turns it off readily, and a screen
/// with it off refuses every such desktop out of hand. It is put back the
/// way it was found nowhere: what it allows is a size being offered, not
/// a size being used, and leaving it on costs the person nothing.
fn allow_more_than_the_panel(path: &DISPLAYCONFIG_PATH_INFO) -> bool {
    let mut asking: DISPLAYCONFIG_SUPPORT_VIRTUAL_RESOLUTION = unsafe { std::mem::zeroed() };
    asking.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: DISPLAYCONFIG_DEVICE_INFO_SET_SUPPORT_VIRTUAL_RESOLUTION,
        size: size_of::<DISPLAYCONFIG_SUPPORT_VIRTUAL_RESOLUTION>() as u32,
        adapterId: path.targetInfo.adapterId,
        id: path.targetInfo.id,
    };
    // Nought in the one bit this carries is « do not disable it », which
    // is the double negative Windows chose and the thing wanted here.
    asking.Anonymous.Anonymous._bitfield = 0;
    // SAFETY: as the reads above, on the call that writes.
    unsafe { DisplayConfigSetDeviceInfo((&raw mut asking).cast()) == ERROR_SUCCESS as i32 }
}

/// Where that path keeps the block saying how its desktop lands on its
/// panel, when it has one.
///
/// The lower half of the same word that carries the panel's own mode in
/// its upper half, which is the sort of thing that is obvious only once
/// it has cost an afternoon.
fn the_desktop_image_of(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Option<usize> {
    // SAFETY: reading the union as the word it also is.
    let word = unsafe { path.targetInfo.Anonymous.modeInfoIdx };
    let index = word & u32::from(u16::MAX);
    (index != u32::from(u16::MAX))
        .then_some(index as usize)
        .filter(|index| {
            *index < modes.len()
                && modes[*index].infoType == DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE
        })
}

/// Whether a desktop that size lands on the panel one pixel for one,
/// which is when there is nothing to shrink and nothing to letterbox.
fn fills_the_panel(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
    wide: u32,
    high: u32,
) -> bool {
    // SAFETY: as on the source side, reading the union as a word.
    let word = unsafe { path.targetInfo.Anonymous.modeInfoIdx };
    let Some(target) = mode_at(word, modes) else {
        return false;
    };
    if modes[target].infoType != DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
        return false;
    }
    // SAFETY: the union is read as what `infoType` just said it holds.
    let signal = unsafe { modes[target].Anonymous.targetMode.targetVideoSignalInfo };
    (signal.activeSize.cx, signal.activeSize.cy) == (wide, high)
}

/// Where in the list of modes a path keeps one of its two halves.
///
/// The index sits in a union whose shape depends on how the list was
/// read, and this product reads it one way only: told that a desktop may
/// differ from its panel, the whole word becomes two halves, and the one
/// wanted is the upper one at both ends of a path. The lower half is
/// something else entirely on each side, which is why reading the word as
/// a number would name a mode that has nothing to do with this screen.
///
/// All ones is how a path says it has no such mode.
fn mode_at(word: u32, modes: &[DISPLAYCONFIG_MODE_INFO]) -> Option<usize> {
    let index = word >> 16;
    (index != u32::from(u16::MAX))
        .then_some(index as usize)
        .filter(|index| *index < modes.len())
}

/// What Windows means by the number this call answers with.
///
/// Named because the difference between them decides what to try next: a
/// malformed request is ours to fix, a size refused is the machine's
/// answer, and there is no telling them apart from a bare number.
fn why(answer: i32) -> String {
    match answer {
        5 => "it is not allowed to".to_string(),
        31 => "it failed".to_string(),
        50 => "this computer does not support it".to_string(),
        87 => "the request itself was malformed, which is ours to fix".to_string(),
        1004 => "the flags were wrong".to_string(),
        other => format!("answer {other}"),
    }
}
