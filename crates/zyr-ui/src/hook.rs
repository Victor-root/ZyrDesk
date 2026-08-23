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

/// One hook, and the thread it lives on for as long as it is held.
pub struct Held {
    /// That thread by the number the system knows it as, which is what a
    /// message is posted to, to ask it to stop.
    thread: AtomicU32,
    /// And the thread itself, kept so the end of a session can wait for
    /// it to have really let go before the next one takes hold.
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Held {
    pub const fn new() -> Self {
        Self {
            thread: AtomicU32::new(0),
            worker: Mutex::new(None),
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
    pub fn hold(&self, put: fn() -> isize, take_back: fn(isize)) -> Option<bool> {
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, PM_NOREMOVE, PeekMessageW,
        };

        if self.thread.load(Ordering::SeqCst) != 0 {
            return None;
        }
        let (say, hear) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let hook = put();
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
