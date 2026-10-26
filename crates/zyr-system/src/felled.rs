//! What a program says of what fells it.
//!
//! A program started by the service has no console anyone reads. What
//! Rust says of a panic goes to an error stream nobody looks at, and a
//! fault Windows catches (an access violation, most of all) ends the
//! program on the spot, leaving only a line in Windows' own event log,
//! far from the journal and without the stack that led there. Both are
//! handed here to whoever asks, as lines to write down.
//!
//! The stack is said twice: as modules and offsets, which mean something
//! even in a build without its symbols, and as functions and lines when
//! the build's symbols lie beside the program.

use std::sync::OnceLock;

/// Whoever is told what fells the program.
type Teller = Box<dyn Fn(&str) + Send + Sync>;

/// Who is told, once for the whole program.
static TOLD: OnceLock<Teller> = OnceLock::new();

/// From now on, every panic and every fault nothing handles is told to
/// `say`, as lines, before the program goes on as it would have.
///
/// Once per program: a second call changes nothing.
pub fn tell_what_fells_it(say: impl Fn(&str) + Send + Sync + 'static) {
    if TOLD.set(Box::new(say)).is_err() {
        return;
    }
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if let Some(say) = TOLD.get() {
            let place = panic
                .location()
                .map_or_else(|| "an unknown place".to_string(), ToString::to_string);
            say(&format!(
                "panicked in thread {} at {place}: {}",
                this_thread(),
                panic.payload_as_str().unwrap_or("no message"),
            ));
            say(&format!("its stack then: {}", functions()));
        }
        before(panic);
    }));
    mechanism::watch_for_faults();
}

/// The thread being felled, by its name when it has one.
fn this_thread() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .map_or_else(|| format!("{:?}", thread.id()), str::to_string)
}

/// The stack as functions and lines, on one line.
fn functions() -> String {
    std::backtrace::Backtrace::force_capture()
        .to_string()
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(windows)]
mod mechanism {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, HMODULE};
    use windows_sys::Win32::System::Diagnostics::Debug::{
        EXCEPTION_POINTERS, EXCEPTION_RECORD, RtlCaptureStackBackTrace, SetUnhandledExceptionFilter,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW,
    };

    use super::{TOLD, functions, this_thread};

    /// What a filter answers to let Windows go on as it would have: its
    /// own report, then the end of the program.
    const LET_IT_GO_ON: i32 = 0;

    /// Frames read off the stack at most.
    const FRAMES: u32 = 48;

    pub fn watch_for_faults() {
        // SAFETY: a filter of ours, which never fails to return.
        unsafe { SetUnhandledExceptionFilter(Some(faulted)) };
    }

    /// Called by Windows, on the faulting thread, for a fault nothing
    /// handled.
    unsafe extern "system" fn faulted(pointers: *const EXCEPTION_POINTERS) -> i32 {
        let Some(say) = TOLD.get() else {
            return LET_IT_GO_ON;
        };
        // SAFETY: Windows hands the filter a record of the fault that
        // lives for as long as the filter runs.
        let record = unsafe {
            pointers
                .as_ref()
                .and_then(|pointers| pointers.ExceptionRecord.as_ref())
        };
        if let Some(record) = record {
            say(&format!(
                "fault 0x{:08x} in thread {} at {}{}",
                record.ExceptionCode as u32,
                this_thread(),
                where_in(record.ExceptionAddress),
                touched(record),
            ));
        }
        say(&format!("its stack then: {}", modules()));
        say(&format!("and as functions: {}", functions()));
        LET_IT_GO_ON
    }

    /// For an access violation, what was touched and how.
    fn touched(record: &EXCEPTION_RECORD) -> String {
        if record.ExceptionCode != EXCEPTION_ACCESS_VIOLATION || record.NumberParameters < 2 {
            return String::new();
        }
        let how = match record.ExceptionInformation[0] {
            0 => "reading",
            1 => "writing",
            8 => "running",
            _ => "touching",
        };
        format!(", {how} 0x{:x}", record.ExceptionInformation[1])
    }

    /// The stack as modules and offsets, on one line.
    fn modules() -> String {
        let mut frames = [std::ptr::null_mut::<c_void>(); FRAMES as usize];
        // SAFETY: room for as many frames as asked for, no hash wanted.
        let found = unsafe {
            RtlCaptureStackBackTrace(0, FRAMES, frames.as_mut_ptr(), std::ptr::null_mut())
        };
        frames[..usize::from(found)]
            .iter()
            .map(|&frame| where_in(frame))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// An address as the module it lies in and how far into it.
    fn where_in(address: *mut c_void) -> String {
        let mut module: HMODULE = std::ptr::null_mut();
        // SAFETY: the address is only looked up, the module's count left
        // alone, and the slot for it is ours.
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                address.cast(),
                &mut module,
            )
        };
        if found == 0 {
            return format!("0x{:x}", address as usize);
        }
        let mut path = [0u16; 260];
        // SAFETY: a module just found, and room of ours said in full.
        let length = unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) };
        let path = String::from_utf16_lossy(&path[..length as usize]);
        let name = path.rsplit('\\').next().unwrap_or(&path);
        // A module is loaded at the address its handle is.
        format!("{name}+0x{:x}", address as usize - module as usize)
    }
}

#[cfg(not(windows))]
mod mechanism {
    /// Faults outside Windows end the program with the system's own word,
    /// on a console that is read.
    pub fn watch_for_faults() {}
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn a_panic_is_told_with_its_thread_its_place_and_its_stack() {
        let told = Arc::new(Mutex::new(Vec::<String>::new()));
        let kept = Arc::clone(&told);
        tell_what_fells_it(move |what| kept.lock().unwrap().push(what.to_string()));

        let felled = std::thread::Builder::new()
            .name("felled on purpose".to_string())
            .spawn(|| panic!("as the test asks"))
            .unwrap()
            .join();
        assert!(felled.is_err());

        let told = told.lock().unwrap();
        let panicked = told
            .iter()
            .position(|line| line.contains("as the test asks"))
            .unwrap_or_else(|| panic!("{told:?}"));
        assert!(
            told[panicked].starts_with("panicked in thread felled on purpose at "),
            "{}",
            told[panicked]
        );
        assert!(told[panicked].contains("felled.rs"), "{}", told[panicked]);
        assert!(
            told[panicked + 1].starts_with("its stack then: "),
            "{told:?}"
        );
    }
}
