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
//! Le menu est une fenêtre à côté, dessinée de la même façon : il ne
//! reste plus de vue web nulle part sur l'image.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use tauri::{AppHandle, Manager};

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

    use crate::design::Couleur;

    /// The outline, and the two fills, in the numbers everything that
    /// draws wants.
    const fn teinte(red: u8, green: u8, blue: u8) -> Couleur {
        Couleur {
            red: red as f32 / 255.0,
            green: green as f32 / 255.0,
            blue: blue as f32 / 255.0,
            alpha: 1.0,
        }
    }

    pub const LINE: Couleur = teinte(9, 13, 22);
    pub const WHITE: Couleur = teinte(255, 255, 255);
    pub const GOLD: Couleur = teinte(239, 181, 54);

    /// Half the stroke's width, which is how far it reaches either side
    /// of the path it is drawn on.
    pub const HALF_STROKE: f32 = 14.0;

    /// One rounded rectangle: its middle, its half width and height, the
    /// radius of its corners, and what fills it.
    pub struct Round {
        pub middle: (f32, f32),
        pub half: (f32, f32),
        pub radius: f32,
        pub fill: Couleur,
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

/// The three sizes the logo takes, in the page pixels they were written
/// in: at rest, under a hand, and held.
///
/// The same three to the hundredth. They are what Victor's button does
/// and he has said so twice; what changed is who draws them.
const AT_REST: f32 = 44.0;
const UNDER_A_HAND: f32 = 46.64;
const HELD: f32 = 42.68;

/// The three again, as fractions of the window, which is the largest of
/// them.
///
/// The window never changes size, so the logo is drawn as a share of it:
/// a fraction of one is the whole window, and the standing size is
/// therefore **not** one. Reading it as one drew the logo six per cent
/// too big at rest and made it grow out of its own window under a hand,
/// where it lost its right hand side to the edge.
const STANDING: f32 = AT_REST / UNDER_A_HAND;
const GROWN: f32 = 1.0;
const SHRUNK: f32 = HELD / UNDER_A_HAND;

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
/// Whether a hand is over it, whether one is holding it, and whether the
/// hold has turned into a move.
///
/// Les deux derniers ne sont pas le même moment : un bouton pressé se
/// dessine pressé tout de suite, mais il ne se déplace qu'une fois la
/// main partie, et le curseur doit dire lequel des deux arrive.
static UNDER: AtomicBool = AtomicBool::new(false);
static TAKEN: AtomicBool = AtomicBool::new(false);
static MOVING: AtomicBool = AtomicBool::new(false);
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

// What this window is drawn on, made once and kept: making it is what
// costs, drawing on it is not. The same one the whole interface will be
// drawn on the day the last page goes: two ways of drawing is two
// products that look alike by accident.
//
// Held by the thread rather than by the program, and that is not a
// detail: a drawing surface and the window it dresses belong to the
// thread that made them, so a repaint asked from anywhere else has to
// travel to that thread first. Saying so here is what makes it
// impossible to forget.
thread_local! {
    static TOILE: std::cell::RefCell<Option<crate::paint::Toile>> =
        const { std::cell::RefCell::new(None) };
}

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
    (side as f32 * UNDER_A_HAND / AT_REST).ceil() as u32
}

/// The side of the logo's window, which is what the menu hangs under.
pub fn box_side() -> i32 {
    ITS_BOX.load(Ordering::Relaxed) as i32
}

/// La fenêtre elle-même, pour ce qui a besoin de la nommer au système.
///
/// L'écran sur lequel le bouton pend, et donc son agrandissement, se lit
/// à travers elle : c'est la seule fenêtre du bouton qui soit toujours là.
pub fn its_window() -> isize {
    ITS_WINDOW.load(Ordering::Relaxed)
}

/// Montre ou range le logo depuis le fil qui dessine, où l'on est déjà.
///
/// Le même geste que `shown`, sans le détour par la boucle : ce qui pose
/// le bouton tourne déjà sur ce fil-là, et y repasser par la file des
/// messages retarderait d'une image un bouton qu'on vient de déplacer.
pub fn shown_now(visible: bool) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

    let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if window.is_null() {
        return;
    }
    // SAFETY: a window of ours, shown or hidden without taking the front,
    // on the thread that made it.
    unsafe { ShowWindow(window, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
}

/// Dit que la main qui tient le bouton a commencé à le déplacer, ou
/// qu'elle a fini.
///
/// Le curseur seul en dépend, et c'est bien le déplacement qu'il annonce
/// et non l'appui : un simple clic presse le bouton lui aussi, et il ne
/// doit pas pour autant montrer la croix des quatre directions.
pub fn moving(yes: bool) {
    MOVING.store(yes, Ordering::Relaxed);
}

/// Takes the logo's window down with the session.
pub fn lower(app: &AppHandle) {
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    UNDER.store(false, Ordering::Relaxed);
    TAKEN.store(false, Ordering::Relaxed);
    MOVING.store(false, Ordering::Relaxed);
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
    if UPWARD.swap(upward, Ordering::Relaxed) != upward
        && let Some(app) = PROGRAM.lock().expect("programme du logo").clone()
    {
        // Redemandé au fil qui possède la fenêtre : c'est lui qui tient
        // la toile, et ceci court sur celui qui suit la main.
        let _ = app.run_on_main_thread(move || repaint(window as HWND));
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
    note(&format!(
        "bouton flottant : logo dessiné par ZyrDesk, sans vue web ; \
         fenêtre de {side} px, dessin de {:.0} au repos et {side} sous la main",
        side as f32 * STANDING
    ));
}

/// What the logo is heading for, given what a hand is doing to it.
fn wanted() -> f32 {
    if TAKEN.load(Ordering::Relaxed) {
        SHRUNK
    } else if UNDER.load(Ordering::Relaxed) {
        GROWN
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
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let side = ITS_BOX.load(Ordering::Relaxed) as i32;
    if side <= 0 {
        return;
    }
    let part = drawn(&GROWTH.lock().expect("croissance du logo"));
    let wide = side as f32 * part;
    // The drawing hangs by the right edge, and by the top or the bottom
    // depending on which way the menu opens, so growing and shrinking
    // never moves the corner the window hangs by.
    let left = side as f32 - wide;
    let top = if UPWARD.load(Ordering::Relaxed) {
        side as f32 - wide
    } else {
        0.0
    };
    let per_unit = wide / drawing::SIDE;
    let at = |x: f32, y: f32| {
        (
            left + (x - drawing::ORIGIN) * per_unit,
            top + (y - drawing::ORIGIN) * per_unit,
        )
    };

    TOILE.with_borrow_mut(|toile| {
        if toile.is_none() {
            *toile = crate::paint::Toile::neuve(side, side);
        }
        let Some(toile) = toile.as_ref() else {
            return;
        };
        toile.commence();
        for shape in &drawing::SHAPES {
            let (x, y) = at(shape.middle.0 - shape.half.0, shape.middle.1 - shape.half.1);
            let cadre = crate::paint::Cadre::pose(
                x,
                y,
                shape.half.0 * 2.0 * per_unit,
                shape.half.1 * 2.0 * per_unit,
            );
            let radius = shape.radius * per_unit;
            toile.remplis(cadre, radius, shape.fill);
            if shape.outlined {
                // Sur le bord et non dedans : c'est ce que fait un trait dans
                // le dessin d'origine, et un contour rentré dedans amincirait
                // le logo de la moitié de son trait.
                toile.trace_sur(
                    cadre,
                    radius,
                    drawing::HALF_STROKE * 2.0 * per_unit,
                    drawing::LINE,
                );
            }
        }
        if !toile.finit() {
            return;
        }

        // Placée en même temps qu'elle est peinte : la fenêtre ne peut
        // donc pas être vue à son nouvel endroit avec son ancienne image.
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
        toile.pose(window as isize, place.left, place.top);
    });
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
            let shape = if MOVING.load(Ordering::Relaxed) {
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
        // A plain click opens and closes the menu; a drag does not, or
        // the button would open its menu every time it was put down.
        if plain {
            crate::menu::montre(!crate::menu::ouvert());
        }
    });
}
