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
//! **Elle est dessinée**, comme tout le reste du produit, à la taille
//! exacte que la barre demande. Il n'y a donc aucune image à réduire ni à
//! agrandir : c'était le seul moyen d'avoir une icône nette à seize
//! pixels comme à vingt-huit, et c'est maintenant la même marque que
//! celle du bouton flottant et de l'accueil, tracée par le même dessin.

// Une zone de notification est une chose du système, et ce produit ne
// tourne que sous Windows. Ailleurs, il n'y a pas d'icône à poser.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::app::App;

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "tray";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Ce que le menu répond quand on choisit une de ses lignes.
const OPEN: usize = 1;
const QUIT: usize = 2;

/// Ce qu'il reste de la marque quand cet ordinateur n'est pas joignable.
///
/// Pâlie plutôt qu'un autre dessin : elle reste reconnaissable à seize
/// pixels, là où un second symbole ne serait qu'une tache.
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

/// La fenêtre qui reçoit ce que l'icône a à dire, et l'icône elle-même
/// telle que le système la garde.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static ITS_ICON: AtomicIsize = AtomicIsize::new(0);

/// Le numéro sous lequel cette icône est déposée, et le message par
/// lequel elle parle.
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
    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages. Elle ne montre rien : c'est ce
    // que le système demande pour porter une icône.
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
    // SAFETY: un bloc à nous, dont la taille est écrite dedans comme
    // l'appel le demande.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
        return Err("Windows n'a pas pris l'icône de la zone de notification".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn raise() -> Result<(), String> {
    Err("il n'y a pas de zone de notification hors de Windows".to_string())
}

/// Le bloc que le système attend, rempli de ce qui ne change jamais.
#[cfg(windows)]
fn icon_data(
    hwnd: windows_sys::Win32::Foundation::HWND,
) -> windows_sys::Win32::UI::Shell::NOTIFYICONDATAW {
    use windows_sys::Win32::UI::Shell::NOTIFYICONDATAW;

    // SAFETY: un bloc à nous, rempli de zéros puis des seuls champs que
    // les drapeaux annoncent.
    let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = ICON_ID;
    data
}

/// Écrit un mot dans un des champs de longueur fixe du système.
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
    // SAFETY: un bloc à nous, et l'ancien dessin rendu une fois que le
    // système ne s'en sert plus.
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

/// La marque, tracée à la taille que la barre demande.
///
/// Rien n'est réduit ni agrandi : c'est le dessin lui-même qui est fait à
/// cette taille-là, ce qui est la seule façon d'avoir un bord net à seize
/// pixels.
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

/// SAFETY: appelée par le système sur le fil qui a fait cette fenêtre,
/// avec les arguments qu'il documente.
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
            // Le clic gauche ouvre la fenêtre, ce que tout le monde
            // attend d'une icône là-dessous ; le menu reste sur le bouton
            // droit.
            WM_LBUTTONUP => open(),
            WM_RBUTTONUP => pop_up_the_menu(window),
            _ => {}
        }
        return 0;
    }
    // SAFETY: la réponse du système à tout ce qui n'est pas répondu ici.
    unsafe { DefWindowProcW(window, message, holding, with) }
}

/// Ce que le menu de l'icône propose, et ce qu'il fait de la réponse.
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
    // SAFETY: un menu fait ici et défait ici, et la place du curseur lue
    // dans un bloc à nous. Le premier plan est donné à cette fenêtre
    // avant de dérouler le menu, faute de quoi le menu resterait ouvert
    // après le clic suivant : c'est ce que le système demande.
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

/// Retire l'icône avant de partir.
///
/// Sans ça elle reste dans la barre, fantôme, jusqu'à ce que quelqu'un
/// passe la souris dessus : le système ne s'aperçoit qu'à ce moment-là
/// que le programme n'est plus là.
#[cfg(windows)]
fn remove_the_icon() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Shell::{NIM_DELETE, Shell_NotifyIconW};

    let hwnd = ITS_WINDOW.swap(0, Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return;
    }
    let data = icon_data(hwnd);
    // SAFETY: un bloc à nous, nommant l'icône déposée au démarrage.
    unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

#[cfg(not(windows))]
fn remove_the_icon() {}
