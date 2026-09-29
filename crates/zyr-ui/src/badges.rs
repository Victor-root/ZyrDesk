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
//! Never a warning drawn into the picture: the picture is the far
//! computer's desktop and not ours to write on.
//!
//! What lights them is decided by `zyr_session::health`, from what the
//! player measures: that is arithmetic, and it is tried there. Here is
//! what shows it: the words, compiled and tested everywhere, and the
//! badges themselves, which are a window, so Windows code, like the
//! session.

// Outside Windows there is no picture to cover, but what is said is
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::Duration;

use zyr_session::health::{Lit, Reads};

/// What this module files its journal lines under.
const TAG: &str = "badges";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

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
            Which::Link => "link",
            Which::Far => "far picture",
            Which::Here => "picture here",
        })
    }
}

/* ---- The loop that keeps them ---------------------------------------- */

/// How many times a second the reading is read again.
///
/// The player measures five; this loop reads a little more often, so
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
        show(&app, Lit::default(), &Reads::default(), false);
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
pub fn watch(_app: &crate::app::App) {}

/// The loop itself.
#[cfg(windows)]
async fn keep_up(app: &crate::app::App) {
    use std::time::Instant;

    use zyr_session::health::{self, Steady};

    let mut steady = Steady::default();
    let mut was = Lit::default();
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            return;
        }
        let held = crate::floating::the_badges_are_held_up(app);
        let reads = health::read(&crate::session::measures());
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

/// Says what has just changed, and nothing else.
///
/// One line when a badge lights up and one when it goes out, never one
/// per reading: a dozen of those go by every second, and a journal that
/// carried them all would carry nothing else.
#[cfg(windows)]
fn said(reads: &Reads, was: Lit, shown: Lit) {
    for (which, before, after, why) in [
        (Which::Link, was.link, shown.link, &reads.link),
        (Which::Far, was.far, shown.far, &reads.far),
        (Which::Here, was.here, shown.here, &reads.here),
    ] {
        if before == after {
            continue;
        }
        note(&match (after, why) {
            (true, Some(why)) => format!("{which} badge: {why}"),
            // Lit with no reason in this reading: the cause came and
            // went between two readings and the badge is still
            // holding.
            (true, None) => format!("{which} badge lit"),
            (false, _) => format!("{which} badge out"),
        });
    }
}

/* ---- The badges themselves ------------------------------------------- */

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
    static CANVAS: std::cell::RefCell<Option<zyr_draw::Canvas>> =
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
fn show(app: &crate::app::App, shown: Lit, reads: &Reads, held: bool) {
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
    *WHY.lock().expect("badges' reasons") = reads.clone();
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
fn show(_app: &crate::app::App, _shown: Lit, _reads: &Reads, _held: bool) {}

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
            .as_ref()
            .map_or_else(|| zyr_i18n::say!("badge.link_is_fine"), zyr_i18n::fact);
    }
    let both: Vec<String> = [&why.far, &why.here]
        .into_iter()
        .flatten()
        .map(zyr_i18n::fact)
        .collect();
    if both.is_empty() {
        return zyr_i18n::say!("badge.both_keep_up");
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

    if !crate::floating::still_to_be_made(&ITS_WINDOW) {
        return;
    }
    let name = zyr_win32::wide("ZyrDeskVoyants");
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
        note("badges: the window could not open");
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

    use zyr_draw::Rect;
    use zyr_draw::design::{Colour, DARK};

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
            *canvas = zyr_draw::Canvas::new(wide_px, high);
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
                canvas.icon(&zyr_draw::icons::LINK, icon_area, colour);
                continue;
            }
            // The picture badge carries both computers, the far one
            // behind and this one in front, as the product's logo draws
            // them. Both are laid down dimmed, then the one that is
            // struggling is drawn over again brightly: that is all it
            // takes to say which of the two, and it reads without a
            // caption since it is the brand's own drawing.
            canvas.icon(&zyr_draw::icons::HOST_SCREEN, icon_area, DARK.text_faint);
            if lit & bit::FAR != 0 {
                canvas.icon(&zyr_draw::icons::SCREEN_OVER_THERE, icon_area, DARK.warning);
            }
            if lit & bit::HERE != 0 {
                canvas.icon(&zyr_draw::icons::SCREEN_HERE, icon_area, DARK.warning);
            }
        }
        // The bubble after the badges, so that it goes on top should the
        // two ever touch.
        if lit & bit::OVER != 0 {
            let rank = usize::from(lit & bit::OVER_LINK == 0);
            let text = what_it_says(rank, &WHY.lock().expect("badges' reasons"));
            let pen = zyr_draw::Pen::of(WORDS * scale);
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

#[cfg(test)]
mod tests {
    use zyr_player::Measures;
    use zyr_session::health::read;

    use super::*;

    #[test]
    fn a_badge_under_the_hand_says_what_it_reads() {
        let reads = read(&Measures {
            since_frame_ms: Some(350.0),
            ..Default::default()
        });
        let said = what_it_says(0, &reads);
        assert!(said.contains("350 ms"), "{said}");
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
        let reads = read(&Measures {
            fps: Some(24.0),
            host_ms: Some(45.0),
            decode_ms: Some(50.0),
            ..Default::default()
        });
        let said = what_it_says(1, &reads);
        assert!(said.contains("45 ms") && said.contains("50 ms"), "{said}");
    }
}
