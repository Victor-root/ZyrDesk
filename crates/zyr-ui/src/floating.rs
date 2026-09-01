//! The floating button of a session.
//!
//! During a session the picture fills the window and belongs to the
//! engine. This is the one thing of ours left on top of it: a small
//! button, hanging in a corner, that opens what can be done without
//! leaving the picture.
//!
//! It is a window of our own rather than something drawn inside the
//! picture. Drawing inside would mean teaching the engine what ZyrDesk
//! is, which is exactly what the engines are kept from knowing; and a
//! window of ours can be hit by the mouse without the engine having to
//! hand it over.
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
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

// What the button did goes into the same journal as everything else: the
// window has nowhere else to say it, standing behind the picture, and a
// menu entry that seems to do nothing is exactly the kind of thing that
// cannot be diagnosed from a screenshot.
use crate::journal::note;

/// Name this window is known by, inside the program.
pub const WINDOW: &str = "flottant";

/// Name the page listens on to be told to show its menu.
const TOGGLE: &str = "floating-toggle";

/// Name the page listens on to be told a new session begins: the menu
/// closes, the last session's refusal goes, the window shrinks back to
/// the button.
const RESET: &str = "floating-reset";

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

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
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

/// How many turns of the watch the button's page is given to say what it
/// draws before the button is called stillborn.
const SPEAKS_WITHIN: u32 = 3;

/// How many times one session will put it up again before leaving it be.
///
/// Bounded on purpose. A button that cannot be drawn at all is a fault
/// worth one line in the journal and not one a second, and a session
/// without its button is still a session.
const TRIES: u32 = 3;

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
    /// Whether the pointer is kept inside the picture.
    PointerLock,
    End,
}

impl Act {
    fn read(name: &str) -> Option<Self> {
        match name {
            "fullscreen" => Some(Act::Fullscreen),
            "stats" => Some(Act::Stats),
            "mouse" => Some(Act::MouseMode),
            "cad" => Some(Act::SecureAttention),
            "lock" => Some(Act::LockScreen),
            "sound" => Some(Act::Sound),
            "keys" => Some(Act::SystemKeys),
            "pointer" => Some(Act::PointerLock),
            "end" => Some(Act::End),
            _ => None,
        }
    }

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
            Act::Fullscreen | Act::SecureAttention | Act::LockScreen | Act::Sound | Act::End => {
                None
            }
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
            Act::Fullscreen | Act::SecureAttention | Act::LockScreen | Act::Sound | Act::End => {
                None
            }
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
}

/// What this window knows of a session it started, before the service
/// believes it.
struct Expected {
    process: u32,
    /// The far computer, as the person named it.
    towards: String,
    /// Where the tunnel puts it on this machine.
    at: String,
}

/// Everything the button's place is worked out from, kept where the
/// system's own call into our window can reach it.
///
/// That call happens on every step of a drag, and it cannot wait for
/// anything: no lock, no message to another thread, no toolkit. Three
/// numbers do it, so three numbers are what is kept.
///
/// `NUDGE` is where the person dragged the button last, as a distance
/// from the corner of the picture rather than a place on screen: a
/// session opened later on another screen, or at another size, then finds
/// it where it was left rather than off the edge. It outlives every
/// session, on purpose.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static NUDGE: AtomicI64 = AtomicI64::new(0);

/// Whether the menu opens upward, the picture having no room below.
///
/// The window is as tall as the menu for the whole session and hangs by
/// the logo, which sits in one of its corners. Hung by the top, a button
/// dragged low leaves the menu running off the bottom of the picture,
/// where it is simply cut off. Hung by the bottom, the menu grows into
/// the room that is there.
///
/// Decided here and not in the page: the page has no idea where on a
/// screen it has been put. It is answered back to the page each time the
/// page says what it draws, which is the one conversation the two have
/// about this window's shape.
static UPWARD: AtomicBool = AtomicBool::new(false);

/// Which way round the drawing now on screen was measured.
///
/// Not the same thing as `UPWARD`, and keeping the two apart is the whole
/// of what makes the turn seamless. `UPWARD` is what this program would
/// rather have; this is what the page has actually drawn, which it says
/// each time it says what it draws. Everything about where this window
/// sits and what is cut out of it follows this one, because those two
/// things describe a drawing and a drawing has only ever been made one
/// way round.
///
/// They differ for exactly as long as it takes the page to hear the
/// answer and lay itself out again. Read from the wrong one, a window
/// hung by its bottom while its page still draws from the top puts the
/// logo a whole menu's height away from the hand holding it, and cuts a
/// hole where the page paints nothing, which the system fills with the
/// window's own ground. Both were seen: a button that stopped following
/// the cursor mid-drag, and a cross left standing over the picture.
static DRAWN_UPWARD: AtomicBool = AtomicBool::new(false);

/// The top and bottom of the picture, as the button was last laid on it.
///
/// Kept because the direction has to be decided again the moment the
/// window changes height, and that happens between two turns of the
/// watch: a line appearing in the menu is a taller window, and a taller
/// window may no longer fit below.
static PICTURE: AtomicI64 = AtomicI64::new(0);

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

/// Set once the page has measured itself and said what it draws.
///
/// Nothing is shown before then: a window that has said nothing yet is
/// the size of a logo with no logo drawn in it.
static READY: AtomicBool = AtomicBool::new(false);

/// Turns of the watch the button has stood there without that ever
/// happening, and how many times this session may still put it up again.
///
/// Both exist for the same reason. Waiting for the page to speak is
/// right; waiting for ever is not. A page that never speaks at all,
/// because it never loaded or because the view carrying it did not
/// survive the computer being asleep, leaves a window that is invisible
/// for the whole session and deaf to the shortcut meant to bring it back,
/// with nothing anywhere saying so. That is a whole session with no way
/// out of the picture but the keyboard.
static SILENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static TRIES_LEFT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(TRIES);

/// Set while the person has hidden the button from its own menu.
///
/// A choice they made stands until they ask for the button back. ZyrDesk
/// being minimised and restored is not that ask, and the system would
/// otherwise put the button back up with the window.
static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Set while the button's menu is open.
///
/// Kept for one reason: the keyboard. Clicking anywhere in this window
/// hands the focus to its own web view, which is right while a menu is
/// being read and wrong the moment it closes, and the system never
/// notices either way since this window is never the active one. So the
/// picture is given the keyboard back as soon as this goes down, and
/// left alone while it is up.
static MENU_UP: AtomicBool = AtomicBool::new(false);

/// The logo's own size, as a square.
fn logo() -> (i32, i32) {
    let side = ITS_LOGO.load(Ordering::Relaxed).max(1);
    (side, side)
}

/// Whether the button is to be shown or hidden right now, as the system
/// spells it.
///
/// Two questions only, and neither is about the front. The button belongs
/// to ZyrDesk's window, and the system already knows what to do with a
/// window that belongs to another: it keeps it above that one, takes it
/// down when that one goes down, and lets anything switched to cover the
/// pair. Nothing here has to say any of that.
///
/// It used to say it, and wrongly. The button was drawn above every
/// window on the machine, which was what it took back when the picture
/// was a window of the engine's own, sitting beside ours rather than
/// inside it. Above everything, it had to be hidden the moment the front
/// went elsewhere or it would hang in a corner over somebody's work. The
/// picture has been carried inside our own window since, and the reason
/// went with it; the hiding stayed, and cost a session on a second screen
/// its button every time the person looked at the first one.
#[cfg(windows)]
fn how_it_shows() -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_HIDEWINDOW, SWP_SHOWWINDOW};

    let up = READY.load(Ordering::Relaxed) && !HIDDEN.load(Ordering::Relaxed);
    // Written down when it turns and never otherwise: this is asked at
    // every laying, which is every step of a hand dragging our window.
    //
    // Worth a line because a button taken down and put back up is the one
    // thing that can leave it wearing a shape and a drawing from either
    // side of the gap, and nothing else in this journal says it happened.
    if SHOWN.swap(up, Ordering::Relaxed) != up {
        note(&format!(
            "bouton flottant {} (masqué à la main : {}) ; il portait alors {}",
            if up { "montré" } else { "retiré" },
            HIDDEN.load(Ordering::Relaxed),
            what_it_wears()
        ));
    }
    if up { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW }
}

/// Whether the button was showing the last time that was decided.
#[cfg(windows)]
static SHOWN: AtomicBool = AtomicBool::new(false);

/// The shape the window is wearing right now, as the system holds it.
///
/// Asked of the system and not of what was last sent to it, because the
/// two parting company is the one fault worth catching: a window shown
/// wearing a shape larger than what its page draws shows its own ground
/// in the difference, and that is a white patch over the picture until
/// something cuts it again.
#[cfg(windows)]
fn what_it_wears() -> String {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::GetWindowRgnBox;

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if button.is_null() {
        return "aucune fenêtre".to_string();
    }
    let mut held = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, and the rectangle is ours.
    let taken = unsafe { GetWindowRgnBox(button, &mut held) };
    format!(
        "la découpe ({}, {}, {}, {}), sorte {taken}, dans une fenêtre {}",
        held.left,
        held.top,
        held.right,
        held.bottom,
        match its_place() {
            Some((left, top, right, bottom)) =>
                format!("{}x{} en ({left}, {top})", right - left, bottom - top),
            None => "plus là".to_string(),
        }
    )
}

#[cfg(not(windows))]
fn how_it_shows() -> u32 {
    0
}

/// Whether a window that tall, hung at that corner, is better off
/// growing upward.
///
/// Only ever when it would actually help: a picture too short for the
/// menu either way keeps it below, where at least the top of it is the
/// part that shows.
fn opens_upward(picture: (i32, i32, i32, i32), anchor: (i32, i32), height: i32) -> bool {
    let below = picture.3 - anchor.1;
    if height <= below {
        return false;
    }
    let above = anchor.1 + logo().1 - picture.1;
    above > below
}

/// The picture as the button was last laid on it, top and bottom.
fn picture_now() -> (i32, i32) {
    let both = PICTURE.load(Ordering::Relaxed);
    ((both >> 32) as i32, both as i32)
}

/// Works the direction out again for a window about to be that tall, and
/// remembers it.
fn decide_the_direction(picture: (i32, i32, i32, i32), anchor: (i32, i32), height: i32) {
    PICTURE.store(
        (i64::from(picture.1) << 32) | i64::from(picture.3) & 0xFFFF_FFFF,
        Ordering::Relaxed,
    );
    UPWARD.store(opens_upward(picture, anchor, height), Ordering::Relaxed);
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
    fn closing(app: &AppHandle, asked: bool) {
        app.state::<Floating>()
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
    pub fn a_close_was_asked_for(app: &AppHandle) -> bool {
        app.state::<Floating>()
            .closing
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether the session that just ended was closed on purpose, and
    /// forgets it either way.
    pub fn was_closed_on_purpose(app: &AppHandle) -> bool {
        app.state::<Floating>()
            .closing
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}

/// Whether a session is running right now.
///
/// Read from what the button hangs on rather than asked of the service:
/// it is the same answer, it is already kept up to date every second, and
/// it costs nothing to whoever asks.
pub fn a_session_is_up(app: &AppHandle) -> bool {
    app.state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .is_some()
}

/// Says which player this window has just started, before anybody else
/// knows, and where the session it shows can be ended.
pub fn expect(app: &AppHandle, process: u32, towards: &str, at: &str) {
    *app.state::<Floating>()
        .expected
        .lock()
        .expect("session attendue") = Some(Expected {
        process,
        towards: towards.to_string(),
        at: at.to_string(),
    });
}

/// Forgets it, the session being over one way or another.
pub fn expect_nothing(app: &AppHandle) {
    *app.state::<Floating>()
        .expected
        .lock()
        .expect("session attendue") = None;
}

/// The player the button belongs to right now.
///
/// The service first: it knows every session on this computer, including
/// those another window opened. Failing that, the one this window has
/// just started, for as long as it has a picture up. That second answer
/// is what puts the button on screen with the picture rather than
/// several seconds behind it.
pub async fn player(app: &AppHandle) -> Option<u32> {
    if let Some(session) = crate::session::sessions().await.into_iter().next() {
        return Some(session.process);
    }
    let expected = app
        .state::<Floating>()
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
    expected.filter(|process| still_running(*process))
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
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(LOOK).await;
            match player(&app).await {
                Some(process) => {
                    // The picture first: the button hangs from the corner
                    // of it, and a corner read before the picture has
                    // been laid in our window is the wrong corner.
                    crate::picture::hold(&app, process);
                    if adopt(&app, process) {
                        // A session just adopted starts on the two sides
                        // its settings asked for; every toggle after that
                        // goes through this window and is counted as it
                        // is sent.
                        let preferred = crate::settings::preferred().await;
                        let state = app.state::<Floating>();
                        state
                            .game_mouse
                            .store(!preferred.absolute_mouse, Ordering::Relaxed);
                        state
                            .system_keys
                            .store(preferred.system_keys, Ordering::Relaxed);
                        state.pointer_held.store(false, Ordering::Relaxed);
                    }
                    put_the_button_up(&app, process);
                    keep_the_pointer_in_step(&app, process).await;
                    // And the keyboard belongs to the picture whenever
                    // the menu is not being read. The button's own page
                    // says so as it opens and closes, which covers every
                    // ordinary road; this is the net under the others,
                    // for a page reloaded or a window taken down with a
                    // menu still up. It costs a focus asked for again,
                    // and the system refuses it outright while another
                    // program is in front.
                    if !MENU_UP.load(Ordering::Relaxed) {
                        crate::picture::the_keyboard_back(&app);
                    }
                }
                None => {
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
fn adopt(app: &AppHandle, process: u32) -> bool {
    let state = app.state::<Floating>();
    let already = state
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .is_some_and(|seen| *seen == process);
    if already {
        return false;
    }
    // A new session starts with the button on screen and its menu shut,
    // whatever was done with the one before, and with its whole allowance
    // of tries at being drawn at all.
    HIDDEN.store(false, Ordering::Relaxed);
    MENU_UP.store(false, Ordering::Relaxed);
    SILENT.store(0, Ordering::Relaxed);
    TRIES_LEFT.store(TRIES, Ordering::Relaxed);
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
fn put_the_button_up(app: &AppHandle, process: u32) {
    let Some(picture) = picture_of(process) else {
        return;
    };
    // A button over a window that is not on screen would be the only
    // thing showing, hanging in a corner over somebody else's work. It
    // goes up when the window does, which the watch sees a second later.
    // Minimised counts as not on screen and has to be asked for
    // separately: a window down in the taskbar still calls itself
    // visible.
    let Some(home) = app.get_webview_window(crate::HOME) else {
        return;
    };
    if !home.is_visible().unwrap_or(false) || home.is_minimized().unwrap_or(false) {
        return;
    }

    // Already up: it only has to be put where the picture is now, which
    // is what laying it does.
    if let Some(window) = app.get_webview_window(WINDOW) {
        if ITS_WINDOW.load(Ordering::Relaxed) == 0 {
            // Coming back from a session that ended in a way we did not
            // see. Taken hold of again, since everything that places this
            // button reaches it by that one number; and told to start
            // over, because the page still shows whatever the last
            // session left on it, an open menu most of all, which is a
            // sheet of nothing laid over the picture that swallows every
            // click meant for it.
            remember_the_button(&window);
            let _ = window.emit(RESET, ());
            let _ = window.show();
        }
        if READY.load(Ordering::Relaxed) {
            SILENT.store(0, Ordering::Relaxed);
            lay_the_button(picture);
        } else if stillborn() {
            // Closed and not merely hidden: what is asked for here is a
            // page that runs, and only a new window brings one. The next
            // turn of the watch finds nothing standing and builds it.
            let _ = window.close();
            ITS_WINDOW.store(0, Ordering::Relaxed);
        }
        return;
    }

    // Asked before anything is held. This runs on a thread of its own and
    // the answer comes from the one that draws: waiting for it while
    // holding what that thread may want next is how both stop for good.
    //
    // This is the logo's size, which the window only has until the page
    // has measured its menu; everything about where the button hangs
    // goes on using it afterwards.
    let size = button_size(app) as i32;
    ITS_LOGO.store(size, Ordering::Relaxed);

    // Which way this window is cut and redrawn this time round. An
    // instrument, and the whole of what it does is in `trial`.
    crate::trial::starts();

    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("bouton.html".into()))
        .title("ZyrDesk")
        // The same light or dark as the window it hangs over. Both pages
        // decide their own colours, and both offer to follow the system;
        // but « the system » is a question asked of the window, and a
        // window built here is not asked the same way as the one the
        // toolkit opens at start-up. Left alone, this one answered light
        // over a dark product.
        .theme(home.theme().ok())
        .decorations(false)
        // Asked to be transparent, and this time with the half that was
        // missing; see `let_the_alpha_through`.
        //
        // It matters for exactly one thing: the edge. The shape this
        // window is cut to is a mask with one bit per pixel and no half
        // pixels, so it cuts the page's smooth rounded corners into a
        // staircase, and what that staircase steps between is the window's
        // ground and somebody else's picture. Reported as « la bordure est
        // toute saccadée », and it cannot be answered by a better ground:
        // any opaque window has a hard edge, whatever colour it is.
        //
        // Transparent, the pixels the page draws half-covered are blended
        // against the picture underneath rather than against a ground, and
        // the edge is the one the page drew.
        //
        .transparent(true)
        // And a ground of pure black with it, which is not a contradiction
        // but the other half of how this toolkit makes a window
        // transparent. It does not ask the system for a transparent
        // window: it turns the blur behind the window on over an empty
        // region, which is the old trick whose whole rule is that **a
        // pixel painted pure black becomes fully transparent**.
        //
        // Without a ground colour the toolkit erases nothing at all, and
        // what is never erased is the window's own back buffer: memory
        // nobody cleared, holding whatever was last in it. That is the
        // white artefact this button has been showing since it became
        // transparent, and the flash on every click: opening the menu
        // resizes the window, a resize grows that buffer, and the new
        // strip of it has never been painted by anybody.
        //
        // Pure black, so it costs nothing: the ground is erased over the
        // whole window and every pixel of it is transparent. Nothing in
        // this product's own drawing is pure black, the logo's outline
        // being 9,13,22, so nothing of ours is made see-through by it.
        // An earlier attempt used that outline colour as the ground and
        // concluded the two were exclusive: it was not black enough, so
        // it stayed opaque and the button sat on a plate.
        //
        // The alpha is nought and that is read: the window layer ignores
        // it and takes the black, the web view layer honours it and stays
        // transparent.
        .background_color(tauri::window::Color(0, 0, 0, 0))
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        // Above ZyrDesk's window and no higher. It is made to belong to
        // that window a moment later, which is what keeps it over the
        // picture, brings it back up with it, and lets another program
        // switched to cover it like anything else. Above everything, as
        // it was, it had to be hidden whenever the front went elsewhere,
        // and a session watched on a second screen lost its button every
        // time the person turned to the first.
        // Never takes the picture's place: the engine keeps the keyboard
        // and the mouse, and this only catches what is clicked on it.
        .focused(false)
        // Neither its size nor its place can be given here: both are
        // counted in real pixels everywhere in this file, and a window
        // builder counts them in page pixels, which are not the same
        // number on a screen that is magnified. Built out of sight and
        // shown once put right.
        .visible(false)
        .build();

    match built {
        Ok(window) => {
            keep_out_of_the_way(app, &window);
            remember_the_button(&window);
            // Sized and placed here rather than through the toolkit. A
            // size asked of the toolkit is applied a turn of its event
            // queue later, and the placing that followed read the window
            // as it still was: the button was born the wrong size in the
            // wrong corner and only found its place once the page had
            // loaded and measured itself, which is the jump seen at the
            // start of every session.
            //
            // Not shown here either. What it is to look like is the
            // page's to say, and until it has said so this is an empty
            // sheet in the corner of the picture.
            put_the_button(
                hung_from(picture, nudge(), (size, size), margin()),
                (size, size),
                0,
            );
        }
        // A button that could not be drawn is not a reason to disturb a
        // session that is otherwise fine. It is a reason to write it
        // down: this program is built without a console, so a refusal
        // said on the error stream is said to nobody at all, and a
        // session with no button and no line about it cannot be told
        // from one where nothing was ever tried.
        Err(e) => note(&format!("le bouton flottant n'a pas pu s'ouvrir : {e}")),
    }
}

/// Whether the button standing there has gone too long without ever
/// saying what it draws, and counts one more try against this session
/// when it has.
///
/// Answered here rather than watched from outside because this is the
/// only place that comes round once a second with the button in hand.
fn stillborn() -> bool {
    if SILENT.fetch_add(1, Ordering::Relaxed) + 1 < SPEAKS_WITHIN {
        return false;
    }
    SILENT.store(0, Ordering::Relaxed);
    let left = TRIES_LEFT.load(Ordering::Relaxed);
    if left == 0 {
        return false;
    }
    TRIES_LEFT.store(left - 1, Ordering::Relaxed);
    note(&format!(
        "bouton flottant : rien de dessiné après {} s, la fenêtre est refermée et remontée ; \
         {} tentative(s) après celle-ci",
        (LOOK * SPEAKS_WITHIN).as_secs(),
        left - 1
    ));
    true
}

/// What the button comes to in real pixels, on the screen it hangs over.
///
/// Everything in this file is counted in real pixels: the picture is
/// measured with the system's own ruler, and so is the mouse. What the
/// page draws is counted in the other kind, and on a screen magnified to
/// a hundred and seventy-five per cent the same button is forty-four of
/// one and seventy-seven of the other. A window taken to be the smaller
/// of the two shows its own background all around the button.
fn button_size(app: &AppHandle) -> u32 {
    let scale = app
        .get_webview_window(crate::HOME)
        .and_then(|window| window.scale_factor().ok())
        .unwrap_or(1.0);
    (BUTTON * scale).ceil() as u32
}

/// Takes the button down.
///
/// Called by the watch when the session is no longer there, and by
/// whoever ended it the moment they know: a second of a button hanging
/// over a picture that has gone is a second too many, and the watch only
/// comes round once a second.
pub fn lower(app: &AppHandle) {
    let state = app.state::<Floating>();
    if state
        .watched
        .lock()
        .expect("session suivie")
        .take()
        .is_none()
    {
        return;
    }
    ITS_WINDOW.store(0, Ordering::Relaxed);
    READY.store(false, Ordering::Relaxed);
    // The size goes with the window that had it. Kept, it belonged to a
    // window that no longer exists, and the next session's button, being
    // the same size, changed nothing and therefore said nothing: the one
    // line that tells a button that was drawn from one that never was
    // went missing on every session but the first.
    SIZED.store(0, Ordering::Relaxed);
    // A menu that goes down with its window never says it closed, and a
    // yes left standing here would stop the keyboard ever being given
    // back.
    MENU_UP.store(false, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.close();
    }
}

/// One rounded rectangle of what the page draws, in real pixels.
///
/// `x` is counted from the window's **right** edge, and is therefore
/// never positive; `y` from its top. Those are the two edges the page is
/// glued to, and the two the window is hung by, so they are the only two
/// that stay put when it is resized. The page measures itself in the
/// window as it stands and the cut is made in the window as it becomes:
/// counted from the left, the whole drawing landed short by the
/// difference between the two, and the menu lost its right edge.
#[derive(Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    radius: i32,
    /// What the page really paints, in real pixels and not rounded to
    /// any of them: left, top, right, bottom, counted from the same two
    /// edges as the four above.
    ///
    /// Carried for the journal and for nothing else. Whether the cut
    /// clips the drawing is the one question this button keeps being
    /// asked, it is a question about fractions of a pixel, and it cannot
    /// be answered from the rounded numbers: they are what is being
    /// weighed.
    drawn: [f32; 4],
    /// The same for the corner it is rounded by, which is where the two
    /// curves come closest and where a cut bites first.
    drawn_radius: f32,
}

/// Grows the button to what the page turned out to need, keeping the
/// corner it hangs from, and cuts the window to the shape it draws.
///
/// The page measures itself rather than being told a size: the menu's
/// height depends on what is in it, and a number written twice would
/// stop matching the first time an entry is added.
///
/// The shape it sends is the same measurement carried one step further.
/// A window is a rectangle and this one is transparent, but transparent
/// is a thing the layers below the page each have to agree to, and one
/// of them does not: an opaque rectangle showed through the logo's
/// rounded corners and around the menu. Cutting the window to the shape
/// settles it wherever it comes from, since nothing at all is drawn
/// outside a shape.
///
/// Never made smaller, only ever larger, and the two halves of that rule
/// are not the same statement. A window larger than what is cut out of it
/// shows nothing extra and catches nothing extra, so keeping it is free;
/// and every change of its size makes the page lay itself out again,
/// during which nothing at all is drawn. That is the flicker, and it is
/// why the size a window has once been given is kept. The page measures
/// every submenu at once, open or not, so the size it asks for settles
/// while the menu is still shut and does not change again.
///
/// What makes keeping it safe is that the shape is counted from the
/// window's right edge rather than its left; see `Piece`. Counted from
/// the left, a window kept wider than the drawing put the cut that many
/// pixels beside it, which is what took the right-hand side off the menu.
#[tauri::command]
pub fn floating_size(
    width: u32,
    height: u32,
    shape: Vec<Piece>,
    upward: bool,
    painted: f64,
) -> Result<bool, String> {
    // Read before anything moves, and while the drawing still on screen
    // is the one this window was last placed for: what is kept is the
    // corner the button hangs from, which is the logo's own, and the logo
    // is the one part of this window nobody may see move.
    let (Some(corner), Some(was)) = (where_it_hangs(), its_place()) else {
        // Said out loud, once per run of them. The page swallows this
        // refusal and stops asking as soon as two of its frames draw the
        // same thing, so a shape refused here is a shape never applied,
        // and nothing anywhere said so: the button then wears whatever
        // it was last cut to, over a drawing that has moved on.
        if !REFUSING.swap(true, Ordering::Relaxed) {
            note("bouton flottant : forme refusée, la fenêtre n'est plus là pour être découpée");
        }
        return Err("le bouton flottant n'est plus là".to_string());
    };
    if REFUSING.swap(false, Ordering::Relaxed) {
        note("bouton flottant : la fenêtre répond à nouveau, la forme reprend");
    }
    let size = (
        (width as i32).max(was.2 - was.0),
        (height as i32).max(was.3 - was.1),
    );
    // The page has said which way round it drew what follows, so from
    // here on that is the drawing this window is cut and placed for. The
    // corner above was read a line too early to be caught by it, and that
    // is the point: the logo was there under the old drawing, and this is
    // what puts it back there under the new one.
    DRAWN_UPWARD.store(upward, Ordering::Relaxed);

    // Which way the menu would be better off opening, worked out again
    // here and not only at every turn of the watch: a line appearing in
    // the menu makes the window taller, and a taller window may no longer
    // fit below the logo. Only ever an answer to the page; nothing here
    // acts on it, and the page acting on it is what brings it back as a
    // drawing.
    let (top, bottom) = picture_now();
    if bottom > top {
        UPWARD.store(
            opens_upward((0, top, 0, bottom), corner, size.1),
            Ordering::Relaxed,
        );
    }

    // The shape, and against the size the window **has** rather than the
    // one it is about to have. Those pieces are counted from the window's
    // right edge, and the page counted them in the window as it stands:
    // cut against a width the page has not laid itself out in yet, they
    // land the whole difference to the right of what is actually painted.
    //
    // That is not reasoning, it is a photograph. Opening the menu widens
    // this window by eighteen pixels, and the picture the button takes of
    // itself at that moment shows the menu's card drawn eighteen pixels
    // to the left of the shape kept for it: a strip of window nobody has
    // painted along one edge of the card, and the logo shaved on the
    // other. That strip is the white flash, and this is where it is made.
    //
    // The page asks again on its next frame, laid out at the new width,
    // and that one is cut against the new width because the window has
    // it by then. One frame of the old shape over the old drawing is what
    // is wanted: they match.
    //
    // And only when it is a different shape. Cutting is neither free nor
    // silent: the system redraws the window on every cut, and what it
    // redraws is the window's own ground until the page paints over it.
    // The page asks for this several times a frame while a hand runs over
    // the logo, and once a second all session long for the measures,
    // nearly always with the very shape the window already wears.
    let standing = (was.2 - was.0, was.3 - was.1);
    let drawn = the_shape_of(&shape, standing, upward);
    if SHAPED.swap(drawn, Ordering::Relaxed) != drawn {
        cut_to_what_is_drawn(&shape, standing);
    }
    // The page has drawn something, so there is something to show.
    READY.store(true, Ordering::Relaxed);
    put_the_button(corner, size, how_it_shows());
    say_where_the_page_ends(standing.0, painted);
    tell_the_button(was, size, &shape);
    Ok(UPWARD.load(Ordering::Relaxed))
}

/// How far the drawing reaches from the two edges its pieces are counted
/// from, which is the width and the height a window needs to hold it all.
///
/// Opening upward the pieces are counted from the bottom, so what reaches
/// furthest is the one whose top is highest above that edge.
fn how_far_it_reaches(shape: &[Piece], upward: bool) -> (i32, i32) {
    shape.iter().fold((0, 0), |(wide, high), piece| {
        let reach = if upward {
            -piece.y
        } else {
            piece.y + piece.height
        };
        (wide.max(-piece.x), high.max(reach))
    })
}

/// The shape this window was last cut to, so an identical cut can be
/// left alone; see `floating_size`. Nought is « never cut », which is
/// every window that has just been taken hold of.
static SHAPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// That shape in one number.
///
/// The size goes in with it, and so does which way round it was drawn:
/// the pieces are counted from the window's right edge, and from its
/// bottom edge when the menu opens upward, so the same pieces on a window
/// of another size, or turned the other way, are another cut.
fn the_shape_of(shape: &[Piece], size: (i32, i32), upward: bool) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut how = std::collections::hash_map::DefaultHasher::new();
    size.hash(&mut how);
    upward.hash(&mut how);
    for piece in shape {
        (piece.x, piece.y, piece.width, piece.height, piece.radius).hash(&mut how);
    }
    // Nought is reserved for a window nobody has cut yet.
    how.finish().max(1)
}

/// Whether the last shape the page sent was refused, so a run of them
/// costs one line and not one per frame.
static REFUSING: AtomicBool = AtomicBool::new(false);

/// The last size this window was given, so a change of it can be written
/// down and nothing else.
static SIZED: AtomicI64 = AtomicI64::new(0);

/// Says what the button's window was asked to become, and what it became.
///
/// The two are the same number every time it works, and that is the point:
/// a window that refuses a size, or that a toolkit clamps behind our back,
/// cannot be told from one that was never asked. What the page draws is
/// said beside it, because a window the right size under a shape of the
/// wrong size looks exactly like a window of the wrong size.
///
/// Once per change. The logo is measured every frame while a hand runs
/// over it, and the size does not change on any of them.
fn tell_the_button(was: (i32, i32, i32, i32), size: (i32, i32), shape: &[Piece]) {
    let both = (i64::from(size.0) << 32) | i64::from(size.1) & 0xFFFF_FFFF;
    if SIZED.swap(both, Ordering::Relaxed) == both {
        return;
    }
    let drawn = how_far_it_reaches(shape, DRAWN_UPWARD.load(Ordering::Relaxed));
    let now = its_place().map(|(left, top, right, bottom)| (right - left, bottom - top));
    note(&format!(
        "bouton flottant : {}x{} demandés, {}x{} avant, {} après ; \
         {} morceaux dessinés jusqu'à {}x{}",
        size.0,
        size.1,
        was.2 - was.0,
        was.3 - was.1,
        match now {
            Some((wide, high)) => format!("{wide}x{high}"),
            None => "plus là".to_string(),
        },
        shape.len(),
        drawn.0,
        drawn.1,
    ));
}

/// The last pair said, so the line is written when it changes and not on
/// every frame the page draws.
static ENDED: AtomicI64 = AtomicI64::new(i64::MIN);

/// Says where the page's right edge falls, against the window's own.
///
/// Everything the page sends is counted from its right edge, and the core
/// lays it from the window's. They are meant to be the same edge: the
/// page's block is pinned to the right of the view, so the block's right
/// edge is the view's, and the view's is the window's. But the page
/// measures in its own kind of pixel and the window is a whole number of
/// real ones, so a window whose width is not a whole number of page
/// pixels puts the two a fraction apart. Everything drawn then lands that
/// fraction off, all the way round the drawing, which is how a half
/// painted pixel becomes an unpainted one.
///
/// The two numbers are said and nothing is concluded from them here. Held
/// against each other they also say something the core cannot ask for:
/// the page measures in the window as it stands, so a page still laid out
/// for a window it has outgrown gives its old edge, and the gap is then
/// not a fraction but hundreds of pixels. That is not a fault, a shape
/// counted from an edge stays right wherever that edge is; but it does
/// mean the pair is only worth weighing when the two are close.
fn say_where_the_page_ends(standing: i32, painted: f64) {
    let both = (i64::from(standing) << 32) | (painted * 100.0) as i64 & 0xFFFF_FFFF;
    if ENDED.swap(both, Ordering::Relaxed) == both {
        return;
    }
    note(&format!(
        "bouton flottant : la page finit à {painted:.2}, la fenêtre à {standing}, \
         soit {:.2} px d'écart",
        f64::from(standing) - painted
    ));
}

/// Says whether the button's menu is open, and gives the picture back the
/// front and the keyboard the moment it is not.
///
/// The page is the only one who knows: this window is never activated, so
/// nothing about it reaches the system, and a menu opened and closed left
/// the session deaf with no way back short of reopening it.
#[tauri::command]
pub fn floating_menu(app: AppHandle, open: bool) {
    // Written down at both ends. This is the one moment the keyboard
    // leaves the picture without the system saying so, and the journal
    // had no trace of it at all: a session gone deaf and a session never
    // touched read exactly alike.
    note(&format!(
        "menu du bouton flottant {}",
        if open { "ouvert" } else { "fermé" }
    ));
    MENU_UP.store(open, Ordering::Relaxed);
    if !open {
        crate::picture::the_session_back(&app);
    }
}

/// Hides the button until the next session.
#[tauri::command]
pub fn floating_hide(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    HIDDEN.store(true, Ordering::Relaxed);
    window.hide().map_err(|e| e.to_string())
}

/// Takes hold of the button, moves it with the mouse until it is let go,
/// and says whether the whole thing turned out to be a plain click.
///
/// The gesture is followed here and not in the page. That window is
/// forty-four pixels wide: the mouse leaves it on the first movement, and
/// what a web view reports as the place of a pointer is not always where
/// that pointer is on the screen. Where the system says the cursor is is
/// neither of those things, and it is always true.
///
/// Nothing is asked of the page while this runs, and the menu is left
/// open if it was: a window that changes size under the mouse gets away
/// from it.
#[tauri::command]
pub async fn floating_grab(app: AppHandle) -> Result<bool, String> {
    let held = *app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie");
    let (Some(process), Some(from)) = (held, where_it_hangs()) else {
        return Ok(true);
    };
    // The picture is read once: it does not move while the button is
    // being dragged over it, and looking for it again at every step
    // would mean enumerating every window on the machine a hundred times
    // a second.
    let (Some(start), Some(picture)) = (cursor_now(), picture_of(process)) else {
        return Ok(true);
    };

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
        if !moved && (now.0 - start.0).abs() < GRIP && (now.1 - start.1).abs() < GRIP {
            tokio::time::sleep(FOLLOW).await;
            continue;
        }
        moved = true;
        at = slide(picture, at, now.0 - was.0, now.1 - was.1);
        was = now;
        tokio::time::sleep(FOLLOW).await;
    }
    if moved {
        leave_it_there();
    }
    Ok(!moved)
}

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
    let Some((left, top, right, bottom)) = its_place() else {
        return from;
    };
    let logo = logo();
    let anchor = held_inside((from.0 + dx, from.1 + dy), picture, logo.0, logo.1);

    let margin = margin();
    nudged_to(
        anchor.0 - (picture.2 - margin),
        anchor.1 - (picture.1 + margin),
    );
    // A button dragged towards the bottom of the picture works out as it
    // goes that its menu would be better off above, rather than at the
    // next turn of the watch. Worked out and not applied: the turn itself
    // belongs to the page, which is not asked anything while a hand is on
    // the button, and which will ask at the next thing it draws.
    decide_the_direction(picture, anchor, bottom - top);

    // Asked of the system and not of the toolkit, like everywhere else
    // the button moves: this runs a hundred times a second under a hand,
    // and a trip through an event queue at that rhythm is what a button
    // lagging its own cursor is made of.
    put_the_button(anchor, (right - left, bottom - top), 0);
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
pub fn show_the_menu(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("aucune session en cours")?;
    // Asked for again with the menu already open is asking to be rid of
    // it. Everything below is about getting to a menu, and none of it is
    // wanted here: the session is already where it should be, and putting
    // the pointer back or showing a window that is shown would be undoing
    // what the person did between the two presses. The page is told, and
    // closing it there is the same closing as any other, which is what
    // hands the keyboard back to the picture.
    if MENU_UP.load(Ordering::Relaxed) {
        return window.emit(TOGGLE, ()).map_err(|e| e.to_string());
    }
    // The session first, when it was put away: the button hangs on the
    // picture, and a menu opened over an empty desktop, picture down in
    // the taskbar, is a button floating over somebody else's work. The
    // shortcut asks to do something with the session, so the session
    // comes back.
    if let Some(home) = app.get_webview_window(crate::HOME)
        && (!home.is_visible().unwrap_or(false) || home.is_minimized().unwrap_or(false))
    {
        crate::show_home(app);
    }
    // Asked for by name, which takes back the choice of hiding it.
    HIDDEN.store(false, Ordering::Relaxed);
    // In game mouse mode the pointer belongs entirely to the far
    // computer: it is held inside the picture and warped back to the
    // middle of it at every movement, so it cannot be brought to this
    // button at all. Asking for the menu is asking to do something, so
    // the pointer comes back first. The entry that gives it away again is
    // in that very menu.
    give_the_pointer_back(app);
    window.show().map_err(|e| e.to_string())?;
    window.emit(TOGGLE, ()).map_err(|e| e.to_string())
}

/// Which mouse the session is using right now.
///
/// True for the game one, which pins the pointer inside the picture and
/// sends how far it moved; false for the desktop one, which sends where
/// it is. The menu asks for this so it can show which of the two is on
/// instead of offering a switch that says nothing about where it stands.
///
/// What this window believes, and it believes it because every switch
/// goes through here: the entry in the menu and the shortcut this program
/// holds both end up in `ask`, which counts them. The engine's own
/// shortcut, typed straight into the picture, does not pass this way and
/// would leave the two disagreeing until the next switch from here.
#[tauri::command]
pub fn floating_mouse(app: AppHandle) -> bool {
    app.state::<Floating>().game_mouse.load(Ordering::Relaxed)
}

/// Which computer Alt+Tab, Échap and the Windows key are going to right
/// now.
///
/// True for the session, false for this computer. Believed rather than
/// asked, exactly as the mouse mode beside it, and for the same reason:
/// the keys are held in the engine and the engine never says.
#[tauri::command]
pub fn floating_keys(app: AppHandle) -> bool {
    app.state::<Floating>().system_keys.load(Ordering::Relaxed)
}

/// Asks the session for something, in its own language.
#[tauri::command]
pub async fn floating_act(app: AppHandle, what: String) -> Result<(), String> {
    let act = Act::read(&what).ok_or_else(|| format!("action inconnue : {what}"))?;
    ask(&app, act).await
}

/// The same, from anywhere in the program rather than from the menu.
pub async fn ask(app: &AppHandle, act: Act) -> Result<(), String> {
    // Ours to do, both of them, and neither goes through the engine's
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
        _ => {}
    }

    let process = the_player(app)?;

    type_at_the_picture(app, act, process).await?;
    // The keystroke left, so the engine will act on it: what this window
    // believes of the two switches follows the keystrokes it sends.
    match act {
        Act::MouseMode => {
            let _ = app
                .state::<Floating>()
                .game_mouse
                .fetch_xor(true, Ordering::Relaxed);
        }
        Act::PointerLock => {
            let _ = app
                .state::<Floating>()
                .pointer_held
                .fetch_xor(true, Ordering::Relaxed);
        }
        Act::SystemKeys => {
            let theirs = !app
                .state::<Floating>()
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
///
/// Nothing is done while the menu is open. Throwing the switch means
/// handing the keyboard to the picture, and a menu being read is the one
/// moment the keyboard is somewhere else on purpose.
async fn keep_the_pointer_in_step(app: &AppHandle, process: u32) {
    let wanted = crate::picture::on_the_whole_screen();
    let state = app.state::<Floating>();
    if wanted == state.pointer_held.load(Ordering::Relaxed) || MENU_UP.load(Ordering::Relaxed) {
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

/// The player the button is hanging on right now.
///
/// What the watch adopted, and never the first session it can find: with
/// two sessions open, what this window's menu asks for is this window's.
fn the_player(app: &AppHandle) -> Result<u32, String> {
    app.state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .copied()
        .ok_or_else(|| "aucune session en cours".to_string())
}

/// Whether the session's sound is hushed on this computer right now.
///
/// Asked of Windows rather than remembered here, unlike the mouse mode
/// next to it in the menu. The two are not the same kind of thing: the
/// mouse mode lives in the engine, which never says where it stands, so
/// this program has to count its own switches; the mute lives in the
/// volume mixer, which anybody can open and which will gladly say. A
/// switch showing what it believes rather than what is true is a switch
/// nobody trusts twice.
#[tauri::command]
pub async fn floating_sound(app: AppHandle) -> Result<bool, String> {
    let process = the_player(&app)?;
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
async fn hush_the_session(app: &AppHandle) -> Result<(), String> {
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
    tauri::async_runtime::spawn_blocking(ask)
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
async fn type_at_the_picture(app: &AppHandle, act: Act, process: u32) -> Result<(), String> {
    let (say, mut heard) = tauri::async_runtime::channel(1);
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
async fn press_ctrl_alt_del_over_there(app: &AppHandle) -> Result<(), String> {
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
async fn lock_over_there(app: &AppHandle) -> Result<(), String> {
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
async fn the_way_of_this_session(app: &AppHandle) -> Result<zyr_control::WayId, String> {
    let watched = *app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie");

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
async fn end_the_session(app: &AppHandle) -> Result<(), String> {
    let watched = *app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie");

    let mut sessions = crate::session::sessions().await;
    let ours = watched
        .and_then(|process| {
            sessions
                .iter()
                .position(|session| session.process == process)
        })
        .or_else(|| (!sessions.is_empty()).then_some(0));

    let (process, towards, at) = match ours {
        Some(place) => {
            let session = sessions.swap_remove(place);
            (session.process, session.towards, session.at)
        }
        None => {
            let expected = app
                .state::<Floating>()
                .expected
                .lock()
                .expect("session attendue")
                .as_ref()
                .map(|expected| {
                    (
                        expected.process,
                        expected.towards.clone(),
                        expected.at.clone(),
                    )
                });
            expected.ok_or("aucune session en cours")?
        }
    };
    note(&format!("fermeture demandée sur {towards} à travers {at}"));
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
    tauri::async_runtime::spawn(async move {
        let answered = tauri::async_runtime::spawn_blocking(move || {
            zyr_session::close_on_the_far_computer(&towards, &at)
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

/// Remembers the button's window, so it can be moved without asking the
/// toolkit.
///
/// A window taken hold of wears no shape yet, whatever the last one wore:
/// the first cut asked of it has to be made and not recognised as one it
/// already has.
#[cfg(windows)]
fn remember_the_button(window: &tauri::WebviewWindow) {
    if let Ok(handle) = window.hwnd() {
        ITS_WINDOW.store(handle.0 as isize, Ordering::Relaxed);
        SHAPED.store(0, Ordering::Relaxed);
    }
}

/// Lays the button on the corner of the picture it hangs from.
///
/// Called every time the picture is laid, which is every step of a drag
/// of our window. So: no lock, no message to another thread, nothing of
/// the toolkit. Asking the toolkit where a window is and putting it
/// somewhere else are two trips through its message queue, and doing
/// that a hundred times a second is what made resizing a session judder.
#[cfg(windows)]
pub fn lay_the_button(picture: (i32, i32, i32, i32)) {
    let Some((left, top, right, bottom)) = its_place() else {
        return;
    };
    // Where it hangs is worked out from the logo, and the window is put
    // where that leaves it: the two share their top right corner and the
    // rest of the window is cut away.
    let anchor = hung_from(picture, nudge(), logo(), margin());
    decide_the_direction(picture, anchor, bottom - top);

    // The system puts an owned window back up with the one that owns it,
    // which is right for a button that is only down because the window
    // is. What it does not decide is decided by `how_it_shows`, here,
    // where everything else about the button's place is. Its place is
    // settled either way: a button put away in the wrong corner would
    // show itself there when called back.
    put_the_button(anchor, (right - left, bottom - top), how_it_shows());
}

/// Puts the button's window where and how big it should be, in one move.
///
/// One call and never a resize followed by a placing: between the two the
/// window is the new size at the old place, and the eye catches that
/// every time the menu opens or closes. `anchor` is the corner it hangs
/// from, which is its top right.
///
/// `visibility` is the system's own word for show, hide, or neither;
/// neither is what a window that is being got ready wants.
#[cfg(windows)]
fn put_the_button(anchor: (i32, i32), size: (i32, i32), visibility: u32) {
    use crate::trial::Move;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOREDRAW, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if button.is_null() {
        return;
    }
    // The size is said to stand when it stands, which is nearly every
    // time this is called: a hand dragging the button asks for this a
    // hundred and twenty times a second, always at the same size.
    //
    // Not what makes the white, though it was put here for that and the
    // white stayed. It is kept on its own worth: without it Windows sends
    // the window a resize on every step of a drag, and the toolkit
    // answers each one by handing the web view its bounds again. A
    // hundred and twenty of those a second, for a window whose size has
    // not moved, is work nobody asked for.
    let stands = its_place()
        .is_some_and(|(left, top, right, bottom)| right - left == size.0 && bottom - top == size.1);
    let same_size = if stands { SWP_NOSIZE } else { 0 };
    // And the trials, on the last thing this does while a button is held
    // that it does not do at rest: move.
    let moving = match crate::trial::now() {
        Move::AsToday => 0,
        Move::NoCopy => SWP_NOCOPYBITS,
        Move::NoRedraw => SWP_NOREDRAW,
        Move::Still if stands => return,
        Move::Still => 0,
    };
    // Opening upward, the corner the window hangs by is its bottom right
    // and no longer its top right: the logo stays where the hand left it
    // and the menu grows above it. Which of the two is read from the
    // drawing on screen and never from the wish: a window hung by a
    // corner its page is not drawing in puts the logo a whole menu away
    // from the hand holding it.
    let top = if DRAWN_UPWARD.load(Ordering::Relaxed) {
        anchor.1 + logo().1 - size.1
    } else {
        anchor.1
    };
    // SAFETY: a window of ours, placed and sized without being activated.
    unsafe {
        SetWindowPos(
            button,
            std::ptr::null_mut(),
            anchor.0 - size.0,
            top,
            size.0,
            size.1,
            SWP_NOZORDER | SWP_NOACTIVATE | same_size | moving | visibility,
        )
    };
}

#[cfg(not(windows))]
fn put_the_button(_anchor: (i32, i32), _size: (i32, i32), _visibility: u32) {}

/// Cuts the button's window to the pieces the page drew, on a window that
/// is about to be `width` wide.
///
/// Nothing is drawn outside a window's shape, by anybody: that is what
/// makes this the answer to an opaque rectangle appearing under a page
/// that asked for none.
///
/// The size is taken rather than asked of the window because the window
/// does not have it yet: a piece is placed from the right edge, and from
/// the bottom edge when the menu opens upward, and those edges are where
/// this size is about to put them.
#[cfg(windows)]
fn cut_to_what_is_drawn(shape: &[Piece], size: (i32, i32)) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
    };

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if button.is_null() || shape.is_empty() {
        return;
    }
    // SAFETY: every shape made here is ours until the system takes the
    // one they are gathered into.
    unsafe {
        // Empty to begin with, and every piece added to it.
        let whole = CreateRectRgn(0, 0, 0, 0);
        if whole.is_null() {
            return;
        }
        let upward = DRAWN_UPWARD.load(Ordering::Relaxed);
        for piece in shape {
            // Where it starts, which the page counted from the edges that
            // do not move and the system counts from the top left. The
            // right edge is one of those always; the bottom is the other
            // when the window grows upward, the top then being the edge
            // that moves.
            let left = size.0 + piece.x;
            let top = if upward { size.1 + piece.y } else { piece.y };
            // Exactly the pixels the page painted, and not one more. A
            // shape is cut exclusive of its right and bottom edge, so a
            // piece `width` wide covers `left` to `left + width`; asking
            // for one more claims a column the page never painted, which
            // the window fills with its own ground. That is the pale
            // hairline seen down the right of the logo, and it is the
            // same fault the page already guards against on the other
            // two edges by rounding its measurements inward.
            let one = CreateRoundRectRgn(
                left,
                top,
                left + piece.width,
                top + piece.height,
                piece.radius * 2,
                piece.radius * 2,
            );
            if one.is_null() {
                continue;
            }
            CombineRgn(whole, whole, one, RGN_OR);
            DeleteObject(one);
        }
        // The system owns it from here, but only once it has taken it:
        // refused, it is still ours to free.
        //
        // Handed over without asking for a redraw, and that one word is
        // the whole of the white artifact. Asked to redraw, the system
        // wipes the window with the brush it was made with before
        // anything else happens, and that brush is white; the web view
        // paints over it whenever it next gets round to it, which on the
        // machine's busiest second is not soon. What showed in between
        // was the shape itself filled with white, which is to say a white
        // logo standing over the picture, and it stayed until something
        // moved. Every cut did it; the ones nobody noticed were the ones
        // where a hand was on the logo and the next frame was a
        // millisecond away.
        //
        // What is wanted instead is below: the window drawn again, its
        // ground left alone.
        if SetWindowRgn(button, whole, 0) == 0 {
            DeleteObject(whole);
            return;
        }
    }
    draw_it_all_again(button);
    say_what_it_was_cut_to(button, shape);
}

/// Has the window drawn again, and everything carried inside it with it.
///
/// The shape belongs to the window and the drawing belongs to the web
/// view carried in it, and to the system those are two windows and not
/// one. Redrawing the outer one alone leaves the inner one's last picture
/// standing wherever the new shape lets it through, which is a piece of
/// something that is no longer drawn anywhere. It stays until the page
/// next moves of its own accord, and a page whose menu has just closed
/// under a hand that is somewhere else does not move again.
#[cfg(windows)]
fn draw_it_all_again(button: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::{RDW_ALLCHILDREN, RDW_INVALIDATE, RedrawWindow};

    // Marked as wanting to be drawn, and not drawn here and now. This runs
    // on whichever thread the page asked from, and the window belongs to
    // the one that draws: made to happen on the spot, it would hold this
    // thread until that one came round, which is a wait inside a wait.
    //
    // And without asking for the ground to be wiped first. What would wipe
    // it is the brush the window was made with, and that brush is the very
    // white this is here to be rid of: what is wanted is the page drawn
    // again, not the window emptied and then drawn again.
    //
    // Measured and cleared: this line was taken apart three ways, with no
    // erase, without the web view, and not called at all, and the white
    // was there every time. It is not what makes it.
    //
    // SAFETY: a window of ours, and neither a rectangle nor a shape is
    // named, so the whole of it is meant.
    unsafe {
        RedrawWindow(
            button,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        )
    };
}

/// Says what the window was cut to, and what the system holds of it.
///
/// Every cut, and there are few: a cut only happens when the shape it
/// would make differs from the one the window wears, so the logo's own
/// animation and the once-a-second measures cost nothing here. What is
/// left is the moments that matter, which is the menu opening, the menu
/// closing, and the window changing size.
///
/// Worth having at all because this is the one fault a screenshot cannot
/// show. What the button looks like is entirely this shape, so a shape
/// that has drifted from the drawing and a page that has drawn the wrong
/// thing look exactly alike from the outside.
#[cfg(windows)]
fn say_what_it_was_cut_to(button: windows_sys::Win32::Foundation::HWND, shape: &[Piece]) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::GetWindowRgnBox;

    let mut held = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, and the rectangle is ours.
    let taken = unsafe { GetWindowRgnBox(button, &mut held) };
    // Counted from the edge the pieces were counted from, which is the
    // bottom when the menu opens upward. Counted from the top whatever
    // the drawing, this line reported a height of nought on every menu
    // opening upward: a zero height is the signature this journal is
    // read for, and it was printing one of its own.
    let upward = DRAWN_UPWARD.load(Ordering::Relaxed);
    let drawn = how_far_it_reaches(shape, upward);
    say_where_the_cut_falls(shape);
    // Only what has changed, and never the frame in between. This runs on
    // every cut, a cut happens on every frame of the hover animation, and
    // a journal keeps the last hundred and twenty lines of a file: told at
    // every turn, twelve seconds of a hand resting on the button pushed
    // out everything else, this button's own answers included. What is
    // worth a line is the shape changing kind, the window changing size,
    // and a refusal; the frames of an animation between two of those say
    // the same thing forty times.
    // The rectangle the system holds is part of what has changed, and
    // leaving it out is not a saving: a cut whose geometry moves without
    // its piece count moving said nothing at all, which is exactly what
    // happened the first time the window was cut to a box. A line that
    // stays silent when the thing it reports has changed is worse than no
    // line.
    let said = (
        shape.len(),
        upward,
        taken,
        (held.left, held.top, held.right, held.bottom),
        its_place().map(|(left, top, right, bottom)| (right - left, bottom - top)),
    );
    let changed = LAST_SAID
        .lock()
        .expect("dernière découpe dite")
        .replace(said)
        .is_none_or(|before| before != said);
    if !changed {
        return;
    }
    note(&format!(
        "bouton flottant découpé en {} morceaux jusqu'à {}x{}, dessiné vers le {} ({} voulu) ; \
         le système en tient ({}, {}, {}, {}), sorte {taken} ; la fenêtre est {}",
        shape.len(),
        drawn.0,
        drawn.1,
        if upward { "haut" } else { "bas" },
        if UPWARD.load(Ordering::Relaxed) {
            "haut"
        } else {
            "bas"
        },
        held.left,
        held.top,
        held.right,
        held.bottom,
        match its_place() {
            Some((left, top, right, bottom)) =>
                format!("{}x{} en ({left}, {top})", right - left, bottom - top),
            None => "plus là".to_string(),
        }
    ));
}

/// What the line above last said, so it is only said again when it
/// changes: how many pieces, which way the menu opens, whether the system
/// took the shape, the rectangle it holds, and the size of the window.
#[cfg(windows)]
type Told = (usize, bool, i32, (i32, i32, i32, i32), Option<(i32, i32)>);

#[cfg(windows)]
static LAST_SAID: Mutex<Option<Told>> = Mutex::new(None);

/// The pieces the line below last said, so the same numbers are never
/// said twice, and empty until this button has said anything at all.
///
/// Said at every cut, it came ten times a second while a hand rested on
/// the logo, and a journal keeps the last hundred and twenty lines of a
/// file: it pushed out the very line it was written to deliver, in the
/// one journal that was gathered to read it. A shape that has not moved
/// has nothing to add.
#[cfg(windows)]
static LAST_PIECES: Mutex<Vec<Piece>> = Mutex::new(Vec::new());

/// Says, piece by piece, how far the cut falls from what the page painted.
///
/// The one question this button keeps being asked, and the one the line
/// above cannot answer: whether the stencil bites into the drawing. It is
/// a question about fractions of a pixel, so the drawing is carried here
/// unrounded and the four margins are subtracted rather than reasoned
/// about. A negative one is the cut inside the drawing, which is a hard
/// edge cut through a smoothed one, which is what « le tour est pixelisé »
/// looks like from in here.
///
/// The corner is named apart because it is where the two curves come
/// closest: at forty-five degrees a corner of radius r only reaches
/// 0.29 r out of its box, and the cut's radius is rounded to a whole
/// pixel while the drawing's is not, so a margin that is comfortable
/// along the edges can be nothing at all in the corners.
///
/// Said once when the button is built, and after that only when one of
/// the five margins reaches a whole pixel, which is the one thing that
/// must never happen: a whole pixel of margin is a pixel of window the
/// page paints nothing in, and a window pixel nobody paints is not empty,
/// it is the frosted glass the toolkit turns on to be transparent at all.
/// That was the fault, and this line is what catches it coming back.
///
/// Never twice for the same numbers, and no other threshold. Told at
/// every cut, the line drowned the journal while a hand rested on the
/// logo; told whenever a margin fell under a whole pixel, it drowned it
/// just as fast. A margin between nought and one is the ordinary state of
/// a stencil laid on a drawing whose edges fall between pixels, and the
/// corner one dips a quarter of a pixel below nought by design, the cut's
/// corner being rounded to sit inside the drawing's rather than outside
/// it.
#[cfg(windows)]
fn say_where_the_cut_falls(shape: &[Piece]) {
    let mut before = LAST_PIECES.lock().expect("derniers morceaux dits");
    let first = before.is_empty();
    for (rank, piece) in shape.iter().enumerate() {
        if before.get(rank) == Some(piece) {
            continue;
        }
        let [left, top, right, bottom] = piece.drawn;
        // Positive is the cut outside the drawing, which is what is
        // wanted on all four; negative is the drawing cut into.
        let margins = [
            left - piece.x as f32,
            top - piece.y as f32,
            (piece.x + piece.width) as f32 - right,
            (piece.y + piece.height) as f32 - bottom,
        ];
        // What the corner has left once the radius has been rounded, at
        // the point of the arc closest to the box's own corner, which is
        // the one at forty-five degrees and sits a shade under three
        // tenths of the radius in from each edge. The smaller the radius,
        // the less the arc is set in, so the further out it reaches: the
        // drawing's own radius is the larger of the two here, and the
        // difference is added the way round it is because the cut's arc
        // is the one that reaches out.
        //
        // It used to be subtracted, which is a sign the wrong way round.
        // The journal therefore called every corner negative, on every
        // cut, which is both a lie and a flood; and that lie is what put
        // a whole pixel of margin on the four edges in the first place,
        // the pixel that turned out to be the fault itself.
        const OUT_OF_ITS_BOX: f32 = 1.0 - std::f32::consts::FRAC_1_SQRT_2;
        let corner = margins[0].min(margins[1])
            + (piece.drawn_radius - piece.radius as f32) * OUT_OF_ITS_BOX;
        // Dit aussi la première fois qu'un morceau apparaît, et pas
        // seulement à la toute première découpe : la carte du menu
        // n'existe pas encore quand le bouton est bâti, donc sans ça ses
        // marges ne seraient jamais écrites, alors que ce sont les plus
        // longues du dessin. C'est celle-là qui portait la rangée pâle de
        // quatre cent soixante pixels sous le menu.
        let neuf = first || rank >= before.len();
        if !neuf && margins.iter().chain([&corner]).all(|margin| *margin < 1.0) {
            continue;
        }
        note(&format!(
            "bouton flottant, morceau {} : dessiné {:.2}x{:.2} en ({left:.2}, {top:.2}) \
             coins {:.2}, contour {:.2} px ; découpé ({}, {}, {}, {}) coins {} ; marges \
             g {:.2} h {:.2} d {:.2} b {:.2}, dans le coin {corner:.2}",
            rank + 1,
            right - left,
            bottom - top,
            piece.drawn_radius,
            // What the outline of the logo comes to at this size, which is
            // the number the eye is actually judging: the drawing gives it
            // as a twenty-eighth of its own box, and everything else about
            // this button is downstream of how many pixels that is.
            (right - left) * OUTLINE / LOGO_SCREEN,
            piece.x,
            piece.y,
            piece.x + piece.width,
            piece.y + piece.height,
            piece.radius,
            margins[0],
            margins[1],
            margins[2],
            margins[3],
        ));
    }
    before.clear();
    before.extend_from_slice(shape);
}

/// The logo's own numbers, read off `zyrdesk.svg`: how wide one of its two
/// screens is, outline included, and how thick that outline is.
///
/// Here so the journal can say what the outline comes to in real pixels
/// on the screen it is being looked at. That number is the whole of what
/// « ce n'est pas lisse » is about once the cut has been cleared: an
/// outline three pixels wide with corners rounded by ten cannot be
/// smoother than three pixels allow, however it is composited.
#[cfg(windows)]
const LOGO_SCREEN: f32 = 356.0;
#[cfg(windows)]
const OUTLINE: f32 = 28.0;

#[cfg(not(windows))]
fn cut_to_what_is_drawn(_shape: &[Piece], _size: (i32, i32)) {}

/// Where the button is on screen, as left, top, right and bottom in real
/// pixels, or nothing when there is no button.
///
/// Read from the window itself rather than remembered beside it, and
/// asked of the system rather than of the toolkit: this is called from
/// inside the system's own call into our window, where nothing may wait.
#[cfg(windows)]
fn its_place() -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if button.is_null() {
        return None;
    }
    let mut own = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, and the rectangle is ours.
    if unsafe { GetWindowRect(button, &mut own) } == 0 {
        return None;
    }
    Some((own.left, own.top, own.right, own.bottom))
}

#[cfg(not(windows))]
fn its_place() -> Option<(i32, i32, i32, i32)> {
    None
}

/// The corner the button hangs from right now, which is the logo's own.
///
/// The window's top right ordinarily, and its bottom right less the logo
/// when the menu opens upward: the logo is what the person placed, and it
/// is the only part of this window that does not move.
///
/// Which of the two is read from the drawing on screen and not from the
/// direction this program would rather have. Where the logo is is a fact
/// about what is drawn, and there is only ever one drawing.
#[cfg(windows)]
fn where_it_hangs() -> Option<(i32, i32)> {
    its_place().map(|(_, top, right, bottom)| {
        if DRAWN_UPWARD.load(Ordering::Relaxed) {
            (right, bottom - logo().1)
        } else {
            (right, top)
        }
    })
}

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
    // The logo and not the window carrying it: the window is as wide as
    // the menu all session long, and a picture is not too small for a
    // button because a menu that is not open would not fit in it.
    its_place()?;
    let (wide, high) = logo();
    let margin = margin();
    Some((wide + margin, high + margin))
}

/// Hands the pointer back when the far computer is holding it.
///
/// In game mouse mode the engine keeps the cursor inside the picture and
/// puts it back in the middle at every movement, so nothing on screen can
/// be pointed at any more, this button included. Asked of the engine in
/// its own language.
///
/// Whether the mouse is in that mode is read from what this window has
/// sent, never from the system. It was read from the pointer's cage
/// once, and that lied both ways: a session covering the only screen
/// cages the pointer to a rectangle exactly the size of the screen,
/// which is what no cage at all looks like, and a third program caging
/// the pointer for its own reasons looked like ours.
#[cfg(windows)]
fn give_the_pointer_back(app: &AppHandle) {
    let state = app.state::<Floating>();
    let Some(process) = *state.watched.lock().expect("session suivie") else {
        return;
    };
    if !state.game_mouse.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    note("le pointeur est tenu par la session : rendu avant d'ouvrir le menu");
    // Already on the thread that draws, this being a menu opening, so
    // the whole thing is done on the spot rather than sent round.
    match hand_over_and_type(Act::MouseMode, process) {
        Ok(()) => {
            state
                .game_mouse
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
        Err(reason) => note(&format!("pointeur non rendu : {reason}")),
    }
}

#[cfg(not(windows))]
fn remember_the_button(_window: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn lay_the_button(_picture: (i32, i32, i32, i32)) {}

#[cfg(not(windows))]
fn where_it_hangs() -> Option<(i32, i32)> {
    None
}

#[cfg(not(windows))]
pub fn room_for_the_button() -> Option<(i32, i32)> {
    None
}

#[cfg(not(windows))]
fn give_the_pointer_back(_app: &AppHandle) {}

/// Keeps the button from ever taking the place of the picture, and ties
/// it to the window it hangs over.
///
/// Two things, and neither is optional. Without the first, clicking the
/// button would put the session window in the background: the engine
/// would let go of the keyboard, and the next keystroke would land who
/// knows where.
///
/// Without the second, the button is a window of its own with no ties to
/// anything, and behaves like one: minimising ZyrDesk left it hanging
/// alone in the corner of an empty desktop, over other people's windows,
/// with the picture it belongs to nowhere in sight. Handed to the home
/// window as its owner, it goes down with it and comes back up with it,
/// and the system does that part without being asked. It stays in front
/// of the picture all the same: the picture is owned by that same window
/// and this one is marked always on top, which the picture is not.
#[cfg(windows)]
fn keep_out_of_the_way(app: &AppHandle, window: &tauri::WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };

    let Ok(handle) = window.hwnd() else {
        return;
    };
    let button = handle.0 as _;
    // SAFETY: the handle belongs to a window we have just built, and
    // only its extended style is read and written back.
    unsafe {
        let style = GetWindowLongPtrW(button, GWL_EXSTYLE);
        SetWindowLongPtrW(
            button,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
    let_the_alpha_through(button);

    let Some(home) = app
        .get_webview_window(crate::HOME)
        .and_then(|home| home.hwnd().ok())
    else {
        return;
    };
    // SAFETY: both windows are ours, and only the owner is written.
    unsafe { SetWindowLongPtrW(button, GWLP_HWNDPARENT, home.0 as isize) };
}

/// Gives the window the half of transparency the toolkit does not ask
/// for, without which asking for it changes nothing.
///
/// Asking a window to be transparent has the toolkit tell the compositor
/// to honour the alpha of every pixel, which it does by handing it an
/// empty blur region. That was the whole of it, and on Windows 10 and 11
/// it is not enough: the compositor's own documentation warns that a
/// window's children contribute alpha it cannot predict, and this window
/// is a frame carrying a web view, which is a child. Asked and no more,
/// the window came out opaque, which is why this button has been cut to a
/// shape ever since and why its edge is a staircase.
///
/// What was missing is this: the window declares itself layered, and says
/// that its one constant alpha is « fully opaque », which is the way that
/// call is documented to mean « use the alpha of each pixel and not one
/// number for all of them ». Toolkits that had this fault fixed it here,
/// and nowhere else.
///
/// Said in the journal when the system refuses, and only then: a refusal
/// is the difference between a button with a smooth edge and a button
/// standing on a coloured plate, and there is nothing else on screen that
/// would say which of the two happened.
#[cfg(windows)]
fn let_the_alpha_through(button: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, LWA_ALPHA, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes,
        SetWindowLongPtrW, SetWindowPos, WS_BORDER, WS_CAPTION, WS_DLGFRAME, WS_EX_CLIENTEDGE,
        WS_EX_DLGMODALFRAME, WS_EX_LAYERED, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX,
        WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
    };

    /// Everything that gives this window a frame of its own, which is
    /// the whole of what the system paints into it before the page has
    /// drawn anything.
    ///
    /// The toolkit builds every window with a caption and a system menu
    /// and, asked for no decorations, removes only the caption and the
    /// sizing frame. The system menu stays, and the raised edge with it,
    /// which the styles read back from this very window confirm bit for
    /// bit. A frame is painted in the window itself, not over the page,
    /// so a transparent page never covers it: that is « la fameuse croix
    /// par-dessus le FAB », and the white flash on every click, a click
    /// being exactly when the system repaints a window's frame.
    ///
    /// A button that is cut to the shape of a drawing has no frame to
    /// show, so it is given none to paint.
    const NO_FRAME: u32 = WS_CAPTION
        | WS_BORDER
        | WS_DLGFRAME
        | WS_THICKFRAME
        | WS_SYSMENU
        | WS_MINIMIZEBOX
        | WS_MAXIMIZEBOX;
    const NO_EDGE: u32 =
        WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE | WS_EX_DLGMODALFRAME;

    // SAFETY: a window we have just built, whose two styles are read and
    // written back, which is told how to read its own alpha, and which is
    // then asked to work its frame out again. Nothing here moves it,
    // resizes it, restacks it or activates it.
    let told = unsafe {
        let plain = GetWindowLongPtrW(button, GWL_STYLE);
        SetWindowLongPtrW(button, GWL_STYLE, plain & !(NO_FRAME as isize));
        let style = GetWindowLongPtrW(button, GWL_EXSTYLE);
        SetWindowLongPtrW(
            button,
            GWL_EXSTYLE,
            (style | WS_EX_LAYERED as isize) & !(NO_EDGE as isize),
        );
        // A style is only read when the frame is worked out again, and
        // nothing else here asks for that: without this the window keeps
        // the frame it was born with for the rest of the session.
        SetWindowPos(
            button,
            std::ptr::null_mut::<std::ffi::c_void>() as HWND,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED
                | SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOZORDER
                | SWP_NOACTIVATE
                | SWP_NOOWNERZORDER,
        );
        // No colour is keyed out, so the first argument is unread, and
        // two hundred and fifty-five is « nothing taken off the whole ».
        //
        // Not what makes each pixel carry its own alpha, which is what
        // this line used to claim and what the journal used to print:
        // this call sets one opacity for the whole window and the
        // system's own documentation says so. Per-pixel alpha comes from
        // the toolkit, which turns on blur-behind over an empty region.
        // The call is kept because the window was measured with it and
        // without it and the plate went away with it; what a window is
        // composited from once it is layered is not something the two
        // documentations settle between them. Kept and said plainly is
        // the honest state of it.
        SetLayeredWindowAttributes(button, 0, 255, LWA_ALPHA) != 0
    };
    if !told {
        note("bouton flottant : Windows a refusé l'alpha d'ensemble du calque");
    }
    say_what_it_wears(button);
}

/// Reads back what the window ended up wearing, and writes it down.
///
/// Asked of the system rather than assumed from the calls just made:
/// setting a style and having it is not the same statement, and the
/// difference between the two is the whole of « le bouton est posé sur
/// une plaque » against « le bord est lisse ». Said once, when the button
/// is built, which is the only moment any of it changes.
#[cfg(windows)]
fn say_what_it_wears(button: windows_sys::Win32::Foundation::HWND) {
    // A new button says everything it has to say once more: it is a new
    // window, with its own styles and its own drawing, and its own
    // pictures to sit for.
    LAST_PIECES.lock().expect("derniers morceaux dits").clear();
    *LAST_SAID.lock().expect("dernière découpe dite") = None;

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetLayeredWindowAttributes, GetWindowLongPtrW, LWA_ALPHA,
        LWA_COLORKEY, WS_BORDER, WS_CAPTION, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TRANSPARENT, WS_SYSMENU, WS_THICKFRAME,
    };

    let mut keyed = 0u32;
    let mut alpha = 0u8;
    let mut how = 0u32;
    // SAFETY: our own window, and the three slots are ours. A refusal is
    // one of the answers and leaves them as they were.
    let read = unsafe { GetLayeredWindowAttributes(button, &mut keyed, &mut alpha, &mut how) };
    // SAFETY: our own window, read only.
    let style = unsafe { GetWindowLongPtrW(button, GWL_EXSTYLE) } as u32;
    let wearing = |what: u32| style & what != 0;
    // « alpha d'ensemble » et pas « chaque pixel porte le sien », qui est
    // ce que cette ligne disait et qui est faux : cet appel-là règle une
    // opacité unique pour toute la fenêtre, la documentation du système
    // est explicite dessus, et l'alpha par pixel de cette fenêtre vient
    // d'ailleurs, du flou-derrière sur région vide que pose la boîte à
    // outils. Le journal affirmait donc une chose que l'appel ne fait
    // pas ; l'appel est gardé, la phrase est corrigée.
    note(&format!(
        "bouton flottant : styles {style:#x} (calque {}, sans clic {}, sans premier plan {}, \
         hors barre {}) ; alpha relu {}, {}",
        wearing(WS_EX_LAYERED),
        wearing(WS_EX_TRANSPARENT),
        wearing(WS_EX_NOACTIVATE),
        wearing(WS_EX_TOOLWINDOW),
        if read == 0 {
            "refusé".to_string()
        } else {
            format!("{alpha} sur 255")
        },
        match (how & LWA_ALPHA != 0, how & LWA_COLORKEY != 0) {
            (true, false) => "alpha d'ensemble".to_string(),
            (true, true) => format!("alpha d'ensemble et couleur effacée {keyed:#x}"),
            (false, true) => format!("couleur effacée {keyed:#x}, pas d'alpha"),
            (false, false) => "aucun des deux".to_string(),
        }
    ));

    // And the ordinary style beside them, which is the half that was
    // missing when this button was asked what painted the frame found
    // baked into its own window. A caption, a system menu, a border or a
    // sizing frame are all painted by the system into the window itself,
    // before the page has drawn anything, and a page that never paints
    // over them leaves them there for the life of the session.
    // SAFETY: our own window, read only.
    let plain = unsafe { GetWindowLongPtrW(button, GWL_STYLE) } as u32;
    let has = |what: u32| plain & what != 0;
    note(&format!(
        "bouton flottant : style ordinaire {plain:#x} (barre de titre {}, menu système {}, \
         bordure {}, cadre redimensionnable {})",
        has(WS_CAPTION),
        has(WS_SYSMENU),
        has(WS_BORDER),
        has(WS_THICKFRAME),
    ));
}

#[cfg(not(windows))]
fn keep_out_of_the_way(_app: &AppHandle, _window: &tauri::WebviewWindow) {}

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

/// Whether the primary mouse button is down right now.
///
/// Asked of the system rather than waited for as an event: the window
/// this is dragging is too small to keep the mouse inside it, and a
/// release that happened over the picture is a release all the same.
///
/// Primary, not left. The page starts this on the button the person
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
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, SendInput,
    };

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

    // Where the modifiers sit, in the same numbering: the left-hand ones,
    // which is what a person would press.
    const CTRL: u16 = 0x1D;
    const ALT: u16 = 0x38;
    const SHIFT: u16 = 0x2A;

    let keys = [CTRL, ALT, SHIFT, key, key, SHIFT, ALT, CTRL];
    let events: Vec<INPUT> = keys
        .iter()
        .enumerate()
        .map(|(rank, key)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    // The place is what is sent, and the name is left for
                    // the far end to work out from its own keyboard.
                    wVk: 0,
                    wScan: *key,
                    // The first half presses, the second half releases,
                    // in the mirror order: no key is left down.
                    dwFlags: KEYEVENTF_SCANCODE
                        | if rank >= keys.len() / 2 {
                            KEYEVENTF_KEYUP
                        } else {
                            0
                        },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
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
    if sent as usize == events.len() {
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
        for (name, letter, place) in [("stats", b'S', 0x1Fu16), ("mouse", b'M', 0x32)] {
            let act = Act::read(name).expect(name);
            assert_eq!(act.letter(), Some(letter), "sur « {name} »");
            assert_eq!(act.where_it_sits(), Some(place), "sur « {name} »");
        }
        // Quatre ne passent pas par le clavier du lecteur : terminer se
        // demande à l'ordinateur d'en face à travers le tunnel, couvrir
        // l'écran se fait à notre propre fenêtre, celle du moteur étant
        // posée dedans, Ctrl+Alt+Suppr est la combinaison que Windows
        // garde pour lui aux deux bouts, et le son se coupe dans le
        // mélangeur de cet ordinateur-ci.
        for name in ["end", "fullscreen", "cad", "sound"] {
            assert_eq!(
                Act::read(name).expect(name).letter(),
                None,
                "sur « {name} »"
            );
        }
        assert!(Act::read("teleport").is_none());
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
    fn there_is_one_way_to_end_a_session_and_not_two() {
        // Les moteurs en offrent deux : partir en laissant le bureau
        // distant ouvert, et le rendre. Porter cette différence jusqu'à
        // la personne lui laisserait une session ni en cours ni finie.
        assert!(Act::read("leave").is_none());
        assert!(Act::read("close").is_none());
    }
}
