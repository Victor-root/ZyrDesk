//! The icon beside the clock, and what it says.
//!
//! It is the whole answer to a question a remote desktop must never
//! leave unanswered: is this computer reachable right now? Everything
//! else about ZyrDesk can be closed, minimised or forgotten; this stays
//! for as long as the product runs, and it goes out with it.
//!
//! So the icon is not decoration. It is bright while this computer can
//! be taken over and dim while it cannot, its tooltip says which in
//! words, and the one thing its menu offers besides opening the window
//! is a way to stop everything at once.
//!
//! **It is drawn**, like everything else in the product, at the exact
//! size the bar asks for. So there is no picture to shrink or to enlarge:
//! that was the only way to have an icon that is sharp at sixteen pixels
//! as well as at twenty-eight, and it is now the same mark as that of the
//! floating button and of the home window, traced by the same drawing.

// A notification area is a thing of the system, and this product only
// runs on Windows. Elsewhere, there is no icon to put up.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::app::App;

/// What this module's lines are filed under.
const TAG: &str = "tray";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// What the menu answers when one of its lines is chosen.
const OPEN: usize = 1;
const QUIT: usize = 2;

/// What is left of the mark when this computer cannot be reached.
///
/// Faded rather than a different drawing: it stays recognisable at
/// sixteen pixels, where a second symbol would only be a smudge.
const DIMMED: f32 = 90.0 / 255.0;

/// What the icon last said, so it is only redrawn when it changes.
///
/// Windows redraws the notification area on every change, and a product
/// that rewrote its own icon twice a second would be visible for that
/// alone.
///
/// Two things are said, so both are remembered: whether this computer can
/// be reached, and whether a session is running from it.
#[derive(Default)]
pub struct Shown(Mutex<Option<(bool, bool)>>);

/// The window that receives what the icon has to say, and the icon
/// itself as the system keeps it.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static ITS_ICON: AtomicIsize = AtomicIsize::new(0);

/// The number this icon is put up under, and the message it speaks
/// through.
const ICON_ID: u32 = 1;
#[cfg(windows)]
const CALLBACK: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Puts the icon up, for as long as the program runs.
#[cfg(windows)]
pub fn raise() -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, HWND_MESSAGE, RegisterClassW, WNDCLASSW,
    };

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return Ok(());
    }
    let class_name: Vec<u16> = "ZyrDeskIcone".encode_utf16().chain(Some(0)).collect();
    // SAFETY: a class declared once and a window built on it, on the
    // thread that will pump its messages. It shows nothing: it is what
    // the system asks for to carry an icon.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(answers),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err("l'icône de la zone de notification n'a pas de fenêtre".to_string());
    }
    ITS_WINDOW.store(hwnd as isize, Ordering::Relaxed);

    let mut data = icon_data(hwnd);
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = CALLBACK;
    data.hIcon = drawn(false);
    ITS_ICON.store(data.hIcon as isize, Ordering::Relaxed);
    copy_into(&mut data.szTip, "ZyrDesk");
    // SAFETY: a block of ours, with its size written into it as the
    // call asks.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
        return Err("Windows n'a pas pris l'icône de la zone de notification".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn raise() -> Result<(), String> {
    Err("il n'y a pas de zone de notification hors de Windows".to_string())
}

/// The block the system expects, filled with what never changes.
#[cfg(windows)]
fn icon_data(
    hwnd: windows_sys::Win32::Foundation::HWND,
) -> windows_sys::Win32::UI::Shell::NOTIFYICONDATAW {
    use windows_sys::Win32::UI::Shell::NOTIFYICONDATAW;

    // SAFETY: a block of ours, filled with zeros and then with only the
    // fields the flags announce.
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = ICON_ID;
    data
}

/// Writes a word into one of the system's fixed-length fields.
#[cfg(windows)]
fn copy_into(cursor: &mut [u16], text: &str) {
    for (place, letter) in cursor.iter_mut().zip(text.encode_utf16().chain(Some(0))) {
        *place = letter;
    }
}

/// How often the icon asks what this computer is doing.
///
/// From here rather than from anywhere that draws: a window that is
/// hidden has its timers slowed to a crawl by the system, and the icon
/// has to keep telling the truth precisely when the window is nowhere to
/// be seen.
const LOOK: std::time::Duration = std::time::Duration::from_secs(3);

/// Keeps the icon saying the truth, for as long as the program runs.
pub fn watch(app: App) {
    crate::app::spawn(async move {
        loop {
            let standing = crate::desk::standing().await;
            let playing = crate::floating::a_session_is_up(&app);
            says(&app, standing.hosting, playing);
            tokio::time::sleep(LOOK).await;
        }
    });
}

/// Says what this computer is doing, in the icon and in its tooltip.
///
/// A session in progress comes before anything else in the tooltip, and
/// for one reason: the window can be closed while it runs, the picture
/// goes away with it, and this icon is then the only thing on screen that
/// says the far computer is still being held. Somebody who has forgotten
/// that has to be able to read it here.
#[cfg(windows)]
fn says(app: &App, reachable: bool, playing: bool) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Shell::{NIF_ICON, NIF_TIP, NIM_MODIFY, Shell_NotifyIconW};
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let mut last = app.shown().0.lock().expect("état de l'icône");
    if *last == Some((reachable, playing)) {
        return;
    }
    let hwnd = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return;
    }
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_ICON | NIF_TIP;
    data.hIcon = drawn(!reachable);
    copy_into(
        &mut data.szTip,
        match (playing, reachable) {
            (true, _) => "ZyrDesk : une session est en cours, cliquez pour revenir à la fenêtre",
            (false, true) => "ZyrDesk : cet ordinateur peut être contrôlé",
            (false, false) => "ZyrDesk : cet ordinateur n'est pas joignable",
        },
    );
    // SAFETY: a block of ours, and the old drawing given back once the
    // system no longer uses it.
    unsafe {
        if Shell_NotifyIconW(NIM_MODIFY, &data) == 0 {
            let _ = DestroyIcon(data.hIcon);
            return;
        }
        let before = ITS_ICON.swap(data.hIcon as isize, Ordering::Relaxed);
        if before != 0 {
            let _ = DestroyIcon(before as _);
        }
    }
    *last = Some((reachable, playing));
}

#[cfg(not(windows))]
fn says(_app: &App, _reachable: bool, _playing: bool) {}

/// The side, in real pixels, this bar draws an icon at.
///
/// Sixteen logical pixels, multiplied by the scaling of the screen it is
/// on: sixteen at a hundred per cent, twenty-eight at a hundred and
/// seventy-five, and so on. Asked of the system rather than worked out,
/// since it is the system that decides.
#[cfg(windows)]
fn asked_for() -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

    // SAFETY: no argument beyond the metric asked for.
    unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16)
}

/// The mark, traced at the size the bar asks for.
///
/// Nothing is shrunk or enlarged: the drawing itself is made at that
/// size, which is the only way to have a sharp edge at sixteen pixels.
#[cfg(windows)]
fn drawn(dimmed: bool) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    let side = asked_for();
    let Some(canvas) = crate::paint::Canvas::new(side, side) else {
        return std::ptr::null_mut();
    };
    canvas.begin(crate::design::Colour::TRANSPARENT);
    crate::logo::brand(
        &canvas,
        crate::paint::Rect::at(0.0, 0.0, side as f32, side as f32),
        if dimmed { DIMMED } else { 1.0 },
        false,
    );
    if !canvas.finish() {
        return std::ptr::null_mut();
    }
    canvas
        .to_icon()
        .map_or(std::ptr::null_mut(), |icon| icon.0 as _)
}

/// SAFETY: called by the system on the thread that made this window,
/// with the arguments it documents.
#[cfg(windows)]
unsafe extern "system" fn answers(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_LBUTTONUP, WM_RBUTTONUP};

    if message == CALLBACK {
        match (with & 0xFFFF) as u32 {
            // The left click opens the window, which everyone expects of
            // an icon down there; the menu stays on the right button.
            WM_LBUTTONUP => open(),
            WM_RBUTTONUP => pop_up_the_menu(window),
            _ => {}
        }
        return 0;
    }
    // SAFETY: the system's answer to everything not answered here.
    unsafe { DefWindowProcW(window, message, holding, with) }
}

/// What the icon's menu offers, and what it does with the answer.
#[cfg(windows)]
fn pop_up_the_menu(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_SEPARATOR, MF_STRING,
        SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    };

    let open_label: Vec<u16> = "Ouvrir ZyrDesk".encode_utf16().chain(Some(0)).collect();
    let quit_label: Vec<u16> = "Quitter".encode_utf16().chain(Some(0)).collect();
    let mut cursor = POINT { x: 0, y: 0 };
    // SAFETY: a menu made here and unmade here, and the position of the
    // pointer read into a block of ours. The foreground is given to this
    // window before the menu drops down, otherwise the menu would stay
    // open after the next click: that is what the system asks.
    let chosen = unsafe {
        GetCursorPos(&mut cursor);
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, OPEN, open_label.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, QUIT, quit_label.as_ptr());
        SetForegroundWindow(window);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            0,
            window,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        chosen
    };
    match chosen as usize {
        OPEN => open(),
        QUIT => quit(),
        _ => {}
    }
}

fn open() {
    crate::main_window::show();
}

/// Stops everything and leaves.
///
/// The service goes first and on purpose: it is what holds the tunnel,
/// the engine and the announcement, and leaving it behind would be the
/// very thing this icon exists to make impossible. It is asked rather
/// than stopped through Windows, which would want administrator rights
/// every single time.
fn quit() {
    note("fermeture demandée depuis la zone de notification");
    crate::app::spawn(async move {
        match crate::desk::stop_service().await {
            Ok(()) => note("service arrêté, fermeture"),
            Err(reason) => note(&format!("service non arrêté : {reason}")),
        }
        remove_the_icon();
        crate::app::quit();
    });
}

/// Takes the icon down before leaving.
///
/// Without this it stays in the bar, a ghost, until someone moves the
/// mouse over it: only then does the system notice that the program is
/// gone.
#[cfg(windows)]
fn remove_the_icon() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Shell::{NIM_DELETE, Shell_NotifyIconW};

    let hwnd = ITS_WINDOW.swap(0, Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return;
    }
    let data = icon_data(hwnd);
    // SAFETY: a block of ours, naming the icon put up at start-up.
    unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

#[cfg(not(windows))]
fn remove_the_icon() {}
