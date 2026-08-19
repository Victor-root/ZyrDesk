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

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

// What the button did goes into the same journal as everything else: the
// window has nowhere else to say it, standing behind the picture, and a
// menu entry that seems to do nothing is exactly the kind of thing that
// cannot be diagnosed from a screenshot.
use crate::journal::note;

/// Name this window is known by, inside the program.
pub const WINDOW: &str = "flottant";

/// Name the page listens on to be told to show its menu.
const OPEN: &str = "floating-open";

/// How often the session is looked for.
///
/// Short enough that the button is there by the time the picture is, and
/// gone shortly after it.
const LOOK: Duration = Duration::from_secs(1);

/// Distance kept from the corner of the picture.
const MARGIN: i32 = 16;

/// Size of the button alone, as the page draws it, before the page has
/// had a chance to measure itself and say better.
const BUTTON: f64 = 52.0;

/// How far the mouse has to travel, while holding the button, before it
/// is a drag and no longer a click.
///
/// Without it a hand that shakes would move the button every time
/// somebody wanted to open the menu, and the other way round.
const GRIP: i32 = 4;

/// How often the button catches up with the mouse while being dragged.
const FOLLOW: Duration = Duration::from_millis(8);

/// How long the picture is given to come back in front before an entry
/// of the menu gives up on it.
const FRONT_TAKES: Duration = Duration::from_millis(400);

/// Pause between two looks at which window is in front.
const FRONT_STEP: Duration = Duration::from_millis(10);

/// How long the picture is given to go once the far computer has been
/// asked to hand its desktop back.
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
    expected: Mutex<Option<u32>>,
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

/// Set while the person has hidden the button from its own menu.
///
/// A choice they made stands until they ask for the button back. ZyrDesk
/// being minimised and restored is not that ask, and the system would
/// otherwise put the button back up with the window.
static HIDDEN: AtomicBool = AtomicBool::new(false);

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
/// knows.
pub fn expect(app: &AppHandle, process: u32) {
    *app.state::<Floating>()
        .expected
        .lock()
        .expect("session attendue") = Some(process);
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
async fn player(app: &AppHandle) -> Option<u32> {
    if let Some(session) = crate::session::sessions().await.into_iter().next() {
        return Some(session.process);
    }
    let expected = *app
        .state::<Floating>()
        .expected
        .lock()
        .expect("session attendue");
    expected.filter(|process| picture_of(*process).is_some())
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
                    raise(&app, process);
                }
                None => {
                    crate::picture::let_go(&app);
                    lower(&app);
                }
            }
        }
    });
}

/// Puts the button up for that player, if it is not up already.
///
/// Waits for the player to have a window before showing anything. The
/// service calls a session held from the moment the player starts, but
/// the engine only opens its window once the far computer has answered
/// and the stream stands: showing the button any earlier would put it
/// over a screen that has no picture on it yet.
fn raise(app: &AppHandle, process: u32) {
    let state = app.state::<Floating>();
    let already = state
        .watched
        .lock()
        .expect("session suivie")
        .as_ref()
        .is_some_and(|seen| *seen == process);
    if already {
        return;
    }

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
    // Asked before anything is held. This runs on a thread of its own and
    // the answer comes from the one that draws: waiting for it while
    // holding what that thread may want next is how both stop for good.
    let size = button_size(app);

    // A new session starts with the button on screen, whatever was done
    // with the one before.
    HIDDEN.store(false, Ordering::Relaxed);
    *state.watched.lock().expect("session suivie") = Some(process);

    // A leftover window from a session that ended in a way we did not
    // see: put it where the new one is rather than open a second.
    //
    // Taken hold of again first. Letting the last session go forgets the
    // window, and everything that places this button reaches it by that
    // one number: without this the button would sit wherever the previous
    // session left it and follow nothing for the whole of this one.
    if let Some(window) = app.get_webview_window(WINDOW) {
        remember_the_button(&window);
        let _ = window.show();
        lay_the_button(picture);
        return;
    }

    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("bouton.html".into()))
        .title("ZyrDesk")
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
            let _ = window.set_size(PhysicalSize::new(size, size));
            lay_the_button(picture);
            let _ = window.show();
        }
        // A button that could not be drawn is not a reason to disturb a
        // session that is otherwise fine.
        Err(e) => eprintln!("le bouton flottant n'a pas pu s'ouvrir : {e}"),
    }
}

/// What the button comes to in real pixels, on the screen it hangs over.
///
/// Everything in this file is counted in real pixels: the picture is
/// measured with the system's own ruler, and so is the mouse. What the
/// page draws is counted in the other kind, and on a screen magnified to
/// a hundred and seventy-five per cent the same button is fifty-two of
/// one and ninety-one of the other. A window taken to be the smaller of
/// the two shows its own background all around the button.
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
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.close();
    }
}

/// Resizes the button to what the page turned out to need, keeping the
/// corner it hangs from.
///
/// The page measures itself rather than being told a size: the menu's
/// height depends on what is in it, and a number written twice would
/// stop matching the first time an entry is added.
#[tauri::command]
pub fn floating_size(app: AppHandle, width: u32, height: u32) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    // Read before the resize and not after: what is kept is the corner
    // the button hangs from, and a menu that unfolds downwards moves the
    // opposite corner.
    let corner = where_it_hangs();
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|e| e.to_string())?;
    let Some((right, top)) = corner else {
        return Ok(());
    };
    let size = window.outer_size().map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(right - size.width as i32, top))
        .map_err(|e| e.to_string())
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
/// fifty pixels wide: the mouse leaves it on the first movement, and
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
            slide(&app, picture, from, dx, dy)?;
        }
        tokio::time::sleep(FOLLOW).await;
    }
    Ok(!moved)
}

/// Puts the button where the mouse has dragged it to.
///
/// The distance from the corner of the picture is what is remembered
/// rather than the place on screen: a session opened later on another
/// screen, or at another size, then finds the button where it was left
/// rather than off the edge.
fn slide(
    app: &AppHandle,
    picture: (i32, i32, i32, i32),
    from: (i32, i32),
    dx: i32,
    dy: i32,
) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("le bouton flottant n'est plus là")?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let anchor = held_inside(
        (from.0 + dx, from.1 + dy),
        picture,
        size.width as i32,
        size.height as i32,
    );

    nudged_to(
        anchor.0 - (picture.2 - MARGIN),
        anchor.1 - (picture.1 + MARGIN),
    );

    window
        .set_position(PhysicalPosition::new(
            anchor.0 - size.width as i32,
            anchor.1,
        ))
        .map_err(|e| e.to_string())
}

/// Where the button hangs: the top right of the picture, moved by
/// whatever dragging has moved it since.
fn hung_from(picture: (i32, i32, i32, i32), nudge: (i32, i32), size: (i32, i32)) -> (i32, i32) {
    let corner = (picture.2 - MARGIN + nudge.0, picture.1 + MARGIN + nudge.1);
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

/// Brings the button back and opens its menu.
///
/// What a shortcut needs to be able to do above all else: hiding the
/// button is otherwise a decision with no way back before the session
/// ends.
pub fn show_the_menu(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW)
        .ok_or("aucune session en cours")?;
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
    window.emit(OPEN, ()).map_err(|e| e.to_string())
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
    // keyboard.
    match act {
        Act::Fullscreen => return crate::picture::toggle_the_screen(app),
        Act::End => return end_it_on_the_far_computer(app).await,
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

    put_the_picture_in_front(process).await?;
    shortcut(act, process)
}

/// Puts the picture back in front, and waits for it to actually be
/// there.
///
/// Keystrokes go to whatever window is in front. Clicking the button is
/// not supposed to move the picture out of the way, but a web view can
/// take the focus on its own; bringing the picture back is both the fix
/// and what the person expects after using the menu.
///
/// The waiting is the whole point. Windows does not always change the
/// front window on the spot: the ask is posted, the call returns, and a
/// window read straight afterwards is still the old one. Asking once and
/// believing the answer immediately is why every entry in this menu
/// reported that the picture was not in front and did nothing.
async fn put_the_picture_in_front(process: u32) -> Result<(), String> {
    if in_front(process) {
        return Ok(());
    }
    bring_forward(process);

    let until = std::time::Instant::now() + FRONT_TAKES;
    while std::time::Instant::now() < until {
        tokio::time::sleep(FRONT_STEP).await;
        if in_front(process) {
            return Ok(());
        }
    }

    note(&format!(
        "la fenêtre du lecteur {process} n'est pas passée au premier plan"
    ));
    Err("la fenêtre de la session n'est pas au premier plan.\n  \
         Cliquez d'abord dans l'image."
        .to_string())
}

/// Whether that player has stopped showing anything.
///
/// Given a moment: the picture does not go the instant the far computer
/// is asked to let go of its desktop.
async fn the_picture_is_gone(process: u32) -> bool {
    let until = std::time::Instant::now() + CLOSING_SHOWS;
    while std::time::Instant::now() < until {
        if picture_of(process).is_none() {
            return true;
        }
        tokio::time::sleep(FRONT_STEP).await;
    }
    false
}

/// Hands the far computer's desktop back, instead of merely leaving it.
///
/// Where to ask comes from the service rather than from anything this
/// window remembers: the tunnel address is the only one the engine can
/// reach that computer at, and it exists for exactly as long as the way
/// does.
async fn end_it_on_the_far_computer(app: &AppHandle) -> Result<(), String> {
    let session = crate::session::sessions()
        .await
        .into_iter()
        .next()
        .ok_or("aucune session en cours")?;

    note(&format!(
        "fermeture demandée sur {} à travers {}",
        session.towards, session.at
    ));
    // Said before the asking. The engine can lose its stream and stop
    // before the far computer has finished answering, and a session
    // reported as broken to whoever just closed it would be a lie.
    Floating::closing(app, true);

    let process = session.process;
    // On a thread of its own: this asks the far computer a question over
    // the network, and the window must not stop drawing while it waits.
    let answered = tauri::async_runtime::spawn_blocking(move || {
        zyr_session::close_on_the_far_computer(&session.towards, &session.at)
    })
    .await;

    match answered {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            // Asking this well is what takes the answer away. The far
            // computer lets its desktop go, the stream stops, the tunnel
            // that carried the question goes with it, and nothing comes
            // back. A silence that leaves no picture behind is the thing
            // having worked, and saying otherwise would put a red line
            // across the screen every time it did.
            if the_picture_is_gone(process).await {
                note(&format!("fermeture faite, sans réponse ({e})"));
                return Ok(());
            }
            note(&format!("fermeture refusée : {e}"));
            // Refused, so the session is still standing: whatever befalls
            // it later is nobody's doing but its own.
            Floating::closing(app, false);
            Err(e.to_string())
        }
        Err(e) => {
            Floating::closing(app, false);
            Err(e.to_string())
        }
    }
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let button = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    let Some(own) = its_place() else {
        return;
    };
    let size = (own.right - own.left, own.bottom - own.top);
    let anchor = hung_from(picture, nudge(), size);

    // The system puts an owned window back up with the one that owns it,
    // which is right for a button that is only down because the window
    // is. It is not right for one the person hid on purpose, so that
    // choice is put back here, where everything else about its place is
    // decided. Its place is settled all the same: a hidden button left
    // behind would show itself in the wrong corner when called back.
    let hidden = if HIDDEN.load(Ordering::Relaxed) {
        SWP_HIDEWINDOW
    } else {
        0
    };
    // SAFETY: a window of ours, moved without being resized or
    // activated.
    unsafe {
        SetWindowPos(
            button,
            std::ptr::null_mut(),
            anchor.0 - size.0,
            anchor.1,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | hidden,
        )
    };
}

/// Where the button is on screen, in real pixels, or nothing when there
/// is no button.
///
/// Read from the window itself rather than remembered beside it, and
/// asked of the system rather than of the toolkit: this is called from
/// inside the system's own call into our window, where nothing may wait.
#[cfg(windows)]
fn its_place() -> Option<windows_sys::Win32::Foundation::RECT> {
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
    Some(own)
}

/// The corner the button hangs from right now.
#[cfg(windows)]
fn where_it_hangs() -> Option<(i32, i32)> {
    its_place().map(|own| (own.right, own.top))
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
    its_place().map(|own| (own.right - own.left + MARGIN, own.bottom - own.top + MARGIN))
}

/// Hands the pointer back when the far computer is holding it.
///
/// In game mouse mode the engine keeps the cursor inside the picture and
/// puts it back in the middle at every movement, so nothing on screen can
/// be pointed at any more, this button included. Asked of the engine in
/// its own language, and only when it is really holding it: whether it is
/// shows in where the system says the pointer may go.
#[cfg(windows)]
fn give_the_pointer_back(app: &AppHandle) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClipCursor, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };

    let Some(process) = *app
        .state::<Floating>()
        .watched
        .lock()
        .expect("session suivie")
    else {
        return;
    };
    let mut allowed = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: the rectangle is ours.
    if unsafe { GetClipCursor(&mut allowed) } == 0 {
        return;
    }
    // SAFETY: no argument beyond the metric asked for.
    let whole = unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let held = allowed.right - allowed.left < whole.0 || allowed.bottom - allowed.top < whole.1;
    if !held {
        return;
    }
    note("le pointeur est tenu par la session : rendu avant d'ouvrir le menu");
    if let Err(reason) = shortcut(Act::MouseMode, process) {
        note(&format!("pointeur non rendu : {reason}"));
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

/// Whether the left mouse button is down right now.
///
/// Asked of the system rather than waited for as an event: the window
/// this is dragging is too small to keep the mouse inside it, and a
/// release that happened over the picture is a release all the same.
#[cfg(windows)]
fn held_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

    // SAFETY: no argument, and the answer is a plain bit field.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON)) };
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
#[cfg(windows)]
fn shortcut(act: Act, process: u32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, SendInput,
    };

    // The picture was brought in front and waited for by the caller. If
    // it slipped away since, the keys would land in someone else's lap,
    // and a quit combo in the wrong window is not a mistake worth
    // risking.
    if !in_front(process) {
        note(&format!(
            "{act} refusé : la fenêtre au premier plan n'est pas celle du lecteur {process}"
        ));
        return Err("la fenêtre de la session n'est pas au premier plan.\n  \
             Cliquez d'abord dans l'image."
            .to_string());
    }

    let (Some(letter), Some(key)) = (act.letter(), act.where_it_sits()) else {
        return Ok(());
    };
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

/// Brings that player's picture back in front.
#[cfg(windows)]
fn bring_forward(process: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let Some(window) = window_of(process, Looked::Taken) else {
        return;
    };
    // SAFETY: the window comes from the enumeration just above. Windows
    // may refuse to change the foreground, which the caller checks for
    // rather than trusts.
    unsafe { SetForegroundWindow(window) };
}

/// Whether the window in front belongs to that process.
#[cfg(windows)]
fn in_front(process: u32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: no argument, and a null answer is one of the answers.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return false;
    }
    let mut owner = 0u32;
    // SAFETY: the window comes from the call above and the slot is ours.
    unsafe { GetWindowThreadProcessId(window, &mut owner) };
    owner == process
}

#[cfg(not(windows))]
fn shortcut(_act: Act, _process: u32) -> Result<(), String> {
    Err("les sessions ne tournent que sous Windows".to_string())
}

#[cfg(not(windows))]
fn bring_forward(_process: u32) {}

#[cfg(not(windows))]
fn in_front(_process: u32) -> bool {
    false
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
            let ou = hung_from(image, (0, 0), bouton);
            assert!(ou.0 >= image.0 && ou.0 <= image.2, "sur {image:?} : {ou:?}");
            assert!(ou.1 >= image.1, "sur {image:?} : {ou:?}");
        }
    }

    #[test]
    fn the_button_hangs_in_the_top_right_corner_of_the_picture() {
        let image = (100, 200, 1_000, 800);
        let ou = hung_from(image, (0, 0), (91, 91));
        assert_eq!(ou, (1_000 - MARGIN, 200 + MARGIN));
    }

    #[test]
    fn a_button_dragged_past_an_edge_comes_back_against_it() {
        let image = (100, 200, 1_000, 800);
        let loin = hung_from(image, (5_000, 5_000), (91, 91));
        assert_eq!(loin, (1_000, 800 - 91));
        let avant = hung_from(image, (-5_000, -5_000), (91, 91));
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
