//! The one keystroke Windows keeps for itself.
//!
//! Ctrl+Alt+Suppr is the system's own at both ends of a session. The
//! computer watching never sees it, its Windows taking it before any
//! program does; and the computer being watched cannot be made to feel
//! it by an engine, because the way an engine types is exactly the way
//! Windows refuses for this one. That refusal is the point of the
//! combination: it is what tells a person that the screen in front of
//! them is really Windows and not something dressed up as it.
//!
//! There is one door, `SendSAS`, and Windows names precisely who may go
//! through it: a program **running as a service**, or one carrying a
//! `uiAccess` manifest, signed, and installed under Program Files or
//! System32. This service is the first of those two, so it is the
//! service itself that presses, in its own process.
//!
//! That is the correction of a first attempt that did not work. The
//! press was handed to a short errand started in the session that owns
//! the screen, the way the engine is tapped on the shoulder: a process
//! running as LocalSystem but not a service and carrying no manifest,
//! which is neither of the two Windows accepts. Everything reported
//! success, because `SendSAS` returns nothing at all and cannot refuse,
//! and nothing happened on the screen.
//!
//! A service sits in session 0, which has no screen of its own, and the
//! sequence lands on the session that has one. That is the whole reason
//! the policy below offers « services » as an answer: a service that
//! could only wake its own session could wake nobody.
//!
//! # Saying why, since Windows will not
//!
//! `SendSAS` returns void. Whether the sequence was really let through
//! is decided by one registry value, and nothing anywhere says so
//! afterwards. So what is in force is read and written down at the
//! moment of pressing, beside the console session it should land on:
//! without that line, a press that does nothing and a press that was
//! never allowed read exactly alike.

use std::io;

use zyr_proto::log::Log;

/// Where Windows keeps the one setting that decides whether a program
/// may press Ctrl+Alt+Suppr on this computer's behalf.
const POLICY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\System";
const POLICY_VALUE: &str = "SoftwareSASGeneration";

/// Services may press it. Two other values exist, for accessibility
/// tools and for both at once; ours is a service, and taking more than
/// that would be taking what nobody asked for.
const BY_SERVICES: u32 = 1;

/// Lets this service press Ctrl+Alt+Suppr for a session.
///
/// Laid where the service is registered **and at every start of it**.
/// Registration alone is never enough, and this product has learned it
/// three times already: a computer whose service was registered before
/// this existed would otherwise go without for ever, and nothing would
/// try again or even say so. The firewall rules, the right to start the
/// service and the virtual screen all sit beside this for the same
/// reason.
///
/// A failure is written down and never raised. A computer whose policies
/// are held by an employer is a computer where this will not take, and
/// it must still work in every other way; what it loses is one menu
/// entry, and the journal says which.
pub fn let_it_be_pressed(log: Option<&Log>) {
    let said = match set_the_policy(Some(BY_SERVICES)) {
        Ok(()) => "Ctrl+Alt+Suppr may now be pressed for a session".to_string(),
        Err(code) => format!(
            "Ctrl+Alt+Suppr cannot be pressed for a session on this computer, its policy would \
             not take (code {code})"
        ),
    };
    if let Some(log) = log {
        log.write(&said);
    }
}

/// Takes that permission away again.
///
/// A machine that no longer runs this product has no reason to go on
/// letting a service press what Windows keeps for itself.
pub fn forget_it() {
    let _ = set_the_policy(None);
}

/// Presses it on this computer, for the far one.
///
/// Everything that decides the outcome is written down first, because
/// the call itself decides nothing and reports nothing.
pub fn press(log: &Log) -> io::Result<()> {
    log.write(&format!(
        "Ctrl+Alt+Suppr: policy {}, this service is in session {}, the screen is on session {}",
        match what_the_policy_says() {
            Some(value) => value.to_string(),
            None => "unset".to_string(),
        },
        said(our_session()),
        said(crate::session::session_on_screen()),
    ));

    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let name = wide("sas.dll");
    // SAFETY: a library of the system's own, named as a wide string that
    // outlives the call. It is deliberately not freed: this service goes
    // on running and will press again, and a library of the system left
    // loaded costs one handle.
    let found = unsafe {
        let library = LoadLibraryW(name.as_ptr());
        if library.is_null() {
            return Err(io::Error::other(
                "sas.dll est introuvable sur cet ordinateur",
            ));
        }
        GetProcAddress(library, c"SendSAS".as_ptr().cast())
    };
    let Some(send) = found else {
        return Err(io::Error::other("cet ordinateur n'a pas de SendSAS"));
    };
    // SAFETY: the one function that library exports, whose shape is
    // `VOID SendSAS(BOOL)`, taken from the system's own documentation.
    let send: unsafe extern "system" fn(i32) = unsafe { std::mem::transmute(send) };
    // Nought is « not as the signed-in person », which is what a service
    // is. One is for a program running as that person, which this never
    // is.
    unsafe { send(0) };
    Ok(())
}

fn said(session: Option<u32>) -> String {
    match session {
        Some(session) => session.to_string(),
        None => "none".to_string(),
    }
}

/// The session this program runs in, which for a service is nought.
///
/// Read rather than assumed, because it is half the diagnosis: a press
/// made from anywhere but a service is a press Windows throws away
/// without a word.
fn our_session() -> Option<u32> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    let mut session = 0u32;
    // SAFETY: our own process, and the slot written into is a local.
    let asked = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    (asked != 0).then_some(session)
}

/// What the policy really says right now, `None` when it says nothing.
fn what_the_policy_says() -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
    };

    let key = wide(POLICY_KEY);
    let value = wide(POLICY_VALUE);
    let mut found = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: both names outlive the call, and the four bytes written
    // into are those of the number beside them.
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut found).cast::<std::ffi::c_void>(),
            &mut size,
        )
    };
    (code == 0).then_some(found)
}

/// Writes that one policy value, or takes it away.
///
/// The value alone is removed and never the key. That key is Windows'
/// own and holds settings this product never touched; taking it away
/// would take theirs along with ours.
fn set_the_policy(to: Option<u32>) -> Result<(), u32> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
    };

    let key = wide(POLICY_KEY);
    let value = wide(POLICY_VALUE);
    let mut open: HKEY = std::ptr::null_mut();
    // SAFETY: both names outlive the call, and the slot for the key it
    // hands back is ours.
    let code = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut open,
            std::ptr::null_mut(),
        )
    };
    if code != 0 {
        return Err(code);
    }
    let code = match to {
        // SAFETY: the key is open, the name outlives the call, and the
        // four bytes handed over are those of the number beside them.
        Some(number) => unsafe {
            RegSetValueExW(
                open,
                value.as_ptr(),
                0,
                REG_DWORD,
                (&raw const number).cast::<u8>(),
                size_of::<u32>() as u32,
            )
        },
        // SAFETY: the key is open and the name outlives the call.
        None => unsafe { RegDeleteValueW(open, value.as_ptr()) },
    };
    // SAFETY: a key this function opened, closed exactly once.
    unsafe { RegCloseKey(open) };
    if code != 0 { Err(code) } else { Ok(()) }
}

/// Zero-terminated string, the way Windows expects them.
fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect()
}
