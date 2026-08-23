//! A hook of the system, on a thread that does nothing else.
//!
//! Windows calls a hook back on the thread that installed it, and only
//! while that thread is reading its messages. Two are held here for the
//! length of a session, and neither may be answered late: the one on the
//! keyboard has every keystroke of the whole computer waiting behind it,
//! and a call that takes more than a third of a second has the keystroke
//! handed on as though there had been no hook at all. So each of them
//! gets a thread that reads its messages and does nothing else, and this
//! is that thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

/// The message that asks the thread to lay its hook down and take it
/// straight back up, so that it becomes the newest of the chain.
const AGAIN: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

/// One hook, and the thread it lives on for as long as it is held.
pub struct Held {
    /// That thread by the number the system knows it as, which is what a
    /// message is posted to, to ask it to stop or to start over.
    thread: AtomicU32,
    /// And the thread itself, kept so the end of a session can wait for
    /// it to have really let go before the next one takes hold.
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// How many times the hook has been laid down, the first included.
    laid: AtomicU32,
}

impl Held {
    pub const fn new() -> Self {
        Self {
            thread: AtomicU32::new(0),
            worker: Mutex::new(None),
            laid: AtomicU32::new(0),
        }
    }

    /// Puts the hook on and says whether the system took it, or says
    /// nothing at all while one is already held.
    ///
    /// `put` runs on the new thread and answers the hook as a plain
    /// number, nought meaning refused; `take_back` is handed that number
    /// on that same thread once the wait is over. A hook belongs to the
    /// thread that installed it and may only be given back there, which
    /// is why neither of them is done here.
    pub fn hold(&'static self, put: fn() -> isize, take_back: fn(isize)) -> Option<bool> {
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, PM_NOREMOVE, PeekMessageW,
        };

        if self.thread.load(Ordering::SeqCst) != 0 {
            return None;
        }
        self.laid.store(0, Ordering::SeqCst);
        let (say, hear) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut hook = put();
            if hook != 0 {
                self.laid.fetch_add(1, Ordering::SeqCst);
            }
            let mut message = nothing_yet();
            // A thread has nowhere to receive a message until it has
            // looked for one once, and the end of a session posts it the
            // message that asks it to stop and then waits for it to have
            // stopped. Looked for here, before anyone is told this thread
            // exists, so that message cannot be posted into nothing and
            // waited on for ever.
            //
            // SAFETY: the slot is ours, and nothing is taken from the
            // queue.
            unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
            // SAFETY: no argument.
            let me = unsafe { GetCurrentThreadId() };
            let _ = say.send((me, hook != 0));
            if hook == 0 {
                return;
            }
            // SAFETY: the slot is ours, and no window is named, so this
            // reads what is posted to this thread. It answers 0 for the
            // message that asks it to stop, and -1 for a fault; both end
            // the wait.
            while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
                if message.message == AGAIN {
                    take_back(hook);
                    hook = put();
                    if hook == 0 {
                        return;
                    }
                    self.laid.fetch_add(1, Ordering::SeqCst);
                }
                // SAFETY: the message comes from the call above.
                unsafe { DispatchMessageW(&message) };
            }
            take_back(hook);
        });

        let (thread, taken) = hear.recv().unwrap_or((0, false));
        self.thread.store(thread, Ordering::SeqCst);
        *self.worker.lock().expect("fil d'un crochet du système") = Some(worker);
        Some(taken)
    }

    /// Asks the thread to lay its hook down and take it straight back up,
    /// putting it at the head of the chain again.
    ///
    /// Asked and not done, which is the whole of it. The system calls
    /// these hooks newest first, so a hook laid after ours takes the
    /// keystroke before us and may keep it; there is no way to stay
    /// first, and laying it down again is how that is answered. But the
    /// hook belongs to its thread and may only be taken off there, and a
    /// caller that tore the thread down and built another blocked the
    /// thread that draws while every keystroke of the whole computer
    /// waited on the one being taken down. Victor, on the build that did
    /// that: « ça m'a carrément bloqué le alt tab sur mon propre pc ».
    /// So the thread is told, in one posted message that waits for
    /// nothing, and does it between two of its own messages.
    ///
    /// How many times it has really been done is counted on the far side
    /// and read with `laid`.
    pub fn lay_it_again(&self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

        let thread = self.thread.load(Ordering::SeqCst);
        if thread == 0 {
            return;
        }
        // SAFETY: a thread this program started, told the only way a
        // thread waiting on its messages can be told.
        unsafe { PostThreadMessageW(thread, AGAIN, 0, 0) };
    }

    /// How many times the hook has been laid down, the first included.
    pub fn laid(&self) -> u32 {
        self.laid.load(Ordering::SeqCst)
    }

    /// Takes it off, waits for the thread to have really gone, and says
    /// whether there was one to take off at all.
    pub fn let_go(&self) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

        let thread = self.thread.swap(0, Ordering::SeqCst);
        if thread == 0 {
            return false;
        }
        // SAFETY: a thread this program started, told to stop the only
        // way a thread waiting on its messages can be.
        unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
        if let Some(worker) = self
            .worker
            .lock()
            .expect("fil d'un crochet du système")
            .take()
        {
            let _ = worker.join();
        }
        true
    }
}

/// An empty message, for the slot the system fills in.
fn nothing_yet() -> windows_sys::Win32::UI::WindowsAndMessaging::MSG {
    use windows_sys::Win32::Foundation::POINT;

    windows_sys::Win32::UI::WindowsAndMessaging::MSG {
        hwnd: std::ptr::null_mut(),
        message: 0,
        wParam: 0,
        lParam: 0,
        time: 0,
        pt: POINT { x: 0, y: 0 },
    }
}
