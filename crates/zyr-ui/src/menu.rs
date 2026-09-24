//! The floating button's menu, drawn by ZyrDesk.
//!
//! The card that opens under the logo, in a window of its own, made of
//! the same pixels as the logo: a picture that carries its own
//! transparency and is handed to Windows as it is. No cut, no background
//! to erase, no frame, and clicks go through wherever the picture is
//! clear.
//!
//! Everything that decides how it looks comes from the design system,
//! read from the stylesheet at build time. Nothing is hard-coded here:
//! this file says where things go, never what colour they are.
//!
//! Lengths are written in page pixels, as in the stylesheet, and `scale`
//! turns them into real pixels at drawing time. It is the same split as
//! everywhere else, and it is what lets a measurement be read here and
//! found again over there.
//!
//! The window follows the card: it is measured again at every drawing,
//! and the picture is handed to it together with its size. So there is
//! never a moment when it is large without being painted, which is what a
//! web view could not do and what forced it to be built once and for all
//! at its largest possible size.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::app::App;

use crate::design::{self, Colour, Palette};
use crate::floating::{Act, Opens};
use crate::icons;
use crate::measures::Measures;
use crate::paint::{Align, Canvas, Icon, Pen, Rect};
use crate::settings::{Offered, SessionMenu};
use crate::shortcuts::Doing;

/// What this module files its journal lines under.
const TAG: &str = "floating";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// A line of the card.
///
/// The card is described first and drawn afterwards: measuring its width
/// needs all of its lines known before a single one is laid down, and a
/// card as wide as its longest line is what the stylesheet has always
/// asked for.
enum Line {
    /// What the session costs: four numbers and a sentence.
    Measures,
    /// A stroke between two groups.
    Separator,
    /// What the menu has just refused to do, and why.
    Refusal,
    /// A line that is clicked, as the page calls them.
    Entry(Entry),
    /// A line that carries a choice between two sides.
    Toggle(Toggle),
    /// A line that carries a few values side by side.
    Choice(Choice),
    /// A line that is pushed along a bar.
    Slider(Slider),
    /// A line that opens a list of its own.
    List(List),
}

/// A line that carries a few values with no order between them.
///
/// Buttons and not a bar: the codec is not a scale, it is a few names,
/// one of them an "Automatique" that is not a value but a renunciation,
/// and pushing a slider would promise a more and a less that do not
/// exist.
struct Choice {
    icon: &'static Icon,
    label: &'static str,
    setting: Setting,
}

/// A line set by pushing a slider, with the value written above it.
///
/// A scale: bigger, faster, and the right notch on it is found by
/// watching the picture move. The notches come from the product, one per
/// megabit, and the slider goes from zero to the number of values minus
/// one: it pushes ranks and not numbers, like the other list lines.
struct Slider {
    icon: &'static Icon,
    label: &'static str,
    setting: Setting,
}

/// A line that opens a list of its own, beside the card.
///
/// A list rather than a bar, for two reasons: its first entries are
/// not numbers but say which of the two computers decides, which no
/// bar can say, and there are fifteen of them below, which makes
/// notches one can no longer aim at.
struct List {
    icon: &'static Icon,
    label: &'static str,
    setting: Setting,
}

/// A line that carries a choice rather than an action.
///
/// Both words are there and the one in place is lit: the old line
/// announced what the click would do and never where things stood, and
/// the two modes cannot be told apart by eye on a still desktop. It has
/// to be seen without reading.
///
/// The line itself is not clicked, only its two sides: no hand under the
/// pointer and no lit background over the rest, which would promise a
/// click that does nothing.
struct Toggle {
    icon: &'static Icon,
    label: &'static str,
    /// The two sides, in the order they are read. The second is the one
    /// that stands for "yes".
    sides: [&'static str; 2],
    /// What the session is asked for to switch sides.
    act: Act,
    /// Where things stand: true for the right-hand side.
    state: &'static AtomicBool,
}

/// An entry of the menu: an icon, a word, what is written to its right,
/// and what it asks for.
struct Entry {
    icon: &'static Icon,
    label: &'static str,
    trailing: Trailing,
    does: Does,
    /// Written in the colour of things that cannot be undone. Only one
    /// line of the menu is, and it is the one that cuts the session off.
    destructive: bool,
}

/// What is written to the right of a line.
enum Trailing {
    /// What the line does, spelled out.
    Text(&'static str),
    /// The combination in place for it, or this word here as long as
    /// nobody has given it one.
    Key(Doing, &'static str),
}

/// What a line asks for when it is clicked.
#[derive(Clone, Copy)]
enum Does {
    /// What the session can do, in its own language.
    Session(Act),
    /// Put the button away until the shortcut calls it back.
    PutAway,
}

/// One of the settings the session carries.
///
/// Named here as the product names it on both sides: that is the word
/// that travels to the service, and having a second one for display
/// would be two names for one setting.
#[derive(Clone, Copy, PartialEq)]
enum Setting {
    Size,
    Screen,
    Bitrate,
    Codec,
    Steady,
}

/// What the card holds, in order.
///
/// The same lines as the page, in the same order, with the same words,
/// the same icons and the same actions. What is still missing is said in
/// the journal on opening rather than replaced by an empty space that
/// would look like a fault.
const LINES: [Line; 21] = [
    Line::Measures,
    // Just under the readings, so at the top of what is read: what has
    // just been refused is read before whatever was going to be clicked
    // next. It takes no room at all while there is nothing to say.
    Line::Refusal,
    Line::Separator,
    Line::Entry(Entry {
        icon: &icons::FULL_SCREEN,
        label: "Fenêtré ou plein écran",
        trailing: Trailing::Key(Doing::Fullscreen, ""),
        does: Does::Session(Act::Fullscreen),
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::STATISTICS,
        label: "Statistiques",
        trailing: Trailing::Text("Ctrl+Alt+Maj+S"),
        does: Does::Session(Act::Stats),
        destructive: false,
    }),
    Line::Toggle(Toggle {
        icon: &icons::LINK,
        label: "Voyants",
        sides: ["Au besoin", "Tenus"],
        act: Act::Badges,
        state: &HELD,
    }),
    Line::Toggle(Toggle {
        icon: &icons::MOUSE,
        label: "Souris",
        sides: ["Bureau", "Jeu"],
        act: Act::MouseMode,
        state: &IN_GAME,
    }),
    Line::Toggle(Toggle {
        icon: &icons::SOUND,
        label: "Son",
        sides: ["Actif", "Coupé"],
        act: Act::Sound,
        state: &MUTED,
    }),
    Line::Toggle(Toggle {
        icon: &icons::KEYBOARD,
        label: "Clavier",
        sides: ["Partagé", "Immersif"],
        act: Act::SystemKeys,
        state: &IMMERSIVE,
    }),
    Line::Toggle(Toggle {
        icon: &icons::CLIPBOARD,
        label: "Presse-papiers",
        sides: ["Chacun le sien", "Partagé"],
        act: Act::Clipboard,
        state: &SHARED,
    }),
    Line::Entry(Entry {
        icon: &icons::CAD,
        label: "Ctrl+Alt+Suppr",
        trailing: Trailing::Text("sur l'ordinateur distant"),
        does: Does::Session(Act::SecureAttention),
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::LOCK,
        label: "Verrouiller",
        trailing: Trailing::Text("l'ordinateur distant"),
        does: Does::Session(Act::LockScreen),
        destructive: false,
    }),
    Line::Separator,
    Line::List(List {
        icon: &icons::RESOLUTION,
        label: "Résolution",
        setting: Setting::Size,
    }),
    Line::List(List {
        icon: &icons::HOST_SCREEN,
        label: "Écran de l'hôte",
        setting: Setting::Screen,
    }),
    Line::Slider(Slider {
        icon: &icons::BITRATE,
        label: "Débit",
        setting: Setting::Bitrate,
    }),
    Line::Choice(Choice {
        icon: &icons::CODEC,
        label: "Codec",
        setting: Setting::Codec,
    }),
    Line::Choice(Choice {
        icon: &icons::FAR_SCREEN,
        label: "Écran d'en face",
        setting: Setting::Steady,
    }),
    Line::Separator,
    Line::Entry(Entry {
        icon: &icons::HIDE,
        label: "Masquer ce bouton",
        trailing: Trailing::Key(Doing::Menu, "jusqu'à la fin"),
        does: Does::PutAway,
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::QUIT,
        label: "Terminer la session",
        trailing: Trailing::Key(Doing::End, "rend le bureau distant"),
        does: Does::Session(Act::End),
        destructive: true,
    }),
];

/// What the stylesheet says about a line, in page pixels.
mod layout {
    /// The height a line never goes below.
    pub const LINE: f32 = 38.0;
    /// An icon's side, and the gap between it and the word.
    pub const ICON: f32 = 18.0;
    /// What separates the word from what is written to its
    /// right.
    pub const AFTER_THE_LABEL: f32 = 24.0;
    /// The thickness of a separating stroke, and that of a border.
    pub const HAIRLINE: f32 = 1.0;
    /// The width each reading keeps whatever its number, so that the bar
    /// does not breathe in time with the figures.
    pub const READING: f32 = 78.0;
    /// What separates two readings, and what separates their word
    /// from their number.
    pub const BETWEEN_READINGS: f32 = 16.0;
    pub const UNDER_THE_LABEL: f32 = 2.0;
    /// The height of a switch: its caption, what surrounds it above
    /// and below, and its border. The page gets it from the browser's
    /// line height, which does not exist here: so it is stated.
    pub const TOGGLE: f32 = 24.0;
    /// The room a slider takes, its thumb included.
    pub const SLIDER: f32 = 18.0;
    /// The thickness of a slider's bar, and the side of its thumb.
    pub const BAR: f32 = 4.0;
    pub const THUMB: f32 = 14.0;
    /// The side of a chevron and of a tick: smaller than a line's icon,
    /// because they are marks and not drawings.
    pub const BRAND: f32 = 16.0;
}

/// One of the bar's four figures: what it costs, how it reads, and
/// where it is taken from in what the engine writes.
struct Reading {
    label: &'static str,
    unit: &'static str,
    /// How many decimals: the network reads in whole milliseconds, the
    /// rest to the hundredth.
    decimals: usize,
    read: fn(&Measures) -> Option<f64>,
}

/// The four readings, in the order they are read: what a frame costs
/// here, what it cost over there, what lies between the two, and what the
/// wire really carries.
///
/// The same words and the same units as the page, because they are the
/// same readings: inventing them here would give four others, and two
/// bars that do not say the same thing about the same engine.
const READINGS: [Reading; 4] = [
    Reading {
        label: "Décodage",
        unit: "ms",
        decimals: 2,
        read: |said| said.decode_ms,
    },
    Reading {
        label: "Encodage",
        unit: "ms",
        decimals: 2,
        read: |said| said.host_ms,
    },
    Reading {
        label: "Réseau",
        unit: "ms",
        decimals: 0,
        read: |said| said.network_ms,
    },
    Reading {
        label: "Débit",
        unit: "Mb/s",
        decimals: 2,
        read: |said| said.bitrate_mbps,
    },
];

/// What a reading shows while it has nothing to say.
///
/// The engine says nothing rather than zero when it has measured
/// nothing, and zero would be a lie: a second with no frame decoded
/// does not have a zero decoding time.
const NO_READING: &str = "-";

/// How long a missing reading keeps what it was saying.
///
/// One of these four is sometimes missing for one second and back the
/// next: what the far computer measures does not travel with every
/// frame, and a second can go by without any frame carrying it. Wiped at
/// once, the reading flickers between a number and a dash, and a
/// flickering number is harder to read than a number one second late,
/// which is what is being read anyway, since these four are averages
/// over the second just gone.
///
/// Three seconds and no more: beyond that it is no longer a reading that
/// skipped but a measurement that no longer exists, and the dash then
/// tells the truth.
const KEEP_FOR: std::time::Duration = std::time::Duration::from_secs(3);

/// The engine's pulse, which writes once a second. Asking more often
/// would reread the same file for the same number.
const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// How much a colour tints the background when it serves as a hover:
/// what the stylesheet writes as `color-mix(in srgb, ... 12%,
/// transparent)`.
const VEIL: f32 = 0.12;

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

/// What the readings bar shows: four numbers already written out and the
/// stream's sentence.
///
/// Written out where they are read rather than kept as numbers: the
/// formatting then happens once a second and not once a frame, and the
/// drawing thread only has to lay down text.
struct ReadingsBar {
    figures: [String; 4],
    /// When each one was really read, and not copied over from the
    /// reading before. Left out of every comparison: these instants move
    /// at every turn without anything reading differently.
    read_at: [Option<Instant>; 4],
    stream: String,
}

static READINGS_BAR: Mutex<ReadingsBar> = Mutex::new(ReadingsBar::empty());

/// The round of the readings watch.
///
/// It changes at every opening and every closing, which stops the
/// previous round: without that, opening and closing quickly would
/// leave two watches behind the same card.
static ROUND: AtomicU32 = AtomicU32::new(0);

/// Where each of the six switches stands.
///
/// Read again at every opening of the card rather than remembered: the
/// product's shortcut flips the mouse, and the Windows mixer is open to
/// everyone. A switch that shows what it believes rather than what is, is
/// a switch nobody believes twice.
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
/// The web view's menu carried a red line for that. The one ZyrDesk draws
/// had not carried it over, so a refusal only went to the journal: a
/// switch that rightly refuses and simply does not flip is a broken
/// switch, even when it is perfectly right.
static REFUSAL: Mutex<Option<(String, Instant)>> = Mutex::new(None);

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
    let mut refusal = REFUSAL.lock().expect("refus du menu");
    if refusal
        .as_ref()
        .is_some_and(|(_, since)| since.elapsed() >= REFUSAL_TIME)
    {
        *refusal = None;
    }
    refusal.as_ref().map(|(said, _)| said.clone())
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

/// The notch where a hand holds the bitrate slider, while it holds it.
///
/// What is chosen is only written on release: a slider pushed from one
/// end to the other crosses fifteen notches, and each of them would be a
/// round trip to the service for a bitrate nobody wanted.
static PUSHED: Mutex<Option<usize>> = Mutex::new(None);

/// The program, for the places the system calls and that the toolkit
/// gives nothing to.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

/// The combinations in place, read when the session opens.
///
/// Read and not set in stone: they are chosen in the settings. Read once,
/// because the card takes the width of its longest line and that width is
/// its window's, which does not change size from one session to the next.
/// It is the moment the page chooses too.
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

impl ReadingsBar {
    const fn empty() -> Self {
        ReadingsBar {
            figures: [String::new(), String::new(), String::new(), String::new()],
            read_at: [None; 4],
            stream: String::new(),
        }
    }

    /// What a read of the engine shows, with the previous one at hand.
    ///
    /// The previous one because a missing reading keeps what it was
    /// saying for a while rather than being wiped; see `KEEP_FOR`.
    fn of(readings: &Measures, before: &ReadingsBar, now: Instant) -> Self {
        let mut figures: [String; 4] = std::array::from_fn(|_| String::new());
        let mut read_at = [None; 4];
        for (rank, reading) in READINGS.iter().enumerate() {
            if let Some(number) = (reading.read)(readings) {
                figures[rank] = format!("{number:.*} {}", reading.decimals, reading.unit);
                read_at[rank] = Some(now);
                continue;
            }
            match before.read_at[rank] {
                Some(when) if now.duration_since(when) < KEEP_FOR => {
                    figures[rank].clone_from(&before.figures[rank]);
                    read_at[rank] = Some(when);
                }
                _ => figures[rank] = NO_READING.to_string(),
            }
        }
        ReadingsBar {
            figures,
            read_at,
            stream: stream(readings),
        }
    }

    /// Whether what is read has changed, leaving the
    /// instants aside.
    fn reads_differently(&self, other: &ReadingsBar) -> bool {
        self.figures != other.figures || self.stream != other.stream
    }
}

impl Setting {
    /// The name it travels under, on both sides.
    fn name(self) -> &'static str {
        match self {
            Setting::Size => "asked",
            Setting::Screen => "screen",
            Setting::Bitrate => "bitrate",
            Setting::Codec => "codec",
            Setting::Steady => "steady",
        }
    }

    /// The values on offer, in the product's order.
    fn values(self, menu: &SessionMenu) -> Vec<String> {
        match self {
            Setting::Size => menu.sizes.iter().map(|size| size.value.clone()).collect(),
            Setting::Screen => menu
                .screens
                .iter()
                .map(|screen| screen.id.clone())
                .collect(),
            Setting::Bitrate => menu.rates.iter().map(u32::to_string).collect(),
            Setting::Codec => menu.codecs.clone(),
            // Two words and not a list: it is a switch, and its two
            // sides are named in the window like the ones next to it.
            Setting::Steady => vec!["off".to_string(), "on".to_string()],
        }
    }

    /// What is written for this value, where it is chosen.
    fn label(self, menu: &SessionMenu, value: &str) -> String {
        match self {
            Setting::Size => match value {
                "client" => "Résolution du client".to_string(),
                "host" => "Résolution de l'hôte".to_string(),
                _ => menu
                    .sizes
                    .iter()
                    .find(|size| size.value == value)
                    .map_or_else(|| value.to_string(), in_pixels),
            },
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == value)
                .map_or_else(
                    || value.to_string(),
                    |screen| {
                        if screen.main {
                            format!("{} (principal)", screen.name)
                        } else {
                            screen.name.clone()
                        }
                    },
                ),
            Setting::Bitrate => format!(
                "{} Mb/s",
                (value.parse::<f64>().unwrap_or(0.0) / 1000.0).round()
            ),
            Setting::Codec => {
                if value == "auto" {
                    "Automatique".to_string()
                } else {
                    value.to_string()
                }
            }
            Setting::Steady => if value == "on" { "Fluide" } else { "Économe" }.to_string(),
        }
    }

    /// What is written to the right of the menu line, when the value
    /// in place is not already read there.
    fn summary(self, menu: &SessionMenu) -> String {
        let current = self.current(menu);
        match self {
            // What the choice really comes to here: "client" does not say
            // whether 4K or 1080p is being asked for, and that is exactly
            // what one wants to know before opening the session.
            Setting::Size => {
                if current == "host" {
                    return "hôte".to_string();
                }
                let pixels = menu
                    .sizes
                    .iter()
                    .find(|size| size.value == current)
                    .map_or_else(|| current.clone(), in_pixels);
                if current == "client" {
                    format!("client, {pixels}")
                } else {
                    pixels
                }
            }
            // The name alone: "(principal)" would take the name's room
            // there without teaching anything, since the list already
            // says it.
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == current)
                .map_or_else(String::new, |screen| screen.name.clone()),
            _ => self.label(menu, &current),
        }
    }

    /// What is written in the list's right-hand column.
    fn aside(self, menu: &SessionMenu, value: &str) -> String {
        match self {
            // The size's ratio, said the way screens are sold: two
            // numbers compare badly, and 21:9 next to 16:9 says at once
            // what is going to be cut. Nothing for the first two: what
            // they come to depends on the screen one is facing.
            Setting::Size if value != "client" && value != "host" => menu
                .sizes
                .iter()
                .find(|size| size.value == value)
                .filter(|size| size.width > 0)
                .map_or_else(String::new, |size| ratio(size.width, size.height)),
            // The screen's size, as the ratio is for the resolution: two
            // screens are told apart by that first, and a model name
            // says nothing to anyone who did not buy it.
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == value)
                .map_or_else(String::new, |screen| {
                    format!("{}x{}", screen.wide, screen.high)
                }),
            _ => String::new(),
        }
    }

    /// Where things stand.
    fn current(self, menu: &SessionMenu) -> String {
        match self {
            Setting::Size => menu.now.asked.clone(),
            Setting::Screen => menu.now.screen.clone(),
            Setting::Bitrate => menu.now.bitrate_kbps.to_string(),
            Setting::Codec => menu.now.codec.clone(),
            Setting::Steady => if menu.now.steady { "on" } else { "off" }.to_string(),
        }
    }

    /// What the far machine said it cannot do.
    ///
    /// Nothing at all means it has said nothing, never that it can do
    /// nothing: outside a session, or while its engine is starting, the
    /// question has no answer, and a question with no answer must leave
    /// the menu exactly as it was.
    fn out_of_reach(self, menu: &SessionMenu, value: &str) -> bool {
        self == Setting::Codec && menu.beyond_it.iter().any(|other| other == value)
    }
}

/// A size, in pixels.
fn in_pixels(size: &Offered) -> String {
    format!("{}x{}", size.width, size.height)
}

/// A size's ratio, reduced as it reads on a screen's spec sheet.
///
/// Worked out rather than written next to each number: a second table
/// would drift from the first the day a size is added. The two ratios
/// nobody writes in their reduced form are said the way everybody says
/// them.
fn ratio(width: u32, top: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }

    let divisor = gcd(width, top).max(1);
    match (width / divisor, top / divisor) {
        (8, 5) => "16:10".to_string(),
        (683, 384) => "16:9".to_string(),
        (x, y) => format!("{x}:{y}"),
    }
}

/// The grey line under the figures: what the picture is made of. What
/// is missing leaves no gap, it is not written.
fn stream(said: &Measures) -> String {
    let mut pieces: Vec<String> = Vec::new();
    if let Some(codec) = &said.codec {
        pieces.push(codec.clone());
    }
    if let (Some(width), Some(height)) = (said.width, said.height) {
        pieces.push(format!("{width}x{height}"));
    }
    if let Some(frames) = said.fps {
        pieces.push(format!("{frames:.0} images/s"));
    }
    pieces.join(" · ")
}

impl Trailing {
    /// What is written, once the shortcuts are known.
    fn text(&self) -> String {
        match self {
            Trailing::Text(label) => (*label).to_string(),
            Trailing::Key(doing, otherwise) => KEYS
                .lock()
                .expect("raccourcis du menu")
                .iter()
                .find(|(other, _)| other == doing)
                .and_then(|(_, said)| said.clone())
                .unwrap_or_else(|| (*otherwise).to_string()),
        }
    }
}

impl Line {
    /// The height this line takes, in real pixels.
    fn height(&self, scale: f32) -> f32 {
        match self {
            Line::Measures => readings_height(scale),
            Line::Separator => (design::SPACE_2 * 2.0 + layout::HAIRLINE) * scale,
            // Measured when drawing, where there is what it takes to
            // measure wrapped text, and read back here as the card's
            // width is.
            Line::Refusal => load(&REFUSAL_HEIGHT),
            Line::Slider(_) => slider_height(scale),
            _ => layout::LINE * scale,
        }
    }

    /// Whether this line has a reason to be there right now.
    ///
    /// A far machine with only one screen, or whose engine has not yet
    /// said which ones, leaves nothing to choose: the line goes away
    /// rather than open an empty list.
    fn is_visible(&self, menu: Option<&SessionMenu>) -> bool {
        // With no refusal to say, the line is not there at all: it must
        // cost nothing the nine hundred and ninety-nine times when all
        // goes well.
        if matches!(self, Line::Refusal) {
            return refusal_to_say().is_some();
        }
        let Some(menu) = menu else {
            // With no answer, the card shrinks to what does not depend
            // on the session: a short card is better than a card of
            // empty lines.
            return !matches!(self, Line::Choice(_) | Line::Slider(_) | Line::List(_));
        };
        match self {
            Line::List(list) => !list.setting.values(menu).is_empty(),
            _ => true,
        }
    }
}

impl Toggle {
    /// What is written on its two sides.
    fn words(&self) -> Vec<String> {
        self.sides
            .iter()
            .map(|label| (*label).to_string())
            .collect()
    }

    /// Which of the two is in place.
    fn current_side(&self) -> usize {
        usize::from(self.state.load(Ordering::Relaxed))
    }
}

/// What is written on the sides of a choice line.
///
/// Apart from the line so that it can be asked for with the settings
/// already in hand: asking for them again at that moment would take
/// again a lock that is already held.
fn words_of(menu: &SessionMenu, setting: Setting) -> Vec<String> {
    setting
        .values(menu)
        .iter()
        .map(|value| setting.label(menu, value))
        .collect()
}

impl Choice {
    /// What is written on its sides, as the session offers them.
    fn words(&self) -> Option<Vec<String>> {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        Some(words_of(session_menu.as_ref()?, self.setting))
    }

    /// Which one is in place, and the ones the far machine cannot do.
    fn current(&self) -> Option<(usize, Vec<bool>)> {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let menu = session_menu.as_ref()?;
        let values = self.setting.values(menu);
        let current = self.setting.current(menu);
        Some((
            values.iter().position(|value| *value == current)?,
            values
                .iter()
                .map(|value| self.setting.out_of_reach(menu, value))
                .collect(),
        ))
    }
}

impl Slider {
    /// The notch it is at: the one a hand is holding, otherwise the one
    /// that is written.
    fn notch(&self) -> Option<(usize, usize)> {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let menu = session_menu.as_ref()?;
        let values = self.setting.values(menu);
        if values.is_empty() {
            return None;
        }
        let current = self.setting.current(menu);
        let written = values
            .iter()
            .position(|value| *value == current)
            .unwrap_or(0);
        let pushed = *PUSHED.lock().expect("curseur du menu");
        Some((
            pushed.unwrap_or(written).min(values.len() - 1),
            values.len(),
        ))
    }

    /// What is written to the right of its word: what it is worth at
    /// the notch it is at, including while a hand is pushing it.
    fn value(&self) -> String {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let Some(menu) = session_menu.as_ref() else {
            return String::new();
        };
        let values = self.setting.values(menu);
        match *PUSHED.lock().expect("curseur du menu") {
            Some(notch) if notch < values.len() => self.setting.label(menu, &values[notch]),
            _ => self.setting.summary(menu),
        }
    }
}

/// Opens the card's window, once per session.
///
/// Built on the drawing thread, like the logo's: a window belongs to the
/// thread that made it, and a window made on the watch's thread would
/// never hear a mouse.
pub fn raise(app: &App, scale: f32, light: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = crate::main_window::handle();
    *PROGRAM.lock().expect("programme du menu") = Some(app.clone());
    *KEYS.lock().expect("raccourcis du menu") = crate::shortcuts::engraved();
    // Four dashes before the first read, and not four blanks: the bar is
    // there from the first opening, and what it shows then is what the
    // product shows for a missing reading.
    *READINGS_BAR.lock().expect("mesures du menu") =
        ReadingsBar::of(&Measures::default(), &ReadingsBar::empty(), Instant::now());
    store(&SCALE, scale);
    LIGHT.store(light, Ordering::Relaxed);
    OPEN.store(false, Ordering::Relaxed);
    *PANEL.lock().expect("panneau du menu") = None;
    let _ = app.run_on_main_thread(move || build(owner));
    // What the session offers, asked for once: the notches do not change
    // from one click to the next. The window is built without waiting,
    // because a closed card has nothing to show and the answer will catch
    // up with it before the first opening.
    reread_the_session_menu(app);
}

/// Asks again what the session offers and where it stands, and starts over
/// as long as the far machine has not said what it can encode.
///
/// It takes a few seconds to say so: its engine starts, then the road
/// starts serving the session. Asked only once when the button opened, the
/// question always came before that, and the menu opened offering a codec
/// that particular machine cannot do; it only corrected itself once the
/// card was already in front of the eyes, which shows.
///
/// A numbered round, as for the readings: two openings close together do
/// not leave two watches behind the same card.
fn reread_the_session_menu(app: &App) {
    let app = app.clone();
    let round = SESSION_MENU_ROUND.fetch_add(1, Ordering::Relaxed) + 1;
    crate::app::spawn(async move {
        while SESSION_MENU_ROUND.load(Ordering::Relaxed) == round
            && ITS_WINDOW.load(Ordering::Relaxed) != 0
        {
            let read = crate::settings::session_menu(app.clone()).await;
            // Nothing at all means it has said nothing, never that it can
            // do nothing: so it is on that, and nowhere else, that the
            // question is asked again.
            let answered = !read.beyond_it.is_empty();
            let change = {
                let mut session_menu = SESSION_MENU.lock().expect("réglages du menu");
                let change = session_menu.as_ref() != Some(&read);
                *session_menu = Some(read);
                change
            };
            if change {
                redraw(&app);
            }
            if answered {
                return;
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Closes the card and returns its window with the session.
pub fn lower(app: &App) {
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    OPEN.store(false, Ordering::Relaxed);
    // The readings watch does not put itself away: it follows the
    // card, and a card open at the end of a session does not close, it
    // disappears.
    follow_the_readings(app, false);
    *PROGRAM.lock().expect("programme du menu") = None;
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: a window of ours, destroyed on the thread that made
        // it.
        unsafe { DestroyWindow(window as HWND) };
        CANVAS.with_borrow_mut(|canvas| *canvas = None);
    });
}

/// Shows the card, or puts it away.
pub fn show(is_open: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) == 0 || OPEN.swap(is_open, Ordering::Relaxed) == is_open {
        return;
    }
    // A card put away keeps nothing of the hand that was reading it:
    // opened again, it would show a line lit under a mouse resting
    // elsewhere.
    *HOVER.lock().expect("survol du menu") = None;
    *PRESSED.lock().expect("appui du menu") = None;
    HAND_INSIDE.store(false, Ordering::Relaxed);
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    // A menu opened again opens on itself: staying in a list chosen two
    // sessions ago would be a menu that looks like another one.
    *PANEL.lock().expect("panneau du menu") = None;
    *PUSHED.lock().expect("curseur du menu") = None;
    // What lives in the card only lives while it is being looked at. The
    // switches and the settings are read again at every opening because
    // they may have moved without it.
    follow_the_readings(&app, is_open);
    if is_open {
        let asked = app.clone();
        crate::app::spawn(async move { reread_the_toggles(&asked).await });
        reread_the_session_menu(&app);
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if window.is_null() {
            return;
        }
        if is_open {
            repaint(window);
        }
        // SAFETY: a window of ours, shown without taking the
        // foreground.
        unsafe {
            ShowWindow(window, if is_open { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    });
}

/// Says whether the card is open, for whoever needs to toggle it.
pub fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

/// How tall its window is.
///
/// For the button, which uses it to decide whether the menu has
/// room to open downwards.
pub fn height() -> i32 {
    HEIGHT.load(Ordering::Relaxed) as i32
}

/// How wide its window is in all, for that vertical direction.
///
/// Sideways, it also counts the button and the space between them: it
/// is its whole window that is laid beside the button, never its card
/// alone, see `lay`. For the button, which uses it to decide from
/// which edge there is room to send it off.
pub fn width(opens: Opens, logo: i32) -> i32 {
    let width = WIDTH.load(Ordering::Relaxed) as i32;
    match opens {
        Opens::Side => logo + (design::SPACE_2 * scale()).round() as i32 + width,
        _ => width,
    }
}

/// Lays the card under the logo, above it, or beside it, in the
/// direction the button decided; and by its right edge or its left
/// edge, whichever it decided has the room to carry the card.
///
/// The same anchor as the logo, in the same move: so the two windows
/// cannot disagree about where the button is.
pub fn lay(
    anchor: (i32, i32),
    opens: Opens,
    on_the_right: bool,
    logo: i32,
    picture: (i32, i32, i32, i32),
) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let (width, height) = (
        WIDTH.load(Ordering::Relaxed) as i32,
        HEIGHT.load(Ordering::Relaxed) as i32,
    );
    // The window is larger than the card by all that the shadow spills
    // over: so it is the **card** that is laid, and the window around
    // it. Laid as if the two were one, the card fell twenty pixels too
    // low and twenty too far left, which shows at first glance next to
    // the old menu.
    let scale = scale();
    // The canvas carries the card drawn for the edge it set out from,
    // and nothing redraws it by itself: without this, an edge that had
    // just changed moved the window at once, over a picture still laid
    // out for the old one, which showed for a glimpse before the next
    // drawing.
    let vertical_change =
        UPWARD.swap(opens == Opens::Up, Ordering::Relaxed) != (opens == Opens::Up);
    let horizontal_change = RIGHTWARD.swap(on_the_right, Ordering::Relaxed) != on_the_right;
    if (vertical_change || horizontal_change)
        && let Some(app) = PROGRAM.lock().expect("programme du menu").clone()
    {
        // Asked again of the thread that owns the window: it is the one
        // holding the canvas, and this runs on the one that follows the
        // hand.
        let _ = app.run_on_main_thread(move || repaint(window as HWND));
    }
    let overflow_px = shadow_overflow(scale).round() as i32;
    let card_height = height - overflow_px * 2;
    // Stuck to the same edge as the logo, and separated from it by the
    // space the stylesheet puts between the two: its right edge as a
    // rule, its left edge when the first has no room, which the button
    // has already decided.
    let between = (design::SPACE_2 * scale).round() as i32;
    // The corner `SetWindowPos` receives further down takes yet one more
    // overflow, for a reason that stands higher up in this function: laid
    // as it is, the card fell twenty pixels too far left. When it is the
    // card that is stuck to that edge rather than left at the right edge,
    // it carries a second overflow itself (its own shadow, `card` laying it
    // at `overflow_px` and not at zero), and the two add up without either
    // accounting for the other: without taking it off twice here, the
    // card's edge would have fallen two overflows past the button rather
    // than at the same place as it.
    let horizontal = if on_the_right {
        anchor.0 - logo - overflow_px * 2
    } else {
        anchor.0 - width
    };
    let (left, top) = match opens {
        Opens::Down => (horizontal, anchor.1 + logo + between - overflow_px),
        Opens::Up => (
            horizontal,
            anchor.1 - logo - between - card_height - overflow_px,
        ),
        // To the side, the card starts from the top of the button and
        // slides by as much as it takes to fit in the picture: that is
        // its whole reason for being there rather than below. Its whole
        // window and not its card alone, since a list's panel opens
        // inside it.
        Opens::Side => (
            if on_the_right {
                anchor.0 + between - overflow_px * 2
            } else {
                anchor.0 - logo - between - width
            },
            (anchor.1 - overflow_px).clamp(picture.1, (picture.3 - height).max(picture.1)),
        ),
    };
    // SAFETY: a window of ours, laid without being activated
    // or resized.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            left + overflow_px,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

/// Builds the window, at the size its lines ask for.
fn build(owner: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, IDC_ARROW, LoadCursorW, RegisterClassW, WNDCLASSW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    /// The class's name, in the characters Windows counts, ended by the
    /// zero it looks for.
    const CLASS: [u16; 13] = [
        b'Z' as u16,
        b'y' as u16,
        b'r' as u16,
        b'D' as u16,
        b'e' as u16,
        b's' as u16,
        b'k' as u16,
        b'M' as u16,
        b'e' as u16,
        b'n' as u16,
        b'u' as u16,
        0,
        0,
    ];

    // The size is measured before the window exists: it depends on the
    // text, and measuring text takes something to draw it with.
    let Some(measure) = Canvas::new(1, 1) else {
        note("bouton flottant : le menu n'a pas pu être mesuré");
        return;
    };
    let scale = scale();
    // The height of a line of text, asked of the font once and for all:
    // everything stacked in this card rests on it.
    store(
        &CAPTION_HEIGHT,
        measure.line_height(Pen::of(design::CAPTION * scale)),
    );
    store(
        &BODY_HEIGHT,
        measure.line_height(Pen::of(design::BODY * scale)),
    );
    measure_the_card(&measure, scale);
    let (width, height) = size(&measure);
    WIDTH.store(width as u32, Ordering::Relaxed);
    HEIGHT.store(height as u32, Ordering::Relaxed);
    drop(measure);

    // SAFETY: a class declared once and a window built on it, on the
    // thread that will pump its messages. A class declared twice is
    // refused with no other effect, hence the unread answer: the second
    // session finds the first one's again.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: CLASS.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            CLASS.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            width,
            height,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("bouton flottant : la fenêtre du menu n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    note(&format!(
        "bouton flottant : menu dessiné par ZyrDesk, {width}x{height} px au \
         départ ; la fenêtre suit ensuite ce que la carte demande. Il ne \
         reste dans la vue web que la ligne rouge qui porte un refus, \
         lequel n'est donc dit ici que dans ce journal"
    ));
}

/// What the card takes up, in real pixels.
///
/// As wide as its longest line, which the stylesheet has always asked
/// for and which no number written by hand could hold to: a label made
/// longer would cut off its shortcut.
fn size(canvas: &Canvas) -> (i32, i32) {
    let scale = scale();
    let overflow_px = shadow_overflow(scale);
    let panel = panels_width(canvas, scale);
    let width = card_width(scale)
        + if panel > 0.0 {
            panel + design::SPACE_2 * scale
        } else {
            0.0
        };
    let height = content(scale).max(panels_height(scale));
    (
        (width + overflow_px * 2.0).ceil() as i32,
        (height + overflow_px * 2.0).ceil() as i32,
    )
}

/// How wide the card is: its longest line.
///
/// What the stylesheet has always asked for and no number written by
/// hand could hold to: a label made longer would cut off its shortcut.
/// Measured over **all** of its lines, including the ones not showing
/// right now: a card that shrinks when a line goes away is a card that
/// changes width under the hand.
fn card_width(scale: f32) -> f32 {
    load(&CARD_WIDTH).max(design::SPACE_2 * 2.0 * scale)
}

/// The same, measured. Stored afterwards, because the layout asks
/// for it again at every frame and measuring text costs.
fn measure_the_card(canvas: &Canvas, scale: f32) {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let mut width: f32 = 0.0;
    for line in &LINES {
        width = width.max(match line {
            Line::Measures => {
                (layout::READING * 4.0 + layout::BETWEEN_READINGS * 3.0 + design::SPACE_2 * 2.0)
                    * scale
            }
            // Wrapped to the width the other lines decide: a refusal is
            // a sentence, and a card as wide as a sentence would be a
            // card twice too wide for everything else.
            Line::Separator | Line::Refusal => 0.0,
            Line::Entry(entry) => {
                let right =
                    canvas.width_of(&entry.trailing.text(), Pen::of(design::CAPTION * scale));
                around(canvas, entry.label, right, scale)
            }
            Line::Toggle(toggle) => around(
                canvas,
                toggle.label,
                sides_width(canvas, &toggle.words(), scale),
                scale,
            ),
            // Its words are asked for with the settings already in hand:
            // asking the line for them again would take again the lock
            // being held, which stops the drawing thread for good.
            Line::Choice(choice) => match session_menu.as_ref() {
                Some(menu) => around(
                    canvas,
                    choice.label,
                    sides_width(canvas, &words_of(menu, choice.setting), scale),
                    scale,
                ),
                None => 0.0,
            },
            // Its bar takes the whole width, so it asks for none: it is
            // its head that decides, as for the others.
            Line::Slider(slider) => {
                let value = session_menu
                    .as_ref()
                    .map_or_else(String::new, |menu| slider.setting.summary(menu));
                let right = canvas.width_of(&value, Pen::of(design::BODY * scale));
                around(canvas, slider.label, right, scale)
            }
            Line::List(list) => {
                let value = session_menu
                    .as_ref()
                    .map_or_else(String::new, |menu| list.setting.summary(menu));
                let right = canvas.width_of(&value, Pen::of(design::CAPTION * scale))
                    + (design::SPACE_2 + layout::BRAND) * scale;
                around(canvas, list.label, right, scale)
            }
        });
    }
    drop(session_menu);
    store(&CARD_WIDTH, width);
}

/// How wide a line is: its icon, its word, what comes on the right,
/// and everything around them.
///
/// The same measurement for every kind of line, because it is the same
/// layout: what changes is what is on the right.
fn around(canvas: &Canvas, label: &str, right: f32, scale: f32) -> f32 {
    canvas.width_of(label, Pen::of(design::BODY * scale))
        + right
        + (design::SPACE_2 * 2.0 + layout::ICON + design::SPACE_3 + layout::AFTER_THE_LABEL) * scale
}

/// How wide the sides of a choice line are, together.
fn sides_width(canvas: &Canvas, words: &[String], scale: f32) -> f32 {
    words
        .iter()
        .map(|label| side_width(canvas, label, scale))
        .sum()
}

/// And what one side takes: its word and what surrounds it.
fn side_width(canvas: &Canvas, label: &str, scale: f32) -> f32 {
    canvas.width_of(label, Pen::of(design::CAPTION * scale)) + design::SPACE_3 * 2.0 * scale
}

/// Where the sides of a choice line fall, pushed to the right edge
/// and stuck to one another.
///
/// They form a single object, with one border around them all and
/// nothing between them.
fn sides_of(canvas: &Canvas, at: Rect, words: &[String], scale: f32) -> Vec<Rect> {
    let widths: Vec<f32> = words
        .iter()
        .map(|label| side_width(canvas, label, scale))
        .collect();
    let height = layout::TOGGLE * scale;
    let top = at.top + (at.bottom - at.top - height) / 2.0;
    let mut left = at.right - design::SPACE_2 * scale - widths.iter().sum::<f32>();
    widths
        .iter()
        .map(|width| {
            let place = Rect::at(left, top, *width, height);
            left += width;
            place
        })
        .collect()
}

/// The bar of a slider, under the head of its line.
fn slider_bar(at: Rect, scale: f32) -> Rect {
    let edge = design::SPACE_2 * scale;
    let top = at.top
        + edge
        + load(&BODY_HEIGHT)
        + layout::UNDER_THE_LABEL * scale
        + (layout::SLIDER - layout::BAR) * scale / 2.0;
    Rect::at(
        at.left + edge,
        top,
        at.right - at.left - edge * 2.0,
        layout::BAR * scale,
    )
}

/// The settings that open a list, in the card's order.
///
/// Read from the lines rather than written a second time: adding a
/// list to the menu is then enough to give it its panel.
fn with_a_panel() -> impl Iterator<Item = Setting> {
    LINES.iter().filter_map(|line| match line {
        Line::List(list) => Some(list.setting),
        _ => None,
    })
}

/// How wide the widest of the panels is, or nothing when none has what
/// it takes to open.
///
/// The widest and not the one that is open: the window cannot change
/// width at the moment a list is opened without the drawing it carries
/// moving at the same instant.
fn panels_width(canvas: &Canvas, scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let Some(menu) = session_menu.as_ref() else {
        return 0.0;
    };
    with_a_panel()
        .map(|setting| panel_width(canvas, menu, setting, scale))
        .fold(0.0, f32::max)
}

/// How wide a panel is: its longest value.
fn panel_width(canvas: &Canvas, menu: &SessionMenu, setting: Setting, scale: f32) -> f32 {
    setting
        .values(menu)
        .iter()
        .map(|value| {
            let aside = canvas.width_of(
                &setting.aside(menu, value),
                Pen::of(design::CAPTION * scale),
            );
            around(canvas, &setting.label(menu, value), aside, scale)
        })
        .fold(0.0, f32::max)
}

/// The height of the tallest panel, for the same reason.
fn panels_height(scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let Some(menu) = session_menu.as_ref() else {
        return 0.0;
    };
    with_a_panel()
        .map(|setting| panel_height(menu, setting, scale))
        .fold(0.0, f32::max)
}

/// How tall a panel is: its values, and nothing else.
///
/// No title: one knows where one is, the line that opened it is facing
/// it and its chevron says so. One more line to repeat the word next to
/// it would be one line less for the values.
fn panel_height(menu: &SessionMenu, setting: Setting, scale: f32) -> f32 {
    let how_many = setting.values(menu).len();
    if how_many == 0 {
        return 0.0;
    }
    (design::SPACE_2 * 2.0 + layout::LINE * how_many as f32) * scale
}

/// The open panel in its window, on the side of the card it did not
/// set out from: on its left as a rule, on its right when the card
/// itself is stuck to the window's left edge.
fn panel(canvas: &Canvas, setting: Setting, scale: f32) -> Option<Rect> {
    // The card and the line first, the settings lock afterwards:
    // measuring them takes that same lock, and a lock taken again while
    // it is held stops the drawing thread for good.
    let card = card(scale);
    let line = panel_line(setting, scale)?;
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let menu = session_menu.as_ref()?;
    let height = panel_height(menu, setting, scale);
    if height <= 0.0 {
        return None;
    }
    let width = panel_width(canvas, menu, setting, scale);
    // Opened facing the line that opens it, its first value level with
    // it: a panel of two values stuck at the top of the card while a
    // line at the bottom is being clicked is a panel one has to search
    // for with one's eyes. It comes down by as much as it takes to fit
    // in the window, which is built tall enough for the largest of them.
    let edge = design::SPACE_2 * scale;
    let inside = shadow_overflow(scale);
    let bottom = (HEIGHT.load(Ordering::Relaxed) as f32 - inside - height).max(inside);
    let left = if RIGHTWARD.load(Ordering::Relaxed) {
        card.right + edge
    } else {
        card.left - edge - width
    };
    Some(Rect::at(
        left,
        (line.top - edge).clamp(inside, bottom),
        width,
        height,
    ))
}

/// Where the line that opens this panel falls, when it shows.
fn panel_line(setting: Setting, scale: f32) -> Option<Rect> {
    walk(scale)
        .into_iter()
        .find(|(_, line, _)| matches!(line, Line::List(list) if list.setting == setting))
        .map(|(_, _, place)| place)
}

/// The place of each of the open panel's values.
fn panel_walk(canvas: &Canvas, setting: Setting, scale: f32) -> Vec<Rect> {
    let Some(panel) = panel(canvas, setting, scale) else {
        return Vec::new();
    };
    let edge = design::SPACE_2 * scale;
    let how_many = SESSION_MENU
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .map_or(0, |menu| setting.values(menu).len());
    let mut top = panel.top + edge;
    (0..how_many)
        .map(|_| {
            let place = Rect::at(
                panel.left + edge,
                top,
                panel.right - panel.left - edge * 2.0,
                layout::LINE * scale,
            );
            top = place.bottom;
            place
        })
        .collect()
}

/// How far the shadow spills out of the card, on each side.
fn shadow_overflow(scale: f32) -> f32 {
    let shadow = palette().shadow_2;
    (shadow.soft + shadow.down.abs().max(shadow.across.abs())) * scale
}

/// The height of the readings bar.
fn readings_height(scale: f32) -> f32 {
    design::SPACE_2 * scale
        + load(&CAPTION_HEIGHT)
        + layout::UNDER_THE_LABEL * scale
        + load(&BODY_HEIGHT)
        + design::SPACE_1 * scale
        + load(&CAPTION_HEIGHT)
        + design::SPACE_1 * scale
}

/// The height of a slider line: its head, then the bar below.
fn slider_height(scale: f32) -> f32 {
    (design::SPACE_2 + layout::UNDER_THE_LABEL + layout::SLIDER + design::SPACE_3) * scale
        + load(&BODY_HEIGHT)
}

/// The card in its window.
///
/// As tall as what it shows, and no taller. Lines come and go with the
/// session, and the window is built once for the largest card possible: so
/// this one is stuck to the edge the menu opens from, which is the only
/// one nobody must see move.
fn card(scale: f32) -> Rect {
    let (width, height) = (
        WIDTH.load(Ordering::Relaxed) as f32,
        HEIGHT.load(Ordering::Relaxed) as f32,
    );
    let overflow_px = shadow_overflow(scale);
    let inside = height - overflow_px * 2.0;
    let show = content(scale).min(inside);
    let top = if UPWARD.load(Ordering::Relaxed) {
        overflow_px + inside - show
    } else {
        overflow_px
    };
    let left = if RIGHTWARD.load(Ordering::Relaxed) {
        overflow_px
    } else {
        width - overflow_px - card_width(scale)
    };
    Rect::at(left, top, card_width(scale), show)
}

/// The height of what the card is showing right now.
fn content(scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    design::SPACE_2 * scale * 2.0
        + LINES
            .iter()
            .filter(|line| line.is_visible(session_menu.as_ref()))
            .map(|line| line.height(scale))
            .sum::<f32>()
}

/// Each visible line and the room it takes, from the top of the card
/// downwards.
///
/// Read by the drawing and by the mouse, written only once: a card whose
/// lines are drawn in one place and clicked in another is a card that
/// serves the wrong menu.
fn walk(scale: f32) -> Vec<(usize, &'static Line, Rect)> {
    let card = card(scale);
    let edge = design::SPACE_2 * scale;
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let mut top = card.top + edge;
    let mut placed = Vec::with_capacity(LINES.len());
    for (rank, line) in LINES.iter().enumerate() {
        if !line.is_visible(session_menu.as_ref()) {
            continue;
        }
        let height = line.height(scale);
        placed.push((
            rank,
            line,
            Rect::at(
                card.left + edge,
                top,
                card.right - card.left - edge * 2.0,
                height,
            ),
        ));
        top += height;
    }
    placed
}

/// What is under this point of the window, when it is something that
/// gets clicked.
///
/// What comes in pieces, the sides of a switch and the values of a
/// panel, needs to know where they fall, and so something to measure
/// text with: the window's canvas, the very one they were drawn on. A
/// mouse aiming by another measurement than the drawing would miss.
fn under(point: (i32, i32)) -> Option<Target> {
    let (x, y) = (point.0 as f32, point.1 as f32);
    let scale = scale();
    let inside =
        |place: &Rect| x >= place.left && x < place.right && y >= place.top && y < place.bottom;

    if let Some(setting) = *PANEL.lock().expect("panneau du menu") {
        let in_the_panel = CANVAS.with_borrow(|canvas| {
            panel_walk(canvas.as_ref()?, setting, scale)
                .iter()
                .position(inside)
                .map(Target::Value)
        });
        if in_the_panel.is_some() {
            return in_the_panel;
        }
    }

    let (rank, line, place) = walk(scale)
        .into_iter()
        .find(|(_, _, place)| inside(place))?;
    match line {
        Line::Entry(_) | Line::List(_) => Some(Target::Line(rank)),
        Line::Toggle(toggle) => CANVAS.with_borrow(|canvas| {
            sides_of(canvas.as_ref()?, place, &toggle.words(), scale)
                .iter()
                .position(inside)
                .map(|side| Target::Side(rank, side))
        }),
        Line::Choice(choice) => CANVAS.with_borrow(|canvas| {
            let canvas = canvas.as_ref()?;
            sides_of(canvas, place, &choice.words()?, scale)
                .iter()
                .position(inside)
                .map(|side| Target::Side(rank, side))
        }),
        Line::Slider(_) => inside(&slider_bar(place, scale).grown(
            // The bar is four pixels tall: aiming at four pixels with a
            // mouse is a chore, and nobody asked for a chore. What gets
            // caught is the thumb's height.
            (layout::THUMB - layout::BAR) * scale / 2.0,
        ))
        .then_some(Target::Bar(rank)),
        Line::Measures | Line::Separator | Line::Refusal => None,
    }
}

/// Draws the card and hands it to the window.
///
/// The window follows what the card asks for. It can change size without
/// anything flickering: the picture and the size are handed to Windows in
/// the same move, so there is no moment when the window is large without
/// being painted. That is what a web view cannot do, and it is what lets
/// the card here be measured on what it really holds rather than on what
/// it might hold one day.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let scale = scale();
    let colours = palette();
    let radius = design::RADIUS * scale;
    let hover = *HOVER.lock().expect("survol du menu");
    let is_open = *PANEL.lock().expect("panneau du menu");

    CANVAS.with_borrow_mut(|canvas| {
        // The room needed, measured on the canvas that is there: measuring
        // text does not need the right size of canvas, only a canvas.
        if canvas.is_none() {
            *canvas = Canvas::new(1, 1);
        }
        let Some(measure) = canvas.as_ref() else {
            return;
        };
        measure_the_card(measure, scale);
        let (width, height) = size(measure);
        if width <= 0 || height <= 0 {
            return;
        }
        WIDTH.store(width as u32, Ordering::Relaxed);
        HEIGHT.store(height as u32, Ordering::Relaxed);
        // Made again as soon as it is no longer the right size, which is
        // also the case of the one-pixel canvas that has just served for
        // measuring.
        if measure.size() != (width, height) {
            *canvas = Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };

        let card = card(scale);
        canvas.begin(Colour::TRANSPARENT);
        canvas.shadow(card, radius, colours.shadow_2, scale);
        canvas.fill(card, radius, colours.surface_1);
        canvas.stroke_inside(
            card,
            radius,
            layout::HAIRLINE * scale,
            colours.border_strong,
        );

        let painter = Painter {
            canvas,
            scale,
            colours,
        };
        for (rank, line, at) in walk(scale) {
            let under_the_hand = hover.filter(|target| target.line() == Some(rank));
            let side = match under_the_hand {
                Some(Target::Side(_, side)) => Some(side),
                _ => None,
            };
            match line {
                Line::Measures => painter.measures(at),
                Line::Separator => painter.separator(at),
                Line::Refusal => painter.refusal(at),
                Line::Entry(entry) => painter.entry(at, entry, under_the_hand.is_some()),
                Line::Toggle(toggle) => painter.sides(
                    at,
                    &Sides {
                        icon: toggle.icon,
                        label: toggle.label,
                        words: &toggle.words(),
                        current_side: toggle.current_side(),
                        struck: &[],
                    },
                    side,
                ),
                Line::Choice(choice) => {
                    if let (Some(words), Some((current_side, struck))) =
                        (choice.words(), choice.current())
                    {
                        painter.sides(
                            at,
                            &Sides {
                                icon: choice.icon,
                                label: choice.label,
                                words: &words,
                                current_side,
                                struck: &struck,
                            },
                            side,
                        );
                    }
                }
                Line::Slider(slider) => painter.slider(at, slider),
                Line::List(list) => painter.list(at, list, under_the_hand.is_some()),
            }
        }

        if let Some(setting) = is_open {
            painter.panel(setting, hover);
        }
        if !canvas.finish() {
            return;
        }

        let mut place = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a window of ours, whose rectangle is read into ours.
        if unsafe { GetWindowRect(window, &mut place) } == 0 {
            return;
        }
        // Hung by the edge the menu opens from, and by the one it sets
        // out from vertically: those are the only two nobody must see
        // move when the window changes size. They are also the ones
        // `lay` works out, so the two agree by themselves.
        let x = if RIGHTWARD.load(Ordering::Relaxed) {
            place.left
        } else {
            place.right - width
        };
        let y = if UPWARD.load(Ordering::Relaxed) {
            place.bottom - height
        } else {
            place.top
        };
        canvas.lay_on(window as isize, x, y);
    });
}

/// What does not change while a card is being drawn: something to draw
/// with, how much a page pixel counts for, and the theme.
///
/// Carried together rather than passed three times to every line, and
/// the menu now has seven kinds of them.
struct Painter<'a> {
    canvas: &'a Canvas,
    scale: f32,
    colours: Palette,
}

/// What a line with sides shows: its head, its words, the one in place,
/// and the ones the far machine cannot do.
///
/// Carried together because it is drawn together, and because a switch
/// and a line of buttons are described no differently.
struct Sides<'a> {
    icon: &'a Icon,
    label: &'a str,
    words: &'a [String],
    current_side: usize,
    struck: &'a [bool],
}

impl Painter<'_> {
    /// The start of a line, which is the same for all of them: its icon
    /// in its place, and its word after it.
    fn head(&self, at: Rect, icon: &Icon, label: &str, ink: Colour) {
        let (canvas, scale) = (self.canvas, self.scale);
        let side = layout::ICON * scale;
        canvas.icon(
            icon,
            Rect::at(
                at.left + design::SPACE_2 * scale,
                at.top + (at.bottom - at.top - side) / 2.0,
                side,
                side,
            ),
            ink,
        );
        canvas.draw_text(
            label,
            Pen::of(design::BODY * scale),
            ink,
            Rect {
                left: at.left + (design::SPACE_2 + layout::ICON + design::SPACE_3) * scale,
                ..at
            },
        );
    }

    /// The background a line takes under the hand.
    fn hover(&self, at: Rect, tint: Option<Colour>) {
        if let Some(tint) = tint {
            self.canvas
                .fill(at, design::RADIUS_SMALL * self.scale, tint);
        }
    }

    /// What is written to the right of a line, in the colour of things
    /// one reads without looking for them.
    fn on_the_right(&self, at: Rect, label: &str, size: f32, ink: Colour) {
        if label.is_empty() {
            return;
        }
        self.canvas.draw_text(
            label,
            Pen::of(size).aligned(Align::Right),
            ink,
            Rect {
                right: at.right - design::SPACE_2 * self.scale,
                ..at
            },
        );
    }

    /// An entry: its icon, its word, what is written to its right, and
    /// the background the hover gives it.
    fn entry(&self, at: Rect, entry: &Entry, under_the_hand: bool) {
        let colours = self.colours;
        let ink = if entry.destructive {
            colours.error
        } else {
            colours.text
        };
        // The line that cuts the session off lights up in its own colour
        // rather than the grey of the others: it is not one more hover,
        // it is the one to be wary of.
        self.hover(
            at,
            under_the_hand.then(|| {
                if entry.destructive {
                    colours.error.faded(VEIL)
                } else {
                    colours.surface_3
                }
            }),
        );
        self.head(at, entry.icon, entry.label, ink);
        self.on_the_right(
            at,
            &entry.trailing.text(),
            design::CAPTION * self.scale,
            colours.text_faint,
        );
    }

    /// A line that opens a list: its value in place, then the chevron that
    /// says it leads elsewhere.
    fn list(&self, at: Rect, list: &List, under_the_hand: bool) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        self.hover(at, under_the_hand.then_some(colours.surface_3));
        self.head(at, list.icon, list.label, colours.text);

        let brand = layout::BRAND * scale;
        let edge = design::SPACE_2 * scale;
        let open = *PANEL.lock().expect("panneau du menu") == Some(list.setting);
        canvas.icon(
            // The chevron says which way the list opens, so it turns
            // round when the list is open: the list appears on the left
            // as a rule, and it points towards it; on the right when the
            // card itself is stuck to the window's left edge, it points
            // towards it by simply pointing where it already pointed
            // while closed.
            if open && !RIGHTWARD.load(Ordering::Relaxed) {
                &icons::BACK
            } else {
                &icons::CHEVRON
            },
            Rect::at(
                at.right - edge - brand,
                at.top + (at.bottom - at.top - brand) / 2.0,
                brand,
                brand,
            ),
            colours.text_faint,
        );
        let value = SESSION_MENU
            .lock()
            .expect("réglages du menu")
            .as_ref()
            .map_or_else(String::new, |menu| list.setting.summary(menu));
        self.on_the_right(
            Rect {
                right: at.right - brand - edge,
                ..at
            },
            &value,
            design::CAPTION * scale,
            colours.text_faint,
        );
    }

    /// A line with sides: a switch or a row of buttons, only one of which
    /// is filled.
    ///
    /// Both are drawn here because they are drawn the same way. What sets
    /// them apart is what they do, not what they show: one flips the
    /// session at once, the other writes a choice that the session takes
    /// up where it stands.
    fn sides(&self, at: Rect, spec: &Sides, under_the_hand: Option<usize>) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let words = spec.words;
        self.head(at, spec.icon, spec.label, colours.text);

        let sides = sides_of(canvas, at, words, scale);
        let Some(whole) = sides.first().map(|first| Rect {
            left: first.left,
            ..*sides.last().unwrap_or(first)
        }) else {
            return;
        };
        let radius = design::RADIUS_SMALL * scale;
        for (rank, place) in sides.iter().enumerate() {
            let bar = spec.struck.get(rank).copied().unwrap_or(false);
            let (background, ink) = if rank == spec.current_side {
                (Some(colours.accent_bright), colours.on_accent)
            } else if bar {
                (None, colours.text_faint)
            } else if under_the_hand == Some(rank) {
                (Some(colours.surface_3), colours.text)
            } else {
                (None, colours.text_faint)
            };
            if let Some(background) = background {
                // The background of the whole object, seen through that
                // side: the sides make up a single one, rounded on the
                // outside and straight where they touch, which no rounded
                // rectangle can be on its own.
                canvas.clipped(*place, || canvas.fill(whole, radius, background));
            }
            canvas.draw_text(
                &words[rank],
                Pen::of(design::CAPTION * scale).aligned(Align::Centre),
                ink,
                *place,
            );
            if bar {
                // What the far machine cannot do keeps its place: an
                // option that disappears from one computer to the next
                // suggests a menu that changes its mind, when it is the
                // machine being looked at that does not have the same
                // graphics card. Struck through, then, and not wiped.
                let middle = (place.top + place.bottom) / 2.0;
                let half_label =
                    canvas.width_of(&words[rank], Pen::of(design::CAPTION * scale)) / 2.0;
                let centre = (place.left + place.right) / 2.0;
                canvas.fill(
                    Rect::at(
                        centre - half_label,
                        middle,
                        half_label * 2.0,
                        layout::HAIRLINE * scale,
                    ),
                    0.0,
                    colours.text_faint,
                );
            }
        }
        canvas.stroke_inside(whole, radius, layout::HAIRLINE * scale, colours.border);
    }

    /// A slider line: its head, its value, and the bar below.
    fn slider(&self, at: Rect, slider: &Slider) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        // Its head fits in the height of a body line, the bar taking
        // the rest.
        let head = Rect {
            bottom: at.top + design::SPACE_2 * scale * 2.0 + load(&BODY_HEIGHT),
            ..at
        };
        self.head(head, slider.icon, slider.label, colours.text);
        // A setting's value is read where the shortcuts are read, but it is
        // not one: it is what the line is worth, so it reads like the rest
        // of the line and not toned down.
        self.on_the_right(head, &slider.value(), design::BODY * scale, colours.text);

        let Some((notch, how_many)) = slider.notch() else {
            return;
        };
        let bar = slider_bar(at, scale);
        let radius = layout::BAR * scale / 2.0;
        canvas.fill(bar, radius, colours.border);
        let part = if how_many > 1 {
            notch as f32 / (how_many - 1) as f32
        } else {
            0.0
        };
        let thumb = layout::THUMB * scale;
        // The thumb stays whole inside the bar at both its ends: placed
        // by its share alone, it would spill over by half of itself.
        let thumb_x = bar.left + thumb / 2.0 + (bar.right - bar.left - thumb) * part;
        let middle = (bar.top + bar.bottom) / 2.0;
        canvas.fill(
            Rect::at(bar.left, bar.top, thumb_x - bar.left, radius * 2.0),
            radius,
            colours.accent_bright,
        );
        canvas.fill(
            Rect::at(thumb_x - thumb / 2.0, middle - thumb / 2.0, thumb, thumb),
            thumb / 2.0,
            colours.accent_bright,
        );
    }

    /// The stroke between two groups, centred in the room it takes.
    ///
    /// Brought in by one step on each side, as the stylesheet asks: a
    /// stroke that runs from one edge to the other cuts the card in
    /// two instead of separating two groups of lines.
    fn separator(&self, at: Rect) {
        let edge = design::SPACE_2 * self.scale;
        self.canvas.fill(
            Rect::at(
                at.left + edge,
                at.top + edge,
                at.right - at.left - edge * 2.0,
                layout::HAIRLINE * self.scale,
            ),
            0.0,
            self.colours.border,
        );
    }

    /// What the menu has just refused to do, spelled out.
    ///
    /// Wrapped to the card's width: what a refusal has to say is what
    /// needs doing elsewhere, and cutting that short would come down to
    /// saying nothing at all.
    fn refusal(&self, at: Rect) {
        let Some(said) = refusal_to_say() else {
            return;
        };
        let edge = design::SPACE_2 * self.scale;
        let width = at.right - at.left - edge * 2.0;
        let pen = Pen::of(design::CAPTION * self.scale);
        // Measured here because here is the only place that can measure
        // wrapped text, and stored so that the card opens on it, as its
        // width already is.
        let height = self.canvas.height_of(&said, pen, width);
        store(&REFUSAL_HEIGHT, height + edge * 2.0);
        self.canvas.draw_text(
            &said,
            pen,
            self.colours.warning,
            Rect::at(at.left + edge, at.top + edge, width, height),
        );
    }

    /// The bar of the four readings: a word over a number, four times,
    /// and the stream's sentence below.
    fn measures(&self, at: Rect) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let edge = design::SPACE_2 * scale;
        let top = at.top + edge;
        let bar = READINGS_BAR.lock().expect("mesures du menu");
        for (rank, reading) in READINGS.iter().enumerate() {
            let left =
                at.left + edge + rank as f32 * (layout::READING + layout::BETWEEN_READINGS) * scale;
            let column = layout::READING * scale;
            canvas.draw_text(
                reading.label,
                Pen::of(design::CAPTION * scale),
                colours.text_faint,
                Rect::at(left, top, column, load(&CAPTION_HEIGHT)),
            );
            canvas.draw_text(
                &bar.figures[rank],
                Pen::of(design::BODY * scale),
                colours.text,
                Rect::at(
                    left,
                    top + load(&CAPTION_HEIGHT) + layout::UNDER_THE_LABEL * scale,
                    column,
                    load(&BODY_HEIGHT),
                ),
            );
        }
        if !bar.stream.is_empty() {
            canvas.draw_text(
                &bar.stream,
                Pen::of(design::CAPTION * scale),
                colours.text_faint,
                Rect::at(
                    at.left + edge,
                    top + load(&CAPTION_HEIGHT)
                        + layout::UNDER_THE_LABEL * scale
                        + load(&BODY_HEIGHT)
                        + design::SPACE_1 * scale,
                    at.right - at.left - edge * 2.0,
                    load(&CAPTION_HEIGHT),
                ),
            );
        }
    }

    /// A setting's panel, on the side of the card it did not set out
    /// from: its values, one of which carries the mark.
    ///
    /// No title. One knows where one is: the line that opened it is
    /// facing it, its chevron has turned round towards it, and clicking
    /// it again closes it. A title that repeats the word next to it takes
    /// a line to teach nothing.
    fn panel(&self, setting: Setting, hover: Option<Target>) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let Some(place) = panel(canvas, setting, scale) else {
            return;
        };
        let radius = design::RADIUS * scale;
        canvas.shadow(place, radius, colours.shadow_2, scale);
        canvas.fill(place, radius, colours.surface_1);
        canvas.stroke_inside(
            place,
            radius,
            layout::HAIRLINE * scale,
            colours.border_strong,
        );

        let values = panel_walk(canvas, setting, scale);
        let side = layout::BRAND * scale;
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let Some(menu) = session_menu.as_ref() else {
            return;
        };
        let values_here = setting.values(menu);
        let at = setting.current(menu);
        for (rank, place) in values.iter().enumerate() {
            let Some(value) = values_here.get(rank) else {
                break;
            };
            self.hover(
                *place,
                (hover == Some(Target::Value(rank))).then_some(colours.surface_3),
            );
            if *value == at {
                canvas.icon(
                    &icons::TICK,
                    Rect::at(
                        place.left + design::SPACE_2 * scale,
                        place.top + (place.bottom - place.top - side) / 2.0,
                        side,
                        side,
                    ),
                    colours.accent_bright,
                );
            }
            canvas.draw_text(
                &setting.label(menu, value),
                Pen::of(design::BODY * scale),
                colours.text,
                Rect {
                    left: place.left + (design::SPACE_2 + layout::ICON + design::SPACE_3) * scale,
                    ..*place
                },
            );
            self.on_the_right(
                *place,
                &setting.aside(menu, value),
                design::CAPTION * scale,
                colours.text_faint,
            );
        }
    }
}

/// What the window answers when the system speaks to it.
///
/// SAFETY: called by the system on the thread that made this window,
/// with the arguments it documents.
unsafe extern "system" fn answer(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTCLIENT, IDC_ARROW, IDC_HAND, LoadCursorW, SetCursor, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_SETCURSOR,
    };

    match message {
        WM_MOUSEMOVE => {
            if !HAND_INSIDE.swap(true, Ordering::Relaxed) {
                // Asked for as soon as a hand arrives: without that
                // nothing ever says it has left, and the last line
                // hovered would stay lit under a mouse that is no
                // longer there.
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                // SAFETY: a window of ours, and the request is ours.
                unsafe { TrackMouseEvent(&mut tracking) };
            }
            if pushes(window, point(with)) {
                return 0;
            }
            hovers(window, under(point(with)));
            0
        }
        WM_MOUSELEAVE => {
            HAND_INSIDE.store(false, Ordering::Relaxed);
            hovers(window, None);
            0
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            // The line is asked of the system rather than taken from
            // the last hover: the pointer's shape is decided before the
            // move is announced, and the hand would then be one move
            // late.
            let cursor = if under_the_mouse(window).is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: one of the system's pointer shapes, asked
            // for by its name.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), cursor)) };
            1
        }
        WM_LBUTTONDOWN => {
            let target = under(point(with));
            *PRESSED.lock().expect("appui du menu") = target;
            // A slider is taken and pushed: the gesture starts here and
            // only ends on release, where only the notch it arrives at is
            // written.
            if matches!(target, Some(Target::Bar(_))) {
                pushes(window, point(with));
            }
            0
        }
        // On release, and where the press began: that is what a click
        // means, and it is what lets one slip away from a button one
        // should not have aimed at.
        WM_LBUTTONUP => {
            let pressed = PRESSED.lock().expect("appui du menu").take();
            if let Some(Target::Bar(rank)) = pressed {
                released(window, rank);
                return 0;
            }
            if let Some(target) = under(point(with))
                && Some(target) == pressed
            {
                acts(target);
            }
            0
        }
        // SAFETY: the system's answer to everything not answered here.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// Where the mouse is in the window, as the system writes it in a
/// message: two signed numbers in the two halves of a single one.
fn point(with: windows_sys::Win32::Foundation::LPARAM) -> (i32, i32) {
    (
        i32::from((with & 0xFFFF) as i16),
        i32::from(((with >> 16) & 0xFFFF) as i16),
    )
}

/// What is under the pointer, asked of the system.
fn under_the_mouse(window: windows_sys::Win32::Foundation::HWND) -> Option<Target> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: a point of ours, and a window of ours it is brought into.
    let read = unsafe { GetCursorPos(&mut at) != 0 && ScreenToClient(window, &mut at) != 0 };
    if !read {
        return None;
    }
    under((at.x, at.y))
}

/// Lights up what is under the mouse, and redraws when it is no longer
/// the same thing.
fn hovers(window: windows_sys::Win32::Foundation::HWND, target: Option<Target>) {
    let mut hover = HOVER.lock().expect("survol du menu");
    if *hover == target {
        return;
    }
    *hover = target;
    drop(hover);
    repaint(window);
}

/// Does what the thing just clicked asks for.
///
/// A refusal only goes to the journal while the web view's menu is still
/// there: it is the one carrying the red line that says it, and drawing a
/// second one here would make two places to keep up for the same sentence.
///
/// Said before it goes off, and not only when it refuses. This menu is
/// behind the picture and its lines are few: without this line, an entry
/// that seems to do nothing cannot be told apart from a click that never
/// arrived, and the two are fixed in different places.
fn acts(target: Target) {
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    match (target, target.line().and_then(|rank| LINES.get(rank))) {
        (Target::Line(_), Some(Line::Entry(entry))) => {
            say_the_click(entry.label);
            let does = entry.does;
            // Closed before it goes off, as the page does: what follows
            // takes the time it takes, and a card left open on top would
            // be a tablecloth laid over the picture.
            show(false);
            crate::app::spawn(async move {
                let refusal = match does {
                    Does::Session(session_act) => crate::floating::ask(&app, session_act).await,
                    Does::PutAway => crate::floating::hide(&app),
                };
                say_the_refusal(refusal);
            });
        }
        (Target::Line(_), Some(Line::List(list))) => {
            // The same line opens and closes: a list opened beside the
            // menu closes where it was opened, and not only by its title.
            let mut panel = PANEL.lock().expect("panneau du menu");
            *panel = (*panel != Some(list.setting)).then_some(list.setting);
            drop(panel);
            redraw(&app);
        }
        (Target::Side(_, side), Some(Line::Toggle(toggle))) => {
            // Pushing a switch to the side it is already on does nothing,
            // like any switch.
            if toggle.current_side() == side {
                return;
            }
            note(&format!(
                "menu du bouton flottant : « {} » mis sur « {} »",
                toggle.label, toggle.sides[side]
            ));
            // The card stays open: one looks at the picture after
            // flipping, and opening it again for the next line would
            // make two gestures for one setting.
            let act = toggle.act;
            crate::app::spawn(async move {
                match crate::floating::ask(&app, act).await {
                    // Read again rather than assumed: it is the only
                    // way to show where things really stand, and the
                    // sound is read in the Windows mixer and not here.
                    Ok(()) => reread_the_toggles(&app).await,
                    Err(refusal) => say_the_refusal(Err(refusal)),
                }
            });
        }
        (Target::Side(_, side), Some(Line::Choice(choice))) => {
            let Some(value) = value_of(choice.setting, side) else {
                return;
            };
            // What the far machine cannot do is not a choice: offering
            // it struck through says why, letting it be clicked would
            // say the opposite.
            let refuse = SESSION_MENU
                .lock()
                .expect("réglages du menu")
                .as_ref()
                .is_some_and(|menu| choice.setting.out_of_reach(menu, &value));
            if refuse {
                return;
            }
            choose(&app, choice.setting, value);
        }
        (Target::Value(rank), _) => {
            let Some(setting) = *PANEL.lock().expect("panneau du menu") else {
                return;
            };
            let Some(value) = value_of(setting, rank) else {
                return;
            };
            // The list closes on the choice: staying in it after choosing
            // would suggest there is something left to do there.
            *PANEL.lock().expect("panneau du menu") = None;
            // And the card with it: what is chosen in a list shows at
            // once, what one wants to look at then is the picture, and a
            // card left on top would be a tablecloth laid over it.
            show(false);
            choose(&app, setting, value);
        }
        _ => {}
    }
}

/// A setting's value at that rank.
fn value_of(setting: Setting, rank: usize) -> Option<String> {
    SESSION_MENU
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .and_then(|menu| setting.values(menu).get(rank).cloned())
}

/// Writes this choice, gives it to the session where it stands, and reads
/// back what the session says about it.
///
/// Read back and not assumed: choosing a size changes what "client" is
/// worth, and it is the answer that carries it.
fn choose(app: &App, setting: Setting, value: String) {
    note(&format!(
        "menu du bouton flottant : {} mis sur « {value} »",
        setting.name()
    ));
    let app = app.clone();
    crate::app::spawn(async move {
        match crate::settings::choose_session(app.clone(), setting.name().to_string(), value).await
        {
            Ok(choice) => {
                if let Some(menu) = SESSION_MENU.lock().expect("réglages du menu").as_mut() {
                    menu.now = choice;
                }
                redraw(&app);
            }
            Err(refusal) => say_the_refusal(Err(refusal)),
        }
    });
}

/// Says that a line was clicked.
///
/// Said before it goes off, and not only when it refuses. This menu is
/// behind the picture and its lines are few: without this line, an entry
/// that seems to do nothing cannot be told apart from a click that never
/// arrived, and the two are fixed in different places.
fn say_the_click(label: &str) {
    note(&format!("menu du bouton flottant : « {label} » cliqué"));
}

/// And says a refusal, if there is one.
///
/// On the card and in the journal. On the card because that is where the
/// person who has just clicked is looking, and in the journal because the
/// card closes and a sentence read once cannot be found again.
fn say_the_refusal(refusal: Result<(), String>) {
    let Err(refusal) = refusal else {
        return;
    };
    note(&format!("menu du bouton flottant : {refusal}"));
    *REFUSAL.lock().expect("refus du menu") = Some((refusal, Instant::now()));
    if let Some(app) = PROGRAM.lock().expect("programme du menu").clone() {
        redraw(&app);
    }
}

/// Pushes the slider to where the hand is, and says whether it was
/// holding one.
///
/// Nothing is written while it holds it: a slider pushed from one end to
/// the other crosses all its notches, and each would be a round trip to
/// the service for a bitrate nobody wanted.
fn pushes(window: windows_sys::Win32::Foundation::HWND, at: (i32, i32)) -> bool {
    let Some(Target::Bar(rank)) = *PRESSED.lock().expect("appui du menu") else {
        return false;
    };
    let Some(Line::Slider(slider)) = LINES.get(rank) else {
        return false;
    };
    let Some((_, how_many)) = slider.notch() else {
        return false;
    };
    let scale = scale();
    let Some((_, _, place)) = walk(scale).into_iter().find(|(other, _, _)| *other == rank) else {
        return false;
    };
    let bar = slider_bar(place, scale);
    let thumb = layout::THUMB * scale;
    // The thumb does not go from one edge to the other but from one
    // centre to the other: counted over the whole bar, the two end
    // notches could not be reached.
    let travel = (bar.right - bar.left - thumb).max(1.0);
    let part = ((at.0 as f32 - bar.left - thumb / 2.0) / travel).clamp(0.0, 1.0);
    let notch = (part * (how_many.max(1) - 1) as f32).round() as usize;
    let mut pushed = PUSHED.lock().expect("curseur du menu");
    if *pushed != Some(notch) {
        *pushed = Some(notch);
        drop(pushed);
        repaint(window);
    }
    true
}

/// Lets the slider go, and writes the notch it was left at.
fn released(window: windows_sys::Win32::Foundation::HWND, rank: usize) {
    let Some(notch) = PUSHED.lock().expect("curseur du menu").take() else {
        return;
    };
    repaint(window);
    let Some(Line::Slider(slider)) = LINES.get(rank) else {
        return;
    };
    let Some(value) = value_of(slider.setting, notch) else {
        return;
    };
    let already = SESSION_MENU
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .is_some_and(|menu| slider.setting.current(menu) == value);
    if already {
        return;
    }
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    choose(&app, slider.setting, value);
}

/// Rereads where the four switches stand, and redraws if anything moved.
///
/// Three of them are what this program believes, because it is the one
/// flipping them and the engine never says where it stands; the sound is
/// asked of the Windows mixer, which knows it and is open to everyone.
async fn reread_the_toggles(app: &App) {
    /// Sets where a switch stands, and says if it moved.
    fn set(cell: &AtomicBool, value: bool) -> bool {
        cell.swap(value, Ordering::Relaxed) != value
    }

    let mut change = set(&IN_GAME, crate::floating::in_game_mouse(app));
    change |= set(&IMMERSIVE, crate::floating::keys_to_the_session(app));
    change |= set(&SHARED, crate::floating::the_clipboard_is_shared(app));
    change |= set(&HELD, crate::floating::the_badges_are_held_up(app));
    // Without a session the mixer has nothing to say, and the card does
    // not open without a session: so a refusal is left as it is rather
    // than turning the switch off.
    if let Ok(muted) = crate::floating::hushed(app).await {
        change |= set(&MUTED, muted);
    }
    if change {
        redraw(app);
    }
}

/// Follows what the session costs while the card is open, and not a
/// second longer: figures nobody looks at are worth neither the file nor
/// the wake-up.
fn follow_the_readings(app: &App, is_open: bool) {
    // The round changes at every call, which stops the one before:
    // without that, opening and closing quickly would leave two watches
    // behind the same card.
    let round = ROUND.fetch_add(1, Ordering::Relaxed) + 1;
    if !is_open {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        while ROUND.load(Ordering::Relaxed) == round {
            let said = crate::measures::session_measures();
            let now = Instant::now();
            // The lock is given back before the wait: a lock held
            // across a wait is a lock held for a second. Taken before
            // the read and not after, because the read starts from the
            // previous one for the missing readings.
            let change = {
                let mut bar = READINGS_BAR.lock().expect("mesures du menu");
                let load = ReadingsBar::of(&said, &bar, now);
                let change = bar.reads_differently(&load);
                *bar = load;
                change
            };
            if change {
                redraw(&app);
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Redraws the card from a thread that is not the one drawing it.
fn redraw(app: &App) {
    let _ = app.run_on_main_thread(|| {
        use windows_sys::Win32::Foundation::HWND;

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if !window.is_null() {
            repaint(window);
        }
    });
}
