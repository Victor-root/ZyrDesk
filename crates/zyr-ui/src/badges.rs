//! The two badges of a session, in the top left corner of the picture.
//!
//! Two small badges laid over the picture, opposite the floating button,
//! which only light up when there is something to say: one for the link,
//! when the picture freezes or frames get lost on the way, the other for
//! the picture itself, when one of the two computers can no longer keep
//! up with encoding or decoding.
//!
//! The second one says which of the two. It carries the two screens of
//! the product's logo, the far one behind and this one in front, both
//! dimmed, and lights up again the one that is struggling; both when both
//! are struggling. It reads without a caption since it is the brand's own
//! drawing, and it fits in eighteen pixels where a word would not.
//!
//! What they replace. The client engine has a warning of its own for the
//! first of the two: red letters at thirty-six points, burnt into the
//! frames it decodes, in a colour and a size it chose. Two things are
//! wrong with it and neither is a matter of taste. It is drawn into the
//! picture, which is the far computer's desktop and not ours to write on;
//! and it arrives late, several seconds after the person has watched the
//! picture stop, because it is worked out over a window of its own on the
//! far side of a decoder that has nothing to decode. The engine's switch
//! for it is thrown at the session's start (patch P-M16) and this says it
//! instead.
//!
//! What makes it quick. The engine writes down what a session costs, and
//! lately it writes it five times a second rather than once, with one
//! more number: how long the picture has not moved. That one is not
//! averaged and not waited for, it is true the moment it is read, and it
//! is what lights the first badge before the hand has had time to move
//! the mouse to check.
//!
//! What decides is not Windows code and compiles everywhere: it is
//! arithmetic on a reading, and it is the only half a test can say
//! anything about. The badges, for their part, are a window, so Windows
//! code, like the session.

// Outside Windows there is no picture to cover, but what decides is
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use crate::measures::Measures;

/* ---- Ce qu'une lecture dit ------------------------------------------- */

/// What this module files its journal lines under.
const TAG: &str = "voyants";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// How long the picture must have been frozen for it to show.
///
/// A third of a second. Below that, it is one of the late frames that go
/// by all the time, and a badge that blinks at that pace no longer means
/// anything; above it, the person has already noticed and the badge
/// arrives after them.
const FROZEN_MS: u64 = 350;

/// How many frames lost on the way, as a percentage of the second gone
/// by, before it is said.
///
/// Two percent: one frame in fifty, which shows on a desktop being
/// scrolled and does not show on a still desktop.
const LOST_PCT: f64 = 2.0;

/// And how many arriving too late to be shown.
///
/// Higher than for the ones before: these did arrive, and what they
/// say is that the link is shaking rather than losing.
const TOO_LATE_PCT: f64 = 5.0;

/// How long a badge stays lit after its cause has stopped.
///
/// Without this it blinks: the cause holds for one reading, readings
/// arrive five times a second, and a network that is doing badly does
/// badly in fits and starts. A second and a half is what it takes for
/// the person, looking up at the corner of the picture, to still find
/// something there.
const HOLDS: Duration = Duration::from_millis(1500);

/// What a badge can say.
///
/// Three and not two, for two badges: the picture one carries both
/// computers and lights the one that is struggling, so it counts as
/// two here and as one on screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    /// The link between the two computers.
    Link,
    /// The picture as the far computer makes it.
    Far,
    /// And as this one makes it again.
    Here,
}

impl std::fmt::Display for Which {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Which::Link => "lien",
            Which::Far => "image là-bas",
            Which::Here => "image ici",
        })
    }
}

/// What a reading says about each one: nothing, or what is wrong.
///
/// The words and not only the fact: a badge that lights up with
/// nothing saying why is a badge people end up ignoring, and the
/// journal is the only place where the reason fits.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Reads {
    pub link: Option<String>,
    pub far: Option<String>,
    pub here: Option<String>,
}

/// What a reading says, with no memory of any kind.
///
/// The time available for a frame is worked out from the measured frame
/// rate and not from the one asked for, and that is not a second best:
/// what we are after is whether one of the two computers is what sets
/// the pace. A host that takes twenty-five milliseconds per frame serves
/// forty frames a second, so its encoding time **is** the time
/// available, and the badge lights up; the same host at three
/// milliseconds on a thirty-frame session has thirty-three milliseconds
/// ahead of it and gets in nobody's way. Asking for the wanted frame
/// rate would have cost a round trip to the service on every reading,
/// and would have been wrong as soon as someone changes it during the
/// session.
pub fn read(measures: &Measures) -> Reads {
    let mut reads = Reads::default();

    if let Some(frozen) = measures.since_frame_ms.filter(|held| *held >= FROZEN_MS) {
        reads.link = Some(format!("l'image est figée depuis {frozen} ms"));
    } else if let Some(lost) = measures.dropped_network_pct.filter(|pct| *pct >= LOST_PCT) {
        reads.link = Some(format!("{lost:.1} % des images se perdent en route"));
    } else if let Some(late) = measures
        .dropped_jitter_pct
        .filter(|pct| *pct >= TOO_LATE_PCT)
    {
        reads.link = Some(format!("{late:.1} % des images arrivent trop tard"));
    }

    // A missing frame rate leaves these two off: without it there is no
    // time available, so nothing to compare, and a badge lit for want of
    // a measure would be a badge lit for nothing.
    //
    // Each of the two is weighed on its own and not one or the other:
    // they may very well struggle together, on two tired machines or on a
    // session too big for both, and the badge can say so.
    if let Some(budget) = measures
        .fps
        .filter(|rate| *rate > 0.0)
        .map(|rate| 1000.0 / rate)
    {
        if let Some(host) = measures.host_ms.filter(|each| *each >= budget) {
            reads.far = Some(format!(
                "l'ordinateur d'en face met {host:.0} ms par image, \
                 pour {budget:.0} ms disponibles"
            ));
        }
        if let Some(decode) = measures.decode_ms.filter(|each| *each >= budget) {
            reads.here = Some(format!(
                "cet ordinateur met {decode:.0} ms à décoder une image, \
                 pour {budget:.0} ms disponibles"
            ));
        }
    }
    reads
}

/// What the badges show, once the reading has been calmed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Shown {
    pub link: bool,
    pub far: bool,
    pub here: bool,
}

impl Shown {
    /// Nothing at all.
    pub fn nothing(self) -> bool {
        !self.link && !self.far && !self.here
    }
}

/// What keeps the badges lit from one reading to the next.
///
/// One thing only, and it is all that separates a useful badge from a
/// string of fairy lights: lit on the reading that says so, off only
/// once nothing has said so for a while.
#[derive(Default)]
pub struct Steady {
    link: Option<Instant>,
    far: Option<Instant>,
    here: Option<Instant>,
}

impl Steady {
    /// What this reading leaves lit.
    pub fn after(&mut self, reads: &Reads, now: Instant) -> Shown {
        Shown {
            link: still(&mut self.link, reads.link.is_some(), now),
            far: still(&mut self.far, reads.far.is_some(), now),
            here: still(&mut self.here, reads.here.is_some(), now),
        }
    }
}

/// One badge, lit again for a while when its cause is
/// there.
fn still(until: &mut Option<Instant>, wrong: bool, now: Instant) -> bool {
    if wrong {
        *until = Some(now + HOLDS);
    }
    until.is_some_and(|end| now < end)
}

/* ---- La boucle qui les tient ----------------------------------------- */

/// How many times a second the reading is read again.
///
/// The engine writes five; this loop reads a little more often, so
/// that the delay added on this side is smaller than the one on the
/// other. The aim is for a badge to be there within the third of a
/// second that follows what it reports.
const LOOK_EVERY: Duration = Duration::from_millis(80);

#[cfg(windows)]
static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Follows the session's health until the session ends.
///
/// Called on every round of the floating button's watch, like the
/// pointer's shape: it does nothing while a loop is already running, and
/// starts one again when the previous one has stopped.
#[cfg(windows)]
pub fn watch(app: &crate::app::App) {
    use std::sync::atomic::Ordering;

    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        keep_up(&app).await;
        show(&app, Shown::default(), &Reads::default(), false);
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
pub fn watch(_app: &crate::app::App) {}

/// The loop itself.
#[cfg(windows)]
async fn keep_up(app: &crate::app::App) {
    // From when on a reading belongs to this session. The file outlives
    // the session that wrote it, and a session opening would find it as
    // the previous one left it, so with a picture frozen for hours: a
    // badge lit on the first frame of a perfectly healthy session.
    let started = std::time::SystemTime::now();
    let mut steady = Steady::default();
    let mut was = Shown::default();
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            return;
        }
        let held = crate::floating::the_badges_are_held_up(app);
        let Some(measures) = fresh(started) else {
            // Held on screen, they are there even before the engine
            // over there has written a single reading: what is being
            // looked at then is the badges themselves, and a session
            // whose readings have not started is precisely the moment
            // someone looks at them.
            show(app, Shown::default(), &Reads::default(), held);
            continue;
        };
        let reads = read(&measures);
        let now = Instant::now();
        let shown = steady.after(&reads, now);
        if shown != was {
            said(&reads, was, shown);
            was = shown;
        }
        // Said on every round and not only on a change: this loop starts
        // before the badges' window exists, and a badge lit during that
        // time would never again get the chance to change its mind. What
        // is said twice costs nothing: the window keeps what it shows and
        // only redraws on a real difference.
        show(app, shown, &reads, held);
    }
}

/// The reading, if it belongs to this session.
#[cfg(windows)]
fn fresh(started: std::time::SystemTime) -> Option<Measures> {
    let path = zyr_proto::paths::session_stats();
    // The file's time rather than its contents: nothing in the line says
    // which session wrote it, and its age says so without adding
    // anything to what the engine writes.
    let written = std::fs::metadata(&path).ok()?.modified().ok()?;
    if written < started {
        return None;
    }
    Some(crate::measures::session_measures())
}

/// Says what has just changed, and nothing else.
///
/// One line when a badge lights up and one when it goes out, never one
/// per reading: a dozen of those go by every second, and a journal that
/// carried them all would carry nothing else.
#[cfg(windows)]
fn said(reads: &Reads, was: Shown, shown: Shown) {
    for (which, before, after, why) in [
        (Which::Link, was.link, shown.link, &reads.link),
        (Which::Far, was.far, shown.far, &reads.far),
        (Which::Here, was.here, shown.here, &reads.here),
    ] {
        if before == after {
            continue;
        }
        note(&match (after, why) {
            (true, Some(why)) => format!("voyant {which} : {why}"),
            // Lit with no reason in this reading: the cause came and
            // went between two readings and the badge is still
            // holding.
            (true, None) => format!("voyant {which} allumé"),
            (false, _) => format!("voyant {which} éteint"),
        });
    }
}

/* ---- Les pastilles elles-mêmes --------------------------------------- */

/// The side of a badge, in page pixels.
///
/// A tenth less than the floating button, which is forty-four. Close
/// enough for them to look like the same family, far enough for one not
/// to be taken for the other: that button is what the hand aims at, these
/// are only for reading. Smaller, they could not be read: the picture one
/// carries two screens of which only one is lit, and it takes room for
/// the two to be told apart.
const BADGE: f32 = 40.0;

/// What separates the two.
const BETWEEN: f32 = 8.0;

/// The margin around them in the window.
///
/// Enough for the edge of a badge not to touch the window's, and it
/// is the same margin that serves as the bubble's gutter.
const ROOM: f32 = 8.0;

/// What separates the drawing from the edge of its badge.
const INSET: f32 = 7.0;

/// The width of the bubble that says why, in page pixels.
const BUBBLE: f32 = 260.0;

/// What separates it from the badges.
const UNDER: f32 = 6.0;

/// Its inner margin.
const PADDING: f32 = 10.0;

/// The radius of its corners.
const CORNER: f32 = 8.0;

/// The size of what is written in it.
const WORDS: f32 = 12.0;

/// The most height the bubble can take.
///
/// Set aside in the window without necessarily being drawn: the true
/// height of a text can only be measured once the canvas is made, and the
/// window is sized before it. Three lines are enough for the sentences
/// these two have to say, and what is left over is clear, so invisible.
const BUBBLE_AT_MOST: f32 = 2.0 * PADDING + 3.0 * 17.0;

/// The radius of a badge's corners: half its side, so a circle.
const ROUNDED: f32 = BADGE / 2.0;

/// The thickness of the ring around a badge.
///
/// It is what carries the state: dark when the badge has nothing to say,
/// the warning colour when it has. Two points and not one, because a
/// one-point line reads as an edge and not as a badge, and between the
/// two there is still all the room the drawing needs.
const RING: f32 = 2.0;

/// The window, and what it shows.
#[cfg(windows)]
static ITS_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static LIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// What that number means.
///
/// Three bits for two badges: the picture one is there as soon as either
/// of the two computers is struggling, and lights whichever of the two
/// is struggling.
#[cfg(windows)]
mod bit {
    pub const LINK: u8 = 1;
    pub const FAR: u8 = 2;
    pub const HERE: u8 = 4;
    pub const PICTURE: u8 = FAR | HERE;
    /// The two badges held on screen, lit or not.
    ///
    /// Kept with the others and not beside them, because it is the same
    /// question: this number says what the window shows, and what is
    /// shown is no longer only what is lit.
    pub const HELD: u8 = 8;
    /// The badge the hand is resting on, if there is one.
    ///
    /// Kept here with the rest because it is the same question again:
    /// this number says what the window shows, and a bubble open under a
    /// badge is part of it. Being in it also earns it a redraw when the
    /// hand arrives and when it leaves, without anything else having to
    /// see to it.
    pub const OVER_LINK: u8 = 16;
    pub const OVER_PICTURE: u8 = 32;
    pub const OVER: u8 = OVER_LINK | OVER_PICTURE;
}

/// What the badges have to say, word for word.
///
/// Kept beside what is lit because the bubble needs it at the moment it
/// is drawn, and that moment is not the one when the reading was made.
#[cfg(windows)]
static WHY: std::sync::Mutex<Reads> = std::sync::Mutex::new(Reads {
    link: None,
    far: None,
    here: None,
});

#[cfg(windows)]
thread_local! {
    static CANVAS: std::cell::RefCell<Option<crate::paint::Canvas>> =
        const { std::cell::RefCell::new(None) };
}

/// What the window takes up, in real pixels on the screen it covers.
#[cfg(windows)]
fn its_size() -> (i32, i32) {
    let scale = crate::main_window::scale();
    // Sized for the bubble from the start, and not enlarged when it
    // opens: resizing a layered window under a passing hand would show.
    // What is not drawn only costs the compositor, which only blends
    // this window in when a badge is already there.
    let wide = (2.0 * BADGE + BETWEEN).max(BUBBLE) + 2.0 * ROOM;
    let high = BADGE + UNDER + BUBBLE_AT_MOST + 2.0 * ROOM;
    ((wide * scale).ceil() as i32, (high * scale).ceil() as i32)
}

/// Opens the badges' window, once per session.
///
/// `anchor` is the top left corner of the picture, already moved in by
/// the margin: that is where the first badge sits, and the window spills
/// over around it by what the shadow asks for.
#[cfg(windows)]
pub fn raise(app: &crate::app::App, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = crate::main_window::handle();
    LIT.store(0, Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || build(owner, anchor));
}

#[cfg(not(windows))]
pub fn raise(_app: &crate::app::App, _anchor: (i32, i32)) {}

/// Puts them away with the session.
#[cfg(windows)]
pub fn lower(app: &crate::app::App) {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    LIT.store(0, Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: a window of ours, destroyed on the thread that made
        // it.
        unsafe { DestroyWindow(window as HWND) };
    });
}

#[cfg(not(windows))]
pub fn lower(_app: &crate::app::App) {}

/// Lays them in the top left corner of the picture.
///
/// Called from where the floating button is laid, so a hundred and
/// twenty times a second under a hand that is resizing: nothing here
/// waits for anything at all.
#[cfg(windows)]
pub fn lay(anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let (left, top) = window_corner(anchor);
    // SAFETY: a window of ours, placed without being activated or
    // resized. Placing a window from another thread is asked of the
    // system, which is what makes this safe from the thread that follows
    // a hand.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

#[cfg(not(windows))]
pub fn lay(_anchor: (i32, i32)) {}

/// The window's corner, for a first badge laid there.
#[cfg(windows)]
fn window_corner(anchor: (i32, i32)) -> (i32, i32) {
    let room = (ROOM * crate::main_window::scale()).round() as i32;
    (anchor.0 - room, anchor.1 - room)
}

/// Lights what should be lit, and puts the window away when nothing is
/// any more.
///
/// Put away and not painted empty: a fully clear layered window cannot be
/// seen, but it is still a window the compositor blends into every frame
/// of the session.
#[cfg(windows)]
fn show(app: &crate::app::App, shown: Shown, reads: &Reads, held: bool) {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let mut lit = 0;
    for (on, bit) in [
        (shown.link, bit::LINK),
        (shown.far, bit::FAR),
        (shown.here, bit::HERE),
        (held, bit::HELD),
    ] {
        if on {
            lit |= bit;
        }
    }
    let anything = !shown.nothing() || held;
    // The hand is only looked for over badges that are there: without
    // them the window is put away, and a bubble under an invisible
    // badge would explain nothing.
    if anything {
        lit |= the_hand_over_them(window);
    }
    *WHY.lock().expect("raisons des voyants") = reads.clone();
    // Redrawn on every round while the hand is resting there, and
    // only on a change otherwise: what the bubble says carries
    // numbers that move, and a bubble that kept those of the first
    // reading would say something false the whole time it is being
    // looked at.
    let under_the_hand = lit & bit::OVER != 0;
    if LIT.swap(lit, Ordering::Relaxed) == lit && !under_the_hand {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        let window = window as HWND;
        if anything {
            repaint(window);
        }
        // SAFETY: a window of ours, shown or put away without taking
        // the foreground, on the thread that made it.
        unsafe { ShowWindow(window, if anything { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    });
}

#[cfg(not(windows))]
fn show(_app: &crate::app::App, _shown: Shown, _reads: &Reads, _held: bool) {}

/// Which of the two the hand is resting on, if either.
///
/// Read from the system rather than received as messages, and that is
/// what lets this window stay click-through. The corner it sits in
/// belongs to the far computer: a hand aiming at its Start menu must not
/// land on a badge, so clicks go through, and so do movements. Asking
/// where the pointer is takes nothing from anyone and answers the only
/// question asked.
#[cfg(windows)]
fn the_hand_over_them(window: isize) -> u8 {
    use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};

    let mut hand = POINT { x: 0, y: 0 };
    // SAFETY: a point of ours, which the system fills in.
    if unsafe { GetCursorPos(&mut hand) } == 0 {
        return 0;
    }
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, whose rectangle is read into ours.
    if unsafe { GetWindowRect(window as HWND, &mut place) } == 0 {
        return 0;
    }
    let scale = crate::main_window::scale();
    let side = (BADGE * scale).round() as i32;
    let top = place.top + (ROOM * scale).round() as i32;
    if hand.y < top || hand.y >= top + side {
        return 0;
    }
    for (rank, which) in [bit::OVER_LINK, bit::OVER_PICTURE].into_iter().enumerate() {
        let left = place.left + ((ROOM + rank as f32 * (BADGE + BETWEEN)) * scale).round() as i32;
        if hand.x >= left && hand.x < left + side {
            return which;
        }
    }
    0
}

/// What the badge under the hand has to say.
///
/// A sentence even when all is well: these badges can be held on
/// screen on request, unlit, and an empty bubble under an unlit badge
/// would suggest the question has no answer.
fn what_it_says(rank: usize, why: &Reads) -> String {
    if rank == 0 {
        return why
            .link
            .clone()
            .unwrap_or_else(|| "le lien va bien : rien ne se perd et l'image suit".to_string());
    }
    let both: Vec<&str> = [why.far.as_deref(), why.here.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if both.is_empty() {
        return "les deux ordinateurs suivent : l'image est encodée et décodée à temps".to_string();
    }
    both.join("\n")
}

/// Builds the window, hidden: it only shows itself when the first
/// badge comes on.
#[cfg(windows)]
fn build(owner: isize, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };

    let name = wide("ZyrDeskVoyants");
    let (wide_px, high) = its_size();
    let (left, top) = window_corner(anchor);
    // SAFETY: a class registered once and a window built on it, on the
    // thread that will pump its messages. A class already registered is
    // refused and nothing more, which is why the answer is not read: a
    // second session finds the one from the first.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(nothing),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name.as_ptr(),
        };
        RegisterClassW(&class);
        // Transparent to clicks, and it is the only one of these four
        // attributes worth explaining: these badges are not for clicking,
        // and the top left corner of the picture belongs to the far
        // computer. A hand aiming at its Start menu must not land on a
        // badge.
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            left,
            top,
            wide_px,
            high,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("voyants : la fenêtre n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
}

/// This window has nothing to answer: it only carries its own image,
/// and clicks go through it.
#[cfg(windows)]
unsafe extern "system" fn nothing(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    // SAFETY: called by the system on the thread that made this window,
    // with the arguments it documents.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(window, message, holding, with)
    }
}

/// Redraws the lit badges.
#[cfg(windows)]
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    use crate::design::{Colour, DARK};
    use crate::paint::Rect;

    let lit = LIT.load(Ordering::Relaxed);
    // Held on screen, both are drawn and each one still says what it
    // reads: it is where they are drawn that this changes, and never
    // what they say. Two badges always lit would show nothing of their
    // work.
    let held = lit & bit::HELD != 0;
    let scale = crate::main_window::scale();
    let (wide_px, high) = its_size();
    CANVAS.with_borrow_mut(|canvas| {
        // Made again when the screen's magnification has changed: the
        // canvas is an image of a given size, and the window has
        // followed.
        if canvas
            .as_ref()
            .is_none_or(|had| had.size() != (wide_px, high))
        {
            *canvas = crate::paint::Canvas::new(wide_px, high);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };
        canvas.begin(Colour::TRANSPARENT);
        // Each one has its place and keeps it, even when the other is off:
        // the picture one stays second, with a gap on its left where the
        // link one would be. Packed against each other, the second would
        // jump places every time the first lights up, and a badge that
        // moves is a badge people read again instead of recognising it.
        // The gap does not show: the window is clear wherever nothing is
        // drawn.
        for (rank, on) in [lit & bit::LINK != 0, lit & bit::PICTURE != 0]
            .into_iter()
            .enumerate()
        {
            if !on && !held {
                continue;
            }
            let left = (ROOM + rank as f32 * (BADGE + BETWEEN)) * scale;
            let dot = Rect::at(left, ROOM * scale, BADGE * scale, BADGE * scale);
            let radius = ROUNDED * scale;
            // No drop shadow, and it is the edge that says it all. These
            // badges float over another computer's desktop, which can be
            // any colour: a shadow there is invisible on a black
            // background and makes a grey smudge on a light one, which is
            // the opposite of what it was asked for.
            //
            // Two lines rather than one, each for one background: the
            // thick ring, dark or the warning colour, stands out from a
            // light desktop; the light hairline laid just outside it sets
            // the badge apart from a black desktop, where the ring alone
            // would melt in. Only one of the two shows at a time, and
            // that is why it takes two.
            let ring = if on { DARK.warning } else { DARK.border_strong };
            canvas.fill(dot, radius, DARK.background.faded(0.94));
            canvas.stroke_inside(dot, radius, RING * scale, ring);
            canvas.stroke_on(
                dot.grown(scale / 2.0),
                radius + scale / 2.0,
                scale,
                DARK.text.faded(0.16),
            );
            let icon_area = dot.grown(-INSET * scale);
            if rank == 0 {
                let colour = if on { DARK.warning } else { DARK.text_faint };
                canvas.icon(&crate::icons::LINK, icon_area, colour);
                continue;
            }
            // The picture badge carries both computers, the far one
            // behind and this one in front, as the product's logo draws
            // them. Both are laid down dimmed, then the one that is
            // struggling is drawn over again brightly: that is all it
            // takes to say which of the two, and it reads without a
            // caption since it is the brand's own drawing.
            canvas.icon(&crate::icons::HOST_SCREEN, icon_area, DARK.text_faint);
            if lit & bit::FAR != 0 {
                canvas.icon(&crate::icons::SCREEN_OVER_THERE, icon_area, DARK.warning);
            }
            if lit & bit::HERE != 0 {
                canvas.icon(&crate::icons::SCREEN_HERE, icon_area, DARK.warning);
            }
        }
        // The bubble after the badges, so that it goes on top should the
        // two ever touch.
        if lit & bit::OVER != 0 {
            let rank = usize::from(lit & bit::OVER_LINK == 0);
            let text = what_it_says(rank, &WHY.lock().expect("raisons des voyants"));
            let pen = crate::paint::Pen::of(WORDS * scale);
            let inside = (BUBBLE - 2.0 * PADDING) * scale;
            let bubble = Rect::at(
                ROOM * scale,
                (ROOM + BADGE + UNDER) * scale,
                BUBBLE * scale,
                canvas.height_of(&text, pen, inside) + 2.0 * PADDING * scale,
            );
            let radius = CORNER * scale;
            canvas.fill(bubble, radius, DARK.surface_1.faded(0.96));
            canvas.stroke_inside(bubble, radius, scale, DARK.border_strong);
            canvas.draw_text(&text, pen, DARK.text, bubble.grown(-PADDING * scale));
        }
        if !canvas.finish() {
            return;
        }
        let mut place = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a window of ours, whose rectangle is read into ours.
        if unsafe { GetWindowRect(window, &mut place) } == 0 {
            return;
        }
        canvas.lay_on(window as isize, place.left, place.top);
    });
}

/// A word the way Windows reads them, ending in a nought.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reading from a session that is doing well, at sixty
    /// frames.
    fn healthy() -> Measures {
        Measures {
            fps: Some(60.0),
            decode_ms: Some(0.4),
            render_ms: Some(15.0),
            host_ms: Some(2.3),
            network_ms: Some(8.0),
            dropped_network_pct: Some(0.0),
            dropped_jitter_pct: Some(0.1),
            since_frame_ms: Some(12),
            ..Default::default()
        }
    }

    #[test]
    fn a_badge_under_the_hand_says_what_it_reads() {
        let reads = read(&Measures {
            since_frame_ms: Some(FROZEN_MS),
            ..healthy()
        });
        assert!(what_it_says(0, &reads).contains("figée"));
    }

    #[test]
    fn a_badge_with_nothing_to_report_says_so_rather_than_nothing() {
        // They can be held on screen on request, unlit: an empty bubble
        // would suggest the question has no answer.
        let calm = Reads::default();
        assert!(!what_it_says(0, &calm).is_empty());
        assert!(!what_it_says(1, &calm).is_empty());
    }

    #[test]
    fn the_picture_badge_says_both_computers_when_both_are_late() {
        let reads = Reads {
            link: None,
            far: Some("là-bas".to_string()),
            here: Some("ici".to_string()),
        };
        let said = what_it_says(1, &reads);
        assert!(said.contains("là-bas") && said.contains("ici"));
    }

    #[test]
    fn a_session_that_is_going_well_lights_nothing() {
        assert_eq!(read(&healthy()), Reads::default());
    }

    #[test]
    fn a_reading_that_says_nothing_lights_nothing_either() {
        // A session that has just opened: the engine has not yet
        // written a single second. Nothing is known, so nothing lights
        // up.
        assert_eq!(read(&Measures::default()), Reads::default());
    }

    #[test]
    fn a_picture_that_has_stopped_lights_the_link() {
        let frozen = Measures {
            since_frame_ms: Some(FROZEN_MS),
            ..healthy()
        };
        let reads = read(&frozen);
        assert!(reads.link.is_some_and(|why| why.contains("figée")));
        assert!(reads.far.is_none() && reads.here.is_none());
    }

    #[test]
    fn frames_lost_on_the_way_light_the_link_too() {
        let lost = Measures {
            dropped_network_pct: Some(LOST_PCT),
            ..healthy()
        };
        assert!(read(&lost).link.is_some_and(|why| why.contains("perdent")));

        let late = Measures {
            dropped_jitter_pct: Some(TOO_LATE_PCT),
            ..healthy()
        };
        assert!(
            read(&late)
                .link
                .is_some_and(|why| why.contains("trop tard"))
        );
    }

    #[test]
    fn a_host_that_cannot_keep_up_lights_the_picture() {
        // Twenty-five milliseconds per frame on a session that serves
        // forty: its encoding is what sets the pace.
        let slow = Measures {
            fps: Some(40.0),
            host_ms: Some(25.0),
            ..healthy()
        };
        let reads = read(&slow);
        assert!(reads.far.is_some_and(|why| why.contains("d'en face")));
        // And this one has nothing to do with it: the badge must light
        // the right one of the two screens, not both.
        assert!(reads.here.is_none());
        assert!(reads.link.is_none());
    }

    #[test]
    fn a_computer_that_cannot_decode_in_time_lights_it_as_well() {
        let slow = Measures {
            fps: Some(30.0),
            decode_ms: Some(40.0),
            ..healthy()
        };
        let reads = read(&slow);
        assert!(reads.here.is_some_and(|why| why.contains("cet ordinateur")));
        assert!(reads.far.is_none());
    }

    #[test]
    fn two_computers_that_both_struggle_light_both_screens() {
        // A session too big for both machines: the badge does not have to
        // choose which one to name, it lights them both.
        let both = Measures {
            fps: Some(24.0),
            host_ms: Some(45.0),
            decode_ms: Some(50.0),
            ..healthy()
        };
        let reads = read(&both);
        assert!(reads.far.is_some());
        assert!(reads.here.is_some());

        let mut steady = Steady::default();
        assert_eq!(
            steady.after(&reads, Instant::now()),
            Shown {
                link: false,
                far: true,
                here: true
            }
        );
    }

    #[test]
    fn a_slow_session_that_asked_for_slow_is_not_a_fault() {
        // Thirty frames a second leave thirty-three milliseconds per
        // frame: a host at twenty is late for nothing.
        let calm = Measures {
            fps: Some(30.0),
            host_ms: Some(20.0),
            decode_ms: Some(5.0),
            ..healthy()
        };
        assert_eq!(read(&calm), Reads::default());
    }

    #[test]
    fn the_time_a_frame_waits_for_the_screen_is_not_counted() {
        // The render time includes waiting for the screen's refresh,
        // so it always comes close to the time available: counted,
        // this badge would be lit the whole session.
        let ordinary = Measures {
            render_ms: Some(16.6),
            ..healthy()
        };
        assert_eq!(read(&ordinary), Reads::default());
    }

    #[test]
    fn a_badge_stays_lit_for_a_moment_after_its_cause_has_gone() {
        // Without this it blinks: the cause holds for one reading, and a
        // dozen go by every second.
        let start = Instant::now();
        let mut steady = Steady::default();
        let wrong = read(&Measures {
            since_frame_ms: Some(900),
            ..healthy()
        });

        assert_eq!(
            steady.after(&wrong, start),
            Shown {
                link: true,
                far: false,
                here: false
            }
        );
        let well = read(&healthy());
        assert!(steady.after(&well, start + Duration::from_millis(100)).link);
        assert!(
            steady
                .after(&well, start + HOLDS - Duration::from_millis(1))
                .link
        );
        assert!(!steady.after(&well, start + HOLDS).link);
    }

    #[test]
    fn a_cause_that_comes_back_holds_it_on_from_there() {
        let start = Instant::now();
        let mut steady = Steady::default();
        let wrong = read(&Measures {
            since_frame_ms: Some(900),
            ..healthy()
        });
        let well = read(&healthy());

        steady.after(&wrong, start);
        let again = start + HOLDS - Duration::from_millis(10);
        steady.after(&wrong, again);
        // The second cause starts again from where it is, and not from
        // the first: otherwise a network doing badly in fits and starts
        // would turn the badge off in the middle of its fits.
        assert!(steady.after(&well, start + HOLDS).link);
        assert!(!steady.after(&well, again + HOLDS).link);
    }

    #[test]
    fn the_three_are_counted_apart() {
        let start = Instant::now();
        let mut steady = Steady::default();
        let only_the_far_one = read(&Measures {
            fps: Some(40.0),
            host_ms: Some(25.0),
            ..healthy()
        });
        assert_eq!(
            steady.after(&only_the_far_one, start),
            Shown {
                link: false,
                far: true,
                here: false
            }
        );
        assert!(
            !Shown {
                link: false,
                far: true,
                here: false
            }
            .nothing()
        );
        assert!(Shown::default().nothing());
    }
}
