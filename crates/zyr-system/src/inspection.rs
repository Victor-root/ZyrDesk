//! What Windows says this computer is, read once.
//!
//! Nearly all of it sits in the registry, where Windows and the drivers
//! write it down as they install: the build of Windows, the processor,
//! the graphics cards and the driver each runs on. The memory and the
//! power come from their own calls. No permission beyond reading is
//! needed, and what cannot be read is left out. How it is worded is in
//! `machine.rs`.

use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, PowerGetActiveScheme, PowerReadFriendlyName, SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_ROUTINE_FLAGS, RRF_RT_ANY, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::core::GUID;
use zyr_win32::{read_wide, wide};

use crate::machine::{
    Card, Machine, Power, Source, Windows, letters, memory_declared, overlay_name, tidy,
};

const VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const PROCESSOR_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
const POWER_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes";

/// The class Windows files the graphics cards under.
const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// What this computer is, in one line for the journal.
pub fn describe_this_computer() -> Option<String> {
    Some(this_computer().to_string()).filter(|told| !told.is_empty())
}

/// What Windows says this computer is.
fn this_computer() -> Machine {
    Machine {
        windows: windows(),
        processor: text(PROCESSOR_KEY, "ProcessorNameString").map(|name| tidy(&name)),
        processors: std::thread::available_parallelism().map_or(0, usize::from),
        memory: memory(),
        power: power(),
        cards: cards(),
    }
}

/// A value of the local machine's registry, as the bytes it holds.
fn value(key: &str, name: &str, kinds: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    let (key, name) = (wide(key), wide(name));
    let mut size = 0u32;
    // SAFETY: both names outlive the call, and no room is given, which
    // asks how much there is.
    let asked = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            kinds,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut size,
        )
    };
    if asked != ERROR_SUCCESS {
        return None;
    }
    let mut bytes = vec![0u8; size as usize];
    // SAFETY: as above, into room for the size just given.
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            kinds,
            ptr::null_mut(),
            bytes.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if read != ERROR_SUCCESS {
        return None;
    }
    bytes.truncate(size as usize);
    Some(bytes)
}

/// A text Windows wrote as bytes, when it says something.
fn words(bytes: &[u8]) -> Option<String> {
    Some(read_wide(&letters(bytes))).filter(|text| !text.trim().is_empty())
}

/// A text value, when there is one and it says something.
fn text(key: &str, name: &str) -> Option<String> {
    words(&value(key, name, RRF_RT_REG_SZ)?)
}

/// A number value.
fn number(key: &str, name: &str) -> Option<u32> {
    let bytes = value(key, name, RRF_RT_REG_DWORD)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn windows() -> Option<Windows> {
    let build = text(VERSION_KEY, "CurrentBuildNumber")?
        .trim()
        .parse()
        .ok()?;
    Some(Windows {
        build,
        revision: number(VERSION_KEY, "UBR"),
        version: text(VERSION_KEY, "DisplayVersion").or_else(|| text(VERSION_KEY, "ReleaseId")),
        edition: text(VERSION_KEY, "EditionID"),
    })
}

fn memory() -> Option<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: a structure of ours, with its size said.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys)
}

fn power() -> Power {
    let source = source();
    Power {
        plan: plan(),
        mode: mode(source),
        source,
    }
}

fn source() -> Option<Source> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: a structure of ours.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }
    match status.ACLineStatus {
        0 => Some(Source::Battery),
        1 => Some(Source::Mains),
        _ => None,
    }
}

/// The name of the power plan in force, in the person's own language.
fn plan() -> Option<String> {
    let mut scheme: *mut GUID = ptr::null_mut();
    // SAFETY: a place for the identifier Windows makes.
    if unsafe { PowerGetActiveScheme(ptr::null_mut(), &mut scheme) } != ERROR_SUCCESS
        || scheme.is_null()
    {
        return None;
    }
    let mut size = 0u32;
    // SAFETY: the plan just obtained, and no room, which asks how much.
    unsafe {
        PowerReadFriendlyName(
            ptr::null_mut(),
            scheme,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            &mut size,
        )
    };
    let mut bytes = vec![0u8; size as usize];
    // SAFETY: as above, into room for the size just given.
    let read = unsafe {
        PowerReadFriendlyName(
            ptr::null_mut(),
            scheme,
            ptr::null(),
            ptr::null(),
            bytes.as_mut_ptr(),
            &mut size,
        )
    };
    // SAFETY: the identifier Windows made, which is freed once.
    unsafe { LocalFree(scheme.cast()) };
    if read != ERROR_SUCCESS {
        return None;
    }
    words(&bytes)
}

/// The mode chosen over the plan, for the source the power comes from.
fn mode(source: Option<Source>) -> Option<String> {
    let name = match source {
        Some(Source::Battery) => "ActiveOverlayDcPowerScheme",
        _ => "ActiveOverlayAcPowerScheme",
    };
    overlay_name(&text(POWER_KEY, name)?)
}

/// Every graphics card Windows has a driver for.
fn cards() -> Vec<Card> {
    let class = wide(DISPLAY_CLASS);
    let mut open: HKEY = ptr::null_mut();
    // SAFETY: the name outlives the call, and the slot for the key is
    // ours.
    let opened =
        unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, class.as_ptr(), 0, KEY_READ, &mut open) };
    if opened != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut cards: Vec<Card> = Vec::new();
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;
        // SAFETY: the key opened above, and a name buffer of ours with
        // its length in letters given alongside it.
        let listed = unsafe {
            RegEnumKeyExW(
                open,
                index,
                name.as_mut_ptr(),
                &mut length,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if listed != ERROR_SUCCESS {
            break;
        }
        // The cards are numbered 0000, 0001...; the other keys of the
        // class are not cards.
        let numbered = read_wide(&name[..length as usize]);
        if numbered.is_empty() || !numbered.chars().all(|letter| letter.is_ascii_digit()) {
            continue;
        }
        let card = format!(r"{DISPLAY_CLASS}\{numbered}");
        let Some(described) = text(&card, "DriverDesc") else {
            continue;
        };
        let known = Card {
            name: described,
            driver: text(&card, "DriverVersion"),
            memory: value(&card, "HardwareInformation.qwMemorySize", RRF_RT_ANY)
                .and_then(|bytes| memory_declared(&bytes)),
        };
        if !cards.contains(&known) {
            cards.push(known);
        }
    }
    // SAFETY: the key opened above, closed once.
    unsafe { RegCloseKey(open) };
    // A computer with its own driver keeps the generic one's entry from
    // the day it first started, which says nothing about the session.
    if cards.len() > 1 {
        cards.retain(|card| !card.name.starts_with("Microsoft Basic Display"));
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_says_at_least_what_windows_and_how_much_of_it() {
        let told = this_computer().to_string();
        assert!(told.starts_with("Windows 1"), "{told}");
        assert!(told.contains(" logical processors"), "{told}");
        assert!(told.contains(" of memory"), "{told}");
    }
}
