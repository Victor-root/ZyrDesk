//! Windows: the graphics card decodes and draws into the window, the
//! sound card plays.
//!
//! Every call to Windows that fails is written down with what was being
//! done and its code in hexadecimal: one line of the journal is enough
//! to know what broke on the machine it ran on.

pub mod audio;
mod device;
mod present;

pub use present::Screen;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
};
use windows::core::{Error, HRESULT, PCWSTR};
use zyr_proto::log::Log;

/// What was being done, and how Windows refused it.
fn failure(what: &str, error: &Error) -> String {
    format!("{what}: {}", code_and_words(error.code(), &error.message()))
}

/// A code in hexadecimal, as Windows' documentation lists them, then
/// Windows' own words for it.
fn code_and_words(code: HRESULT, words: &str) -> String {
    format!("{:#010x} {}", code.0 as u32, words.trim())
}

/// The calling thread, scheduled by the multimedia class scheduler as
/// that kind of work for as long as this lives, which is on that
/// thread.
struct Multimedia(Option<HANDLE>);

impl Multimedia {
    /// Joins `task`, one of the tasks Windows lists in the registry
    /// ("Playback", "Pro Audio"...). Refused, the thread goes on at its
    /// ordinary priority, and the journal says why.
    fn join(task: PCWSTR, log: &Log) -> Self {
        let mut index = 0;
        // SAFETY: a task name the macro made, and an index of ours.
        match unsafe { AvSetMmThreadCharacteristicsW(task, &mut index) } {
            Ok(handle) => Self(Some(handle)),
            Err(e) => {
                // SAFETY: the same name, a C string.
                let name = unsafe { task.to_string() }.unwrap_or_default();
                log.write(&failure(
                    &format!("AvSetMmThreadCharacteristicsW({name})"),
                    &e,
                ));
                Self(None)
            }
        }
    }
}

impl Drop for Multimedia {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            // SAFETY: the handle joined on this same thread, left once.
            let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
        }
    }
}
