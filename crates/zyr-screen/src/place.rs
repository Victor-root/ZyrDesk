//! Putting a driver-declared screen on this machine, and taking it off.
//!
//! Two separate things, and both are needed. A driver package has to be
//! taken into Windows' own store of drivers, which is what makes it
//! installable at all; and a device has to be declared for it to be
//! installed onto, which for a screen that no cable leads to nothing
//! else will ever do. Windows itself declares a device when something is
//! plugged in. Nothing is ever plugged in here, so the device is
//! declared by hand, which is what a machine's own driver tooling does
//! and what is done below.
//!
//! Nothing here knows which driver it is working on: it is handed a
//! hardware identifier, a class and a folder, and does the same few
//! calls whatever they are.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CONFIGFLAG_DISABLED, DICD_GENERATE_ID, DICS_DISABLE, DICS_ENABLE, DICS_FLAG_CONFIGSPECIFIC,
    DICS_FLAG_GLOBAL, DICS_PROPCHANGE, DIF_PROPERTYCHANGE, DIF_REGISTERDEVICE, DIF_REMOVE,
    DIGCF_PRESENT, HDEVINFO, INSTALLFLAG_FORCE, SP_CLASSINSTALL_HEADER, SP_DEVINFO_DATA,
    SP_PROPCHANGE_PARAMS, SPDRP_CONFIGFLAGS, SPDRP_HARDWAREID, SPOST_NONE, SUOI_FORCEDELETE,
    SetupCopyOEMInfW, SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList,
    SetupDiCreateDeviceInfoW, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW, SetupDiSetClassInstallParamsW,
    SetupDiSetDeviceRegistryPropertyW, SetupUninstallOEMInfW, UpdateDriverForPlugAndPlayDevicesW,
};
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GetLastError,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW,
};
use windows_sys::core::GUID;

use crate::driver::{Driver, Guid};
use crate::{Done, Trouble};

/// What every call in this file answers with when it has no answer.
const NOTHING: HDEVINFO = -1;

/// Where the name Windows filed our package under is remembered.
///
/// Taking a package back out of the store needs the name Windows gave it
/// when it went in, which is a number nobody chose and which cannot be
/// worked out from anything: `oem41.inf` on one machine, `oem7.inf` on
/// the next. Written down at the one moment it is known.
const FILED_AS: &str = "driver-filed-as.txt";

/// Puts the package in Windows' store and declares the device.
///
/// Both steps are skipped when they have already been done, so this can
/// be called at every start without asking whether it is needed.
pub fn put_in_place(
    driver: &dyn Driver,
    package: &Path,
    home: &Path,
    done: &mut Done,
) -> Result<(), Trouble> {
    let inf = package.join(driver.inf_file());
    let filed = file_the_package(&inf)?;
    done.step(format!("virtual screen driver filed by Windows as {filed}"));
    remember(home, &filed, done);

    if find_device(driver)?.is_some() {
        done.step(format!(
            "virtual screen device {} already declared, left alone",
            driver.hardware_id()
        ));
    } else {
        declare_device(driver)?;
        done.changed = true;
        done.step(format!(
            "virtual screen device {} declared",
            driver.hardware_id()
        ));
    }

    // Said after declaring and not instead of it: declaring a device
    // gives Windows something to install onto, and this is what makes it
    // install our package rather than leave the device driverless.
    let mut later = 0;
    let told = wide(inf.as_os_str());
    let which = wide(OsStr::new(driver.hardware_id()));
    // SAFETY: both strings outlive the call and end in a zero, and the
    // slot for the answer is ours.
    let ok = unsafe {
        UpdateDriverForPlugAndPlayDevicesW(
            std::ptr::null_mut(),
            which.as_ptr(),
            told.as_ptr(),
            INSTALLFLAG_FORCE,
            &mut later,
        )
    };
    if ok == 0 {
        return Err(refused("installing the virtual screen driver"));
    }
    done.step(format!(
        "virtual screen driver installed onto the device{}",
        if later == 0 {
            ""
        } else {
            ", Windows wants a restart to finish"
        }
    ));
    Ok(())
}

/// Takes the device away and then the package, in that order.
///
/// That order and not the other: a package cannot leave the store while
/// a device is still using it, and a device left behind with no driver
/// shows up in Windows' own list of hardware as one that failed.
pub fn take_away(driver: &dyn Driver, home: &Path, done: &mut Done) -> Result<(), Trouble> {
    match find_device(driver)? {
        Some(found) => {
            // SAFETY: the set and the device both live in `found`.
            let ok = unsafe { SetupDiCallClassInstaller(DIF_REMOVE, found.set.0, &found.device) };
            if ok == 0 {
                return Err(refused("removing the virtual screen device"));
            }
            done.changed = true;
            done.step("virtual screen device removed");
        }
        None => done.step("no virtual screen device on this computer, nothing to remove"),
    }

    let noted = home.join(FILED_AS);
    let Ok(filed) = std::fs::read_to_string(&noted) else {
        done.step(format!(
            "no note of how Windows filed the virtual screen driver ({}), its store is left as it is",
            noted.display()
        ));
        return Ok(());
    };
    let filed = filed.trim();
    let name = wide(OsStr::new(filed));
    // SAFETY: the name outlives the call and ends in a zero.
    let ok =
        unsafe { SetupUninstallOEMInfW(name.as_ptr(), SUOI_FORCEDELETE, std::ptr::null_mut()) };
    if ok == 0 {
        return Err(refused(&format!(
            "taking the virtual screen driver {filed} out of Windows' store"
        )));
    }
    let _ = std::fs::remove_file(&noted);
    done.step(format!(
        "virtual screen driver {filed} taken out of the store"
    ));
    Ok(())
}

/// Stops the screen and starts it again, which is the only moment the
/// driver reads the sizes written down for it.
pub fn restart(driver: &dyn Driver, done: &mut Done) -> Result<(), Trouble> {
    change(
        driver,
        DICS_PROPCHANGE,
        DICS_FLAG_CONFIGSPECIFIC,
        "restart",
        "virtual screen restarted so it reads its sizes again",
        done,
    )
}

/// Wakes the screen, which is what makes Windows show it at all.
///
/// The state a screen sits in between sessions is off at the device, not
/// merely unplugged from the desktop: Windows shows a screen that is
/// present, and one more screen on somebody's desk all day long is not
/// something this product is entitled to.
///
/// Globally and not for this hardware profile, because that is what being
/// switched off at the device means and what the machine's own device
/// tooling writes when a person clicks the same thing by hand.
pub fn wake(driver: &dyn Driver, done: &mut Done) -> Result<(), Trouble> {
    change(
        driver,
        DICS_ENABLE,
        DICS_FLAG_GLOBAL,
        "wake",
        "virtual screen woken",
        done,
    )
}

/// Puts it back to sleep, so it stops being one of this machine's screens.
pub fn sleep(driver: &dyn Driver, done: &mut Done) -> Result<(), Trouble> {
    change(
        driver,
        DICS_DISABLE,
        DICS_FLAG_GLOBAL,
        "put to sleep",
        "virtual screen asleep, this machine has its own screens back",
        done,
    )
}

/// Asks the device to change state, which is three calls whatever the
/// change is.
///
/// One function because the three that use it differ by two numbers and a
/// sentence, and a second copy of these calls would be a second place to
/// get the block's shape wrong.
fn change(
    driver: &dyn Driver,
    state: u32,
    scope: u32,
    asking: &str,
    said: &str,
    done: &mut Done,
) -> Result<(), Trouble> {
    let Some(found) = find_device(driver)? else {
        done.step(format!("no virtual screen device to {asking}"));
        return Ok(());
    };
    let asked = SP_PROPCHANGE_PARAMS {
        ClassInstallHeader: SP_CLASSINSTALL_HEADER {
            cbSize: size_of::<SP_CLASSINSTALL_HEADER>() as u32,
            InstallFunction: DIF_PROPERTYCHANGE,
        },
        StateChange: state,
        Scope: scope,
        HwProfile: 0,
    };
    // SAFETY: the set and the device both live in `found`, and the block
    // handed over is the one the call expects, opening on the header it
    // reads and carrying its own size alongside.
    let ok = unsafe {
        SetupDiSetClassInstallParamsW(
            found.set.0,
            &found.device,
            std::ptr::from_ref(&asked).cast::<SP_CLASSINSTALL_HEADER>(),
            size_of::<SP_PROPCHANGE_PARAMS>() as u32,
        ) != 0
            && SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, found.set.0, &found.device) != 0
    };
    if !ok {
        return Err(refused(&format!("asking the virtual screen to {asking}")));
    }
    done.step(said);
    Ok(())
}

/// Whether the virtual screen device is declared on this machine.
pub fn present(driver: &dyn Driver) -> Result<bool, Trouble> {
    Ok(find_device(driver)?.is_some())
}

/// How many screens the desktop is made of right now.
///
/// What waking a screen is waited on with. Windows hands back from
/// starting a device long before that device is a screen anybody can
/// capture: the desktop has to be rebuilt around it first, and nothing
/// says when that is done. Counting is enough to know, needs no name, and
/// is the same question whatever driver grew the screen.
pub fn screens_on_the_desktop() -> usize {
    use windows_sys::Win32::Graphics::Gdi::{
        DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, EnumDisplayDevicesW,
    };

    let mut seen = 0;
    for index in 0.. {
        let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        // SAFETY: the slot is ours with its size written in it as the
        // call requires, and no name is given so the list is the
        // machine's own adapters.
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0) } == 0 {
            break;
        }
        if device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != 0 {
            seen += 1;
        }
    }
    seen
}

/// Size of the machine's main screen, in real pixels.
///
/// The main one and not the largest: a desktop can have several, and the
/// one at the origin is the one a session left alone lands on.
///
/// Read from the system's own display configuration rather than from a
/// window, because the caller here is a service, which has neither a
/// window nor a desktop to put one on. Nothing about this call needs
/// either.
pub fn the_main_screen() -> Option<(u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        DEVMODEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE,
        DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, EnumDisplayDevicesW, EnumDisplaySettingsW,
    };

    for index in 0.. {
        let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        device.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        // SAFETY: the slot is ours with its size written in it as the
        // call requires, and no name is given so the list is the
        // machine's own adapters.
        if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0) } == 0 {
            break;
        }
        let attached = device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != 0;
        let primary = device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0;
        if !attached || !primary {
            continue;
        }
        let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
        mode.dmSize = size_of::<DEVMODEW>() as u16;
        // SAFETY: a name the system just wrote and ended itself, and a
        // slot of ours with its size written in it.
        let read = unsafe {
            EnumDisplaySettingsW(device.DeviceName.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode)
        };
        if read == 0 {
            break;
        }
        return (mode.dmPelsWidth > 0 && mode.dmPelsHeight > 0)
            .then_some((mode.dmPelsWidth, mode.dmPelsHeight));
    }
    None
}

/// Whether it is awake, `None` when there is no such device at all.
///
/// Read from the same flags the machine's own device tooling writes, so
/// a screen somebody switched off by hand reads as asleep here, which is
/// the truth and what this product should act on.
pub fn awake(driver: &dyn Driver) -> Result<Option<bool>, Trouble> {
    let Some(found) = find_device(driver)? else {
        return Ok(None);
    };
    let mut flags = 0u32;
    let mut kind = 0u32;
    let mut written = 0u32;
    // SAFETY: the set and the device live in `found`, and the slot is
    // ours with its size given alongside it.
    let ok = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            found.set.0,
            &found.device,
            SPDRP_CONFIGFLAGS,
            &mut kind,
            std::ptr::from_mut(&mut flags).cast::<u8>(),
            size_of::<u32>() as u32,
            &mut written,
        )
    };
    // A device that has never been switched either way carries no flags
    // at all, and that is a device Windows is showing: awake.
    if ok == 0 {
        return Ok(Some(true));
    }
    Ok(Some(flags & CONFIGFLAG_DISABLED == 0))
}

/// Takes a driver package into Windows' own store of drivers.
///
/// Returns the name it was filed under. Called twice, it files the same
/// package once: Windows recognises a package it already holds.
fn file_the_package(inf: &Path) -> Result<String, Trouble> {
    let source = wide(inf.as_os_str());
    let mut filed = [0u16; 260];
    let mut needed = 0u32;
    let mut part: *mut u16 = std::ptr::null_mut();
    // SAFETY: the source string outlives the call, and the slot handed
    // over is ours with its length given alongside it.
    let ok = unsafe {
        SetupCopyOEMInfW(
            source.as_ptr(),
            std::ptr::null(),
            SPOST_NONE,
            0,
            filed.as_mut_ptr(),
            filed.len() as u32,
            &mut needed,
            &mut part,
        )
    };
    if ok == 0 {
        return Err(refused(&format!(
            "taking {} into Windows' store of drivers",
            inf.display()
        )));
    }
    let whole = read_wide(&filed);
    Ok(Path::new(&whole)
        .file_name()
        .map_or(whole.clone(), |name| name.to_string_lossy().into_owned()))
}

/// Writes down the name Windows filed the package under.
///
/// Not worth failing an installation over: the screen works either way,
/// and what is lost is the ability to take the package back out of the
/// store later. Said out loud here so it is not discovered at uninstall
/// time instead.
fn remember(home: &Path, filed: &str, done: &mut Done) {
    let noted = home.join(FILED_AS);
    if let Err(e) = std::fs::write(&noted, filed.as_bytes()) {
        done.step(format!(
            "could not write down how Windows filed the driver ({}): {e}",
            noted.display()
        ));
    }
}

/// A list of devices, closed once whatever happens.
struct Set(HDEVINFO);

impl Drop for Set {
    fn drop(&mut self) {
        // SAFETY: a list this file opened, closed exactly once.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// A device, together with the list it was found in: the list owns it,
/// and a device outliving its list names nothing.
struct Found {
    set: Set,
    device: SP_DEVINFO_DATA,
}

fn empty_device() -> SP_DEVINFO_DATA {
    SP_DEVINFO_DATA {
        cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    }
}

/// The device carrying that hardware identifier, if it is declared.
fn find_device(driver: &dyn Driver) -> Result<Option<Found>, Trouble> {
    let (_, class) = driver.class();
    let class = guid(class);
    // SAFETY: the number is ours and outlives the call.
    let set = unsafe {
        SetupDiGetClassDevsW(
            &class,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT,
        )
    };
    if set == NOTHING {
        return Err(refused("listing this computer's screens"));
    }
    let set = Set(set);
    for index in 0.. {
        let mut device = empty_device();
        // SAFETY: the list is alive and the slot is ours with its size
        // written in it as the call requires.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut device) } == 0 {
            // SAFETY: reading why the call above stopped.
            if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                break;
            }
            return Err(refused("walking this computer's screens"));
        }
        if identifiers(set.0, &device)
            .iter()
            .any(|had| had.eq_ignore_ascii_case(driver.hardware_id()))
        {
            return Ok(Some(Found { set, device }));
        }
    }
    Ok(None)
}

/// Every hardware identifier one device answers to.
///
/// A device that answers to none, or that refuses to say, is simply not
/// ours: there is nothing here worth stopping an installation for.
fn identifiers(set: HDEVINFO, device: &SP_DEVINFO_DATA) -> Vec<String> {
    let mut room = 512u32;
    loop {
        let mut buffer = vec![0u8; room as usize];
        let mut needed = 0u32;
        // SAFETY: the list and the device are alive, and the slot is
        // ours with its length given alongside it.
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set,
                device,
                SPDRP_HARDWAREID,
                std::ptr::null_mut(),
                buffer.as_mut_ptr(),
                room,
                &mut needed,
            )
        };
        if ok != 0 {
            buffer.truncate(needed as usize);
            return multi_string(&buffer);
        }
        // SAFETY: reading why the call above refused.
        if unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER && needed > room {
            room = needed;
            continue;
        }
        return Vec::new();
    }
}

/// Declares a device nothing was ever plugged in for.
fn declare_device(driver: &dyn Driver) -> Result<(), Trouble> {
    let (class_name, class) = driver.class();
    let class = guid(class);
    // SAFETY: the number is ours and outlives the call.
    let set = unsafe { SetupDiCreateDeviceInfoList(&class, std::ptr::null_mut()) };
    if set == NOTHING {
        return Err(refused("opening an empty list of devices"));
    }
    let set = Set(set);
    let mut device = empty_device();
    let named = wide(OsStr::new(class_name));
    // SAFETY: the list is alive, the string outlives the call, and the
    // slot is ours with its size written in it.
    let ok = unsafe {
        SetupDiCreateDeviceInfoW(
            set.0,
            named.as_ptr(),
            &class,
            std::ptr::null(),
            std::ptr::null_mut(),
            DICD_GENERATE_ID,
            &mut device,
        )
    };
    if ok == 0 {
        return Err(refused("declaring the virtual screen device"));
    }

    // Two zeros at the end and not one: this is a list of identifiers,
    // and such a list ends where an empty entry starts.
    let mut identifier: Vec<u16> = OsStr::new(driver.hardware_id()).encode_wide().collect();
    identifier.push(0);
    identifier.push(0);
    // SAFETY: the list and the device are alive, and the bytes handed
    // over are the list above with its length given alongside it.
    let ok = unsafe {
        SetupDiSetDeviceRegistryPropertyW(
            set.0,
            &mut device,
            SPDRP_HARDWAREID,
            identifier.as_ptr().cast::<u8>(),
            (identifier.len() * size_of::<u16>()) as u32,
        )
    };
    if ok == 0 {
        return Err(refused("naming the virtual screen device"));
    }

    // SAFETY: the list and the device are alive.
    let ok = unsafe { SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set.0, &device) };
    if ok == 0 {
        return Err(refused("registering the virtual screen device"));
    }
    Ok(())
}

/// Writes a path where the driver will look for it.
pub fn write_registry_text(key: &str, value: &str, text: &Path) -> Result<(), Trouble> {
    let key_w = wide(OsStr::new(key));
    let mut open: HKEY = std::ptr::null_mut();
    // SAFETY: the name outlives the call and the slot for the answer is
    // ours.
    let code = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            key_w.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut open,
            std::ptr::null_mut(),
        )
    };
    if code != 0 {
        return Err(Trouble::System {
            doing: format!("opening {key}"),
            code,
        });
    }
    let value_w = wide(OsStr::new(value));
    let text_w = wide(text.as_os_str());
    // SAFETY: the key is open, both strings outlive the call, and the
    // length given counts the closing zero the reader expects.
    let code = unsafe {
        RegSetValueExW(
            open,
            value_w.as_ptr(),
            0,
            REG_SZ,
            text_w.as_ptr().cast::<u8>(),
            (text_w.len() * size_of::<u16>()) as u32,
        )
    };
    // SAFETY: a key this function opened, closed exactly once.
    unsafe { RegCloseKey(open) };
    if code != 0 {
        return Err(Trouble::System {
            doing: format!("writing {key}\\{value}"),
            code,
        });
    }
    Ok(())
}

/// Takes back what [`write_registry_text`] left, key and all.
pub fn forget_registry_key(key: &str) -> Result<(), Trouble> {
    let key_w = wide(OsStr::new(key));
    // SAFETY: the name outlives the call.
    let code = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, key_w.as_ptr()) };
    // Absent is the state being asked for, so it is not a failure.
    if code != 0 && code != ERROR_FILE_NOT_FOUND {
        return Err(Trouble::System {
            doing: format!("removing {key}"),
            code,
        });
    }
    Ok(())
}

fn guid(from: Guid) -> GUID {
    GUID {
        data1: from.a,
        data2: from.b,
        data3: from.c,
        data4: from.d,
    }
}

/// A string in the shape every call here takes one: sixteen bits a
/// letter, ending in a zero.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

fn read_wide(from: &[u16]) -> String {
    let end = from.iter().position(|&c| c == 0).unwrap_or(from.len());
    String::from_utf16_lossy(&from[..end])
}

/// Reads a list of strings written end to end, the way Windows writes
/// several answers into one slot.
fn multi_string(bytes: &[u8]) -> Vec<String> {
    let letters: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
        .collect();
    letters
        .split(|&c| c == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// The last refusal Windows gave, with what was being attempted.
pub(crate) fn refused(doing: &str) -> Trouble {
    Trouble::System {
        doing: doing.to_string(),
        // SAFETY: reading why the call before this one refused.
        code: unsafe { GetLastError() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_of_answers_written_end_to_end_is_read_back_whole() {
        let mut bytes = Vec::new();
        for word in ["Root\\MttVDD", "MttVDD"] {
            for letter in word.encode_utf16() {
                bytes.extend_from_slice(&letter.to_ne_bytes());
            }
            bytes.extend_from_slice(&0u16.to_ne_bytes());
        }
        bytes.extend_from_slice(&0u16.to_ne_bytes());
        assert_eq!(multi_string(&bytes), ["Root\\MttVDD", "MttVDD"]);
    }

    #[test]
    fn a_string_from_windows_stops_at_its_zero() {
        let mut slot = [0u16; 16];
        for (at, letter) in "oem41.inf".encode_utf16().enumerate() {
            slot[at] = letter;
        }
        assert_eq!(read_wide(&slot), "oem41.inf");
    }

    #[test]
    fn a_string_for_windows_ends_in_a_zero() {
        let said = wide(OsStr::new("Display"));
        assert_eq!(said.last(), Some(&0));
        assert_eq!(said.len(), "Display".len() + 1);
    }
}
