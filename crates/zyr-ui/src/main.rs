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

mod app;
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

// Le battement de ce qui bouge, réglé sur le compositeur de Windows.
#[cfg(windows)]
mod rythme;

// Le menu du bouton flottant, dessiné par ce programme.
#[cfg(windows)]
mod menu;

// La fenêtre d'accueil, dessinée par ce programme.
#[cfg(windows)]
mod accueil;

// La fenêtre elle-même, ouverte par ce programme.
mod fenetre;

/// Hors de Windows il n'y a pas de fenêtre d'accueil, et pas de session
/// non plus : ce qu'une session raconte pendant qu'elle s'ouvre tombe
/// alors dans le vide, comme tout le reste de ce qui dessine.
#[cfg(not(windows))]
mod accueil {
    use crate::app::App;

    pub fn etape(_app: &App, _detail: &str, _code: Option<String>) {}
    pub fn relance(_app: &App) {}
    pub fn range_l_ouverture(_app: &App) {}
    pub fn echoue(_app: &App, _texte: &str) {}
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

fn main() {
    // Two ZyrDesk running at once would put two floating buttons on the
    // same session. Whoever starts the second one wanted the window
    // back, which is what they get.
    if app::deja_ouvert() {
        return;
    }
    // Ce que ce programme dessine se compte en vrais pixels, sur chaque
    // écran : dit avant qu'une seule fenêtre existe, faute de quoi le
    // système agrandirait lui-même ce qui est déjà à la bonne taille.
    app::compte_en_vrais_pixels();
    journal::opened();

    let app = app::App::neuf();
    if let Err(e) = app::ouvre_le_courrier() {
        journal::note(&format!("ZyrDesk ne démarre pas : {e}"));
        return;
    }
    // Ce que la personne a choisi de regarder, relu avant que la fenêtre
    // s'ouvre : une fenêtre qui s'ouvrirait dans le mauvais thème, même
    // le temps d'un battement, se verrait.
    theme::what_was_chosen();
    if let Err(e) = fenetre::ouvre(&app) {
        journal::note(&format!("ZyrDesk ne démarre pas : {e}"));
        return;
    }
    theme::on_the_window();
    // Ce que Windows veut, suivi tant que le programme tourne.
    theme::watch(app.clone());
    // The window's own icon: taken from the compiled resource at the two
    // sizes Windows is about to draw it at.
    icon::on_the_window();
    // The icon beside the clock: from here on, something on screen says
    // this program is running, whatever becomes of the window.
    if let Err(e) = tray::raise() {
        journal::note(&format!("pas d'icône dans la zone de notification : {e}"));
    }
    // Nothing of this product runs while nobody is using it, so opening
    // it is what puts the service back on its feet.
    service::wake_the_service();
    tray::watch(app.clone());
    // Where a hand last left the floating button, read before any session
    // can ask for it.
    floating::where_it_was_left();
    floating::watch(app.clone());
    // A session gives the keyboard to the far computer, so what is left
    // to us has to be asked of the system rather than waited for as an
    // ordinary key press.
    shortcuts::listen(app.clone());
    // Et l'accueil lui-même, dessiné dans le dedans de cette fenêtre. En
    // dernier : il demande au service ce qu'il montre, et le service
    // vient d'être réveillé.
    #[cfg(windows)]
    accueil::raise(&app);
    fenetre::montre();

    app::tourne();
}

/// Brings the home window back, wherever it was left.
///
/// The picture and the floating button come back with it without being
/// told: both are windows the system knows this one owns, and it puts
/// them back up when it puts this one back up.
pub fn show_home(_app: &crate::app::App) {
    fenetre::montre();
}
