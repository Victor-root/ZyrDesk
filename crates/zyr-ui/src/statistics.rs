//! The figures of a session, in the bottom left corner of the picture.
//!
//! What « Statistiques » shows: a small card laid over the picture,
//! drawn by this program with the same tools as the badges, that says
//! what every frame costs on its way, from the far computer's screen to
//! this one, five times a second. The player measures; this only reads
//! what it measured and writes it out.
//!
//! A window of ours and never a drawing in the picture: the picture is
//! the far computer's desktop, and what this card says belongs to this
//! side. Clicks go through it, as they go through the badges: the corner
//! it covers is the far computer's.
//!
//! What is written compiles everywhere and is tested; the card is a
//! window, so Windows code.

// Outside Windows there is no picture to cover, but what is written is
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use zyr_player::Measures;

/// What this module files its journal lines under.
#[cfg(windows)]
const TAG: &str = "statistics";

/// Writes a line under this module's tag.
#[cfg(windows)]
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// What a figure shows while it has nothing to say: a frame nobody
/// decoded has no decoding time, not one of nought.
const NOTHING: &str = "-";

/// What the pictures are made of, in one line: codec, size, frames a
/// second. What is missing leaves no gap, it is not written.
pub fn stream(said: &Measures) -> String {
    let mut pieces: Vec<String> = Vec::new();
    if let Some(codec) = &said.codec {
        pieces.push(codec.clone());
    }
    if let (Some(width), Some(height)) = (said.width, said.height) {
        pieces.push(format!("{width}x{height}"));
    }
    if let Some(frames) = said.fps {
        pieces.push(format!("{frames:.0} images/s"));
    }
    pieces.join(" · ")
}

/// One figure, written with its unit.
fn figure(value: Option<f64>, decimals: usize, unit: &str) -> String {
    value.map_or_else(
        || NOTHING.to_string(),
        |value| format!("{value:.decimals$} {unit}"),
    )
}

/// What the card says: the stream's line, then a word and its figure on
/// each line under it.
#[derive(Clone, PartialEq, Debug)]
struct Card {
    stream: String,
    rows: [(&'static str, String); 7],
}

/// The card for those measures.
///
/// The lines in the order a frame goes: made over there, carried,
/// decoded here, shown; then what the wire carries and what it loses; and
/// last the one figure that adds it all up, from the far screen to this
/// one.
fn card(said: &Measures) -> Card {
    let losses = match (said.dropped_network_pct, said.dropped_jitter_pct) {
        (None, None) => NOTHING.to_string(),
        (lost, late) => format!(
            "{} en route, {} trop tard",
            figure(lost, 1, "%"),
            figure(late, 1, "%")
        ),
    };
    Card {
        stream: stream(said),
        rows: [
            ("Hôte", figure(said.host_ms, 2, "ms")),
            ("Réseau", figure(said.network_ms, 0, "ms")),
            ("Décodage", figure(said.decode_ms, 2, "ms")),
            ("Affichage", figure(said.render_ms, 2, "ms")),
            ("Débit", figure(said.bitrate_mbps, 2, "Mb/s")),
            ("Pertes", losses),
            ("Latence de bout en bout", figure(said.latency_ms, 0, "ms")),
        ],
    }
}

/* ---- The card --------------------------------------------------------- */

/// How often the figures are read again: as often as the player takes
/// them.
#[cfg(windows)]
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

/// The card's width, in page pixels.
#[cfg(windows)]
const WIDTH: f32 = 300.0;

/// Its inner margin.
#[cfg(windows)]
const PADDING: f32 = 10.0;

/// The radius of its corners.
#[cfg(windows)]
const CORNER: f32 = 8.0;

/// The size of what is written on it.
#[cfg(windows)]
const WORDS: f32 = 12.0;

/// The height of a line of it: what twelve-pixel type takes with the
/// room above and below it the font asks for.
#[cfg(windows)]
const LINE: f32 = 17.0;

/// The window, and whether the loop that fills it runs.
#[cfg(windows)]
static ITS_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the card says right now, kept for the drawing, which happens on
/// the thread that owns the window.
#[cfg(windows)]
static SAID: std::sync::Mutex<Option<Card>> = std::sync::Mutex::new(None);

#[cfg(windows)]
thread_local! {
    static CANVAS: std::cell::RefCell<Option<crate::paint::Canvas>> =
        const { std::cell::RefCell::new(None) };
}

/// What the window takes up, in real pixels.
#[cfg(windows)]
fn its_size() -> (i32, i32) {
    let scale = crate::main_window::scale();
    let high = 2.0 * PADDING + (1 + card(&Measures::default()).rows.len()) as f32 * LINE;
    ((WIDTH * scale).ceil() as i32, (high * scale).ceil() as i32)
}

/// Where its top left corner goes for a card whose bottom left corner is
/// `anchor`.
#[cfg(windows)]
fn window_corner(anchor: (i32, i32)) -> (i32, i32) {
    (anchor.0, anchor.1 - its_size().1)
}

/// Follows the figures for as long as the session shows and they are
/// asked for, and puts the card away afterwards.
///
/// Called at every turn of the floating button's watch: it does nothing
/// while a loop is already running.
#[cfg(windows)]
pub fn watch(app: &crate::app::App, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    raise(app, anchor);
    let app = app.clone();
    crate::app::spawn(async move {
        while crate::floating::a_session_is_up(&app) && crate::floating::the_figures_are_shown(&app)
        {
            let now = Some(card(&crate::session::measures()));
            let changed = {
                let mut kept = SAID.lock().expect("statistiques");
                let changed = *kept != now;
                *kept = now;
                changed
            };
            if changed {
                let _ = app.run_on_main_thread(repaint);
            }
            tokio::time::sleep(LOOK_EVERY).await;
        }
        lower(&app);
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
pub fn watch(_app: &crate::app::App, _anchor: (i32, i32)) {}

/// Opens the card's window, hidden: it shows itself once drawn.
#[cfg(windows)]
fn raise(app: &crate::app::App, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    *SAID.lock().expect("statistiques") = None;
    let owner = crate::main_window::handle();
    let _ = app.run_on_main_thread(move || build(owner, anchor));
}

/// Puts it away.
#[cfg(windows)]
fn lower(app: &crate::app::App) {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: a window of ours, destroyed on the thread that made it.
        unsafe { DestroyWindow(window as HWND) };
    });
}

/// Lays it in the bottom left corner of the picture.
///
/// Called from where the floating button is laid, so at every step of a
/// hand resizing the window: nothing here waits for anything.
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
    // resized.
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

/// Builds the window, hidden.
#[cfg(windows)]
fn build(owner: isize, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{GetLastError, HWND};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };

    // Asked for by a loop that has ended since, it is not made at all.
    if !WATCHING.load(Ordering::SeqCst) || !crate::floating::still_to_be_made(&ITS_WINDOW) {
        return;
    }
    let name: Vec<u16> = "ZyrDeskStatistiques"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let (width, height) = its_size();
    let (left, top) = window_corner(anchor);
    // SAFETY: a class registered once and a window built on it, on the
    // thread that will pump its messages. A class already registered is
    // refused and nothing more: a second session finds the first one's.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(DefWindowProcW),
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
        // Transparent to clicks: the corner it covers is the far
        // computer's, and a hand aiming at something there must reach it.
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            left,
            top,
            width,
            height,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        // SAFETY: no argument; the error of the call just above.
        let code = unsafe { GetLastError() };
        note(&format!(
            "statistiques : la fenêtre n'a pas pu s'ouvrir (CreateWindowExW, erreur {code})"
        ));
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    // What was read while the window was being made is drawn now: the
    // loop only asks again when the figures change.
    repaint();
}

/// Draws the card with what it says now, and shows it.
#[cfg(windows)]
fn repaint() {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SW_SHOWNOACTIVATE, ShowWindow,
    };

    use crate::design::{Colour, DARK};
    use crate::paint::{Align, Pen, Rect};

    let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if window.is_null() {
        return;
    }
    // A window put down in the taskbar takes the card down with it; shown
    // then, the card would be the only thing left on the desktop.
    if !crate::main_window::on_screen() {
        return;
    }
    let Some(Card { stream, rows }) = SAID.lock().expect("statistiques").clone() else {
        return;
    };
    let scale = crate::main_window::scale();
    let (width, height) = its_size();
    CANVAS.with_borrow_mut(|canvas| {
        // Made again when the screen's magnification has changed: the
        // canvas is an image of a given size, and the window follows.
        if canvas
            .as_ref()
            .is_none_or(|had| had.size() != (width, height))
        {
            *canvas = crate::paint::Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };
        canvas.begin(Colour::TRANSPARENT);
        let card = Rect::at(0.0, 0.0, width as f32, height as f32);
        let radius = CORNER * scale;
        // The dark card whatever the theme, like the badges: it lies on
        // the far computer's desktop, which can be any colour.
        canvas.fill(card, radius, DARK.surface_1.faded(0.92));
        canvas.stroke_inside(card, radius, scale, DARK.border_strong);
        let inside = card.grown(-PADDING * scale);
        let line = LINE * scale;
        let pen = Pen::of(WORDS * scale).ellipsized();
        canvas.draw_text(
            if stream.is_empty() { NOTHING } else { &stream },
            pen.in_bold(),
            DARK.text,
            Rect::at(inside.left, inside.top, inside.right - inside.left, line),
        );
        for (rank, (label, value)) in rows.iter().enumerate() {
            let at = Rect::at(
                inside.left,
                inside.top + (rank + 1) as f32 * line,
                inside.right - inside.left,
                line,
            );
            canvas.draw_text(label, pen, DARK.text_faint, at);
            // Figures of fixed width, lined up on the right: they change
            // five times a second, and figures that dance side to side
            // cannot be read.
            canvas.draw_text(value, pen.monospaced().aligned(Align::Right), DARK.text, at);
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
        // SAFETY: a window of ours, shown without taking the front.
        unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_measured_says_every_figure_with_its_unit() {
        let said = Measures {
            codec: Some("HEVC".to_string()),
            width: Some(2560),
            height: Some(1440),
            fps: Some(59.8),
            decode_ms: Some(0.42),
            render_ms: Some(1.3),
            host_ms: Some(4.25),
            network_ms: Some(12.4),
            bitrate_mbps: Some(18.4),
            dropped_network_pct: Some(0.0),
            dropped_jitter_pct: Some(0.14),
            latency_ms: Some(38.2),
            ..Measures::default()
        };
        let card = card(&said);
        assert_eq!(card.stream, "HEVC · 2560x1440 · 60 images/s");
        let value = |label: &str| {
            card.rows
                .iter()
                .find(|(named, _)| *named == label)
                .map(|(_, value)| value.clone())
                .unwrap()
        };
        assert_eq!(value("Décodage"), "0.42 ms");
        assert_eq!(value("Affichage"), "1.30 ms");
        assert_eq!(value("Hôte"), "4.25 ms");
        assert_eq!(value("Réseau"), "12 ms");
        assert_eq!(value("Débit"), "18.40 Mb/s");
        assert_eq!(value("Pertes"), "0.0 % en route, 0.1 % trop tard");
        assert_eq!(value("Latence de bout en bout"), "38 ms");
    }

    #[test]
    fn a_figure_not_measured_says_so_rather_than_nought() {
        let empty = card(&Measures::default());
        assert!(
            empty.rows.iter().all(|(_, value)| value == NOTHING),
            "{empty:?}"
        );
        assert_eq!(empty.stream, "");
        // Half a loss measured is still worth saying.
        let half = Measures {
            dropped_jitter_pct: Some(2.0),
            ..Measures::default()
        };
        assert_eq!(card(&half).rows[5].1, "- en route, 2.0 % trop tard");
    }
}
