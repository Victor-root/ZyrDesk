//! What the product's named pipes share on Windows: an access list
//! given with the pipe, and patience with a pipe busy for an instant.
//!
//! The service's control channel and the engines' links are both named
//! pipes. Who may open one is written into it when it is made, never
//! checked afterwards, so the list travels with the call that makes it.

use std::ffi::c_void;
use std::io;
use std::time::Duration;

use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::Foundation::{ERROR_PIPE_BUSY, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

/// An access list, held while a pipe instance is created with it,
/// and given back to Windows afterwards.
struct AccessList(PSECURITY_DESCRIPTOR);

impl AccessList {
    /// Builds the list from its text form.
    fn from_text(text: &str) -> io::Result<Self> {
        let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: the text is null-terminated UTF-16 and the output
        // pointer is ours; Windows fills it or says why not.
        let built = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if built == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for AccessList {
    fn drop(&mut self) {
        // SAFETY: the pointer comes from the call above, which
        // allocates it, and is given back exactly once.
        unsafe { LocalFree(self.0) };
    }
}

/// Creates one instance of a pipe that only the accounts named in
/// `access`, an access list in its text form, may open.
pub(crate) fn create(
    options: &ServerOptions,
    address: &str,
    access: &str,
) -> io::Result<NamedPipeServer> {
    let list = AccessList::from_text(access)?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: list.0,
        bInheritHandle: 0,
    };
    // SAFETY: the attributes, and the list they point to, live until
    // the call returns, which is all Windows reads them for.
    unsafe {
        options.create_with_security_attributes_raw(address, &raw mut attributes as *mut c_void)
    }
}

/// How many times a busy pipe is tried before giving up.
///
/// The control channel is busy for the moment between one program
/// being taken in and its next spare instance being made ready. That
/// moment is normally far under a millisecond, so this is about
/// tolerating it happening at all, not about waiting on a pipe that is
/// truly not there. A link, which has a single instance, stays busy
/// once taken: its caller hears so after the same second.
const ATTEMPTS: u32 = 50;

/// How long is left between two tries.
const BETWEEN_TRIES: Duration = Duration::from_millis(20);

/// Opens a pipe, trying again for a moment while it is busy.
pub(crate) async fn open(address: &str) -> io::Result<NamedPipeClient> {
    let mut attempt = 1;
    loop {
        match ClientOptions::new().open(address) {
            Err(e) if attempt < ATTEMPTS && e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                attempt += 1;
                tokio::time::sleep(BETWEEN_TRIES).await;
            }
            opened => return opened,
        }
    }
}
