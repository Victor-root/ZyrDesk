//! The ZyrDesk application: the only window the user ever sees.
//!
//! It holds almost nothing. The service owns the identity, the tunnels
//! and the far computer's side of everything; this program asks it
//! questions and shows the answers.
//!
//! And it draws them itself. There is no web view left anywhere in this
//! product: the window the toolkit opens is a bare one, and its inside is
//! a canvas this program paints, like the floating button and its menu.
//!
//! The one thing it does hold is the player of a session, which it
//! starts and which goes when it goes. That is deliberate: a player left
//! running behind a window that is no longer there would hold the far
//! computer's desktop with nothing on screen to give it back.
//!
//! The video is never drawn here either: the player draws it in a window
//! of its own, so nothing about this interface is on the path of a
//! frame. That window is laid over the inside of this one and made to
//! follow it, which is what puts one window on screen instead of two
//! without a picture ever passing through a web view.

// A second console window opening behind the interface would give the
// game away immediately.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod design;
mod desk;
mod floating;
mod folders;
mod icon;
mod journal;

// Le logo du bouton flottant, dessiné par ce programme : il n'existe
// que sous Windows, comme la fenêtre qui le porte.
#[cfg(windows)]
mod logo;

// Ce qui dessine l'interface sans navigateur. Windows seulement, comme
// les fenêtres qu'elle habille.
#[cfg(windows)]
mod paint;

// Les icônes que ce programme dessine, partagées par tous ses écrans.
#[cfg(windows)]
mod icones;

// Le menu du bouton flottant, dessiné par ce programme.
#[cfg(windows)]
mod menu;

// La fenêtre d'accueil, dessinée par ce programme.
#[cfg(windows)]
mod accueil;

/// Hors de Windows il n'y a pas de fenêtre d'accueil, et pas de session
/// non plus : ce qu'une session raconte pendant qu'elle s'ouvre tombe
/// alors dans le vide, comme tout le reste de ce qui dessine.
#[cfg(not(windows))]
mod accueil {
    use tauri::AppHandle;

    pub fn etape(_app: &AppHandle, _detail: &str, _code: Option<String>) {}
    pub fn relance(_app: &AppHandle) {}
    pub fn range_l_ouverture(_app: &AppHandle) {}
    pub fn echoue(_app: &AppHandle, _texte: &str) {}
}

mod mesures;
mod picture;
mod service;
mod session;
mod settings;
mod shortcuts;
mod startup;
mod theme;
mod tray;

#[cfg(windows)]
mod elevated;

// Being told where the front is going is a thing only Windows does, and
// the watch that hears it lives on a thread of its own.
#[cfg(windows)]
mod hook;

use tauri::{Manager, WindowEvent};

/// The home window, as the configuration names it.
const HOME: &str = "main";

fn main() {
    let building = tauri::Builder::default();

    // Two ZyrDesk running at once would put two floating buttons on the
    // same session. Whoever starts the second one wanted the window
    // back, which is what they get.
    #[cfg(windows)]
    let building = building.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        show_home(app);
    }));

    building
        .manage(floating::Floating::default())
        .manage(picture::Picture::default())
        .manage(tray::Shown::default())
        .setup(|app| {
            journal::opened();
            // Ce que la personne a choisi de regarder, relu avant que la
            // fenêtre s'ouvre : une fenêtre qui s'ouvrirait dans le
            // mauvais thème, même le temps d'un battement, se verrait.
            theme::what_was_chosen();
            open_home(app.handle())?;
            // Ce que Windows veut, suivi tant que le programme tourne.
            theme::watch(app.handle().clone());
            // The window's own icon, which the toolkit has already put a
            // stretched one of: taken from the compiled resource at the
            // two sizes Windows is about to draw it at.
            icon::on_the_window(app.handle());
            // The icon beside the clock: from here on, something on
            // screen says this program is running, whatever becomes of
            // the window.
            if let Err(e) = tray::raise(app.handle()) {
                journal::note(&format!("pas d'icône dans la zone de notification : {e}"));
            }
            // Nothing of this product runs while nobody is using it, so
            // opening it is what puts the service back on its feet.
            service::wake_the_service();
            tray::watch(app.handle().clone());
            // Where a hand last left the floating button, read before
            // any session can ask for it.
            floating::where_it_was_left();
            floating::watch(app.handle().clone());
            // A session gives the keyboard to the far computer, so what
            // is left to us has to be asked of the system rather than
            // waited for as an ordinary key press.
            shortcuts::listen(app.handle().clone());
            // Et l'accueil lui-même, dessiné dans le dedans de cette
            // fenêtre. En dernier : il demande au service ce qu'il
            // montre, et le service vient d'être réveillé.
            #[cfg(windows)]
            accueil::raise(app.handle());
            show_home(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != HOME {
                return;
            }
            match event {
                // The cross means two different things, and which one it
                // is depends on what the window is showing.
                //
                // Showing a session, it ends the session and stays: the
                // picture is inside this window, so a cross that only
                // hid the window would leave the far computer held by
                // something with nothing left on screen to give it back.
                //
                // Showing the home screen, it puts the window away
                // without stopping anything. This computer can be
                // reachable while nobody is looking at a window, the
                // icon beside the clock says so, and « Quitter » there
                // is the one thing that stops the product.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    if floating::a_session_is_up(window.app_handle()) || session::opening() {
                        // While a session is merely opening there may be
                        // nothing to end yet; the ask then only reaches
                        // the journal, and the window stays. Hiding it
                        // instead let the opening run on unseen, and the
                        // session arrived as a bare rectangle on the
                        // desktop with no window to live in.
                        session::end_it(window.app_handle());
                    } else {
                        let _ = window.hide();
                    }
                }
                // Laying the picture where the window went is not done
                // from here: it is done inside the window's own message
                // handler, which runs a queue earlier than this and is
                // the difference between a picture glued to the frame
                // and one visibly trailing it.
                //
                // What is left here is putting the window back on the
                // picture's shape, for the resizes that are not a hand
                // dragging an edge: a hand is held to shape while it
                // drags, before the resize happens.
                WindowEvent::Resized(_) => {
                    picture::hold_the_shape(window.app_handle());
                }
                // The window has changed screen, or the screen has
                // changed magnification: the icon and everything drawn
                // inside are counted in real pixels, so both are asked
                // for again at the new ones.
                WindowEvent::ScaleFactorChanged { .. } => {
                    icon::on_the_window(window.app_handle());
                    #[cfg(windows)]
                    accueil::mesure_l_ecran(window.app_handle());
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("l'interface ZyrDesk n'a pas pu démarrer");
}

/// Opens the one window, bare.
///
/// Bare because nothing of this product is a web page any more: the
/// toolkit gives us a window, a frame and an event loop, and what is
/// inside it is drawn by this program. Built here rather than declared in
/// the configuration, which only ever makes windows with a browser in
/// them.
fn open_home(app: &tauri::AppHandle) -> tauri::Result<()> {
    tauri::window::WindowBuilder::new(app, HOME)
        .title("ZyrDesk")
        .inner_size(1060.0, 720.0)
        .min_inner_size(880.0, 600.0)
        .resizable(true)
        .center()
        // Cachée le temps qu'on la remplisse : une fenêtre montrée avant
        // d'avoir été peinte se voit vide, et c'est la première image du
        // produit.
        .visible(false)
        .build()?;
    theme::on_the_window(app);
    Ok(())
}

/// Brings the home window back, wherever it was left.
///
/// The picture and the floating button come back with it without being
/// told: both are windows the system knows this one owns, and it puts
/// them back up when it puts this one back up.
pub fn show_home(app: &tauri::AppHandle) {
    let Some(window) = app.get_window(HOME) else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}
