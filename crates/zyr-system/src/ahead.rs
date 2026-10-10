//! This program put ahead of the computer's others for as long as a
//! session is open.
//!
//! Every packet of a session crosses the service on both computers: off
//! the engine's link and into the tunnel on one, out of the tunnel and
//! onto the player's link on the other. At the priority Windows gives a
//! service, it waited behind whatever kept the processors busy, a game on
//! the computer being watched first of all, which is exactly when the
//! picture matters. The host engine has long been put ahead the same way.
//!
//! Windows 11 may also take a program with no window for one whose speed
//! does not matter: it runs it on its slowest cores at a lower clock, and
//! lets its timers wake a whole tick late. Both are turned down for the
//! session.
//!
//! The first session asks, the last one gives everything back.

use zyr_proto::log::Log;

/// What this module's lines are filed under.
const TAG: &str = "priority";

/// This program kept ahead for as long as this is held.
///
/// Handed to whatever owns a session, so that it ends exactly when the
/// session does, whichever way it ends.
pub struct Ahead;

impl Drop for Ahead {
    fn drop(&mut self) {
        mechanism::behind();
    }
}

/// A session is opening here: until it ends, this program goes before
/// the computer's others.
pub fn put_ahead(log: &Log) -> Ahead {
    mechanism::ahead(&log.about(TAG));
    Ahead
}

#[cfg(windows)]
mod mechanism {
    use std::ffi::c_void;
    use std::io;
    use std::sync::Mutex;

    use windows_sys::Win32::Media::{TIMERR_NOERROR, timeBeginPeriod, timeEndPeriod};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetPriorityClass, HIGH_PRIORITY_CLASS,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE,
        ProcessPowerThrottling, SetPriorityClass, SetProcessInformation,
    };
    use zyr_proto::log::Log;

    /// Timer resolution asked for, in milliseconds.
    const TIMER_MS: u32 = 1;

    /// Sessions open, and what was obtained for them.
    static HELD: Mutex<Held> = Mutex::new(Held {
        sessions: 0,
        obtained: None,
    });

    struct Held {
        sessions: usize,
        obtained: Option<Obtained>,
    }

    /// What the first session obtained, to give back after the last.
    struct Obtained {
        /// The class before, when it was changed.
        class_before: Option<u32>,
        full_speed: bool,
        timer: bool,
    }

    pub fn ahead(log: &Log) {
        let mut held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        held.sessions += 1;
        if held.obtained.is_none() {
            held.obtained = Some(obtain(log));
        }
    }

    pub fn behind() {
        let mut held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        held.sessions = held.sessions.saturating_sub(1);
        if held.sessions == 0
            && let Some(obtained) = held.obtained.take()
        {
            give_back(&obtained);
        }
    }

    fn obtain(log: &Log) -> Obtained {
        let mut said = Vec::new();
        // SAFETY: the pseudo handle of this process, never closed; nought
        // means the class could not be read.
        let before = unsafe { GetPriorityClass(GetCurrentProcess()) };
        // SAFETY: as above, and a class that needs no privilege.
        let class_before =
            if unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) } != 0 {
                said.push("before the computer's other programs");
                (before != 0).then_some(before)
            } else {
                log.write(&format!(
                    "putting this program before the computer's others was refused: {}",
                    io::Error::last_os_error()
                ));
                None
            };
        let full_speed = throttling(
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
                | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        );
        if full_speed {
            said.push("never slowed down to save power");
        } else {
            log.write(&format!(
                "keeping this program at full speed was refused: {}",
                io::Error::last_os_error()
            ));
        }
        // SAFETY: a plain value, undone once with the same value.
        let timer = unsafe { timeBeginPeriod(TIMER_MS) } == TIMERR_NOERROR;
        if timer {
            said.push("timers to the millisecond");
        } else {
            log.write("timers to the millisecond were refused");
        }
        log.write(&format!(
            "set up for the sessions: {}",
            if said.is_empty() {
                "nothing".to_string()
            } else {
                said.join(", ")
            }
        ));
        Obtained {
            class_before,
            full_speed,
            timer,
        }
    }

    fn give_back(obtained: &Obtained) {
        if let Some(before) = obtained.class_before {
            // SAFETY: the pseudo handle of this process, and the class it
            // had; a refusal leaves the class until the process ends.
            unsafe { SetPriorityClass(GetCurrentProcess(), before) };
        }
        if obtained.full_speed {
            // Nothing controlled: Windows decides again.
            throttling(0);
        }
        if obtained.timer {
            // SAFETY: the value given to timeBeginPeriod.
            unsafe { timeEndPeriod(TIMER_MS) };
        }
    }

    /// Takes the switches named in `mask` out of Windows' hands and sets
    /// them off, the others being left to Windows; whether it was done.
    fn throttling(mask: u32) -> bool {
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: mask,
            StateMask: 0,
        };
        // SAFETY: the pseudo handle of this process, and a structure of
        // the class named, which outlives the call and says its size.
        let done = unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                (&raw const state).cast::<c_void>(),
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
        };
        done != 0
    }
}

/// Outside Windows there is no service to put ahead.
#[cfg(not(windows))]
mod mechanism {
    use zyr_proto::log::Log;

    pub fn ahead(_log: &Log) {}

    pub fn behind() {}
}

#[cfg(all(test, windows))]
mod tests {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetPriorityClass, HIGH_PRIORITY_CLASS,
    };

    use super::*;

    fn class() -> u32 {
        // SAFETY: the pseudo handle of this process.
        unsafe { GetPriorityClass(GetCurrentProcess()) }
    }

    #[test]
    fn the_program_goes_ahead_while_a_session_is_open_and_back_after_the_last() {
        let path = std::env::temp_dir().join(format!("zyrdesk-ahead-{}.log", std::process::id()));
        let log = Log::open(&path).unwrap();
        let before = class();
        let first = put_ahead(&log);
        let second = put_ahead(&log);
        assert_eq!(class(), HIGH_PRIORITY_CLASS);
        drop(first);
        assert_eq!(class(), HIGH_PRIORITY_CLASS, "one session is still open");
        drop(second);
        assert_eq!(class(), before);
        let _ = std::fs::remove_file(path);
    }
}
