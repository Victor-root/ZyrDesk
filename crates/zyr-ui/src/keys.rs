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

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The thread that holds the hook, while there is one, by the number the
/// system knows it as: that is what a message is posted to, to ask it to
/// stop.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);

/// And that thread itself, kept so the end of a session can wait for it
/// to have really given the keys back before the next one takes them.
static WORKER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

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
/// refusing, and the tally of answers says which.
static ANY: AtomicU32 = AtomicU32::new(0);

/// Candidate keys, in the same spirit.
static SEEN: AtomicU32 = AtomicU32::new(0);

/// How many candidates each possible answer has accounted for.
///
/// The answers, in order: 1 no picture, 2 the front is elsewhere, 3 the
/// keyboard is not the picture's, 4 the system would not have eaten it
/// anyway, 5 somebody else sent it, 6 a release of a key that was never
/// taken, 7 taken.
///
/// The last answer alone is not enough and reading it as though it were
/// cost a whole round. Several candidates arrive within the one second
/// between two readings, and Alt+Tab comes in pairs of opposite meaning:
/// the one that leaves the session, which ought to be carried, and the
/// one that comes back to it, which ought not. Read from the last of
/// them, a session where every leaving one failed and every returning
/// one was rightly refused reads as a session where nothing was wrong.
static WHYS: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

/// Everything else about the last candidate: which key it was, whether
/// it was a release, who held the front, and what Alt and Control were
/// doing. Each of the four changes the meaning of the same answer.
static LAST_KEY: AtomicU32 = AtomicU32::new(0);
static LAST_UP: AtomicBool = AtomicBool::new(false);
static LAST_FRONT: AtomicU32 = AtomicU32::new(0);
static LAST_MODS: AtomicU32 = AtomicU32::new(0);

/// What the journal last said about the numbers above.
static TOLD: AtomicU32 = AtomicU32::new(0);
static TOLD_SEEN: AtomicU32 = AtomicU32::new(0);

/// Takes the system's keys for as long as a session lasts.
///
/// On a thread of its own, which does nothing else for the whole of a
/// session, and that is not a nicety. A hook of this kind is called back
/// on the thread that installed it, every keystroke of the whole computer
/// waits on that call, and a thread too busy to answer in time has the
/// keystroke handed to the system as though there had been no hook at
/// all. Hung on the thread that draws, as it was, it missed a burst of
/// keys every time that thread had a moment's work: closing the floating
/// menu costs a message to a window, a change of focus and a line written
/// to a file on disk, and the journal caught what it costs, three keys in
/// a row gone from the count, the first Alt+Tab after the menu among
/// them. A thread that only reads its messages has no such moments.
///
/// The callback reads nothing that belongs to a thread. Where the front
/// is, whether a picture is held, whether the keyboard is at it: all of
/// them are asked of the system at large or of numbers this program keeps.
pub fn hold() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PM_NOREMOVE, PeekMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_KEYBOARD_LL,
    };

    if HOOK_THREAD.load(Ordering::SeqCst) != 0 {
        return;
    }
    read_the_modifiers();
    let (say, hear) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        // SAFETY: no argument names this program's own module, and the
        // callback is a plain function of it.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(seen),
                GetModuleHandleW(std::ptr::null()),
                0,
            )
        };
        let mut message = nothing_yet();
        // A thread has nowhere to receive a message until it has looked
        // for one once, and the end of a session posts it the message
        // that asks it to stop and then waits for it to have stopped.
        // Looked for here, before anyone is told this thread exists, so
        // that message cannot be posted into nothing and waited on for
        // ever.
        //
        // SAFETY: the slot is ours, and nothing is taken from the queue.
        unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
        // SAFETY: no argument.
        let me = unsafe { GetCurrentThreadId() };
        let _ = say.send((me, !hook.is_null()));
        if hook.is_null() {
            return;
        }
        // SAFETY: the slot is ours, and no window is named, so this reads
        // what is posted to this thread. It answers 0 for the message
        // that asks it to stop, and -1 for a fault; both end the wait.
        while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            // SAFETY: the message comes from the call above.
            unsafe { DispatchMessageW(&message) };
        }
        // SAFETY: the hook this thread installed, given back once, from
        // the thread that owns it.
        unsafe { UnhookWindowsHookEx(hook) };
    });

    let (thread, taken) = hear.recv().unwrap_or((0, false));
    HOOK_THREAD.store(thread, Ordering::SeqCst);
    *WORKER.lock().expect("fil des touches système") = Some(worker);
    crate::journal::note(if taken {
        "touches système reprises pour la session : Alt+Tab, Alt+Échap et Ctrl+Échap"
    } else {
        "touches système non reprises : Windows a refusé le crochet clavier"
    });
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

/// Gives them back, the session being over.
pub fn let_go() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

    let thread = HOOK_THREAD.swap(0, Ordering::SeqCst);
    if thread == 0 {
        return;
    }
    tell();
    // SAFETY: the thread this program started, told to stop the only way
    // a thread waiting on its messages can be.
    unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
    if let Some(worker) = WORKER.lock().expect("fil des touches système").take() {
        let _ = worker.join();
    }
    for counter in [
        &HELD,
        &SAID,
        &TAKEN,
        &ANY,
        &SEEN,
        &TOLD,
        &TOLD_SEEN,
        &DOWN,
        &LAST_KEY,
        &LAST_FRONT,
        &LAST_MODS,
    ] {
        counter.store(0, Ordering::SeqCst);
    }
    for answer in &WHYS {
        answer.store(0, Ordering::SeqCst);
    }
    HELD_SINCE.store(false, Ordering::Relaxed);
    LAST_UP.store(false, Ordering::SeqCst);
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

/// Which of Alt and Control are held down, counted from the very stream
/// of keys this is filtering.
///
/// Bit one for Alt, bit two for Control, either side of the keyboard.
static DOWN: AtomicU32 = AtomicU32::new(0);

/// Follows Alt and Control through the stream, and says nothing about
/// whether the key should be taken: they never are.
///
/// Counted here rather than asked of the system at the moment a Tab
/// arrives. The system is asked about a key it has not finished
/// processing, from inside its own handling of another one, and what it
/// answers there is not a thing to rest a whole feature on: one Alt+Tab
/// in four was read as a bare Tab and let through, and Windows switched
/// windows on this computer. The stream itself is the only authority on
/// what the stream is carrying.
fn follow_the_modifiers(code: u32, up: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_MENU, VK_RCONTROL, VK_RMENU,
    };

    let bit = match code as u16 {
        VK_MENU | VK_LMENU | VK_RMENU => 1,
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => 2,
        _ => return,
    };
    let was = DOWN.load(Ordering::Relaxed);
    DOWN.store(if up { was & !bit } else { was | bit }, Ordering::Relaxed);
    if !up {
        HELD_SINCE.store(true, Ordering::Relaxed);
    }
}

/// Whether a modifier has gone down since the last time the far computer
/// was checked for one left holding.
///
/// A session in which nobody has touched Alt or Control cannot have
/// stranded either of them, and there is then nothing to put right and
/// nothing to send.
static HELD_SINCE: AtomicBool = AtomicBool::new(false);

/// Reads the same two off the keyboard itself, for the one moment the
/// stream cannot be asked: before there has been any of it.
///
/// A session opened with a finger already on Alt would otherwise start
/// with this program believing nothing is held.
fn read_the_modifiers() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU};

    // SAFETY: a key is named and its state is read; nothing is written.
    let down = |key: u16| unsafe { GetAsyncKeyState(i32::from(key)) } as u16 & 0x8000 != 0;
    let mut held = 0;
    if down(VK_MENU) {
        held |= 1;
    }
    if down(VK_CONTROL) {
        held |= 2;
    }
    DOWN.store(held, Ordering::Relaxed);
}

/// Whether that key is one the system would act on itself rather than
/// hand over.
///
/// Tab and Escape on their own are ordinary keys and are left alone: a
/// session where Tab moved nothing and Escape closed nothing would be a
/// session nobody can work in. It is the company they keep that makes
/// them the system's, and that is what is read here.
fn the_system_would_eat_it(code: u32) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_TAB};

    let held = DOWN.load(Ordering::Relaxed);
    match code as u16 {
        VK_TAB => held & 1 != 0,
        VK_ESCAPE => held != 0,
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
fn the_session_has_the_keyboard() -> u32 {
    use crate::picture::Front;

    if crate::picture::the_engines_window().is_none() {
        return 1;
    }
    let front = match crate::picture::who_holds_the_front() {
        Front::Ours => 0,
        Front::ThePlayer => 1,
        Front::Elsewhere => 2,
    };
    LAST_FRONT.store(front, Ordering::SeqCst);
    if front == 2 {
        return 2;
    }
    if !crate::picture::the_keyboard_is_at_the_picture() {
        return 3;
    }
    0
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
    ANY.fetch_add(1, Ordering::SeqCst);
    // SAFETY: as above.
    let key = unsafe { &*(told as *const KBDLLHOOKSTRUCT) };
    let up = what as u32 == WM_KEYUP || what as u32 == WM_SYSKEYUP;
    // Before anything else, and for every key including the ones that go
    // straight through: what Alt and Control are doing is read off this
    // stream and nowhere else.
    follow_the_modifiers(key.vkCode, up);
    let Some(bit) = a_key_of_ours(key.vkCode) else {
        return pass();
    };

    // What was decided about this one, and everything that decided it.
    let held = HELD.load(Ordering::SeqCst);
    let why = if key.flags & LLKHF_INJECTED != 0 {
        // Ours coming back, or another program's. Either way it is not a
        // person typing, and taking it again would be this hook
        // answering itself for as long as the session lasted.
        5
    } else if up {
        // Released: taken if it was taken on the way down, and only then.
        if held & bit != 0 { 7 } else { 6 }
    } else {
        match the_session_has_the_keyboard() {
            0 if the_system_would_eat_it(key.vkCode) => 7,
            0 => 4,
            no => no,
        }
    };

    // Written down before the counts, and every count read back after
    // them by the one that says all this out loud: read the other way
    // about, a count could arrive at that reader ahead of the answer that
    // goes with it, and a candidate refused would read as one nothing was
    // yet known about. That happened, and a whole round was spent
    // chasing what it seemed to say.
    LAST_KEY.store(key.vkCode, Ordering::SeqCst);
    LAST_UP.store(up, Ordering::SeqCst);
    LAST_MODS.store(DOWN.load(Ordering::SeqCst), Ordering::SeqCst);
    WHYS[why as usize].fetch_add(1, Ordering::SeqCst);
    SEEN.fetch_add(1, Ordering::SeqCst);
    if why != 7 {
        return pass();
    }
    HELD.store(if up { held & !bit } else { held | bit }, Ordering::SeqCst);

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
        SAID.store(key.vkCode, Ordering::SeqCst);
        TAKEN.fetch_add(1, Ordering::SeqCst);
    }
}

/// Says what has been handed over since the last time it was asked.
///
/// Called from the watch that follows a session, once a second, which is
/// where the journal may be written to without holding up a keystroke.
/// Says nothing while nothing has changed, so a session in which no
/// system key is ever pressed leaves no line at all.
pub fn tell() {
    let seen = SEEN.load(Ordering::SeqCst);
    let taken = TAKEN.load(Ordering::SeqCst);
    // Both read and put back before either is judged. Joined with « or »,
    // the first change would be the other's excuse for never being
    // written down, and the line would then come out a second time on the
    // strength of a change already reported.
    let carried = TOLD.swap(taken, Ordering::SeqCst) != taken;
    let candidates = TOLD_SEEN.swap(seen, Ordering::SeqCst) != seen;
    if !(carried || candidates) {
        return;
    }
    let mods = LAST_MODS.load(Ordering::SeqCst);
    let counted: Vec<String> = (1..WHYS.len())
        .filter_map(|answer| {
            let how_many = WHYS[answer].load(Ordering::SeqCst);
            (how_many > 0).then(|| format!("{how_many} {}", in_words(answer as u32)))
        })
        .collect();
    crate::journal::note(&format!(
        "touches système : {} frappe(s) vues, {seen} candidate(s), {taken} portée(s) ; \
         {} ; la dernière était {} {}, Alt {}, Ctrl {}, premier plan {}",
        ANY.load(Ordering::SeqCst),
        counted.join(", "),
        named(LAST_KEY.load(Ordering::SeqCst)),
        if LAST_UP.load(Ordering::SeqCst) {
            "relâchée"
        } else {
            "enfoncée"
        },
        if mods & 1 != 0 { "oui" } else { "non" },
        if mods & 2 != 0 { "oui" } else { "non" },
        match LAST_FRONT.load(Ordering::SeqCst) {
            0 => "à ZyrDesk",
            1 => "à l'image",
            _ => "ailleurs",
        },
    ));
}

/// What one of those answers is called.
fn in_words(answer: u32) -> &'static str {
    match answer {
        1 => "sans image",
        2 => "premier plan ailleurs",
        3 => "clavier pas à l'image",
        4 => "que le système n'aurait pas mangées",
        5 => "venues d'un programme",
        6 => "relâchements de touches jamais reprises",
        _ => "portées à la session",
    }
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

    // Nothing held since the last time through means nothing can have
    // been stranded, and this is asked once a second for the length of a
    // session: without this it would be eight messages a second, every
    // second, saying nothing.
    if !HELD_SINCE.swap(false, Ordering::Relaxed) {
        return;
    }
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
