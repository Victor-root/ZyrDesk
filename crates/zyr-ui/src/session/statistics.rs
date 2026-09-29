//! The figures of a session, in a banner along the top of the picture.
//!
//! What « Statistics » shows: a strip across the whole width of the
//! picture, stuck to its top edge, drawn by this program with the same
//! tools as the badges, that says what every frame costs on its way,
//! from the far computer's screen to this one, five times a second. The
//! player measures; this only reads what it measured and writes it out.
//!
//! One line when the picture is wide enough for all of it, more when it
//! is not: the figures follow one another like words, and one that no
//! longer fits on a line starts the next. The floating button and the
//! badges hang under the banner, which never covers them.
//!
//! A window of ours and never a drawing in the picture: the picture is
//! the far computer's desktop, and what this banner says belongs to this
//! side. Clicks go through it, as they go through the badges: the strip
//! it covers is the far computer's.
//!
//! What is written and where it goes compile everywhere and are tested;
//! the banner is a window, so Windows code.

// Outside Windows there is no picture to cover, but what is written is
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use zyr_i18n::key;
use zyr_player::Measures;

/// What this module files its journal lines under.
#[cfg(windows)]
const TAG: &str = "statistics";

/// Writes a line under this module's tag.
#[cfg(windows)]
fn note(what: &str) {
    crate::shell::journal::note_about(TAG, what);
}

/// What a figure shows while it has nothing to say: a frame nobody
/// decoded has no decoding time, not one of nought.
pub const NOTHING: &str = "-";

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
        pieces.push(zyr_i18n::say!(
            "figures.frames_per_second",
            frames = format!("{frames:.0}")
        ));
    }
    pieces.join(" · ")
}

/// A figure, written with its unit: the banner's way and the menu's.
pub fn written(value: f64, decimals: usize, unit: &str) -> String {
    format!("{value:.decimals$} {unit}")
}

/// One figure, or what says it was not measured.
fn figure(value: Option<f64>, decimals: usize, unit: &str) -> String {
    value.map_or_else(
        || NOTHING.to_string(),
        |value| written(value, decimals, unit),
    )
}

/// What the banner says: the stream's line, then each figure after its
/// word, the word kept as the key of its text and said where it is drawn.
#[derive(Clone, PartialEq, Debug)]
struct Banner {
    stream: String,
    rows: [(&'static str, String); 7],
}

impl Banner {
    /// Its pieces in the order they are read: the stream's line, with no
    /// word before it, then each figure after its word.
    fn pieces(&self) -> impl Iterator<Item = (Option<&'static str>, &str)> {
        let stream = if self.stream.is_empty() {
            NOTHING
        } else {
            &self.stream
        };
        std::iter::once((None, stream)).chain(
            self.rows
                .iter()
                .map(|(word, figure)| (Some(*word), figure.as_str())),
        )
    }
}

/// The banner for those measures.
///
/// The figures in the order a frame goes: made over there, carried,
/// decoded here, shown; then what the wire carries and what it loses; and
/// last the one figure that adds it all up, from the far screen to this
/// one.
fn banner(said: &Measures) -> Banner {
    let losses = match (said.dropped_network_pct, said.dropped_jitter_pct) {
        (None, None) => NOTHING.to_string(),
        (lost, late) => zyr_i18n::say!(
            "figures.losses_on_the_way",
            lost = figure(lost, 1, "%"),
            late = figure(late, 1, "%")
        ),
    };
    Banner {
        stream: stream(said),
        rows: [
            (key!("figures.host"), figure(said.host_ms, 2, "ms")),
            (key!("figures.network"), figure(said.network_ms, 0, "ms")),
            (key!("figures.decode"), figure(said.decode_ms, 2, "ms")),
            (key!("figures.display"), figure(said.render_ms, 2, "ms")),
            (
                key!("figures.bitrate"),
                figure(said.bitrate_mbps, 2, "Mb/s"),
            ),
            (key!("figures.losses"), losses),
            (key!("figures.latency"), figure(said.latency_ms, 0, "ms")),
        ],
    }
}

/// The widest the figures of a session get short of a broken one: under
/// a hundred milliseconds where they are read to the hundredth, under a
/// thousand where they are whole, under a thousand megabits a second and
/// under a hundred per cent.
///
/// The room each figure keeps from the moment the banner opens, so that
/// it opens at the height it keeps and nothing moves as the figures
/// change. The stream says nothing here: its line only changes with the
/// stream itself, and takes the room it needs.
fn widest() -> Banner {
    banner(&Measures {
        host_ms: Some(88.88),
        network_ms: Some(888.0),
        decode_ms: Some(88.88),
        render_ms: Some(88.88),
        bitrate_mbps: Some(888.88),
        dropped_network_pct: Some(88.8),
        dropped_jitter_pct: Some(88.8),
        latency_ms: Some(888.0),
        ..Measures::default()
    })
}

/// Where each piece of the banner goes, laid out like words: the line it
/// lands on, and how far along that line it starts.
///
/// A piece that does not fit after the ones before it on a line starts
/// the next one. The first piece of a line stays on it however wide it
/// is: pushed on, it would only leave an empty line behind.
fn flowed(widths: &[f32], room: f32, gap: f32) -> Vec<(usize, f32)> {
    let mut line = 0;
    let mut along = 0.0;
    widths
        .iter()
        .enumerate()
        .map(|(rank, &width)| {
            if rank > 0 && along + width > room {
                line += 1;
                along = 0.0;
            }
            let at = (line, along);
            along += width + gap;
            at
        })
        .collect()
}

/* ---- The banner ------------------------------------------------------- */

/// How often the figures are read again: as often as the player takes
/// them.
#[cfg(windows)]
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

/// The size of what is written on it, in page pixels: the product's
/// captions.
#[cfg(windows)]
const WORDS: f32 = zyr_draw::design::CAPTION;

/// The room above and below its lines.
#[cfg(windows)]
const ABOVE: f32 = zyr_draw::design::SPACE_1;

/// The room before its lines and after them.
#[cfg(windows)]
const ASIDE: f32 = zyr_draw::design::SPACE_3;

/// What separates two pieces of a line, a stroke standing in the middle.
#[cfg(windows)]
const BETWEEN: f32 = zyr_draw::design::SPACE_4;

/// What separates a word from its figure: about a space.
#[cfg(windows)]
const AFTER_THE_WORD: f32 = zyr_draw::design::SPACE_1;

/// The window, and whether the loop that fills it runs.
#[cfg(windows)]
static ITS_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The program, kept to ask the thread that draws for a new drawing from
/// wherever the banner is laid.
#[cfg(windows)]
static PROGRAM: std::sync::Mutex<Option<crate::shell::app::App>> = std::sync::Mutex::new(None);

/// What the banner says right now, kept for the drawing, which happens on
/// the thread that owns the window.
#[cfg(windows)]
static SAID: std::sync::Mutex<Option<Banner>> = std::sync::Mutex::new(None);

/// What its pieces take; see `Measured`.
#[cfg(windows)]
static MEASURED: std::sync::Mutex<Option<Measured>> = std::sync::Mutex::new(None);

/// How tall it stands over the top of the picture, in real pixels, and
/// nought while it does not: the button and the badges hang under it.
#[cfg(windows)]
static STRIP: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(windows)]
thread_local! {
    static CANVAS: std::cell::RefCell<Option<zyr_draw::Canvas>> =
        const { std::cell::RefCell::new(None) };
}

/// What the banner's pieces take, in real pixels on a screen of one
/// magnification.
///
/// Measured on the thread that draws, the only one with something to
/// measure text with, and read wherever the banner is laid: how tall it
/// is depends on how many lines its pieces take across the picture.
#[cfg(windows)]
#[derive(Clone)]
struct Measured {
    scale: f32,
    /// The height of one of its lines.
    line: f32,
    /// The room each piece keeps whatever its figure says: never less
    /// than the widest it usually gets, and grown for good the first time
    /// it needs more. Pieces that followed their figures would dance from
    /// side to side five times a second, and from one line to the next.
    slots: Vec<f32>,
}

#[cfg(windows)]
impl Measured {
    /// Where its pieces go across a banner that wide.
    fn flowed(&self, wide: i32) -> Vec<(usize, f32)> {
        flowed(
            &self.slots,
            wide as f32 - 2.0 * ASIDE * self.scale,
            BETWEEN * self.scale,
        )
    }
}

/// The three ways the banner writes: its words, the stream's line, and
/// its figures.
#[cfg(windows)]
#[derive(Clone, Copy)]
struct Pens {
    word: zyr_draw::Pen,
    stream: zyr_draw::Pen,
    /// Of fixed width: figures that change five times a second cannot be
    /// read unless every digit keeps its own room.
    figure: zyr_draw::Pen,
}

#[cfg(windows)]
impl Pens {
    /// The pens at that magnification, on one line whatever the room:
    /// where each piece goes is decided by `flowed`, not by the pen.
    fn at(scale: f32) -> Self {
        let word = zyr_draw::Pen::of(WORDS * scale).overflowing();
        Pens {
            word,
            stream: word.in_bold(),
            figure: word.monospaced(),
        }
    }
}

/// How wide each piece of that banner is written, in the order it is
/// read.
#[cfg(windows)]
fn widths(canvas: &zyr_draw::Canvas, banner: &Banner, pens: Pens, scale: f32) -> Vec<f32> {
    banner
        .pieces()
        .map(|(word, text)| match word {
            None => canvas.width_of(text, pens.stream),
            Some(word) => {
                canvas.width_of(&zyr_i18n::text(word), pens.word)
                    + AFTER_THE_WORD * scale
                    + canvas.width_of(text, pens.figure)
            }
        })
        .collect()
}

/// What the pieces of that banner take now, kept for wherever it is laid.
///
/// Started again from the widest the figures usually get when the banner
/// opens, and when the screen's magnification changes: every length here
/// is in its real pixels.
#[cfg(windows)]
fn measure(canvas: &zyr_draw::Canvas, banner: &Banner, scale: f32) -> Measured {
    let pens = Pens::at(scale);
    let mut kept = MEASURED.lock().expect("figures' measures");
    let had = kept
        .take()
        .filter(|had| had.scale == scale)
        .unwrap_or_else(|| Measured {
            scale,
            line: canvas.line_height(pens.word),
            slots: widths(canvas, &widest(), pens, scale),
        });
    let slots = had
        .slots
        .iter()
        .zip(widths(canvas, banner, pens, scale))
        .map(|(slot, now)| slot.max(now))
        .collect();
    let measured = Measured { slots, ..had };
    *kept = Some(measured.clone());
    measured
}

/// The size of the banner across that picture, in real pixels: its whole
/// width, and as many lines as its pieces take there, never taller than
/// the picture itself.
#[cfg(windows)]
fn size_of(picture: (i32, i32, i32, i32), measured: &Measured) -> (i32, i32) {
    let (left, top, right, bottom) = picture;
    let lines = measured
        .flowed(right - left)
        .last()
        .map_or(1, |&(line, _)| line + 1);
    let high = (2.0 * ABOVE * measured.scale + lines as f32 * measured.line).ceil() as i32;
    (right - left, high.min(bottom - top))
}

/// The same from what was last measured, for wherever the banner is laid:
/// nothing when that was on a screen of another magnification, or not
/// yet at all.
#[cfg(windows)]
fn size_for(picture: (i32, i32, i32, i32)) -> Option<(i32, i32)> {
    let scale = crate::shell::main_window::scale();
    MEASURED
        .lock()
        .expect("figures' measures")
        .as_ref()
        .filter(|measured| measured.scale == scale)
        .map(|measured| size_of(picture, measured))
}

/// How tall the banner stands over the top of the picture, in real
/// pixels, and nought while there is none: what the button and the
/// badges hang under.
#[cfg(windows)]
pub fn strip() -> i32 {
    STRIP.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(not(windows))]
pub fn strip() -> i32 {
    0
}

/// Follows the figures for as long as the session shows and they are
/// asked for, and puts the banner away afterwards.
///
/// Called at every turn of the floating button's watch: it does nothing
/// while a loop is already running.
#[cfg(windows)]
pub fn watch(app: &crate::shell::app::App) {
    use std::sync::atomic::Ordering;

    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    raise(app);
    let app = app.clone();
    crate::shell::app::spawn(async move {
        while crate::session::floating::a_session_is_up(&app)
            && crate::session::floating::the_figures_are_shown(&app)
        {
            let now = Some(banner(&crate::session::measures()));
            let changed = {
                let mut kept = SAID.lock().expect("figures said");
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
pub fn watch(_app: &crate::shell::app::App) {}

/// Opens the banner's window, hidden: it shows itself once drawn.
#[cfg(windows)]
fn raise(app: &crate::shell::app::App) {
    use std::sync::atomic::Ordering;

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    *SAID.lock().expect("figures said") = None;
    // What the figures of a session once took is no reason for the next
    // banner to keep room for it.
    *MEASURED.lock().expect("figures' measures") = None;
    *PROGRAM.lock().expect("figures' program") = Some(app.clone());
    let owner = crate::shell::main_window::handle();
    let _ = app.run_on_main_thread(move || build(owner));
}

/// Puts it away, and the button and the badges back up where it stood.
#[cfg(windows)]
fn lower(app: &crate::shell::app::App) {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    *PROGRAM.lock().expect("figures' program") = None;
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: a window of ours, destroyed on the thread that made it.
        unsafe { DestroyWindow(window as HWND) };
        if STRIP.swap(0, Ordering::Relaxed) != 0
            && let Some(picture) = crate::session::video::where_it_is()
        {
            crate::session::floating::lay_the_button(picture);
        }
    });
}

/// Lays it along the top of the picture.
///
/// Called from where the floating button is laid, so at every step of a
/// hand resizing the window: nothing here waits for anything. A picture
/// of another width asks for a banner of another size, which may take a
/// line more or less: that is a new drawing, asked of the thread that
/// draws, which hands the window its place, its size and its picture in
/// one move. So it is never seen at its new size with its old picture.
#[cfg(windows)]
pub fn lay(picture: (i32, i32, i32, i32)) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if window.is_null() {
        return;
    }
    let mut now = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, whose rectangle is read into ours.
    let same_size = unsafe { GetWindowRect(window, &mut now) } != 0
        && size_for(picture) == Some((now.right - now.left, now.bottom - now.top));
    if !same_size {
        if let Some(app) = PROGRAM.lock().expect("figures' program").clone() {
            let _ = app.run_on_main_thread(repaint);
        }
        return;
    }
    // SAFETY: a window of ours, placed without being activated or
    // resized.
    unsafe {
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            picture.0,
            picture.1,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

#[cfg(not(windows))]
pub fn lay(_picture: (i32, i32, i32, i32)) {}

/// Builds the window, hidden.
#[cfg(windows)]
fn build(owner: isize) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{GetLastError, HWND};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };

    // Asked for by a loop that has ended since, it is not made at all.
    if !WATCHING.load(Ordering::SeqCst) || !crate::session::floating::still_to_be_made(&ITS_WINDOW)
    {
        return;
    }
    let name = zyr_win32::wide("ZyrDeskStatistiques");
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
        // Of no size and nowhere yet: a layered window takes both with its
        // first drawing. Transparent to clicks: the strip it covers is the
        // far computer's, and a hand aiming at something there must reach
        // it.
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
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
            "figures: the window could not open (CreateWindowExW, error {code})"
        ));
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    // What was read while the window was being made is drawn now: the
    // loop only asks again when the figures change.
    repaint();
}

/// Draws the banner with what it says now across the picture, and shows
/// it.
#[cfg(windows)]
fn repaint() {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    // A window put down in the taskbar takes the banner down with it;
    // shown then, the banner would be the only thing left on the desktop.
    if window == 0 || !crate::shell::main_window::on_screen() {
        return;
    }
    let (Some(banner), Some(picture)) = (
        SAID.lock().expect("figures said").clone(),
        crate::session::video::where_it_is(),
    ) else {
        return;
    };
    let scale = crate::shell::main_window::scale();
    let Some(high) = CANVAS.with_borrow_mut(|canvas| draw(canvas, window, &banner, picture, scale))
    else {
        return;
    };
    // The button and the badges hang under it, so they follow it when it
    // shows and when it takes a line more or less.
    if STRIP.swap(high, Ordering::Relaxed) != high {
        crate::session::floating::lay_the_button(picture);
    }
}

/// Draws that banner across that picture and lays it, shown, along its
/// top edge; says how tall it came out.
#[cfg(windows)]
fn draw(
    kept: &mut Option<zyr_draw::Canvas>,
    window: isize,
    banner: &Banner,
    picture: (i32, i32, i32, i32),
    scale: f32,
) -> Option<i32> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};

    use zyr_draw::design::{Colour, DARK};
    use zyr_draw::{Align, Canvas, Rect};

    // Measured on whatever canvas there is: measuring text takes
    // something to measure it with, not a canvas of the right size.
    if kept.is_none() {
        *kept = Canvas::new(1, 1);
    }
    let measured = measure(kept.as_ref()?, banner, scale);
    let (wide, high) = size_of(picture, &measured);
    // Made again when it is no longer the right size, which includes the
    // one-pixel canvas that has just measured.
    if kept.as_ref().is_none_or(|had| had.size() != (wide, high)) {
        *kept = Canvas::new(wide, high);
    }
    let canvas = kept.as_ref()?;
    let pens = Pens::at(scale);
    let line = measured.line;
    let between = BETWEEN * scale;
    // One page pixel.
    let hairline = scale;

    canvas.begin(Colour::TRANSPARENT);
    // Dark whatever the theme, like the badges: it lies on the far
    // computer's desktop, which can be any colour. Edged where it meets
    // the picture.
    let (across, down) = (wide as f32, high as f32);
    canvas.fill(
        Rect::at(0.0, 0.0, across, down),
        0.0,
        DARK.surface_1.faded(0.92),
    );
    canvas.fill(
        Rect::at(0.0, down - hairline, across, hairline),
        0.0,
        DARK.border_strong,
    );
    let pieces = measured
        .flowed(wide)
        .into_iter()
        .zip(banner.pieces())
        .zip(&measured.slots);
    for (((on, along), (word, text)), &slot) in pieces {
        let at = Rect::at(
            ASIDE * scale + along,
            ABOVE * scale + on as f32 * line,
            slot,
            line,
        );
        // A stroke between two pieces of one line, as tall as the type.
        if along > 0.0 {
            canvas.fill(
                Rect::at(
                    at.left - (between + hairline) / 2.0,
                    at.top + (line - pens.word.size) / 2.0,
                    hairline,
                    pens.word.size,
                ),
                0.0,
                DARK.border_strong,
            );
        }
        match word {
            None => canvas.draw_text(text, pens.stream, DARK.text, at),
            Some(word) => {
                canvas.draw_text(&zyr_i18n::text(word), pens.word, DARK.text_faint, at);
                // Against the right of the room the piece keeps: as a
                // figure changes, only its own digits move.
                canvas.draw_text(text, pens.figure.aligned(Align::Right), DARK.text, at);
            }
        }
    }
    if !canvas.finish() || !canvas.lay_on(window, picture.0, picture.1) {
        return None;
    }
    // SAFETY: a window of ours, shown without taking the front.
    unsafe { ShowWindow(window as HWND, SW_SHOWNOACTIVATE) };
    Some(high)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session as it goes.
    fn a_session() -> Measures {
        Measures {
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
        }
    }

    #[test]
    fn a_session_measured_says_every_figure_with_its_unit() {
        let banner = banner(&a_session());
        assert_eq!(banner.stream, "HEVC · 2560x1440 · 60 fps");
        let value = |label: &str| {
            banner
                .rows
                .iter()
                .find(|(named, _)| *named == label)
                .map(|(_, value)| value.clone())
                .unwrap()
        };
        assert_eq!(value("figures.decode"), "0.42 ms");
        assert_eq!(value("figures.display"), "1.30 ms");
        assert_eq!(value("figures.host"), "4.25 ms");
        assert_eq!(value("figures.network"), "12 ms");
        assert_eq!(value("figures.bitrate"), "18.40 Mb/s");
        assert_eq!(value("figures.losses"), "0.0 % on the way, 0.1 % too late");
        assert_eq!(value("figures.latency"), "38 ms");
        // Read in that order, the stream first.
        let pieces: Vec<_> = banner.pieces().collect();
        assert_eq!(pieces.len(), 8);
        assert_eq!(pieces[0], (None, "HEVC · 2560x1440 · 60 fps"));
        assert_eq!(pieces[1], (Some("figures.host"), "4.25 ms"));
    }

    #[test]
    fn a_figure_not_measured_says_so_rather_than_nought() {
        let empty = banner(&Measures::default());
        assert!(
            empty.rows.iter().all(|(_, value)| value == NOTHING),
            "{empty:?}"
        );
        assert_eq!(empty.stream, "");
        // A stream not known yet is said the same way, not left blank.
        assert_eq!(empty.pieces().next(), Some((None, NOTHING)));
        // Half a loss measured is still worth saying.
        let half = Measures {
            dropped_jitter_pct: Some(2.0),
            ..Measures::default()
        };
        assert_eq!(banner(&half).rows[5].1, "- on the way, 2.0 % too late");
    }

    #[test]
    fn a_heavy_session_takes_no_more_room_than_the_banner_keeps_from_the_start() {
        // The figures are written with every digit as wide as the next:
        // what one takes is how many characters it has.
        let heavy = banner(&Measures {
            host_ms: Some(16.67),
            network_ms: Some(250.0),
            decode_ms: Some(12.5),
            render_ms: Some(8.33),
            bitrate_mbps: Some(150.0),
            dropped_network_pct: Some(12.5),
            dropped_jitter_pct: Some(3.2),
            latency_ms: Some(320.0),
            ..a_session()
        });
        let widest = widest();
        assert_eq!(widest.rows[0].1, "88.88 ms");
        for ((word, figure), (_, room)) in heavy.rows.iter().zip(&widest.rows) {
            assert!(
                figure.chars().count() <= room.chars().count(),
                "{word}: « {figure} » does not fit in « {room} »"
            );
        }
    }

    #[test]
    fn the_figures_hold_on_one_line_when_the_picture_is_wide_enough() {
        // 200, then 16 and 90, then 16 and 80: 402 in all.
        let pieces = [200.0, 90.0, 80.0];
        assert_eq!(
            flowed(&pieces, 402.0, 16.0),
            vec![(0, 0.0), (0, 216.0), (0, 322.0)]
        );
    }

    #[test]
    fn a_figure_that_no_longer_fits_starts_the_next_line() {
        let pieces = [200.0, 90.0, 80.0];
        assert_eq!(
            flowed(&pieces, 401.0, 16.0),
            vec![(0, 0.0), (0, 216.0), (1, 0.0)]
        );
        // Narrower, the second starts a line the third still fits on.
        assert_eq!(
            flowed(&pieces, 250.0, 16.0),
            vec![(0, 0.0), (1, 0.0), (1, 106.0)]
        );
        // Narrower still, one on each line.
        assert_eq!(
            flowed(&pieces, 150.0, 16.0),
            vec![(0, 0.0), (1, 0.0), (2, 0.0)]
        );
    }

    #[test]
    fn a_figure_wider_than_the_picture_keeps_a_line_to_itself() {
        // First, it stays on the first line rather than leave it empty.
        assert_eq!(
            flowed(&[500.0, 50.0], 300.0, 16.0),
            vec![(0, 0.0), (1, 0.0)]
        );
        // After others, it starts a line, and whatever follows the next.
        assert_eq!(
            flowed(&[50.0, 500.0, 50.0], 300.0, 16.0),
            vec![(0, 0.0), (1, 0.0), (2, 0.0)]
        );
    }
}
