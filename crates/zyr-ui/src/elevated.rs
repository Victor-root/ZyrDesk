//! Asking Windows for the rights the product needs exactly once.
//!
//! Registering a service is the only thing ZyrDesk cannot do as the
//! person running it. Windows answers that with a prompt of its own, and
//! the only way to raise one from a program is to hand the shell a verb.
//!
//! What runs elevated is our own program, beside this one, with a word
//! after it that is written here. Nothing arriving from a page ever gets
//! that far: an elevation is not a place for a value somebody else
//! chose.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// What Windows says when the person turns the prompt down.
const REFUSED: i32 = 1223;

/// What COM says when this thread is already in another apartment.
///
/// Written out rather than imported: it is one number, and it decides
/// whether this thread is ours to put back the way it was.
const RPC_E_CHANGED_MODE: i32 = -2_147_417_850;

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
        // A thread already in another apartment stays in it: the shell
        // works either way, and taking it over would break whoever set
        // it up.
        Self {
            ours: outcome != RPC_E_CHANGED_MODE,
        }
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
pub fn run(program: &Path, arguments: &str) -> Result<(), String> {
    let _com = Com::entered();

    let verb = wide("runas");
    let file: Vec<u16> = program.as_os_str().encode_wide().chain(Some(0)).collect();
    let words = wide(arguments);

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
            Some(REFUSED) => "les droits administrateur ont été refusés.".to_string(),
            _ => format!("Windows n'a pas lancé le service : {refused}"),
        });
    }

    let running = about.hProcess;
    if running.is_null() {
        // Started with nothing to watch it by: saying it worked would be
        // a guess, and the window would go on to show a service that may
        // not be there.
        return Err("Windows n'a rien dit de ce qu'il a lancé".to_string());
    }
    let outcome = waited(running);
    unsafe { CloseHandle(running) };
    outcome
}

/// Waits for the elevated program, and reads what it made of it.
fn waited(running: HANDLE) -> Result<(), String> {
    if unsafe { WaitForSingleObject(running, INFINITE) } != WAIT_OBJECT_0 {
        return Err("l'attente de la mise en service a échoué".to_string());
    }

    let mut code: u32 = 0;
    if unsafe { GetExitCodeProcess(running, &mut code) } == 0 {
        return Err("la mise en service n'a pas dit comment elle s'est terminée".to_string());
    }
    if code == 0 {
        return Ok(());
    }
    Err(format!(
        "la mise en service a échoué (code {code}).\n  \
         Le journal en dit plus."
    ))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
