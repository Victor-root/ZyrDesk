//! How large Windows draws everything on a screen.
//!
//! A remote desktop that carries the size of a screen but not how large
//! that screen draws is only half a remote desktop. A laptop at
//! 1920x1200 with everything drawn a quarter larger than life is not the
//! same desk as the same panel at life size: the text is half as tall,
//! and the person who asked to see their own screen sees somebody else's
//! idea of it.
//!
//! Read as well as written, because it is half of what a screen is: an
//! arrangement noted before a session and put back after it carries this
//! number beside the size, or the desk comes back with everything on it
//! the wrong size.
//!
//! Windows keeps that number per screen and offers no documented way to
//! set it. What it does have is a private message on the same call that
//! reads the display configuration, which its own settings page uses and
//! which every tool that moves this number uses too. It has been there
//! since Windows 8.1 and has not moved since.
//!
//! Two things follow from that, and both are honoured below. It is asked
//! for by number rather than by name, so the numbers are written here
//! with what they mean; and it is never allowed to fail anything: a
//! session on a screen drawn at the wrong size is a session, a session
//! refused over it is not.
//!
//! # Where this runs
//!
//! In the session that owns the screen, and nowhere else. Everything
//! about the display configuration is answered for the window station of
//! whoever asks, and a service sits on one with no screens at all: asked
//! from there, this finds nothing to set. The service therefore sends
//! this errand into the session on screen, the same way it sends the one
//! that puts the lock screen up.

/// The magnifications Windows offers, in the order it offers them.
///
/// Fixed by Windows and not by us: the private message below speaks in
/// steps along this list rather than in percentages, so the list is what
/// turns one into the other. A percentage that is not on it is not a
/// magnification Windows can be asked for.
const OFFERED: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];

/// Reading the magnification of a screen, and setting it.
///
/// Two numbers Windows does not publish, on a call it does. They are
/// negative because everything Windows does publish on that call is
/// positive, which is how a private message stays out of the way of the
/// ones anybody may use.
const READ_THE_MAGNIFICATION: i32 = -3;
const SET_THE_MAGNIFICATION: i32 = -4;

/// What a screen answers about its own magnification.
///
/// The three are steps along [`OFFERED`] and not percentages, counted
/// from the one Windows recommends for that screen: `lowest` is how far
/// below the recommended one it will go, `highest` how far above, and
/// `current` where it stands now. The recommended one itself is
/// therefore at `-lowest` in the list, which is the whole of what turns
/// these steps into percentages.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Magnification {
    header: Header,
    lowest: i32,
    current: i32,
    highest: i32,
}

/// What is sent to move it, in the same steps.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Wanted {
    header: Header,
    step: i32,
}

/// Who is being asked about, and what is being asked.
///
/// The shape Windows expects at the top of every one of these blocks. It
/// is written out here rather than borrowed from the crate that binds
/// Windows, because the two messages this file sends are not in it.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Header {
    kind: i32,
    size: u32,
    adapter: Luid,
    id: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Luid {
    low: u32,
    high: i32,
}

/// How large that screen draws, in per cent, or what stopped the answer.
///
/// Three different things can stop it and they call for three different
/// fixes, so they are told apart here rather than folded into one
/// « nothing ». That distinction is not a nicety: a desk came back from a
/// session with its magnification not put back, and the only thing the
/// journal could say was that the screen never came back, which is what
/// all three look like from outside.
pub fn reading(screen: &str) -> Result<u32, String> {
    let Some((adapter, id)) = the_screen_called(screen) else {
        return Err(format!(
            "{screen} is not among the screens Windows describes"
        ));
    };
    let Some(now) = read(adapter, id) else {
        return Err(format!("Windows would not say how large {screen} draws"));
    };
    let recommended = usize::try_from(-now.lowest).unwrap_or(0);
    let percent = at(recommended, now.current);
    if percent == 0 {
        return Err(format!(
            "{screen} draws at a magnification not on the list Windows offers: it is at step {} \
             counted from step {recommended}, which goes from {} to {}",
            now.current, now.lowest, now.highest
        ));
    }
    Ok(percent)
}

/// The same, for whoever has nothing to do with a refusal.
///
/// Read by the arrangement that is noted before a session, so the desk
/// comes back with everything on it the size it was.
pub fn of(screen: &str) -> Option<u32> {
    reading(screen).ok()
}

/// Sets how large that screen draws, and says what happened.
///
/// Nought asks for the magnification Windows recommends for it, which is
/// what a session that could not measure the screen it is watched on
/// wants: a magnification taken off a panel nobody is looking at is
/// worse than the one this computer would have chosen itself.
///
/// Never fails anything: the answer is a sentence for the journal, and a
/// screen at the wrong magnification is a screen.
pub fn magnify(screen: &str, percent: u32) -> String {
    if percent != 0 && !OFFERED.contains(&percent) {
        return format!(
            "{percent} % is not a magnification Windows offers, {screen} keeps its own"
        );
    }
    let Some((adapter, id)) = the_screen_called(screen) else {
        return format!(
            "{screen} is not among the screens this session shows, so how large it draws was left \
             alone"
        );
    };
    let now = match read(adapter, id) {
        Some(now) => now,
        None => {
            return format!("Windows would not say how large {screen} draws, so it was left alone");
        }
    };
    // The recommended magnification sits at `-lowest` in the list, so a
    // step is the distance from there to the one wanted. Nothing wanted
    // is the recommended one itself, which is the step of nought: the
    // guard above leaves that as the only way past the search.
    let recommended = usize::try_from(-now.lowest).unwrap_or(0);
    let step = match OFFERED.iter().position(|offered| *offered == percent) {
        Some(wanted) => {
            i32::try_from(wanted).unwrap_or(0) - i32::try_from(recommended).unwrap_or(0)
        }
        None => 0,
    };
    let wanted = at(recommended, step);
    if step < now.lowest || step > now.highest {
        return format!(
            "{screen} will not draw at {wanted} %: Windows offers it from {} % to {} %",
            at(recommended, now.lowest),
            at(recommended, now.highest)
        );
    }
    if step == now.current {
        return format!("{screen} already draws at {wanted} %");
    }
    if set(adapter, id, step) {
        format!("{screen} draws at {wanted} %")
    } else {
        format!("Windows refused to make {screen} draw at {wanted} %")
    }
}

/// The percentage a step stands for, for a sentence that names both ends.
fn at(recommended: usize, step: i32) -> u32 {
    let index = i32::try_from(recommended).unwrap_or(0) + step;
    usize::try_from(index)
        .ok()
        .and_then(|index| OFFERED.get(index).copied())
        .unwrap_or(0)
}

/// Which of this session's screens Windows takes orders about under that
/// name, `\\.\DISPLAY1` and its like.
///
/// The two halves of Windows name screens differently, and this is the
/// join between them: the call that arranges screens takes the name in
/// the list, while the one that carries the magnification takes a pair of
/// numbers. Only the newer call can be asked to say both, so it is asked.
fn the_screen_called(screen: &str) -> Option<(Luid, u32)> {
    use windows_sys::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
        GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QDC_VIRTUAL_MODE_AWARE,
        QueryDisplayConfig,
    };
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;

    // Told that a desktop may differ in size from the panel it is drawn
    // on, because on this machine it sometimes does. Asked without that,
    // Windows cannot describe a desktop in that state at all and answers
    // that it has no screens: a laptop whose desktop had been stretched
    // for a session was then never found again, so its magnification was
    // never put back and the desk was never counted as returned.
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
    for path in found.iter().take(paths as usize) {
        let mut about: DISPLAYCONFIG_SOURCE_DEVICE_NAME = unsafe { std::mem::zeroed() };
        about.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        about.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        about.header.adapterId = path.sourceInfo.adapterId;
        about.header.id = path.sourceInfo.id;
        // SAFETY: the block is ours, opens on the header the call reads,
        // and carries its own size as that header requires.
        if unsafe { DisplayConfigGetDeviceInfo((&raw mut about).cast()) } != ERROR_SUCCESS as i32 {
            continue;
        }
        let end = about
            .viewGdiDeviceName
            .iter()
            .position(|letter| *letter == 0)
            .unwrap_or(0);
        if String::from_utf16_lossy(&about.viewGdiDeviceName[..end]) == screen {
            return Some((
                Luid {
                    low: path.sourceInfo.adapterId.LowPart,
                    high: path.sourceInfo.adapterId.HighPart,
                },
                path.sourceInfo.id,
            ));
        }
    }
    None
}

fn read(adapter: Luid, id: u32) -> Option<Magnification> {
    use windows_sys::Win32::Devices::Display::DisplayConfigGetDeviceInfo;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;

    let mut asking = Magnification {
        header: Header {
            kind: READ_THE_MAGNIFICATION,
            size: size_of::<Magnification>() as u32,
            adapter,
            id,
        },
        ..Magnification::default()
    };
    // SAFETY: the block is ours, opens on the header the call reads, and
    // carries its own size as that header requires.
    let answer = unsafe { DisplayConfigGetDeviceInfo((&raw mut asking).cast()) };
    (answer == ERROR_SUCCESS as i32).then_some(asking)
}

fn set(adapter: Luid, id: u32, step: i32) -> bool {
    use windows_sys::Win32::Devices::Display::DisplayConfigSetDeviceInfo;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;

    let mut asking = Wanted {
        header: Header {
            kind: SET_THE_MAGNIFICATION,
            size: size_of::<Wanted>() as u32,
            adapter,
            id,
        },
        step,
    };
    // SAFETY: as above, on the call that writes rather than reads.
    unsafe { DisplayConfigSetDeviceInfo((&raw mut asking).cast()) == ERROR_SUCCESS as i32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_magnifications_windows_offers_are_written_in_its_own_order() {
        // Le message privé parle en pas le long de cette liste : dans un
        // autre ordre, demander 150 % en donnerait 175.
        assert!(OFFERED.windows(2).all(|two| two[0] < two[1]));
        assert_eq!(OFFERED.first(), Some(&100));
    }

    #[test]
    fn a_step_names_the_percentage_it_stands_for() {
        // La recommandée est au rang `-lowest` : un pas de zéro vaut
        // donc la recommandée, et les autres se comptent depuis elle.
        assert_eq!(at(0, 0), 100);
        assert_eq!(at(1, 0), 125);
        assert_eq!(at(1, -1), 100);
        assert_eq!(at(1, 2), 175);
        // Hors de la liste, rien plutôt qu'un chiffre inventé.
        assert_eq!(at(0, -1), 0);
        assert_eq!(at(11, 1), 0);
    }
}
