//! The touchpad of this computer, read by ZyrDesk itself.
//!
//! Un ordinateur portable répond aux doigts posés dessus : trois glissés
//! sur le côté changent de fenêtre, trois posés et relevés d'un coup font
//! un clic molette, quatre posés et relevés mettent en lecture ou en
//! pause. Windows répond aux trois lui-même, de ce côté-ci du réseau,
//! quelle que soit l'image qu'une session est en train de montrer. Ce sont
//! les derniers gestes d'une journée ordinaire qui ne traversent pas.
//!
//! They do not cross because nothing carries them. What the engines speak
//! knows about keys, buttons and a pointer, and nothing about a hand with
//! several fingers on it; and Windows answers a global gesture in its own
//! shell, before any program is offered it. There is one documented way
//! for a program to be offered them instead, `TouchpadGesturesController`,
//! and it is marked experimental on a contract no shipped Windows carries:
//! writing against it would be writing for a machine nobody has.
//!
//! So the pad itself is read. A precision touchpad is an ordinary HID
//! device that reports where each finger is, and any program may ask the
//! system for that stream by naming a window: what arrives is the pad's
//! own reports, whatever the shell does with them beside us. This file
//! turns those reports into the gestures the product carries, and
//! `crate::floating` decides what each of them means for the session.
//!
//! The one thing this cannot do is stop Windows answering them too. That
//! lives in the person's own touchpad settings, on a page of Windows, and
//! there is no supported call for it: the registry values behind that page
//! are read once, deep in the input stack, and a program writing them
//! changes what the page shows and nothing about what the pad does. So
//! this reads them rather than writing them, and the switch that turns the
//! reading on refuses while Windows still holds the gestures: on and held
//! at once would have every gesture happen twice, once at each end, which
//! is worse than either.
//!
//! Ce qui reconnaît un geste ne parle pas à Windows et se compile
//! partout : c'est de l'arithmétique sur des doigts, et c'est la seule
//! moitié dont un essai puisse dire quoi que ce soit.

// Hors de Windows il n'y a pas de pavé à lire, mais le reconnaisseur est
// compilé et éprouvé partout.
#![cfg_attr(not(windows), allow(dead_code))]

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "touchpad";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Ce qu'un geste à plusieurs doigts veut dire.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gesture {
    /// Trois doigts glissés vers la gauche.
    Leftwards,
    /// Trois doigts glissés vers la droite.
    Rightwards,
    /// Trois doigts posés et relevés sans bouger.
    ThreeTap,
    /// Quatre doigts posés et relevés sans bouger.
    FourTap,
}

impl std::fmt::Display for Gesture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Gesture::Leftwards => "trois doigts vers la gauche",
            Gesture::Rightwards => "trois doigts vers la droite",
            Gesture::ThreeTap => "trois doigts posés",
            Gesture::FourTap => "quatre doigts posés",
        })
    }
}

/// How many fingers make a gesture of ours.
///
/// Three and four. Five is Windows' own and always was, and a hand that
/// puts five fingers down did not ask for anything here.
const THREE: u32 = 3;
const FOUR: u32 = 4;

/// How far the hand travels for one window, in thousandths of the pad.
///
/// A fifth of the pad, so an ordinary swipe changes one window and a long
/// slide changes several, which is what the hand expects of a gesture it
/// can keep going. Counted in thousandths rather than in what the pad
/// reports: every pad has its own ruler, and a threshold in its units
/// would be a different gesture on every laptop.
const A_STEP: i32 = 200;

/// How long the fingers may stay down and still be a tap.
const AT_MOST_A_TAP: u64 = 300;

/// And how far they may drift while doing it, in thousandths of the pad.
///
/// Not nought: fingers landing and leaving together always slide a little,
/// and a tap that demanded perfect stillness would almost never be one.
const STILL: i32 = 40;

/// Where the hand is in a gesture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// Nothing of ours on the pad.
    Nothing,
    /// Fingers down, and nothing decided yet: it is still both a tap that
    /// has not ended and a slide that has not started.
    Down {
        /// The most fingers seen at once so far, which is what the
        /// gesture turns out to be about. Three fingers landing a moment
        /// before a fourth are a four-finger gesture, and only the
        /// largest count ever says so.
        how_many: u32,
        from: (i32, i32),
        since: u64,
        /// The furthest the hand has been from where it landed, which is
        /// what keeps a slow drift from being read as a tap.
        apart: i32,
    },
    /// Three fingers sliding sideways, `steps` windows asked for so far.
    Sliding { from: i32, steps: i32 },
    /// A gesture that is not ours: four fingers, or three going up and
    /// down. Nothing more is read of it until the hand lifts.
    ///
    /// Kept rather than simply ignored, so that a vertical swipe which
    /// happens to end sideways does not turn into a window change on its
    /// way out.
    Elsewhere,
}

/// What the pad has been saying, gesture after gesture.
///
/// Fed one frame at a time, a frame being every finger the pad reports at
/// one instant, and it answers with a gesture at the moment that gesture
/// becomes true. Nothing is buffered and nothing is asked of a clock: the
/// caller says when, so this can be tried at any speed.
#[derive(Debug)]
pub struct Reading {
    state: State,
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            state: State::Nothing,
        }
    }
}

impl Reading {
    pub const fn new() -> Self {
        Self {
            state: State::Nothing,
        }
    }

    /// Takes one frame of the pad and says what it just made, if
    /// anything.
    ///
    /// `x` and `y` are where the hand is, in thousandths of the pad, and
    /// `at` is a count of milliseconds that only has to move forward.
    pub fn saw(&mut self, fingers: u32, x: i32, y: i32, at: u64) -> Option<Gesture> {
        if fingers == 0 {
            return self.lifted(at);
        }
        if fingers > FOUR {
            self.state = State::Elsewhere;
            return None;
        }
        // Fewer than three, with something already begun: a finger
        // landing a moment after the others, or leaving a moment before
        // them. Nothing is decided here and, above all, nothing is
        // cancelled: read as an ending, every gesture would end halfway.
        if fingers < THREE {
            return None;
        }
        match self.state {
            State::Elsewhere => None,
            State::Nothing => {
                self.state = State::Down {
                    how_many: fingers,
                    from: (x, y),
                    since: at,
                    apart: 0,
                };
                None
            }
            State::Down {
                how_many,
                from,
                since,
                apart,
            } => {
                let (dx, dy) = (x - from.0, y - from.1);
                let how_many = how_many.max(fingers);
                self.state = State::Down {
                    how_many,
                    from,
                    since,
                    apart: apart.max(dx.abs()).max(dy.abs()),
                };
                // A four-finger slide changes this computer's own virtual
                // desktops and always has: only the tap is taken from it,
                // and the tap is what the hand not having moved says.
                if how_many != THREE || dx.abs() < A_STEP {
                    return None;
                }
                // Which way the hand went is decided once, when it has
                // gone far enough to have gone anywhere at all. Up and
                // down belong to Windows, and asked again at every frame
                // this would change its mind halfway through a slide.
                if dx.abs() <= dy.abs() {
                    self.state = State::Elsewhere;
                    return None;
                }
                self.state = State::Sliding {
                    from: from.0,
                    steps: 0,
                };
                self.stepped(x)
            }
            // A fourth finger landing on a slide already under way makes
            // it a gesture of Windows', halfway through.
            State::Sliding { .. } if fingers == FOUR => {
                self.state = State::Elsewhere;
                None
            }
            State::Sliding { .. } => self.stepped(x),
        }
    }

    /// The hand has left the pad.
    fn lifted(&mut self, at: u64) -> Option<Gesture> {
        // A tap is what a gesture turns out to have been, and it can only
        // be told at the end: three fingers down are a tap that has not
        // finished until they have travelled too far or stayed too long.
        match std::mem::replace(&mut self.state, State::Nothing) {
            State::Down {
                how_many,
                since,
                apart,
                ..
            } if at.saturating_sub(since) <= AT_MOST_A_TAP && apart <= STILL => {
                Some(if how_many == THREE {
                    Gesture::ThreeTap
                } else {
                    Gesture::FourTap
                })
            }
            _ => None,
        }
    }

    /// The next step, if the hand has just crossed one.
    ///
    /// One at a time, and counted from where the hand landed rather than
    /// from the last step: a hand that comes back the way it went undoes
    /// its own windows, which is what a hand that overshot is asking for.
    fn stepped(&mut self, x: i32) -> Option<Gesture> {
        let State::Sliding { from, steps } = self.state else {
            return None;
        };
        let wanted = (x - from) / A_STEP;
        if wanted == steps {
            return None;
        }
        let onwards = if wanted > steps { 1 } else { -1 };
        self.state = State::Sliding {
            from,
            steps: steps + onwards,
        };
        Some(if onwards > 0 {
            Gesture::Rightwards
        } else {
            Gesture::Leftwards
        })
    }
}

/* ---- Ce que Windows garde encore pour lui ---------------------------- */

/// What Windows still answers itself, of the gestures this reads.
///
/// One by one, because the person sets them one by one on that page and
/// because only some of them are usually in the way. The four-finger
/// slide is not among them: it changes this computer's own virtual
/// desktops, nothing here ever takes it, and asking somebody to give up a
/// gesture that is not wanted would be asking for nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Held {
    /// Whether a three-finger slide still does something here.
    pub slide: bool,
    /// Whether a three-finger tap still does.
    pub tap: bool,
    /// And whether a four-finger tap still does.
    pub four_tap: bool,
}

impl Held {
    /// Whether Windows still takes any of them.
    pub fn any(self) -> bool {
        self.slide || self.tap || self.four_tap
    }

    /// Ce qu'il faut dire à la personne pour qu'elle les lui reprenne.
    ///
    /// Les mots de la page de Windows, dans l'ordre où elle les montre :
    /// c'est une page qu'on ouvre une fois, et une phrase qui décrit un
    /// chemin sans le nommer est une phrase qu'on relit trois fois.
    pub fn what_to_do(self) -> String {
        let mut lines = Vec::new();
        if let Some(which) = match (self.slide, self.tap) {
            (true, true) => Some("« Balayages » et « Appuis »"),
            (true, false) => Some("« Balayages »"),
            (false, true) => Some("« Appuis »"),
            (false, false) => None,
        } {
            lines.push(format!("Gestes à trois doigts : {which} sur « Rien »."));
        }
        if self.four_tap {
            lines.push("Gestes à quatre doigts : « Appuis » sur « Rien ».".to_string());
        }
        format!(
            "Windows garde encore ces gestes pour lui.\n  \
             Paramètres, Bluetooth et appareils, Pavé tactile :\n  {}",
            lines.join("\n  ")
        )
    }
}

/// Where Windows keeps what its own touchpad page decides.
#[cfg(windows)]
const WHERE_WINDOWS_KEEPS_IT: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\PrecisionTouchPad";

/// What Windows still holds, `None` on a computer with no precision
/// touchpad at all.
///
/// Read and never written. The page these two values belong to is read
/// once, deep in the input stack, and a program that writes them changes
/// what the page shows and nothing about what the pad does: written here,
/// this would be a switch that reports success and does nothing, which is
/// the one kind of switch worth refusing to build.
#[cfg(windows)]
pub fn what_windows_still_holds() -> Option<Held> {
    // Nought is the one value that means « nothing », for both: it is what
    // the page writes when a gesture is set to « Rien ». Anything else is
    // some action of Windows' own, and a value that cannot be read at all
    // is the default, which is an action too.
    let slide = a_setting("ThreeFingerSlideEnabled");
    let tap = a_setting("ThreeFingerTapEnabled");
    let four_tap = a_setting("FourFingerTapEnabled");
    // None of them written is a computer whose pad Windows never had a
    // page for, which is a computer with no precision touchpad: there is
    // nothing here to take back and nothing to read either.
    if slide.is_none() && tap.is_none() && four_tap.is_none() {
        return None;
    }
    Some(Held {
        slide: slide != Some(0),
        tap: tap != Some(0),
        four_tap: four_tap != Some(0),
    })
}

#[cfg(not(windows))]
pub fn what_windows_still_holds() -> Option<Held> {
    None
}

/// One of that page's values, `None` when it has never been written.
#[cfg(windows)]
fn a_setting(named: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let path = wide(WHERE_WINDOWS_KEEPS_IT);
    let name = wide(named);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: two strings of ours, ended by nought, and a slot of ours
    // whose size is the one the call is told to expect.
    let asked = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut value).cast(),
            &mut size,
        )
    };
    (asked == 0).then_some(value)
}

/// A string as Windows reads them, ended by nought.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens the page of Windows where those gestures are given back.
///
/// Opened rather than described. It is one page, it is three clicks deep,
/// and the person is in front of a picture of another computer when they
/// ask: a sentence naming a path is a sentence read three times, and a
/// page that is already open is read once.
#[cfg(windows)]
pub fn open_the_windows_page() {
    // The shell knows what to do with the system's own addresses, and it
    // is the same call that opens a folder for this product elsewhere.
    let opened = std::process::Command::new("explorer")
        .arg("ms-settings:devices-touchpad")
        .spawn();
    if let Err(e) = opened {
        note(&format!("réglages du pavé tactile non ouverts : {e}"));
    }
}

#[cfg(not(windows))]
pub fn open_the_windows_page() {}

/* ---- Ce que le système raconte du pavé ------------------------------- */

/// The pad, as HID names it: a digitizer, and a touch pad among them.
#[cfg(windows)]
const A_TOUCHPAD: (u16, u16) = (0x0D, 0x05);

/// The usages read out of one report, on the two pages they live on.
///
/// A finger is a collection of its own, and it carries where it is on the
/// generic desktop page, like a mouse, and whether it is really touching
/// on the digitizer page. How many fingers the pad is reporting at all is
/// said once, at the top.
#[cfg(windows)]
mod usage {
    pub const DESKTOP: u16 = 0x01;
    pub const DIGITIZER: u16 = 0x0D;
    pub const ACROSS: u16 = 0x30;
    pub const DOWN: u16 = 0x31;
    pub const TOUCHING: u16 = 0x42;
    pub const A_FINGER: u16 = 0x51;
    pub const HOW_MANY: u16 = 0x54;
}

/// The reading, and the thread it lives on; see `crate::hook`.
///
/// A thread of its own for the same reason the hooks have one: what
/// arrives here arrives as a message, a message only reaches a thread
/// that is reading its own, and the thread that draws this program has an
/// interface to keep flowing.
#[cfg(windows)]
static READING: crate::hook::Held = crate::hook::Held::new();

/// Where the hand is, from one frame to the next.
#[cfg(windows)]
static HAND: std::sync::Mutex<Reading> = std::sync::Mutex::new(Reading::new());

/// The frame being put together, the pad splitting one across several
/// reports when it has more fingers than a report holds.
#[cfg(windows)]
static FRAME: std::sync::Mutex<Frame> = std::sync::Mutex::new(Frame::new());

/// The pad as the system describes it, worked out once.
#[cfg(windows)]
static PAD: std::sync::Mutex<Option<Pad>> = std::sync::Mutex::new(None);

/// What this reading has seen, for the one line it writes when it stops.
///
/// Counted rather than told frame by frame: a pad reports a hundred times
/// a second, and a journal that said so would say nothing else. What has
/// to be readable afterwards is whether the pad was read at all, whether
/// enough fingers ever landed on it, and how many gestures came of them.
#[cfg(windows)]
mod counted {
    use std::sync::atomic::AtomicU32;

    pub static FRAMES: AtomicU32 = AtomicU32::new(0);
    pub static THREES: AtomicU32 = AtomicU32::new(0);
    pub static GESTURES: AtomicU32 = AtomicU32::new(0);
}

/// Reads the pad for as long as it is wanted, and says whether the
/// reading stands.
///
/// Asked again for what is already the case costs nothing, which is what
/// lets the watch of the floating button say it at every turn rather than
/// remember where it stood.
#[cfg(windows)]
pub fn read_the_pad(wanted: bool) -> bool {
    use std::sync::atomic::Ordering;

    if !wanted {
        if READING.let_go() {
            note(&format!(
                "pavé tactile : {} trames lues, {} à trois doigts ou plus, {} gestes",
                counted::FRAMES.load(Ordering::Relaxed),
                counted::THREES.load(Ordering::Relaxed),
                counted::GESTURES.load(Ordering::Relaxed),
            ));
        }
        return false;
    }
    let Some(taken) = READING.hold(open_the_reading, close_the_reading) else {
        // Already held, which is every turn of the watch but the first.
        return true;
    };
    counted::FRAMES.store(0, Ordering::Relaxed);
    counted::THREES.store(0, Ordering::Relaxed);
    counted::GESTURES.store(0, Ordering::Relaxed);
    *HAND.lock().expect("lecture du pavé") = Reading::new();
    *FRAME.lock().expect("trame du pavé") = Frame::new();
    note(if taken {
        "pavé tactile lu par ZyrDesk : ses gestes partent dans la session"
    } else {
        "pavé tactile non lu : Windows a refusé de le donner"
    });
    taken
}

#[cfg(not(windows))]
pub fn read_the_pad(_wanted: bool) -> bool {
    false
}

/// Opens the window the pad's reports are delivered to, and asks for
/// them.
///
/// A window of this program's own, never shown and never sized: what the
/// system needs is somewhere to deliver to, and the one window this
/// program shows belongs to the thread that draws.
#[cfg(windows)]
fn open_the_reading() -> isize {
    let window = a_window_of_our_own();
    if window.is_null() {
        return 0;
    }
    if !ask_for_the_pad(window, true) {
        // SAFETY: a window this call has just opened, on this thread.
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(window) };
        return 0;
    }
    window as isize
}

/// Gives the pad back and closes that window.
#[cfg(windows)]
fn close_the_reading(window: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

    let window = window as *mut core::ffi::c_void as HWND;
    ask_for_the_pad(window, false);
    // SAFETY: the window this reading opened, given back on the thread
    // that opened it, which is the only thread allowed to.
    unsafe { DestroyWindow(window) };
    *PAD.lock().expect("pavé décrit") = None;
}

/// Asks the system for the pad's own reports, named at that window, or
/// gives them back.
#[cfg(windows)]
fn ask_for_the_pad(window: windows_sys::Win32::Foundation::HWND, wanted: bool) -> bool {
    use windows_sys::Win32::UI::Input::RegisterRawInputDevices;
    use windows_sys::Win32::UI::Input::{RAWINPUTDEVICE, RIDEV_INPUTSINK, RIDEV_REMOVE};

    let pad = RAWINPUTDEVICE {
        usUsagePage: A_TOUCHPAD.0,
        usUsage: A_TOUCHPAD.1,
        dwFlags: if wanted {
            RIDEV_INPUTSINK
        } else {
            RIDEV_REMOVE
        },
        hwndTarget: if wanted { window } else { std::ptr::null_mut() },
    };
    // SAFETY: one description of ours, and its size is the one the call is
    // told to expect.
    unsafe { RegisterRawInputDevices(&pad, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 }
}

/// A window of this program's own, never shown.
#[cfg(windows)]
fn a_window_of_our_own() -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    let name = wide("ZyrDeskPaveTactile");
    // SAFETY: no argument, and a refusal comes back as null, which the
    // call below is allowed to be given.
    let program = unsafe { GetModuleHandleW(std::ptr::null()) };
    let sort = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(what_the_pad_says),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: program,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: name.as_ptr(),
    };
    // SAFETY: a description of ours, whose strings outlive the call. A
    // class already registered comes back as nought, which is one of the
    // answers: the reading is stopped and started again within one run.
    unsafe { RegisterClassW(&sort) };
    // SAFETY: our own class and name, and no parent. Never shown: the
    // style carries no visibility, and nothing shows it afterwards.
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            name.as_ptr(),
            name.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            program,
            std::ptr::null(),
        )
    }
}

/// The pad's reports, as they arrive.
#[cfg(windows)]
unsafe extern "system" fn what_the_pad_says(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    which: windows_sys::Win32::Foundation::WPARAM,
    packet: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_INPUT};

    if message == WM_INPUT {
        a_report_came_in(packet as *mut core::ffi::c_void);
    }
    // SAFETY: the message is handed on exactly as it came, this window
    // having no behaviour of its own.
    unsafe { DefWindowProcW(window, message, which, packet) }
}

/// Reads one packet of the pad and hands what it says to the reading.
#[cfg(windows)]
fn a_report_came_in(packet: *mut core::ffi::c_void) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::System::SystemInformation::GetTickCount64;
    use windows_sys::Win32::UI::Input::{RAWHID, RAWINPUT, RIM_TYPEHID};

    let Some(read) = whole_of(packet) else {
        return;
    };
    // SAFETY: the buffer was filled by the call above and is at least as
    // large as the header it starts with.
    let raw = unsafe { &*read.as_ptr().cast::<RAWINPUT>() };
    if raw.header.dwType != RIM_TYPEHID {
        return;
    }
    // SAFETY: the same buffer, read on the side the type above names.
    let hid = unsafe { &raw.data.hid };
    let (each, how_many) = (hid.dwSizeHid as usize, hid.dwCount as usize);
    if each == 0 {
        return;
    }
    // The reports themselves, which the system lays end to end after the
    // header. Bounded by what was really read rather than by what the
    // header claims: a length taken on trust is a length that walks off
    // the end of the buffer.
    let from = std::mem::offset_of!(RAWINPUT, data) + std::mem::offset_of!(RAWHID, bRawData);
    let there = read.len().saturating_sub(from) / each;

    let mut pad = PAD.lock().expect("pavé décrit");
    if pad
        .as_ref()
        .is_none_or(|known| known.device != raw.header.hDevice as isize)
    {
        *pad = described(raw.header.hDevice);
    }
    let Some(pad) = pad.as_ref() else {
        return;
    };

    for report in 0..how_many.min(there) {
        let at = from + report * each;
        let mut one = read[at..at + each].to_vec();
        let Some((fingers, across, down)) = FRAME
            .lock()
            .expect("trame du pavé")
            .gathering(pad, &mut one)
        else {
            continue;
        };
        counted::FRAMES.fetch_add(1, Ordering::Relaxed);
        if fingers >= THREE {
            counted::THREES.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: no argument, and it answers with a count of
        // milliseconds since the machine started.
        let now = unsafe { GetTickCount64() };
        let made = HAND
            .lock()
            .expect("lecture du pavé")
            .saw(fingers, across, down, now);
        if let Some(gesture) = made {
            counted::GESTURES.fetch_add(1, Ordering::Relaxed);
            crate::floating::the_pad_said(gesture);
        }
    }
}

/// The whole of one raw input packet, asked for at the size the system
/// says it is.
#[cfg(windows)]
fn whole_of(packet: *mut core::ffi::c_void) -> Option<Vec<u8>> {
    use windows_sys::Win32::UI::Input::{GetRawInputData, RAWINPUTHEADER, RID_INPUT};

    let header = size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    // SAFETY: the slot is ours, and nothing is written while no buffer is
    // named.
    let asked =
        unsafe { GetRawInputData(packet, RID_INPUT, std::ptr::null_mut(), &mut size, header) };
    if asked != 0 || size == 0 {
        return None;
    }
    // Kept in words rather than in bytes: what is read back begins with a
    // structure the machine expects aligned, and a buffer of bytes is
    // aligned to one.
    let mut room = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
    // SAFETY: the buffer is ours and at least as large as the size handed
    // to the call, which is the one the call asked for.
    let read = unsafe {
        GetRawInputData(
            packet,
            RID_INPUT,
            room.as_mut_ptr().cast(),
            &mut size,
            header,
        )
    };
    if read == u32::MAX || read == 0 {
        return None;
    }
    // SAFETY: `read` bytes of that buffer were filled by the call above.
    let bytes =
        unsafe { std::slice::from_raw_parts(room.as_ptr().cast::<u8>(), read as usize) }.to_vec();
    Some(bytes)
}

/// The pad as the system describes it, prepared once for the whole
/// reading.
#[cfg(windows)]
struct Pad {
    /// The device this was worked out for.
    device: isize,
    /// What the system's own parser needs, kept in words for the same
    /// reason as above.
    preparsed: Vec<u64>,
    /// The collections that carry one finger each.
    fingers: Vec<u16>,
    /// The pad's own ruler, across and down.
    across: (i32, i32),
    down: (i32, i32),
}

#[cfg(windows)]
impl Pad {
    /// The system's own parser, as the calls that want it name it: a
    /// plain number, the thing behind it being none of our business.
    fn parser(&self) -> isize {
        self.preparsed.as_ptr() as isize
    }

    /// Where a value sits between the two ends of the pad's own ruler, in
    /// thousandths.
    ///
    /// Thousandths because every pad has its own ruler: a threshold in
    /// what a pad reports would be a different gesture on every laptop.
    fn between(range: (i32, i32), value: i32) -> i32 {
        let (low, high) = range;
        if high <= low {
            return 0;
        }
        ((i64::from(value.clamp(low, high) - low) * 1000) / i64::from(high - low)) as i32
    }
}

/// Works out what a pad reports, from what the system knows of it.
#[cfg(windows)]
fn described(device: windows_sys::Win32::Foundation::HANDLE) -> Option<Pad> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidP_GetCaps, HidP_GetValueCaps,
        HidP_Input,
    };

    let preparsed = what_the_system_knows(device)?;
    let parser = preparsed.as_ptr() as isize;
    let mut caps: HIDP_CAPS = unsafe { std::mem::zeroed() };
    // SAFETY: the parser comes from the system and the slot is ours.
    if unsafe { HidP_GetCaps(parser, &mut caps) } != HIDP_STATUS_SUCCESS {
        return None;
    }
    let mut described = vec![
        unsafe { std::mem::zeroed::<HIDP_VALUE_CAPS>() };
        usize::from(caps.NumberInputValueCaps).max(1)
    ];
    let mut how_many = caps.NumberInputValueCaps;
    // SAFETY: the buffer is ours and as long as the count handed with it.
    if unsafe { HidP_GetValueCaps(HidP_Input, described.as_mut_ptr(), &mut how_many, parser) }
        != HIDP_STATUS_SUCCESS
    {
        return None;
    }

    let mut fingers = Vec::new();
    let mut across = None;
    let mut down = None;
    for one in described.iter().take(usize::from(how_many)) {
        let ruler = (one.LogicalMin, one.LogicalMax);
        match (one.UsagePage, named(one)) {
            // A finger is known by the one thing every finger has and
            // nothing else does: an identifier that follows it while it
            // stays down. Where it is and whether it touches are read
            // from the collection that identifier belongs to.
            (usage::DIGITIZER, usage::A_FINGER) => fingers.push(one.LinkCollection),
            (usage::DESKTOP, usage::ACROSS) => across = across.or(Some(ruler)),
            (usage::DESKTOP, usage::DOWN) => down = down.or(Some(ruler)),
            _ => {}
        }
    }
    if fingers.is_empty() {
        return None;
    }
    Some(Pad {
        device: device as isize,
        preparsed,
        fingers,
        across: across?,
        down: down?,
    })
}

/// What one described value is called, whichever way it was written down.
#[cfg(windows)]
fn named(one: &windows_sys::Win32::Devices::HumanInterfaceDevice::HIDP_VALUE_CAPS) -> u16 {
    // SAFETY: the two sides of the union are told apart by the flag the
    // system sets beside them, and only the named side is read.
    unsafe {
        if one.IsRange {
            one.Anonymous.Range.UsageMin
        } else {
            one.Anonymous.NotRange.Usage
        }
    }
}

/// What the system's own parser needs to read this device's reports.
#[cfg(windows)]
fn what_the_system_knows(device: windows_sys::Win32::Foundation::HANDLE) -> Option<Vec<u64>> {
    use windows_sys::Win32::UI::Input::{GetRawInputDeviceInfoW, RIDI_PREPARSEDDATA};

    let mut size = 0u32;
    // SAFETY: the slot is ours, and nothing is written while no buffer is
    // named.
    let asked = unsafe {
        GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, std::ptr::null_mut(), &mut size)
    };
    if asked != 0 || size == 0 {
        return None;
    }
    let mut room = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
    // SAFETY: the buffer is ours and at least as large as the size handed
    // to the call.
    let read = unsafe {
        GetRawInputDeviceInfoW(
            device,
            RIDI_PREPARSEDDATA,
            room.as_mut_ptr().cast(),
            &mut size,
        )
    };
    (read != u32::MAX && read != 0).then_some(room)
}

/// One frame of the pad, put together from the reports that carry it.
///
/// A pad with more fingers on it than one report holds says how many
/// there are in the first report of a frame and nought in the ones that
/// finish it. So a nought means two different things, all fingers gone
/// and there is more coming, and only whether a frame is still short of
/// its fingers tells the two apart.
#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct Frame {
    wanted: u32,
    seen: u32,
    across: i64,
    down: i64,
}

#[cfg(windows)]
impl Frame {
    const fn new() -> Self {
        Self {
            wanted: 0,
            seen: 0,
            across: 0,
            down: 0,
        }
    }

    /// Adds one report, and hands back the frame when it is whole.
    fn gathering(&mut self, pad: &Pad, report: &mut [u8]) -> Option<(u32, i32, i32)> {
        let told = how_many_fingers(pad, report);
        // A frame begins when the one being put together has all its
        // fingers, and again whenever a report announces a count of its
        // own: only the first report of a frame carries one, so that is
        // what a new frame looks like. Without the second half, a pad
        // that announced more fingers than it ever sent would leave a
        // frame open for the rest of the session.
        if self.seen >= self.wanted || told.is_some_and(|how_many| how_many > 0) {
            *self = Self::new();
            self.wanted = told.unwrap_or(0);
        }

        for finger in &pad.fingers {
            let Some((across, down)) = a_finger(pad, *finger, report) else {
                continue;
            };
            self.seen += 1;
            self.across += i64::from(across);
            self.down += i64::from(down);
        }
        // A pad that never says how many fingers it has is read one
        // report at a time, which is what a pad with room for all of them
        // at once comes to anyway.
        if told.is_none() {
            self.wanted = self.seen;
        }
        if self.seen < self.wanted {
            return None;
        }
        let whole = match self.seen {
            // The hand has left the pad, which is the one answer with no
            // place to it.
            0 => (0, 0, 0),
            fingers => (
                fingers,
                Pad::between(pad.across, (self.across / i64::from(fingers)) as i32),
                Pad::between(pad.down, (self.down / i64::from(fingers)) as i32),
            ),
        };
        *self = Self::new();
        Some(whole)
    }
}

/// How many fingers the pad says are on it, `None` when this report does
/// not say.
#[cfg(windows)]
fn how_many_fingers(pad: &Pad, report: &mut [u8]) -> Option<u32> {
    a_value(pad, 0, usage::DIGITIZER, usage::HOW_MANY, report)
}

/// Where one finger is, `None` when it is not touching the pad.
#[cfg(windows)]
fn a_finger(pad: &Pad, finger: u16, report: &mut [u8]) -> Option<(i32, i32)> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HIDP_STATUS_SUCCESS, HidP_GetUsages, HidP_Input,
    };

    // Whether a finger is really down is a button and not a number, so it
    // is read as one: the pad names every button that is set inside that
    // finger's collection, and touching is one of them.
    let mut set = [0u16; 8];
    let mut how_many = set.len() as u32;
    // SAFETY: the buffers are ours and as long as the counts handed with
    // them, and the parser comes from the system.
    let asked = unsafe {
        HidP_GetUsages(
            HidP_Input,
            usage::DIGITIZER,
            finger,
            set.as_mut_ptr(),
            &mut how_many,
            pad.parser(),
            report.as_mut_ptr(),
            report.len() as u32,
        )
    };
    if asked != HIDP_STATUS_SUCCESS || !set[..how_many as usize].contains(&usage::TOUCHING) {
        return None;
    }
    let across = a_value(pad, finger, usage::DESKTOP, usage::ACROSS, report)?;
    let down = a_value(pad, finger, usage::DESKTOP, usage::DOWN, report)?;
    Some((across as i32, down as i32))
}

/// One number of one collection of a report.
#[cfg(windows)]
fn a_value(pad: &Pad, collection: u16, page: u16, usage: u16, report: &mut [u8]) -> Option<u32> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HIDP_STATUS_SUCCESS, HidP_GetUsageValue, HidP_Input,
    };

    let mut value = 0u32;
    // SAFETY: the slot is ours, the report is ours, and the parser comes
    // from the system.
    let asked = unsafe {
        HidP_GetUsageValue(
            HidP_Input,
            page,
            collection,
            usage,
            &mut value,
            pad.parser(),
            report.as_ptr(),
            report.len() as u32,
        )
    };
    (asked == HIDP_STATUS_SUCCESS).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Des doigts posés puis relevés, aux temps dits.
    fn tapped(reading: &mut Reading, fingers: u32, from: u64, to: u64) -> Vec<Gesture> {
        let mut made = Vec::new();
        made.extend(reading.saw(fingers, 500, 500, from));
        made.extend(reading.saw(0, 500, 500, to));
        made
    }

    /// Trois doigts glissés de `from` à `to`, un dixième de pavé à la
    /// fois, comme un vrai pavé les rapporte.
    fn slid(reading: &mut Reading, from: i32, to: i32) -> Vec<Gesture> {
        let mut made = Vec::new();
        let step = if to > from { 20 } else { -20 };
        let mut x = from;
        let mut at = 0;
        made.extend(reading.saw(3, x, 500, at));
        while x != to {
            x += step;
            at += 8;
            made.extend(reading.saw(3, x, 500, at));
        }
        made.extend(reading.saw(0, x, 500, at + 8));
        made
    }

    #[test]
    fn three_fingers_put_down_and_lifted_are_a_tap() {
        let mut reading = Reading::new();
        assert_eq!(tapped(&mut reading, 3, 0, 90), vec![Gesture::ThreeTap]);
    }

    #[test]
    fn four_fingers_put_down_and_lifted_are_their_own_tap() {
        // Celui-là ne veut pas dire la même chose, et il ne doit surtout
        // pas se confondre avec celui d'à côté : un clic molette envoyé
        // là où on demandait lecture ou pause se voit tout de suite.
        let mut reading = Reading::new();
        assert_eq!(tapped(&mut reading, 4, 0, 90), vec![Gesture::FourTap]);
    }

    #[test]
    fn a_fourth_finger_landing_late_still_makes_a_four_finger_tap() {
        // Quatre doigts ne touchent pas le pavé au même instant : compté
        // sur la première trame, un appui à quatre doigts sur trois en
        // ferait un à trois, donc un clic molette.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(3, 500, 500, 0));
        made.extend(reading.saw(4, 500, 500, 8));
        made.extend(reading.saw(0, 500, 500, 60));
        assert_eq!(made, vec![Gesture::FourTap]);
    }

    #[test]
    fn three_fingers_left_down_are_not_a_tap() {
        // Une main posée sur le pavé pendant qu'on lit l'écran n'a rien
        // demandé, et un clic molette envoyé là-bas se voit.
        let mut reading = Reading::new();
        assert_eq!(tapped(&mut reading, 3, 0, AT_MOST_A_TAP + 1), vec![]);
    }

    #[test]
    fn a_tap_may_drift_a_little() {
        // Trois doigts ne se posent ni ne se lèvent ensemble au pixel
        // près : une immobilité parfaite ne serait jamais atteinte.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(3, 500, 500, 0));
        made.extend(reading.saw(3, 500 + STILL, 500, 40));
        made.extend(reading.saw(0, 500 + STILL, 500, 80));
        assert_eq!(made, vec![Gesture::ThreeTap]);
    }

    #[test]
    fn a_slide_is_one_window_for_each_step() {
        // Un balayage ordinaire change une fenêtre, un long glissement en
        // change plusieurs : c'est ce qu'une main qui continue demande.
        let mut reading = Reading::new();
        assert_eq!(
            slid(&mut reading, 100, 100 + A_STEP),
            vec![Gesture::Rightwards]
        );

        let mut reading = Reading::new();
        assert_eq!(
            slid(&mut reading, 100, 100 + 3 * A_STEP),
            vec![
                Gesture::Rightwards,
                Gesture::Rightwards,
                Gesture::Rightwards,
            ]
        );
    }

    #[test]
    fn a_slide_the_other_way_is_the_other_way() {
        let mut reading = Reading::new();
        assert_eq!(
            slid(&mut reading, 900, 900 - A_STEP),
            vec![Gesture::Leftwards]
        );
    }

    #[test]
    fn a_hand_that_comes_back_undoes_its_own_windows() {
        // Une main qui est allée trop loin revient : les crans sont
        // comptés depuis le point de départ, donc le retour rend ce que
        // l'aller avait pris.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(3, 100, 500, 0));
        made.extend(reading.saw(3, 100 + A_STEP, 500, 40));
        made.extend(reading.saw(3, 100, 500, 80));
        made.extend(reading.saw(0, 100, 500, 120));
        assert_eq!(made, vec![Gesture::Rightwards, Gesture::Leftwards]);
    }

    #[test]
    fn a_slide_is_never_a_tap_as_well() {
        // Le geste s'est déclaré en glissement : la main qui se lève au
        // bout ne doit pas envoyer un clic molette par-dessus.
        let mut reading = Reading::new();
        let made = slid(&mut reading, 100, 100 + A_STEP);
        assert!(!made.contains(&Gesture::ThreeTap));
    }

    #[test]
    fn a_swipe_up_or_down_belongs_to_windows() {
        // Ces deux-là montrent le bureau et les fenêtres ouvertes de cet
        // ordinateur-ci, et rien de ce que le produit porte ne leur
        // correspond là-bas.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        for step in 0..10 {
            made.extend(reading.saw(3, 500 + step * 5, 900 - step * 60, (step * 8) as u64));
        }
        made.extend(reading.saw(0, 545, 360, 100));
        assert_eq!(made, vec![]);
    }

    #[test]
    fn a_vertical_swipe_that_ends_sideways_changes_nothing_either() {
        // Le sens se décide une fois, au premier cran : sans ça, la fin
        // d'un balayage vertical un peu de travers changerait de fenêtre.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(3, 500, 900, 0));
        made.extend(reading.saw(3, 500 + A_STEP, 300, 40));
        made.extend(reading.saw(3, 500 + 3 * A_STEP, 300, 80));
        made.extend(reading.saw(0, 500 + 3 * A_STEP, 300, 120));
        assert_eq!(made, vec![]);
    }

    #[test]
    fn a_four_finger_slide_is_left_alone() {
        // Quatre doigts qui glissent changent de bureau virtuel sur cet
        // ordinateur-ci, et le produit n'a rien à en faire : seul l'appui
        // lui est repris.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(4, 100, 500, 0));
        made.extend(reading.saw(4, 100 + 2 * A_STEP, 500, 40));
        made.extend(reading.saw(0, 100 + 2 * A_STEP, 500, 80));
        assert_eq!(made, vec![]);
    }

    #[test]
    fn a_fourth_finger_landing_on_a_slide_gives_it_back_to_windows() {
        // Un glissement à trois doigts qu'un quatrième rejoint est un
        // geste de Windows depuis cet instant : continuer à compter des
        // crans changerait de fenêtre là-bas pendant qu'on change de
        // bureau ici.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(3, 100, 500, 0));
        made.extend(reading.saw(3, 100 + A_STEP, 500, 40));
        made.extend(reading.saw(4, 100 + 2 * A_STEP, 500, 80));
        made.extend(reading.saw(4, 100 + 3 * A_STEP, 500, 120));
        made.extend(reading.saw(0, 100 + 3 * A_STEP, 500, 160));
        assert_eq!(made, vec![Gesture::Rightwards]);
    }

    #[test]
    fn five_fingers_are_left_alone() {
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(5, 500, 500, 0));
        made.extend(reading.saw(0, 500, 500, 60));
        assert_eq!(made, vec![]);
    }

    #[test]
    fn two_fingers_are_left_alone_too() {
        // Deux doigts font défiler et cliquent droit, et les deux passent
        // déjà par le flux comme une souris ordinaire.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(2, 100, 500, 0));
        made.extend(reading.saw(2, 100 + 2 * A_STEP, 500, 40));
        made.extend(reading.saw(0, 100 + 2 * A_STEP, 500, 80));
        assert_eq!(made, vec![]);
    }

    #[test]
    fn a_finger_landing_late_does_not_end_the_gesture() {
        // Trois doigts ne touchent pas le pavé au même instant, et ne le
        // quittent pas au même instant non plus : lu comme une fin, tout
        // geste finirait au milieu.
        let mut reading = Reading::new();
        let mut made = Vec::new();
        made.extend(reading.saw(1, 100, 500, 0));
        made.extend(reading.saw(3, 100, 500, 8));
        made.extend(reading.saw(2, 100 + A_STEP / 2, 500, 16));
        made.extend(reading.saw(3, 100 + A_STEP, 500, 24));
        made.extend(reading.saw(2, 100 + A_STEP, 500, 32));
        made.extend(reading.saw(0, 100 + A_STEP, 500, 40));
        assert_eq!(made, vec![Gesture::Rightwards]);
    }

    #[test]
    fn one_gesture_never_leaks_into_the_next() {
        // La main se lève et repose : le second geste part de zéro, sans
        // quoi le premier lui laisserait ses crans.
        let mut reading = Reading::new();
        assert_eq!(
            slid(&mut reading, 100, 100 + A_STEP),
            vec![Gesture::Rightwards]
        );
        assert_eq!(tapped(&mut reading, 3, 200, 260), vec![Gesture::ThreeTap]);
    }

    #[test]
    fn what_windows_holds_says_what_to_do_about_it() {
        // Les mots de la page de Windows et non les nôtres : ce sont
        // ceux-là qu'on cherche des yeux en y arrivant.
        let all = Held {
            slide: true,
            tap: true,
            four_tap: true,
        };
        assert!(all.any());
        assert!(all.what_to_do().contains("« Balayages » et « Appuis »"));
        assert!(all.what_to_do().contains("quatre doigts"));

        let one = Held {
            slide: false,
            tap: true,
            four_tap: false,
        };
        assert!(one.any());
        assert!(one.what_to_do().contains("trois doigts : « Appuis »"));
        assert!(!one.what_to_do().contains("Balayages"));
        assert!(!one.what_to_do().contains("quatre doigts"));

        // Et celui de quatre doigts seul ne parle pas des trois.
        let four = Held {
            slide: false,
            tap: false,
            four_tap: true,
        };
        assert!(four.any());
        assert!(!four.what_to_do().contains("trois doigts"));

        assert!(
            !Held {
                slide: false,
                tap: false,
                four_tap: false
            }
            .any()
        );
    }
}
