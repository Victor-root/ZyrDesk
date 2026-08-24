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

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// What the menu entries are called on the way back.
const OPEN: &str = "open";
const QUIT: &str = "quit";

/// The product's own icon, compiled in, at the sizes this bar draws at.
///
/// Drawn from the same file the window and the installer use: one drawing
/// for the whole product, and no second one to keep in step.
///
/// One image per size, and never one reduced from another. The tray is
/// handed a single picture and Windows scales it to whatever the bar is
/// drawing at, so handing it the largest meant handing it a two hundred
/// and fifty-six pixel drawing to be squeezed into twenty-eight. That is
/// the difference between an icon that looks drawn and one that looks
/// blurred, and it is the same mistake the .ico file was making.
const DRAWINGS: &[(u32, &[u8])] = &[
    (
        16,
        include_bytes!("../../../packaging/brand/zyrdesk-16.png"),
    ),
    (
        20,
        include_bytes!("../../../packaging/brand/zyrdesk-20.png"),
    ),
    (
        24,
        include_bytes!("../../../packaging/brand/zyrdesk-24.png"),
    ),
    (
        28,
        include_bytes!("../../../packaging/brand/zyrdesk-28.png"),
    ),
    (
        32,
        include_bytes!("../../../packaging/brand/zyrdesk-32.png"),
    ),
    (
        40,
        include_bytes!("../../../packaging/brand/zyrdesk-40.png"),
    ),
];

/// How much of the icon is left when this computer cannot be reached.
///
/// Dim rather than another drawing: it stays recognisable at sixteen
/// pixels, where a second symbol would only be a smudge.
const DIMMED: u16 = 90;

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

/// Puts the icon up, for as long as the program runs.
pub fn raise(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Ouvrir ZyrDesk", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;

    TrayIconBuilder::with_id(NAME)
        .icon(drawn(false)?)
        .tooltip("ZyrDesk")
        .menu(&menu)
        // Left click opens the window, which is what everyone expects of
        // an icon down there; the menu stays on the right button.
        .show_menu_on_left_click(false)
        .on_menu_event(chosen)
        .on_tray_icon_event(clicked)
        .build(app)?;
    Ok(())
}

/// Name the icon answers to, so it can be found again to be changed.
const NAME: &str = "zyrdesk";

/// How often the icon asks what this computer is doing.
///
/// From here rather than from the page: a window that is hidden has its
/// timers slowed to a crawl by the system, and the icon has to keep
/// telling the truth precisely when the window is nowhere to be seen.
const LOOK: std::time::Duration = std::time::Duration::from_secs(3);

/// Keeps the icon saying the truth, for as long as the program runs.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
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
fn says(app: &AppHandle, reachable: bool, playing: bool) {
    let shown = app.state::<Shown>();
    let mut last = shown.0.lock().expect("état de l'icône");
    if *last == Some((reachable, playing)) {
        return;
    }
    let Some(icon) = app.tray_by_id(NAME) else {
        return;
    };
    if let Ok(drawing) = drawn(!reachable) {
        let _ = icon.set_icon(Some(drawing));
    }
    let _ = icon.set_tooltip(Some(match (playing, reachable) {
        (true, _) => "ZyrDesk : une session est en cours, cliquez pour revenir à la fenêtre",
        (false, true) => "ZyrDesk : cet ordinateur peut être contrôlé",
        (false, false) => "ZyrDesk : cet ordinateur n'est pas joignable",
    }));
    *last = Some((reachable, playing));
}

/// The side, in real pixels, this bar draws an icon at.
///
/// Sixteen logical pixels, multiplied by the scaling of the screen it is
/// on: sixteen at a hundred per cent, twenty-eight at a hundred and
/// seventy-five, and so on. Asked of the system rather than worked out,
/// since it is the system that decides.
#[cfg(windows)]
fn asked_for() -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

    // SAFETY: no argument beyond the metric asked for.
    u32::try_from(unsafe { GetSystemMetrics(SM_CXSMICON) }).unwrap_or(16)
}

#[cfg(not(windows))]
fn asked_for() -> u32 {
    16
}

/// The icon, bright or dimmed.
fn drawn(dim: bool) -> tauri::Result<Image<'static>> {
    // The smallest that is not too small. A drawing enlarged by the
    // system is soft, one reduced by it is soft as well, but the second
    // keeps everything it was drawn with while the first invents.
    let wanted = asked_for();
    let (_, bytes) = DRAWINGS
        .iter()
        .find(|(side, _)| *side >= wanted)
        .unwrap_or_else(|| DRAWINGS.last().expect("un dessin au moins"));
    let drawing = Image::from_bytes(bytes)?;
    if !dim {
        return Ok(drawing.to_owned());
    }
    let (width, height) = (drawing.width(), drawing.height());
    let mut faded = drawing.rgba().to_vec();
    // Only what makes a pixel visible is touched: dimming the colours
    // instead would turn the drawing grey on a dark background and black
    // on a light one.
    // Four bytes to a pixel, said as a size and not as a number: taken as
    // a number the slices come back one at a time and nothing promises
    // they are four long, and every reading of one has to answer for a
    // length that cannot happen.
    for pixel in faded.as_chunks_mut::<4>().0 {
        pixel[3] = (u16::from(pixel[3]) * DIMMED / 255) as u8;
    }
    Ok(Image::new_owned(faded, width, height))
}

fn clicked(icon: &TrayIcon, event: TrayIconEvent) {
    let TrayIconEvent::Click { button, .. } = event else {
        return;
    };
    if button == tauri::tray::MouseButton::Left {
        crate::show_home(icon.app_handle());
    }
}

fn chosen(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        OPEN => crate::show_home(app),
        QUIT => quit(app),
        _ => {}
    }
}

/// Stops everything and leaves.
///
/// The service goes first and on purpose: it is what holds the tunnel,
/// the engine and the announcement, and leaving it behind would be the
/// very thing this icon exists to make impossible. It is asked rather
/// than stopped through Windows, which would want administrator rights
/// every single time.
fn quit(app: &AppHandle) {
    crate::journal::note("fermeture demandée depuis la zone de notification");
    let leaving = app.clone();
    tauri::async_runtime::spawn(async move {
        match crate::desk::stop_service().await {
            Ok(()) => crate::journal::note("service arrêté, fermeture"),
            Err(reason) => crate::journal::note(&format!("service non arrêté : {reason}")),
        }
        leaving.exit(0);
    });
}
