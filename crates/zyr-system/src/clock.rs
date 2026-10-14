//! The time where the computer is, for a person to read.
//!
//! The product keeps its times in universal time; a person reads the
//! hour on their wall. Windows knows the rules of each place, summer
//! time included, for any day, so the hour of a session of last winter
//! is read with last winter's rule.

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/// A moment as the clock of this computer shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

/// Seconds between the start of 1601, where Windows counts from, and
/// the start of 1970.
const FROM_1601_TO_1970: u64 = 11_644_473_600;

/// That moment, given in seconds since 1970 in universal time, as this
/// computer's clock shows it.
pub fn local_time(unix: u64) -> Option<LocalTime> {
    let ticks = (unix + FROM_1601_TO_1970) * 10_000_000;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: two structures of ours, filled by the system.
    let mut universal: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: the structures above, valid for the length of the calls;
    // no time zone given is the computer's own.
    let read = unsafe {
        FileTimeToSystemTime(&file, &mut universal) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal, &mut local) != 0
    };
    read.then_some(LocalTime {
        year: local.wYear,
        month: local.wMonth as u8,
        day: local.wDay as u8,
        hour: local.wHour as u8,
        minute: local.wMinute as u8,
    })
}
