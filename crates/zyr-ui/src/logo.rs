//! The floating button's logo, drawn by this program into a window of
//! its own, without a web view anywhere near it.
//!
//! Why it exists. The pale edging on that button was hunted through
//! eleven trials, and eleven said no: not the cut, whether refreshed
//! every frame or frozen for the session; not the redraw, with no erase,
//! without the web view, or not asked for at all; not the layered
//! attribute; not the window's ground; not its transparency; not the
//! resize message; and not the move, since the last trial nailed the
//! window down and the edging was still there. The page was cleared too,
//! measured rather than argued: rendered in a browser at 125 and 175 per
//! cent, over black and over white, at rest and stopped dead at seven
//! points of its animation, the brightest pixel of the four surrounding
//! the drawing is nought in every case.
//!
//! What was never taken out of the picture is the web view itself, which
//! is also the only layer in there whose own ground is white. So it comes
//! out.
//!
//! And with it goes a great deal more than a browser. This window is
//! **layered by the picture it is given**: we hand Windows a rectangle of
//! pixels that each carry their own transparency, and that is the whole
//! of the window. There is no shape to cut, because the shape is the
//! transparency; no ground to erase, because nothing is ever erased; no
//! frame; and no clicks to let through, because the system already lets
//! them through wherever the picture is clear. Four of the faults this
//! button has worn since it was born cannot happen here at all.
//!
//! What is left in the web view is the menu, which is real interface and
//! has no business being redrawn by hand. It never moves, and it only
//! exists while it is open.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use tauri::{AppHandle, Emitter, Manager};

use crate::journal::note;

/// The drawing, in the units of its own file, which is what makes this
/// the same logo as everywhere else in the product rather than a second
/// one that drifts.
///
/// `zyrdesk.svg` draws two screens with a stroke centred on their path,
/// so each is painted from half a stroke outside its rectangle to half a
/// stroke inside. Its frame runs from 36 to 476, four hundred and forty
/// units wide, and everything below is counted in those.
mod drawing {
    /// The frame's width, and the origin of its top left corner.
    pub const SIDE: f32 = 440.0;
    pub const ORIGIN: f32 = 36.0;

    /// The outline, and the two fills.
    pub const LINE: [f32; 3] = [9.0, 13.0, 22.0];
    pub const WHITE: [f32; 3] = [255.0, 255.0, 255.0];
    pub const GOLD: [f32; 3] = [239.0, 181.0, 54.0];

    /// Half the stroke's width, which is how far it reaches either side
    /// of the path it is drawn on.
    pub const HALF_STROKE: f32 = 14.0;

    /// One rounded rectangle: its middle, its half width and height, the
    /// radius of its corners, and what fills it.
    pub struct Round {
        pub middle: (f32, f32),
        pub half: (f32, f32),
        pub radius: f32,
        pub fill: [f32; 3],
        pub outlined: bool,
    }

    /// The four, in the order the file draws them: the far screen and its
    /// dark pane, then the near one over it and its own.
    pub const SHAPES: [Round; 4] = [
        Round {
            middle: (282.0, 193.0),
            half: (164.0, 123.0),
            radius: 68.0,
            fill: WHITE,
            outlined: true,
        },
        Round {
            middle: (282.0, 193.0),
            half: (100.0, 59.0),
            radius: 24.0,
            fill: LINE,
            outlined: false,
        },
        Round {
            middle: (230.0, 319.0),
            half: (164.0, 123.0),
            radius: 68.0,
            fill: GOLD,
            outlined: true,
        },
        Round {
            middle: (230.0, 319.0),
            half: (100.0, 59.0),
            radius: 24.0,
            fill: LINE,
            outlined: false,
        },
    ];
}

/// The three sizes the logo takes, as fractions of its standing one.
///
/// The same three the page used, kept to the hundredth: at rest, under a
/// hand, and held. They are what Victor's button does and he has said so
/// twice; what changed is who draws them.
const HELD: f32 = 0.97;
const STANDING: f32 = 1.0;
const UNDER_A_HAND: f32 = 1.06;

/// How long the logo takes to change size, and how often it is redrawn
/// while it does.
const GROWS_IN: std::time::Duration = std::time::Duration::from_millis(120);
const EVERY: u32 = 8;

/// The timer that carries that growth, named so nothing else answers it.
const GROWING: usize = 1;

/// The window itself, and what it is showing.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// The side of the window in real pixels, which is the largest of the
/// three sizes and never changes while a session lasts.
static ITS_BOX: AtomicU32 = AtomicU32::new(0);
/// Whether a hand is over it, and whether one is holding it.
static UNDER: AtomicBool = AtomicBool::new(false);
static TAKEN: AtomicBool = AtomicBool::new(false);
/// Which way the menu opens, which is the corner the logo is drawn in.
static UPWARD: AtomicBool = AtomicBool::new(false);

/// Where the growth is: what it left, what it is heading for, and when
/// it set off.
///
/// Counted in time and not in steps: a share of what is left taken at
/// every tick never quite arrives, and the clock would go on ticking long
/// after the drawing looked still.
struct Growth {
    from: f32,
    to: f32,
    since: Option<std::time::Instant>,
}

static GROWTH: Mutex<Growth> = Mutex::new(Growth {
    from: STANDING,
    to: STANDING,
    since: None,
});

/// The program, kept here because this window is the only thing that
/// needs it and nothing hands it one: a hand comes down on it from the
/// system, not from the toolkit.
static PROGRAM: Mutex<Option<AppHandle>> = Mutex::new(None);

/// Opens the logo's window, once per session.
///
/// Built on the thread that draws, which is the only one whose messages
/// are ever pumped: a window belongs to the thread that made it, and one
/// made on the watch's thread would never hear a mouse.
pub fn raise(app: &AppHandle, side: u32, upward: bool, anchor: (i32, i32)) {
    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = app
        .get_webview_window(crate::HOME)
        .and_then(|home| home.hwnd().ok())
        .map(|handle| handle.0 as isize)
        .unwrap_or(0);
    *PROGRAM.lock().expect("programme du logo") = Some(app.clone());
    ITS_BOX.store(box_of(side), Ordering::Relaxed);
    UPWARD.store(upward, Ordering::Relaxed);
    *GROWTH.lock().expect("croissance du logo") = Growth {
        from: STANDING,
        to: STANDING,
        since: None,
    };
    let _ = app.run_on_main_thread(move || build(owner, anchor));
}

/// The window's side: the largest of the three sizes, so the logo grows
/// and shrinks inside a window that never changes size.
///
/// A window that resizes under a hand is a window the system has to lay
/// out again, and this one is laid a hundred and twenty times a second
/// while it is dragged.
fn box_of(side: u32) -> u32 {
    (side as f32 * UNDER_A_HAND).ceil() as u32
}

/// Takes the logo's window down with the session.
pub fn lower(app: &AppHandle) {
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    UNDER.store(false, Ordering::Relaxed);
    TAKEN.store(false, Ordering::Relaxed);
    *PROGRAM.lock().expect("programme du logo") = None;
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: a window of ours, destroyed on the thread that made it.
        unsafe { DestroyWindow(window as HWND) };
    });
}

/// Shows or hides the logo, for the menu entry that puts the button away
/// and the shortcut that brings it back.
pub fn shown(app: &AppHandle, visible: bool) {
    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        // SAFETY: a window of ours, shown or hidden without taking the
        // front, on the thread that made it.
        unsafe {
            ShowWindow(
                window as HWND,
                if visible { SW_SHOWNOACTIVATE } else { SW_HIDE },
            )
        };
    });
}

/// Lays the logo where the button hangs, and says which corner it hangs
/// by.
///
/// `anchor` is the corner the whole button hangs from, the top right of
/// the picture as moved by whatever dragging has moved it. Called from
/// wherever the button is placed, which is a hundred and twenty times a
/// second under a hand, so nothing here waits for anything.
pub fn lay(anchor: (i32, i32), upward: bool) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    if UPWARD.swap(upward, Ordering::Relaxed) != upward {
        repaint(window as HWND);
    }
    let side = ITS_BOX.load(Ordering::Relaxed) as i32;
    // Hung by its top right corner, or by its bottom right one when the
    // menu opens upward: the logo is the one part of this button nobody
    // may see move, so it keeps the corner and the rest gives way.
    let top = if upward { anchor.1 - side } else { anchor.1 };
    // SAFETY: a window of ours, placed without being activated and
    // without being resized. Placing a window from another thread is
    // asked of the system, not done here, which is what makes it safe to
    // call from the one that follows a hand.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            anchor.0 - side,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

/// Builds the window and puts its first picture in it.
fn build(owner: isize, anchor: (i32, i32)) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, IDC_ARROW, LoadCursorW, RegisterClassW,
        SW_SHOWNOACTIVATE, ShowWindow, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_POPUP,
    };

    /// The class name, in the wide characters Windows counts in, ending
    /// in the nought it looks for.
    const CLASS: [u16; 13] = [
        b'Z' as u16,
        b'y' as u16,
        b'r' as u16,
        b'D' as u16,
        b'e' as u16,
        b's' as u16,
        b'k' as u16,
        b'L' as u16,
        b'o' as u16,
        b'g' as u16,
        b'o' as u16,
        0,
        0,
    ];

    let side = ITS_BOX.load(Ordering::Relaxed) as i32;
    // Born where it belongs rather than at the corner of the screen: a
    // window is shown where it was made, and the page only asks for it to
    // be placed again on its next frame.
    let top = if UPWARD.load(Ordering::Relaxed) {
        anchor.1 - side
    } else {
        anchor.1
    };
    // SAFETY: a class registered once and a window built from it, on the
    // thread that will pump its messages. Registering a class twice is
    // answered by the system with a refusal and nothing else, which is
    // why the answer is not read: a second session finds the class of the
    // first still there.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: CLASS.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            CLASS.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            anchor.0 - side,
            top,
            side,
            side,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("bouton flottant : la fenêtre du logo n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    repaint(window);
    // SAFETY: a window of ours, shown without taking the front.
    unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
    note("bouton flottant : logo dessiné par ZyrDesk, sans vue web");
}

/// What the logo is heading for, given what a hand is doing to it.
fn wanted() -> f32 {
    if TAKEN.load(Ordering::Relaxed) {
        HELD
    } else if UNDER.load(Ordering::Relaxed) {
        UNDER_A_HAND
    } else {
        STANDING
    }
}

/// Starts the logo growing towards what it should be, and says so to the
/// clock that carries it there.
fn head_for(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetTimer;

    let aim = wanted();
    {
        let mut growth = GROWTH.lock().expect("croissance du logo");
        if (growth.to - aim).abs() < f32::EPSILON {
            return;
        }
        growth.from = drawn(&growth);
        growth.to = aim;
        growth.since = Some(std::time::Instant::now());
    }
    // SAFETY: a window of ours, given a clock named by us.
    unsafe { SetTimer(window, GROWING, EVERY, None) };
}

/// What fraction of its full size the logo is drawn at right now.
///
/// Fast at first and slow at the end, which is the curve the page used
/// and the one a size change wants: what the eye follows is the start.
fn drawn(growth: &Growth) -> f32 {
    let Some(since) = growth.since else {
        return growth.to;
    };
    let part = since.elapsed().as_secs_f32() / GROWS_IN.as_secs_f32();
    if part >= 1.0 {
        return growth.to;
    }
    let eased = 1.0 - (1.0 - part).powi(3);
    growth.from + (growth.to - growth.from) * eased
}

/// Whether the growth has arrived, so its clock can be put away.
fn arrived() -> bool {
    let mut growth = GROWTH.lock().expect("croissance du logo");
    let done = growth.since.is_none_or(|since| since.elapsed() >= GROWS_IN);
    if done {
        growth.from = growth.to;
        growth.since = None;
    }
    done
}

/// Draws the logo as it stands and hands the whole picture to Windows.
///
/// Not a repaint in the ordinary sense: nothing is invalidated, no
/// message is queued, and the system never asks this window what it looks
/// like. The picture **is** the window, transparency included, and it is
/// replaced whole. That is what leaves nothing for a compositor to guess
/// at between two frames.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::{POINT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
        ReleaseDC, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};

    let side = ITS_BOX.load(Ordering::Relaxed) as i32;
    if side <= 0 {
        return;
    }
    let part = drawn(&GROWTH.lock().expect("croissance du logo"));

    // SAFETY: a bitmap of ours from first to last, drawn into through the
    // pointer the system hands back, hung on a device of ours, given to
    // the window, and then taken apart in the order it was built.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let surface = CreateCompatibleDC(screen);
        let mut about: BITMAPINFO = std::mem::zeroed();
        about.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side,
            // Upside down, which is how a bitmap counts its rows unless
            // it is told otherwise, and the way round the rest of this
            // file thinks.
            biHeight: -side,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut pixels: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(
            surface,
            &about,
            DIB_RGB_COLORS,
            &mut pixels,
            std::ptr::null_mut(),
            0,
        );
        if bitmap.is_null() || pixels.is_null() {
            DeleteDC(surface);
            ReleaseDC(std::ptr::null_mut(), screen);
            return;
        }
        let held = SelectObject(surface, bitmap as _);
        let count = (side * side) as usize;
        draw(
            std::slice::from_raw_parts_mut(pixels.cast::<u32>(), count),
            side,
            part,
            UPWARD.load(Ordering::Relaxed),
        );

        let size = SIZE { cx: side, cy: side };
        let from = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        UpdateLayeredWindow(
            window,
            screen,
            std::ptr::null(),
            &size,
            surface,
            &from,
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(surface, held);
        DeleteObject(bitmap as _);
        DeleteDC(surface);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
}

/// Draws the logo into a square of premultiplied pixels.
///
/// `side` is the square's, which is the largest the logo ever gets;
/// `part` is what fraction of that the logo is drawn at, and it keeps the
/// corner the window hangs by, so growing and shrinking never moves it.
///
/// Every pixel is worked out from the drawing's own geometry rather than
/// from a picture scaled to fit: a rounded rectangle knows exactly how
/// much of each pixel it covers, and that is what a smooth edge is. The
/// only thing that ever showed a hard edge on this button was a stencil
/// with one bit per pixel, and there is none here.
fn draw(into: &mut [u32], side: i32, part: f32, upward: bool) {
    let wide = side as f32 * part;
    // The drawing hangs by the right edge, and by the top or the bottom
    // depending on which way the menu opens.
    let left = side as f32 - wide;
    let top = if upward { side as f32 - wide } else { 0.0 };
    // How many pixels one unit of the drawing comes to.
    let per_unit = wide / drawing::SIDE;

    for y in 0..side {
        for x in 0..side {
            // The middle of this pixel, in the drawing's own units.
            let unit = (
                (x as f32 + 0.5 - left) / per_unit + drawing::ORIGIN,
                (y as f32 + 0.5 - top) / per_unit + drawing::ORIGIN,
            );
            let mut colour = [0.0f32; 3];
            let mut alpha = 0.0f32;
            for shape in &drawing::SHAPES {
                let away = how_far(unit, shape.middle, shape.half, shape.radius) * per_unit;
                let stroke = if shape.outlined {
                    drawing::HALF_STROKE * per_unit
                } else {
                    0.0
                };
                // What this shape covers of this pixel, outside edge and
                // inside edge apart: between the two is the outline, and
                // inside the second is the fill.
                let out = (0.5 - (away - stroke)).clamp(0.0, 1.0);
                let inside = (0.5 - (away + stroke)).clamp(0.0, 1.0);
                if out <= 0.0 {
                    continue;
                }
                for (band, under) in colour.iter_mut().enumerate() {
                    let over = drawing::LINE[band] * (out - inside) + shape.fill[band] * inside;
                    *under = over + *under * (1.0 - out);
                }
                alpha = out + alpha * (1.0 - out);
            }
            let byte = |value: f32| value.round().clamp(0.0, 255.0) as u32;
            into[(y * side + x) as usize] = (byte(alpha * 255.0) << 24)
                | (byte(colour[0]) << 16)
                | (byte(colour[1]) << 8)
                | byte(colour[2]);
        }
    }
}

/// How far a point is from the edge of a rounded rectangle, in the
/// drawing's units and negative inside it.
///
/// The one measurement the whole picture is made of: a pixel's share of a
/// shape is what is left of half a pixel once this is taken off it, which
/// is exact on the straight edges and true to a thousandth on the curves.
fn how_far(point: (f32, f32), middle: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let out = (
        (point.0 - middle.0).abs() - (half.0 - radius),
        (point.1 - middle.1).abs() - (half.1 - radius),
    );
    let corner = (out.0.max(0.0).powi(2) + out.1.max(0.0).powi(2)).sqrt();
    corner + out.0.max(out.1).min(0.0) - radius
}

/// What the window answers when the system speaks to it.
///
/// SAFETY: called by the system on the thread that made this window, with
/// the arguments it documents.
unsafe extern "system" fn answer(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTCLIENT, IDC_HAND, IDC_SIZEALL, KillTimer, LoadCursorW, SetCursor,
        WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_SETCURSOR, WM_TIMER,
    };

    match message {
        WM_MOUSEMOVE => {
            if !UNDER.swap(true, Ordering::Relaxed) {
                // Asked for once a hand arrives: without it nothing ever
                // says it left, and the logo would stay grown for the
                // rest of the session.
                let mut watch = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                unsafe { TrackMouseEvent(&mut watch) };
                head_for(window);
            }
            0
        }
        WM_MOUSELEAVE => {
            UNDER.store(false, Ordering::Relaxed);
            head_for(window);
            0
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            let shape = if TAKEN.load(Ordering::Relaxed) {
                IDC_SIZEALL
            } else {
                IDC_HAND
            };
            // SAFETY: a cursor of the system's own, asked for by name.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), shape)) };
            1
        }
        WM_LBUTTONDOWN => {
            taken(window);
            0
        }
        WM_TIMER if holding == GROWING => {
            if arrived() {
                // SAFETY: a clock of ours on a window of ours.
                unsafe { KillTimer(window, GROWING) };
            }
            repaint(window);
            0
        }
        // SAFETY: the system's own answer to everything not answered here.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// The hand that comes down on the logo.
///
/// The whole gesture belongs to the core, which follows the cursor rather
/// than this window: a window forty-four pixels wide loses the mouse on
/// the first movement, and where the system says the cursor is is the
/// only answer that is always true. What is left here is saying that the
/// logo is held, so it draws itself held.
fn taken(window: windows_sys::Win32::Foundation::HWND) {
    let Some(app) = PROGRAM.lock().expect("programme du logo").clone() else {
        return;
    };
    if TAKEN.swap(true, Ordering::Relaxed) {
        return;
    }
    head_for(window);
    let handle = window as isize;
    tauri::async_runtime::spawn(async move {
        let plain = crate::floating::grabbed(&app).await;
        TAKEN.store(false, Ordering::Relaxed);
        head_for(handle as windows_sys::Win32::Foundation::HWND);
        // A plain click is what opens and closes the menu, and the menu
        // belongs to the page: this window has none of it and wants none.
        if plain && let Some(menu) = app.get_webview_window(crate::floating::WINDOW) {
            let _ = menu.emit(crate::floating::TOGGLE, ());
        }
    });
}
