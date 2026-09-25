//! The floating button of a session.
//!
//! During a session the picture fills the window. This is the one thing
//! of ours left on top of it: a small button, hanging in a corner, that
//! opens what can be done without leaving the picture.
//!
//! It is made of two windows of ours, the logo and the menu card, which
//! this program draws itself. This file draws nothing; it keeps track of
//! where the button hangs, what it does, and when it goes up and comes
//! down.
//!
//! Windows of ours rather than a drawing made in the picture: the picture
//! is the far computer's desktop, and a window of ours lets itself be
//! clicked without the player having to give the mouse back.
//!
//! Two things make that work, and both are why no session ever takes the
//! screen exclusively. A window that owns the screen lets nothing be
//! drawn above it. And the pointer, in the ordinary desktop mode, stays
//! free to leave the picture: this computer draws it over the picture in
//! the far computer's shape, and in its own shape the moment it crosses
//! onto this button.
//!
//! What the menu asks of the session, it asks of the player, in the same
//! program: a call that answers, where it once was a keystroke typed at
//! another program in the hope that it listened. Ctrl+Alt+Suppr and the
//! lock screen are asked of the far computer's service through the way,
//! since only a service may do them over there.

// Off Windows there is no session to float over. The rest stays compiled
// and tested everywhere all the same.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering};
use std::time::Duration;

use crate::app::App;

// What the button did goes into the same journal as everything else: it
// has nowhere else to say it, standing behind the picture, and a menu
// entry that seems to do nothing is exactly the kind of thing that cannot
// be diagnosed from a screenshot.

/// What this module files its journal lines under.
const TAG: &str = "floating";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

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
/// The same number as the logo's in `button.css`, and it has to stay the
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

/// How long a drag may last before it is called over.
///
/// A mouse unplugged mid-drag, or a button released where nothing
/// noticed, would otherwise leave the button following a cursor nobody
/// is holding for as long as the program runs.
const AT_MOST: Duration = Duration::from_secs(60);

/// What the menu can ask of the session.
///
/// Ending a session is one entry and not two. A session could be left
/// with the far desktop open and waiting, or closed with it handed back,
/// and carrying that difference up to the person would leave them with a
/// session that is neither running nor over. A session is on or it is
/// not.
#[derive(Clone, Copy)]
pub enum Act {
    Fullscreen,
    Stats,
    MouseMode,
    /// Ctrl+Alt+Suppr, pressed on the far computer.
    SecureAttention,
    /// The lock screen of the far computer, put up.
    LockScreen,
    /// The session's own sound, hushed or given back on this computer.
    Sound,
    /// Which of the two computers Alt+Tab, Échap and the Windows key
    /// belong to.
    SystemKeys,
    /// Whether the two computers share one clipboard.
    Clipboard,
    /// Whether the two badges in the corner of the picture are drawn at
    /// all times rather than only when they have something to say.
    Badges,
    End,
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
            Act::Clipboard => "presse-papiers partagé",
            Act::Badges => "voyants montrés en permanence",
            Act::End => "fin de la session",
        })
    }
}

/// The session the button belongs to.
#[derive(Default)]
pub struct Floating {
    /// Whether the button is up over the picture of a session.
    up: AtomicBool,
    /// Set while this window is closing the session.
    ///
    /// Read by the opening, which lets go where it stands, and by the end
    /// of the session, which is then no failure to report: a session that
    /// ended because the person closed it must never be told to them as
    /// a session that broke.
    closing: AtomicBool,
    /// Whether the two computers share one clipboard right now.
    ///
    /// This one holds nothing at all beyond the switch: sharing is done
    /// by the two services, on the product's own channel inside the
    /// tunnel, and this window's whole part in it is to say which way the
    /// switch is and to write that down.
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
    badges: AtomicBool,
    /// Whether the figures of the session are shown in the corner of the
    /// picture, which « Statistiques » turns on and off.
    figures: AtomicBool,
}

static NUDGE: AtomicI64 = AtomicI64::new(0);

/// Which way the menu card opens, as seen from the button.
///
/// The button can be put down anywhere in the picture, and the card
/// is taller than half a screen: there is not one direction that
/// always works, there are three, and it is the room left around the
/// button that decides which.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opens {
    /// Below the button, when the picture has room below
    /// it.
    Down,
    /// Above, when it only has room above.
    Up,
    /// To the left of the button, when neither.
    ///
    /// A button put down halfway up leaves enough room neither below nor
    /// above, and the card was cut off there by the bottom of the
    /// picture. Beside it, the card has the whole height of the picture
    /// to itself: it no longer starts from the button, it sits to its
    /// left and slides as far as it needs to fit whole.
    Side,
}

impl Opens {
    /// The direction stored in a number, the only type an atomic
    /// carries.
    fn number(self) -> u8 {
        match self {
            Opens::Down => 0,
            Opens::Up => 1,
            Opens::Side => 2,
        }
    }

    /// And read back. A number nobody else writes: anything that is
    /// not a known direction is the default direction.
    fn from_number(number: u8) -> Self {
        match number {
            1 => Opens::Up,
            2 => Opens::Side,
            _ => Opens::Down,
        }
    }
}

/// The direction in force, decided every time the button is put down.
///
/// The window of the card is as tall as the card for the whole session
/// and hangs from the logo, which takes up one of its corners. Decided
/// here and not in the card: it does not know where it was put on a
/// screen, and the logo needs the answer just as the card does.
static OPENS: AtomicU8 = AtomicU8::new(0);

/// Whether the window opens held by its left edge rather than by its
/// right edge, decided at the same moment as `OPENS` and for the same
/// reason: a button put down near the left edge of the picture does not
/// leave the card the room to start from its right edge as it usually
/// does.
static TO_THE_RIGHT: AtomicBool = AtomicBool::new(false);

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

/// Which way a card of this height opens, for a button hanging there.
///
/// Below as long as it fits, above otherwise, and beside when neither:
/// that is the order in which a card reads most naturally from the
/// button that opens it.
///
/// A picture too short for the card whatever the direction keeps it on
/// the side with the most room left: putting it beside would not keep
/// it from being cut off, and would also cost it no longer starting
/// from the button.
fn where_it_opens(picture: (i32, i32, i32, i32), anchor: (i32, i32), height: i32) -> Opens {
    let below = picture.3 - anchor.1;
    if height <= below {
        return Opens::Down;
    }
    let above = anchor.1 + logo().1 - picture.1;
    if height <= above {
        return Opens::Up;
    }
    if height <= picture.3 - picture.1 {
        return Opens::Side;
    }
    if above > below {
        Opens::Up
    } else {
        Opens::Down
    }
}

/// Which side a window of this width has room to start from, for a
/// button hanging there: held by its right edge as usual, as long as it
/// fits to its left; by its left edge otherwise, as long as it fits to
/// its right.
///
/// A picture too narrow for it on both sides keeps it on the side with
/// the most room left, for the same reason as `where_it_opens`.
fn opens_rightwards(picture: (i32, i32, i32, i32), anchor: (i32, i32), width: i32) -> bool {
    let room_left = anchor.0 - picture.0;
    if width <= room_left {
        return false;
    }
    let room_right = picture.2 - (anchor.0 - logo().0);
    if width <= room_right {
        return true;
    }
    room_right > room_left
}

/// How wide the menu window is in all for that vertical direction,
/// which decides on which side it has room to open.
///
/// Beside, it also counts the button and the gap between them: it is
/// its whole window that sits beside the button, never its card alone.
#[cfg(windows)]
fn menu_width(opens: Opens) -> i32 {
    crate::menu::width(opens, logo().0)
}

#[cfg(not(windows))]
fn menu_width(_opens: Opens) -> i32 {
    0
}

/// Works the direction out again for a window about to be that tall, and
/// remembers it.
fn decide_the_direction(picture: (i32, i32, i32, i32), anchor: (i32, i32), height: i32) {
    let opens = where_it_opens(picture, anchor, height);
    OPENS.store(opens.number(), Ordering::Relaxed);
    TO_THE_RIGHT.store(
        opens_rightwards(picture, anchor, menu_width(opens)),
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
    /// Says a close is being asked for, or that none is any more.
    fn closing(app: &App, asked: bool) {
        app.floating().closing.store(asked, Ordering::Relaxed);
    }

    /// Whether a close has been asked for, without forgetting it.
    ///
    /// Asked while a session is still opening, which lets go where it
    /// stands, and during the pause before a picture comes back. Left
    /// standing for `was_closed_on_purpose` to take, since that is what
    /// the session reads once it is over.
    pub fn a_close_was_asked_for(app: &App) -> bool {
        app.floating().closing.load(Ordering::Relaxed)
    }

    /// Whether the session that just ended was closed on purpose, and
    /// forgets it either way.
    pub fn was_closed_on_purpose(app: &App) -> bool {
        app.floating().closing.swap(false, Ordering::Relaxed)
    }
}

/// Whether a session is running right now: a player plays in this
/// window, whether or not its first picture has come.
pub fn a_session_is_up(_app: &App) -> bool {
    crate::session::player().is_some()
}

/// Follows the session for as long as the program runs, and puts the
/// button up and down with its picture.
///
/// The picture going up puts the button up at once as well; this comes
/// round once a second for everything that can change without anybody
/// saying so, a window coming back from the taskbar first of all.
pub fn watch(app: App) {
    crate::app::spawn(async move {
        loop {
            tokio::time::sleep(LOOK).await;
            keep_up_with_the_picture(&app);
        }
    });
}

/// Puts the button up over the picture and keeps what goes with it
/// running, or takes it all down when there is no picture.
pub fn keep_up_with_the_picture(app: &App) {
    let Some(picture) = crate::video::where_it_is() else {
        // The pointer first: without a picture there is nothing left to
        // shut it in, and a cage left behind by a session holds the
        // whole desktop.
        crate::picture::shut_the_pointer_in(crate::picture::Cage::Free);
        lower(app);
        return;
    };
    put_the_button_up(app, picture);
    // The pointer of this computer stays inside the picture as long as
    // the mouse is a game's, or the picture is the whole screen; see
    // `picture::Cage`.
    crate::picture::shut_the_pointer_in(crate::picture::Cage::for_the(
        crate::video::in_a_game(),
        crate::main_window::holds_the_screen(),
        the_menu_is_open(),
        crate::main_window::in_front(),
    ));
    // And the shape this pointer takes, which comes from the far
    // computer and is asked for far more often than this watch goes
    // round: it has its own loop, started again here when the previous
    // one has stopped.
    crate::pointer::follow(app);
    // And the health of the session, read again far more often than this
    // watch goes round: what it lights up must show within a third of a
    // second.
    crate::badges::watch(app);
    // And its figures, when they are asked for.
    if the_figures_are_shown(app) {
        crate::statistics::watch(app, the_bottom_corner(picture));
    }
    // And what arrives of the files being pasted, read again at the same
    // rhythm and for the same reason: a bar that moves once a second does
    // not look as if it is moving.
    crate::transfer::watch(app);
}

/// Takes a new session's player as the one the button belongs to, and
/// starts its switches on the sides its settings asked for.
///
/// Every session, and every picture that comes back after a fall: the
/// switches that live in this window start over with it, as they did
/// when a player was a program of its own.
pub fn adopt(app: &App, preferred: &zyr_proto::session::Preferred) {
    // A new session starts with the button on screen, whatever was done
    // with the one before.
    HIDDEN.store(false, Ordering::Relaxed);
    let state = app.floating();
    state
        .clipboard
        .store(preferred.shared_clipboard, Ordering::Relaxed);
    state
        .figures
        .store(preferred.stats_overlay, Ordering::Relaxed);
    // Off at every session, whatever the last one was left on: see the
    // field itself.
    state.badges.store(false, Ordering::Relaxed);
}

/// Puts the button up over that picture, and keeps it up.
///
/// Called at every turn of the watch and does nothing when there is
/// nothing to do, so a session begun while ZyrDesk was down in the
/// taskbar still gets its button the moment the window comes back.
// Off Windows, the logo and the card do not exist, and nothing of what
// remains here needs the program.
#[cfg_attr(not(windows), allow(unused_variables))]
fn put_the_button_up(app: &App, picture: (i32, i32, i32, i32)) {
    // A button over a window that is not on screen would be the only
    // thing showing, hanging in a corner over somebody else's work. It
    // goes up when the window does, which the watch sees a second later.
    // Minimised counts as not on screen and has to be asked for
    // separately: a window down in the taskbar still calls itself
    // visible.
    if !crate::main_window::on_screen() {
        return;
    }
    app.floating().up.store(true, Ordering::Relaxed);

    let size = button_size() as i32;
    ITS_LOGO.store(size, Ordering::Relaxed);

    // The button is made of two windows this program draws: the logo,
    // which a hand comes down on, and the menu card. Opening them costs
    // nothing when they are already open, which makes this part the same
    // work at every turn of the watch.
    #[cfg(windows)]
    {
        let anchor = hung_from(picture, nudge(), (size, size), margin());
        let opens = Opens::from_number(OPENS.load(Ordering::Relaxed));
        let room_right = TO_THE_RIGHT.load(Ordering::Relaxed);
        crate::logo::raise(app, size as u32, opens == Opens::Up, room_right, anchor);
        // The card measures itself on what its lines ask for, so it needs
        // to know how much a page pixel counts for here and which theme
        // the window wears.
        //
        // The theme is asked of the product and not of the window: it is
        // the same answer for every screen, and only one to keep.
        crate::menu::raise(app, crate::main_window::scale(), crate::theme::light());
    }
    // And the two badges, in the opposite corner. What opens here is the
    // window that will carry them: it stays put away as long as there is
    // nothing to say, which is most of a session.
    crate::badges::raise(app, the_other_corner(picture));
    lay_the_button(picture);
}

/// Whether a window of the button is still to be made, asked by the
/// thread that makes it, at the moment it would.
///
/// The button is put up from more than one place, the watch and the
/// picture's own window among them, and each asks for the windows it
/// finds missing: two asks crossing before the first is answered made a
/// second logo over the first, and the first hung there, lost, for as long
/// as the program ran. Nor is one made once the picture it hangs on has
/// gone.
#[cfg(windows)]
pub fn still_to_be_made(its_window: &std::sync::atomic::AtomicIsize) -> bool {
    its_window.load(Ordering::Relaxed) == 0 && crate::video::shown()
}

/// The top left corner of the picture, at the same margin as the button.
///
/// The same margin and not a second one: the two corners are looked at
/// one after the other, and two different gaps show at once.
fn the_other_corner(picture: (i32, i32, i32, i32)) -> (i32, i32) {
    let margin = margin();
    (picture.0 + margin, picture.1 + margin)
}

/// The bottom left corner of the picture, at the same margin again: where
/// the figures of the session stand.
fn the_bottom_corner(picture: (i32, i32, i32, i32)) -> (i32, i32) {
    let margin = margin();
    (picture.0 + margin, picture.3 - margin)
}

/// What the button comes to in real pixels, on the screen it hangs over.
///
/// Everything in this file is counted in real pixels: the picture is
/// measured with the system's own ruler, and so is the mouse. The design
/// system counts in the other kind, and on a screen magnified to a
/// hundred and seventy-five per cent the same button is forty-four of one
/// and seventy-seven of the other.
fn button_size() -> u32 {
    (BUTTON * f64::from(crate::main_window::scale())).ceil() as u32
}

/// Takes the button down.
///
/// Called by the watch when the picture is no longer there, and by the
/// end of the session the moment it is over: a second of a button hanging
/// over a picture that has gone is a second too many, and the watch only
/// comes round once a second.
pub fn lower(app: &App) {
    if app.floating().up.swap(false, Ordering::Relaxed) {
        crate::badges::lower(app);
        #[cfg(windows)]
        {
            crate::menu::lower(app);
            crate::logo::lower(app);
        }
    }
}

/// Puts the button away until the next session, or until the shortcut
/// that calls it back.
///
/// The two windows it is made of go away together, the logo and the
/// card: one left standing would be a button half put away.
pub fn hide(app: &App) -> Result<(), String> {
    if !a_session_is_up(app) {
        return Err("le bouton flottant n'est plus là".to_string());
    }
    HIDDEN.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    {
        crate::menu::show(false);
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
pub async fn grabbed() -> bool {
    // The picture is read once: it does not move while the button is
    // being dragged over it.
    let (Some(start), Some(picture)) = (cursor_now(), crate::video::where_it_is()) else {
        return true;
    };
    // Where the button starts from, worked out and not read back: it is
    // the same calculation that puts it down, from the same numbers, so
    // the two cannot contradict each other.
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

/// Tells the logo that the gesture has become a drag, at the very step
/// where it becomes one, then that it is over.
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
    if crate::menu::is_open() {
        crate::menu::show(false);
        return Ok(());
    }
    // The session first, when it was put away: the button hangs on the
    // picture, and a menu opened over an empty desktop, picture down in
    // the taskbar, is a button floating over somebody else's work. The
    // shortcut asks to do something with the session, so the session
    // comes back.
    if !crate::main_window::on_screen() {
        crate::show_home(app);
    }
    // Asked for by name, which takes back the choice of hiding it.
    HIDDEN.store(false, Ordering::Relaxed);
    // In a game the pointer is shut on a point in the middle of the
    // picture, so it cannot be brought to this button at all. Asking for
    // the menu is asking to do something with the pointer, so the cage
    // is opened here rather than at the next turn of the watch: a menu
    // that takes a second to become usable reads as a menu that does not
    // work. The mouse mode itself is left exactly where it is.
    crate::picture::shut_the_pointer_in(crate::picture::Cage::Free);
    #[cfg(windows)]
    {
        crate::logo::shown(app, true);
        // And the pointer is put on the button, the cage having just been
        // opened. A game hides the pointer over the picture and only over
        // it, so one freed in the middle of the picture has to cross it
        // unseen to reach this button: aiming blind, on the one thing a
        // session cannot be left without.
        if crate::video::in_a_game() {
            crate::picture::put_the_pointer_on(crate::logo::its_window());
        }
        crate::menu::show(true);
    }
    Ok(())
}

/// Whether the two computers share one clipboard right now.
pub fn the_clipboard_is_shared(app: &App) -> bool {
    app.floating().clipboard.load(Ordering::Relaxed)
}

/// Whether the badges in the corner of the picture are being held on
/// screen rather than left to show themselves.
pub fn the_badges_are_held_up(app: &App) -> bool {
    app.floating().badges.load(Ordering::Relaxed)
}

/// Whether the figures of the session are shown over the picture.
pub fn the_figures_are_shown(app: &App) -> bool {
    app.floating().figures.load(Ordering::Relaxed)
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
fn always_show_the_badges(app: &App) -> Result<(), String> {
    let state = app.floating();
    let held = !state.badges.load(Ordering::Relaxed);
    state.badges.store(held, Ordering::Relaxed);
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

/// Shows the figures of the session in the corner of the picture, or
/// takes them away.
///
/// For this session only, like the badges held up: the settings screen
/// says whether a session opens with them.
fn show_the_figures(app: &App) -> Result<(), String> {
    if !a_session_is_up(app) {
        return Err("aucune session en cours".to_string());
    }
    let shown = !app.floating().figures.fetch_xor(true, Ordering::Relaxed);
    note(if shown {
        "statistiques montrées sur l'image"
    } else {
        "statistiques retirées de l'image"
    });
    keep_up_with_the_picture(app);
    Ok(())
}

/// Gives the mouse to a game, or back to a desktop.
///
/// Two ends move together. This window reads movement from the device
/// in a game and places elsewhere, and hides its own pointer over the
/// picture; the far computer is asked to draw its own pointer into the
/// picture in a game, which is the only pointer a game shows, and to
/// stop on a desktop, where this computer draws it with no round trip
/// behind the hand.
fn change_the_mouse(app: &App) -> Result<(), String> {
    if !a_session_is_up(app) {
        return Err("aucune session en cours".to_string());
    }
    let game = !crate::video::in_a_game();
    crate::video::play_a_game(app, game);
    crate::session::ask_the_player(|settings, _| settings.absolute_mouse = !game);
    note(if game {
        "souris de jeu : le mouvement va à la session, l'ordinateur distant dessine son curseur"
    } else {
        "souris de bureau : la position va à la session, le curseur est dessiné ici"
    });
    Ok(())
}

/// Gives the system's keys to the session, or back to this computer, and
/// remembers where they were left: that is where the next session opens.
async fn change_the_keyboard() -> Result<(), String> {
    let theirs = !crate::system_keys::immersive();
    crate::system_keys::switch(theirs);
    crate::settings::remember_system_keys(theirs).await;
    Ok(())
}

/// Whether the session's sound is hushed on this computer.
pub fn hushed() -> Option<bool> {
    crate::session::player().map(|player| player.muted())
}

/// Hushes the session's sound on this computer, or gives it back.
///
/// Here and not over there. The far computer goes on playing whatever it
/// plays, and the person who asked is not asking for silence in a room
/// they are not in: they are asking for silence in theirs. Nothing else
/// playing here is touched.
fn hush_the_session() -> Result<(), String> {
    let player = crate::session::player().ok_or("aucune session en cours")?;
    let quiet = !player.muted();
    player.set_muted(quiet);
    note(if quiet {
        "son de la session coupé"
    } else {
        "son de la session rendu"
    });
    Ok(())
}

/// Does what the menu or a shortcut asks of the session.
pub async fn ask(app: &App, act: Act) -> Result<(), String> {
    match act {
        // Covering the screen is still a session matter: the shortcut is
        // registered with the system for the whole life of the program,
        // and without a session it would fullscreen the empty home screen
        // and write that down as the choice for the next session.
        Act::Fullscreen => {
            if !a_session_is_up(app) {
                return Err("aucune session en cours".to_string());
            }
            crate::picture::toggle_the_screen(app)
        }
        Act::Stats => show_the_figures(app),
        Act::MouseMode => change_the_mouse(app),
        Act::SecureAttention => press_ctrl_alt_del_over_there().await,
        Act::LockScreen => lock_over_there().await,
        Act::Sound => hush_the_session(),
        Act::SystemKeys => change_the_keyboard().await,
        Act::Clipboard => share_the_clipboard(app).await,
        Act::Badges => always_show_the_badges(app),
        Act::End => end_the_session(app),
    }
}

/// Whether the menu of the floating button is open right now.
///
/// What gives the pointer back to the desk while a hand is in it: a hand
/// reading the menu is aiming at something else than the picture.
fn the_menu_is_open() -> bool {
    #[cfg(windows)]
    {
        crate::menu::is_open()
    }
    #[cfg(not(windows))]
    {
        false
    }
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
async fn press_ctrl_alt_del_over_there() -> Result<(), String> {
    let way = the_way_of_this_session().await?;
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
async fn lock_over_there() -> Result<(), String> {
    let way = the_way_of_this_session().await?;
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
/// Asked of the service, which knows every session of this computer: the
/// one this program holds, and failing that the first.
async fn the_way_of_this_session() -> Result<zyr_control::WayId, String> {
    let sessions = crate::session::sessions().await;
    let ours = sessions
        .iter()
        .find(|session| session.process == std::process::id())
        .or_else(|| sessions.first())
        .ok_or("aucune session en cours")?;
    Ok(zyr_control::WayId(ours.way))
}

/// Ends the session: the player says goodbye to the far computer, and
/// the way closes behind it.
///
/// And before there is a player at all, there is an opening: a tunnel
/// being raced for, the far computer being asked for its screen. Closing
/// then is closing that, and it is said before anything else so the
/// opening reads it at its very next step.
fn end_the_session(app: &App) -> Result<(), String> {
    if !crate::session::opening() {
        return Err("aucune session en cours".to_string());
    }
    // Said before the player stops, and never taken back: a session
    // reported as broken to whoever just closed it would be a lie.
    Floating::closing(app, true);
    if crate::session::close() {
        note("fermeture de la session demandée");
    }
    Ok(())
}

/* ---- What belongs to Windows ----------------------------------------- */

/// Lays the button where the picture is now.
///
/// Called at every turn of the watch and at every step of a drag, so
/// nothing here waits for anything.
pub fn lay_the_button(picture: (i32, i32, i32, i32)) {
    let anchor = hung_from(picture, nudge(), logo(), margin());
    decide_the_direction(picture, anchor, menu_height());
    put_the_button(picture, anchor);
    // The badges and the figures follow the picture from here, and not
    // from a watch of their own: a picture being resized would carry each
    // of them off at its own rhythm.
    crate::badges::lay(the_other_corner(picture));
    crate::statistics::lay(the_bottom_corner(picture));
}

/// How tall the menu card is, which decides the direction it opens in.
#[cfg(windows)]
fn menu_height() -> i32 {
    crate::menu::height()
}

#[cfg(not(windows))]
fn menu_height() -> i32 {
    0
}

/// Lays the two windows of the button, and shows them or puts them away.
///
/// One anchor for both: that is what keeps them from disagreeing on
/// where the button is, and there is nothing left between what is wanted
/// and what is drawn, the page that ran a frame behind being gone.
#[cfg(windows)]
fn put_the_button(picture: (i32, i32, i32, i32), anchor: (i32, i32)) {
    let opens = Opens::from_number(OPENS.load(Ordering::Relaxed));
    let room_right = TO_THE_RIGHT.load(Ordering::Relaxed);
    // The logo only moves between two corners, the card no longer
    // starting from it when it opens beside: it keeps the corner it has
    // when the card is below. Its drawing, for its part, turns round
    // with the edge the card starts from, to face the menu rather than
    // turn its back on it.
    crate::logo::lay(anchor, opens == Opens::Up, room_right);
    crate::menu::lay(anchor, opens, room_right, crate::logo::box_side(), picture);

    // The system brings an owned window back up with the one that owns
    // it, which is right for a button that is only down because the
    // window is. What it does not decide is decided here: the button put
    // away by hand stays put away.
    let up = !HIDDEN.load(Ordering::Relaxed);
    if SHOWN.swap(up, Ordering::Relaxed) != up {
        note(&format!(
            "bouton flottant {}",
            if up { "montré" } else { "retiré" }
        ));
    }
    crate::logo::shown_now(up);
    if !up {
        crate::menu::show(false);
    }
}

/// What the button was showing the last time it was decided.
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
    // The logo and not the menu card: a picture is not too small for a
    // button because a closed menu would not fit in it.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_smaller_than_the_button_is_answered_and_not_refused() {
        // This runs inside the call the system makes to our window: a
        // panic there takes the whole program down. A window can change
        // size without a hand having asked for it, so the case must
        // have an answer.
        let button = (91, 91);
        for image in [
            (100, 100, 160, 134),
            (0, 0, 1, 1),
            (500, 500, 500, 500),
            (-50, -50, 10, 10),
        ] {
            let hung = hung_from(image, (0, 0), button, MARGIN);
            assert!(
                hung.0 >= image.0 && hung.0 <= image.2,
                "sur {image:?} : {hung:?}"
            );
            assert!(hung.1 >= image.1, "sur {image:?} : {hung:?}");
        }
    }

    #[test]
    fn the_button_hangs_in_the_top_right_corner_of_the_picture() {
        let image = (100, 200, 1_000, 800);
        let hung = hung_from(image, (0, 0), (91, 91), MARGIN);
        assert_eq!(hung, (1_000 - MARGIN, 200 + MARGIN));
    }

    #[test]
    fn a_button_dragged_past_an_edge_comes_back_against_it() {
        let image = (100, 200, 1_000, 800);
        let far = hung_from(image, (5_000, 5_000), (91, 91), MARGIN);
        assert_eq!(far, (1_000, 800 - 91));
        let before = hung_from(image, (-5_000, -5_000), (91, 91), MARGIN);
        assert_eq!(before, (100 + 91, 200));
    }

    #[test]
    fn a_menu_with_no_room_below_nor_above_opens_beside_the_button() {
        let image = (0, 0, 1_920, 1_080);
        let button = 91;
        ITS_LOGO.store(button, Ordering::Relaxed);
        let tall = 700;
        // The button at the top: the card fits below, where it reads
        // from the button.
        assert_eq!(where_it_opens(image, (1_904, 16), tall), Opens::Down);
        // At the bottom: it only fits above now.
        assert_eq!(where_it_opens(image, (1_904, 1_000), tall), Opens::Up);
        // Halfway up: neither, and that is where it was cut off by the
        // bottom of the picture.
        assert_eq!(where_it_opens(image, (1_904, 500), tall), Opens::Side);
        // A picture too short for it anyway: beside, it would be cut
        // off too, so it keeps the side with the most room left.
        let short = (0, 0, 1_920, 600);
        assert_eq!(where_it_opens(short, (1_904, 300), tall), Opens::Up);
        assert_eq!(where_it_opens(short, (1_904, 100), tall), Opens::Down);
    }

    #[test]
    fn a_menu_with_no_room_on_the_left_opens_on_the_right_of_the_button() {
        let image = (0, 0, 1_920, 1_080);
        ITS_LOGO.store(91, Ordering::Relaxed);
        let width = 320;
        // The button near the right edge of the picture: the card fits
        // to its left, where it usually opens.
        assert!(!opens_rightwards(image, (1_904, 16), width));
        // Near the left edge: it no longer fits there, and fits to
        // its right.
        assert!(opens_rightwards(image, (16, 16), width));
        // A picture too narrow for it on both sides: it keeps the side
        // with the most room left, for the same reason as the vertical
        // direction.
        let narrow = (0, 0, 200, 1_080);
        assert!(!opens_rightwards(narrow, (150, 16), width));
        assert!(opens_rightwards(narrow, (50, 16), width));
    }

    #[test]
    fn there_is_one_way_to_end_a_session_and_not_two() {
        // A session could be left with the far desktop open, or closed
        // with it handed back. Carrying that difference up to the person
        // would leave them a session neither running nor over. One single
        // line of the menu ends it, and it carries one single act.
        assert_eq!(Act::End.to_string(), "fin de la session");
    }

    #[test]
    fn the_figures_stand_in_the_corner_under_the_badges() {
        let image = (100, 200, 1_000, 800);
        assert_eq!(the_other_corner(image), (100 + MARGIN, 200 + MARGIN));
        assert_eq!(the_bottom_corner(image), (100 + MARGIN, 800 - MARGIN));
    }
}
