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

use std::io;
use std::path::Path;
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

/// From now on, what this program says on its console lands at the end
/// of that file, after a line marking where this run begins.
///
/// Rust says a few last things there and nowhere else, and says them just
/// before it ends a program without a panic or a fault: memory it could
/// not have, a stack run out of room, its own runtime giving up. A
/// program started by the system has no console, and they go to nobody.
/// An engine is handed a file for it when it is started; a program
/// nobody starts that way sets up its own here.
pub fn console_into(console: &Path, starting: &str) -> io::Result<()> {
    mechanism::console_into(console, starting)
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
    use std::fs::OpenOptions;
    use std::io::{self, Write};
    use std::os::windows::io::IntoRawHandle;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, HMODULE};
    use windows_sys::Win32::System::Console::{STD_ERROR_HANDLE, SetStdHandle};
    use windows_sys::Win32::System::Diagnostics::Debug::{
        EXCEPTION_POINTERS, EXCEPTION_RECORD, RtlCaptureStackBackTrace, SetUnhandledExceptionFilter,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW,
    };
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use zyr_proto::log::Room;

    use super::{TOLD, functions, this_thread};

    /// What a filter answers to let Windows go on as it would have: its
    /// own report, then the end of the program.
    const LET_IT_GO_ON: i32 = 0;

    /// Frames read off the stack at most.
    const FRAMES: u32 = 48;

    /// How long a line about a fault can be.
    const LINE: usize = 4096;

    pub fn watch_for_faults() {
        // SAFETY: a filter of ours, which never fails to return.
        unsafe { SetUnhandledExceptionFilter(Some(faulted)) };
    }

    pub fn console_into(console: &Path, starting: &str) -> io::Result<()> {
        crate::onscreen::mark_where_this_run_begins(console, starting);
        // Appended to, whatever the program's own idea of where it is,
        // like the engine's.
        let file = OpenOptions::new().append(true).create(true).open(console)?;
        // Kept open for as long as the program runs: it is its console.
        let handle = file.into_raw_handle();
        // SAFETY: a handle of ours, open, and never closed from here on.
        if unsafe { SetStdHandle(STD_ERROR_HANDLE, handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Called by Windows, on the faulting thread, for a fault nothing
    /// handled.
    ///
    /// The fault and the stack are put together in room of their own and
    /// said before anything asks the heap for memory: a heap broken
    /// already is the likeliest reason for being here, and asking it for
    /// more fells the program again before it has said a word. Only the
    /// functions, which cannot be named without memory, come last.
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
            let mut line = Room::<LINE>::new();
            // SAFETY: no argument, and a number back.
            let thread = unsafe { GetCurrentThreadId() };
            let _ = write!(
                line,
                "fault 0x{:08x} in thread {thread} at ",
                record.ExceptionCode as u32
            );
            where_in(&mut line, record.ExceptionAddress);
            touched(&mut line, record);
            say(line.as_str());
        }
        let mut frames = [std::ptr::null_mut::<c_void>(); FRAMES as usize];
        // SAFETY: room for as many frames as asked for, no hash wanted.
        let found = unsafe {
            RtlCaptureStackBackTrace(0, FRAMES, frames.as_mut_ptr(), std::ptr::null_mut())
        };
        let mut line = Room::<LINE>::new();
        let _ = write!(line, "its stack then: ");
        for (at, &frame) in frames[..usize::from(found)].iter().enumerate() {
            if at > 0 {
                let _ = write!(line, " | ");
            }
            where_in(&mut line, frame);
        }
        say(line.as_str());
        say(&format!(
            "and as functions, in thread {}: {}",
            this_thread(),
            functions()
        ));
        LET_IT_GO_ON
    }

    /// For an access violation, what was touched and how.
    fn touched(line: &mut impl Write, record: &EXCEPTION_RECORD) {
        if record.ExceptionCode != EXCEPTION_ACCESS_VIOLATION || record.NumberParameters < 2 {
            return;
        }
        let how = match record.ExceptionInformation[0] {
            0 => "reading",
            1 => "writing",
            8 => "running",
            _ => "touching",
        };
        let _ = write!(line, ", {how} 0x{:x}", record.ExceptionInformation[1]);
    }

    /// An address, as the module it lies in and how far into it.
    pub(super) fn where_in(line: &mut impl Write, address: *mut c_void) {
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
            let _ = write!(line, "0x{:x}", address as usize);
            return;
        }
        let mut path = [0u16; 260];
        // SAFETY: a module just found, and room of ours said in full.
        let length = unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) };
        let path = &path[..(length as usize).min(path.len())];
        let name = path
            .iter()
            .rposition(|&letter| letter == u16::from(b'\\'))
            .map_or(path, |at| &path[at + 1..]);
        for letter in char::decode_utf16(name.iter().copied()) {
            let _ = write!(line, "{}", letter.unwrap_or(char::REPLACEMENT_CHARACTER));
        }
        // A module is loaded at the address its handle is.
        let _ = write!(line, "+0x{:x}", address as usize - module as usize);
    }
}

#[cfg(not(windows))]
mod mechanism {
    use std::io;
    use std::path::Path;

    /// Faults outside Windows end the program with the system's own word,
    /// on a console that is read.
    pub fn watch_for_faults() {}

    /// Nothing of the product runs here without a console.
    pub fn console_into(_console: &Path, _starting: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this is not Windows: every program here keeps its console",
        ))
    }
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

    #[cfg(windows)]
    #[test]
    fn what_is_said_on_the_console_lands_in_its_file_after_the_mark_of_the_run() {
        use std::io::Write;
        use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, SetStdHandle};

        let console = std::env::temp_dir().join(format!(
            "zyr-system-console-{}.log",
            zyr_proto::random::alphanumeric_string(8)
        ));
        // SAFETY: no argument, and a handle back that is only kept.
        let before = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        console_into(&console, "--- run starting ---").unwrap();
        let said = std::io::stderr().write_all(b"said on the console\n");
        // SAFETY: the handle this process had before, put back.
        unsafe { SetStdHandle(STD_ERROR_HANDLE, before) };
        said.unwrap();

        let written = std::fs::read_to_string(&console).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.len(), 2, "{written}");
        assert!(lines[0].ends_with("--- run starting ---"), "{}", lines[0]);
        assert_eq!(lines[1], "said on the console");
        let _ = std::fs::remove_file(&console);
    }

    #[cfg(windows)]
    #[test]
    fn a_place_in_this_program_is_said_as_its_module_and_how_far_into_it() {
        let mut line = zyr_proto::log::Room::<256>::new();
        let place: fn() = a_place_in_this_program_is_said_as_its_module_and_how_far_into_it;
        mechanism::where_in(&mut line, place as *mut std::ffi::c_void);
        let program = std::env::current_exe().unwrap();
        let program = program.file_name().unwrap().to_string_lossy();
        assert!(
            line.as_str().starts_with(&format!("{program}+0x")),
            "{}",
            line.as_str()
        );
    }
}
