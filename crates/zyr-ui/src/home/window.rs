//! The window the home page is drawn in, and what Windows says to it.
//!
//! A child of the window Windows frames: the outer one is placed, moved
//! and framed by the system, and what is ours is its inside, which is
//! exactly what a child window is. Every frame is drawn whole, and only
//! when something changed or is still moving; the rest of the time the
//! window costs nothing.

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use zyr_draw::Canvas;
use zyr_win32::wide;

use crate::home::state::Dialog;
use crate::home::ui::Painter;
use crate::home::ui::fields::{self, Field};
use crate::home::{frame, input, note, seen};
use crate::shell::app::App;

/// The window, and how much a page pixel counts for on its screen, in
/// hundredths.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static SCALE: AtomicU32 = AtomicU32::new(100);

/// The program, kept here because nothing hands one to a window of the
/// system.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

// The canvas, held by the thread that owns the window: a drawing surface
// and the window it dresses belong to the thread that made them.
thread_local! {
    static CANVAS: std::cell::RefCell<Option<Canvas>> = const { std::cell::RefCell::new(None) };
}

/// A frame of something moving, asked for by the beat of the screen.
const ANIMATE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 2;

/// The timer that draws a frame later with nothing happening: a bubble
/// about to open, good news about to go.
const WAKE: usize = 1;

/// The timer that gives the sift time to be written before the journal
/// is read again, and how long the last letter is given.
///
/// The sift narrows the page as one types, with nothing to click; read
/// again at every letter, « clipboard » would read the journal nine
/// times, so it is the letter nobody follows that sets the reading off.
const SIFT_PAUSE: usize = 2;
const SIFT_PAUSE_MS: u32 = 300;

pub fn handle() -> isize {
    ITS_WINDOW.load(Ordering::Relaxed)
}

pub fn scale() -> f32 {
    SCALE.load(Ordering::Relaxed) as f32 / 100.0
}

pub fn program() -> Option<App> {
    PROGRAM.lock().expect("the home window's program").clone()
}

/// Opens the window in the one Windows frames, on the thread that owns
/// that one: a window belongs to the thread that made it, and one made
/// elsewhere would never hear a mouse.
pub fn open(app: &App) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, GetClientRect, IDC_ARROW, LoadCursorW,
        RegisterClassW, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
    };

    if handle() != 0 {
        return true;
    }
    let outer = crate::shell::main_window::handle() as HWND;
    if outer.is_null() {
        note("no window to draw the home page in");
        return false;
    }
    *PROGRAM.lock().expect("the home window's program") = Some(app.clone());
    SCALE.store(
        (crate::shell::main_window::scale() * 100.0).round() as u32,
        Ordering::Relaxed,
    );
    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of the program, whose rectangle is read into ours.
    if unsafe { GetClientRect(outer, &mut inside) } == 0 {
        note("the window does not say its size");
        return false;
    }
    let class_name = wide("ZyrDeskAccueil");
    // SAFETY: a class declared once and a window built on it, on the
    // thread that will pump its messages. A class declared twice is
    // refused with no other effect, hence the answer left unread.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            // Drawn again whole as soon as its size changes: otherwise
            // the system asks only for the strip that has just appeared,
            // and nothing at all when the window shrinks, and the page
            // stays laid out for a size that no longer exists.
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            // No background: every frame covers the whole window, and a
            // background laid by the system would be one more colour,
            // seen for the length of a frame at every resize.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            // Clipped by its siblings: a session's picture is laid over
            // it in the same window, and without this the home page would
            // draw itself again behind it at every frame.
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            inside.right,
            inside.bottom,
            outer,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("the home page's canvas could not open");
        return false;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    note(&format!(
        "home page drawn by ZyrDesk: canvas of {}x{} px",
        inside.right, inside.bottom
    ));
    true
}

/// Reads again how much a page pixel counts for, after the window moved
/// to another screen or its screen changed magnification.
pub fn measure_the_screen(app: &App) {
    let wanted = (crate::shell::main_window::scale() * 100.0).round() as u32;
    if SCALE.swap(wanted, Ordering::Relaxed) == wanted {
        return;
    }
    // The fields are windows of the system: their font does not follow
    // the rest, it is made again for them.
    fields::dress(scale());
    crate::home::redraw(app);
}

/// Asks for a frame, from the thread that draws.
pub fn invalidate() {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

    let window = handle();
    if window != 0 {
        // SAFETY: a window of ours, on the thread that owns it.
        unsafe { InvalidateRect(window as HWND, std::ptr::null(), 0) };
    }
}

/// Asks for a frame of the window and of the fields in it: what changes
/// their colours, the theme, has to reach the fields too, which are
/// windows of their own and otherwise keep the colours they last had.
pub fn invalidate_with_the_fields() {
    use windows_sys::Win32::Graphics::Gdi::{
        RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RedrawWindow,
    };

    let window = handle();
    if window != 0 {
        // SAFETY: a window of ours, on the thread that owns it.
        unsafe {
            RedrawWindow(
                window as HWND,
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
            )
        };
    }
}

/// Forgets a reading of the journal that was waiting for the pause after
/// a letter: another has replaced it.
pub fn forget_the_sift_pause() {
    use windows_sys::Win32::UI::WindowsAndMessaging::KillTimer;

    let window = handle();
    if window != 0 {
        // SAFETY: a timer of ours, on the thread that set it; nothing if
        // it was not set.
        unsafe { KillTimer(window as HWND, SIFT_PAUSE) };
    }
}

/// What the window answers when Windows speaks to it.
///
/// SAFETY: called by the system on the thread that made this window, with
/// the arguments it documents.
unsafe extern "system" fn answer(
    window: HWND,
    message: u32,
    holding: WPARAM,
    with: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, EN_CHANGE, EN_KILLFOCUS, EN_SETFOCUS, HTCLIENT, IDC_ARROW, IDC_HAND,
        KillTimer, LoadCursorW, SetCursor, SetTimer, WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND,
        WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
        WM_PAINT, WM_SETCURSOR, WM_SYSKEYDOWN, WM_TIMER,
    };

    match message {
        // Nothing to erase: every frame covers the whole window, and an
        // erase in between would be a blink of bare background.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            repaint(window);
            0
        }
        // Asked for and not drawn at once: drawing with nothing made
        // invalid draws nothing, the system lending an empty surface.
        ANIMATE => {
            invalidate();
            0
        }
        WM_TIMER if holding == WAKE => {
            // SAFETY: a timer of ours, on the thread that set it.
            unsafe { KillTimer(window, WAKE) };
            invalidate();
            0
        }
        WM_TIMER if holding == SIFT_PAUSE => {
            forget_the_sift_pause();
            if let Some(app) = program() {
                crate::home::dialogs::reread_the_journal(&app, crate::home::dialogs::After::Show);
            }
            0
        }
        WM_MOUSEMOVE => {
            input::moves(window, crate::shell::win32::pointer_in(with));
            0
        }
        WM_MOUSELEAVE => {
            input::left();
            0
        }
        WM_LBUTTONDOWN => {
            input::presses(window, crate::shell::win32::pointer_in(with));
            0
        }
        WM_LBUTTONUP => {
            input::releases(crate::shell::win32::pointer_in(with));
            0
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let notches = f32::from(((holding >> 16) & 0xFFFF) as i16) / 120.0;
            // Sideways for a wheel tilted, or turned with Shift held.
            let across = message == WM_MOUSEHWHEEL || holding & 0x0004 != 0;
            input::wheel(notches, across);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if input::key_down(holding as u32, with) {
                return 0;
            }
            // SAFETY: the system's own answer to a key not taken here.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            let shape = if crate::home::state().hovered().is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: a pointer shape of the system, asked for by name.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), shape)) };
            1
        }
        // A field's inside belongs to the system, which asks here what
        // colours to paint it in, so that it matches the rest.
        WM_CTLCOLOREDIT => {
            let colours = zyr_draw::design::palette(crate::shell::theme::light());
            fields::tint(holding, colours.surface_2, colours.text)
        }
        WM_COMMAND => {
            let said = ((holding >> 16) & 0xFFFF) as u32;
            if said == EN_CHANGE || said == EN_SETFOCUS || said == EN_KILLFOCUS {
                // What a field holds changes what its dialog says and
                // what its buttons allow; the field the keyboard is in
                // is drawn ringed.
                invalidate();
            }
            // The sift reads this computer's journal again by itself,
            // after a pause. Not a far one's: reading it again opens a
            // road all the way to the other computer, and one per pause
            // in the typing would cost seconds; there, it is Enter or
            // « Refresh » that reads.
            let own_journal = matches!(
                crate::home::state().dialog,
                Some(Dialog::Journal { of: None })
            );
            if said == EN_CHANGE && own_journal && fields::which(with) == Some(Field::Sift) {
                // SAFETY: a timer on a window of ours, from its thread.
                // Set again, it starts again from nought, which is why
                // one more letter pushes the reading back.
                unsafe { SetTimer(window, SIFT_PAUSE, SIFT_PAUSE_MS, None) };
            }
            0
        }
        // What a field asked for: Enter or Escape, done here because both
        // close the dialog, and so destroy the field asking.
        fields::ACT => {
            if let Some(app) = program() {
                if holding == 1 {
                    crate::home::dialogs::confirm(&app);
                } else {
                    crate::home::dialogs::close(&app);
                }
            }
            0
        }
        // SAFETY: the system's own answer to everything not answered here.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// Draws the window and pours it into its surface, then sets the fields
/// where the frame laid them and says when the next frame is due.
fn repaint(window: HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let Some(app) = program() else {
        return;
    };
    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, whose rectangle is read into ours.
    if unsafe { GetClientRect(window, &mut inside) } == 0 {
        return;
    }
    let (width, height) = (inside.right.max(1), inside.bottom.max(1));
    // SAFETY: an empty structure, filled by the system below.
    let mut paint: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    // SAFETY: a window of ours, whose surface is given back below.
    let surface = unsafe { BeginPaint(window, &mut paint) };
    if surface.is_null() {
        return;
    }
    let scale = scale();
    let light = crate::shell::theme::light();
    let lively = crate::shell::win32::lets_things_move();
    // Every frame says again where the fields go: one the frame no longer
    // lays, because its dialog changed shape, has no place and is put
    // away.
    fields::unplace();
    let mut next = Next::default();
    CANVAS.with_borrow_mut(|canvas| {
        let fits = match canvas.as_mut() {
            Some(canvas) => canvas.resize(width, height),
            None => false,
        };
        if !fits {
            *canvas = Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };
        let colours = zyr_draw::design::palette(light);
        canvas.begin(colours.background);
        next = seen::with(|seen| {
            let mut state = crate::home::state();
            state.motion.begin(lively);
            state.wake_at = None;
            let mut painter = Painter::new(canvas, scale, light, seen, &mut state, &app);
            frame::draw(&mut painter, width as f32, height as f32);
            let hits = painter.into_hits();
            state.hits = hits;
            Next {
                moving: state.motion.end(),
                wake_at: state.wake_at,
            }
        });
        if canvas.finish() {
            canvas.copy_to(surface as isize, 0, 0);
        }
    });
    // SAFETY: the painting opened above.
    unsafe { EndPaint(window, &paint) };
    fields::lay_out(scale);
    // What is under the hand now that things have moved: a card that
    // slid under a still mouse is under it all the same.
    input::hover_again();
    schedule(window, next);
}

/// When the next frame is due.
#[derive(Default)]
struct Next {
    /// Something is on its way: at the beat of the screen.
    moving: bool,
    /// Otherwise, at that instant, if anything waits for one.
    wake_at: Option<Instant>,
}

fn schedule(window: HWND, next: Next) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

    if next.moving {
        zyr_draw::pulse::beat(window, ANIMATE);
    } else {
        zyr_draw::pulse::stop(window);
    }
    match next.wake_at {
        Some(at) if !next.moving => {
            let wait = at
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, u128::from(u32::MAX));
            // SAFETY: a timer on a window of ours, from its thread. Set
            // again, it replaces the one before.
            unsafe { SetTimer(window, WAKE, wait as u32, None) };
        }
        _ => {
            // SAFETY: a timer of ours, on its thread; nothing if unset.
            unsafe { KillTimer(window, WAKE) };
        }
    }
}
