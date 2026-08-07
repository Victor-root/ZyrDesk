//! Launching the engine in the session shown on screen.
//!
//! This is what makes remote access possible before anyone opens a
//! Windows session. A service runs in a session of its own, with no
//! screen and no desktop: an engine started there would capture nothing.
//! It has to go into the session attached to the physical screen, the
//! one where the sign-in prompt appears.
//!
//! The token used is the service's own, the system account's, simply
//! attached to that session. Borrowing the logged-in user's token would
//! feel more natural but would forbid capturing the secure desktop:
//! elevation prompts and the sign-in screen would stay black, which is
//! exactly what this milestone has to make visible.
//!
//! The process we start is locked inside a job object set to kill it
//! along with its parent. Without that, a service stopping abruptly
//! would leave an orphan engine behind, invisible and impossible to take
//! back in hand.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::{
    DuplicateTokenEx, SECURITY_ATTRIBUTES, SecurityImpersonation, SetTokenInformation,
    TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
    TOKEN_QUERY, TokenPrimary, TokenSessionId,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_CREATION_DISPOSITION, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, GetCurrentProcess,
    GetExitCodeProcess, OpenProcessToken, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
    TerminateProcess, WaitForSingleObject,
};
use zyr_engine_host::{Launch, Launcher, Running};

/// Value Windows returns when no session is attached to the screen.
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// Desktop we aim at: the one carrying the interactive display.
const DESKTOP: &str = "winsta0\\default";

/// Device that swallows what is written to it, and gives nothing back.
const NOTHING: &str = "NUL";

/// Time left to the engine to disappear once told to stop. Beyond it,
/// the job object takes over, Windows killing a service that drags out a
/// stop.
const STOP_DELAY: Duration = Duration::from_secs(10);

/// Identifier of the session attached to the physical screen.
///
/// It changes on sign-in, on user switch and on sign-out: this is not a
/// value to remember.
pub fn session_on_screen() -> Option<u32> {
    // Safe: the function takes nothing and returns an integer.
    let session = unsafe { WTSGetActiveConsoleSessionId() };
    (session != NO_SESSION).then_some(session)
}

/// Starts the engine in the session attached to the screen.
#[derive(Debug, Clone, Copy)]
pub struct SessionLauncher {
    session: u32,
}

impl SessionLauncher {
    pub fn new(session: u32) -> Self {
        Self { session }
    }
}

impl Launcher for SessionLauncher {
    fn launch(&self, launch: &Launch) -> io::Result<Box<dyn Running>> {
        Ok(Box::new(start_in_session(launch, self.session)?))
    }
}

/// Handle closed for certain, whatever happens next.
///
/// Windows handles leak silently: one error in the middle of a run of
/// calls is enough to abandon one, and nothing reports it.
#[derive(Debug)]
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // Safe: the handle is valid and closed only here.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Environment block, given back to the system at the end.
#[derive(Debug)]
struct Environment(*mut core::ffi::c_void);

impl Drop for Environment {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // Safe: the block comes from CreateEnvironmentBlock.
            unsafe { DestroyEnvironmentBlock(self.0) };
        }
    }
}

/// Process started in the session, and the job object holding it.
///
/// Dropping it closes the job object, which kills the process: that is
/// the guarantee no engine outlives its supervisor.
#[derive(Debug)]
pub struct SessionProcess {
    _job: Handle,
    process: Handle,
    identifier: u32,
}

// Safe: a Windows handle belongs to the process, not to the thread that
// obtained it, and is usable from any of them. The standard library
// makes the same promise for the children it starts.
unsafe impl Send for SessionProcess {}

impl Running for SessionProcess {
    fn identifier(&self) -> u32 {
        self.identifier
    }

    fn exit_seen(&mut self) -> io::Result<Option<Option<i32>>> {
        // Asking the handle rather than the exit code: a process is
        // free to return the very value that means "still running".
        // Safe: the handle stays valid for as long as this structure.
        let waited = unsafe { WaitForSingleObject(self.process.0, 0) };
        if waited == WAIT_TIMEOUT {
            return Ok(None);
        }
        if waited != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }

        let mut code: u32 = 0;
        // Safe: the handle is valid and the code is written into a local.
        if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(Some(code as i32)))
    }

    fn stop(&mut self) -> io::Result<()> {
        // A process already gone refuses to be terminated, which is not
        // a problem: what counts is that it is no longer there when we
        // hand back.
        // Safe: the handle stays valid for as long as this structure.
        unsafe { TerminateProcess(self.process.0, 1) };
        let waited = unsafe { WaitForSingleObject(self.process.0, STOP_DELAY.as_millis() as u32) };
        if waited == WAIT_OBJECT_0 {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("the engine did not stop within {} s", STOP_DELAY.as_secs()),
        ))
    }
}

/// Starts a program in the given session.
fn start_in_session(launch: &Launch, session: u32) -> io::Result<SessionProcess> {
    let token = service_token_for(session)?;
    let environment = environment_of(&token)?;
    let job = job_object()?;

    let nothing = inheritable_file(OsStr::new(NOTHING), GENERIC_READ, OPEN_EXISTING)?;
    let log = inheritable_file(launch.log.as_os_str(), GENERIC_WRITE, CREATE_ALWAYS)?;

    let mut line = command_line(&launch.exe, &launch.arguments);
    let mut desktop: Vec<u16> = wide(DESKTOP);
    let folder: Option<Vec<u16>> = launch
        .working_dir
        .as_deref()
        .map(|path| wide(path.as_os_str()));

    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    startup.lpDesktop = desktop.as_mut_ptr();
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = nothing.0;
    startup.hStdOutput = log.0;
    startup.hStdError = log.0;

    let mut started: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // A new console would open a black window on the user's screen; the
    // engine's output already goes to our log file.
    // Safe: every buffer lives until the call returns, and both handles
    // it hands back are taken in charge straight away.
    let obtained = unsafe {
        CreateProcessAsUserW(
            token.0,
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            environment.0,
            folder.as_ref().map_or(std::ptr::null(), |f| f.as_ptr()),
            &startup,
            &mut started,
        )
    };
    if obtained == 0 {
        return Err(io::Error::last_os_error());
    }

    let process = Handle(started.hProcess);
    drop(Handle(started.hThread));

    // Safe: both handles are valid at this point.
    if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(SessionProcess {
        _job: job,
        process,
        identifier: started.dwProcessId,
    })
}

/// The service's token, duplicated and attached to the wanted session.
fn service_token_for(session: u32) -> io::Result<Handle> {
    let mut current: HANDLE = std::ptr::null_mut();
    // Safe: the current process is always valid, and the handle it hands
    // back is taken in charge right after.
    let obtained = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY,
            &mut current,
        )
    };
    if obtained == 0 {
        return Err(io::Error::last_os_error());
    }
    let current = Handle(current);

    let mut copy: HANDLE = std::ptr::null_mut();
    // Safe: the original token is valid, and the copy is taken in charge
    // right after.
    let obtained = unsafe {
        DuplicateTokenEx(
            current.0,
            TOKEN_ASSIGN_PRIMARY
                | TOKEN_DUPLICATE
                | TOKEN_QUERY
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID,
            std::ptr::null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut copy,
        )
    };
    if obtained == 0 {
        return Err(io::Error::last_os_error());
    }
    let copy = Handle(copy);

    // This is the line that moves the future process to the screen.
    // Safe: the size announced is that of the variable pointed at.
    let obtained = unsafe {
        SetTokenInformation(
            copy.0,
            TokenSessionId,
            (&raw const session).cast(),
            size_of::<u32>() as u32,
        )
    };
    if obtained == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(copy)
}

/// Environment block matching the token.
fn environment_of(token: &Handle) -> io::Result<Environment> {
    let mut block: *mut core::ffi::c_void = std::ptr::null_mut();
    // Safe: the token is valid, and the block it hands back is taken in
    // charge right after.
    if unsafe { CreateEnvironmentBlock(&mut block, token.0, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Environment(block))
}

/// Job object that kills what it holds when it is closed.
fn job_object() -> io::Result<Handle> {
    // Safe: the function accepts null parameters, and the handle it
    // hands back is taken in charge right after.
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = Handle(job);

    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    // Safe: the structure and its size match the information class we
    // ask for.
    let obtained = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if obtained == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(job)
}

/// File the started process inherits, for its output or its input.
///
/// The engine lands in another session, where nothing of ours reaches
/// it: handing it an open file is the only way to keep its output.
fn inheritable_file(
    path: &OsStr,
    access: u32,
    disposition: FILE_CREATION_DISPOSITION,
) -> io::Result<Handle> {
    let name = wide(path);
    let mut attributes: SECURITY_ATTRIBUTES = unsafe { std::mem::zeroed() };
    attributes.nLength = size_of::<SECURITY_ATTRIBUTES>() as u32;
    attributes.bInheritHandle = 1;

    // Safe: the name lives until the call returns, and the handle it
    // hands back is taken in charge right after.
    let file = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &attributes,
            disposition,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(file))
}

/// Zero-terminated string, the way Windows expects them.
fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Full command line, quotes included.
///
/// Windows receives it in one piece and cuts it up itself: without
/// quotes, a path containing a space would split into two arguments and
/// the program would not be found.
fn command_line(executable: &Path, arguments: &[String]) -> Vec<u16> {
    let mut line = OsString::new();
    line.push("\"");
    line.push(executable.as_os_str());
    line.push("\"");
    for argument in arguments {
        line.push(" \"");
        line.push(argument);
        line.push("\"");
    }
    wide(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a wide string back, without its trailing zero.
    fn read_back(wide: &[u16]) -> String {
        let without_zero = wide.strip_suffix(&[0]).expect("string not terminated");
        String::from_utf16(without_zero).unwrap()
    }

    #[test]
    fn a_wide_string_ends_with_a_zero() {
        let encoded = wide("desktop");
        assert_eq!(encoded.last(), Some(&0));
        assert_eq!(read_back(&encoded), "desktop");
    }

    #[test]
    fn a_path_with_spaces_stays_one_argument() {
        let line = command_line(
            Path::new(r"C:\Program Files\ZyrDesk\engine.exe"),
            &["config with spaces.conf".to_string()],
        );
        assert_eq!(
            read_back(&line),
            r#""C:\Program Files\ZyrDesk\engine.exe" "config with spaces.conf""#
        );
    }

    #[test]
    fn a_launch_without_arguments_stays_valid() {
        let line = command_line(Path::new(r"C:\engine.exe"), &[]);
        assert_eq!(read_back(&line), r#""C:\engine.exe""#);
    }

    #[test]
    fn the_absence_of_a_session_is_recognised() {
        // On a machine with no screen attached, Windows returns a
        // sentinel value that must never be taken for a session.
        assert_eq!(NO_SESSION, u32::MAX);
    }

    #[test]
    fn a_session_is_either_named_or_absent() {
        // Whatever this machine answers, the sentinel never comes back
        // out as a session number.
        assert_ne!(session_on_screen(), Some(NO_SESSION));
    }
}
