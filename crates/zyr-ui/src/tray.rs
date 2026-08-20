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

/// The product's own icon, compiled in.
///
/// Read from the same file the window and the installer use: one drawing
/// for the whole product, and no second one to keep in step.
const DRAWING: &[u8] = include_bytes!("../../../packaging/brand/zyrdesk-256.png");

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

/// The icon, bright or dimmed.
fn drawn(dim: bool) -> tauri::Result<Image<'static>> {
    let drawing = Image::from_bytes(DRAWING)?;
    if !dim {
        return Ok(drawing.to_owned());
    }
    let (width, height) = (drawing.width(), drawing.height());
    let mut faded = drawing.rgba().to_vec();
    // Only what makes a pixel visible is touched: dimming the colours
    // instead would turn the drawing grey on a dark background and black
    // on a light one.
    for pixel in faded.chunks_exact_mut(4) {
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
