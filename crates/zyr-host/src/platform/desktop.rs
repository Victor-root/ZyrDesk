//! The desktop that receives input: the person's own, the sign-in
//! screen's, or an administrator prompt's.
//!
//! A thread captures and injects on the desktop it is attached to, and
//! Windows switches desktops under it: at Ctrl+Alt+Del, at the lock
//! screen, at every administrator prompt. Only the system's account may
//! attach to the secure ones, which is why the engine runs as it. A
//! thread follows the desktop receiving input whenever what it does
//! there is refused.

use windows::Win32::Foundation::{GENERIC_ALL, HANDLE};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, GetUserObjectInformationW, HDESK,
    OpenInputDesktop, SetThreadDesktop, UOI_NAME,
};
use zyr_proto::log::Log;

use super::failed;

/// The desktop the calling thread is attached to, once it followed the
/// input.
///
/// Belongs to one thread: made, used and dropped there.
pub(super) struct InputDesktop {
    /// Open for as long as the thread is attached to it: Windows does
    /// not let a desktop in use be closed.
    current: Option<HDESK>,
    name: String,
    /// The last refusal said, so that one that repeats is said once.
    said: Option<String>,
}

impl InputDesktop {
    pub(super) fn new() -> Self {
        Self {
            current: None,
            name: String::new(),
            said: None,
        }
    }

    /// Follows the input desktop, writing a refusal to the log unless it
    /// is the one written last.
    pub(super) fn follow_saying(&mut self, log: &Log) {
        match self.follow() {
            Ok(()) => self.said = None,
            Err(e) => {
                if self.said.as_deref() != Some(e.as_str()) {
                    log.write(&e);
                    self.said = Some(e);
                }
            }
        }
    }

    /// The name of the desktop followed last ("Default", "Winlogon"...).
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// Attaches the calling thread to the desktop receiving input now.
    pub(super) fn follow(&mut self) -> Result<(), String> {
        // SAFETY: plain values; the handle comes back owned, and is
        // closed below or kept for as long as the thread uses it.
        let desktop = unsafe {
            OpenInputDesktop(
                DESKTOP_CONTROL_FLAGS(0),
                false,
                DESKTOP_ACCESS_FLAGS(GENERIC_ALL.0),
            )
        }
        .map_err(|e| failed("opening the input desktop", &e))?;
        let name = name_of(desktop);
        // SAFETY: a desktop handle just opened; the thread has no window
        // and no hook, which SetThreadDesktop requires.
        if let Err(e) = unsafe { SetThreadDesktop(desktop) } {
            // SAFETY: the handle opened above, used by nobody.
            let _ = unsafe { CloseDesktop(desktop) };
            return Err(failed(
                &format!("attaching to the input desktop {name}"),
                &e,
            ));
        }
        if let Some(before) = self.current.replace(desktop) {
            // SAFETY: the desktop the thread just left, opened by us.
            let _ = unsafe { CloseDesktop(before) };
        }
        self.name = name;
        Ok(())
    }
}

impl Drop for InputDesktop {
    fn drop(&mut self) {
        if let Some(desktop) = self.current.take() {
            // SAFETY: a handle opened by us. Windows refuses to close it
            // while the thread is still attached, and closes it with the
            // process then.
            let _ = unsafe { CloseDesktop(desktop) };
        }
    }
}

/// A desktop's name, for the log.
fn name_of(desktop: HDESK) -> String {
    let mut name = [0u16; 128];
    let mut needed = 0u32;
    // SAFETY: the buffer is ours and its size in bytes is what is given.
    let read = unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            std::mem::size_of_val(&name) as u32,
            Some(&mut needed),
        )
    };
    if read.is_err() {
        return "(unnamed)".to_string();
    }
    let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    String::from_utf16_lossy(&name[..end])
}
