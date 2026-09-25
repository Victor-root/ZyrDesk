//! What the process and its threads ask of Windows for a session.
//!
//! The screen stays awake and the computer does not sleep while someone
//! watches it; timers wake to the millisecond; the process sees real
//! pixels, which Desktop Duplication requires and without which the
//! desktop's size would be scaled; the process's work comes first on
//! the graphics card, and its threads before those of the programs on
//! the screen, which a game keeps busy (the link and the input, which
//! sit in no multimedia class, would otherwise wait behind it); an
//! NVIDIA card is kept at full speed (see `nvidia`); the desktop's
//! compositor is scheduled as multimedia, so that its images come on
//! time; and the threads that capture sit in the multimedia scheduler's
//! classes. Each refusal is said and costs only what it was for.

use windows::Wdk::Graphics::Direct3D::{
    D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH, D3DKMTSetProcessSchedulingPriorityClass,
};
use windows::Win32::Foundation::{E_ACCESSDENIED, HANDLE};
use windows::Win32::Graphics::Dwm::DwmEnableMMCSS;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, GetCurrentProcess,
    GetPriorityClass, HIGH_PRIORITY_CLASS, PROCESS_CREATION_FLAGS, SetPriorityClass,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::core::PCWSTR;
use zyr_proto::log::Log;

use super::failed;

/// Timer resolution asked for, in milliseconds.
const TIMER_MS: u32 = 1;

/// The process set up for a session, put back as it was when dropped.
///
/// Made on the thread that runs the session, which the state that keeps
/// the screen awake belongs to.
pub struct Tuned {
    timer: bool,
    /// The process's class before the session's, to go back to.
    priority_before: Option<PROCESS_CREATION_FLAGS>,
    compositor: bool,
}

impl Tuned {
    pub fn for_the_session(log: &Log) -> Self {
        // SAFETY: a process-wide setting, made before any window exists.
        if let Err(e) =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
        {
            // Refused when the manifest set it already, which is as good.
            if e.code() != E_ACCESSDENIED {
                log.write(&failed("seeing screens in real pixels", &e));
            }
        }
        // SAFETY: plain flags for the calling thread.
        let before = unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)
        };
        if before.0 == 0 {
            log.write("keeping the screen awake was refused");
        }
        // SAFETY: a plain value, undone once with the same value on drop.
        let timer = unsafe { timeBeginPeriod(TIMER_MS) } == 0;
        if !timer {
            log.write("timers to the millisecond were refused");
        }
        // SAFETY: the pseudo handle of this process and a plain class.
        let status = unsafe {
            D3DKMTSetProcessSchedulingPriorityClass(
                GetCurrentProcess(),
                D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH,
            )
        };
        if status.0 != 0 {
            log.write(&format!(
                "putting this process first on the graphics card was refused: NTSTATUS 0x{:08X}",
                status.0 as u32
            ));
        }
        // Before this process makes its Direct3D device, which is when the
        // driver reads it.
        super::nvidia::full_speed(log);
        // SAFETY: the pseudo handle of this process, which is never closed;
        // nought means the class could not be read.
        let before = unsafe { GetPriorityClass(GetCurrentProcess()) };
        // SAFETY: as above, and a class that needs no privilege.
        let priority_before =
            match unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) } {
                Ok(()) => (before != 0).then_some(PROCESS_CREATION_FLAGS(before)),
                Err(e) => {
                    log.write(&failed(
                        "putting this process's threads before the screen's programs",
                        &e,
                    ));
                    None
                }
            };
        // SAFETY: a plain flag, for as long as this process lives or until
        // taken back on drop.
        let compositor = match unsafe { DwmEnableMMCSS(true) } {
            Ok(()) => true,
            Err(e) => {
                log.write(&failed(
                    "scheduling the desktop's compositor as multimedia",
                    &e,
                ));
                false
            }
        };
        Self {
            timer,
            priority_before,
            compositor,
        }
    }
}

impl Drop for Tuned {
    fn drop(&mut self) {
        // SAFETY: plain flags; ES_CONTINUOUS alone lets the screen and the
        // computer sleep again.
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        if self.timer {
            // SAFETY: the value given to timeBeginPeriod.
            unsafe { timeEndPeriod(TIMER_MS) };
        }
        if let Some(before) = self.priority_before {
            // SAFETY: the pseudo handle of this process, and the class it
            // had; a refusal leaves the class until the process ends.
            let _ = unsafe { SetPriorityClass(GetCurrentProcess(), before) };
        }
        if self.compositor {
            // SAFETY: a plain flag, taken back as it was given.
            let _ = unsafe { DwmEnableMMCSS(false) };
        }
    }
}

/// The calling thread in one of the multimedia scheduler's classes,
/// until dropped.
pub(super) struct ThreadTask {
    handle: Option<HANDLE>,
}

impl ThreadTask {
    /// Joins the class `task` ("Capture", "Pro Audio"...), saying so if
    /// Windows refuses.
    pub(super) fn join(task: PCWSTR, log: &Log) -> Self {
        let mut index = 0;
        // SAFETY: a task name Windows knows and a plain out value.
        let handle = match unsafe { AvSetMmThreadCharacteristicsW(task, &mut index) } {
            Ok(handle) => Some(handle),
            Err(e) => {
                // SAFETY: the name is one of ours, a terminated constant.
                let name = unsafe { task.to_string() }.unwrap_or_default();
                log.write(&failed(&format!("joining the multimedia class {name}"), &e));
                None
            }
        };
        Self { handle }
    }
}

impl Drop for ThreadTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // SAFETY: the handle AvSetMmThreadCharacteristicsW gave this
            // same thread.
            let _ = unsafe { AvRevertMmThreadCharacteristics(handle) };
        }
    }
}
