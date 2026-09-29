//! The floating button's menu, drawn by ZyrDesk.
//!
//! The card that opens under the logo, in a window of its own, made of
//! the same pixels as the logo: a picture that carries its own
//! transparency and is handed to Windows as it is. No cut, no background
//! to erase, no frame, and clicks go through wherever the picture is
//! clear.
//!
//! Everything that decides how it looks comes from the design system,
//! read from `design.css` at build time. Nothing is hard-coded here:
//! this module says where things go, never what colour they are.
//!
//! Lengths are written in page pixels, as in the design system, and `scale`
//! turns them into real pixels at drawing time. It is the same split as
//! everywhere else, and it is what lets a measurement be read here and
//! found again over there.
//!
//! The window follows the card: it is measured again at every drawing,
//! and the picture is handed to it together with its size. So there is
//! never a moment when it is large without being painted, which is what a
//! web view could not do and what forced it to be built once and for all
//! at its largest possible size.

mod gestures;
mod layout;
mod lines;
mod painting;
mod window;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::shell::app::App;

use crate::session::floating::{Act, Opens};
use crate::shell::settings::{Offered, SessionMenu};
use crate::shell::shortcuts::Doing;
use crate::shell::win32::pointer_in;
use zyr_draw::design::{self, Colour, Palette};
use zyr_draw::icons::{self, Icon};
use zyr_draw::{Align, Canvas, Pen, Rect};
use zyr_i18n::key;
use zyr_player::Measures;
use zyr_proto::fact::Fact;

// One card, in five parts that each know what the others hold: what is
// on it, where it falls, how it is drawn, what the hand does on it, and
// its window.
use gestures::*;
use layout::*;
use lines::*;
use painting::*;
use window::*;

pub use window::{height, is_open, lay, lower, raise, show, width};

/// What this module files its journal lines under.
const TAG: &str = "floating";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::shell::journal::note_about(TAG, what);
}

/// The card's window, and what it knows about itself.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static WIDTH: AtomicU32 = AtomicU32::new(0);
static HEIGHT: AtomicU32 = AtomicU32::new(0);
static OPEN: AtomicBool = AtomicBool::new(false);
static LIGHT: AtomicBool = AtomicBool::new(false);

/// What is under the mouse, and what a click started on.
///
/// Written by the window's answer, which the system calls, and read by
/// the drawing. Both run on the thread that owns the window, so these
/// locks are never fought over.
static HOVER: Mutex<Option<Target>> = Mutex::new(None);
static PRESSED: Mutex<Option<Target>> = Mutex::new(None);

/// Whether the mouse is in this window, so as to ask only once to be
/// told when it leaves.
static HAND_INSIDE: AtomicBool = AtomicBool::new(false);

/// What can be clicked, in the card or in the open panel.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    /// A line clicked as a whole, by its rank in `LINES`.
    Line(usize),
    /// A side of a switch or of a line of buttons: the rank of its line,
    /// and which of the sides.
    Side(usize, usize),
    /// The bar of a slider.
    Bar(usize),
    /// A value of the open panel, by its rank in the list.
    Value(usize),
}

impl Target {
    /// The card line in question, when it is one.
    fn line(self) -> Option<usize> {
        match self {
            Target::Line(rank) | Target::Side(rank, _) | Target::Bar(rank) => Some(rank),
            Target::Value(_) => None,
        }
    }
}

static READINGS_BAR: Mutex<ReadingsBar> = Mutex::new(ReadingsBar::empty());

/// The round of the readings watch.
///
/// It changes at every opening and every closing, which stops the
/// previous round: without that, opening and closing quickly would
/// leave two watches behind the same card.
static ROUND: AtomicU32 = AtomicU32::new(0);

/// Where each of the switches stands.
///
/// Read again at every opening of the card rather than remembered, from
/// what each of them really is: a switch that shows what it believes
/// rather than what is, is a switch nobody believes twice.
static IN_GAME: AtomicBool = AtomicBool::new(false);
static MUTED: AtomicBool = AtomicBool::new(false);
static IMMERSIVE: AtomicBool = AtomicBool::new(false);
static SHARED: AtomicBool = AtomicBool::new(false);
static HELD: AtomicBool = AtomicBool::new(false);

/// How many real pixels a page pixel is worth.
static SCALE: AtomicU32 = AtomicU32::new(100);

/// The height of a caption line and of a body line, in real pixels.
///
/// It is not the size of the type: a twelve-pixel line takes up about
/// sixteen, the space above and below being what the font asks for.
/// Stacking text by its size rather than its height squeezes everything
/// stacked, and that is what made the readings bar more cramped than the
/// page's.
///
/// Measured once, when the card is: they depend only on the text size and
/// the screen's magnification, neither of which moves during a session.
static CAPTION_HEIGHT: AtomicU32 = AtomicU32::new(0);
static BODY_HEIGHT: AtomicU32 = AtomicU32::new(0);

/// Which way the menu opens, and so which edge of its window the card
/// is stuck to.
static UPWARD: AtomicBool = AtomicBool::new(false);

/// Whether the card is stuck to the left edge of its window rather
/// than the right, and the panel to its right rather than its left:
/// decided by the button when its right edge does not have the room to
/// carry the card.
static RIGHTWARD: AtomicBool = AtomicBool::new(false);

/// How wide the card is, measured over all its lines.
static CARD_WIDTH: AtomicU32 = AtomicU32::new(0);

/// What the menu has just refused to do, and since when.
///
/// Said on the card, in red: a switch that rightly refuses and simply
/// does not flip is a broken switch, even when it is perfectly right.
static REFUSAL: Mutex<Option<(Fact, Instant)>> = Mutex::new(None);

/// How tall this refusal is, measured when drawing, as the card is.
static REFUSAL_HEIGHT: AtomicU32 = AtomicU32::new(0);

/// How long a refusal stays on the card.
///
/// Long, because it carries what there is to do elsewhere, and it is
/// elsewhere that one goes off to do it: a refusal wiped while the
/// Windows page is being read would be a refusal never read.
const REFUSAL_TIME: Duration = Duration::from_secs(20);

/// What there is to say about a refusal, while it is fresh.
///
/// One that has had its time is forgotten on the way: the card opens
/// again often, and a refusal from an hour ago would read like the one
/// for the click just made.
fn refusal_to_say() -> Option<String> {
    let mut refusal = REFUSAL.lock().expect("menu's refusal");
    if refusal
        .as_ref()
        .is_some_and(|(_, since)| since.elapsed() >= REFUSAL_TIME)
    {
        *refusal = None;
    }
    refusal.as_ref().map(|(why, _)| zyr_i18n::fact(why))
}

/// The round of the settings watch, which stops the
/// previous one.
static SESSION_MENU_ROUND: AtomicU32 = AtomicU32::new(0);

/// What the session offers and where it stands, asked for when the card
/// opens.
///
/// Asked for in one go rather than one list at a time: the card is
/// measured on what it holds, so it needs everything before laying
/// anything down.
static SESSION_MENU: Mutex<Option<SessionMenu>> = Mutex::new(None);

/// The open submenu, or nothing.
static PANEL: Mutex<Option<Setting>> = Mutex::new(None);

/// The notch a hand has pushed the bitrate slider to: while it holds the
/// slider, and after it lets go, until the session has answered.
///
/// What is chosen is only written on release: a slider pushed from one
/// end to the other crosses dozens of notches, and each of them would be
/// a round trip to the service for a bitrate nobody wanted.
///
/// And it stays through that round trip. The thumb goes back to what is in
/// force only once the session has said what is in force; sent back sooner,
/// it flashed to its old place at every release.
static PUSHED: Mutex<Option<usize>> = Mutex::new(None);

/// The program, for the places the system calls and that the toolkit
/// gives nothing to.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

/// The combinations in place, read when the session opens.
///
/// Read and not set in stone: they are chosen in the settings. Read once,
/// because the card takes the width of its longest line and that width is
/// its window's, which does not change size from one session to the next.
static KEYS: Mutex<Vec<(Doing, Option<String>)>> = Mutex::new(Vec::new());

// This window's canvas, held by the thread that owns it: a drawing
// surface and the window it dresses belong to the thread that made
// them.
thread_local! {
    static CANVAS: std::cell::RefCell<Option<Canvas>> = const { std::cell::RefCell::new(None) };
}

/// A length stored in a shared integer, in hundredths of a pixel: a
/// decimal number cannot be stored there, and a hundredth is enough for a
/// screen magnified to a hundred and seventy-five per cent.
fn store(cell: &AtomicU32, how_many: f32) {
    cell.store((how_many * 100.0).round() as u32, Ordering::Relaxed);
}

fn load(cell: &AtomicU32) -> f32 {
    cell.load(Ordering::Relaxed) as f32 / 100.0
}

fn scale() -> f32 {
    load(&SCALE)
}

fn palette() -> Palette {
    design::palette(LIGHT.load(Ordering::Relaxed))
}
