//! The keys Windows keeps for itself, handed to the far computer.
//!
//! Alt+Tab, Alt+Échap, Ctrl+Échap and the Windows key never reach the
//! window they are typed at: the system acts on them itself, before any
//! program sees them. A remote desktop wants the opposite, and the client
//! engine has an option for exactly that, asked for by every session
//! (`--capture-system-keys always`, D28). It cannot act on it here.
//!
//! Why it cannot is worth having in writing, because it looks like a
//! defect of the engine and is not one. The engine decides it holds the
//! keyboard by comparing its own window with the window the system calls
//! the front one. Its window is carried inside ours for the whole of a
//! session, and a carried window is a child window; the system gives the
//! front to the head of a family and never to a member of it. So the
//! engine's answer is no from the first moment it is asked, it lets go of
//! those keys for the rest of the session, and nothing it offers brings
//! them back.
//!
//! The window the system does call the front one is ours. So the program
//! that can take those keys is this one, and it takes them and passes
//! them on unchanged. Nothing of this is in the engine, and nothing of
//! this asks the engine for anything it does not already do: what it
//! receives is an ordinary keystroke at its own window, which it forwards
//! like any other.
//!
//! Taken only while a session really has the keyboard, which is the whole
//! of the safety here: at every other moment, on every other key, and for
//! every keystroke this program itself sends, the system is left to do
//! exactly what it always does.

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

/// The hook, while it is installed, as a plain number: a hook handle is a
/// raw pointer, which does not travel between threads on its own.
static HOOKED: AtomicIsize = AtomicIsize::new(0);

/// Which of the keys below this program is currently holding down on the
/// far computer's behalf, one bit each.
///
/// A key taken on the way down is taken on the way up as well, whatever
/// has happened in between. Left to the ordinary answer, a session that
/// ends between the two hands the system a Windows key released that it
/// never saw pressed, which is precisely what opens the Start menu.
static HELD: AtomicU32 = AtomicU32::new(0);

/// The last key handed to the session, and how many have been handed
/// over in all, left for `tell` to read a moment later.
///
/// Written here and read there, rather than written to the journal on
/// the spot. Writing to the journal is a lock and a file flushed to
/// disk, and the road it would be put on is the one every keystroke of
/// every program on this computer travels, held up until this program
/// returns; a system that finds it slow takes the whole thing off
/// without a word. Two numbers cost nothing and say the same thing one
/// second later.
static SAID: AtomicU32 = AtomicU32::new(0);

/// How many of them there have been, for the same reason.
static TAKEN: AtomicU32 = AtomicU32::new(0);

/// What the journal last said about the two above.
static TOLD: AtomicU32 = AtomicU32::new(0);

/// Takes the system's keys for as long as a session lasts.
///
/// Asked from the thread that draws, and only from there: a hook of this
/// kind is called back on the thread that installed it, and that thread
/// has to be one that reads its messages, which is that one.
pub fn hold() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_KEYBOARD_LL};

    if HOOKED.load(Ordering::Relaxed) != 0 {
        return;
    }
    // SAFETY: no argument names this program's own module, the callback
    // is a plain function of it, and the hook is asked for on this
    // thread alone.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(seen),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    };
    HOOKED.store(hook as isize, Ordering::Relaxed);
    crate::journal::note(if hook.is_null() {
        "touches système non reprises : Windows a refusé le crochet clavier"
    } else {
        "touches système reprises pour la session : Alt+Tab, Alt+Échap, Ctrl+Échap et Windows"
    });
}

/// Gives them back, the session being over.
pub fn let_go() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{HHOOK, UnhookWindowsHookEx};

    let hook = HOOKED.swap(0, Ordering::Relaxed);
    if hook == 0 {
        return;
    }
    tell();
    HELD.store(0, Ordering::Relaxed);
    SAID.store(0, Ordering::Relaxed);
    TAKEN.store(0, Ordering::Relaxed);
    TOLD.store(0, Ordering::Relaxed);
    // SAFETY: the hook this program installed, given back once.
    unsafe { UnhookWindowsHookEx(hook as HHOOK) };
    crate::journal::note("touches système rendues à cet ordinateur");
}

/// The four keys this program may take, and the bit each is remembered
/// by while it is held down.
fn a_key_of_ours(code: u32) -> Option<u32> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_LWIN, VK_RWIN, VK_TAB};

    match code as u16 {
        VK_TAB => Some(1),
        VK_ESCAPE => Some(2),
        VK_LWIN => Some(4),
        VK_RWIN => Some(8),
        _ => None,
    }
}

/// Whether that key is one the system would act on itself rather than
/// hand over.
///
/// Tab and Escape on their own are ordinary keys and are left alone: a
/// session where Tab moved nothing and Escape closed nothing would be a
/// session nobody can work in. It is the company they keep that makes
/// them the system's, and that is what is read here. The Windows key is
/// the system's whatever it is pressed with, itself included.
fn the_system_would_eat_it(code: u32) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_TAB,
    };

    // SAFETY: a key is named and its state is read; nothing is written.
    let down = |key: i32| unsafe { GetAsyncKeyState(key) } as u16 & 0x8000 != 0;
    match code as u16 {
        VK_TAB => down(VK_MENU as i32),
        VK_ESCAPE => down(VK_MENU as i32) || down(VK_CONTROL as i32),
        VK_LWIN | VK_RWIN => true,
        _ => false,
    }
}

/// Whether a session is on screen with the keyboard really in it.
///
/// Three answers and all three have to be yes. There has to be a picture
/// at all, our own program has to be the one the system calls the front,
/// and the picture has to be the window this program's keyboard goes to.
/// Anything else means the person is doing something outside the session,
/// on a window of ours or somebody else's, and their own Alt+Tab is theirs.
///
/// The second of the three also hands the very start of a session back to
/// the engine, which is where it belongs. A picture is a window of its
/// own until it is taken into ours, a moment or so in; for as long as it
/// is, it can hold the front itself, its own capture works exactly as it
/// was asked to, and this answers no and stays out of it.
fn the_session_has_the_keyboard() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;

    let Some(engine) = crate::picture::the_engines_window() else {
        return false;
    };
    if crate::picture::who_holds_the_front() != crate::picture::Front::Ours {
        return false;
    }
    // SAFETY: no argument, read from the thread whose input this program
    // joined to the engine's, which is this one.
    let holds = unsafe { GetFocus() };
    holds == engine
}

/// Every key of this computer passes through here while a session lasts.
///
/// Kept as short as it can be. The system holds every keystroke of every
/// program until this returns, and drops the hook outright if it takes
/// too long, so the ordinary road out is the first line and everything
/// else is only reached by four keys.
unsafe extern "system" fn seen(
    code: i32,
    what: windows_sys::Win32::Foundation::WPARAM,
    told: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, WM_KEYUP, WM_SYSKEYUP,
    };

    // SAFETY: the system is telling us about a key, so what it points at
    // is a key, and it lives for the length of this call.
    let pass = || unsafe { CallNextHookEx(std::ptr::null_mut(), code, what, told) };
    if code != HC_ACTION as i32 {
        return pass();
    }
    // SAFETY: as above.
    let key = unsafe { &*(told as *const KBDLLHOOKSTRUCT) };
    let Some(bit) = a_key_of_ours(key.vkCode) else {
        return pass();
    };
    // Ours coming back, or another program's. Either way it is not a
    // person typing, and taking it again would be this hook answering
    // itself for as long as the session lasted.
    if key.flags & LLKHF_INJECTED != 0 {
        return pass();
    }

    let up = what as u32 == WM_KEYUP || what as u32 == WM_SYSKEYUP;
    let held = HELD.load(Ordering::Relaxed);
    let take = if up {
        // Released: taken if it was taken on the way down, and only then.
        held & bit != 0
    } else {
        the_session_has_the_keyboard() && the_system_would_eat_it(key.vkCode)
    };
    if !take {
        return pass();
    }
    HELD.store(if up { held & !bit } else { held | bit }, Ordering::Relaxed);

    hand_it_over(key, what);
    // Eaten here, so the system never sees it and never acts on it.
    1
}

/// Puts that key at the picture's own window, as the window's own
/// message rather than as a keystroke of this computer.
///
/// Told rather than typed, which is the difference that matters for the
/// Windows key. A keystroke sent back out would be read by the system
/// first, exactly as the one just taken was, and the Start menu would
/// open after all. A message goes to the one window it names and nowhere
/// else, and the engine reads it as the keystroke it is.
fn hand_it_over(
    key: &windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT,
    what: windows_sys::Win32::Foundation::WPARAM,
) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LLKHF_EXTENDED, PostMessageW, WM_KEYUP, WM_SYSKEYUP,
    };

    let Some(engine) = crate::picture::the_engines_window() else {
        return;
    };
    let up = what as u32 == WM_KEYUP || what as u32 == WM_SYSKEYUP;

    // What a window is told about a key, in the shape it expects: one
    // press, where the key sits, whether it is one of the pair that live
    // off the far end of the keyboard, whether Alt was down with it, and
    // whether this is the release.
    let mut about: isize = 1;
    about |= ((key.scanCode & 0xFF) as isize) << 16;
    if key.flags & LLKHF_EXTENDED != 0 {
        about |= 1 << 24;
    }
    // SAFETY: a key is named and its state is read.
    if unsafe { GetAsyncKeyState(VK_MENU as i32) } as u16 & 0x8000 != 0 {
        about |= 1 << 29;
    }
    if up {
        about |= (1 << 30) | (1 << 31);
    }

    // SAFETY: a window this program took in hand, told about a key in
    // the system's own words. Posted rather than sent: this is the
    // system's own hook, where nothing may wait for another program.
    unsafe { PostMessageW(engine, what as u32, key.vkCode as usize, about) };

    if !up {
        SAID.store(key.vkCode, Ordering::Relaxed);
        TAKEN.fetch_add(1, Ordering::Relaxed);
    }
}

/// Says what has been handed over since the last time it was asked.
///
/// Called from the watch that follows a session, once a second, which is
/// where the journal may be written to without holding up a keystroke.
/// Says nothing while nothing has changed, so a session in which no
/// system key is ever pressed leaves no line at all.
pub fn tell() {
    let taken = TAKEN.load(Ordering::Relaxed);
    if TOLD.swap(taken, Ordering::Relaxed) == taken {
        return;
    }
    crate::journal::note(&format!(
        "touches système portées à la session plutôt qu'à cet ordinateur : {taken} en tout, \
         la dernière {}",
        named(SAID.load(Ordering::Relaxed))
    ));
}

/// What that key is called, for the journal.
fn named(code: u32) -> &'static str {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_LWIN, VK_RWIN, VK_TAB};

    match code as u16 {
        VK_TAB => "Tab",
        VK_ESCAPE => "Échap",
        VK_LWIN => "Windows gauche",
        VK_RWIN => "Windows droite",
        _ => "?",
    }
}
