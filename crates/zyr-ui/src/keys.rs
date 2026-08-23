//! The keys Windows keeps for itself, handed to the far computer.
//!
//! Alt+Tab, Alt+Maj+Tab, Alt+Échap and Ctrl+Échap never reach the window
//! they are typed at: the system acts on them itself, before any program
//! sees them. A remote desktop wants the opposite.
//!
//! Claimed the way the system offers them to be claimed, which is the
//! same way this product already claims its own combinations
//! (`shortcuts.rs`): a combination is registered, and from then on the
//! system hands it to the program that asked rather than acting on it.
//! One claim, honoured every time, and the switcher of this computer
//! never opens at all.
//!
//! It was done the other way first, by stepping in front of every
//! keystroke of the whole computer and swallowing these when they went
//! past. That is a race with the system's own handling of the very same
//! keys and it was lost about one time in four, which is what « Alt+Tab
//! works until I touch the floating button » really was: nothing to do
//! with the button, only with what else was going on at that moment. The
//! journal caught it in the end, the release of a Tab arriving with no
//! press to match it. A claim is not a race.
//!
//! The client engine has an option of its own for all this, asked for
//! once and taken back out (D32): it cannot use it, its window being
//! carried inside ours, and the way it goes about it swallows Alt and
//! Control whole, which costs this product every shortcut it has.
//!
//! Held only while a session really has the keyboard, and given back the
//! moment it does not: a combination held is held against the whole
//! computer, so the front leaving this program hands them all back before
//! the person can type one.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The thread that holds the combinations, by the number the system knows
/// it as: that is what a message is posted to.
static BOARD: Mutex<Option<u32>> = Mutex::new(None);

/// And the thread itself, so the end of a session can wait for it to have
/// really given the combinations back before the next one asks for them.
static WORKER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// Whether that thread is holding them right now, so it is only ever told
/// when the answer changes.
static HOLDING: AtomicBool = AtomicBool::new(false);

/// How many combinations have been carried to the far computer, and what
/// the journal last said about it.
///
/// Counted here and written down by the session's watch a moment later:
/// the journal is a lock and a file flushed to disk, and this thread is
/// the one the system waits on for these keys.
static CARRIED: AtomicU32 = AtomicU32::new(0);
static TOLD: AtomicU32 = AtomicU32::new(0);

/// Which modifiers were physically down the last time they were looked
/// at, so only the ones that have come up since are released at the far
/// computer; see `no_key_left_down`.
static WAS_DOWN: AtomicU32 = AtomicU32::new(0);

/// What this thread is asked to do, past every message the system has
/// names for.
const TAKE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_USER + 1;
const GIVE_BACK: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_USER + 2;

/// The combinations, in the order their numbers are given out: what is
/// held down with them, which key, and where that key sits on a keyboard.
///
/// Alt+Maj+Tab is its own claim and not a variation of the one above it:
/// the system hands over a combination exactly as it was asked for, so
/// the one with Maj has to be asked for as well or it goes on switching
/// windows here.
const COMBINATIONS: [(u32, u16, u16); 4] = [
    (MOD_ALT, VK_TAB, 0x0F),
    (MOD_ALT | MOD_SHIFT, VK_TAB, 0x0F),
    (MOD_ALT, VK_ESCAPE, 0x01),
    (MOD_CONTROL, VK_ESCAPE, 0x01),
];

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, VK_ESCAPE, VK_TAB,
};

/// Starts the thread that holds them, for the length of a session.
///
/// A thread of its own because a combination is handed to the thread that
/// asked for it and nowhere else, and this one must be free to answer at
/// once: hung on the thread that draws, every message that thread is
/// busy with would be a combination answered late.
pub fn hold() {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PM_NOREMOVE, PeekMessageW, WM_HOTKEY,
    };

    if BOARD.lock().expect("fil des touches système").is_some() {
        return;
    }
    let (say, hear) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut message = nothing_yet();
        // A thread has nowhere to receive a message until it has looked
        // for one once, and everything below is asked of it by message.
        // Looked for here, before anyone is told this thread exists.
        //
        // SAFETY: the slot is ours, and nothing is taken from the queue.
        unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
        // SAFETY: no argument, and the answer is this thread's own name.
        let _ = say.send(unsafe { GetCurrentThreadId() });

        let mut held = false;
        // SAFETY: the slot is ours, and no window means every message of
        // this thread, which is where the combinations land. It answers 0
        // for the message that asks it to stop and -1 for a fault.
        while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            match message.message {
                TAKE if !held => held = take_them(),
                GIVE_BACK if held => {
                    give_them_back();
                    held = false;
                }
                WM_HOTKEY => carry(message.wParam),
                // Nothing of this thread's own is a window's message, but
                // the system sends a few to every thread that waits, and
                // they belong to it and not to us.
                _ => {
                    // SAFETY: the message comes from the call above.
                    unsafe { DispatchMessageW(&message) };
                }
            }
        }
        if held {
            give_them_back();
        }
    });

    let thread = hear.recv().ok();
    *BOARD.lock().expect("fil des touches système") = thread;
    *WORKER.lock().expect("fil des touches système") = Some(worker);
    HOLDING.store(false, Ordering::SeqCst);
}

/// Stops it, the session being over, and waits for the combinations to
/// have really been given back.
pub fn let_go() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

    let Some(thread) = BOARD.lock().expect("fil des touches système").take() else {
        return;
    };
    tell();
    // SAFETY: the name is one this program gave itself, and the message
    // carries nothing.
    unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
    if let Some(worker) = WORKER.lock().expect("fil des touches système").take() {
        let _ = worker.join();
    }
    CARRIED.store(0, Ordering::SeqCst);
    TOLD.store(0, Ordering::SeqCst);
    HOLDING.store(false, Ordering::SeqCst);
    WAS_DOWN.store(0, Ordering::SeqCst);
}

/// Takes the combinations when the session has the keyboard, and hands
/// them back the moment it does not.
///
/// Asked at every turn of the session's watch and at every settling of
/// the front, which is the moment that matters: a combination held is
/// held against the whole computer, so the instant the front leaves this
/// program they have to be somebody else's again, and a second's wait
/// would be a second in which this person's own Alt+Tab did nothing they
/// expected.
pub fn follow() {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

    let Some(thread) = *BOARD.lock().expect("fil des touches système") else {
        return;
    };
    let wanted = the_session_has_the_keyboard();
    if HOLDING.swap(wanted, Ordering::SeqCst) == wanted {
        return;
    }
    // SAFETY: the name is one this program gave itself, and the message
    // carries nothing.
    unsafe { PostThreadMessageW(thread, if wanted { TAKE } else { GIVE_BACK }, 0, 0) };
}

/// Whether a session is on screen with the keyboard really in it.
///
/// Three answers and all three have to be yes: there is a picture, the
/// front belongs to this session, and the picture is the window this
/// program's keyboard goes to. Anything else means the person is doing
/// something outside the session, and their own Alt+Tab is theirs.
///
/// « This session » and not « this program »: the front reads as the
/// player's as readily as ours, the engine's program keeping a window of
/// its own beside the picture, and the two mean the same thing here.
fn the_session_has_the_keyboard() -> bool {
    crate::picture::the_engines_window().is_some()
        && crate::picture::who_holds_the_front() != crate::picture::Front::Elsewhere
        && crate::picture::the_keyboard_is_at_the_picture()
}

/// Asks the system for every combination, and says whether any was given.
fn take_them() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey;

    let mut taken = 0;
    let mut refused = 0;
    for (rank, (modifiers, key, _)) in COMBINATIONS.iter().enumerate() {
        // SAFETY: no window, so the combination belongs to this thread,
        // and the number is ours and unique within it.
        if unsafe {
            RegisterHotKey(
                std::ptr::null_mut(),
                rank as i32,
                *modifiers,
                u32::from(*key),
            )
        } != 0
        {
            taken += 1;
        } else {
            refused += 1;
        }
    }
    crate::journal::note(&format!(
        "touches système reprises pour la session : {taken} combinaison(s) tenue(s), \
         {refused} refusée(s) par Windows"
    ));
    taken > 0
}

/// Hands them all back.
fn give_them_back() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;

    for rank in 0..COMBINATIONS.len() {
        // SAFETY: the number is one this thread registered above, and a
        // number never registered is refused and costs nothing.
        unsafe { UnregisterHotKey(std::ptr::null_mut(), rank as i32) };
    }
    crate::journal::note("touches système rendues à cet ordinateur");
}

/// Types the combination the system just handed over at the picture.
///
/// Told rather than typed back out. A keystroke sent out again would be
/// read by the system first, exactly as the one just claimed was, and
/// this thread would be handed its own combination for as long as the
/// session lasted. A message goes to the one window it names.
///
/// Both halves of the key at once: what the system hands over is the
/// combination, once, and not a press followed by a release.
fn carry(which: usize) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostMessageW, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    let Some((_, key, place)) = COMBINATIONS.get(which) else {
        return;
    };
    let Some(engine) = crate::picture::the_engines_window() else {
        return;
    };

    // SAFETY: a key is named and its state is read; nothing is written.
    let alt = unsafe { GetAsyncKeyState(i32::from(VK_MENU)) } as u16 & 0x8000 != 0;
    // What a window is told about a key, in the shape it expects: one
    // press, where the key sits, and whether Alt was down with it. That
    // last one is also what makes it the system's own kind of keystroke
    // rather than an ordinary one.
    let mut about = 1 | ((*place as isize) << 16);
    if alt {
        about |= 1 << 29;
    }
    let (down, up) = if alt {
        (WM_SYSKEYDOWN, WM_SYSKEYUP)
    } else {
        (WM_KEYDOWN, WM_KEYUP)
    };
    // SAFETY: a window this program took in hand, told about a key in the
    // system's own words. Posted rather than sent: nothing here may wait
    // on another program.
    unsafe {
        PostMessageW(engine, down, usize::from(*key), about);
        PostMessageW(engine, up, usize::from(*key), about | (1 << 30) | (1 << 31));
    }
    CARRIED.fetch_add(1, Ordering::SeqCst);
}

/// Releases, at the picture, every modifier no finger is holding.
///
/// A modifier goes down here, something takes the keyboard off the
/// picture before it comes back up, and the release never reaches the far
/// computer: it goes on believing Alt is held, and every letter typed
/// afterwards arrives there as Alt and a letter, which does nothing and
/// looks exactly like a dead keyboard. The engine says so at the end of
/// every such session, in its own log, in three words: « Raising 1 keys ».
///
/// Read from the fingers rather than from anything remembered: what is
/// physically down is the one truth here, and a release sent for a key
/// that is genuinely held would be a worse fault than the one being
/// fixed. A release the far computer did not need costs nothing, its own
/// engine dropping any that says what it already believes.
///
/// Only for the ones that have just come up, which is what this is about
/// and what keeps it quiet: sending all eight at every turn would be
/// eight messages a second for the length of a session, saying nothing
/// almost every time.
pub fn no_key_left_down() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
        VK_RSHIFT, VK_RWIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_KEYUP};

    let Some(engine) = crate::picture::the_engines_window() else {
        return;
    };
    // Each with where it sits and whether it is one of the pair that live
    // off the far end of the keyboard, which is how the two sides of a
    // modifier are told apart.
    const MODIFIERS: [(u16, u32, bool); 8] = [
        (VK_LSHIFT, 0x2A, false),
        (VK_RSHIFT, 0x36, false),
        (VK_LCONTROL, 0x1D, false),
        (VK_RCONTROL, 0x1D, true),
        (VK_LMENU, 0x38, false),
        (VK_RMENU, 0x38, true),
        (VK_LWIN, 0x5B, true),
        (VK_RWIN, 0x5C, true),
    ];
    let mut now = 0u32;
    for (rank, (key, _, _)) in MODIFIERS.iter().enumerate() {
        // SAFETY: a key is named and its state is read; nothing is
        // written.
        if unsafe { GetAsyncKeyState(i32::from(*key)) } as u16 & 0x8000 != 0 {
            now |= 1 << rank;
        }
    }
    let just_up = WAS_DOWN.swap(now, Ordering::SeqCst) & !now;
    for (rank, (key, place, far)) in MODIFIERS.into_iter().enumerate() {
        if just_up & (1 << rank) == 0 {
            continue;
        }
        let mut about: isize = 1 | ((place as isize) << 16) | (1 << 30) | (1 << 31);
        if far {
            about |= 1 << 24;
        }
        // SAFETY: a window this program took in hand, told about a key in
        // the system's own words.
        unsafe { PostMessageW(engine, WM_KEYUP, usize::from(key), about) };
    }
}

/// Says what has been carried since the last time it was asked.
///
/// Called from the watch that follows a session, once a second, which is
/// where the journal may be written to without holding up a keystroke.
pub fn tell() {
    let carried = CARRIED.load(Ordering::SeqCst);
    if TOLD.swap(carried, Ordering::SeqCst) == carried {
        return;
    }
    crate::journal::note(&format!(
        "touches système portées à la session plutôt qu'à cet ordinateur : {carried} en tout"
    ));
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
