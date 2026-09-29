//! Running a program with administrator rights.
//!
//! Registering a service is the one thing a program cannot do as the
//! person running it. Windows answers that with a prompt of its own, and
//! the only way to raise one from a program is to hand the shell a verb.

use std::io;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::Refusal;

/// What Windows says when the person turns the prompt down.
const DECLINED: i32 = 1223;

/// COM, initialised for as long as this lasts.
///
/// The shell expects it. The thread this runs on is the caller's, so it
/// is handed back the way it was found.
struct Com {
    ours: bool,
}

impl Com {
    fn entered() -> Self {
        // SAFETY: no reserved pointer, and a model any thread may enter.
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
            // SAFETY: the one entry made above, paid back on its thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// Runs that program with administrator rights, waits for it to finish,
/// and answers its exit code.
///
/// Waiting is the point: whoever asked would otherwise go on as if it
/// had done its work, before it had, or when it never did.
pub fn run_as_administrator(program: &Path, arguments: &str) -> Result<u32, Refusal> {
    let _com = Com::entered();

    let verb = zyr_win32::wide("runas");
    let file = zyr_win32::wide(program);
    let words = zyr_win32::wide(arguments);

    // SAFETY: a structure of plain fields, for which nothing is a valid
    // value, filled in below.
    let mut about: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    about.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    about.fMask = SEE_MASK_NOCLOSEPROCESS;
    about.lpVerb = verb.as_ptr();
    about.lpFile = file.as_ptr();
    about.lpParameters = words.as_ptr();
    about.nShow = SW_HIDE;

    // SAFETY: every text it points to outlives the call.
    if unsafe { ShellExecuteExW(&mut about) } == 0 {
        let refused = io::Error::last_os_error();
        return Err(match refused.raw_os_error() {
            Some(DECLINED) => Refusal::Declined,
            _ => Refusal::NotStarted(refused),
        });
    }

    let running = about.hProcess;
    if running.is_null() {
        // Started with nothing to watch it by: saying it worked would be
        // a guess.
        return Err(Refusal::Unwatched);
    }
    let outcome = waited(running);
    // SAFETY: the handle the shell gave, closed once.
    unsafe { CloseHandle(running) };
    outcome
}

/// Waits for the program, and reads the code it ended with.
fn waited(running: HANDLE) -> Result<u32, Refusal> {
    // SAFETY: a handle of ours, open until the caller closes it.
    if unsafe { WaitForSingleObject(running, INFINITE) } != WAIT_OBJECT_0 {
        return Err(Refusal::NotWaited);
    }
    let mut code: u32 = 0;
    // SAFETY: as above, and a number of ours to write the code into.
    if unsafe { GetExitCodeProcess(running, &mut code) } == 0 {
        return Err(Refusal::NoExitCode);
    }
    Ok(code)
}
