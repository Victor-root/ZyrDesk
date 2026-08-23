//! The keys Windows keeps for itself, handed to the far computer.
//!
//! Alt+Tab, Alt+Échap and Ctrl+Échap never reach the window they are
//! typed at: the system acts on them itself, before any program sees
//! them. A remote desktop wants the opposite.
//!
//! The client engine has an option for exactly this, and it was asked for
//! and has been taken back out (D32). Two things stood in the way, and
//! neither is a defect of the engine. It decides it holds the keyboard by
//! comparing its own window with the window the system calls the front
//! one, and that window is carried inside ours for the length of a
//! session, which makes it a child window; the system gives the front to
//! the head of a family and never to a member of it, so the answer is no
//! seconds in and stays no. And the way it takes those keys is by putting
//! itself in front of every keystroke of the whole computer and
//! swallowing Alt and Control whole. Every shortcut this product has is an
//! Alt combination, so for as long as the engine held those keys, none of
//! them worked.
//!
//! The window the system does call the front one is ours. So the program
//! that can take these keys is this one, and it takes them and passes
//! them on unchanged, Alt and Control untouched. Nothing of this is in the
//! engine, and nothing of this asks the engine for anything it does not
//! already do: what it receives is an ordinary keystroke at its own
//! window, which it forwards like any other.
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

/// Every key of this computer that reached this program at all, and every
/// one of them that was one of ours to consider.
///
/// The two numbers that say where a round of « nothing was ever taken »
/// went wrong, and they cannot be worked out from anything else. The
/// first at nought means the hook is not being called; the first counting
/// with the second at nought means it is called and these keys never
/// reach it; both counting with nothing taken means a condition below is
/// refusing, and `WHY` says which.
static ANY: AtomicU32 = AtomicU32::new(0);

/// Candidate keys, in the same spirit.
static SEEN: AtomicU32 = AtomicU32::new(0);

/// Why the last candidate was left to the system, as one small number.
///
/// 1 no picture, 2 the front is elsewhere, 3 the keyboard is not the
/// picture's, 4 the system would not have eaten it anyway, 5 somebody
/// else sent it.
static WHY: AtomicU32 = AtomicU32::new(0);

/// What the journal last said about the numbers above.
static TOLD: AtomicU32 = AtomicU32::new(0);
static TOLD_SEEN: AtomicU32 = AtomicU32::new(0);
static TOLD_WHY: AtomicU32 = AtomicU32::new(0);

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
        "touches système reprises pour la session : Alt+Tab, Alt+Échap et Ctrl+Échap"
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
    for counter in [
        &HELD, &SAID, &TAKEN, &ANY, &SEEN, &WHY, &TOLD, &TOLD_SEEN, &TOLD_WHY,
    ] {
        counter.store(0, Ordering::Relaxed);
    }
    // SAFETY: the hook this program installed, given back once.
    unsafe { UnhookWindowsHookEx(hook as HHOOK) };
    crate::journal::note("touches système rendues à cet ordinateur");
}

/// The keys this program may take, and the bit each is remembered by
/// while it is held down.
///
/// The Windows key is not among them, and that is the one thing this
/// cannot do. The engine refuses to pass it to the far computer unless
/// its own capture of the system's keys is running, which in this product
/// it never is. Taken here it would open no menu anywhere, neither the
/// far computer's nor this one's, which is worse than leaving it alone:
/// left alone it does what it has always done on this computer.
fn a_key_of_ours(code: u32) -> Option<u32> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_TAB};

    match code as u16 {
        VK_TAB => Some(1),
        VK_ESCAPE => Some(2),
        _ => None,
    }
}

/// Whether that key is one the system would act on itself rather than
/// hand over.
///
/// Tab and Escape on their own are ordinary keys and are left alone: a
/// session where Tab moved nothing and Escape closed nothing would be a
/// session nobody can work in. It is the company they keep that makes
/// them the system's, and that is what is read here.
fn the_system_would_eat_it(code: u32) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_TAB,
    };

    // SAFETY: a key is named and its state is read; nothing is written.
    let down = |key: i32| unsafe { GetAsyncKeyState(key) } as u16 & 0x8000 != 0;
    match code as u16 {
        VK_TAB => down(VK_MENU as i32),
        VK_ESCAPE => down(VK_MENU as i32) || down(VK_CONTROL as i32),
        _ => false,
    }
}

/// Whether a session is on screen with the keyboard really in it.
///
/// Three answers and all three have to be yes. There has to be a picture
/// at all, the front has to belong to this session, and the picture has
/// to be the window this program's keyboard goes to. Anything else means
/// the person is doing something outside the session, on a window of ours
/// or somebody else's, and their own Alt+Tab is theirs.
///
/// « This session » and not « this program », which was the whole of one
/// round where nothing was ever taken at all. The front does read as the
/// player's during a session and not only as ours: the engine's program
/// keeps a window of its own beside the picture, and the two answers mean
/// the same thing here. Asked of ours alone, this said no for the length
/// of every session and Alt+Tab went on switching windows on this
/// computer, exactly as before there was any of this.
///
/// The third is read from what this program worked out a moment ago
/// rather than asked of the system here. Where the keyboard is, asked
/// from a thread, is answered about that thread's own; this runs inside
/// the system's own handling of a keystroke, which is not an ordinary
/// place to ask it from, and the answer is one this program already
/// keeps and refreshes every second and at every settling of the front.
fn the_session_has_the_keyboard() -> bool {
    if crate::picture::the_engines_window().is_none() {
        WHY.store(1, Ordering::Relaxed);
        return false;
    }
    if crate::picture::who_holds_the_front() == crate::picture::Front::Elsewhere {
        WHY.store(2, Ordering::Relaxed);
        return false;
    }
    if !crate::picture::the_keyboard_is_at_the_picture() {
        WHY.store(3, Ordering::Relaxed);
        return false;
    }
    true
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
    ANY.fetch_add(1, Ordering::Relaxed);
    // SAFETY: as above.
    let key = unsafe { &*(told as *const KBDLLHOOKSTRUCT) };
    let Some(bit) = a_key_of_ours(key.vkCode) else {
        return pass();
    };
    SEEN.fetch_add(1, Ordering::Relaxed);
    // Ours coming back, or another program's. Either way it is not a
    // person typing, and taking it again would be this hook answering
    // itself for as long as the session lasted.
    if key.flags & LLKHF_INJECTED != 0 {
        WHY.store(5, Ordering::Relaxed);
        return pass();
    }

    let up = what as u32 == WM_KEYUP || what as u32 == WM_SYSKEYUP;
    let held = HELD.load(Ordering::Relaxed);
    let take = if up {
        // Released: taken if it was taken on the way down, and only then.
        held & bit != 0
    } else if !the_session_has_the_keyboard() {
        false
    } else if !the_system_would_eat_it(key.vkCode) {
        WHY.store(4, Ordering::Relaxed);
        false
    } else {
        true
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
    let seen = SEEN.load(Ordering::Relaxed);
    let why = WHY.load(Ordering::Relaxed);
    // The three read and put back before any of them is judged. Joined
    // with « or », the first change would be the last two's excuse for
    // never being written down, and the line would then come out a second
    // time on the strength of a change already reported.
    let carried = TOLD.swap(taken, Ordering::Relaxed) != taken;
    let candidates = TOLD_SEEN.swap(seen, Ordering::Relaxed) != seen;
    let refusal = TOLD_WHY.swap(why, Ordering::Relaxed) != why;
    if !(carried || candidates || refusal) {
        return;
    }
    crate::journal::note(&format!(
        "touches système : {} frappe(s) vues en tout, {seen} candidate(s), {taken} portée(s) à la \
         session ; la dernière portée {}, la dernière laissée parce que {}",
        ANY.load(Ordering::Relaxed),
        named(SAID.load(Ordering::Relaxed)),
        match why {
            0 => "rien n'a encore été laissé",
            1 => "il n'y a pas d'image",
            2 => "le premier plan est ailleurs",
            3 => "le clavier n'est pas à l'image",
            4 => "le système ne l'aurait pas mangée",
            5 => "elle vient d'un programme et non d'un doigt",
            _ => "?",
        }
    ));
}

/// Releases, at the picture, every modifier no finger is holding.
///
/// What « I lost the keyboard in the session » turned out to be. A
/// modifier goes down here, something takes the keyboard off the picture
/// before it comes back up, and the release never reaches the far
/// computer: it goes on believing Alt is held, and every letter typed
/// afterwards arrives there as Alt and a letter, which does nothing and
/// looks exactly like a dead keyboard. The engine says so at the end of
/// every such session, in its own log, in three words: « Raising 1 keys ».
///
/// Windows' own window switcher is the surest way to cause it, since it
/// takes the keyboard for itself between the press and the release of the
/// very key that opened it.
///
/// Read from the fingers rather than from anything remembered: what is
/// physically down is the one truth here, and a release sent for a key
/// that is genuinely held would be a worse fault than the one being
/// fixed. A release the far computer did not need costs nothing, its own
/// engine dropping any that says what it already believes.
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
    for (key, place, far) in MODIFIERS {
        // SAFETY: a key is named and its state is read; nothing is
        // written.
        let down = unsafe { GetAsyncKeyState(i32::from(key)) } as u16 & 0x8000 != 0;
        if down {
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

/// What that key is called, for the journal.
fn named(code: u32) -> &'static str {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_TAB};

    match code as u16 {
        VK_TAB => "Tab",
        VK_ESCAPE => "Échap",
        _ => "?",
    }
}
