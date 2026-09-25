//! The program itself: what it keeps, what is waiting for it, and the
//! thread that owns its windows.
//!
//! It was the last service the toolkit provided. It comes down to three
//! things.
//!
//! **A handle.** What every part of the program passes from hand to hand
//! to find what the program keeps. A single thing behind it, shared:
//! copying it copies nothing.
//!
//! **A mailbox.** A window that shows nothing, whose only job is to carry
//! work to the thread that owns the others. A window and not a message to
//! the thread itself, and that is not a detail: Windows throws away the
//! messages addressed to a thread while it is moving a window, and it is
//! precisely while the window is being moved that a session's picture has
//! to follow it.
//!
//! **A loop.** The main thread takes the system's messages and hands them
//! to whoever they are addressed to, for as long as the program runs.

// A message loop and a mailbox belong to the system, and this product
// only runs on Windows. Elsewhere, each one answers what a program
// without a window answers, so that everything else stays compiled and
// checked.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::floating::Floating;
use crate::tray::Shown;

/// The program, as every part of it holds it.
///
/// What a toolkit called a "handle". What is behind it is what the
/// product keeps from one session to the next: what the floating button
/// follows, and what the icon by the clock said last time.
#[derive(Clone)]
pub struct App(Arc<Inner>);

#[derive(Default)]
struct Inner {
    floating: Floating,
    shown: Shown,
}

impl App {
    /// The program at its very first moment, before anything
    /// runs.
    pub fn new() -> Self {
        App(Arc::new(Inner::default()))
    }

    pub fn floating(&self) -> &Floating {
        &self.0.floating
    }

    pub fn shown(&self) -> &Shown {
        &self.0.shown
    }

    /// Has this work done by the thread that owns the windows.
    ///
    /// A window belongs to the thread that made it: moving it, resizing
    /// it or giving it a frame from another thread does not work, or
    /// works until the day it stops working.
    pub fn run_on_main_thread(&self, work: impl FnOnce() + Send + 'static) -> Result<(), String> {
        post(Box::new(work))
    }
}

/// What is waiting for the main thread.
type Work = Box<dyn FnOnce() + Send>;

static TO_DO: Mutex<Vec<Work>> = Mutex::new(Vec::new());

/// The mailbox, as the system knows it.
static MAILBOX: AtomicIsize = AtomicIsize::new(0);

/// The message that tells it there is mail, and the one that tells it
/// it is over.
#[cfg(windows)]
const WORK_WAITING: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;
#[cfg(windows)]
const FINISH: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[cfg(windows)]
fn post(work: Work) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    let mailbox = MAILBOX.load(Ordering::Relaxed) as HWND;
    if mailbox.is_null() {
        return Err("le fil principal n'a pas encore de boîte aux lettres".to_string());
    }
    TO_DO.lock().expect("travail du fil principal").push(work);
    // SAFETY: a window of ours, to which we post a message that
    // is ours alone.
    if unsafe { PostMessageW(mailbox, WORK_WAITING, 0, 0) } == 0 {
        return Err("le fil principal ne prend plus de courrier".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn post(_work: Work) -> Result<(), String> {
    Err("il n'y a pas de fil de fenêtres hors de Windows".to_string())
}

/// Opens the mailbox. To be done on the main thread, before everything
/// else.
#[cfg(windows)]
pub fn open_the_mailbox() -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, HWND_MESSAGE, RegisterClassW, WNDCLASSW,
    };

    let class_name: Vec<u16> = "ZyrDeskCourrier".encode_utf16().chain(Some(0)).collect();
    // SAFETY: a class declared once and a window built on it, on the
    // thread that will pump its messages. It shows nothing: a window
    // whose parent is this one is never drawn.
    let mailbox = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(receives),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if mailbox.is_null() {
        return Err("le fil principal n'a pas pu ouvrir de boîte aux lettres".to_string());
    }
    MAILBOX.store(mailbox as isize, Ordering::Relaxed);
    Ok(())
}

#[cfg(not(windows))]
pub fn open_the_mailbox() -> Result<(), String> {
    Err("il n'y a pas de fil de fenêtres hors de Windows".to_string())
}

/// SAFETY: called by the system on the thread that made this window,
/// with the arguments it documents.
#[cfg(windows)]
unsafe extern "system" fn receives(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, PostQuitMessage};

    match message {
        WORK_WAITING => {
            // Taken out of the lock before being done: a job that
            // carried another one would take again a lock that is
            // still held, and the main thread would stop for good.
            let works = std::mem::take(&mut *TO_DO.lock().expect("travail du fil principal"));
            for work in works {
                work();
            }
            0
        }
        FINISH => {
            // SAFETY: nothing but the word that stops the loop.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: the system's answer to everything not answered here.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// Takes the system's messages and hands them to whoever they are
/// addressed to, for as long as the program runs.
#[cfg(windows)]
pub fn run() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, TranslateMessage,
    };

    // SAFETY: a block of ours, which the system fills in on every
    // round.
    let mut message: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: the block above, and nothing else. A zero says the
    // program is stopping, a minus one that the queue is broken: either
    // way there is nothing left to wait for.
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: the message that has just arrived, translated and
        // then handed on.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(not(windows))]
pub fn run() {}

/// Stops the program.
///
/// Asked of the main thread, the only one able to stop its own loop.
#[cfg(windows)]
pub fn quit() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    let mailbox = MAILBOX.load(Ordering::Relaxed) as HWND;
    if !mailbox.is_null() {
        // SAFETY: a window of ours, to which we post a message that
        // is ours alone.
        unsafe { PostMessageW(mailbox, FINISH, 0, 0) };
    }
}

#[cfg(not(windows))]
pub fn quit() {}

/* ---- One ZyrDesk at a time ------------------------------------------ */

/// The lock that says a ZyrDesk is already running, held for as long as
/// it runs.
static LOCK: AtomicIsize = AtomicIsize::new(0);

/// The lock's name. Local and not global: it is one window per signed-in
/// person, not one per machine.
const ONLY_ONE: &str = r"Local\ZyrDesk";

/// Whether a ZyrDesk is already running, and if so asks it to show
/// itself.
///
/// Two ZyrDesks at once are two floating buttons on the same session.
/// Whoever starts a second one wanted to see their window again: that
/// is what they get.
#[cfg(windows)]
pub fn already_open() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = ONLY_ONE.encode_utf16().chain(Some(0)).collect();
    // SAFETY: a name that outlives the call, and a lock held until the
    // end of the program, which gives it back as it stops.
    let (lock, already) = unsafe {
        let lock = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        (lock, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    if lock.is_null() {
        // Nothing answers: better to start than not to start.
        return false;
    }
    if !already {
        LOCK.store(lock as isize, Ordering::Relaxed);
        return false;
    }
    // SAFETY: a lock that this call has just handed back, closed once.
    unsafe { CloseHandle(lock) };
    crate::main_window::show_the_one_running();
    true
}

#[cfg(not(windows))]
pub fn already_open() -> bool {
    false
}

/* ---- The screens ---------------------------------------------------- */

/// Tells the system that this program counts in real pixels, on every
/// screen.
///
/// Before a single window exists, because that is when the system
/// decides: without it, the system would itself enlarge what we already
/// draw at the right size, and everything would be blurred.
#[cfg(windows)]
pub fn count_in_real_pixels() {
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };

    // SAFETY: nothing but a word to the system about this program. A
    // refusal means it has already been said, which comes to the same
    // thing.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

#[cfg(not(windows))]
pub fn count_in_real_pixels() {}

/* ---- What runs without blocking the window thread -------------------- */

/// The task engine, made once and kept.
///
/// Everything that talks to the service goes through a pipe, and a pipe
/// is waited on: none of that may stop the thread that draws.
static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("le moteur des tâches n'a pas pu démarrer")
    })
}

/// Starts a task, which will live its own life.
pub fn spawn<F>(task: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(task)
}

/// Starts what really waits on a thread where waiting is allowed.
pub fn spawn_blocking<F, R>(work: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    runtime().spawn_blocking(work)
}

/// Waits for a task from a thread that is not one.
pub fn block_on<F: std::future::Future>(task: F) -> F::Output {
    runtime().block_on(task)
}
