//! The keys Windows keeps for itself, taken for the session when the
//! keyboard is « Immersif ».
//!
//! Alt+Tab, the Windows key, Ctrl+Échap and a few more never reach a
//! window: the system acts on them before any window hears them. The only
//! way to have them go to the far computer is to step in front of every
//! keystroke of the whole computer with a low-level hook, take those, and
//! hand them to the player. What `docs/CLAVIER.md` learnt the hard way
//! holds here, and this file is written to it:
//!
//! - the hook lives on a thread that only reads its messages
//!   (`crate::hook`), and decides from what it already holds, with no
//!   lock, no journal line and no wait: every key of the computer waits
//!   for its answer;
//! - it is laid again every time the picture gets the keyboard back,
//!   which are the moments another program lays its own and goes ahead of
//!   it, and laid again with no gap;
//! - a key is only taken when the keyboard really is the picture's: its
//!   window has the focus of the thread at the front, one question asked
//!   of the system at once, never the front alone nor the focus alone;
//! - Alt, Ctrl and Shift are never taken: every shortcut of ZyrDesk is an
//!   Alt combination, and the system never shows a hotkey a key a hook
//!   swallowed.
//!
//! What is not taken goes on through the picture's window like any other
//! key. A key taken here reaches the player ahead of what the window has
//! not read yet, so the modifiers it saw held go first: Alt, then Tab.
//! The window's own Alt, arriving after, is a press the player already
//! has and does not send twice.

// The hook is Windows'; the decision is arithmetic, tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// What this module files its journal lines under.
const TAG: &str = "keyboard";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// The switch: whether the system's keys go to the session.
static IMMERSIVE: AtomicBool = AtomicBool::new(false);

/// The modifiers the hook has seen down and not yet up, one bit each.
static HELD: AtomicU8 = AtomicU8::new(0);

/// The keys whose press the hook took and whose release has not come.
static TAKEN: AtomicU8 = AtomicU8::new(0);

/// The keys, by the numbers Windows names them with.
mod named {
    pub const TAB: u32 = 0x09;
    pub const ESCAPE: u32 = 0x1B;
    pub const SNAPSHOT: u32 = 0x2C;
    pub const LEFT_WINDOWS: u32 = 0x5B;
    pub const RIGHT_WINDOWS: u32 = 0x5C;
    pub const F4: u32 = 0x73;
    pub const LEFT_SHIFT: u32 = 0xA0;
    pub const RIGHT_SHIFT: u32 = 0xA1;
    pub const LEFT_CTRL: u32 = 0xA2;
    pub const RIGHT_CTRL: u32 = 0xA3;
    pub const LEFT_ALT: u32 = 0xA4;
    pub const RIGHT_ALT: u32 = 0xA5;
    pub const PLAY_PAUSE: u32 = 0xB3;
}

/// A modifier as the hook follows it: its name, its bit in `HELD`, and
/// where it sits on the keyboard, to be pressed over there.
struct Modifier {
    named: u32,
    bit: u8,
    scancode: u8,
    extended: bool,
}

const MODIFIERS: [Modifier; 6] = [
    Modifier {
        named: named::LEFT_SHIFT,
        bit: 1,
        scancode: 0x2A,
        extended: false,
    },
    Modifier {
        named: named::RIGHT_SHIFT,
        bit: 2,
        scancode: 0x36,
        extended: false,
    },
    Modifier {
        named: named::LEFT_CTRL,
        bit: 4,
        scancode: 0x1D,
        extended: false,
    },
    Modifier {
        named: named::RIGHT_CTRL,
        bit: 8,
        scancode: 0x1D,
        extended: true,
    },
    Modifier {
        named: named::LEFT_ALT,
        bit: 16,
        scancode: 0x38,
        extended: false,
    },
    Modifier {
        named: named::RIGHT_ALT,
        bit: 32,
        scancode: 0x38,
        extended: true,
    },
];

const CTRL: u8 = 4 | 8;
const ALT: u8 = 16 | 32;

/// The modifiers held once that key has gone down or up.
fn after(held: u8, key: u32, down: bool) -> u8 {
    match MODIFIERS.iter().find(|modifier| modifier.named == key) {
        Some(modifier) if down => held | modifier.bit,
        Some(modifier) => held & !modifier.bit,
        None => held,
    }
}

/// The bit a key the system keeps holds in `TAKEN`, for the few it
/// keeps. Alt, Ctrl and Shift have none: they are never taken.
fn slot(key: u32) -> Option<u8> {
    Some(match key {
        named::TAB => 1,
        named::ESCAPE => 2,
        named::LEFT_WINDOWS => 4,
        named::RIGHT_WINDOWS => 8,
        named::SNAPSHOT => 16,
        named::F4 => 32,
        named::PLAY_PAUSE => 64,
        _ => return None,
    })
}

/// Whether the system would act on that press itself, the modifiers held
/// being what they are: Alt+Tab, Alt+Échap, Ctrl+Échap, either Windows
/// key, Impr. écran, Alt+F4, and the media key that plays and pauses.
fn the_system_keeps(key: u32, held: u8) -> bool {
    let (alt, ctrl) = (held & ALT != 0, held & CTRL != 0);
    match key {
        named::TAB | named::F4 => alt,
        named::ESCAPE => alt || ctrl,
        named::LEFT_WINDOWS | named::RIGHT_WINDOWS | named::SNAPSHOT | named::PLAY_PAUSE => true,
        _ => false,
    }
}

/// What the hook does with a keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Left to go on, to the system and to whichever window has the
    /// keyboard.
    Pass,
    /// Taken, and pressed over there after the modifiers held.
    Take,
    /// Taken again, the keyboard repeating a key held down.
    Again,
    /// The release of a key taken, taken too.
    Release,
}

/// The whole decision, with nothing but numbers: the key, whether it goes
/// down, the modifiers held, the keys taken, and whether the picture has
/// the keyboard, asked only of a key worth asking about.
///
/// A release follows its press: a key taken down is taken up, wherever
/// the keyboard has gone since, so the far computer never keeps a key
/// down; and a key the system saw go down is left to the system going up.
fn verdict(key: u32, down: bool, held: u8, taken: u8, ours: impl FnOnce() -> bool) -> Verdict {
    let Some(bit) = slot(key) else {
        return Verdict::Pass;
    };
    if !down {
        return if taken & bit != 0 {
            Verdict::Release
        } else {
            Verdict::Pass
        };
    }
    if taken & bit != 0 {
        return Verdict::Again;
    }
    if the_system_keeps(key, held) && ours() {
        Verdict::Take
    } else {
        Verdict::Pass
    }
}

/// Whether the system's keys go to the session.
pub fn immersive() -> bool {
    IMMERSIVE.load(Ordering::Relaxed)
}

/// Sets the switch for a session about to start, as it was left.
pub fn start_as(immersive: bool) {
    IMMERSIVE.store(immersive, Ordering::Relaxed);
}

/// Throws the switch in the middle of a session: the keys are taken, or
/// given back, at once.
pub fn switch(immersive: bool) {
    IMMERSIVE.store(immersive, Ordering::Relaxed);
    if immersive {
        take_them();
    } else {
        give_them_back();
    }
    note(if immersive {
        "clavier immersif : les touches du système vont à la session"
    } else {
        "clavier partagé : les touches du système restent à cet ordinateur"
    });
}

/// The hook, on its thread, for as long as the keys are taken.
#[cfg(windows)]
static HOOK: crate::hook::Held = crate::hook::Held::new();

/// Why the system last refused the hook, as it said it.
#[cfg(windows)]
static REFUSED_WITH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// The player, where the hook can reach it without a lock: the hook runs
// on the thread that installed it, and this is that thread's.
#[cfg(windows)]
thread_local! {
    static PLAYER: std::cell::RefCell<Option<zyr_player::Player>> =
        const { std::cell::RefCell::new(None) };
}

/// Takes the keys, when the switch says so and a picture is on screen.
#[cfg(windows)]
pub fn take_them() {
    if !immersive() || !crate::video::shown() {
        return;
    }
    match HOOK.hold(put, take_back) {
        Some(true) => note("touches du système prises pour la session"),
        Some(false) => note(&format!(
            "touches du système non prises : Windows a refusé le crochet du clavier \
             (SetWindowsHookExW, erreur {})",
            REFUSED_WITH.load(Ordering::Relaxed)
        )),
        None => {}
    }
}

#[cfg(not(windows))]
pub fn take_them() {}

/// Gives them back, the session being over or the switch thrown.
#[cfg(windows)]
pub fn give_them_back() {
    if HOOK.let_go() {
        note("touches du système rendues à cet ordinateur");
    }
    HELD.store(0, Ordering::Relaxed);
    TAKEN.store(0, Ordering::Relaxed);
}

#[cfg(not(windows))]
pub fn give_them_back() {}

/// Lays the hook again, newest of the chain, the picture having just got
/// the keyboard back; or lays it at all, if the system refused it before.
#[cfg(windows)]
pub fn lay_again() {
    if !HOOK.lay_again() {
        take_them();
    }
}

#[cfg(not(windows))]
pub fn lay_again() {}

/// Forgets the keys taken, the picture having lost the keyboard.
///
/// The player lets go of everything over there at that moment, so their
/// releases, when they come, are this computer's again. Kept, a key whose
/// release happened where no hook sees it, on the screen Ctrl+Alt+Suppr
/// or Windows+L bring up, would stay « taken », and its next press would
/// go to the session from any window at all.
pub fn forget_what_was_taken() {
    TAKEN.store(0, Ordering::Relaxed);
}

/// Lays the hook, on the thread that will hold it.
#[cfg(windows)]
fn put() -> isize {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_KEYBOARD_LL};

    PLAYER.with_borrow_mut(|player| *player = crate::session::player());
    // The modifiers let go of where no hook sees them, on the screen
    // Ctrl+Alt+Suppr brings up, would read as held for ever, and every
    // Tab after that as Alt+Tab. Put down here, as the keyboard comes
    // back to the picture, when the system's own reading of the keys has
    // long settled; a modifier is only ever put down, never made up.
    let mut held = HELD.load(Ordering::Relaxed);
    for modifier in &MODIFIERS {
        // SAFETY: a plain question about one key.
        if held & modifier.bit != 0 && unsafe { GetAsyncKeyState(modifier.named as i32) } >= 0 {
            held &= !modifier.bit;
        }
    }
    HELD.store(held, Ordering::Relaxed);
    // SAFETY: a hook of the whole desk, answered by a plain function of
    // this program, which is the module named.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(heard),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    } as isize;
    if hook == 0 {
        // SAFETY: no argument; the error of the call just above.
        REFUSED_WITH.store(unsafe { GetLastError() }, Ordering::Relaxed);
    }
    hook
}

/// Takes it off, on that same thread.
#[cfg(windows)]
fn take_back(hook: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;

    // SAFETY: a hook this thread laid, given back once.
    unsafe { UnhookWindowsHookEx(hook as _) };
}

/// Every keystroke of the computer, before any window.
///
/// SAFETY: called by the system on the thread that laid the hook, with the
/// arguments it documents.
#[cfg(windows)]
unsafe extern "system" fn heard(
    code: i32,
    what: windows_sys::Win32::Foundation::WPARAM,
    told: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_UP,
    };

    if code == HC_ACTION as i32 {
        // SAFETY: for this code the system hands a description of the
        // keystroke, alive for the length of the call.
        let key = unsafe { &*(told as *const KBDLLHOOKSTRUCT) };
        let down = key.flags & LLKHF_UP == 0;
        let held = after(HELD.load(Ordering::Relaxed), key.vkCode, down);
        HELD.store(held, Ordering::Relaxed);
        let decided = verdict(
            key.vkCode,
            down,
            held,
            TAKEN.load(Ordering::Relaxed),
            the_picture_has_the_keyboard,
        );
        if decided != Verdict::Pass && hand_it_over(key, decided, held) {
            return 1;
        }
    }
    // SAFETY: the arguments the system handed in, untouched.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, what, told) }
}

/// Whether the keyboard is the picture's: its window has the focus of
/// the thread at the front.
#[cfg(windows)]
fn the_picture_has_the_keyboard() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GUITHREADINFO, GetGUIThreadInfo};

    let picture = crate::video::its_window();
    if picture == 0 {
        return false;
    }
    // SAFETY: a plain block, its size written in it as the call asks.
    let mut front: GUITHREADINFO = unsafe { std::mem::zeroed() };
    front.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    // SAFETY: nought names the thread at the front; the block is ours.
    unsafe { GetGUIThreadInfo(0, &mut front) != 0 && front.hwndFocus as isize == picture }
}

/// Hands a key taken to the player, and says whether there was a player
/// to hand it to: without one the key goes on, as if there were no hook.
#[cfg(windows)]
fn hand_it_over(
    key: &windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT,
    decided: Verdict,
    held: u8,
) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::LLKHF_EXTENDED;
    use zyr_player::InputEvent;

    let Some(bit) = slot(key.vkCode) else {
        return false;
    };
    // An on-screen keyboard can type the Windows key by its name alone.
    let said = (key.scanCode as u8, key.flags & LLKHF_EXTENDED != 0);
    let Some((scancode, extended)) =
        crate::video::placed(said, || crate::video::the_layouts_place(key.vkCode))
    else {
        return false;
    };
    PLAYER.with_borrow(|player| {
        let Some(player) = player else {
            return false;
        };
        match decided {
            Verdict::Take => {
                for modifier in MODIFIERS.iter().filter(|modifier| held & modifier.bit != 0) {
                    player.send(InputEvent::Key {
                        scancode: modifier.scancode,
                        extended: modifier.extended,
                        down: true,
                    });
                }
                TAKEN.fetch_or(bit, Ordering::Relaxed);
                player.send(InputEvent::Key {
                    scancode,
                    extended,
                    down: true,
                });
            }
            Verdict::Again => player.send_repeat(scancode, extended),
            Verdict::Release => {
                TAKEN.fetch_and(!bit, Ordering::Relaxed);
                player.send(InputEvent::Key {
                    scancode,
                    extended,
                    down: false,
                });
            }
            Verdict::Pass => return false,
        }
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTHING: u8 = 0;

    /// The decision on a press, the picture having the keyboard.
    fn pressed(key: u32, held: u8) -> Verdict {
        verdict(key, true, held, 0, || true)
    }

    fn holding(keys: &[u32]) -> u8 {
        keys.iter()
            .fold(NOTHING, |held, key| after(held, *key, true))
    }

    #[test]
    fn what_the_system_keeps_is_taken() {
        let alt = holding(&[named::LEFT_ALT]);
        let ctrl = holding(&[named::RIGHT_CTRL]);
        assert_eq!(pressed(named::TAB, alt), Verdict::Take);
        assert_eq!(pressed(named::ESCAPE, alt), Verdict::Take);
        assert_eq!(pressed(named::ESCAPE, ctrl), Verdict::Take);
        assert_eq!(pressed(named::F4, alt), Verdict::Take);
        for alone in [
            named::LEFT_WINDOWS,
            named::RIGHT_WINDOWS,
            named::SNAPSHOT,
            named::PLAY_PAUSE,
        ] {
            assert_eq!(pressed(alone, NOTHING), Verdict::Take, "{alone:#x}");
        }
        // Right Alt counts as Alt, left Shift beside it changes nothing:
        // Alt+Maj+Tab goes round the other way over there.
        let both = holding(&[named::RIGHT_ALT, named::LEFT_SHIFT]);
        assert_eq!(pressed(named::TAB, both), Verdict::Take);
    }

    #[test]
    fn what_the_system_would_not_act_on_goes_its_way() {
        // Tab, Échap and F4 alone are ordinary keys, which the picture's
        // window hears like any other.
        assert_eq!(pressed(named::TAB, NOTHING), Verdict::Pass);
        assert_eq!(pressed(named::ESCAPE, NOTHING), Verdict::Pass);
        assert_eq!(
            pressed(named::F4, holding(&[named::LEFT_CTRL])),
            Verdict::Pass
        );
        assert_eq!(pressed(0x41, holding(&[named::LEFT_ALT])), Verdict::Pass);
    }

    #[test]
    fn alt_ctrl_and_shift_are_never_taken() {
        // Every shortcut of ZyrDesk is an Alt combination, and the system
        // never shows a hotkey a key a hook swallowed.
        for modifier in &MODIFIERS {
            for held in [NOTHING, ALT, CTRL, ALT | CTRL] {
                assert_eq!(pressed(modifier.named, held), Verdict::Pass);
                assert_eq!(
                    verdict(modifier.named, false, held, u8::MAX, || true),
                    Verdict::Pass
                );
            }
        }
    }

    #[test]
    fn nothing_is_taken_while_the_keyboard_is_elsewhere() {
        let alt = holding(&[named::LEFT_ALT]);
        assert_eq!(verdict(named::TAB, true, alt, 0, || false), Verdict::Pass);
        assert_eq!(
            verdict(named::LEFT_WINDOWS, true, NOTHING, 0, || false),
            Verdict::Pass
        );
    }

    #[test]
    fn the_keyboard_is_only_asked_about_a_key_worth_asking_about() {
        // Asked for every keystroke of the computer, the question would
        // cost the whole desk a call into the system per key.
        let asked = std::cell::Cell::new(0);
        let ask = || {
            asked.set(asked.get() + 1);
            true
        };
        let _ = verdict(0x41, true, NOTHING, 0, ask);
        let _ = verdict(named::TAB, true, NOTHING, 0, ask);
        let _ = verdict(named::LEFT_ALT, true, NOTHING, 0, ask);
        assert_eq!(asked.get(), 0);
    }

    #[test]
    fn a_key_taken_down_is_taken_up_wherever_the_keyboard_went() {
        let alt = holding(&[named::LEFT_ALT]);
        let taken = slot(named::TAB).unwrap();
        // Held down, the keyboard repeats it: still the session's.
        assert_eq!(
            verdict(named::TAB, true, alt, taken, || false),
            Verdict::Again
        );
        // Alt let go first, then Tab: the release still goes over there,
        // or the far computer would keep Tab down.
        assert_eq!(
            verdict(named::TAB, false, NOTHING, taken, || false),
            Verdict::Release
        );
        // And a release the hook never took the press of is not taken.
        assert_eq!(verdict(named::TAB, false, alt, 0, || true), Verdict::Pass);
    }

    #[test]
    fn the_modifiers_are_followed_down_and_up() {
        let held = after(NOTHING, named::LEFT_CTRL, true);
        let held = after(held, named::RIGHT_ALT, true);
        assert_eq!(held, 4 | 32);
        let held = after(held, named::LEFT_CTRL, false);
        assert_eq!(held, 32);
        // A key that is no modifier changes nothing.
        assert_eq!(after(held, named::TAB, true), 32);
    }
}
