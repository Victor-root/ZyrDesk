//! Asking Windows for the rights the product needs exactly once.
//!
//! Registering a service is the only thing ZyrDesk cannot do as the
//! person running it. Windows answers that with a prompt of its own, and
//! the only way to raise one from a program is to hand the shell a verb.
//!
//! What runs elevated is our own program, beside this one, with a word
//! after it that is written here. Nothing handed to this program from
//! outside ever gets that far: an elevation is not a place for a value
//! somebody else chose.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
use zyr_proto::fact::Fact;

/// What Windows says when the person turns the prompt down.
const REFUSED: i32 = 1223;

/// The point from which an exit code is no longer a program's own.
///
/// Windows puts there what it says about a program that stopped by
/// itself: a crash, an overflow, a damaged image. Ours only ever exits
/// with zero or one, so nothing of that form can come from it, and
/// anything that comes from there means it was unable to say anything.
const CRASHED: u32 = 0xC000_0000;

/// COM, initialised for as long as this lasts.
///
/// The shell expects it. The thread this runs on is ours alone, so it is
/// handed back the way it was found.
struct Com {
    ours: bool,
}

impl Com {
    fn entered() -> Self {
        let outcome = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        // Only an entry that happened is ours to undo. A thread already
        // in another apartment stays in it, and a refusal of any kind
        // entered nothing: paying it back anyway would take one entry
        // too many off whoever really made one.
        Self { ours: outcome >= 0 }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.ours {
            unsafe { CoUninitialize() };
        }
    }
}

/// Runs one of our own programs with administrator rights, and waits for
/// it to finish.
///
/// Waiting is the point: without it the window would announce a service
/// that is not there yet, or one that never started at all.
pub fn run(program: &Path, arguments: &str) -> Result<(), Fact> {
    let _com = Com::entered();

    let verb = crate::win32::wide("runas");
    let file: Vec<u16> = program.as_os_str().encode_wide().chain(Some(0)).collect();
    let words = crate::win32::wide(arguments);

    let mut about: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    about.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    about.fMask = SEE_MASK_NOCLOSEPROCESS;
    about.lpVerb = verb.as_ptr();
    about.lpFile = file.as_ptr();
    about.lpParameters = words.as_ptr();
    about.nShow = SW_HIDE;

    if unsafe { ShellExecuteExW(&mut about) } == 0 {
        let refused = std::io::Error::last_os_error();
        return Err(match refused.raw_os_error() {
            Some(REFUSED) => Fact::new("window.elevation_refused"),
            _ => Fact::new("window.elevation_failed").with("detail", refused),
        });
    }

    let running = about.hProcess;
    if running.is_null() {
        // Started with nothing to watch it by: saying it worked would be
        // a guess, and the window would go on to show a service that may
        // not be there.
        return Err(Fact::new("window.elevation_silent"));
    }
    let outcome = waited(running);
    unsafe { CloseHandle(running) };
    outcome
}

/// Waits for the elevated program, and reads what it made of it.
fn waited(running: HANDLE) -> Result<(), Fact> {
    if unsafe { WaitForSingleObject(running, INFINITE) } != WAIT_OBJECT_0 {
        return Err(Fact::new("window.setup_wait_failed"));
    }

    let mut code: u32 = 0;
    if unsafe { GetExitCodeProcess(running, &mut code) } == 0 {
        return Err(Fact::new("window.setup_no_code"));
    }
    if code == 0 {
        return Ok(());
    }
    // A code of this form is not a report from the program but the way
    // Windows says it stopped abruptly. The service has then written
    // nothing, and sending the person off to read a journal that will say
    // nothing is the worst referral there is: an hour goes on searching
    // the place where there is nothing to find.
    if code >= CRASHED {
        return Err(Fact::new("window.setup_crashed").with("code", format!("{code:08X}")));
    }
    Err(Fact::new("window.setup_failed").with("code", code))
}
