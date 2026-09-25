//! Launching the engine in the session shown on screen.
//!
//! This is what makes remote access possible before anyone opens a
//! Windows session. A service runs in a session of its own, with no
//! screen and no desktop: an engine started there would capture nothing.
//! It has to go into the session attached to the physical screen, the
//! one where the sign-in prompt appears.
//!
//! The engine is this very program, started again with a reserved
//! argument naming the link it is to serve a session on. The token used
//! is the service's own, the system account's, simply attached to that
//! session. Borrowing the logged-in user's token would feel more natural
//! but would forbid capturing the secure desktop: elevation prompts and
//! the sign-in screen would stay black.
//!
//! The process we start is born inside a job object set to kill it along
//! with its parent. Without that, a service stopping abruptly would leave
//! an orphan engine behind, invisible and impossible to take back in
//! hand.

use std::ffi::{OsStr, OsString, c_void};
use std::fmt;
use std::io;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, Instant};

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
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, DETACHED_PROCESS,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
    GetExitCodeProcess, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    OpenProcessToken, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use zyr_proto::paths;
use zyr_proto::session::WantedScreen;

use crate::gateway::{Launched, Launcher, with_its_code};

/// Value Windows returns when no session is attached to the screen.
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// What this module's lines are filed under.
const TAG: &str = "desk";

/// Desktop we aim at: the one carrying the interactive display.
const DESKTOP: &str = "winsta0\\default";

/// Device that swallows what is written to it, and gives nothing back.
const NOTHING: &str = "NUL";

/// Reserved argument that turns this program into the engine of one
/// session, followed by the name of the link it serves it on.
///
/// An argument and not a command, like the one Windows starts the
/// service with: nobody types it, and it names a moment rather than
/// something a person can ask for.
pub const SERVE_ARGUMENT: &str = "--serve-a-session";

/// Where the engine's own console output goes, which is what it says
/// before its journal is open, and what a crash leaves behind.
const ENGINE_CONSOLE: &str = "engine-console.log";

/// The same for the speakers of this computer; see
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

/// And the same for locking this computer's screen; see
/// `lock_this_desktop`.
///
/// Windows will only take that order from a program on the interactive
/// desktop, which a service is not, and there is no way round it: it is
/// what makes a lock screen worth trusting.
pub const LOCK_ARGUMENT: &str = "--lock-the-screen";

/// And the same for this computer's desk; see `do_this_to_the_desk`.
///
/// Two names and not one, because they are two errands with nothing in
/// common but the subject: one holds the desk for a session that is
/// starting, the other gives it back when that session has gone. The
/// first carries what the session wants, the second carries nothing at
/// all, what to put back having been written down when it was taken.
///
/// Here for the reason all of these are here. Everything Windows says
/// about the arrangement of screens is answered for the window station of
/// whoever asks, and a service sits on one with no screens at all: asked
/// from there, this computer has no screens, which is what it used to
/// answer a session that asked what it was showing.
pub const DESK_ARGUMENT: &str = "--hold-the-desk";
pub const DESK_BACK_ARGUMENT: &str = "--give-the-desk-back";

/// And a third, for the computer whose own screens cannot draw the size
/// a session asked for: the desktop moves onto the screen this computer
/// grew for itself, which the service has just woken at that size.
///
/// Its own errand and not part of the first, because the two happen
/// either side of something only the service can do. Starting a display
/// device is administrator work, so the service wakes the screen; putting
/// a desktop on it is window station work, so the session on screen does
/// that. One cannot wait for the other inside a single errand.
pub const DESK_GROWN_ARGUMENT: &str = "--take-the-grown-screen";

/// And a sixth, for the shape of this computer's pointer; see
/// `crate::pointer`.
///
/// The same blindness once more, and the plainest case of it: a pointer
/// belongs to a desktop, the desktop that owns the input belongs to the
/// session on screen, and the service's window station carries no
/// desktop at all. Asked from there, this computer has no pointer, which
/// is exactly what it answered a session that asked for the shape of it.
///
/// This one differs from the five above in one way: it does not do a
/// thing and come back, it reads for a while. It ends by itself after a
/// short life so that nothing has to end it, and the service starts
/// another for as long as somebody is asking.
pub const POINTER_ARGUMENT: &str = "--follow-the-pointer";

/// And a seventh, for this computer's clipboard; see
/// `crate::clipboard`.
///
/// The same blindness again: a clipboard belongs to a window station, and
/// the one a service sits on carries none at all. Asked from there, this
/// computer's clipboard is a clipboard nobody has ever copied anything
/// to, and anything written to it is written where nobody will paste.
///
/// Like the pointer above, it reads for a while rather than doing one
/// thing, and ends by itself. Unlike it, it writes too: what was copied
/// on the far computer is put on this one from here.
pub const CLIPBOARD_ARGUMENT: &str = "--carry-the-clipboard";

/// What the first of those carries when a session wants the desk noted
/// and nothing moved, which is what « keep your own screen » asks for.
///
/// A word and not an absent argument: an errand that names what it wants
/// and an errand that lost its argument on the way must not look alike.
const NOTHING_WANTED: &str = "none";

/// Time left to an errand: starting a program in another session, doing
/// the one thing it went for and coming back. An errand that has not
/// come back in this long is not going to.
const ASKING: Duration = Duration::from_secs(5);

/// Identifier of the session attached to the physical screen.
///
/// It changes on sign-in, on user switch and on sign-out: this is not a
/// value to remember.
pub fn session_on_screen() -> Option<u32> {
    // Safe: the function takes nothing and returns an integer.
    let session = unsafe { WTSGetActiveConsoleSessionId() };
    (session != NO_SESSION).then_some(session)
}

/// Starts the engine of each incoming session in the session attached
/// to the screen.
#[derive(Debug, Clone, Copy)]
pub struct ServingInSession {
    session: u32,
}

impl ServingInSession {
    pub fn new(session: u32) -> Self {
        Self { session }
    }
}

impl Launcher for ServingInSession {
    fn launch(&self, link: &str) -> io::Result<Box<dyn Launched>> {
        let ourselves = std::env::current_exe()?;
        let arguments = [SERVE_ARGUMENT.to_string(), link.to_string()];
        let console = paths::logs_dir().join(ENGINE_CONSOLE);
        let launch = Launch {
            exe: &ourselves,
            arguments: &arguments,
            working_dir: ourselves.parent(),
            log: &console,
        };
        Ok(Box::new(start_in_session(&launch, self.session)?))
    }
}

/// The link this program was started to serve a session on, if that is
/// what it was started for.
pub fn the_link_to_serve() -> Option<String> {
    the_link_named_in(std::env::args())
}

/// The same, over any list of arguments, so it can be read without
/// starting a program to hold them.
fn the_link_named_in(arguments: impl Iterator<Item = String>) -> Option<String> {
    let mut after = arguments.skip_while(|a| a != SERVE_ARGUMENT);
    after.next()?;
    after.next().filter(|link| !link.is_empty())
}

/// A program to start in another session.
struct Launch<'a> {
    exe: &'a Path,
    arguments: &'a [String],
    working_dir: Option<&'a Path>,
    /// Where what it writes on its console goes.
    log: &'a Path,
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
/// the guarantee no engine outlives the service that started it.
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

impl Launched for SessionProcess {
    fn process(&self) -> u32 {
        self.identifier
    }

    fn let_go(self: Box<Self>, within: Duration) -> io::Result<Option<u32>> {
        // Safe: the handle stays valid for as long as this structure, and
        // the wait is bounded.
        let waited = unsafe {
            WaitForSingleObject(
                self.process.0,
                within.as_millis().min(u128::from(u32::MAX - 1)) as u32,
            )
        };
        if waited == WAIT_TIMEOUT {
            // Dropped on the way out, which closes the job and takes it.
            return Ok(None);
        }
        if waited != WAIT_OBJECT_0 {
            return Err(refusal_of("WaitForSingleObject"));
        }
        let mut code: u32 = 0;
        // Safe: the handle is valid and the code is written into a local.
        if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
            return Err(refusal_of("GetExitCodeProcess"));
        }
        Ok(Some(code))
    }
}

/// What the system just refused, named by the call that refused it and
/// with its own number for the refusal.
///
/// One line of the journal then says which step broke and why, where the
/// bare message would leave a person with « Accès refusé » and a dozen
/// calls to choose from.
fn refusal_of(call: &str) -> io::Error {
    let refused = io::Error::last_os_error();
    io::Error::new(
        refused.kind(),
        format!("{call}: {}", with_its_code(&refused)),
    )
}

/// How many things a process is given at its birth: the handles it
/// inherits and the job it is born in.
const BIRTH_ATTRIBUTES: u32 = 2;

/// What a process is handed at its birth beyond its startup information:
/// the only handles it inherits, and the job it is born in.
///
/// Both answer a service that starts programs from several threads at
/// once. Told to inherit with no list, a process takes every inheritable
/// handle of the service at that moment, those another thread is handing
/// to a program of its own included: holding the writing end of a pipe
/// the service reads until it closes, an engine keeps that reader waiting
/// for as long as its session lasts. And started outside its job, then
/// put into it, a process spends a moment held by nothing, which a
/// service falling over at that moment turns into an engine nobody can
/// reach.
///
/// Borrows the handles and the job it names: Windows reads them where
/// they stand when the process starts, so they must neither move nor
/// close until then.
struct Birth<'a> {
    /// The list Windows fills, as large as it asked for. Words and not
    /// bytes, so it sits where the structure inside it wants to.
    list: Vec<usize>,
    named: PhantomData<&'a [HANDLE]>,
}

impl<'a> Birth<'a> {
    fn new(inherited: &'a [HANDLE], job: &'a HANDLE) -> io::Result<Self> {
        let mut size = 0usize;
        // Safe: asked without a list, the call only says how large one
        // has to be, refusing as it does.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), BIRTH_ATTRIBUTES, 0, &mut size)
        };
        if size == 0 {
            return Err(refusal_of("InitializeProcThreadAttributeList"));
        }
        let mut list = vec![0usize; size.div_ceil(size_of::<usize>())];
        // Safe: the memory is ours, and as large as Windows asked for.
        let made = unsafe {
            InitializeProcThreadAttributeList(
                list.as_mut_ptr().cast(),
                BIRTH_ATTRIBUTES,
                0,
                &mut size,
            )
        };
        if made == 0 {
            return Err(refusal_of("InitializeProcThreadAttributeList"));
        }
        let mut birth = Self {
            list,
            named: PhantomData,
        };
        birth.give(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            inherited.as_ptr().cast(),
            size_of_val(inherited),
            "UpdateProcThreadAttribute (handles inherited)",
        )?;
        birth.give(
            PROC_THREAD_ATTRIBUTE_JOB_LIST,
            std::ptr::from_ref(job).cast(),
            size_of::<HANDLE>(),
            "UpdateProcThreadAttribute (job)",
        )?;
        Ok(birth)
    }

    fn give(
        &mut self,
        attribute: u32,
        value: *const c_void,
        size: usize,
        call: &str,
    ) -> io::Result<()> {
        // Safe: the list is ready, and what it is given outlives it, which
        // the lifetime of this structure holds it to.
        let given = unsafe {
            UpdateProcThreadAttribute(
                self.list(),
                0,
                attribute as usize,
                value,
                size,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if given == 0 {
            return Err(refusal_of(call));
        }
        Ok(())
    }

    /// The list, as the startup information carries it.
    fn list(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.list.as_mut_ptr().cast()
    }
}

impl Drop for Birth<'_> {
    fn drop(&mut self) {
        // Safe: a `Birth` only exists once its list was made ready, and
        // it is let go of here only.
        unsafe { DeleteProcThreadAttributeList(self.list()) };
    }
}

/// Starts a program in the given session.
fn start_in_session(launch: &Launch, session: u32) -> io::Result<SessionProcess> {
    let token = service_token_for(session)?;
    let environment = environment_of(&token)?;
    let job = job_object()?;

    let nothing = inheritable_file(OsStr::new(NOTHING), GENERIC_READ, OPEN_EXISTING)?;
    keep_what_the_engine_said(launch.log);
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
    let inherited = [nothing.0, log.0];
    let mut birth = Birth::new(&inherited, &job.0)?;

    let mut line = command_line(launch.exe, launch.arguments);
    let mut desktop: Vec<u16> = wide(DESKTOP);
    let folder: Option<Vec<u16>> = launch.working_dir.map(|path| wide(path.as_os_str()));
    let startup = startup_with(desktop.as_mut_ptr(), inherited, &mut birth);
    let mut started = PROCESS_INFORMATION::default();

    // A new console would open a black window on the user's screen; the
    // engine's output already goes to our log file.
    // Safe: every buffer, the list and what it names live until the call
    // returns, and both handles it hands back are taken in charge
    // straight away.
    let obtained = unsafe {
        CreateProcessAsUserW(
            token.0,
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
            environment.0,
            folder.as_ref().map_or(std::ptr::null(), |f| f.as_ptr()),
            (&raw const startup).cast(),
            &mut started,
        )
    };
    if obtained == 0 {
        return Err(refusal_of("CreateProcessAsUserW"));
    }
    drop(Handle(started.hThread));
    // The list names the job, which is kept from here on.
    drop(birth);

    Ok(SessionProcess {
        _job: job,
        process: Handle(started.hProcess),
        identifier: started.dwProcessId,
    })
}

/// Startup information for a process whose input is the first of those
/// handles and whose output, both streams of it, is the second: the two
/// its birth lets it inherit. On that desktop, or on its parent's when
/// none is named.
fn startup_with(
    desktop: *mut u16,
    [input, output]: [HANDLE; 2],
    birth: &mut Birth,
) -> STARTUPINFOEXW {
    let mut startup = STARTUPINFOEXW::default();
    // The extended size, which is what tells Windows the list follows.
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.lpDesktop = desktop;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = input;
    startup.StartupInfo.hStdOutput = output;
    startup.StartupInfo.hStdError = output;
    startup.lpAttributeList = birth.list();
    startup
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
        (SPEAKERS_MOVED, _) => Ok(true),
        (SPEAKERS_ALREADY, _) => Ok(false),
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
            log.about(TAG).write(&what);
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

/// Locks this computer's screen, from the session that owns it.
///
/// The other half of Ctrl+Alt+Suppr, and the other way round. That one
/// goes through the service's own process, because Windows takes it from
/// a service and from nothing else; this one goes through a program on
/// the interactive desktop, because Windows takes it from there and from
/// nothing else. Both refusals protect the same thing: what a lock screen
/// is worth depends on nobody being able to put one up, or take one
/// down, from outside the desk it belongs to.
pub fn lock_the_screen() -> io::Result<Errand> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    errand(
        session,
        &[LOCK_ARGUMENT.to_string()],
        "the screen could not be locked from the session that owns it",
    )
}

/// Notes this computer's desk and puts its main screen where a session
/// wants it, from the session that owns that screen.
///
/// Asked before the engine opens on it: what the engine captures is
/// pixels, and both the size and the magnification decide how many of
/// them a letter is made of. Changed afterwards they would land in the
/// middle of a picture somebody is already watching, and everything on
/// the desktop would jump.
///
/// Nothing asked for still runs, and is not a wasted errand: it is how
/// the service learns what this computer is showing, which it cannot see
/// for itself and used to answer « I cannot measure my own screen » to.
///
/// Never fails a session. What a session loses is a desk the size it
/// asked for, which is a session slightly wrong and not a session
/// missing, and the sentences saying why go into this computer's journal.
pub fn hold_the_desk_for(wanted: Option<WantedScreen>) -> io::Result<Errand> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    let asked = match wanted {
        Some(screen) => screen.to_string(),
        None => NOTHING_WANTED.to_string(),
    };
    errand(
        session,
        &[DESK_ARGUMENT.to_string(), asked],
        "this computer's desk could not be set from the session that owns the screen",
    )
}

/// Moves this computer's desktop onto the screen it grew for itself, at
/// the size a session asked for, from the session that owns the screen.
///
/// Asked only after the service has woken that screen, and only where
/// this computer's own screens refused the size: everywhere else the
/// desktop stays where its owner left it.
pub fn take_the_grown_screen(wanted: WantedScreen) -> io::Result<Errand> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    errand(
        session,
        &[DESK_GROWN_ARGUMENT.to_string(), wanted.to_string()],
        "this computer's desktop could not be moved onto the screen it grew for itself",
    )
}

/// Whose a helper is, which decides what the desk lets it touch.
#[derive(Clone, Copy)]
enum Whose {
    /// The service's own account, moved onto that session's screen. What
    /// every errand here has always been: enough to read a desk, change a
    /// screen, tap an engine on the shoulder.
    TheService,
    /// The person signed in at that screen.
    ///
    /// For the one helper that does not merely look at the desk but acts
    /// on it in somebody's name. See [`start_carrying_the_clipboard`].
    ThePerson,
}

/// Starts a helper in the session that owns the screen, to read the
/// shape of this computer's pointer.
pub fn start_reading_the_pointer() -> io::Result<()> {
    start_a_helper(POINTER_ARGUMENT, Whose::TheService)
}

/// Whether this program was started to read the pointer.
pub fn asked_to_follow_the_pointer() -> bool {
    std::env::args().any(|argument| argument == POINTER_ARGUMENT)
}

/// Starts a helper in that same session, to read and write this
/// computer's clipboard.
///
/// The one helper started as the person and not as the service, and it
/// has to be. A clipboard is not simply a thing sitting on a window
/// station: text and pictures do sit there as plain blocks anybody with
/// the station can read, but files never do. What a program puts there
/// for files is a promise, an object living inside it, and reading or
/// paying that promise means one program calling into another. Windows
/// refuses that across accounts and across levels: the service is the
/// system, the Explorer is the person, and neither can reach into the
/// other. Under the service's account the object came back hollow going
/// one way, and what this computer offered was invisible going the other,
/// which is precisely the two halves that never worked.
pub fn start_carrying_the_clipboard() -> io::Result<()> {
    start_a_helper(CLIPBOARD_ARGUMENT, Whose::ThePerson)
}

/// Whether this program was started to carry the clipboard.
pub fn asked_to_carry_the_clipboard() -> bool {
    std::env::args().any(|argument| argument == CLIPBOARD_ARGUMENT)
}

/// Starts this program again in the session that owns the screen, to do
/// nothing but what that argument names.
///
/// Started and never waited for, which is what sets these apart from
/// every other errand here: those do one thing and hand back an answer,
/// these read for as long as they live and say what they read through
/// files. Nothing here holds on to one either; each ends by itself.
fn start_a_helper(argument: &str, whose: Whose) -> io::Result<()> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    let ourselves = std::env::current_exe()?;
    let token = match whose {
        Whose::TheService => service_token_for(session)?,
        Whose::ThePerson => the_person_at(session)?,
    };
    let environment = environment_of(&token)?;

    let mut line = command_line(&ourselves, &[argument.to_string()]);
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
        return Err(refusal_of("CreateProcessAsUserW"));
    }
    // Both handles are let go of at once: this program is not waited for
    // and not ended by anybody, so there is nothing to hold.
    drop(Handle(started.hProcess));
    drop(Handle(started.hThread));
    Ok(())
}

/// Puts the desk back the way it was noted, from the session that owns
/// the screen.
///
/// Asked when the last session goes, and asked again by the watch that
/// holds the engine for as long as a desk stays noted: a session whose
/// computer was closed, unplugged or crashed says nothing at all, and
/// that is exactly the session after which somebody's screens would stay
/// the way a stranger left them.
pub fn give_the_desk_back() -> io::Result<Errand> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    errand(
        session,
        &[DESK_BACK_ARGUMENT.to_string()],
        "this computer's desk could not be put back from the session that owns the screen",
    )
}

/// What this program was started to do to the desk, if that is what it
/// was started for.
pub fn the_desk_asked_for() -> Option<Desk> {
    the_desk_named_in(std::env::args())
}

/// One errand about this computer's desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desk {
    /// Note it, and put the main screen where a session wants it. Nothing
    /// wanted notes it and moves nothing.
    Hold(Option<WantedScreen>),
    /// Put it back the way it was noted.
    Back,
    /// Move the desktop onto the screen this computer grew for itself,
    /// at that size, this computer's own screens having refused it.
    Borrow(WantedScreen),
}

/// The same, over any list of arguments, so it can be read without
/// starting a program to hold them.
fn the_desk_named_in(arguments: impl Iterator<Item = String>) -> Option<Desk> {
    let arguments: Vec<String> = arguments.collect();
    if arguments.iter().any(|a| a == DESK_BACK_ARGUMENT) {
        return Some(Desk::Back);
    }
    if let Some(asked) = after_the_word(&arguments, DESK_GROWN_ARGUMENT) {
        return asked.parse().ok().map(Desk::Borrow);
    }
    let asked = after_the_word(&arguments, DESK_ARGUMENT)?;
    if asked == NOTHING_WANTED {
        return Some(Desk::Hold(None));
    }
    // A size that will not read is not nothing asked for: it is an errand
    // that was meant to move a screen and cannot say where to. Answering
    // « note the desk and move nothing » to it would leave the session
    // watching a desk at the wrong size with nothing in any journal.
    asked.parse().ok().map(|screen| Desk::Hold(Some(screen)))
}

/// What follows that word among those arguments, when it is there and
/// something follows it.
fn after_the_word(arguments: &[String], word: &str) -> Option<String> {
    let at = arguments.iter().position(|argument| argument == word)?;
    arguments.get(at + 1).cloned()
}

/// Does it, from inside the session that owns the screen.
///
/// This is the whole of what this program does when started with either
/// desk argument. What happened is written into the service's own journal
/// from here rather than carried back in an exit code: there is more than
/// one way for a desk not to move, and a number would tell nobody which
/// of them happened.
#[cfg(windows)]
pub fn do_this_to_the_desk(asked: Desk) {
    let said = match asked {
        Desk::Hold(wanted) => crate::screen::hold_the_desk_for(
            wanted.map(|screen| (screen.wide, screen.high, screen.scale)),
        ),
        Desk::Back => crate::screen::give_the_desk_back(),
        Desk::Borrow(screen) => {
            crate::screen::take_the_grown_screen_for((screen.wide, screen.high, screen.scale))
        }
    };
    if let Ok(log) = zyr_proto::log::Log::open(&crate::service::log_path()) {
        let log = log.about(TAG);
        for line in said {
            log.write(&line);
        }
    }
}

/// How long an errand took, in its two halves.
///
/// Split, and not added together, because the two costs have nothing to
/// do with each other and only one of them is ever worth working on.
/// Getting a program running in another Windows session is Windows'
/// price, paid every time and roughly the same; what the program then
/// takes to answer is the errand itself. A single number cannot say
/// which of the two a session is waiting on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Errand {
    /// Time spent getting the program running over there.
    pub started: Duration,
    /// Time it then took to do what it went for and answer.
    pub answered: Duration,
}

impl fmt::Display for Errand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ms starting a program in the session on screen, {} ms waiting for it",
            self.started.as_millis(),
            self.answered.as_millis()
        )
    }
}

/// Whether this program was started to lock the screen.
pub fn asked_to_lock_the_screen() -> bool {
    std::env::args().any(|a| a == LOCK_ARGUMENT)
}

/// Locks it, from inside the session that owns the screen.
///
/// This is the whole of what this program does when started with
/// `LOCK_ARGUMENT`. Windows takes the order and returns before the screen
/// has actually gone: what comes back says the order was accepted, and
/// nothing more is worth waiting for, the person who asked being at the
/// other end of a picture that will show them the lock screen.
#[cfg(windows)]
pub fn lock_this_desktop() -> bool {
    use windows_sys::Win32::System::Shutdown::LockWorkStation;

    // Safe: no buffer of ours, and it answers with nought when it
    // refuses.
    if unsafe { LockWorkStation() } == 0 {
        return false;
    }
    // Waited on from here, which is the only place it can be waited on:
    // locking hands the screen to another desktop, and which desktop has
    // the input is a question about the window station of whoever asks.
    // A service sits on one with no desktop and can only ever be told
    // « none ». This program is over here, on the interactive one, so
    // this is where the answer exists.
    //
    // Worth the wait rather than answering on the order alone: what the
    // person at the far end watches is the picture, the picture stops
    // while the desktop changes hands, and a « done » sent before that
    // starts is a « done » about nothing anyone can see. It also puts
    // the number in the host's journal, in the half of the errand it
    // belongs to.
    the_desktop_changed_hands();
    true
}

/// How long the desktop is given to change hands, and how often that is
/// asked.
///
/// Short at the top because nothing is owed past it: the order was taken,
/// the machine is locking, and standing here longer would only delay a
/// « done » that changes nothing. Short at the bottom because what is
/// being measured is a fraction of a second.
#[cfg(windows)]
const CHANGING_HANDS: Duration = Duration::from_millis(1500);
#[cfg(windows)]
const ASKING_AGAIN: Duration = Duration::from_millis(10);

/// The desktop nobody is locked out of, and the one a session runs on.
#[cfg(windows)]
const ORDINARY_DESKTOP: &str = "Default";

/// Waits until the screen belongs to another desktop than the ordinary
/// one, which is what locking really means.
#[cfg(windows)]
fn the_desktop_changed_hands() -> bool {
    let start = Instant::now();
    while start.elapsed() < CHANGING_HANDS {
        match desktop_with_the_input() {
            Some(name) if name != ORDINARY_DESKTOP => return true,
            // No answer at all is the same as not yet: the desktop is
            // being handed over and there is nothing to open in between.
            _ => std::thread::sleep(ASKING_AGAIN),
        }
    }
    false
}

/// Name of the desktop the screen and the keyboard belong to right now.
///
/// `Default` while somebody works, `Winlogon` while the machine is
/// locked or asking for a password.
#[cfg(windows)]
fn desktop_with_the_input() -> Option<String> {
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, UOI_NAME,
    };

    // Safe: nothing of ours is handed over, and a refusal answers null.
    let desktop = unsafe { OpenInputDesktop(0, 0, GENERIC_READ) };
    if desktop.is_null() {
        return None;
    }
    let mut name = [0u16; 64];
    let mut needed = 0u32;
    // Safe: the desktop is open, and the slot is ours with its length in
    // bytes given alongside it as the call expects.
    let read = unsafe {
        GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            (name.len() * size_of::<u16>()) as u32,
            &mut needed,
        )
    };
    // Safe: a desktop this function opened, closed exactly once.
    unsafe { CloseDesktop(desktop) };
    if read == 0 {
        return None;
    }
    let end = name.iter().position(|letter| *letter == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&name[..end]))
}

/// Runs this program in another Windows session, for one short errand.
///
/// The service cannot reach into the session that owns the screen, and
/// several things it has to do live there: moving the speakers the
/// person in front of that session hears, locking the screen, holding
/// the desk. They are the same shape, so they are the same code: this
/// program started again with a reserved argument, as itself, on the
/// interactive desktop, with the answer read back from its exit code.
///
/// Detached, with no console of its own: nobody is there to read one.
fn errand(session: u32, arguments: &[String], refused: &str) -> io::Result<Errand> {
    match errand_code(session, arguments, refused)? {
        (0, took) => Ok(took),
        _ => Err(io::Error::other(refused.to_string())),
    }
}

/// The same, for the errand whose answer is more than « it worked ».
///
/// The refusal covers an errand that never came back as well as one that
/// came back saying no: whoever reads it can do nothing different about
/// the two, and one message means one language to choose rather than two.
fn errand_code(session: u32, arguments: &[String], refused: &str) -> io::Result<(u32, Errand)> {
    let asked_at = Instant::now();
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
        return Err(refusal_of("CreateProcessAsUserW"));
    }
    let asking = Handle(started.hProcess);
    drop(Handle(started.hThread));
    let running_at = Instant::now();

    // Safe: the handle is valid, and the wait is bounded.
    let waited = unsafe { WaitForSingleObject(asking.0, ASKING.as_millis() as u32) };
    if waited != WAIT_OBJECT_0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, refused.to_string()));
    }
    let mut code: u32 = 0;
    // Safe: the handle is valid and the code is written into a local.
    if unsafe { GetExitCodeProcess(asking.0, &mut code) } == 0 {
        return Err(refusal_of("GetExitCodeProcess"));
    }
    Ok((
        code,
        Errand {
            started: running_at - asked_at,
            answered: running_at.elapsed(),
        },
    ))
}

/// The token of whoever is signed in at that session.
///
/// The person as Windows itself knows them at that screen, with the
/// ordinary rights of somebody sitting at their own desk and no others:
/// an administrator is handed the everyday half of their account, which
/// is the half their own Explorer runs under. That is the whole point,
/// since a program is only allowed to speak to another one on the desk
/// when the two are the same person at the same level.
///
/// Refuses where nobody is signed in, and that refusal is worth having:
/// a computer at its sign-in screen has no clipboard belonging to anyone,
/// and saying so beats starting something that would read an empty desk
/// for hours.
fn the_person_at(session: u32) -> io::Result<Handle> {
    let mut theirs: HANDLE = std::ptr::null_mut();
    // Safe: the handle it hands back is taken in charge right after. It
    // asks for a right only the system holds, which the service is.
    if unsafe { WTSQueryUserToken(session, &mut theirs) } == 0 {
        return Err(refusal_of("WTSQueryUserToken"));
    }
    Ok(Handle(theirs))
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
        return Err(refusal_of("OpenProcessToken"));
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
        return Err(refusal_of("DuplicateTokenEx"));
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
        return Err(refusal_of("SetTokenInformation"));
    }

    Ok(copy)
}

/// Environment block matching the token.
fn environment_of(token: &Handle) -> io::Result<Environment> {
    let mut block: *mut core::ffi::c_void = std::ptr::null_mut();
    // Safe: the token is valid, and the block it hands back is taken in
    // charge right after.
    if unsafe { CreateEnvironmentBlock(&mut block, token.0, 0) } == 0 {
        return Err(refusal_of("CreateEnvironmentBlock"));
    }
    Ok(Environment(block))
}

/// Job object that kills what it holds when it is closed.
fn job_object() -> io::Result<Handle> {
    // Safe: the function accepts null parameters, and the handle it
    // hands back is taken in charge right after.
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(refusal_of("CreateJobObjectW"));
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
        return Err(refusal_of("SetInformationJobObject"));
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
        return Err(refusal_of(&format!(
            "CreateFileW {}",
            Path::new(path).display()
        )));
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
    fn the_link_to_serve_is_read_from_the_arguments() {
        let said = |arguments: &[&str]| the_link_named_in(arguments.iter().map(|a| a.to_string()));
        let link = r"\\.\pipe\ZyrDesk-link-8fKq2Lr0aZ3x9Wm1";
        assert_eq!(
            said(&["zyrdeskd.exe", SERVE_ARGUMENT, link]),
            Some(link.to_string())
        );
        // Started for anything else, this program serves no session:
        // the ordinary commands must go on reaching clap untouched.
        assert_eq!(said(&["zyrdeskd.exe", "status"]), None);
        assert_eq!(said(&["zyrdeskd.exe"]), None);
        // And a link that is not named is no link at all.
        assert_eq!(said(&["zyrdeskd.exe", SERVE_ARGUMENT]), None);
        assert_eq!(said(&["zyrdeskd.exe", SERVE_ARGUMENT, ""]), None);
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

    /// A file of one test's own, in the temporary folder.
    fn scratch(what: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "zyrdeskd-session-{what}-{}.log",
            zyr_proto::random::alphanumeric_string(8)
        ))
    }

    /// Starts `cmd.exe` on that command the way an engine is started, the
    /// other session apart: born in its job, inheriting what its birth
    /// lists and nothing else, saying what it says into that file.
    fn born(command: &str, output: &Path) -> SessionProcess {
        let job = job_object().unwrap();
        let nothing = inheritable_file(OsStr::new(NOTHING), GENERIC_READ, OPEN_EXISTING).unwrap();
        let log = inheritable_file(output.as_os_str(), FILE_APPEND_DATA, OPEN_ALWAYS).unwrap();
        let inherited = [nothing.0, log.0];
        let mut birth = Birth::new(&inherited, &job.0).unwrap();
        let startup = startup_with(std::ptr::null_mut(), inherited, &mut birth);
        let mut line = wide(format!("cmd.exe /d /c {command}"));
        let mut started = PROCESS_INFORMATION::default();
        // Safe: as in `start_in_session`.
        let obtained = unsafe {
            windows_sys::Win32::System::Threading::CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                std::ptr::null(),
                std::ptr::null(),
                (&raw const startup).cast(),
                &mut started,
            )
        };
        assert_ne!(obtained, 0, "{}", refusal_of("CreateProcessW"));
        drop(Handle(started.hThread));
        drop(birth);
        SessionProcess {
            _job: job,
            process: Handle(started.hProcess),
            identifier: started.dwProcessId,
        }
    }

    /// Long enough that only being taken ends it within a test.
    const LINGERING: &str = "ping -n 30 127.0.0.1";

    #[test]
    fn a_process_says_what_it_says_where_it_is_told_and_goes_with_its_code() {
        let output = scratch("said");
        let started = born("echo born here& exit 7", &output);
        let went = Box::new(started).let_go(Duration::from_secs(10)).unwrap();
        assert_eq!(went, Some(7));
        let said = std::fs::read_to_string(&output).unwrap();
        assert!(said.contains("born here"), "{said:?}");
        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn a_process_is_born_in_its_job_and_taken_with_it() {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};

        let output = scratch("job");
        let started = born(LINGERING, &output);
        let mut inside = 0;
        // Safe: both handles stay open for as long as `started`.
        let asked = unsafe { IsProcessInJob(started.process.0, started._job.0, &mut inside) };
        assert_ne!(asked, 0, "{}", refusal_of("IsProcessInJob"));
        assert_ne!(inside, 0, "the process was born outside its job");

        // Safe: the handle is taken in charge right after.
        let watching = Handle(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, started.identifier) });
        assert!(!watching.0.is_null(), "{}", refusal_of("OpenProcess"));
        let went = Box::new(started)
            .let_go(Duration::from_millis(200))
            .unwrap();
        assert_eq!(went, None, "it was not meant to go by itself");
        // Safe: the handle is ours and the wait is bounded.
        let gone = unsafe { WaitForSingleObject(watching.0, 10_000) };
        assert_eq!(gone, WAIT_OBJECT_0, "the job did not take it");
        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn a_process_inherits_what_its_birth_lists_and_nothing_else() {
        use std::io::Read;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

        // The writing end of a pipe the service would be reading, left
        // inheritable by whoever started a program of their own at that
        // moment, and listed nowhere.
        let (mut reading, writing) = std::io::pipe().unwrap();
        // Safe: the handle is the pipe's own and open.
        let marked = unsafe {
            SetHandleInformation(
                writing.as_raw_handle(),
                HANDLE_FLAG_INHERIT,
                HANDLE_FLAG_INHERIT,
            )
        };
        assert_ne!(marked, 0, "{}", refusal_of("SetHandleInformation"));

        let output = scratch("inherited");
        let started = born(LINGERING, &output);
        drop(writing);
        // The only writer left is the one the process would hold had it
        // taken what it was not given: the reading ends at once, and not
        // when the process does.
        let (ended, end) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut rest = Vec::new();
            let _ = ended.send(reading.read_to_end(&mut rest).map(|_| rest));
        });
        let read = end
            .recv_timeout(Duration::from_secs(10))
            .expect("the process holds a handle it was never given");
        assert!(read.unwrap().is_empty());
        let _ = Box::new(started).let_go(Duration::ZERO);
        let _ = std::fs::remove_file(&output);
    }
}
