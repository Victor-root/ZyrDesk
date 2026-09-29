//! The card's window: raised and put away with the session, shown and
//! hidden under the button, laid where the button hangs, and kept up to
//! date while it is being looked at.

use super::*;

/// How often the bar is read again. Its four figures are averages over
/// the second just gone: read more often, they would only flicker.
const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// Opens the card's window, once per session.
///
/// Built on the drawing thread, like the logo's: a window belongs to the
/// thread that made it, and a window made on the watch's thread would
/// never hear a mouse.
pub fn raise(app: &App, scale: f32, light: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = crate::shell::main_window::handle();
    *PROGRAM.lock().expect("menu's program") = Some(app.clone());
    *KEYS.lock().expect("menu's shortcuts") = crate::shell::shortcuts::engraved();
    // Four dashes before the first read, and not four blanks: the bar is
    // there from the first opening, and what it shows then is what the
    // product shows for a missing reading.
    *READINGS_BAR.lock().expect("menu's readings") =
        ReadingsBar::of(&Measures::default(), &ReadingsBar::empty(), Instant::now());
    store(&SCALE, scale);
    LIGHT.store(light, Ordering::Relaxed);
    OPEN.store(false, Ordering::Relaxed);
    *PANEL.lock().expect("menu's panel") = None;
    let _ = app.run_on_main_thread(move || build(owner));
    // What the session offers, asked for once: the notches do not change
    // from one click to the next. The window is built without waiting,
    // because a closed card has nothing to show and the answer will catch
    // up with it before the first opening.
    reread_the_session_menu(app);
}

/// Asks again what the session offers and where it stands, and starts over
/// as long as the far machine has not said what it can encode.
///
/// It says so when its engine welcomes the player, which can come after
/// the button: asked only once when the button opened, the menu could
/// open offering a codec that particular machine cannot do, and only
/// correct itself once the card was already in front of the eyes.
///
/// A numbered round, as for the readings: two openings close together do
/// not leave two watches behind the same card.
fn reread_the_session_menu(app: &App) {
    let app = app.clone();
    let round = SESSION_MENU_ROUND.fetch_add(1, Ordering::Relaxed) + 1;
    crate::shell::app::spawn(async move {
        while SESSION_MENU_ROUND.load(Ordering::Relaxed) == round
            && ITS_WINDOW.load(Ordering::Relaxed) != 0
        {
            let read = crate::shell::settings::session_menu(app.clone()).await;
            // Asked again until it has said, and no longer: every ask
            // costs a question to the far computer about its screens.
            let answered = read.beyond_it.is_some();
            let change = {
                let mut session_menu = SESSION_MENU.lock().expect("menu's settings");
                let change = session_menu.as_ref() != Some(&read);
                *session_menu = Some(read);
                change
            };
            if change {
                redraw(&app);
            }
            if answered {
                return;
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Closes the card and returns its window with the session.
pub fn lower(app: &App) {
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    OPEN.store(false, Ordering::Relaxed);
    // The readings watch does not put itself away: it follows the
    // card, and a card open at the end of a session does not close, it
    // disappears.
    follow_the_readings(app, false);
    *PROGRAM.lock().expect("menu's program") = None;
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: a window of ours, destroyed on the thread that made
        // it.
        unsafe { DestroyWindow(window as HWND) };
        CANVAS.with_borrow_mut(|canvas| *canvas = None);
    });
}

/// Shows the card, or puts it away.
pub fn show(is_open: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) == 0 || OPEN.swap(is_open, Ordering::Relaxed) == is_open {
        return;
    }
    // A card put away keeps nothing of the hand that was reading it:
    // opened again, it would show a line lit under a mouse resting
    // elsewhere.
    *HOVER.lock().expect("menu's hover") = None;
    *PRESSED.lock().expect("menu's press") = None;
    HAND_INSIDE.store(false, Ordering::Relaxed);
    let Some(app) = PROGRAM.lock().expect("menu's program").clone() else {
        return;
    };
    // A menu opened again opens on itself: staying in a list chosen two
    // sessions ago would be a menu that looks like another one.
    *PANEL.lock().expect("menu's panel") = None;
    *PUSHED.lock().expect("menu's slider") = None;
    // What lives in the card only lives while it is being looked at. The
    // switches and the settings are read again at every opening because
    // they may have moved without it.
    follow_the_readings(&app, is_open);
    if is_open {
        reread_the_toggles(&app);
        reread_the_session_menu(&app);
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if window.is_null() {
            return;
        }
        if is_open {
            repaint(window);
        }
        // SAFETY: a window of ours, shown without taking the
        // foreground.
        unsafe {
            ShowWindow(window, if is_open { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    });
}

/// Says whether the card is open, for whoever needs to toggle it.
pub fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

/// How tall its window is.
///
/// For the button, which uses it to decide whether the menu has
/// room to open downwards.
pub fn height() -> i32 {
    HEIGHT.load(Ordering::Relaxed) as i32
}

/// How wide its window is in all, for that vertical direction.
///
/// Sideways, it also counts the button and the space between them: it
/// is its whole window that is laid beside the button, never its card
/// alone, see `lay`. For the button, which uses it to decide from
/// which edge there is room to send it off.
pub fn width(opens: Opens, logo: i32) -> i32 {
    let width = WIDTH.load(Ordering::Relaxed) as i32;
    match opens {
        Opens::Side => logo + (design::SPACE_2 * scale()).round() as i32 + width,
        _ => width,
    }
}

/// Lays the card under the logo, above it, or beside it, in the
/// direction the button decided; and by its right edge or its left
/// edge, whichever it decided has the room to carry the card.
///
/// The same anchor as the logo, in the same move: so the two windows
/// cannot disagree about where the button is.
pub fn lay(
    anchor: (i32, i32),
    opens: Opens,
    on_the_right: bool,
    logo: i32,
    picture: (i32, i32, i32, i32),
) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let (width, height) = (
        WIDTH.load(Ordering::Relaxed) as i32,
        HEIGHT.load(Ordering::Relaxed) as i32,
    );
    // The window is larger than the card by all that the shadow spills
    // over: so it is the **card** that is laid, and the window around
    // it. Laid as if the two were one, the card fell twenty pixels too
    // low and twenty too far left, which shows at first glance next to
    // the old menu.
    let scale = scale();
    // The canvas carries the card drawn for the edge it set out from,
    // and nothing redraws it by itself: without this, an edge that had
    // just changed moved the window at once, over a picture still laid
    // out for the old one, which showed for a glimpse before the next
    // drawing.
    let vertical_change =
        UPWARD.swap(opens == Opens::Up, Ordering::Relaxed) != (opens == Opens::Up);
    let horizontal_change = RIGHTWARD.swap(on_the_right, Ordering::Relaxed) != on_the_right;
    if (vertical_change || horizontal_change)
        && let Some(app) = PROGRAM.lock().expect("menu's program").clone()
    {
        // Asked again of the thread that owns the window: it is the one
        // holding the canvas, and this runs on the one that follows the
        // hand.
        let _ = app.run_on_main_thread(move || repaint(window as HWND));
    }
    let overflow_px = shadow_overflow(scale).round() as i32;
    let card_height = height - overflow_px * 2;
    // Stuck to the same edge as the logo, and separated from it by one
    // step of the design system: its right edge as a rule, its left edge
    // when the first has no room, which the button has already decided.
    let between = (design::SPACE_2 * scale).round() as i32;
    // The corner `SetWindowPos` receives further down takes yet one more
    // overflow, for a reason that stands higher up in this function: laid
    // as it is, the card fell twenty pixels too far left. When it is the
    // card that is stuck to that edge rather than left at the right edge,
    // it carries a second overflow itself (its own shadow, `card` laying it
    // at `overflow_px` and not at zero), and the two add up without either
    // accounting for the other: without taking it off twice here, the
    // card's edge would have fallen two overflows past the button rather
    // than at the same place as it.
    let horizontal = if on_the_right {
        anchor.0 - logo - overflow_px * 2
    } else {
        anchor.0 - width
    };
    let (left, top) = match opens {
        Opens::Down => (horizontal, anchor.1 + logo + between - overflow_px),
        Opens::Up => (
            horizontal,
            anchor.1 - logo - between - card_height - overflow_px,
        ),
        // To the side, the card starts from the top of the button and
        // slides by as much as it takes to fit in the picture: that is
        // its whole reason for being there rather than below. Its whole
        // window and not its card alone, since a list's panel opens
        // inside it.
        Opens::Side => (
            if on_the_right {
                anchor.0 + between - overflow_px * 2
            } else {
                anchor.0 - logo - between - width
            },
            (anchor.1 - overflow_px).clamp(picture.1, (picture.3 - height).max(picture.1)),
        ),
    };
    // SAFETY: a window of ours, laid without being activated
    // or resized.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            left + overflow_px,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

/// Builds the window, at the size its lines ask for.
pub(super) fn build(owner: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, IDC_ARROW, LoadCursorW, RegisterClassW, WNDCLASSW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    /// The class's name, in the characters Windows counts, ended by the
    /// zero it looks for.
    const CLASS: [u16; 13] = [
        b'Z' as u16,
        b'y' as u16,
        b'r' as u16,
        b'D' as u16,
        b'e' as u16,
        b's' as u16,
        b'k' as u16,
        b'M' as u16,
        b'e' as u16,
        b'n' as u16,
        b'u' as u16,
        0,
        0,
    ];

    if !crate::session::floating::still_to_be_made(&ITS_WINDOW) {
        return;
    }
    // The size is measured before the window exists: it depends on the
    // text, and measuring text takes something to draw it with.
    let Some(measure) = Canvas::new(1, 1) else {
        note("floating button: the menu could not be measured");
        return;
    };
    let scale = scale();
    // The height of a line of text, asked of the font once and for all:
    // everything stacked in this card rests on it.
    store(
        &CAPTION_HEIGHT,
        measure.line_height(Pen::of(design::CAPTION * scale)),
    );
    store(
        &BODY_HEIGHT,
        measure.line_height(Pen::of(design::BODY * scale)),
    );
    measure_the_card(&measure, scale);
    let (width, height) = size(&measure);
    WIDTH.store(width as u32, Ordering::Relaxed);
    HEIGHT.store(height as u32, Ordering::Relaxed);
    drop(measure);

    // SAFETY: a class declared once and a window built on it, on the
    // thread that will pump its messages. A class declared twice is
    // refused with no other effect, hence the unread answer: the second
    // session finds the first one's again.
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
            0,
            0,
            width,
            height,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("floating button: the menu's window could not open");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    note(&format!(
        "floating button: menu drawn by ZyrDesk, {width}x{height} px to begin \
         with; the window then follows what the card asks for"
    ));
}

/// Rereads where the switches stand, and redraws if anything moved.
///
/// Each is read where it lives: the mouse and the keyboard in what takes
/// them, the sound in the player that plays it, the two others in this
/// window.
pub(super) fn reread_the_toggles(app: &App) {
    /// Sets where a switch stands, and says if it moved.
    fn set(cell: &AtomicBool, value: bool) -> bool {
        cell.swap(value, Ordering::Relaxed) != value
    }

    let mut change = set(&IN_GAME, crate::session::video::in_a_game());
    change |= set(&IMMERSIVE, crate::session::system_keys::immersive());
    change |= set(
        &SHARED,
        crate::session::floating::the_clipboard_is_shared(app),
    );
    change |= set(&HELD, crate::session::floating::the_badges_are_held_up(app));
    // Without a session there is no player to ask, and the card does not
    // open without a session: the switch is then left as it is rather
    // than turned off.
    if let Some(muted) = crate::session::floating::hushed() {
        change |= set(&MUTED, muted);
    }
    if change {
        redraw(app);
    }
}

/// Follows what the session costs while the card is open, and not a
/// second longer: figures nobody looks at are worth no wake-up.
fn follow_the_readings(app: &App, is_open: bool) {
    // The round changes at every call, which stops the one before:
    // without that, opening and closing quickly would leave two watches
    // behind the same card.
    let round = ROUND.fetch_add(1, Ordering::Relaxed) + 1;
    if !is_open {
        return;
    }
    let app = app.clone();
    crate::shell::app::spawn(async move {
        while ROUND.load(Ordering::Relaxed) == round {
            let said = crate::session::measures();
            let now = Instant::now();
            // The lock is given back before the wait: a lock held
            // across a wait is a lock held for a second. Taken before
            // the read and not after, because the read starts from the
            // previous one for the missing readings.
            let change = {
                let mut bar = READINGS_BAR.lock().expect("menu's readings");
                let load = ReadingsBar::of(&said, &bar, now);
                let change = bar.reads_differently(&load);
                *bar = load;
                change
            };
            if change {
                redraw(&app);
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Redraws the card from a thread that is not the one drawing it.
pub(super) fn redraw(app: &App) {
    let _ = app.run_on_main_thread(|| {
        use windows_sys::Win32::Foundation::HWND;

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if !window.is_null() {
            repaint(window);
        }
    });
}
