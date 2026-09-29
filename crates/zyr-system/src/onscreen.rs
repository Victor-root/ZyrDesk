//! The session that holds the screen, and starting this program in it.
//!
//! A service runs in a session of its own, with no screen and no
//! desktop: a program started there captures nothing, reads no pointer
//! and reaches no clipboard. What has to happen where the person is goes
//! into the session attached to the physical screen, the one where the
//! sign-in prompt appears, and it is this very program that goes there,
//! started again with arguments naming what it is there for.
//!
//! Three ways, and the whole of what sets them apart is how long the
//! program stays and what it answers. The engine of a session is started
//! under the service's own token, the system account's, simply attached
//! to that session: borrowing the signed-in person's would forbid
//! capturing the secure desktop, and elevation prompts and the sign-in
//! screen would stay black. It is born inside a job object set to kill
//! it along with its parent, so that a service stopping abruptly leaves
//! no orphan engine behind, invisible and impossible to take back in
//! hand. An errand does one thing and answers with its exit code. A
//! helper is left to read for a while and ends by itself.

use std::ffi::{OsStr, OsString, c_void};
use std::io;
use std::marker::PhantomData;
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
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
use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_PROCESS_INFOW, WTSEnumerateProcessesW, WTSFreeMemory,
    WTSGetActiveConsoleSessionId, WTSQuerySessionInformationW, WTSQueryUserToken, WTSUserName,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, DETACHED_PROCESS,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
    GetExitCodeProcess, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    OpenProcessToken, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
    PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use zyr_win32::{Handle, image_of, read_wide, refusal_of, wide};

use crate::{Errand, Launch, Whose};

/// Value Windows returns when no session is attached to the screen.
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// Desktop we aim at: the one carrying the interactive display.
const DESKTOP: &str = "winsta0\\default";

/// Device that swallows what is written to it, and gives nothing back.
const NOTHING: &str = "NUL";

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

/// Whether that program runs in that session.
///
/// Known by its whole path and not by its name alone: another program
/// that happens to be called the same is not the one being looked for.
pub fn runs_in(session: u32, program: &Path) -> io::Result<bool> {
    let name = program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut listed: *mut WTS_PROCESS_INFOW = std::ptr::null_mut();
    let mut count = 0u32;
    // Safe: the list the system makes comes back through the two slots,
    // and is freed below exactly once.
    let asked =
        unsafe { WTSEnumerateProcessesW(WTS_CURRENT_SERVER_HANDLE, 0, 1, &mut listed, &mut count) };
    if asked == 0 {
        return Err(refusal_of("WTSEnumerateProcessesW"));
    }
    if listed.is_null() {
        return Ok(false);
    }
    // Safe: the entries the call above wrote, read before the list is
    // freed.
    let processes = unsafe { std::slice::from_raw_parts(listed, count as usize) };
    let shown = processes
        .iter()
        .filter(|process| process.SessionId == session)
        // Safe: a name the list carries, ended by a nought, alive until
        // the list is freed.
        .filter(|process| unsafe { wide_text(process.pProcessName) }.eq_ignore_ascii_case(&name))
        .any(|process| image_of(process.ProcessId).is_some_and(|image| same_file(&image, program)));
    // Safe: the list the call made, freed once and not read after.
    unsafe { WTSFreeMemory(listed.cast()) };
    Ok(shown)
}

/// Whether somebody is signed in at that session, rather than Windows
/// asking who is there.
pub fn somebody_signed_in(session: u32) -> io::Result<bool> {
    let mut name: *mut u16 = std::ptr::null_mut();
    let mut size = 0u32;
    // Safe: the text the system makes comes back through the two slots,
    // and is freed below exactly once.
    let asked = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session,
            WTSUserName,
            &mut name,
            &mut size,
        )
    };
    if asked == 0 {
        return Err(refusal_of("WTSQuerySessionInformationW"));
    }
    // Safe: the name the call made, ended by a nought, read once, then
    // freed once.
    let signed_in = !unsafe { wide_text(name) }.is_empty();
    unsafe { WTSFreeMemory(name.cast()) };
    Ok(signed_in)
}

/// Who this program runs as, as Windows names them.
///
/// Worth saying from a helper, because it decides what the helper is
/// allowed to see: one under the wrong name reads a desk that looks empty
/// and has no way at all of saying why.
pub fn whoever_this_is() -> String {
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

    let mut spelled = [0u16; 256];
    let mut room = spelled.len() as u32;
    // SAFETY: a buffer of ours, whose length is handed over and written
    // back as the length of what was put in it, the nought counted.
    if unsafe { GetUserNameW(spelled.as_mut_ptr(), &mut room) } == 0 {
        return "a name Windows would not give".to_string();
    }
    String::from_utf16_lossy(&spelled[..room.saturating_sub(1) as usize])
}

/// Whether two paths name the same file, as Windows reads them: with no
/// regard to case.
fn same_file(one: &Path, other: &Path) -> bool {
    one.to_string_lossy().to_lowercase() == other.to_string_lossy().to_lowercase()
}

/// A text the system wrote, ended by a nought.
///
/// # Safety
///
/// `text` is null, or points at letters ending with a nought.
unsafe fn wide_text(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut length = 0;
    // Safe: as the caller vouches, a nought comes before the end.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // Safe: the letters just counted.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) })
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

impl SessionProcess {
    /// The process, as the system numbers it.
    pub fn process(&self) -> u32 {
        self.identifier
    }

    /// Whether it has gone already, asked without waiting.
    pub fn gone(&self) -> bool {
        // Safe: the handle stays valid for as long as this structure, and
        // a wait of no time at all only looks. A wait that fails says
        // nothing about the process, which is then taken as still there.
        unsafe { WaitForSingleObject(self.process.0, 0) == WAIT_OBJECT_0 }
    }

    /// Waits at most that long for it to go by itself, and says with
    /// which code. Nothing when it had to be taken, which letting go of it
    /// does.
    pub fn let_go(self, within: Duration) -> io::Result<Option<u32>> {
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

/// Starts a program in the given session, in its job, its console going
/// to the file its launch names.
pub fn start_in_session(launch: &Launch, session: u32) -> io::Result<SessionProcess> {
    let token = service_token_for(session)?;
    let environment = environment_of(&token)?;
    let job = job_object()?;

    let nothing = inheritable_file(OsStr::new(NOTHING), GENERIC_READ, OPEN_EXISTING)?;
    mark_where_this_run_begins(launch.console, launch.starting);
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
    let log = inheritable_file(launch.console.as_os_str(), FILE_APPEND_DATA, OPEN_ALWAYS)?;
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

/// Starts this program again in the session that owns the screen, to do
/// nothing but what those arguments name.
///
/// Started and never waited for, which is what sets a helper apart from
/// an errand: an errand does one thing and hands back an answer, a helper
/// reads for as long as it lives and says what it read by other means.
/// Nothing here holds on to one either; each ends by itself.
pub fn start_a_helper(arguments: &[String], whose: Whose) -> io::Result<()> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
    let ourselves = std::env::current_exe()?;
    let token = match whose {
        Whose::TheService => service_token_for(session)?,
        Whose::ThePerson => the_person_at(session)?,
    };
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
    // Both handles are let go of at once: this program is not waited for
    // and not ended by anybody, so there is nothing to hold.
    drop(Handle(started.hProcess));
    drop(Handle(started.hThread));
    Ok(())
}

/// Locks the screen, from inside the session that owns it.
///
/// Only a program on the interactive desktop may ask for this, which a
/// service is not, so it is what an errand does there. Windows takes the
/// order and returns before the screen has actually gone: what comes
/// back says the order was accepted, and nothing more is worth waiting
/// for, the person who asked being at the other end of a picture that
/// will show them the lock screen.
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
const CHANGING_HANDS: Duration = Duration::from_millis(1500);
const ASKING_AGAIN: Duration = Duration::from_millis(10);

/// The desktop nobody is locked out of, and the one a session runs on.
const ORDINARY_DESKTOP: &str = "Default";

/// Waits until the screen belongs to another desktop than the ordinary
/// one, which is what locking really means.
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
    Some(read_wide(&name))
}

/// Runs this program in the session that owns the screen, for one short
/// errand, and answers what it cost when it says it did what it went for.
///
/// The service cannot reach into the session that owns the screen, and
/// several things it has to do live there: moving the speakers the
/// person in front of that session hears, locking the screen, holding
/// the desk. They are the same shape, so they are the same code: this
/// program started again with arguments naming the errand, as itself, on
/// the interactive desktop, with the answer read back from its exit code.
///
/// Detached, with no console of its own: nobody is there to read one.
pub fn errand(arguments: &[String], refused: &str) -> io::Result<Errand> {
    match errand_code(arguments, refused)? {
        (0, took) => Ok(took),
        _ => Err(io::Error::other(refused.to_string())),
    }
}

/// The same, for the errand whose answer is more than « it worked ».
///
/// The refusal covers an errand that never came back as well as one that
/// came back saying no: whoever reads it can do nothing different about
/// the two, and one message means one language to choose rather than two.
pub fn errand_code(arguments: &[String], refused: &str) -> io::Result<(u32, Errand)> {
    let session =
        session_on_screen().ok_or_else(|| io::Error::other("no session owns the screen"))?;
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

/// Keeps what the program said before, and marks where its next run
/// begins.
///
/// Written with the product's own journal writer and then let go of, a
/// moment before the program is handed the file: that writer never
/// empties what it opens and cuts the file back from its top once it has
/// grown past reason, which is the rule every other log of the product
/// follows. The line it leaves is what tells one run from the one before
/// it.
///
/// A file that cannot be written to is not a reason to refuse to start a
/// program: it will make its own.
fn mark_where_this_run_begins(console: &Path, starting: &str) {
    if let Ok(kept) = zyr_proto::log::Log::open(console) {
        kept.write(starting);
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

    /// A file of one test's own, in the temporary folder.
    fn scratch(what: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "zyr-system-{what}-{}.log",
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
        let went = started.let_go(Duration::from_secs(10)).unwrap();
        assert_eq!(went, Some(7));
        let said = std::fs::read_to_string(&output).unwrap();
        assert!(said.contains("born here"), "{said:?}");
        let _ = std::fs::remove_file(&output);
    }

    #[test]
    fn a_process_that_went_is_seen_gone_without_being_waited_for() {
        let lingering_output = scratch("lingering");
        let lingering = born(LINGERING, &lingering_output);
        assert!(!lingering.gone(), "a process still running was seen gone");
        let _ = lingering.let_go(Duration::ZERO);

        let quick_output = scratch("quick");
        let quick = born("exit 3", &quick_output);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !quick.gone() {
            assert!(
                Instant::now() < deadline,
                "a process that went was never seen gone"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(quick.let_go(Duration::ZERO).unwrap(), Some(3));
        let _ = std::fs::remove_file(&lingering_output);
        let _ = std::fs::remove_file(&quick_output);
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
        let went = started.let_go(Duration::from_millis(200)).unwrap();
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
        let _ = started.let_go(Duration::ZERO);
        let _ = std::fs::remove_file(&output);
    }
}
