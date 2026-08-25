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
    End,
}

impl Act {
    fn read(name: &str) -> Option<Self> {
        match name {
            "fullscreen" => Some(Act::Fullscreen),
            "stats" => Some(Act::Stats),
            "mouse" => Some(Act::MouseMode),
            "end" => Some(Act::End),
            _ => None,
        }
    }

    /// Letter of the engine's Ctrl+Alt+Shift shortcut, for the ones that
    /// have one.
    ///
    /// Two do not. Ending a session is asked of the far computer over
    /// the tunnel, since what ends it there is that computer letting its
    /// desktop go; and covering the screen is done to our own window,
    /// the engine's having gone inside it.
    fn letter(self) -> Option<u8> {
        match self {
            Act::Stats => Some(b'S'),
            Act::MouseMode => Some(b'M'),
            Act::Fullscreen | Act::End => None,
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
            Act::Fullscreen | Act::End => None,
        }
    }
}

impl std::fmt::Display for Act {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Act::Fullscreen => "plein écran",
            Act::Stats => "statistiques",
            Act::MouseMode => "mode de la souris",
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
/// The button is drawn above every window on the machine, which is what
/// it takes to sit on a picture belonging to another program. Left up, it
/// would hang over whatever was switched to; so it follows the front, and
/// the front of a session is the picture as much as it is ours.
#[cfg(windows)]
fn how_it_shows() -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_HIDEWINDOW, SWP_SHOWWINDOW};

    if READY.load(Ordering::Relaxed)
        && !HIDDEN.load(Ordering::Relaxed)
        && the_session_holds_the_front()
    {
        SWP_SHOWWINDOW
    } else {
        SWP_HIDEWINDOW
    }
}

#[cfg(not(windows))]
fn how_it_shows() -> u32 {
    0
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
                        // A session just adopted starts in the mouse mode
                        // its settings asked for; every toggle after that
                        // goes through this window and is counted as it
                        // is sent.
                        let game = !crate::settings::preferred().await.absolute_mouse;
                        app.state::<Floating>()
                            .game_mouse
                            .store(game, Ordering::Relaxed);
                    }
                    put_the_button_up(&app, process);
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
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
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
#[derive(serde::Deserialize)]
pub struct Piece {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    radius: i32,
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
pub fn floating_size(width: u32, height: u32, shape: Vec<Piece>) -> Result<(), String> {
    // Read before anything moves: what is kept is the corner the button
    // hangs from, which is the window's top right and the logo's.
    let (Some(corner), Some(was)) = (where_it_hangs(), its_place()) else {
        return Err("le bouton flottant n'est plus là".to_string());
    };
    let size = (
        (width as i32).max(was.2 - was.0),
        (height as i32).max(was.3 - was.1),
    );
    // The shape first, and against the size the window is about to have
    // rather than the one it has: a shape wider than the window it is put
    // on is simply clipped by it, so setting it early costs nothing, while
    // a window briefly at its new size under its old shape shows.
    cut_to_what_is_drawn(&shape, size.0);
    // The page has drawn something, so there is something to show.
    READY.store(true, Ordering::Relaxed);
    put_the_button(corner, size, how_it_shows());
    tell_the_button(was, size, &shape);
    Ok(())
}

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
    // How far the drawing reaches from the two edges it is counted from,
    // which is the width and the height a window has to have to hold all
    // of it.
    let drawn = shape.iter().fold((0, 0), |(wide, high), piece| {
        (wide.max(-piece.x), high.max(piece.y + piece.height))
    });
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
    while held_down() && std::time::Instant::now() < until {
        let Some(now) = cursor_now() else {
            break;
        };
        let (dx, dy) = (now.0 - start.0, now.1 - start.1);
        if moved || dx.abs() >= GRIP || dy.abs() >= GRIP {
            moved = true;
            slide(picture, from, dx, dy);
        }
        tokio::time::sleep(FOLLOW).await;
    }
    if moved {
        leave_it_there();
    }
    Ok(!moved)
}

/// Puts the button where the mouse has dragged it to.
///
/// The distance from the corner of the picture is what is remembered
/// rather than the place on screen: a session opened later on another
/// screen, or at another size, then finds the button where it was left
/// rather than off the edge.
fn slide(picture: (i32, i32, i32, i32), from: (i32, i32), dx: i32, dy: i32) {
    let Some((left, top, right, bottom)) = its_place() else {
        return;
    };
    let logo = logo();
    let anchor = held_inside((from.0 + dx, from.1 + dy), picture, logo.0, logo.1);

    let margin = margin();
    nudged_to(
        anchor.0 - (picture.2 - margin),
        anchor.1 - (picture.1 + margin),
    );

    // Asked of the system and not of the toolkit, like everywhere else
    // the button moves: this runs a hundred times a second under a hand,
    // and a trip through an event queue at that rhythm is what a button
    // lagging its own cursor is made of.
    put_the_button(anchor, (right - left, bottom - top), 0);
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
        _ => {}
    }

    let process = app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .copied()
        .ok_or("aucune session en cours")?;

    type_at_the_picture(app, act, process).await?;
    // The keystroke left, so the engine will act on it: the mode this
    // window believes the mouse is in follows the keystrokes it sends.
    if matches!(act, Act::MouseMode) {
        let state = app.state::<Floating>();
        let _ = state
            .game_mouse
            .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
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
#[cfg(windows)]
fn remember_the_button(window: &tauri::WebviewWindow) {
    if let Ok(handle) = window.hwnd() {
        ITS_WINDOW.store(handle.0 as isize, Ordering::Relaxed);
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if button.is_null() {
        return;
    }
    // SAFETY: a window of ours, placed and sized without being activated.
    unsafe {
        SetWindowPos(
            button,
            std::ptr::null_mut(),
            anchor.0 - size.0,
            anchor.1,
            size.0,
            size.1,
            SWP_NOZORDER | SWP_NOACTIVATE | visibility,
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
/// The width is taken rather than asked of the window because the window
/// does not have it yet: a piece is placed from the right edge, and that
/// edge is where this size is about to put it.
#[cfg(windows)]
fn cut_to_what_is_drawn(shape: &[Piece], width: i32) {
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
        for piece in shape {
            // Where it starts, which the page counted from the right edge
            // and the system counts from the left.
            let left = width + piece.x;
            // One more pixel each way: a shape is cut exclusive of its
            // right and bottom edge, and a logo short of its last row is
            // a logo with a line missing.
            let one = CreateRoundRectRgn(
                left,
                piece.y,
                left + piece.width + 1,
                piece.y + piece.height + 1,
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
        // refused, it is still ours to free. Redrawn on the spot, since
        // most of the time this is the only thing that changes: the
        // window keeps one size for the whole session, and opening the
        // menu or running a hand over the logo is a change of shape and
        // nothing else.
        if SetWindowRgn(button, whole, 1) == 0 {
            DeleteObject(whole);
        }
    }
}

#[cfg(not(windows))]
fn cut_to_what_is_drawn(_shape: &[Piece], _width: i32) {}

/// Whether the window at the front belongs to this session.
///
/// Ours or the player's: the picture is another program's window, and it
/// rides inside ours for the length of a session, so the front is ours
/// while the session is being used. Asked of the player's process alone,
/// as it once was before sending it a shortcut, the answer was no for
/// the whole session and every shortcut was refused.
///
/// The question the button is shown or hidden on, and nothing else, and
/// « the picture itself » is not a stricter version of it that some other
/// caller could ask for: the picture is carried as a child of our own
/// window for the length of a session, and the system gives the front to
/// the head of a family and never to a child of it, so the answer would
/// be no for the whole of every session. What the picture can hold is the
/// keyboard, which is asked for elsewhere; see `picture::the_keyboard_back`.
#[cfg(windows)]
fn the_session_holds_the_front() -> bool {
    crate::picture::who_holds_the_front() != crate::picture::Front::Elsewhere
}

#[cfg(not(windows))]
fn the_session_holds_the_front() -> bool {
    false
}

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

/// The corner the button hangs from right now.
#[cfg(windows)]
fn where_it_hangs() -> Option<(i32, i32)> {
    its_place().map(|(_, top, right, _)| (right, top))
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

    let Some(home) = app
        .get_webview_window(crate::HOME)
        .and_then(|home| home.hwnd().ok())
    else {
        return;
    };
    // SAFETY: both windows are ours, and only the owner is written.
    unsafe { SetWindowLongPtrW(button, GWLP_HWNDPARENT, home.0 as isize) };
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
        // Deux ne passent pas par le clavier du lecteur : terminer se
        // demande à l'ordinateur d'en face à travers le tunnel, et
        // couvrir l'écran se fait à notre propre fenêtre, celle du moteur
        // étant posée dedans.
        for name in ["end", "fullscreen"] {
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
