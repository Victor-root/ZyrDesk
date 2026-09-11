//! The floating button of a session.
//!
//! During a session the picture fills the window and belongs to the
//! engine. This is the one thing of ours left on top of it: a small
//! button, hanging in a corner, that opens what can be done without
//! leaving the picture.
//!
//! Il est fait de deux fenêtres à nous, le logo et la carte du menu, que
//! ce programme dessine lui-même : ni l'une ni l'autre ne porte de page,
//! et il n'y a plus de navigateur nulle part sur l'image. Ce fichier ne
//! dessine rien ; il tient où le bouton pend, ce qu'il fait, et quand il
//! monte et descend.
//!
//! Des fenêtres à nous plutôt qu'un dessin fait dans l'image. Dessiner
//! dedans reviendrait à apprendre au moteur ce qu'est ZyrDesk, ce que les
//! moteurs sont précisément tenus d'ignorer ; et une fenêtre à nous se
//! laisse cliquer sans que le moteur ait à rendre la souris.
//!
//! Two things make that work, and both are why no session ever takes the
//! screen exclusively. A window that owns the screen lets nothing be
//! drawn above it. And the pointer, in the ordinary desktop mode, stays
//! free to leave the picture: it is hidden over the picture, where the
//! far computer's own cursor stands in for it, and the system shows it
//! again the moment it crosses onto this button.
//!
//! What the menu asks of the engine, it asks through the engine's own
//! keyboard shortcuts, aimed at the session window and at nothing else.
//! Two entries never reach it: covering the screen is done to our own
//! window, and ending the session is asked of the far computer over the
//! tunnel, since what ends it there is that computer letting its desktop
//! go.

// Off Windows there is no session to float over, and the shortcut the
// letters belong to is never typed. The rest stays compiled and tested
// everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering};
use std::time::Duration;

use crate::app::App;

// What the button did goes into the same journal as everything else: it
// has nowhere else to say it, standing behind the picture, and a menu
// entry that seems to do nothing is exactly the kind of thing that cannot
// be diagnosed from a screenshot.
use crate::journal::note;

/// How often the session is looked for.
///
/// Short enough that the button is there by the time the picture is, and
/// gone shortly after it.
const LOOK: Duration = Duration::from_secs(1);

/// Distance kept from the corner of the picture, in page pixels.
///
/// Everything else in this file is real pixels: what this margin comes
/// to on a given screen is asked of `margin()`, which scales it the way
/// the button itself is scaled. Unscaled, the gap shrank visibly on a
/// magnified screen while the button grew.
const MARGIN: i32 = 16;

/// That distance in real pixels, on the screen the button hangs over.
#[cfg(windows)]
fn margin() -> i32 {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let button = crate::logo::its_window() as HWND;
    if button.is_null() {
        return MARGIN;
    }
    // SAFETY: a window of ours, and only its scale is read.
    let dpi = unsafe { GetDpiForWindow(button) };
    MARGIN * dpi.max(96) as i32 / 96
}

#[cfg(not(windows))]
fn margin() -> i32 {
    MARGIN
}

/// Size of the button alone, in page pixels, as the page draws it.
///
/// The same number as the logo's in `bouton.css`, and it has to stay the
/// same number: everything about where the button hangs is worked out
/// from it, and nothing ever corrects it afterwards. Left behind once
/// when the logo was made smaller, it hung the button ten real pixels off
/// its corner for the whole of every session.
const BUTTON: f64 = 44.0;

/// How far the mouse has to travel, while holding the button, before it
/// is a drag and no longer a click.
///
/// Without it a hand that shakes would move the button every time
/// somebody wanted to open the menu, and the other way round.
const GRIP: i32 = 4;

/// How often the button catches up with the mouse while being dragged.
const FOLLOW: Duration = Duration::from_millis(8);

/// Pause between two looks at whether the player has stopped.
const STOP_STEP: Duration = Duration::from_millis(10);

/// How long the player is given to stop by itself once the far computer
/// has been asked to hand its desktop back.
///
/// Past it the player is stopped here. Long enough that a far computer
/// which answers takes the picture away itself, which is how a session
/// ends when everything works; short enough that one which has stopped
/// answering does not hold the person in front of a picture that is over.
const CLOSING_SHOWS: Duration = Duration::from_secs(3);

/// How long a drag may last before it is called over.
///
/// A mouse unplugged mid-drag, or a button released where nothing
/// noticed, would otherwise leave the button following a cursor nobody
/// is holding for as long as the program runs.
const AT_MOST: Duration = Duration::from_secs(60);

/// What the menu can ask of the session.
///
/// Ending a session is one entry and not two. The engines offer both a
/// leaving that keeps the far desktop open and waiting and a closing
/// that hands it back, and carrying that difference up to the person
/// would leave them with a session that is neither running nor over. A
/// session is on or it is not.
#[derive(Clone, Copy)]
pub enum Act {
    Fullscreen,
    Stats,
    MouseMode,
    /// Ctrl+Alt+Suppr, pressed on the far computer.
    SecureAttention,
    /// The far computer's lock screen, put up.
    LockScreen,
    /// The session's own sound, hushed or given back on this computer.
    Sound,
    /// Which of the two computers Alt+Tab, Échap and the Windows key
    /// belong to.
    SystemKeys,
    /// Which of the two the touchpad's own gestures belong to.
    Touchpad,
    /// Whether the two computers share one clipboard.
    Clipboard,
    /// Whether the two badges in the corner of the picture are drawn at
    /// all times rather than only when they have something to say.
    Voyants,
    /// Whether the pointer is kept inside the picture.
    PointerLock,
    End,
}

impl Act {
    /// Letter of the engine's Ctrl+Alt+Shift shortcut, for the ones that
    /// have one.
    ///
    /// Five do not. Ending a session is asked of the far computer over
    /// the tunnel, since what ends it there is that computer letting its
    /// desktop go; covering the screen is done to our own window, the
    /// engine's having gone inside it; the sound is hushed on this
    /// computer's own mixer, where the player has a strip like any other
    /// program; and the last two are the pair Windows keeps for itself at
    /// both ends of a session, Ctrl+Alt+Suppr and the lock screen, which
    /// travel on the product's own channel and are done over there by the
    /// service, the one program on that machine allowed to.
    fn letter(self) -> Option<u8> {
        match self {
            Act::Stats => Some(b'S'),
            Act::MouseMode => Some(b'M'),
            Act::SystemKeys => Some(b'K'),
            Act::PointerLock => Some(b'L'),
            Act::Fullscreen
            | Act::SecureAttention
            | Act::LockScreen
            | Act::Sound
            | Act::Touchpad
            | Act::Clipboard
            | Act::Voyants
            | Act::End => None,
        }
    }

    /// Where that letter sits on the keyboard.
    ///
    /// The engine is built on a library that reads a key by its place
    /// before it reads it by its name, and the two come apart on the
    /// keyboards this product is used on: the key engraved A in France
    /// is the key engraved Q elsewhere. A key sent by name leaves the
    /// place to be worked out by whatever the system happens to think
    /// the keyboard is, which is one guess too many for a keystroke
    /// nobody typed.
    fn where_it_sits(self) -> Option<u16> {
        match self {
            Act::Stats => Some(0x1F),
            Act::MouseMode => Some(0x32),
            Act::SystemKeys => Some(0x25),
            Act::PointerLock => Some(0x26),
            Act::Fullscreen
            | Act::SecureAttention
            | Act::LockScreen
            | Act::Sound
            | Act::Touchpad
            | Act::Clipboard
            | Act::Voyants
            | Act::End => None,
        }
    }
}

impl std::fmt::Display for Act {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Act::Fullscreen => "plein écran",
            Act::Stats => "statistiques",
            Act::MouseMode => "mode de la souris",
            Act::SecureAttention => "Ctrl+Alt+Suppr",
            Act::LockScreen => "verrouillage de l'ordinateur distant",
            Act::Sound => "son de la session",
            Act::SystemKeys => "touches système",
            Act::Touchpad => "gestes du pavé tactile",
            Act::Clipboard => "presse-papiers partagé",
            Act::Voyants => "voyants montrés en permanence",
            Act::PointerLock => "pointeur tenu dans l'image",
            Act::End => "fin de la session",
        })
    }
}

/// The session the button belongs to.
#[derive(Default)]
pub struct Floating {
    /// Player the button hangs on, and the only window our keystrokes
    /// may reach.
    watched: Mutex<Option<u32>>,
    /// Set while this window is asking the far computer to close the
    /// session.
    ///
    /// The engine loses its stream when that happens and stops on a
    /// failure, which from the outside is exactly what a session that
    /// broke looks like. Without this, the one thing the person asked for
    /// would be reported back to them as an error.
    closing: std::sync::atomic::AtomicBool,
    /// Player this window has just started and the service does not know
    /// about yet.
    ///
    /// A session is only handed to the service once it has been watched
    /// long enough to be believed, and the button would arrive that many
    /// seconds after the picture. Whoever started the engine knows its
    /// number straight away, and the button hangs on nothing else.
    ///
    /// The way to the far computer travels with it, for the same reason:
    /// during those seconds this window is the only thing that can end
    /// the session, and ending is asked at that address. Without it, the
    /// cross and the menu both answered « aucune session en cours » over
    /// a running picture until the service caught up.
    expected: Mutex<Option<Expected>>,
    /// Whether the service named this player among its sessions, the
    /// last time it was asked.
    ///
    /// Compared against itself from one watch to the next: a way not yet
    /// registered, in the first seconds of a session, and one the service
    /// has stopped naming after really holding it, look exactly alike
    /// from a single answer alone. Only the second is a session ended
    /// from outside this window, kicked or otherwise, and it is what
    /// tells the two apart.
    confirmed: AtomicBool,
    /// Whether the session's mouse is in game mode right now.
    ///
    /// Kept by this program because this program is what sets it: the
    /// mode starts from the settings the session was opened with, and
    /// every toggle goes through this window. It cannot be read from the
    /// system. Game mode pins the pointer inside the picture, but a
    /// session covering the only screen pins it to a rectangle exactly
    /// the size of the screen, which is what no pinning at all looks
    /// like; read from there, the menu shortcut left the pointer with
    /// the far computer and the menu it had just opened could not be
    /// clicked.
    game_mouse: AtomicBool,
    /// Whether the engine is keeping the pointer inside the picture.
    ///
    /// The engine decides this from its own window being on a whole
    /// screen, which it can never be here: it is a small windowed one
    /// carried inside ours for the whole session. Asked there, the
    /// pointer was free to wander off the picture all session long, and
    /// on a machine with a second screen it simply left. It is this
    /// program that knows, so it is this program that says, by throwing
    /// the engine's own switch for it.
    ///
    /// Counted here rather than read, like the two beside it: the engine
    /// never says where it stands. It starts off, which is what the
    /// engine leaves it at for a window like ours.
    pointer_held: AtomicBool,
    /// Whether Alt+Tab, Échap and the Windows key are going to the
    /// session right now rather than to this computer.
    ///
    /// Kept here for the same reason as the mouse mode beside it: the
    /// engine is the one holding those keys and it never says where it
    /// stands, so this program counts its own switches. The session
    /// starts on the side its settings asked for.
    system_keys: AtomicBool,
    /// Whether the touchpad's own gestures are going to the session
    /// right now rather than to this computer.
    ///
    /// Kept here like the switch above it, and for a plainer reason: it
    /// is this program that reads the pad, so it is this program that
    /// knows. Nothing else on this computer, and no engine, is even aware
    /// there is a pad being read.
    touchpad: AtomicBool,
    /// Whether the two computers share one clipboard right now.
    ///
    /// Counted here like the two above it, and this one holds nothing at
    /// all beyond the switch: sharing is done by the two services, on the
    /// product's own channel inside the tunnel, and this window's whole
    /// part in it is to say which way the switch is and to write that
    /// down.
    clipboard: AtomicBool,
    /// Whether the two badges in the corner of the picture are drawn at
    /// all times.
    ///
    /// The one switch here that is not about what a session does. The
    /// badges show themselves when something is wrong and hide when
    /// nothing is, which is what they are for and also what makes them
    /// hard to look at: they are almost never there. This holds them on
    /// screen, lit or dim, so they can be watched working.
    ///
    /// Not remembered anywhere, and that is the point: a switch that only
    /// exists to look at something must not be left on by a session
    /// nobody was looking at.
    voyants: AtomicBool,
    /// Whether this computer is drawing its own pointer over the picture.
    ///
    /// Counted like the three above, and put down whenever a player is
    /// adopted: this is the engine's own switch, it lives in that
    /// player, and a player started again starts it where the engine
    /// leaves it, which is off.
    /// Whether the far computer has been asked to stop drawing its
    /// pointer into what it sends.
    ///
    /// The one switch of the set that does not belong to a player. It
    /// lives in the far computer's engine, which is started with that
    /// computer's service and outlives every session towards it, so it
    /// is never put down with a player: opening the picture again in the
    /// middle of a session hands us a new player and changes nothing
    /// over there, and a belief put down with the player would throw
    /// that switch a second time and give the pointer back under a
    /// session still watching a desktop.
    ///
    /// What puts it down is giving the pointer back, which really does
    /// put it back where it was found. That is why nothing puts it down
    /// when a session opens either: a session that closed properly left
    /// this false and the far computer drawing, and one that did not is
    /// better served by a belief that still matches what was left over
    /// there than by a fresh one that does not.
    ///
    /// It cannot be read back, and that is the whole of what is fragile
    /// here: a session ending without passing through the closing below,
    /// which is a machine that crashes or a network that goes, leaves
    /// that computer's engine drawing nothing until its service restarts
    /// it. What that costs is two pointers, or none, on a later session
    /// towards a different computer; it is seen at once and is one
    /// switch away, and nothing about it is silent.
    far_pointer_hidden: AtomicBool,
    /// Whether the last thing said about it was refused.
    ///
    /// So that a run of refusals is one line and not one a second, and
    /// so that the run ending is one line too: what has to be read here
    /// is when it started and when it stopped.
    far_pointer_refused: AtomicBool,
}

/// What this window knows of a session it started, before the service
/// believes it.
struct Expected {
    process: u32,
    /// The far computer, as the person named it.
    towards: String,
    /// And as it recognised itself, which is what its engine state is
    /// filed under: a name changes with the road taken to it.
    peer: Option<String>,
    /// Where the tunnel puts it on this machine.
    at: String,
}

static NUDGE: AtomicI64 = AtomicI64::new(0);

/// D'où la carte du menu s'ouvre, vue du bouton.
///
/// Le bouton se pose n'importe où dans l'image, et la carte est plus
/// haute que la moitié d'un écran : il n'y a pas un sens qui marche
/// toujours, il y en a trois, et c'est la place qui reste autour du
/// bouton qui décide lequel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sens {
    /// Sous le bouton, quand l'image a la place dessous.
    Bas,
    /// Au-dessus, quand elle ne l'a qu'au-dessus.
    Haut,
    /// À gauche du bouton, quand ni l'un ni l'autre.
    ///
    /// Un bouton posé à mi-hauteur ne laisse assez de place ni dessous ni
    /// dessus, et la carte y était coupée par le bas de l'image. À côté,
    /// elle a toute la hauteur de l'image pour elle : elle ne part plus
    /// du bouton, elle se pose à sa gauche et glisse de ce qu'il faut
    /// pour tenir en entier.
    Cote,
}

impl Sens {
    /// Le sens rangé dans un nombre, le seul type qu'un atomique porte.
    fn range(self) -> u8 {
        match self {
            Sens::Bas => 0,
            Sens::Haut => 1,
            Sens::Cote => 2,
        }
    }

    /// Et relu. Un nombre que personne d'autre n'écrit : tout ce qui
    /// n'est pas un sens connu est le sens par défaut.
    fn lu(range: u8) -> Self {
        match range {
            1 => Sens::Haut,
            2 => Sens::Cote,
            _ => Sens::Bas,
        }
    }
}

/// Le sens en place, décidé à chaque pose du bouton.
///
/// La fenêtre de la carte est aussi haute que la carte pendant toute la
/// session et pend au logo, qui occupe un de ses coins. Décidé ici et
/// non dans la carte : elle ne sait pas où on l'a posée sur un écran,
/// et le logo comme elle ont besoin de la réponse.
static SENS: AtomicU8 = AtomicU8::new(0);

/// Si la fenêtre s'ouvre collée par son bord gauche plutôt que par son
/// bord droit, décidé au même instant que `SENS` et pour la même
/// raison : un bouton posé près du bord gauche de l'image ne laisse pas
/// à la carte la place de partir de son bord droit comme elle le fait
/// d'habitude.
static A_DROITE: AtomicBool = AtomicBool::new(false);

/// The logo alone, in real pixels, which is not the size of the window
/// holding it.
///
/// The window is as large as the menu from the moment it opens and stays
/// that size for the whole session, so that clicking the button never
/// resizes it: a window that changes size makes the page inside it lay
/// itself out again, and for the frame that takes, the logo is not drawn
/// anywhere. That was the flash. Everything the window shows is cut out
/// of it, so the part of it nobody is using shows nothing and catches no
/// click.
///
/// But the button is the logo, not the window it is carried in: where it
/// hangs, how far it may be dragged, and how small the picture may be
/// taken down to are all about the logo. Hence this, beside the window.
/// The logo sits in the window's top right corner, so the two share that
/// corner and nothing else.
static ITS_LOGO: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Set while the person has hidden the button from its own menu.
///
/// A choice they made stands until they ask for the button back. ZyrDesk
/// being minimised and restored is not that ask, and the system would
/// otherwise put the button back up with the window.
static HIDDEN: AtomicBool = AtomicBool::new(false);

/// The logo's own size, as a square.
fn logo() -> (i32, i32) {
    let side = ITS_LOGO.load(Ordering::Relaxed).max(1);
    (side, side)
}

#[cfg(not(windows))]
fn how_it_shows() -> u32 {
    0
}

/// D'où une carte de cette hauteur s'ouvre, pour un bouton pendu là.
///
/// Dessous tant que ça tient, dessus sinon, et à côté quand ni l'un ni
/// l'autre : c'est l'ordre dans lequel une carte se lit le plus
/// naturellement depuis le bouton qui l'ouvre.
///
/// Une image trop courte pour la carte quel que soit le sens la garde
/// du côté où il reste le plus de place : la mettre à côté ne
/// l'empêcherait pas d'être coupée et lui coûterait en plus de ne plus
/// partir du bouton.
fn ou_s_ouvre(picture: (i32, i32, i32, i32), anchor: (i32, i32), height: i32) -> Sens {
    let below = picture.3 - anchor.1;
    if height <= below {
        return Sens::Bas;
    }
    let above = anchor.1 + logo().1 - picture.1;
    if height <= above {
        return Sens::Haut;
    }
    if height <= picture.3 - picture.1 {
        return Sens::Cote;
    }
    if above > below { Sens::Haut } else { Sens::Bas }
}

/// D'où une fenêtre de cette largeur a la place de partir, pour un
/// bouton pendu là : collée à son bord droit comme d'habitude, tant que
/// ça tient à sa gauche ; à son bord gauche sinon, tant que ça tient à
/// sa droite.
///
/// Une image trop étroite pour elle des deux côtés la garde du côté où
/// il reste le plus de place, pour la même raison que `ou_s_ouvre`.
fn ou_s_ouvre_a_droite(picture: (i32, i32, i32, i32), anchor: (i32, i32), width: i32) -> bool {
    let a_gauche = anchor.0 - picture.0;
    if width <= a_gauche {
        return false;
    }
    let a_droite = picture.2 - (anchor.0 - logo().0);
    if width <= a_droite {
        return true;
    }
    a_droite > a_gauche
}

/// Ce que la fenêtre du menu prend de large en tout pour ce sens
/// vertical-là, qui décide de quel côté elle a la place de s'ouvrir.
///
/// À côté, elle compte aussi le bouton et l'espace qui l'en sépare :
/// c'est sa fenêtre entière qui se pose à côté de lui, jamais sa seule
/// carte.
#[cfg(windows)]
fn menu_width(sens: Sens) -> i32 {
    crate::menu::large(sens, logo().0)
}

#[cfg(not(windows))]
fn menu_width(_sens: Sens) -> i32 {
    0
}

/// Works the direction out again for a window about to be that tall, and
/// remembers it.
fn decide_the_direction(picture: (i32, i32, i32, i32), anchor: (i32, i32), height: i32) {
    let sens = ou_s_ouvre(picture, anchor, height);
    SENS.store(sens.range(), Ordering::Relaxed);
    A_DROITE.store(
        ou_s_ouvre_a_droite(picture, anchor, menu_width(sens)),
        Ordering::Relaxed,
    );
}

fn nudge() -> (i32, i32) {
    let both = NUDGE.load(Ordering::Relaxed);
    ((both >> 32) as i32, both as i32)
}

fn nudged_to(dx: i32, dy: i32) {
    NUDGE.store(
        (i64::from(dx) << 32) | i64::from(dy) & 0xFFFF_FFFF,
        Ordering::Relaxed,
    );
}

/// Reads back where the button was last put down.
///
/// Read once when the program opens and never again: the answer lives in
/// two numbers from then on, because everything that places the button
/// runs where nothing may touch a disk.
pub fn where_it_was_left() {
    let path = zyr_proto::paths::floating_button();
    let Ok(written) = std::fs::read_to_string(&path) else {
        return;
    };
    let read = |name: &str| {
        written
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix(name)?.trim().strip_prefix('='))
            .and_then(|value| value.trim().parse::<i32>().ok())
    };
    if let (Some(dx), Some(dy)) = (read("x"), read("y")) {
        nudged_to(dx, dy);
        note(&format!("bouton flottant repris à {dx}, {dy} du coin"));
    }
}

/// Writes down where a hand has just left it.
///
/// Once, when the hand lets go, and never during the drag: a hundred
/// writes a second to say where something is being moved to would be a
/// hundred writes of a place nobody chose.
fn leave_it_there() {
    let (dx, dy) = nudge();
    let written = format!(
        "# Où le bouton flottant d'une session a été posé, en pixels\n\
         # réels depuis le coin haut droit de l'image.\n\
         # Écrit par ZyrDesk, peut se corriger à la main.\n\
         x = {dx}\n\
         y = {dy}\n"
    );
    if let Err(e) = zyr_proto::files::replace(&zyr_proto::paths::floating_button(), &written) {
        note(&format!("place du bouton flottant non retenue : {e}"));
    }
}

impl Floating {
    /// Says a close is being asked for, and takes it back when it was
    /// refused: a session still running must be told apart from one this
    /// window brought down.
    fn closing(app: &App, asked: bool) {
        app.floating()
            .closing
            .store(asked, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether a close has been asked for, without forgetting it.
    ///
    /// Asked while a session is still opening, where nothing else can
    /// tell a player the person stopped from one the far computer turned
    /// away: both look like an engine that lost its stream. Left standing
    /// for `was_closed_on_purpose` to take, since that is what the
    /// opening reads once it is over.
    pub fn a_close_was_asked_for(app: &App) -> bool {
        app.floating()
            .closing
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether the session that just ended was closed on purpose, and
    /// forgets it either way.
    pub fn was_closed_on_purpose(app: &App) -> bool {
        app.floating()
            .closing
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}

/// Whether a session is running right now.
///
/// Read from what the button hangs on rather than asked of the service:
/// it is the same answer, it is already kept up to date every second, and
/// it costs nothing to whoever asks.
pub fn a_session_is_up(app: &App) -> bool {
    app.floating()
        .watched
        .lock()
        .expect("session suivie")
        .is_some()
}

/// Says which player this window has just started, before anybody else
/// knows, and where the session it shows can be ended.
pub fn expect(
    app: &App,
    process: u32,
    towards: &str,
    peer: Option<&zyr_transport::Fingerprint>,
    at: &str,
) {
    *app.floating().expected.lock().expect("session attendue") = Some(Expected {
        process,
        towards: towards.to_string(),
        peer: peer.map(|peer| peer.to_string()),
        at: at.to_string(),
    });
}

/// Forgets it, the session being over one way or another.
pub fn expect_nothing(app: &App) {
    *app.floating().expected.lock().expect("session attendue") = None;
}

/// The player the button belongs to right now, and whether the service
/// itself is the one naming it.
///
/// The service first: it knows every session on this computer, including
/// those another window opened. Failing that, the one this window has
/// just started, for as long as it has a picture up. That second answer
/// is what puts the button on screen with the picture rather than
/// several seconds behind it, and it is also what a session the service
/// has stopped naming looks like the moment it does, before its own
/// engine has any way to know: nothing in a single answer tells the two
/// apart, which is why the watch keeps its own memory of which one it
/// was last time.
async fn seen(app: &App) -> Option<(u32, bool)> {
    if let Some(session) = crate::session::sessions().await.into_iter().next() {
        return Some((session.process, true));
    }
    let expected = app
        .floating()
        .expected
        .lock()
        .expect("session attendue")
        .as_ref()
        .map(|expected| expected.process);
    // Still running, and never « still showing a window ». Minimising
    // ZyrDesk hides the picture, because the system takes an owned
    // window down with the one that owns it; read as the session being
    // over, that let go of the picture and closed the button under a
    // session that was still running, and the cross went back to merely
    // putting the window away. The session is the player, so the player
    // is what is asked about.
    expected
        .filter(|process| still_running(*process))
        .map(|process| (process, false))
}

/// The player the button belongs to right now.
pub async fn player(app: &App) -> Option<u32> {
    seen(app).await.map(|(process, _)| process)
}

/// Whether that player is still running.
#[cfg(windows)]
fn still_running(process: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: a refused or finished process gives a null handle, which
    // is one of the answers; a real one is closed right below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) };
    if handle.is_null() {
        return false;
    }
    let mut code = 0u32;
    // SAFETY: the handle is live and the slot is ours.
    let asked = unsafe { GetExitCodeProcess(handle, &mut code) };
    // SAFETY: the handle came from the call above and is closed once.
    unsafe { CloseHandle(handle) };
    // A handle can outlive the process it names: only the exit code says
    // which of the two this is.
    asked != 0 && code == STILL_ACTIVE as u32
}

#[cfg(not(windows))]
fn still_running(_process: u32) -> bool {
    false
}

/// Stops that player, and says whether it was there to be stopped.
///
/// What ends a session on this side when nothing else is going to: a far
/// computer that has stopped answering, or a person changing what the
/// session asks for, which the engine is only ever told once and at its
/// start. Nothing is lost by stopping it. The player holds no state worth
/// saving, the service gives the way back when the process is gone, and
/// whoever was waiting on that process wakes the moment it does.
///
/// Nought as the parting code, which is the one the engine gives when a
/// session ends normally: it did, this being what was asked for.
#[cfg(windows)]
pub fn stop_the_player(process: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: a refused or finished process gives a null handle, which is
    // one of the answers; a real one is closed right below.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, process) };
    if handle.is_null() {
        note(&format!("lecteur {process} déjà arrêté"));
        return false;
    }
    // SAFETY: the handle is live and was opened for exactly this.
    let stopped = unsafe { TerminateProcess(handle, 0) } != 0;
    // SAFETY: the handle came from the call above and is closed once.
    unsafe { CloseHandle(handle) };
    note(&if stopped {
        format!("lecteur {process} arrêté")
    } else {
        format!("Windows a refusé d'arrêter le lecteur {process}")
    });
    stopped
}

#[cfg(not(windows))]
pub fn stop_the_player(_process: u32) -> bool {
    false
}

/// Follows the sessions for as long as the program runs, and puts the
/// button up and down with them.
pub fn watch(app: App) {
    crate::app::spawn(async move {
        loop {
            tokio::time::sleep(LOOK).await;
            match seen(&app).await {
                Some((process, in_service)) => {
                    // The picture first: the button hangs from the corner
                    // of it, and a corner read before the picture has
                    // been laid in our window is the wrong corner.
                    crate::picture::hold(&app, process);
                    let fresh = adopt(&app, process);
                    let was_confirmed =
                        app.floating().confirmed.swap(in_service, Ordering::Relaxed);
                    if !fresh && was_confirmed && !in_service {
                        // Le service ne nomme plus cette voie, alors qu'il
                        // la nommait à l'instant d'avant, et le lecteur
                        // croit toujours tourner : une session finie
                        // d'ailleurs (éjection, réglage qui ferme les
                        // sessions en cours), que son moteur ne
                        // remarquerait de lui-même qu'à son silence,
                        // jusqu'à trente secondes plus tard. Arrêtée ici et
                        // tenue pour volontaire, le même chemin que la
                        // croix de la fenêtre prend.
                        note(&format!(
                            "le service ne connaît plus la voie du lecteur {process}, arrêté ici"
                        ));
                        Floating::closing(&app, true);
                        stop_the_player(process);
                        crate::touchpad::read_the_pad(false);
                        crate::picture::shut_the_pointer_in(false);
                        crate::picture::let_go(&app);
                        lower(&app);
                        continue;
                    }
                    if fresh {
                        // A session just adopted starts on the two sides
                        // its settings asked for; every toggle after that
                        // goes through this window and is counted as it
                        // is sent.
                        let preferred = crate::settings::preferred().await;
                        let state = app.floating();
                        state
                            .game_mouse
                            .store(!preferred.absolute_mouse, Ordering::Relaxed);
                        state
                            .system_keys
                            .store(preferred.system_keys, Ordering::Relaxed);
                        // The pad is only read while Windows has really
                        // let go of it, whatever the settings remember:
                        // both at once would have every gesture happen
                        // twice, once at each end.
                        state.touchpad.store(
                            preferred.touchpad_gestures
                                && crate::touchpad::what_windows_still_holds()
                                    .is_some_and(|held| !held.any()),
                            Ordering::Relaxed,
                        );
                        state
                            .clipboard
                            .store(preferred.shared_clipboard, Ordering::Relaxed);
                        // Off at every session, whatever the last one
                        // was left on: see the field itself.
                        state.voyants.store(false, Ordering::Relaxed);
                        state.pointer_held.store(false, Ordering::Relaxed);
                        // What the far computer draws is not put down
                        // here: it lives over there, in an engine this
                        // new player has not touched.
                    }
                    put_the_button_up(&app, process);
                    // The pad is read for exactly as long as a session is
                    // showing and the switch is on. Said at every turn
                    // rather than only when it moves: asking again for
                    // what is already the case costs nothing, and it is
                    // what puts the reading back after a session that
                    // ended badly.
                    crate::touchpad::read_the_pad(gestures_to_the_session(&app));
                    // Rien de tout cela pendant que le menu est ouvert :
                    // jeter un de ces interrupteurs donne le clavier à
                    // l'image, et une main qui lit le menu vise autre
                    // chose. Le changement de mode de souris, lui, les
                    // jette avec lui : le clavier vient de partir de
                    // toute façon, et attendre la fermeture du menu
                    // laissait un moment sans aucun curseur.
                    if !the_menu_is_open() {
                        keep_the_pointer_in_step(&app, process).await;
                        keep_the_far_pointer_in_step(&app).await;
                    }
                    // Et le pointeur de cet ordinateur-ci reste dans
                    // l'image tant que la souris est celle d'un jeu :
                    // caché et libre, il s'en va derrière la main qui
                    // joue. Ici et non chez le moteur, qui ne peut pas ;
                    // voir `picture::shut_the_pointer_in`. Le menu ouvert
                    // le rend, comme tout le reste : une main qui lit le
                    // menu vise autre chose.
                    crate::picture::shut_the_pointer_in(in_game_mouse(&app) && !the_menu_is_open());
                    // Et la forme que ce curseur prend, qui vient de
                    // l'ordinateur d'en face et se demande bien plus
                    // souvent que cette veille ne tourne : elle a sa
                    // propre boucle, relancée ici quand la précédente
                    // s'est arrêtée.
                    crate::pointeur::follow(&app);
                    // Et la santé de la session, relue bien plus souvent
                    // que cette veille-ci ne tourne : ce qu'elle allume
                    // doit se voir dans le tiers de seconde, et cette
                    // veille passe une fois par seconde.
                    crate::voyants::watch(&app);
                    // Et ce qui arrive des fichiers qu'on colle, relu au
                    // même rythme et pour la même raison : une barre qui
                    // avance une fois par seconde n'a pas l'air d'avancer.
                    crate::transfert::watch(&app);
                    // Et le clavier appartient à l'image, toujours. Le
                    // menu ne le lui prend plus : la carte que ce
                    // programme dessine n'est jamais activée et ne porte
                    // aucune page qui prendrait le focus, ce qui est
                    // encore une chose que la vue web coûtait. Ceci reste
                    // le filet, pour un clavier qu'une autre fenêtre a
                    // pris ; le système refuse de toute façon net tant
                    // qu'un autre programme est devant.
                    crate::picture::the_keyboard_back(&app);
                }
                None => {
                    // Le pointeur d'abord : sans session il n'y a plus
                    // d'image où l'enfermer, et une cage laissée derrière
                    // une session tient tout le bureau.
                    crate::picture::shut_the_pointer_in(false);
                    // Et le pavé n'est plus lu : sans image, un geste à
                    // trois doigts n'a plus où aller, et la main qui le
                    // fait s'adresse à cet ordinateur-ci.
                    crate::touchpad::read_the_pad(false);
                    crate::picture::let_go(&app);
                    lower(&app);
                }
            }
        }
    });
}

/// Takes that player as the session this window is following, and says
/// whether it had not already.
///
/// Nothing about what is on screen is asked here. Whether the button can
/// be shown depends on that; whether there is a session does not, and the
/// two used to be one decision: a window put down in the taskbar was read
/// as no session at all, so the cross went back to merely putting the
/// window away and left the session running behind it.
fn adopt(app: &App, process: u32) -> bool {
    let state = app.floating();
    let already = state
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .is_some_and(|seen| *seen == process);
    if already {
        return false;
    }
    // A new session starts with the button on screen, whatever was done
    // with the one before.
    HIDDEN.store(false, Ordering::Relaxed);
    *state.watched.lock().expect("session suivie") = Some(process);
    true
}

/// Puts the button up for that player, and keeps it up.
///
/// Called at every turn of the watch and does nothing when there is
/// nothing to do, so a session begun while ZyrDesk was down in the
/// taskbar still gets its button the moment the window comes back.
///
/// Waits for the player to have a window before showing anything. The
/// service calls a session held from the moment the player starts, but
/// the engine only opens its window once the far computer has answered
/// and the stream stands: showing the button any earlier would put it
/// over a screen that has no picture on it yet.
// Hors de Windows, le logo et la carte n'existent pas, et rien de ce qui
// reste ici ne demande le programme.
#[cfg_attr(not(windows), allow(unused_variables))]
fn put_the_button_up(app: &App, process: u32) {
    let Some(picture) = picture_of(process) else {
        return;
    };
    // A button over a window that is not on screen would be the only
    // thing showing, hanging in a corner over somebody else's work. It
    // goes up when the window does, which the watch sees a second later.
    // Minimised counts as not on screen and has to be asked for
    // separately: a window down in the taskbar still calls itself
    // visible.
    if !crate::fenetre::a_l_ecran() {
        return;
    }

    let size = button_size() as i32;
    ITS_LOGO.store(size, Ordering::Relaxed);

    // Le bouton est fait de deux fenêtres que ce programme dessine : le
    // logo, sur lequel une main se pose, et la carte du menu. Les ouvrir
    // ne coûte rien quand elles sont déjà ouvertes, ce qui fait de ce
    // passage-ci le même travail à chaque tour de veille.
    #[cfg(windows)]
    {
        let anchor = hung_from(picture, nudge(), (size, size), margin());
        let sens = Sens::lu(SENS.load(Ordering::Relaxed));
        let a_droite = A_DROITE.load(Ordering::Relaxed);
        crate::logo::raise(app, size as u32, sens == Sens::Haut, a_droite, anchor);
        // La carte se mesure sur ce que ses lignes demandent, donc elle a
        // besoin de savoir de combien un pixel de page compte ici et quel
        // thème la fenêtre porte.
        // Le thème est demandé au produit et non à la fenêtre : c'est la
        // même réponse pour tous les écrans, et une seule à tenir.
        crate::menu::raise(app, crate::fenetre::echelle(), crate::theme::light());
    }
    // Et les deux voyants, dans le coin d'en face. Ce qui s'ouvre ici est
    // la fenêtre qui les portera : elle reste rangée tant qu'il n'y a
    // rien à dire, ce qui est la plus grande partie d'une session.
    crate::voyants::raise(app, the_other_corner(picture));
    lay_the_button(picture);
}

/// Le coin haut gauche de l'image, à la même marge que le bouton.
///
/// La même marge et non une deuxième : les deux coins sont regardés l'un
/// après l'autre, et deux écarts différents se voient tout de suite.
fn the_other_corner(picture: (i32, i32, i32, i32)) -> (i32, i32) {
    let margin = margin();
    (picture.0 + margin, picture.1 + margin)
}

/// What the button comes to in real pixels, on the screen it hangs over.
///
/// Everything in this file is counted in real pixels: the picture is
/// measured with the system's own ruler, and so is the mouse. The design
/// system counts in the other kind, and on a screen magnified to a
/// hundred and seventy-five per cent the same button is forty-four of one
/// and seventy-seven of the other.
fn button_size() -> u32 {
    (BUTTON * f64::from(crate::fenetre::echelle())).ceil() as u32
}

/// Takes the button down.
///
/// Called by the watch when the session is no longer there, and by
/// whoever ended it the moment they know: a second of a button hanging
/// over a picture that has gone is a second too many, and the watch only
/// comes round once a second.
pub fn lower(app: &App) {
    let state = app.floating();
    if state
        .watched
        .lock()
        .expect("session suivie")
        .take()
        .is_some()
    {
        crate::voyants::lower(app);
        #[cfg(windows)]
        {
            crate::menu::lower(app);
            crate::logo::lower(app);
        }
    }
}

/// Range le bouton jusqu'à la prochaine session, ou jusqu'au raccourci
/// qui le rappelle.
///
/// Les deux fenêtres dont il est fait s'en vont ensemble, le logo et la
/// carte : l'une laissée debout serait un bouton à moitié rangé.
pub fn hide(app: &App) -> Result<(), String> {
    if !a_session_is_up(app) {
        return Err("le bouton flottant n'est plus là".to_string());
    }
    HIDDEN.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    {
        crate::menu::montre(false);
        crate::logo::shown(app, false);
    }
    Ok(())
}

/// Takes hold of the button, moves it with the mouse until it is let go,
/// and says whether the whole thing turned out to be a plain click.
///
/// Called by the logo's own window, which is what a hand comes down on.
/// The gesture is followed here and not there: that window is forty-four
/// pixels wide, the mouse leaves it on the first movement, and where the
/// system says the cursor is is the only answer that is always true.
///
/// Nothing is asked of the page while this runs, and the menu is left
/// open if it was: a window that changes size under the mouse gets away
/// from it.
pub async fn grabbed(app: &App) -> bool {
    let held = *app.floating().watched.lock().expect("session suivie");
    let Some(process) = held else {
        return true;
    };
    // The picture is read once: it does not move while the button is
    // being dragged over it, and looking for it again at every step
    // would mean enumerating every window on the machine a hundred times
    // a second.
    let (Some(start), Some(picture)) = (cursor_now(), picture_of(process)) else {
        return true;
    };
    // D'où le bouton part, compté et non relu : c'est le même calcul qui
    // le pose, à partir des mêmes nombres, donc les deux ne peuvent pas
    // se contredire.
    let from = hung_from(picture, nudge(), logo(), margin());

    let until = std::time::Instant::now() + AT_MOST;
    let mut moved = false;
    // Where the button hangs, and where the cursor was when it was last
    // moved there. Both carried forward step by step rather than measured
    // from the start of the gesture, and that is the whole of the repair:
    // the button is held against the picture, so a hand that goes on past
    // an edge asks for a place the button cannot take. Counted from the
    // start, every one of those pixels stayed in the sum, and coming back
    // moved nothing until the hand had given all of them back. The button
    // sat at the edge while the cursor was half a screen away, which is
    // exactly what was reported.
    let mut at = from;
    let mut was = start;
    while held_down() && std::time::Instant::now() < until {
        let Some(now) = cursor_now() else {
            break;
        };
        if !moved {
            if (now.0 - start.0).abs() < GRIP && (now.1 - start.1).abs() < GRIP {
                tokio::time::sleep(FOLLOW).await;
                continue;
            }
            moved = true;
            moving(true);
        }
        at = slide(picture, at, now.0 - was.0, now.1 - was.1);
        was = now;
        tokio::time::sleep(FOLLOW).await;
    }
    if moved {
        moving(false);
        leave_it_there();
    }
    !moved
}

/// Dit au logo que le geste est devenu un déplacement, au cran près où
/// il le devient, puis qu'il est fini.
#[cfg(windows)]
fn moving(yes: bool) {
    crate::logo::moving(yes);
}

#[cfg(not(windows))]
fn moving(_yes: bool) {}

/// Puts the button where the mouse has dragged it to, and says where
/// that turned out to be.
///
/// Where it turned out to be, because it is not always where it was
/// asked: the button is held against the picture. Whoever is following a
/// hand has to carry that answer forward rather than their own ask, or
/// the two drift apart by everything the picture refused.
///
/// The distance from the corner of the picture is what is remembered
/// rather than the place on screen: a session opened later on another
/// screen, or at another size, then finds the button where it was left
/// rather than off the edge.
fn slide(picture: (i32, i32, i32, i32), from: (i32, i32), dx: i32, dy: i32) -> (i32, i32) {
    let logo = logo();
    let anchor = held_inside((from.0 + dx, from.1 + dy), picture, logo.0, logo.1);

    let margin = margin();
    nudged_to(
        anchor.0 - (picture.2 - margin),
        anchor.1 - (picture.1 + margin),
    );
    // A button dragged towards the bottom of the picture works out as it
    // goes that its menu would be better off above, rather than at the
    // next turn of the watch.
    decide_the_direction(picture, anchor, menu_height());

    // Asked of the system and not of the toolkit, like everywhere else
    // the button moves: this runs a hundred times a second under a hand,
    // and a trip through an event queue at that rhythm is what a button
    // lagging its own cursor is made of.
    put_the_button(picture, anchor);
    anchor
}

/// Where the button hangs: the top right of the picture, moved by
/// whatever dragging has moved it since.
fn hung_from(
    picture: (i32, i32, i32, i32),
    nudge: (i32, i32),
    size: (i32, i32),
    margin: i32,
) -> (i32, i32) {
    let corner = (picture.2 - margin + nudge.0, picture.1 + margin + nudge.1);
    held_inside(corner, picture, size.0, size.1)
}

/// Keeps the button against the picture, whatever it was asked.
///
/// A button dragged towards an edge, or a session opened on a smaller
/// screen than the last, would otherwise end up somewhere nobody can
/// click it.
///
/// A picture smaller than the button is answered rather than refused.
/// The window is held to a size that leaves room for the button while a
/// hand is resizing it, but a window can change size in ways no hand
/// asked for, and this runs inside the system's own call into that
/// window: there is nowhere for a refusal to go, and a panic there takes
/// the whole program with it. So the button keeps the corner it belongs
/// to and hangs over the edge, which is what the other side of this
/// already did.
fn held_inside(
    corner: (i32, i32),
    picture: (i32, i32, i32, i32),
    width: i32,
    height: i32,
) -> (i32, i32) {
    let (left, top, right, bottom) = picture;
    (
        corner.0.clamp((left + width).min(right), right),
        corner.1.clamp(top, (bottom - height).max(top)),
    )
}

/// Brings the button back and opens its menu, and closes it again when it
/// is already open.
///
/// What a shortcut needs to be able to do above all else: hiding the
/// button is otherwise a decision with no way back before the session
/// ends.
///
/// Both ways round, because one combination that only opens leaves the
/// hand reaching for the mouse to undo what the keyboard just did.
pub fn show_the_menu(app: &App) -> Result<(), String> {
    if !a_session_is_up(app) {
        return Err("aucune session en cours".to_string());
    }
    // Asked for again with the menu already open is asking to be rid of
    // it. Everything below is about getting to a menu, and none of it is
    // wanted here: the session is already where it should be, and putting
    // the pointer back or showing a button that is shown would be undoing
    // what the person did between the two presses.
    #[cfg(windows)]
    if crate::menu::ouvert() {
        crate::menu::montre(false);
        return Ok(());
    }
    // The session first, when it was put away: the button hangs on the
    // picture, and a menu opened over an empty desktop, picture down in
    // the taskbar, is a button floating over somebody else's work. The
    // shortcut asks to do something with the session, so the session
    // comes back.
    if !crate::fenetre::a_l_ecran() {
        crate::show_home(app);
    }
    // Asked for by name, which takes back the choice of hiding it.
    HIDDEN.store(false, Ordering::Relaxed);
    // In game mouse mode the pointer is shut on a point in the middle of
    // the picture, so it cannot be brought to this button at all. Asking
    // for the menu is asking to do something with the pointer, so the
    // cage is opened here rather than at the next turn of the watch: a
    // menu that takes a second to become usable reads as a menu that
    // does not work.
    //
    // The mouse mode itself is left exactly where it is. Opening this
    // menu used to throw the session out of game mouse mode, by typing
    // the engine's own combination into the picture, which meant handing
    // the picture the keyboard first: it was refused four times running
    // on the very session that asked for this, and the menu stayed
    // unusable. It also changed a mode nobody asked to change. The
    // engine now reads the movement of a game only while the pointer
    // stands on the picture, so an open cage is the whole of what this
    // needs.
    #[cfg(windows)]
    crate::picture::shut_the_pointer_in(false);
    #[cfg(windows)]
    {
        crate::logo::shown(app, true);
        // And the pointer is put on the button, the cage having just been
        // opened. A game hides the pointer over the picture and only over
        // it, so one freed in the middle of the picture has to cross it
        // unseen to reach this button: aiming blind, on the one thing a
        // session cannot be left without.
        if in_game_mouse(app) {
            crate::picture::put_the_pointer_on(crate::logo::its_window());
        }
        crate::menu::montre(true);
    }
    Ok(())
}

/// The same, from anywhere in the program rather than from the page.
pub fn in_game_mouse(app: &App) -> bool {
    app.floating().game_mouse.load(Ordering::Relaxed)
}

/// The same, from anywhere in the program rather than from the page.
pub fn keys_to_the_session(app: &App) -> bool {
    app.floating().system_keys.load(Ordering::Relaxed)
}

/// The same, for the touchpad's own gestures.
pub fn gestures_to_the_session(app: &App) -> bool {
    app.floating().touchpad.load(Ordering::Relaxed)
}

/// The same, for the one clipboard the two computers share.
pub fn the_clipboard_is_shared(app: &App) -> bool {
    app.floating().clipboard.load(Ordering::Relaxed)
}

/// Whether the badges in the corner of the picture are being held on
/// screen rather than left to show themselves.
pub fn the_voyants_are_held_up(app: &App) -> bool {
    app.floating().voyants.load(Ordering::Relaxed)
}

/// Holds the two badges on screen, or lets them go back to showing
/// themselves.
///
/// Held up, each is still lit or dim by what it reads: it is where they
/// are drawn that this changes and never what they say. A switch that
/// lit them as well would show two badges that are always wrong, which
/// is no use to anybody looking at how they behave.
///
/// The watch reads this at its own turn, eighty milliseconds away, so
/// nothing has to be told: they are there, or gone, before the hand has
/// left the menu.
fn always_show_the_voyants(app: &App) -> Result<(), String> {
    let state = app.floating();
    let held = !state.voyants.load(Ordering::Relaxed);
    state.voyants.store(held, Ordering::Relaxed);
    note(if held {
        "voyants : tenus à l'écran, allumés ou éteints selon ce qu'ils lisent"
    } else {
        "voyants : rendus à eux-mêmes, ils ne se montrent qu'en cas de besoin"
    });
    Ok(())
}

/// Throws the switch that decides whether the two computers share one
/// clipboard.
///
/// The shortest of them all, and that is the whole design of it: nothing
/// here reads or writes a clipboard. The two services do that, on the
/// product's own channel inside the tunnel, and they read this switch
/// from the settings at every turn of their watch. So throwing it is
/// writing it down, and the sharing starts or stops within the quarter
/// of a second that follows.
async fn share_the_clipboard(app: &App) -> Result<(), String> {
    let state = app.floating();
    let shared = !state.clipboard.load(Ordering::Relaxed);
    state.clipboard.store(shared, Ordering::Relaxed);
    crate::settings::remember_shared_clipboard(shared).await;
    Ok(())
}

/// Throws the switch that decides which of the two computers the
/// touchpad's own gestures belong to.
///
/// Ours to do, like the five above it, and for a plainer reason than any
/// of them: the pad is read by this program and by nothing else. No
/// engine is aware there is one, and nothing in what they speak carries a
/// hand with several fingers on it.
///
/// Turning it on is refused while Windows still answers those gestures
/// itself, and that refusal is the whole honesty of this switch. Windows
/// only lets go of them from a page of its own settings, and no call
/// changes it: the values behind that page are read once, deep in the
/// system's own input, and a program writing them changes what the page
/// shows and nothing about what the pad does. Left to happen anyway, the
/// switch would be on and every gesture would act twice, once at each
/// end, which is worse than not having it at all. So the page is opened
/// where the person has to act, and nothing is said to have been done.
async fn the_pad_to_the_session(app: &App) -> Result<(), String> {
    let state = app.floating();
    if state.touchpad.load(Ordering::Relaxed) {
        state.touchpad.store(false, Ordering::Relaxed);
        crate::touchpad::read_the_pad(false);
        crate::settings::remember_touchpad_gestures(false).await;
        return Ok(());
    }
    let Some(held) = crate::touchpad::what_windows_still_holds() else {
        return Err("cet ordinateur n'a pas de pavé tactile de précision.".to_string());
    };
    if held.any() {
        let what_to_do = held.what_to_do();
        note(&format!("gestes du pavé tactile : {what_to_do}"));
        crate::touchpad::open_the_windows_page();
        return Err(what_to_do);
    }
    if !crate::touchpad::read_the_pad(true) {
        return Err("Windows n'a pas donné le pavé tactile à ZyrDesk.".to_string());
    }
    state.touchpad.store(true, Ordering::Relaxed);
    // Remembered like the keyboard beside it: this is thrown in the
    // middle of a session, and the side it is left on is the side the
    // next session should open on.
    crate::settings::remember_touchpad_gestures(true).await;
    // The slide travels as an Alt+Tab typed by hand, so it travels by the
    // same road and under the same condition. Said rather than refused:
    // the tap is a click and crosses whatever the keyboard is doing, so
    // half of what was asked for works from this moment.
    if !keys_to_the_session(app) {
        note(
            "gestes du pavé tactile : le glissement à trois doigts change de fenêtre \
             sur l'ordinateur d'en face seulement quand le clavier lui appartient \
             (ligne « Clavier », côté « Immersif »)",
        );
    }
    Ok(())
}

/// Ce qu'un geste du pavé fait de la session.
///
/// Appelé depuis le fil qui lit le pavé, et rien ici n'attend quoi que ce
/// soit : un geste est un clic ou une frappe, que le système prend et
/// rend aussitôt.
///
/// Aucun n'est envoyé « à la session » : ils sont faits sur cet
/// ordinateur-ci, exactement comme une main les ferait, et c'est le
/// moteur qui les porte de l'autre côté. Un clic va là où le pointeur est
/// posé, donc dans l'image quand la main y est ; Alt+Tab et la touche
/// lecture/pause sont reprises à Windows par le moteur, qui les donne à
/// l'ordinateur d'en face, ce qui est le chemin d'un Alt+Tab tapé au
/// clavier.
pub fn the_pad_said(gesture: crate::touchpad::Gesture) {
    use crate::touchpad::Gesture;

    let done = match gesture {
        Gesture::ThreeTap => a_middle_click(),
        Gesture::FourTap => play_or_pause(),
        // Vers la droite la fenêtre suivante, vers la gauche la
        // précédente : c'est le sens que Windows donne aux mêmes gestes,
        // et une main n'apprend pas deux sens pour un seul geste.
        Gesture::Rightwards => the_window_after(false),
        Gesture::Leftwards => the_window_after(true),
    };
    if !done {
        note(&format!("{gesture} : Windows a refusé de le transmettre"));
    }
}

/// The same, from anywhere in the program rather than from the menu.
pub async fn ask(app: &App, act: Act) -> Result<(), String> {
    // Ours to do, all of them, and none goes through the engine's
    // keyboard. Covering the screen is still a session matter: the
    // shortcut is registered with the system for the whole life of the
    // program, and without a session it would fullscreen the empty home
    // screen and write that down as the choice for the next session.
    match act {
        Act::Fullscreen => {
            if !a_session_is_up(app) {
                return Err("aucune session en cours".to_string());
            }
            return crate::picture::toggle_the_screen(app);
        }
        Act::End => return end_the_session(app).await,
        Act::SecureAttention => return press_ctrl_alt_del_over_there(app).await,
        Act::LockScreen => return lock_over_there(app).await,
        Act::Sound => return hush_the_session(app).await,
        Act::Touchpad => return the_pad_to_the_session(app).await,
        Act::Clipboard => return share_the_clipboard(app).await,
        Act::Voyants => return always_show_the_voyants(app),
        _ => {}
    }

    let process = the_player(app)?;

    type_at_the_picture(app, act, process).await?;
    // The keystroke left, so the engine will act on it: what this window
    // believes of the two switches follows the keystrokes it sends.
    match act {
        Act::MouseMode => {
            let _ = app.floating().game_mouse.fetch_xor(true, Ordering::Relaxed);
            // The two pointer switches belong to this change and are
            // thrown with it rather than left to the watch. The moment
            // the mode reaches the engine it stops drawing the pointer
            // this window asked it for, and the far computer's is still
            // hidden: between the two there is no pointer at all. Asked
            // from the menu, that hole lasted as long as the menu stayed
            // open, the watch holding off for it.
            keep_the_far_pointer_in_step(app).await;
        }
        Act::PointerLock => {
            let _ = app
                .floating()
                .pointer_held
                .fetch_xor(true, Ordering::Relaxed);
        }
        Act::SystemKeys => {
            let theirs = !app
                .floating()
                .system_keys
                .fetch_xor(true, Ordering::Relaxed);
            // Remembered, unlike the mouse: this one is thrown back and
            // forth in the middle of a session, and the side it is left on
            // is the side the next session should open on.
            crate::settings::remember_system_keys(theirs).await;
        }
        _ => {}
    }
    Ok(())
}

/// Whether the menu of the floating button is open right now.
///
/// What holds the two switches below off while a hand is in it: throwing
/// either of them gives the keyboard to the picture, and a hand reading
/// the menu is aiming at something else.
fn the_menu_is_open() -> bool {
    #[cfg(windows)]
    {
        crate::menu::ouvert()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Keeps the pointer inside the picture for as long as the picture is
/// the whole screen, and lets it go the moment it is not.
///
/// The engine has this and cannot use it. It ties the pointer to its own
/// window being on a whole screen, and its window is a small windowed one
/// carried inside ours for the length of a session: the condition is
/// false all session long, whatever the person is actually looking at.
/// Asked there, the pointer wandered off the picture with nothing to stop
/// it, which on a machine with a second screen means it simply leaves.
///
/// So this program answers instead, and says so with the engine's own
/// switch. The engine stops deciding for itself the first time that
/// switch is thrown, which is exactly what is wanted: from then on there
/// is one opinion about the pointer and it is the right one.
///
/// Windowed, the pointer must be free to leave: the other windows of this
/// computer are around the picture and reaching them is the whole reason
/// somebody is not in full screen.
async fn keep_the_pointer_in_step(app: &App, process: u32) {
    let wanted = crate::picture::on_the_whole_screen();
    let state = app.floating();
    if wanted == state.pointer_held.load(Ordering::Relaxed) {
        return;
    }
    match type_at_the_picture(app, Act::PointerLock, process).await {
        Ok(()) => {
            state.pointer_held.store(wanted, Ordering::Relaxed);
            note(if wanted {
                "pointeur tenu dans l'image, qui est tout l'écran"
            } else {
                "pointeur rendu à l'écran, l'image n'en occupe plus la totalité"
            });
        }
        Err(reason) => note(&format!("pointeur non réglé : {reason}")),
    }
}

/// Puts the pointer a desktop is driven with on this side of the network,
/// and gives it back to the far computer for a game.
///
/// A pointer drawn by the far computer is that computer's answer to a
/// movement that has crossed the network twice and been encoded on the
/// way: it arrives after the hand has already moved on, and no amount of
/// bitrate shortens it. A desktop is aimed at, so that is felt on every
/// click. This computer knows where the hand is with no network at all,
/// and drawing the pointer here is what every remote desktop product
/// does.
///
/// Two switches for one idea, one at each end, because the pointer is two
/// things: the engine here hides ours the moment it is over the picture,
/// and the engine over there draws its own into the stream. Throwing only
/// the first would leave two pointers on screen, one under the hand and
/// one behind it.
///
/// They are thrown in the order that keeps one pointer on screen at every
/// moment, and never nought: the far one is only taken away once this one
/// is drawn, and given back before this one goes. Each is thrown on its
/// own and either can be refused, so the order is all there is.
///
/// A game is the other way round. What a game reads is movement and not a
/// place, the pointer belongs to the game and is drawn by it, and a
/// second one drawn here would sit in the middle of the picture doing
/// nothing. So the far computer draws its own again there.
///
/// The switch on this side needs nothing said to it: the engine draws
/// that pointer because it was asked to follow the file of shapes, and
/// hides it by itself in game mouse mode, the mode doing it.
///
/// Said at every turn of the watch and not only when it changes. It is a
/// value the far engine is told and not a switch flipped, so saying it
/// twice says it once and costs that engine one comparison. And what it
/// was left doing by whoever watched that computer before is not
/// something this window can know: speaking only on a change means
/// speaking from a belief, and a session that opened in game mouse mode
/// agreed with its own belief, said nothing, and watched an engine that
/// had stopped drawing. No pointer at all, again.
async fn keep_the_far_pointer_in_step(app: &App) {
    let drawn = in_game_mouse(app);
    let state = app.floating();
    // What was last said and got through, and nothing else: it decides
    // what the journal says, never what is said to the far computer. One
    // line a second would drown every other line of a session.
    match say_whether_the_far_pointer_is_drawn(app, drawn).await {
        Ok(()) => {
            let was_refused = state.far_pointer_refused.swap(false, Ordering::Relaxed);
            let moved = state.far_pointer_hidden.swap(!drawn, Ordering::Relaxed) == drawn;
            if moved || was_refused {
                note(if drawn {
                    "l'ordinateur distant dessine à nouveau son curseur dans l'image"
                } else {
                    "l'ordinateur distant ne dessine plus son curseur dans l'image"
                });
            }
        }
        // Said once for a run of them and again when it stops, never at
        // every turn. A refusal repeated every second is a page of the
        // same line, and one said only the first time is a session that
        // went wrong in silence for as long as it lasted: both hide
        // exactly what has to be read here.
        Err(reason) => {
            if !state.far_pointer_refused.swap(true, Ordering::Relaxed) {
                note(&format!("curseur d'en face non réglé : {reason}"));
            }
        }
    }
}

/// Says to the far computer whether its engine draws its own pointer.
///
/// Said and never toggled, which is why it goes round by the two
/// services rather than as a keystroke into the picture. A keystroke
/// only flips that switch, and what it flips lives in an engine started
/// with its service: nobody can read where it stands, every session
/// shares it, and one session that ended without putting it back left
/// the next one flipping it the wrong way while believing the opposite.
/// Said, asking twice for the same thing asks for nothing.
async fn say_whether_the_far_pointer_is_drawn(app: &App, drawn: bool) -> Result<(), String> {
    let way = the_way_of_this_session(app).await?;
    crate::service::ask(&zyr_control::Request::FarPointerDrawn { way, drawn })
        .await
        .map(|_| ())
}

/// Gives the far computer its pointer back, the session being over.
///
/// Said before the way is closed, which is the last moment anything can
/// be said to that computer at all: the switch lives in its engine, and
/// that engine is started with its service and outlives every session.
///
/// It is politeness and no longer a duty. A session that opens says what
/// it wants of that pointer and gets it, whatever the one before left
/// behind; this only spares the machine being watched a pointer missing
/// from its own pictures until somebody watches it again. Said whatever
/// this window last asked for, for the same reason as above: what that
/// engine is doing is not something this window knows.
async fn give_the_far_pointer_back(app: &App) {
    let state = app.floating();
    match say_whether_the_far_pointer_is_drawn(app, true).await {
        Ok(()) => {
            state.far_pointer_hidden.store(false, Ordering::Relaxed);
            note("curseur rendu à l'ordinateur distant avant la fin de la session");
        }
        Err(reason) => note(&format!(
            "curseur non rendu à l'ordinateur distant : {reason}"
        )),
    }
}

/// The player the button is hanging on right now.
///
/// What the watch adopted, and never the first session it can find: with
/// two sessions open, what this window's menu asks for is this window's.
fn the_player(app: &App) -> Result<u32, String> {
    app.floating()
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .copied()
        .ok_or_else(|| "aucune session en cours".to_string())
}

/// The same, from anywhere in the program rather than from the page.
pub async fn hushed(app: &App) -> Result<bool, String> {
    let process = the_player(app)?;
    aside(move || zyr_sound::muted(process)).await
}

/// Hushes the session's sound on this computer, or gives it back.
///
/// Here and not over there. The far computer goes on playing whatever it
/// plays, and the person who asked is not asking for silence in a room
/// they are not in: they are asking for silence in theirs. The player
/// has a strip in this computer's volume mixer like any other program,
/// and that is the strip this pulls down, so nothing else playing here
/// is touched.
async fn hush_the_session(app: &App) -> Result<(), String> {
    let process = the_player(app)?;
    let quiet = !aside(move || zyr_sound::muted(process)).await?;
    aside(move || zyr_sound::mute(process, quiet)).await?;
    note(&format!(
        "son du lecteur {process} {}",
        if quiet { "coupé" } else { "rendu" }
    ));
    Ok(())
}

/// Asks the mixer, off the threads that must not wait.
///
/// Every question in `zyr-sound` is a round trip through COM, which is
/// quick and is still not something to do on a runtime that has a
/// picture to keep flowing.
async fn aside<T: Send + 'static>(
    ask: impl FnOnce() -> Result<T, zyr_sound::Trouble> + Send + 'static,
) -> Result<T, String> {
    crate::app::spawn_blocking(ask)
        .await
        .map_err(|e| format!("le mélangeur n'a pas répondu : {e}"))?
        .map_err(|e| e.to_string())
}

/// Gives the picture the keyboard back and types the engine's shortcut
/// into it, from anywhere in the program.
///
/// The two are one thing and are done in one place. A keystroke goes to
/// whatever window has the keyboard, and clicking this button gives it to
/// this button's own page: sent from there, a shortcut is read by our own
/// web view and thrown away, while `SendInput` reports the same success it
/// reports for one that arrived. That is the whole of « the Statistics
/// entry does nothing »: the journal said the keystroke had left, and it
/// had, into our own window.
///
/// Handed to the thread that draws. Giving another program's window the
/// keyboard is only possible from the thread whose input this program
/// joined to that program's, and reading back where it went is only
/// truthful from that same thread.
async fn type_at_the_picture(app: &App, act: Act, process: u32) -> Result<(), String> {
    let (say, mut heard) = tokio::sync::mpsc::channel(1);
    app.run_on_main_thread(move || {
        // Nothing else sends on it and it holds one: this cannot wait.
        let _ = say.try_send(hand_over_and_type(act, process));
    })
    .map_err(|e| e.to_string())?;
    heard
        .recv()
        .await
        .unwrap_or_else(|| Err("la fenêtre de ZyrDesk n'a pas répondu".to_string()))
}

/// The same on the spot, for callers already on the thread that draws.
#[cfg(windows)]
fn hand_over_and_type(act: Act, process: u32) -> Result<(), String> {
    if !crate::picture::the_keyboard_to_the_picture() {
        note(&format!(
            "{act} refusé : l'image du lecteur {process} n'a pas repris le clavier ; \
             le premier plan est {}",
            crate::picture::the_front_in_words()
        ));
        return Err("la session n'a pas repris le clavier.\n  \
             Cliquez d'abord dans l'image."
            .to_string());
    }
    shortcut(act, process)
}

#[cfg(not(windows))]
fn hand_over_and_type(_act: Act, _process: u32) -> Result<(), String> {
    Err("les sessions ne tournent que sous Windows".to_string())
}

/// Waits a moment for that player to stop, and says whether it did.
///
/// The player and not its window. A player that has lost its far
/// computer keeps a window, and puts its own notice in it: read from the
/// window, a session that had nothing left to show counted as over, and
/// nothing took it off the screen.
async fn the_player_has_stopped(process: u32) -> bool {
    let until = std::time::Instant::now() + CLOSING_SHOWS;
    while std::time::Instant::now() < until {
        if !still_running(process) {
            return true;
        }
        tokio::time::sleep(STOP_STEP).await;
    }
    false
}

/// Presses Ctrl+Alt+Suppr on the far computer.
///
/// It goes nowhere near the picture, and could not. Windows keeps that
/// combination for itself at both ends of a session: this computer never
/// sees it, because its own Windows takes it before any program does, and
/// the far computer cannot be made to feel it by an engine, because the
/// way an engine types is exactly the way Windows refuses for this one.
///
/// So it travels on the product's own channel, from this service to the
/// one over there, which presses it on its own machine. That is why this
/// is handled here rather than among the keystrokes: it has no letter and
/// no place on a keyboard, and never will.
///
async fn press_ctrl_alt_del_over_there(app: &App) -> Result<(), String> {
    let way = the_way_of_this_session(app).await?;
    crate::service::ask(&zyr_control::Request::SecureAttention { way })
        .await
        .map(|_| ())
}

/// Puts the far computer's lock screen up.
///
/// What stands in for Windows+L, and it exists because that combination
/// itself cannot be made to travel. Windows handles it where no program
/// can see it, on purpose: it is one of the two gestures that hand a
/// machine back to whoever is sitting at it. Pressed here it locks this
/// computer whatever a session is doing, and there is no way to type it
/// over there either.
///
/// So it goes round the same way Ctrl+Alt+Suppr does, and for the same
/// reason: some things a session needs have no letter, no place on a
/// keyboard, and never will.
async fn lock_over_there(app: &App) -> Result<(), String> {
    let way = the_way_of_this_session(app).await?;
    // Timed from here because here is where the picture is watched. The
    // far computer says what its own half cost, and the two together say
    // whether a picture that stands still for a second is standing still
    // on the road or on the machine.
    let asked_at = std::time::Instant::now();
    let answer = crate::service::ask(&zyr_control::Request::LockScreen { way }).await;
    note(&format!(
        "verrouillage de l'ordinateur distant : {} en {} ms",
        if answer.is_ok() { "fait" } else { "refusé" },
        asked_at.elapsed().as_millis()
    ));
    answer.map(|_| ())
}

/// The way this window's own session runs on.
///
/// Found the way ending one finds it: the service knows every session on
/// this computer, and it is the one the button hangs on that is meant,
/// never merely the first of the list. With two sessions open, what this
/// menu asks for must reach the picture this menu belongs to.
async fn the_way_of_this_session(app: &App) -> Result<zyr_control::WayId, String> {
    let watched = *app.floating().watched.lock().expect("session suivie");

    let sessions = crate::session::sessions().await;
    let ours = watched
        .and_then(|process| sessions.iter().find(|session| session.process == process))
        .or_else(|| sessions.first())
        .ok_or("aucune session en cours")?;
    Ok(zyr_control::WayId(ours.way))
}

/// Ends the session: the far computer is handed its desktop back, and
/// the picture goes here whatever that computer has to say about it.
///
/// The two halves are deliberately not tied together. Handing the desktop
/// back is a question asked over the network, and a computer that has
/// stopped answering takes fifteen seconds to be found out; the person
/// who just closed the session must not be held in front of a dead
/// picture for as long as that takes. So the question is asked on a
/// thread of its own, the player is given a moment to stop of its own
/// accord, which is what happens when the far computer answers, and it is
/// stopped here when it does not.
///
/// Where to ask comes from the service first: it knows every session on
/// this computer, including those another window opened. And it is the
/// session the button hangs on that is ended, never merely the first of
/// the list: with two sessions open, ending from this window must end
/// this window's.
///
/// The service is not the only source, because for the first seconds of
/// a session it does not believe in it yet: until then the way is what
/// this window wrote down when it started the player, and without that
/// fallback the cross and the menu answered « aucune session en cours »
/// over a running picture.
///
/// And before there is a player at all, there is an opening: a tunnel
/// being raced for, a far engine starting over, two computers being
/// introduced. Closing then is closing that, and it is said before
/// anything else here so the opening reads it at its very next step
/// rather than after the question below has been round the service.
async fn end_the_session(app: &App) -> Result<(), String> {
    let opening = crate::session::opening();
    if opening {
        Floating::closing(app, true);
    }
    let watched = *app.floating().watched.lock().expect("session suivie");

    let mut sessions = crate::session::sessions().await;
    let ours = watched
        .and_then(|process| {
            sessions
                .iter()
                .position(|session| session.process == process)
        })
        .or_else(|| (!sessions.is_empty()).then_some(0));

    let (process, towards, peer, at) = match ours {
        Some(place) => {
            let session = sessions.swap_remove(place);
            (
                session.process,
                session.towards,
                Some(session.fingerprint),
                session.at,
            )
        }
        None => {
            let expected = app
                .floating()
                .expected
                .lock()
                .expect("session attendue")
                .as_ref()
                .map(|expected| {
                    (
                        expected.process,
                        expected.towards.clone(),
                        expected.peer.clone(),
                        expected.at.clone(),
                    )
                });
            // An opening with no player yet has been let go of above,
            // and that is the whole of what closing means at that
            // moment: there is nothing to hand back and nothing to stop.
            match expected {
                Some(session) => session,
                None if opening => return Ok(()),
                None => return Err("aucune session en cours".to_string()),
            }
        }
    };
    note(&format!("fermeture demandée sur {towards} à travers {at}"));
    // Before anything else is asked, and through the player while there
    // still is one: the far computer's pointer is given back over its own
    // session's stream, and in a moment there will be no stream.
    give_the_far_pointer_back(app).await;
    // Said before the asking, and never taken back. The engine can lose
    // its stream and stop before the far computer has finished answering,
    // and a session reported as broken to whoever just closed it would be
    // a lie; and the player is stopped here in any case, which is that
    // person's doing too.
    Floating::closing(app, true);

    // Asked on a thread of its own, and nothing here waits for it. What
    // comes back only reaches the journal: by the time it does, the
    // session is over on this side one way or the other, and a refusal
    // shown then would be a red line across a home screen about a session
    // the person has already left.
    crate::app::spawn(async move {
        let answered = crate::app::spawn_blocking(move || {
            zyr_session::close_on_the_far_computer(peer.as_deref(), &towards, &at)
        })
        .await;
        note(&match answered {
            Ok(Ok(())) => "bureau distant rendu".to_string(),
            Ok(Err(e)) => format!(
                "bureau distant non rendu : {}",
                e.to_string().replace('\n', " ")
            ),
            Err(e) => format!("bureau distant non rendu : {e}"),
        });
    });

    // The far computer letting its desktop go is what stops the player,
    // and that is how a session ends when everything works. Given a
    // moment, and no more.
    if the_player_has_stopped(process).await {
        return Ok(());
    }
    note(&format!(
        "l'ordinateur distant n'a pas rendu la main à temps : lecteur {process} arrêté ici"
    ));
    stop_the_player(process);
    Ok(())
}

/* ---- Ce qui appartient à Windows ------------------------------------- */

/// Lays the button where the picture is now.
///
/// Called at every turn of the watch and at every step of a drag, so
/// nothing here waits for anything.
pub fn lay_the_button(picture: (i32, i32, i32, i32)) {
    let anchor = hung_from(picture, nudge(), logo(), margin());
    decide_the_direction(picture, anchor, menu_height());
    put_the_button(picture, anchor);
    // Les voyants suivent l'image d'ici, et non d'une veille à eux : ils
    // sont posés sur le même bord que ce bouton, et une image qu'on
    // redimensionne les emmènerait chacun à son rythme.
    crate::voyants::lay(the_other_corner(picture));
}

/// Ce que la carte du menu prend de haut, qui décide du sens d'ouverture.
#[cfg(windows)]
fn menu_height() -> i32 {
    crate::menu::haute()
}

#[cfg(not(windows))]
fn menu_height() -> i32 {
    0
}

/// Pose les deux fenêtres du bouton, et les montre ou les range.
///
/// Une seule ancre pour les deux : c'est ce qui les empêche d'être en
/// désaccord sur l'endroit où se trouve le bouton, et il n'y a plus rien
/// entre ce qu'on veut et ce qui est dessiné, la page qui prenait une
/// image de retard étant partie.
#[cfg(windows)]
fn put_the_button(picture: (i32, i32, i32, i32), anchor: (i32, i32)) {
    let sens = Sens::lu(SENS.load(Ordering::Relaxed));
    let a_droite = A_DROITE.load(Ordering::Relaxed);
    // Le logo ne bouge que de deux coins, à côté la carte ne partant
    // plus de lui : il garde le coin qu'il a quand elle est dessous. Son
    // dessin, lui, se retourne avec le bord d'où la carte part, pour
    // faire face au menu plutôt que de lui tourner le dos.
    crate::logo::lay(anchor, sens == Sens::Haut, a_droite);
    crate::menu::lay(anchor, sens, a_droite, crate::logo::box_side(), picture);

    // Le système remonte une fenêtre possédée avec celle qui la possède,
    // ce qui est juste pour un bouton qui n'est en bas que parce que la
    // fenêtre l'est. Ce qu'il ne décide pas est décidé ici : le bouton
    // rangé à la main reste rangé.
    let up = !HIDDEN.load(Ordering::Relaxed);
    if SHOWN.swap(up, Ordering::Relaxed) != up {
        note(&format!(
            "bouton flottant {}",
            if up { "montré" } else { "retiré" }
        ));
    }
    crate::logo::shown_now(up);
    if !up {
        crate::menu::montre(false);
    }
}

/// Ce que le bouton montrait la dernière fois qu'on l'a décidé.
#[cfg(windows)]
static SHOWN: AtomicBool = AtomicBool::new(false);

#[cfg(not(windows))]
fn put_the_button(_picture: (i32, i32, i32, i32), _anchor: (i32, i32)) {}

/// The smallest picture the button still fits in, in real pixels.
///
/// What a session window may not be resized below: the button is the one
/// thing of ours left on top of the picture, and a picture it cannot hang
/// on is a session with no way out but the keyboard.
///
/// Nothing when there is no button, which is every moment there is no
/// session to hold to a shape either.
#[cfg(windows)]
pub fn room_for_the_button() -> Option<(i32, i32)> {
    // Le logo et non la carte du menu : une image n'est pas trop petite
    // pour un bouton parce qu'un menu fermé n'y tiendrait pas.
    if crate::logo::its_window() == 0 {
        return None;
    }
    let (wide, high) = logo();
    let margin = margin();
    Some((wide + margin, high + margin))
}

#[cfg(not(windows))]
pub fn room_for_the_button() -> Option<(i32, i32)> {
    None
}

/// Whether the primary mouse button is down right now.
///
/// Asked of the system rather than waited for as an event: the window
/// this is dragging is too small to keep the mouse inside it, and a
/// release that happened over the picture is a release all the same.
///
/// Primary, not left. The logo starts this on the button the person
/// clicks with, and for a left-handed mouse that is the physical right
/// one; the raw key state names physical buttons and ignores the swap,
/// so read as « left » it answered « up » the whole drag, and the button
/// could never be moved by anyone with a swapped mouse.
#[cfg(windows)]
fn held_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};

    // SAFETY: no argument beyond the metric asked for.
    let primary = if unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0 {
        VK_RBUTTON
    } else {
        VK_LBUTTON
    };
    // SAFETY: no argument, and the answer is a plain bit field.
    let state = unsafe { GetAsyncKeyState(i32::from(primary)) };
    state as u16 & 0x8000 != 0
}

#[cfg(not(windows))]
fn cursor_now() -> Option<(i32, i32)> {
    None
}

#[cfg(not(windows))]
fn held_down() -> bool {
    false
}

/// Where the mouse is on the screen, in real pixels.
#[cfg(windows)]
fn cursor_now() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: the slot is ours, and a refusal is one of the answers.
    if unsafe { GetCursorPos(&mut point) } != 0 {
        Some((point.x, point.y))
    } else {
        None
    }
}

/// Which of a player's windows is being looked for.
///
/// It has more than one, and the answer changes with the moment: before
/// the picture is taken in hand it still carries the engine's title, and
/// afterwards it carries nothing at all, that title having gone with the
/// frame it was written on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Looked {
    /// The one the engine has just opened, still its own.
    ///
    /// Recognised by its title, which our own rebranding put there
    /// (patch P-M2, `patches/MANIFEST.md`) and which no other window of
    /// that process carries. Nothing weaker will do: the engine opens
    /// other windows, one of them larger than the picture, and taking
    /// the biggest one meant laying an empty window inside ours and
    /// leaving the picture standing beside it.
    ///
    /// On screen or not, because it is born hidden and only shown once
    /// everything about it is settled. That is the whole point: taken in
    /// hand while still hidden, it is never seen anywhere but where it
    /// belongs.
    Fresh,
    /// The one already laid inside our window.
    ///
    /// Recognised by being the biggest on screen, which it is by then:
    /// it fills our window, and it is the only one of that process the
    /// system shows at all.
    Taken,
}

/// The mark our own rebranding leaves on the engine's picture window.
#[cfg(windows)]
const TITLED: &str = " - ZyrDesk";

/// That player's picture window, and where it sits.
#[cfg(windows)]
fn window_and_place_of(
    process: u32,
    looked: Looked,
) -> Option<(
    windows_sys::Win32::Foundation::HWND,
    windows_sys::Win32::Foundation::RECT,
)> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };
    use windows_sys::core::BOOL;

    // The one already laid inside our window is not looked for: it is
    // held, by the very part of the program that laid it, which weighs
    // the number it holds before answering. And it could not be found by
    // looking any more: a window taken into ours is no longer one of the
    // system's top-level windows, and a top-level window is all an
    // enumeration walks. Looked for all the same, it was not found, and
    // what depends on finding it stopped: the floating button was never
    // put up at all, and the engine's own shortcuts were refused on the
    // grounds that the session was not in front.
    if matches!(looked, Looked::Taken) {
        let window = crate::picture::the_engines_window()?;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a window this program holds, and the rectangle is
        // ours. It answers in screen coordinates whether the window is
        // one of the system's own or one of ours, which is what every
        // caller wants.
        return (unsafe { GetWindowRect(window, &mut rect) } != 0).then_some((window, rect));
    }

    struct Looking {
        process: u32,
        looked: Looked,
        widest: i64,
        found: Option<(HWND, RECT)>,
    }

    unsafe extern "system" fn consider(window: HWND, carried: LPARAM) -> BOOL {
        // SAFETY: the pointer is the one handed to EnumWindows just
        // below, and lives for the whole of the call.
        let looking = unsafe { &mut *(carried as *mut Looking) };

        let mut owner = 0u32;
        // SAFETY: the window comes from the enumeration and the slot is
        // ours.
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        if owner != looking.process {
            return TRUE;
        }
        match looking.looked {
            // SAFETY: same window.
            Looked::Taken if unsafe { IsWindowVisible(window) } == 0 => return TRUE,
            Looked::Fresh if !titled(window) => return TRUE,
            _ => {}
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: same window, and the rectangle is ours.
        if unsafe { GetWindowRect(window, &mut rect) } != 0 {
            let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
            if area > looking.widest {
                looking.widest = area;
                looking.found = Some((window, rect));
            }
        }
        TRUE
    }

    /// Whether that window carries the engine's own title.
    ///
    /// Asked of another program's window, which the system answers from
    /// the caption it is drawing rather than by asking that program: it
    /// therefore only answers while the window still has a caption, which
    /// is exactly as long as it is still the engine's.
    fn titled(window: HWND) -> bool {
        let mut written = [0u16; 256];
        // SAFETY: the window comes from the enumeration and the buffer is
        // ours, of the length the call is told.
        let taken = unsafe { GetWindowTextW(window, written.as_mut_ptr(), written.len() as i32) };
        if taken <= 0 {
            return false;
        }
        String::from_utf16_lossy(&written[..taken as usize]).ends_with(TITLED)
    }

    let mut looking = Looking {
        process,
        looked,
        widest: 0,
        found: None,
    };
    // SAFETY: the callback above is what reads the pointer, and the
    // enumeration is over before this function returns.
    unsafe { EnumWindows(Some(consider), &mut looking as *mut Looking as LPARAM) };
    looking.found
}

/// That player's picture window, for whoever else needs to reach it.
#[cfg(windows)]
pub fn window_of(process: u32, looked: Looked) -> Option<windows_sys::Win32::Foundation::HWND> {
    window_and_place_of(process, looked).map(|(window, _)| window)
}

/// Where that player's picture is, as left, top, right and bottom in
/// real pixels.
#[cfg(windows)]
fn picture_of(process: u32) -> Option<(i32, i32, i32, i32)> {
    window_and_place_of(process, Looked::Taken)
        .map(|(_, rect)| (rect.left, rect.top, rect.right, rect.bottom))
        .filter(|(left, top, right, bottom)| right > left && bottom > top)
}

#[cfg(not(windows))]
fn picture_of(_process: u32) -> Option<(i32, i32, i32, i32)> {
    None
}

/// Types the engine's shortcut, at the session and nowhere else.
///
/// Which is `hand_over_and_type`'s doing, and the only reason this may
/// type at all: the keyboard was given to the picture and seen to land
/// there one call earlier, on this same thread.
#[cfg(windows)]
fn shortcut(act: Act, process: u32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};

    let (Some(letter), Some(key)) = (act.letter(), act.where_it_sits()) else {
        return Ok(());
    };

    // Whether anything will actually receive this. `SendInput` reports
    // success whatever happens downstream: a combination another program
    // has claimed as its own global shortcut swallows the injected keys
    // before they ever reach the session, and nothing about that shows up
    // as a failure here. Checked by claiming it ourselves and handing it
    // straight back, the one way this can be known at all.
    if already_claimed(letter) {
        let combo = format!("Ctrl+Alt+Maj+{}", char::from(letter));
        note(&format!(
            "{act} refusé : {combo} est déjà pris par un autre programme"
        ));
        return Err(format!(
            "{combo} est déjà utilisé par un autre programme sur cet ordinateur.\n  \
             Fermez-le, ou changez son raccourci, puis réessayez."
        ));
    }

    // A modifier a finger is already holding is neither pressed nor
    // released here. Releasing it for them leaves the system certain
    // that finger has gone while it has not, and the next shortcut typed
    // without lifting it is read as the bare key: it does nothing at
    // all. This is typed in answer to a shortcut the person has just
    // typed, so the finger in question is very often still down, and
    // that is a shortcut which works or not depending on whether they
    // let go in between. The engine reads the whole combination either
    // way: what it does not get from us, it already has.
    let to_press: Vec<Key> = [
        (place::CTRL, VK_CONTROL),
        (place::ALT, VK_MENU),
        (place::SHIFT, VK_SHIFT),
    ]
    .into_iter()
    .filter(|(_, named)| !a_finger_holds(*named))
    .map(|(place, _)| Key::Place(place))
    .collect();

    // Pressed in order, released in the mirror order: no key is left
    // down that was not down before.
    let mut keys: Vec<(Key, bool)> = to_press.iter().map(|place| (*place, false)).collect();
    keys.push((Key::Place(key), false));
    keys.push((Key::Place(key), true));
    keys.extend(to_press.iter().rev().map(|place| (*place, true)));

    if typed(&keys) {
        // Said out loud because nothing else can say it: if the picture
        // does not react, this line is what tells a keystroke that never
        // left from one the engine chose to ignore.
        note(&format!(
            "{act} envoyé au lecteur {process} : Ctrl+Alt+Maj+{}, à la place {key:#04x}",
            char::from(letter)
        ));
        Ok(())
    } else {
        note(&format!(
            "{act} refusé par Windows pour le lecteur {process}"
        ));
        Err("Windows a refusé la combinaison de touches".to_string())
    }
}

/// Where the keys this program types sit, in the numbering of places
/// rather than of names.
///
/// The place is what is sent and the name is left for the far end to work
/// out from its own keyboard: the key engraved A in France is the key
/// engraved Q elsewhere, and a name sent instead leaves the place to
/// whatever the far system thinks the keyboard is.
#[cfg(windows)]
mod place {
    pub const CTRL: u16 = 0x1D;
    pub const ALT: u16 = 0x38;
    pub const SHIFT: u16 = 0x2A;
    pub const TAB: u16 = 0x0F;
}

/// A key as it is typed: by its place, or by its name.
///
/// Almost everything goes by its place, for the reason above. A key that
/// has no engraving anywhere, like the one that plays and pauses, is the
/// exception: it means the same thing on every keyboard in the world, its
/// place moves from one laptop to the next, and its name is the only
/// thing about it that is fixed.
#[derive(Clone, Copy)]
enum Key {
    Place(u16),
    Named(u16),
}

/// Types those keys, in that order, and says whether the system took them
/// all.
///
/// Each pair is a key and whether it is going up. Shared by everything
/// this program types: what it hands the system is the same either way,
/// and two copies of it would be two chances to send a keystroke slightly
/// differently.
#[cfg(windows)]
fn typed(keys: &[(Key, bool)]) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, SendInput,
    };

    let events: Vec<INPUT> = keys
        .iter()
        .map(|(key, up)| {
            let (name, place, how) = match key {
                Key::Place(place) => (0, *place, KEYEVENTF_SCANCODE),
                Key::Named(name) => (*name, 0, 0),
            };
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: name,
                        wScan: place,
                        dwFlags: how | if *up { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        })
        .collect();

    // SAFETY: the events are ours and well formed, and their size is the
    // one the call is told to expect.
    let sent = unsafe {
        SendInput(
            events.len() as u32,
            events.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    sent as usize == events.len()
}

#[cfg(not(windows))]
fn typed(_keys: &[(Key, bool)]) -> bool {
    false
}

/// Presses the key that plays and pauses.
///
/// It travels the same road as Alt+Tab and for the same reason: Windows
/// keeps it for itself here, handing it to whatever is playing on this
/// computer, and the engine's own hook is the one thing that ever takes
/// it back for the session. So this needs what that needs, the keyboard
/// belonging to the picture.
#[cfg(windows)]
fn play_or_pause() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE;

    let key = Key::Named(VK_MEDIA_PLAY_PAUSE);
    typed(&[(key, false), (key, true)])
}

#[cfg(not(windows))]
fn play_or_pause() -> bool {
    false
}

/// Types Alt+Tab, or Alt+Maj+Tab to go the other way.
///
/// Typed here rather than sent anywhere, and that is the whole of how it
/// crosses: Windows keeps Alt+Tab for itself on this computer, and the
/// one thing that ever takes it back is the engine's own hook, which
/// steps in front of every keystroke of the machine while the picture has
/// the keyboard. What it takes, it hands to the far computer. So this is
/// the same road a hand's own Alt+Tab takes, and it needs the same thing
/// of the session: the keyboard has to belong to it.
#[cfg(windows)]
fn the_window_after(back: bool) -> bool {
    let (alt, shift, tab) = (
        Key::Place(place::ALT),
        Key::Place(place::SHIFT),
        Key::Place(place::TAB),
    );
    let mut keys = vec![(alt, false)];
    if back {
        keys.push((shift, false));
    }
    keys.push((tab, false));
    keys.push((tab, true));
    if back {
        keys.push((shift, true));
    }
    keys.push((alt, true));
    typed(&keys)
}

#[cfg(not(windows))]
fn the_window_after(_back: bool) -> bool {
    false
}

/// Clicks the middle button where the pointer stands.
///
/// Nothing about the session is asked here, on purpose. A middle click is
/// a click: it lands on whatever window the pointer is over, which is the
/// picture when the hand is on it and one of this computer's own windows
/// when it is not. That is what the same click made by a mouse does, and
/// a gesture that behaved differently would be a gesture nobody could
/// aim.
#[cfg(windows)]
fn a_middle_click() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEINPUT,
        SendInput,
    };

    let click = |what| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                // Where the pointer already is: nothing here moves it.
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: what,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let events = [click(MOUSEEVENTF_MIDDLEDOWN), click(MOUSEEVENTF_MIDDLEUP)];
    // SAFETY: the events are ours and well formed, and their size is the
    // one the call is told to expect.
    let sent = unsafe {
        SendInput(
            events.len() as u32,
            events.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    sent as usize == events.len()
}

#[cfg(not(windows))]
fn a_middle_click() -> bool {
    false
}

/// Whether a finger is on that key at this instant.
///
/// Asked of the keyboard itself and not of this thread's own reading of
/// it: this runs nowhere near the window that has the keys, and what
/// that window has been told is neither here nor now.
#[cfg(windows)]
fn a_finger_holds(named: windows_sys::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    // SAFETY: a named key, which is all this call reads.
    unsafe { GetAsyncKeyState(named as i32) < 0 }
}

/// Whether Ctrl+Alt+Shift+`letter` is already claimed by something else
/// on this computer, found out by claiming it here and handing it
/// straight back.
///
/// The one way this can be known at all. Windows does not say who holds a
/// combination, only whether a new claim on it succeeds, so this is
/// answered the same way the question would be asked of Windows itself: a
/// hotkey of our own, on a thread of our own, id chosen so nothing else in
/// this program is asking for it at the same time. A refusal can then only
/// mean something outside this program got there first: Sunshine and
/// Moonlight answer to nobody's global shortcuts, and this program's own
/// are registered on a thread of their own from a fixed, different set of
/// keys (`shortcuts.rs`).
///
/// Given back at once when it is won: keeping it would be claiming, for
/// the rest of the program's life, a combination that is none of its
/// business the moment this question is answered, and would itself then
/// swallow it from whoever asked for it first.
#[cfg(windows)]
fn already_claimed(letter: u8) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
    };

    const PROBE: i32 = 0x2A11;
    // The virtual-key code of an ordinary letter is its own ASCII value.
    let vk = u32::from(letter);
    // SAFETY: a thread-owned hotkey, id and all, claimed and given back
    // within this one call; no window is named.
    let refused = unsafe {
        RegisterHotKey(
            std::ptr::null_mut(),
            PROBE,
            MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
            vk,
        )
    } == 0;
    if !refused {
        // SAFETY: the same id and the same thread that just claimed it.
        unsafe { UnregisterHotKey(std::ptr::null_mut(), PROBE) };
    }
    refused
}

#[cfg(not(windows))]
fn shortcut(_act: Act, _process: u32) -> Result<(), String> {
    Err("les sessions ne tournent que sous Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_entry_names_a_shortcut_the_engine_answers_to() {
        // Les lettres sont celles du moteur client : les changer sans le
        // moteur ferait taper une combinaison qui ne fait rien, ou pire,
        // une autre que celle voulue.
        // Et les places sont celles d'un clavier, indépendantes de ce
        // qui est gravé dessus : c'est par là que le moteur reconnaît
        // une touche en premier.
        for (act, letter, place) in [
            (Act::Stats, b'S', 0x1Fu16),
            (Act::MouseMode, b'M', 0x32),
            (Act::SystemKeys, b'K', 0x25),
            (Act::PointerLock, b'L', 0x26),
        ] {
            assert_eq!(act.letter(), Some(letter), "sur « {act} »");
            assert_eq!(act.where_it_sits(), Some(place), "sur « {act} »");
        }
        // Les autres ne passent pas par le clavier du lecteur : terminer
        // se demande à l'ordinateur d'en face à travers le tunnel,
        // couvrir l'écran se fait à notre propre fenêtre, celle du moteur
        // étant posée dedans, Ctrl+Alt+Suppr est la combinaison que
        // Windows garde pour lui aux deux bouts, le son se coupe dans le
        // mélangeur de cet ordinateur-ci, et le pavé tactile est lu par
        // ce programme, aucun moteur ne sachant qu'il en existe un.
        for act in [
            Act::End,
            Act::Fullscreen,
            Act::SecureAttention,
            Act::LockScreen,
            Act::Sound,
            Act::Touchpad,
        ] {
            assert_eq!(act.letter(), None, "sur « {act} »");
            assert_eq!(act.where_it_sits(), None, "sur « {act} »");
        }
    }

    #[test]
    fn a_picture_smaller_than_the_button_is_answered_and_not_refused() {
        // Ceci tourne dans l'appel du système à notre fenêtre : une
        // panique y emporte tout le programme. Une fenêtre peut changer
        // de taille sans qu'une main l'ait demandé, donc le cas doit
        // avoir une réponse.
        let bouton = (91, 91);
        for image in [
            (100, 100, 160, 134),
            (0, 0, 1, 1),
            (500, 500, 500, 500),
            (-50, -50, 10, 10),
        ] {
            let ou = hung_from(image, (0, 0), bouton, MARGIN);
            assert!(ou.0 >= image.0 && ou.0 <= image.2, "sur {image:?} : {ou:?}");
            assert!(ou.1 >= image.1, "sur {image:?} : {ou:?}");
        }
    }

    #[test]
    fn the_button_hangs_in_the_top_right_corner_of_the_picture() {
        let image = (100, 200, 1_000, 800);
        let ou = hung_from(image, (0, 0), (91, 91), MARGIN);
        assert_eq!(ou, (1_000 - MARGIN, 200 + MARGIN));
    }

    #[test]
    fn a_button_dragged_past_an_edge_comes_back_against_it() {
        let image = (100, 200, 1_000, 800);
        let loin = hung_from(image, (5_000, 5_000), (91, 91), MARGIN);
        assert_eq!(loin, (1_000, 800 - 91));
        let avant = hung_from(image, (-5_000, -5_000), (91, 91), MARGIN);
        assert_eq!(avant, (100 + 91, 200));
    }

    #[test]
    fn a_menu_with_no_room_below_nor_above_opens_beside_the_button() {
        let image = (0, 0, 1_920, 1_080);
        let bouton = 91;
        ITS_LOGO.store(bouton, Ordering::Relaxed);
        let haute = 700;
        // Le bouton en haut : la carte tient dessous, où elle se lit
        // depuis lui.
        assert_eq!(ou_s_ouvre(image, (1_904, 16), haute), Sens::Bas);
        // En bas : elle ne tient plus que dessus.
        assert_eq!(ou_s_ouvre(image, (1_904, 1_000), haute), Sens::Haut);
        // À mi-hauteur : ni l'un ni l'autre, et c'est là qu'elle était
        // coupée par le bas de l'image.
        assert_eq!(ou_s_ouvre(image, (1_904, 500), haute), Sens::Cote);
        // Une image trop courte pour elle de toute façon : à côté elle
        // serait coupée aussi, donc elle garde le côté où il reste le
        // plus de place.
        let courte = (0, 0, 1_920, 600);
        assert_eq!(ou_s_ouvre(courte, (1_904, 300), haute), Sens::Haut);
        assert_eq!(ou_s_ouvre(courte, (1_904, 100), haute), Sens::Bas);
    }

    #[test]
    fn a_menu_with_no_room_on_the_left_opens_on_the_right_of_the_button() {
        let image = (0, 0, 1_920, 1_080);
        ITS_LOGO.store(91, Ordering::Relaxed);
        let large = 320;
        // Le bouton près du bord droit de l'image : la carte tient à sa
        // gauche, où elle s'ouvre d'habitude.
        assert!(!ou_s_ouvre_a_droite(image, (1_904, 16), large));
        // Près du bord gauche : elle n'y tient plus, et tient à sa
        // droite.
        assert!(ou_s_ouvre_a_droite(image, (16, 16), large));
        // Une image trop étroite pour elle des deux côtés : elle garde
        // le côté où il reste le plus de place, pour la même raison que
        // le sens vertical.
        let etroite = (0, 0, 200, 1_080);
        assert!(!ou_s_ouvre_a_droite(etroite, (150, 16), large));
        assert!(ou_s_ouvre_a_droite(etroite, (50, 16), large));
    }

    #[test]
    fn there_is_one_way_to_end_a_session_and_not_two() {
        // Les moteurs en offrent deux : partir en laissant le bureau
        // distant ouvert, et le rendre. Porter cette différence jusqu'à
        // la personne lui laisserait une session ni en cours ni finie.
        // Une seule ligne du menu la termine, et elle porte un seul acte.
        assert_eq!(Act::End.to_string(), "fin de la session");
    }
}
