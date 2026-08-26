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
    CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::{
    DuplicateTokenEx, SECURITY_ATTRIBUTES, SecurityImpersonation, SetTokenInformation,
    TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
    TOKEN_QUERY, TokenPrimary, TokenSessionId,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_CREATION_DISPOSITION,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_ALWAYS, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, CTRL_C_EVENT, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, DETACHED_PROCESS,
    GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use zyr_engine_host::{Launch, Launcher, Parting, Running};

/// Value Windows returns when no session is attached to the screen.
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// Desktop we aim at: the one carrying the interactive display.
const DESKTOP: &str = "winsta0\\default";

/// Device that swallows what is written to it, and gives nothing back.
const NOTHING: &str = "NUL";

/// Reserved argument that turns this program into the hand tapping the
/// engine on the shoulder; see `let_the_engine_go`.
///
/// An argument and not a command, like the one Windows starts the
/// service with: nobody types it, and it names a moment rather than
/// something a person can ask for.
pub const LET_GO_ARGUMENT: &str = "--let-the-engine-go";

/// The same for the one keystroke Windows keeps for itself; see
/// `send_the_secure_attention`.
pub const ATTENTION_ARGUMENT: &str = "--send-the-secure-attention";

/// The same again for the speakers of this computer; see
/// `move_the_speakers`.
///
/// It carries which way they are to be moved, because both ways are the
/// same errand and one name for it is one name to keep in step.
pub const SPEAKERS_ARGUMENT: &str = "--set-the-speakers";
pub const SPEAKERS_QUIET: &str = "quiet";
pub const SPEAKERS_PLAYING: &str = "playing";

/// What that errand answers with.
///
/// Three answers and not two, because whoever asked has to know whether
/// it now owes the person their sound back. Muting speakers that were
/// already muted owes nothing, and giving that sound back at the end of
/// a session would be undoing something this product never did.
pub const SPEAKERS_MOVED: u32 = 0;
pub const SPEAKERS_REFUSED: u32 = 1;
pub const SPEAKERS_ALREADY: u32 = 2;

/// Time left to the ask itself: starting a program in another session,
/// attaching to a console and sending one interruption down it.
///
/// Nothing here waits for the engine; that is the wait below. This one
/// only covers the messenger, and a messenger that has not come back in
/// this long is not going to.
const ASKING: Duration = Duration::from_secs(5);

/// Time left to the engine to put the screen back and go of its own
/// accord, once it has been asked.
///
/// It is what the engine's own service leaves it, and the engine gives
/// itself ten seconds before calling its shutdown stuck: past this, it is
/// not coming.
const GOING: Duration = Duration::from_secs(20);

/// Time left to the engine to disappear once it has been taken. Beyond
/// it, the job object takes over, Windows killing a service that drags
/// out a stop.
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
    /// Session it was started in, which is where its console lives.
    ///
    /// Remembered rather than asked for again when it is stopped: one of
    /// the reasons for stopping it is that the screen has moved to
    /// another session, and the console did not move with it.
    session: u32,
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

    fn stop(&mut self) -> io::Result<Parting> {
        // Asked before it is taken. The engine puts the far computer's
        // screen back the size and the magnification it found it at as
        // it goes, and only as it goes: taken outright it never runs
        // that, and the screen stays at the size of whoever was watching
        // it. That is what a computer shut down from inside a session
        // came back to.
        if asked_to_go(self.session, self.identifier).is_ok() {
            // Safe: the handle stays valid for as long as this structure.
            let waited = unsafe { WaitForSingleObject(self.process.0, GOING.as_millis() as u32) };
            if waited == WAIT_OBJECT_0 {
                return Ok(Parting::OfItsOwnAccord);
            }
        }
        // A process already gone refuses to be terminated, which is not
        // a problem: what counts is that it is no longer there when we
        // hand back.
        // Safe: the handle stays valid for as long as this structure.
        unsafe { TerminateProcess(self.process.0, 1) };
        let waited = unsafe { WaitForSingleObject(self.process.0, STOP_DELAY.as_millis() as u32) };
        if waited == WAIT_OBJECT_0 {
            return Ok(Parting::Taken);
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
    keep_what_the_engine_said(&launch.log);
    // Append access and not plain write: every line the engine writes
    // then lands at the end of the file as it stands, wherever its own
    // cursor was. With plain write, emptying the journal from the window
    // while the engine ran made its next line land at its old position,
    // behind a gap of nothing, and the file never read clean again.
    //
    // Opened and not created: created, it was emptied at every start of
    // the engine, and the engine starts again whenever the service does,
    // whenever the screen moves to another session, whenever what this
    // computer serves is changed, and whenever it falls over. What it
    // said about a fault was therefore gone minutes later, which is
    // exactly when somebody comes looking for it.
    let log = inheritable_file(launch.log.as_os_str(), FILE_APPEND_DATA, OPEN_ALWAYS)?;

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
        let failure = io::Error::last_os_error();
        // Held by nothing, this engine would outlive the service with
        // nobody able to reach it: it is ended here instead.
        // Safe: the handle is valid and belongs to us alone.
        unsafe { TerminateProcess(process.0, 1) };
        return Err(failure);
    }

    Ok(SessionProcess {
        _job: job,
        process,
        identifier: started.dwProcessId,
        session,
    })
}

/// Asks the engine to go, from where it can be asked.
///
/// Not from here. The way to ask a program to end itself on Windows is
/// the interruption a console carries, and a console belongs to the
/// session it was opened in: a service lives in a session of its own and
/// cannot reach into another one. So the ask is made by this same
/// program, started for that one purpose in the session the engine is in,
/// which is exactly how the engine's own service does it.
///
/// Detached from any console of its own, since attaching to somebody
/// else's is only possible for a program that has none.
fn asked_to_go(session: u32, engine: u32) -> io::Result<()> {
    errand(
        session,
        &[LET_GO_ARGUMENT.to_string(), engine.to_string()],
        "the engine's console would not take the interruption",
    )
}

/// Where an errand's refusal is read decides what language it is in.
///
/// This one is read in the session menu of the far computer, by the
/// person who clicked, so it is written for them. The two others below
/// are read in this service's journal, which is written in English like
/// the rest of it.
const ATTENTION_REFUSED: &str = "la frappe n'est pas partie : la stratégie qui l'autorise \
                                 n'est peut-être pas posée sur cet ordinateur";

/// Sends this computer the one keystroke no keyboard of ours can carry.
///
/// Ctrl+Alt+Suppr is Windows' own, at both ends of a session. The
/// computer watching never sees it, because its Windows takes it first;
/// and the computer being watched cannot be made to feel it by any
/// engine, because engines type the way Windows refuses for this one.
/// The single door is `SendSAS`, and it opens only for a program the
/// system trusts, which on the host is this service.
///
/// From the session on screen and never from the service's own. The
/// sequence lands on the session the token carries, and a service lives
/// in one with no screen: pressed there, it would be pressed where
/// nobody is looking.
pub fn press_the_secure_attention() -> io::Result<()> {
    let session = session_on_screen()
        .ok_or_else(|| io::Error::other("aucune session n'est à l'écran de cet ordinateur"))?;
    errand(
        session,
        &[ATTENTION_ARGUMENT.to_string()],
        ATTENTION_REFUSED,
    )
}

/// Moves this computer's speakers, and says whether they really moved.
///
/// From the session that owns the screen, like everything else here, and
/// for a reason of its own: which device the desktop plays to is a
/// question whose answer depends on who is signed in. Asked from the
/// service's own session, it would name a device nobody is listening to,
/// and the room would go on playing.
///
/// `true` means they were doing the opposite a moment ago and are now
/// doing what was asked, which is also « something is owed back ».
pub fn set_the_speakers(quiet: bool) -> io::Result<bool> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    let way = if quiet {
        SPEAKERS_QUIET
    } else {
        SPEAKERS_PLAYING
    };
    let refused = "the speakers could not be reached from the session on screen";
    match errand_code(
        session,
        &[SPEAKERS_ARGUMENT.to_string(), way.to_string()],
        refused,
    )? {
        SPEAKERS_MOVED => Ok(true),
        SPEAKERS_ALREADY => Ok(false),
        _ => Err(io::Error::other(refused)),
    }
}

/// Whether this program was started to move the speakers, and which way.
pub fn asked_about_the_speakers() -> Option<bool> {
    the_way_named_in(std::env::args())
}

/// The same, over any list of arguments, so it can be checked without
/// starting a program to hold them.
fn the_way_named_in(arguments: impl Iterator<Item = String>) -> Option<bool> {
    let mut after = arguments.skip_while(|a| a != SPEAKERS_ARGUMENT);
    after.next()?;
    match after.next()?.as_str() {
        SPEAKERS_QUIET => Some(true),
        SPEAKERS_PLAYING => Some(false),
        _ => None,
    }
}

/// Moves them, from inside the session that owns the screen.
///
/// This is the whole of what this program does when started with
/// `SPEAKERS_ARGUMENT`. What went wrong is written into the service's own
/// journal from here rather than carried back in the exit code: there is
/// more than one way for a computer to have no reachable sound, and a
/// number would tell nobody which of them happened.
#[cfg(windows)]
pub fn move_the_speakers(quiet: bool) -> u32 {
    let said = |what: String| {
        if let Ok(log) = zyr_proto::log::Log::open(&crate::service::log_path()) {
            log.write(&what);
        }
    };
    let already = match zyr_sound::speakers_muted() {
        Ok(muted) => muted,
        Err(e) => {
            said(format!("speakers not read: {e}"));
            return SPEAKERS_REFUSED;
        }
    };
    if already == quiet {
        return SPEAKERS_ALREADY;
    }
    match zyr_sound::mute_speakers(quiet) {
        Ok(()) => SPEAKERS_MOVED,
        Err(e) => {
            said(format!("speakers not moved: {e}"));
            SPEAKERS_REFUSED
        }
    }
}

/// Runs this program in another Windows session, for one short errand.
///
/// The service cannot reach into the session that owns the screen, and
/// three things it has to do live there: asking the engine to go,
/// pressing what only that session can be pressed on, and moving the
/// speakers the person in front of that session hears. All three are the
/// same shape, so they are the same code: this program started again with
/// a reserved argument, as itself, on the interactive desktop, with the
/// answer read back from its exit code.
///
/// Detached from any console of its own, since one of the errands is
/// attaching to somebody else's, which is only possible for a program
/// that has none.
fn errand(session: u32, arguments: &[String], refused: &str) -> io::Result<()> {
    match errand_code(session, arguments, refused)? {
        0 => Ok(()),
        _ => Err(io::Error::other(refused.to_string())),
    }
}

/// The same, for the errand whose answer is more than « it worked ».
///
/// The refusal covers an errand that never came back as well as one that
/// came back saying no: whoever reads it can do nothing different about
/// the two, and one message means one language to choose rather than two.
fn errand_code(session: u32, arguments: &[String], refused: &str) -> io::Result<u32> {
    let ourselves = std::env::current_exe()?;
    let token = service_token_for(session)?;
    let environment = environment_of(&token)?;

    let mut line = command_line(&ourselves, arguments);
    let mut desktop: Vec<u16> = wide(DESKTOP);

    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    startup.lpDesktop = desktop.as_mut_ptr();

    let mut started: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // Safe: every buffer lives until the call returns, and both handles
    // it hands back are taken in charge straight away.
    let obtained = unsafe {
        CreateProcessAsUserW(
            token.0,
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_UNICODE_ENVIRONMENT | DETACHED_PROCESS,
            environment.0,
            std::ptr::null(),
            &startup,
            &mut started,
        )
    };
    if obtained == 0 {
        return Err(io::Error::last_os_error());
    }
    let asking = Handle(started.hProcess);
    drop(Handle(started.hThread));

    // Safe: the handle is valid, and the wait is bounded.
    let waited = unsafe { WaitForSingleObject(asking.0, ASKING.as_millis() as u32) };
    if waited != WAIT_OBJECT_0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, refused.to_string()));
    }
    let mut code: u32 = 0;
    // Safe: the handle is valid and the code is written into a local.
    if unsafe { GetExitCodeProcess(asking.0, &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}

/// Taps the engine on the shoulder, from inside its own session.
///
/// This is the whole of what this program does when it is started with
/// `LET_GO_ARGUMENT`: it attaches to the engine's console and sends the
/// interruption down it, which the engine answers by putting the screen
/// back and stopping. It then hands back straight away; whether the
/// engine really went is watched by the service, which holds it.
///
/// Its own handling of that interruption is switched off first, or the
/// ask would take this program down before it has been made.
pub fn let_the_engine_go(engine: u32) -> bool {
    // Safe: three calls with no buffer of ours, each answering with
    // nought when it refuses. Letting go of a console we do not have is
    // one of the refusals, and it is the ordinary case: a program
    // started detached has none.
    unsafe {
        FreeConsole();
        if AttachConsole(engine) == 0 {
            return false;
        }
        SetConsoleCtrlHandler(None, 1);
        GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) != 0
    }
}

/// The engine this program was started to tap on the shoulder, if that
/// is what it was started for.
pub fn the_engine_to_let_go() -> Option<u32> {
    the_engine_named_in(std::env::args())
}

/// The same, over any list of arguments, so it can be tried without
/// starting a program to hold them.
fn the_engine_named_in(arguments: impl Iterator<Item = String>) -> Option<u32> {
    let mut after = arguments.skip_while(|a| a != LET_GO_ARGUMENT);
    after.next()?;
    after.next()?.parse().ok()
}

/// Whether this program was started to press what Windows keeps.
pub fn asked_for_the_secure_attention() -> bool {
    std::env::args().any(|argument| argument == ATTENTION_ARGUMENT)
}

/// Presses it, from inside the session that owns the screen.
///
/// This is the whole of what this program does when started with
/// `ATTENTION_ARGUMENT`. The call lives in a library Windows ships and is
/// found by hand rather than linked against: a machine whose policy
/// forbids the sequence still has the library, so linking would buy
/// nothing and would make every other errand of this program depend on a
/// file it has no other use for.
///
/// Windows answers nothing at all: `SendSAS` returns void, and whether
/// the sequence was really let through is decided by a policy this
/// service lays at its own installation. So what is reported back is
/// « the call was made », and the person watching their screen is the
/// one who knows whether it worked.
#[cfg(windows)]
pub fn send_the_secure_attention() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let name = wide("sas.dll");
    // SAFETY: a library of the system's own, named as a wide string that
    // outlives the call, and let go of by the process ending a moment
    // later. Nothing here is unloaded by hand: the sequence is on its way
    // and the process is one line from exiting.
    let found = unsafe {
        let library = LoadLibraryW(name.as_ptr());
        if library.is_null() {
            return false;
        }
        GetProcAddress(library, c"SendSAS".as_ptr().cast())
    };
    let Some(send) = found else {
        return false;
    };
    // SAFETY: the one function that library exports, whose shape is
    // `VOID SendSAS(BOOL)`, taken from the system's own documentation.
    let send: unsafe extern "system" fn(i32) = unsafe { std::mem::transmute(send) };
    // Nought is « as the service », which is what this is: started with
    // the service's own token, moved to the session on screen. One is for
    // a program running as the person, which this never is.
    unsafe { send(0) };
    true
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

/// Keeps what the engine said before, and marks where its next run
/// begins.
///
/// Written with the product's own journal writer and then let go of, a
/// moment before the engine is handed the file: that writer never empties
/// what it opens and cuts the file back from its top once it has grown
/// past reason, which is the rule every other log of this product follows
/// and the one this file was missing. The line it leaves is what tells
/// one run of the engine from the one before it.
///
/// A file that cannot be written to is not a reason to refuse to start an
/// engine: the engine will make its own.
fn keep_what_the_engine_said(log: &Path) {
    if let Ok(kept) = zyr_proto::log::Log::open(log) {
        kept.write("--- engine starting ---");
    }
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
    fn the_engine_to_tap_on_the_shoulder_is_read_from_the_arguments() {
        let said =
            |arguments: &[&str]| the_engine_named_in(arguments.iter().map(|a| a.to_string()));
        assert_eq!(said(&["zyrdeskd.exe", LET_GO_ARGUMENT, "1234"]), Some(1234));
        // Started for anything else, this program has no engine to tap:
        // the ordinary commands must go on reaching clap untouched.
        assert_eq!(said(&["zyrdeskd.exe", "status"]), None);
        assert_eq!(said(&["zyrdeskd.exe"]), None);
        // And a number that is not one names nobody, which is safer than
        // naming whatever happens to hold that place.
        assert_eq!(said(&["zyrdeskd.exe", LET_GO_ARGUMENT]), None);
        assert_eq!(said(&["zyrdeskd.exe", LET_GO_ARGUMENT, "plus tard"]), None);
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
