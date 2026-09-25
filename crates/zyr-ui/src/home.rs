//! ZyrDesk's home window, drawn by this program.
//!
//! It was the product's last page. What replaces it fits in an ordinary
//! window, framed by Windows, whose inside is a canvas: the same drawing
//! as the logo and the session menu, the same palette read from the same
//! style sheet, the same icons.
//!
//! **It decides nothing.** It asks the service, through the core, and
//! draws what comes back. The vocabulary follows the product's:
//! "ordinateur" and not "hôte", "accès distant" and not "service".
//!
//! # One walk
//!
//! Drawing and knowing what is under the mouse are the same job: one pass
//! lays down each thing and notes along the way what answers the click.
//! Two walks would agree only until the day one of them changes.
//!
//! # What is not drawn here
//!
//! The input fields. Writing text is the one place where the system does
//! better than we do: the text cursor, the selection, the clipboard, the
//! keyboards that compose their characters. So these are real Windows
//! fields, set inside the frame we draw, and they live only as long as
//! the dialogue that carries them.

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use zyr_broker::rest::Access;
use zyr_control::{Account, Attach, Device, Registering};

use crate::app::App;

use crate::design::{self, Colour, Palette};
use crate::desk::{Attached, Peer, Standing, Watcher};
use crate::icons;
use crate::paint::{Align, Canvas, Icon, Pen, Rect};
use crate::session::Ongoing;
use crate::settings::Settings;
use crate::shortcuts::{Combination, Doing, Held};
use crate::theme::Choice;

/// What this module's lines are filed under.
const TAG: &str = "home";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// What the service can change without anyone clicking: a session
/// opened from the other end, an engine dropped into its folder,
/// the service stopped. Asked for again this often.
const REFRESH: std::time::Duration = std::time::Duration::from_secs(3);

/// How long a "Copié" stays readable before the button goes back to
/// its own word.
const COPIED_TIME: std::time::Duration = std::time::Duration::from_millis(1600);

/// How long a request for confirmation stays armed.
const CONFIRM_TIME: std::time::Duration = std::time::Duration::from_secs(4);

/// How long a piece of good news stays on screen before it goes away.
const NOTICE_TIME: std::time::Duration = std::time::Duration::from_secs(6);

/// How long the thread that goes back and forth takes to
/// travel one way.
const BACK_AND_FORTH: std::time::Duration = std::time::Duration::from_millis(1400);

/// A fingerprint is always this long. Checking it here saves
/// bothering the service for nothing.
const FINGERPRINT_LENGTH: usize = 64;

const MINUTE: u64 = 60;
const HOUR: u64 = 3600;

/* ---- What the home window shows -------------------------------------- */

/// What the product says about itself.
///
/// Nothing is decided here: everything comes from the service, and the
/// window only draws it. A session belongs to the service and survives
/// this window being closed, updated or crashing.
#[derive(Default, PartialEq)]
struct Seen {
    machine: Option<Standing>,
    peers: Vec<Peer>,
    sessions: Vec<Ongoing>,
    /// The computers connected to this one right now, and
    /// controlling it: the reverse of `sessions`.
    watching: Vec<Watcher>,
    /// Whether FFmpeg is where this window's player loads it from, once
    /// looked at.
    ffmpeg_here: Option<bool>,
    settings: Option<Settings>,
    /// The account, when the service answers: the link if there is
    /// one, and the devices on it.
    account: Option<AccountState>,
    /// The three shortcuts, written as they are engraved on the keyboard
    /// plugged in, and nothing for those that have no key.
    shortcuts: Vec<(Doing, Option<String>)>,
    /// What this window runs, and the folder of the journals: asked
    /// for once, they do not change for the whole life of the program.
    version: String,
    folder: String,
}

impl Seen {
    /// One session at a time from this computer: two video windows at
    /// once cannot be driven.
    fn busy(&self, state: &State) -> bool {
        state.opening.is_some() || !self.sessions.is_empty()
    }

    /// The name a session's machine is recognised by.
    ///
    /// By the fingerprint and not the address: it is the only thing
    /// that stays put from one network to another.
    fn name_of(&self, session: &Ongoing) -> String {
        self.peers
            .iter()
            .find(|peer| peer.fingerprint == session.fingerprint)
            .map_or_else(|| session.towards.clone(), |peer| peer.name.clone())
    }
}

/// This computer's account, as the service holds it.
#[derive(Default, PartialEq)]
struct AccountState {
    /// The link, or nothing: without a link, the product knows no server.
    link: Option<Account>,
    /// The account's devices, this computer included, as the server
    /// reported them. Empty as long as it has said nothing.
    devices: Vec<Device>,
}

/// What is left to do for the product to work, said plainly and with what
/// it takes to fix it.
///
/// Without this, a missing engine reads "démarrage en cours" forever, and
/// a stopped service can only be repaired with a command.
struct ToDo {
    text: &'static str,
    button: &'static str,
    remedy: Remedy,
}

/// What the button of such a banner will do.
#[derive(Clone, Copy)]
enum Remedy {
    StartTheService,
    Ffmpeg,
}

/// What is happening while a session opens.
///
/// The title does not move for the whole opening: what is happening is
/// always the same thing, and a title that changes at every step reads
/// like news when there is none.
struct Opening {
    towards: String,
    detail: String,
    since: std::time::Instant,
}

/// The banner at the top. It serves both: what failed, and what succeeded
/// without leaving a trace anywhere else on screen. A red message to say
/// that all is well would read as a failure.
struct Notice {
    text: String,
    is_trouble: bool,
    since: std::time::Instant,
}

/* ---- Where the screen stands ----------------------------------------- */

/// What is open on top of the home window.
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Adding,
    Journal,
    Settings,
    /// Attaching to a server.
    Account,
    /// Renaming a device of the account.
    Renaming,
}

/// What scrolls, and how far it has scrolled.
#[derive(Clone, Copy, PartialEq)]
enum Scroller {
    Page,
    Dialogue,
    /// The journal's text, which scrolls on its own inside the dialogue
    /// that carries it, the way a page scrolls in a window.
    Lines,
}

/// Where the window stands: what is open, what is under the hand, what
/// is waiting for an answer.
struct State {
    screen: Screen,
    /// The scrolling of the page, that of the open dialogue, and that
    /// of the journal's text. The last one also scrolls sideways: a
    /// journal line does not wrap.
    scroll: f32,
    dialogue_scroll: f32,
    lines_scroll: (f32, f32),
    /// What each scrolling thing measured the last time it was drawn,
    /// the room it had, and the travel of its thumb: what it takes never
    /// to scroll past the end, and to drag the scrollbar at the same
    /// pace as the one that was drawn.
    extents: [(f32, f32, f32); 3],
    hover: Option<Target>,
    pressed: Option<Target>,
    /// The scrollbar held by a hand, and how far above its top the
    /// pointer was when the hand took it.
    held: Option<(Scroller, f32)>,
    /// The switches pushed that the service has not acknowledged
    /// yet. Without them, the state that comes back is still the old
    /// one and the switch would spring back under the finger.
    pushed: Vec<(Toggle, bool)>,
    /// The button that has just copied, and since when.
    copied: Option<(Target, std::time::Instant)>,
    /// The jargon fold, in the settings.
    advanced: bool,
    /// The key waiting for a combination. Only one at a time: two
    /// buttons waiting for the same key would share it.
    listening: Option<Doing>,
    /// Which computer the open journal belongs to. Nothing for this
    /// one: it is the only one whose files can also be emptied and
    /// whose folder can be opened.
    journal_of: Option<Peer>,
    /// What the open journal shows, line by line: cutting it up at every
    /// frame would mean reading it all again just to draw thirty of its
    /// lines.
    lines: Vec<String>,
    /// The sift this page answers, and nothing as long as no answer has
    /// arrived.
    ///
    /// "Copier le tri" takes the page as it is on screen: so it has to be
    /// known whether it really answers what is written in the box, or
    /// else the button would carry off the previous page under the sift's
    /// name.
    sift: Option<String>,
    /// The names the open page says it carries, to tick rather than
    /// type.
    ///
    /// They come from the page itself and never from a list kept here:
    /// half the time it comes from another computer, and a name offered
    /// that none of its lines carries would be a dead end on offer.
    tags: Vec<String>,
    /// The sift of the last question sent.
    ///
    /// Each question opens its own conversation with the service: two
    /// readings started one right after the other can come back in the
    /// other order, and the older one would overwrite the newer.
    sift_asked: String,
    /// When "Vider" started waiting for confirmation.
    emptying: Option<std::time::Instant>,
    notice: Option<Notice>,
    /// What the settings have to complain about, which lives in
    /// their dialogue.
    trouble: Option<String>,
    opening: Option<Opening>,
    /// In the account dialogue: creating the account rather than
    /// signing in to it.
    sign_up: bool,
    /// The key presented by a server nobody vouches for, waiting for
    /// the person to compare it and confirm it.
    pinning: Option<String>,
    /// Attaching in progress: the button awaits the answer.
    attaching: bool,
    /// When "Se détacher" started waiting for confirmation.
    detaching: Option<std::time::Instant>,
    /// Which device "Révoquer" is waiting to confirm, and since when.
    revocation: Option<(usize, std::time::Instant)>,
    /// The device being renamed: its identifier and its name.
    renaming: Option<(String, String)>,
}

impl State {
    const fn new() -> Self {
        State {
            screen: Screen::Home,
            scroll: 0.0,
            dialogue_scroll: 0.0,
            lines_scroll: (0.0, 0.0),
            extents: [(0.0, 0.0, 0.0); 3],
            hover: None,
            pressed: None,
            held: None,
            pushed: Vec::new(),
            copied: None,
            advanced: false,
            listening: None,
            journal_of: None,
            lines: Vec::new(),
            sift: None,
            tags: Vec::new(),
            sift_asked: String::new(),
            emptying: None,
            notice: None,
            trouble: None,
            opening: None,
            sign_up: false,
            pinning: None,
            attaching: false,
            detaching: None,
            revocation: None,
            renaming: None,
        }
    }

    /// What a scrolling thing measures, the room it has and the travel
    /// of its thumb: what bounds its scrolling and what drags it.
    fn measured(&self, which: Scroller) -> (f32, f32, f32) {
        self.extents[match which {
            Scroller::Page => 0,
            Scroller::Dialogue => 1,
            Scroller::Lines => 2,
        }]
    }

    fn keep(&mut self, which: Scroller, content: f32, visible: f32, travel: f32) {
        self.extents[match which {
            Scroller::Page => 0,
            Scroller::Dialogue => 1,
            Scroller::Lines => 2,
        }] = (content, visible, travel);
    }

    /// How far that thing is scrolled right now.
    fn scroll(&self, which: Scroller) -> f32 {
        match which {
            Scroller::Page => self.scroll,
            Scroller::Dialogue => self.dialogue_scroll,
            Scroller::Lines => self.lines_scroll.1,
        }
    }

    /// Scrolls, without ever leaving what there is to see.
    fn scroll_by(&mut self, which: Scroller, by: f32) {
        let (content, visible, _) = self.measured(which);
        let furthest = (content - visible).max(0.0);
        let now_at = (self.scroll(which) + by).clamp(0.0, furthest);
        match which {
            Scroller::Page => self.scroll = now_at,
            Scroller::Dialogue => self.dialogue_scroll = now_at,
            Scroller::Lines => self.lines_scroll.1 = now_at,
        }
    }
}

/* ---- What gets clicked ------------------------------------------------ */

/// A switch, and what it controls.
#[derive(Clone, Copy, PartialEq)]
enum Toggle {
    Access,
    Trust,
    AtBoot,
    Sound,
    Stats,
    /// The two network trial switches: marking the packets, and
    /// listening on the product's port.
    Marking,
    FixedPort,
}

/// A segmented choice: several options that exclude each other, shown
/// all together.
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Theme,
    Codec,
    Display,
    Mouse,
    /// In the account dialogue: signing in, or creating it.
    SignUp,
}

/// What can be clicked, and what it does.
#[derive(Clone, PartialEq)]
enum Target {
    OpenJournal,
    OpenSettings,
    CopyFingerprint,
    /// The button of a "what is left to do" banner.
    ToFix(usize),
    /// A computer's card, and that computer's journal.
    Peer(usize),
    JournalOf(usize),
    /// The same card, but through this network and nothing else: no
    /// server consulted, no going out of the house.
    Local(usize),
    /// Disconnects the computer controlling this one right now, on
    /// this card.
    Disconnect(usize),
    Add,
    Switch(Toggle),
    /// One of the names the journal page carries, by its rank: ticked, it
    /// is added to the sift box; unticked, it leaves it.
    Tag(usize),
    Segment(Pick, usize),
    Shortcut(Doing),
    /// Closing the open dialogue, whichever it is.
    Close,
    Connect,
    /// What the Enter key does in the open dialogue.
    Confirm,
    Forget(usize),
    Empty,
    Refresh,
    CopyJournal,
    /// Opening the journals' folder, from the journal or the settings.
    OpenTheJournals,
    Advanced,
    Scrollbar(Scroller),
    /// The account: opening the dialogue, attaching, confirming a
    /// server's key, detaching.
    OpenAccount,
    Attach,
    Pin,
    Detach,
    /// A device of the account, by its rank: opening its renaming, or
    /// revoking it.
    OpenRenaming(usize),
    Rename,
    Revoke(usize),
}

impl Toggle {
    /// Where the switch is, going by what the product says, and by what
    /// a hand has just pushed with no answer yet.
    fn is_on(self, seen: &Seen, state: &State) -> bool {
        if let Some((_, wanted)) = state.pushed.iter().find(|(target, _)| *target == self) {
            return *wanted;
        }
        match self {
            Toggle::Access => seen.machine.as_ref().is_some_and(|said| said.wanted),
            Toggle::Trust => seen.machine.as_ref().is_some_and(|said| said.trusting),
            Toggle::AtBoot => seen.machine.as_ref().is_some_and(|said| said.at_boot),
            Toggle::Marking => seen.machine.as_ref().is_some_and(|said| said.ecn),
            Toggle::FixedPort => seen.machine.as_ref().is_some_and(|said| said.fixed_port),
            Toggle::Sound => seen
                .settings
                .as_ref()
                .is_some_and(|said| said.mute_far_speakers),
            Toggle::Stats => seen
                .settings
                .as_ref()
                .is_some_and(|said| said.stats_overlay),
        }
    }

    /// Whether it can be pushed.
    ///
    /// A stopped service is not remote access turned off: one is a
    /// choice, the other a failure. The switch then stays in the chosen
    /// position and can no longer be moved, rather than jumping to "off"
    /// and suggesting a decision nobody made.
    fn enabled(self, seen: &Seen, state: &State) -> bool {
        if state.pushed.iter().any(|(target, _)| *target == self) {
            return false;
        }
        match self {
            Toggle::Sound | Toggle::Stats => seen.settings.is_some(),
            _ => seen
                .machine
                .as_ref()
                .is_some_and(|said| said.unreachable.is_none()),
        }
    }
}

impl Pick {
    /// The words on the sides, in the order they are shown.
    fn words(self) -> Vec<&'static str> {
        match self {
            Pick::Theme => Choice::ALL.iter().map(|choice| choice.word()).collect(),
            Pick::Codec => vec!["Auto", "H.264", "HEVC", "AV1"],
            Pick::Display => vec!["Plein écran", "Fenêtre"],
            Pick::Mouse => vec!["Bureau", "Jeu"],
            Pick::SignUp => vec!["J'ai un compte", "Créer un compte"],
        }
    }

    /// The value each side carries, as it travels and as it is
    /// written in the settings.
    fn values(self) -> Vec<&'static str> {
        match self {
            Pick::Theme | Pick::SignUp => Vec::new(),
            Pick::Codec => vec!["auto", "H.264", "HEVC", "AV1"],
            Pick::Display => vec!["fullscreen", "windowed"],
            Pick::Mouse => vec!["desktop", "game"],
        }
    }

    /// Which one is chosen, going by what the product says, or by what
    /// the window itself holds for the two that do not travel.
    fn current(self, seen: &Seen, state: &State) -> Option<usize> {
        let said = match self {
            Pick::Theme => {
                return Choice::ALL
                    .iter()
                    .position(|choice| *choice == crate::theme::chosen());
            }
            Pick::SignUp => return Some(usize::from(state.sign_up)),
            Pick::Codec => seen.settings.as_ref()?.codec.clone(),
            Pick::Display => seen.settings.as_ref()?.display.clone(),
            Pick::Mouse => {
                let desktop = seen.settings.as_ref()?.absolute_mouse;
                (if desktop { "desktop" } else { "game" }).to_string()
            }
        };
        self.values().iter().position(|value| *value == said)
    }

    /// Whether it can be changed.
    fn enabled(self, seen: &Seen) -> bool {
        match self {
            Pick::Theme | Pick::SignUp => true,
            _ => seen.settings.is_some(),
        }
    }
}

/* ---- What the settings screen holds ----------------------------------- */

/// What to decide with, on the right of a setting line.
enum Control {
    /// Nothing: the line only says where the product
    /// stands.
    Status,
    Switch(Toggle),
    Segments(Pick),
    Key(Doing),
    /// A button that opens something outside the window.
    Opens(&'static str, Target),
}

/// A line of the settings screen: what it is about on the left, what
/// to decide it with on the right.
struct Setting {
    label: &'static str,
    caption: &'static str,
    control: Control,
}

/// What the settings screen carries, in order.
enum Element {
    /// A section label, and the words that explain it.
    Section(&'static str, &'static str),
    /// The jargon fold: what follows only shows when open.
    Fold,
    Setting(Setting),
    /// The account: the link as it stands, and the devices on it.
    /// Drawn separately, because it does not have the shape of a line.
    Account,
}

/// The settings screen, line by line.
///
/// A table and not a string of calls: it is the same layout for all of
/// them, whether they carry a segmented choice, a switch, a key or a
/// button, and a table reads the way the screen reads.
///
/// What a session asks for (size, bitrate and codec) is set in its own
/// menu and not here: they are the three numbers one changes while
/// watching the picture they change, and coming back to this screen to
/// try one means moving away from the only thing that says whether it
/// worked. The first line recalls where they stand.
const SETTINGS: &[Element] = &[
    Element::Setting(Setting {
        label: "Ce qu'une session demande",
        caption: "",
        control: Control::Status,
    }),
    Element::Setting(Setting {
        label: "Thème",
        caption: "Suit Windows tant qu'on ne choisit pas.",
        control: Control::Segments(Pick::Theme),
    }),
    Element::Setting(Setting {
        label: "Ordinateurs du réseau local",
        caption: "Ceux qui s'annoncent sur ce réseau peuvent joindre celui-ci sans rien à \
                  recopier. Ne concerne que le réseau local.",
        control: Control::Switch(Toggle::Trust),
    }),
    Element::Setting(Setting {
        label: "Démarrer avec Windows",
        caption: "Cet ordinateur répond dès l'allumage, avant même qu'on ouvre une session \
                  dessus, et ZyrDesk revient tout seul avec l'icône. Sans cela, rien ne tourne \
                  tant que vous n'avez pas ouvert ZyrDesk.",
        control: Control::Switch(Toggle::AtBoot),
    }),
    Element::Section(
        "Essais réseau",
        "Deux façons de parler sur le fil, à comparer quand les sessions se coupent : changez \
         un seul interrupteur à la fois, sur les deux ordinateurs. Chaque changement rouvre la \
         porte de cet ordinateur, donc coupe une session ouverte vers lui.",
    ),
    Element::Setting(Setting {
        label: "Marquer les paquets (ECN)",
        caption: "Le tunnel pose sur chaque paquet le marquage de congestion que QUIC pose \
                  partout. Certains équipements traitent ces paquets à part : à couper pour \
                  voir si les coupures cessent.",
        control: Control::Switch(Toggle::Marking),
    }),
    Element::Setting(Setting {
        label: "Écouter sur le port 47000",
        caption: "Coupé, cet ordinateur écoute sur un port que Windows choisit à chaque \
                  démarrage. Une session par le compte le trouve ; une session en réseau local \
                  ou un renvoi de port fait sur la box, non.",
        control: Control::Switch(Toggle::FixedPort),
    }),
    Element::Section(
        "Compte",
        "Un serveur ZyrDesk retrouve vos ordinateurs où qu'ils soient et les présente l'un à \
         l'autre. Facultatif : sans compte, tout marche comme avant sur le réseau local.",
    ),
    Element::Account,
    Element::Section(
        "Raccourcis clavier",
        "Ils marchent pendant une session, par-dessus l'image. Cliquez sur une combinaison pour \
         la changer, puis tapez-la. Échap annule, Retour arrière la retire.",
    ),
    Element::Setting(Setting {
        label: "Terminer la session",
        caption: "Rend son bureau à l'ordinateur distant. Une session est en cours ou terminée, \
                  jamais entre les deux.",
        control: Control::Key(Doing::End),
    }),
    Element::Setting(Setting {
        label: "Ouvrir le menu flottant",
        caption: "Le seul chemin de retour après avoir masqué le bouton.",
        control: Control::Key(Doing::Menu),
    }),
    Element::Setting(Setting {
        label: "Fenêtré ou plein écran",
        caption: "Bascule l'image de l'un à l'autre.",
        control: Control::Key(Doing::Fullscreen),
    }),
    Element::Setting(Setting {
        label: "Écran suivant de l'hôte",
        caption: "Passe d'un écran de l'ordinateur distant au suivant, sans rien relancer. Ne \
                  fait rien quand cet ordinateur n'en a qu'un.",
        control: Control::Key(Doing::NextScreen),
    }),
    Element::Fold,
    Element::Setting(Setting {
        label: "Codec vidéo",
        caption: "Auto prend le meilleur que les deux ordinateurs savent lire.",
        control: Control::Segments(Pick::Codec),
    }),
    Element::Setting(Setting {
        label: "Fenêtre de la session",
        caption: "L'image s'affiche dans la fenêtre ZyrDesk : ce réglage dit si cette fenêtre \
                  prend l'écran entier.",
        control: Control::Segments(Pick::Display),
    }),
    Element::Setting(Setting {
        label: "Souris",
        caption: "La souris de jeu vise en mouvements plutôt qu'en position.",
        control: Control::Segments(Pick::Mouse),
    }),
    Element::Setting(Setting {
        label: "Couper le son de l'ordinateur distant",
        caption: "Ses enceintes se taisent pendant toute la session : la pièce où il se trouve \
                  reste silencieuse, et vous entendez tout. Le son y revient tout seul à la fin.",
        control: Control::Switch(Toggle::Sound),
    }),
    Element::Setting(Setting {
        label: "Statistiques par-dessus l'image",
        caption: "Images par seconde, débit, pertes.",
        control: Control::Switch(Toggle::Stats),
    }),
    Element::Setting(Setting {
        label: "Journaux",
        caption: "",
        control: Control::Opens("Ouvrir", Target::OpenTheJournals),
    }),
];

/* ---- What the stylesheet says, in page pixels ------------------------- */

mod layout {
    /// The width beyond which the page stops spreading, and what
    /// surrounds it: at the top and bottom, then on the sides.
    pub const PAGE: f32 = 820.0;
    pub const EDGE: f32 = 32.0;
    pub const SIDE: f32 = 24.0;

    /// The brand mark at the top of the page, and the one on the
    /// opening screen.
    pub const BRAND: f32 = 40.0;
    pub const BIG_BRAND: f32 = 56.0;

    /// A button, a big button, and the drawing on an icon button.
    pub const BUTTON: f32 = 36.0;
    pub const BIG_BUTTON: f32 = 44.0;
    pub const GLYPH: f32 = 18.0;

    /// The switch: its size, its thumb and the play around it.
    pub const SWITCH: (f32, f32) = (44.0, 26.0);
    pub const THUMB: f32 = 18.0;
    pub const SLACK: f32 = 3.0;

    /// One side of a segmented choice, and what surrounds the
    /// group.
    pub const SEGMENT: f32 = 26.0;
    pub const AROUND: f32 = 2.0;
    pub const SEGMENT_RADIUS: f32 = 6.0;

    /// The presence dot, and the ring around the one that is live.
    pub const DOT: f32 = 8.0;
    pub const RING: f32 = 3.0;

    /// An input field.
    pub const FIELD: f32 = 40.0;

    /// The thickness of a line and of a border.
    pub const HAIRLINE: f32 = 1.0;

    /// A computer card is never narrower than this.
    pub const CARD: f32 = 240.0;
    /// How tall a computer card is, the room for the word that only
    /// appears on hover included.
    pub const HINT: f32 = 20.0;

    /// The width of the three dialogues.
    pub const DIALOGUE: f32 = 460.0;
    pub const DIALOGUE_SETTINGS: f32 = 560.0;
    pub const DIALOGUE_JOURNAL: f32 = 880.0;

    /// A shortcut's key is never narrower than this.
    pub const KEY: f32 = 150.0;

    /// The thread that goes back and forth while a session opens, and
    /// the share of its length covered by the piece that moves along it.
    pub const THREAD: (f32, f32) = (260.0, 3.0);
    pub const PIECE: f32 = 0.4;

    /// The scrollbar, and what separates it from the edge.
    pub const SCROLLBAR: f32 = 6.0;

    /// The drawing of the empty screen.
    pub const EMPTY: (f32, f32) = (64.0, 44.0);

    /// How far one notch of the wheel scrolls.
    pub const NOTCH: f32 = 60.0;

    /// How much black a dialogue lays over what it covers.
    pub const VEIL: f32 = 0.55;

    /// The height of the journal's text: the most it takes, as a share
    /// of the window, and never more than this.
    pub const JOURNAL: (f32, f32) = (0.6, 560.0);

    /// And never less than this, whatever the rest of the dialogue
    /// takes: a journal window in which the journal can no longer be
    /// seen is no longer one.
    pub const JOURNAL_AT_LEAST: f32 = 140.0;
}

/* ---- What the window holds -------------------------------------------- */

/// The window carrying the drawing, a child of the one the system frames.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// How much a page pixel counts for here, in hundredths.
static SCALE: AtomicU32 = AtomicU32::new(100);

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
static STATE: Mutex<State> = Mutex::new(State::new());
/// What the last frame laid down that answers the click.
static CLICKABLES: Mutex<Vec<(Target, Rect)>> = Mutex::new(Vec::new());
/// The program, kept here because nothing hands one to a system window.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

// This window's canvas, held by the thread that owns it: a drawing
// surface and the window it dresses belong to the thread that made
// them.
thread_local! {
    static CANVAS: std::cell::RefCell<Option<Canvas>> = const { std::cell::RefCell::new(None) };
}

fn scale() -> f32 {
    SCALE.load(Ordering::Relaxed) as f32 / 100.0
}

fn palette() -> Palette {
    design::palette(crate::theme::light())
}

fn program() -> Option<App> {
    PROGRAM.lock().expect("programme de l'accueil").clone()
}

/* ---- The window -------------------------------------------------------- */

/// Opens the home window's canvas in the window the system frames.
///
/// A child window and not the window itself: the outer one belongs to
/// the toolkit, which places it, moves it and frames it. What is ours is
/// its inside, and that is exactly what a child window is.
///
/// On the thread that owns the outer window: a window belongs to the
/// thread that made it, and a window made elsewhere would never hear a
/// mouse.
pub fn raise(app: &App) {
    let outer = crate::main_window::handle() as windows_sys::Win32::Foundation::HWND;
    if outer.is_null() {
        note("accueil : pas de fenêtre où dessiner");
        return;
    }
    *PROGRAM.lock().expect("programme de l'accueil") = Some(app.clone());
    SCALE.store(
        (crate::main_window::scale() * 100.0).round() as u32,
        Ordering::Relaxed,
    );
    build(outer);
    watch(app.clone());
}

/// The canvas, as the system knows it.
///
/// Read by the window that carries it, which resizes it along with itself
/// and gives it the keyboard.
pub fn its_canvas() -> isize {
    ITS_WINDOW.load(Ordering::Relaxed)
}

/// How much a page pixel counts for on the screen the window is on.
///
/// Asked for again when it moves to another screen or when the screen's
/// magnification changes: everything drawn follows from it, the font of
/// the input fields included.
pub fn measure_the_screen(app: &App) {
    let wanted = (crate::main_window::scale() * 100.0).round() as u32;
    if SCALE.swap(wanted, Ordering::Relaxed) == wanted {
        return;
    }
    // The input fields are system windows: their font does not rescale
    // with the rest, it has to be made again for them.
    dress_the_fields();
    redraw(app);
}

/// Builds the canvas and puts itself in front of the messages of the
/// window that carries it.
fn build(outer: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, GetClientRect, IDC_ARROW, LoadCursorW,
        RegisterClassW, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
    };

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let class_name = wide("ZyrDeskAccueil");
    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of the program, whose rectangle is read into
    // ours.
    if unsafe { GetClientRect(outer, &mut inside) } == 0 {
        note("accueil : la fenêtre ne dit pas sa taille");
        return;
    }

    // SAFETY: a class declared once and a window built on it, on the
    // thread that will pump its messages. A class declared twice is
    // refused with no other effect, hence the answer left unread.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            // Redrawn whole as soon as its size changes, like the two
            // other surfaces this program paints itself. Without this,
            // the system only asks for a new frame for the strip that
            // has just appeared, and nothing at all when the window
            // shrinks: the page stayed laid out for the previous size,
            // centred on a width that no longer existed, so shifted to
            // the right and cut off. Visible when coming back from a
            // maximised window, invisible when opening it at that size.
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            // No background: everything this window shows is painted by
            // us, and a background laid by the system would be one more
            // colour, seen for the length of a frame at every resize.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            // Clipped by its siblings: a session's picture is laid on
            // top of it in the same window, and without this the home
            // window would redraw itself behind it at every frame.
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            inside.right,
            inside.bottom,
            outer,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("accueil : la toile n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    note(&format!(
        "accueil dessiné par ZyrDesk, sans vue web : toile de {}x{} px",
        inside.right, inside.bottom
    ));
}

/// Asks for a new frame, from any thread.
pub fn redraw(app: &App) {
    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

        // SAFETY: a window of ours, on the thread that owns it.
        unsafe { InvalidateRect(window as HWND, std::ptr::null(), 0) };
    });
}

/// What the canvas answers when the system speaks to it.
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
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, EN_CHANGE, HTCLIENT, IDC_ARROW, IDC_HAND, KillTimer, LoadCursorW,
        SetCursor, SetTimer, WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND, WM_KEYDOWN,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR,
        WM_SYSKEYDOWN, WM_TIMER,
    };

    match message {
        // Nothing to erase: every frame covers the whole window, and an
        // erase by the system in between would be a blink of bare
        // background.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            repaint(window);
            0
        }
        // Requested and not painted right away: painting when nothing
        // has been invalidated paints nothing at all, since the system
        // then lends only an empty surface.
        ANIMATE => {
            invalidate(window);
            0
        }
        WM_MOUSEMOVE => {
            moves(window, where_is(with));
            0
        }
        WM_MOUSELEAVE => {
            mouse_left(window);
            0
        }
        WM_LBUTTONDOWN => {
            // SAFETY: a window of ours, given the keyboard so that
            // Escape, Enter and the combinations arrive here.
            unsafe { SetFocus(window) };
            presses(window, where_is(with));
            0
        }
        WM_LBUTTONUP => {
            releases(window, where_is(with));
            0
        }
        WM_MOUSEWHEEL => {
            let notches = ((holding >> 16) & 0xFFFF) as i16;
            let across = (holding & 0x0004) != 0;
            wheel(window, f32::from(notches) / 120.0, across);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if key_down(window, holding as u32, with) {
                return 0;
            }
            // SAFETY: same.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            let cursor_shape = if STATE.lock().expect("accueil").hover.is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: a system pointer shape, asked for by name.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), cursor_shape)) };
            1
        }
        // An input field's background, and the ink inside it: they belong
        // to the system, which asks here what colour to paint them so
        // that they match the colour of the rest.
        WM_CTLCOLOREDIT => tint_of_the_field(holding),
        // A field whose text changes also changes what the dialogue
        // says below it and what its button allows.
        WM_COMMAND if (holding >> 16) as u32 & 0xFFFF == EN_CHANGE => {
            invalidate(window);
            // And the sift one reads the journal again by itself: one
            // types, the page narrows, with nothing to click. Only for the
            // journal from here: reading the far one again opens a road
            // all the way to the other machine, and one per pause in the
            // typing would cost seconds. Over there, it is "Actualiser" or
            // Enter that reads.
            //
            // Read then released: the drawing holds the state while it
            // reads the fields, and taking them here in the other order
            // would be two threads waiting for each other.
            let sift_box = FIELDS.lock().expect("accueil")[Field::Sift.rank()];
            if with == sift_box && STATE.lock().expect("accueil").journal_of.is_none() {
                // SAFETY: a timer set on a window of ours, from the
                // thread that owns it. Setting it again restarts it
                // from zero, which is why one more letter pushes the
                // reading back instead of adding another one.
                unsafe { SetTimer(window, SIFT_PAUSE, SIFT_PAUSE_MS, None) };
            }
            0
        }
        WM_TIMER if holding == SIFT_PAUSE => {
            // SAFETY: a timer of ours, on the thread that set it.
            unsafe { KillTimer(window, SIFT_PAUSE) };
            if let Some(app) = program() {
                reread_the_journal(&app, After::Show);
            }
            0
        }
        _ => {
            // What an input field asked for, done here because both
            // close the dialogue and so destroy that field.
            if message == ACT {
                if let Some(app) = program() {
                    act(
                        &app,
                        if holding == 1 {
                            Target::Confirm
                        } else {
                            Target::Close
                        },
                    );
                }
                return 0;
            }
            // SAFETY: the system's answer to everything not
            // answered here.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
    }
}

/// The message nothing in the system sends, by which an input field
/// asks the canvas to do what it cannot do itself.
const ACT: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// One more frame of the thread that goes back and forth, asked for
/// by the pulse.
const ANIMATE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 2;

/// The timer giving the sift time to be written before reading again.
const SIFT_PAUSE: usize = 1;

/// How long the last letter is given, in milliseconds.
///
/// The sift box behaves like a logcat's: one types, the page narrows,
/// with nothing to click. One question per letter would read the four
/// files again nine times for "clipboard", so it is the letter nobody
/// follows that sets off the reading.
const SIFT_PAUSE_MS: u32 = 300;

/// Where the mouse is, in real pixels from the canvas's corner.
fn where_is(with: windows_sys::Win32::Foundation::LPARAM) -> (f32, f32) {
    let x = (with & 0xFFFF) as i16;
    let y = ((with >> 16) & 0xFFFF) as i16;
    (f32::from(x), f32::from(y))
}

/// A word in the characters Windows counts in, ended by the zero it
/// looks for.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Draws the home window and pours it into the window.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let mut inside = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a window of ours, whose rectangle is read into ours.
    if unsafe { GetClientRect(window, &mut inside) } == 0 {
        return;
    }
    let (width, height) = (inside.right.max(1), inside.bottom.max(1));

    let mut paint: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    // SAFETY: a window of ours, whose surface is given back below.
    let surface = unsafe { BeginPaint(window, &mut paint) };
    if surface.is_null() {
        return;
    }
    // Every frame says again where the fields go: a field the frame
    // no longer lays down, because the dialogue has changed shape,
    // has no place any more and is put away.
    *PLACES.lock().expect("accueil") = [None; Field::COUNT];
    CANVAS.with_borrow_mut(|canvas| {
        if canvas
            .as_ref()
            .is_none_or(|already| already.size() != (width, height))
        {
            *canvas = Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };
        let colours = palette();
        canvas.begin(colours.background);
        let clickables = paint_page(canvas, width as f32, height as f32, colours);
        if !canvas.finish() {
            return;
        }
        *CLICKABLES.lock().expect("accueil") = clickables;
        canvas.copy_to(windows::Win32::Graphics::Gdi::HDC(surface), 0, 0);
    });
    // SAFETY: the painting opened just above.
    unsafe { EndPaint(window, &paint) };
    place_the_fields();
    clock(window);
}

/// Makes the home window beat while a thread goes back and forth, and
/// stops it afterwards.
///
/// The only thing in the home window that moves without anyone touching
/// anything. Everywhere else, nothing is redrawn as long as nothing
/// changes.
fn clock(window: windows_sys::Win32::Foundation::HWND) {
    if STATE.lock().expect("accueil").opening.is_some() {
        crate::pulse::beat(window, ANIMATE);
    } else {
        crate::pulse::stop(window);
    }
}

/* ---- The drawing ------------------------------------------------------- */

/// What lays out the home window: the canvas, what is shown, where things
/// stand, and what answers the click once laid down.
struct Painter<'a> {
    canvas: &'a Canvas,
    scale: f32,
    colours: Palette,
    seen: &'a Seen,
    state: &'a State,
    clickables: Vec<(Target, Rect)>,
    /// What each scrolling thing measures, picked up along the way and
    /// handed to the state once the walk is over.
    measures: Vec<(Scroller, f32, f32, f32)>,
    /// False when a dialogue is open: the page behind no longer answers
    /// the click, and what is drawn underneath no longer lights up under
    /// the mouse.
    live: bool,
    /// True when the walk only measures: nothing is laid down, and what
    /// comes back is the height it would take.
    silent: bool,
}

impl Painter<'_> {
    /// A length from the design system, in real pixels.
    fn px(&self, page: f32) -> f32 {
        page * self.scale
    }

    /// A pen of this page size.
    fn pen(&self, size: f32) -> Pen {
        Pen::of(self.px(size))
    }

    fn body(&self) -> Pen {
        self.pen(design::BODY)
    }

    fn caption(&self) -> Pen {
        self.pen(design::CAPTION)
    }

    fn subtitle(&self) -> Pen {
        self.pen(design::SUBTITLE).in_bold()
    }

    /// A section's label: small, in capitals, spaced out, and never
    /// loud.
    fn section(&self, at: Rect, text: &str) {
        self.draw_text(
            &text.to_uppercase(),
            self.caption().in_bold().spaced(0.08),
            self.colours.text_faint,
            at,
        );
    }

    /// The height of a line written with this pen.
    fn line_height(&self, pen: Pen) -> f32 {
        self.canvas.line_height(pen)
    }

    /// The height of a block wrapped at this width.
    fn height_of(&self, text: &str, pen: Pen, width: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.canvas.height_of(text, pen, width)
    }

    /// Writes a block within this width, from this top down, and gives
    /// back what it took.
    fn block(&self, left: f32, top: f32, width: f32, text: &str, pen: Pen, ink: Colour) -> f32 {
        let height = self.height_of(text, pen, width);
        if height > 0.0 {
            self.draw_text(text, pen, ink, Rect::at(left, top, width, height));
        }
        height
    }

    /// A card: its shadow, its background and its outline.
    fn card(&self, at: Rect) {
        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(at, radius, self.colours.surface_1);
        self.stroke(at, radius, self.colours.border);
    }

    /// A card waiting to be filled: no background, a dashed outline.
    fn waiting_card(&self, at: Rect) {
        let radius = self.px(design::RADIUS_LARGE);
        self.dashed(at, radius, self.colours.border_strong);
    }

    /// A separating line, across this whole width.
    fn separator(&self, left: f32, top: f32, width: f32) {
        self.fill(
            Rect::at(left, top, width, self.px(layout::HAIRLINE)),
            0.0,
            self.colours.border,
        );
    }

    /// Whether this thing is under the hand, and whether it is
    /// pressed.
    fn under_the_hand(&self, target: &Target) -> bool {
        self.live && self.state.hover.as_ref() == Some(target)
    }

    fn is_pressed(&self, target: &Target) -> bool {
        self.live && self.state.pressed.as_ref() == Some(target)
    }

    /// Notes that this answers the click.
    ///
    /// Never during a measure: a thing measured is not laid down, and
    /// what is not laid down cannot be clicked. A dialogue is measured
    /// whole before it is drawn, at a place that is not its own, and
    /// taking those places for buttons would make clickable a corner of
    /// the window where there is nothing.
    fn answers(&mut self, target: Target, at: Rect) {
        if self.live && !self.silent {
            self.clickables.push((target, at));
        }
    }

    /// The presence dot.
    fn dot(&self, at: Rect, ink: Colour, live: bool) {
        let radius = (at.right - at.left) / 2.0;
        if live {
            let ring = self.px(layout::RING);
            self.fill(at.grown(ring), radius + ring, ink.faded(0.18));
        }
        self.fill(at, radius, ink);
    }
}

/// What a button is: what calls for the click, what goes along with it,
/// and what warns before destroying.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Quiet,
    Warning,
}

impl Painter<'_> {
    /// How wide a button is: its word and what surrounds it.
    fn button_width(&self, text: &str, big: bool) -> f32 {
        let pen = if big { self.subtitle() } else { self.body() };
        let around = if big {
            design::SPACE_5
        } else {
            design::SPACE_4
        };
        self.canvas.width_of(text, pen) + self.px(around) * 2.0
    }

    /// A button carrying a word, at this place.
    fn button(&mut self, at: Rect, text: &str, kind: Kind, target: Target, enabled: bool) {
        let radius = self.px(design::RADIUS_SMALL);
        let colours = self.colours;
        let hovered = enabled && self.under_the_hand(&target);
        let (background, ink, edge) = match kind {
            Kind::Primary if !enabled => (colours.surface_3, colours.text_faint, None),
            Kind::Primary if hovered => (colours.accent_bright, colours.on_accent, None),
            Kind::Primary => (colours.accent, colours.on_accent, None),
            Kind::Quiet if hovered => {
                (colours.surface_2, colours.text, Some(colours.border_strong))
            }
            Kind::Quiet => (
                Colour::TRANSPARENT,
                colours.text_soft,
                Some(colours.border_strong),
            ),
            Kind::Warning => (
                Colour::TRANSPARENT,
                colours.warning,
                Some(colours.warning.mixed_with(colours.border, 0.55)),
            ),
        };
        let opacity = if enabled { 1.0 } else { 0.45 };
        self.fill(at, radius, background.faded(opacity));
        if let Some(edge) = edge {
            self.stroke(at, radius, edge.faded(opacity));
        }
        let big = at.bottom - at.top > self.px((layout::BUTTON + layout::BIG_BUTTON) / 2.0);
        let pen = if big { self.subtitle() } else { self.body() }.aligned(Align::Centre);
        self.draw_text(text, pen, ink.faded(opacity), at);
        if enabled {
            self.answers(target, at);
        }
    }

    /// A button that carries only a drawing, and keeps the same height
    /// as those that carry a word.
    fn icon_button(&mut self, at: Rect, icon: &'static Icon, target: Target, quiet: bool) {
        let radius = self.px(design::RADIUS_SMALL);
        let hovered = self.under_the_hand(&target);
        if hovered {
            self.fill(at, radius, self.colours.surface_2);
        }
        let ink = if hovered {
            self.colours.text
        } else if quiet {
            self.colours.text_soft.faded(0.4)
        } else {
            self.colours.text_soft
        };
        let side = self.px(layout::GLYPH);
        let middle = ((at.left + at.right) / 2.0, (at.top + at.bottom) / 2.0);
        self.icon(
            icon,
            Rect::at(middle.0 - side / 2.0, middle.1 - side / 2.0, side, side),
            ink,
        );
        self.answers(target, at);
    }

    /// The switch: its rail, and the thumb that slides in it.
    fn switch(&mut self, left: f32, middle: f32, button: Toggle) -> Rect {
        let (width, height) = (self.px(layout::SWITCH.0), self.px(layout::SWITCH.1));
        let at = Rect::at(left, middle - height / 2.0, width, height);
        let is_on = button.is_on(self.seen, self.state);
        let enabled = button.enabled(self.seen, self.state);
        let opacity = if enabled { 1.0 } else { 0.45 };
        let radius = height / 2.0;
        let (background, stroke, thumb) = if is_on {
            (
                self.colours.accent,
                self.colours.accent,
                self.colours.on_accent,
            )
        } else {
            (
                self.colours.surface_3,
                self.colours.border_strong,
                self.colours.text_soft,
            )
        };
        self.fill(at, radius, background.faded(opacity));
        self.stroke(at, radius, stroke.faded(opacity));

        let side = self.px(layout::THUMB);
        let slack = self.px(layout::SLACK);
        let x = if is_on {
            at.right - slack - side
        } else {
            at.left + slack
        };
        self.fill(
            Rect::at(x, at.top + slack, side, side),
            side / 2.0,
            thumb.faded(opacity),
        );
        if enabled {
            self.answers(Target::Switch(button), at);
        }
        at
    }

    /// How wide a segmented choice is.
    fn segments_width(&self, target: Pick) -> f32 {
        let around = self.px(layout::AROUND);
        let sides: f32 = target
            .words()
            .iter()
            .map(|text| self.side_width(text))
            .sum();
        sides + around * 2.0 + self.px(layout::AROUND) * (target.words().len() as f32 - 1.0)
    }

    fn side_width(&self, text: &str) -> f32 {
        self.canvas.width_of(text, self.caption()) + self.px(design::SPACE_3) * 2.0
    }

    /// A segmented choice, laid out from this right edge.
    fn segments(&mut self, right: f32, middle: f32, target: Pick) -> Rect {
        let width = self.segments_width(target);
        let around = self.px(layout::AROUND);
        let height = self.px(layout::SEGMENT) + around * 2.0;
        let at = Rect::at(right - width, middle - height / 2.0, width, height);
        let radius = self.px(design::RADIUS_SMALL);
        let enabled = target.enabled(self.seen);
        let opacity = if enabled { 1.0 } else { 0.45 };
        self.fill(at, radius, self.colours.surface_2.faded(opacity));
        self.stroke(at, radius, self.colours.border.faded(opacity));

        let picked = target.current(self.seen, self.state);
        let mut x = at.left + around;
        for (rank, text) in target.words().iter().enumerate() {
            let side = Rect::at(
                x,
                at.top + around,
                self.side_width(text),
                self.px(layout::SEGMENT),
            );
            let this_one = Target::Segment(target, rank);
            let ink = if picked == Some(rank) {
                let round = self.px(layout::SEGMENT_RADIUS);
                self.shadow(side, round, self.colours.shadow_1);
                self.fill(side, round, self.colours.surface_1.faded(opacity));
                self.colours.text
            } else if enabled && self.under_the_hand(&this_one) {
                self.colours.text_soft
            } else {
                self.colours.text_faint
            };
            self.draw_text(
                text,
                self.caption().aligned(Align::Centre),
                ink.faded(opacity),
                side,
            );
            if enabled {
                self.answers(this_one, side);
            }
            x = side.right + around;
        }
        at
    }

    /// A banner: what it has to say, and what it takes to fix it when
    /// there is something to do.
    fn banner(
        &mut self,
        left: f32,
        top: f32,
        width: f32,
        text: &str,
        alert: bool,
        action: Option<(&str, Target)>,
    ) -> f32 {
        let inside = self.px(design::SPACE_4);
        let button = action.map(|(text, target)| (self.button_width(text, false), text, target));
        let button_room = button
            .as_ref()
            .map_or(0.0, |(width, _, _)| width + self.px(design::SPACE_4));
        let text_width = width - inside * 2.0 - button_room;
        let text_height = self.height_of(text, self.body(), text_width);
        let height = (text_height + self.px(design::SPACE_3) * 2.0).max(if button.is_some() {
            self.px(layout::BUTTON) + self.px(design::SPACE_3) * 2.0
        } else {
            0.0
        });
        let at = Rect::at(left, top, width, height);
        let radius = self.px(design::RADIUS);
        let (background, edge) = if alert {
            (
                self.colours.error.mixed_with(self.colours.surface_2, 0.08),
                self.colours.error.mixed_with(self.colours.border, 0.4),
            )
        } else {
            (self.colours.surface_2, self.colours.border)
        };
        self.fill(at, radius, background);
        self.stroke(at, radius, edge);
        self.draw_text(
            text,
            self.body(),
            self.colours.text_soft,
            Rect::at(
                at.left + inside,
                (at.top + at.bottom) / 2.0 - text_height / 2.0,
                text_width,
                text_height,
            ),
        );
        if let Some((button_width, text, target)) = button {
            let button_height = self.px(layout::BUTTON);
            self.button(
                Rect::at(
                    at.right - inside - button_width,
                    (at.top + at.bottom) / 2.0 - button_height / 2.0,
                    button_width,
                    button_height,
                ),
                text,
                Kind::Quiet,
                target,
                true,
            );
        }
        height
    }

    /// The scrollbar of a thing that scrolls, when there is more to see
    /// than room.
    ///
    /// `corner` is the rounding of what scrolls: the thumb stops where
    /// the corner begins, or else it would stick out of the shape it
    /// runs along.
    fn scrollbar(&mut self, which: Scroller, at: Rect, content: f32, corner: f32) {
        let visible = at.bottom - at.top;
        if content <= visible + 1.0 {
            self.measures.push((which, content, visible, 0.0));
            return;
        }
        let width = self.px(layout::SCROLLBAR);
        let slack = self.px(design::SPACE_1);
        let rail = (visible - corner * 2.0).max(0.0);
        let top = thumb_of(rail, visible, content, self.scale);
        let travel = rail - top;
        self.measures.push((which, content, visible, travel));
        let position = (self.state.scroll(which) / (content - visible)).clamp(0.0, 1.0);
        let thumb = Rect::at(
            at.right - width - slack,
            at.top + corner + travel * position,
            width,
            top,
        );
        let hovered = self.under_the_hand(&Target::Scrollbar(which));
        let ink = if hovered {
            self.colours.text_faint
        } else {
            self.colours.border_strong
        };
        self.fill(thumb, width / 2.0, ink);
        self.answers(Target::Scrollbar(which), thumb);
    }
}

/// The height of a scrollbar's thumb: on its rail, the share of what
/// there is that can be seen, and never so small that it can no longer
/// be caught.
fn thumb_of(rail: f32, visible: f32, content: f32, scale: f32) -> f32 {
    (rail * visible / content)
        .max(design::SPACE_5 * scale)
        .min(rail)
}

/* ---- The page ---------------------------------------------------------- */

/// Draws everything on screen and returns what answers the click.
fn paint_page(canvas: &Canvas, width: f32, height: f32, colours: Palette) -> Vec<(Target, Rect)> {
    let nothing = Seen::default();
    let guard = SEEN.lock().expect("accueil");
    let seen = guard.as_ref().unwrap_or(&nothing);
    let mut state = STATE.lock().expect("accueil");

    let (clickables, measures) = {
        let dialogue_open = state.screen != Screen::Home;
        let opening = state.opening.is_some();
        let mut painter = Painter {
            canvas,
            scale: scale(),
            colours,
            seen,
            state: &state,
            clickables: Vec::new(),
            measures: Vec::new(),
            live: !dialogue_open && !opening,
            silent: false,
        };
        painter.page(width, height);
        if dialogue_open && !opening {
            // The darkened background: what is behind is no longer
            // current and no longer answers the click, which the page
            // already said by making its dialogue modal.
            painter.fill(
                Rect::at(0.0, 0.0, width, height),
                0.0,
                Colour::BLACK.faded(layout::VEIL),
            );
            painter.live = true;
            painter.dialogue(width, height);
        }
        if opening {
            painter.opening(width, height);
        }
        (painter.clickables, painter.measures)
    };
    for (which, content, visible, travel) in measures {
        state.keep(which, content, visible, travel);
    }
    clickables
}

impl Painter<'_> {
    /// The home window itself: what this computer is, then the others.
    fn page(&mut self, width: f32, height: f32) {
        let side = self.px(layout::SIDE);
        let inside = (width - side * 2.0).clamp(self.px(200.0), self.px(layout::PAGE));
        let x = ((width - inside) / 2.0).max(side);
        let start = self.px(layout::EDGE) - self.state.scroll;
        let mut y = start;

        y += self.header(x, y, inside);
        y += self.px(design::SPACE_5);
        y += self.this_computer(x, y, inside);
        y += self.px(design::SPACE_5);
        y += self.my_computers(x, y, inside, height);

        // The version is in sight without ever weighing on anything: at
        // the bottom of the window when the page does not fill its
        // height, and right after the rest when it goes beyond it.
        let version = self.line_height(self.caption());
        let content = y - start + self.px(layout::EDGE) + version;
        let version_y =
            (height - self.px(layout::EDGE) - version).max(y + self.px(design::SPACE_2));
        let (text, ink) = self.the_version();
        self.draw_text(
            &text,
            self.caption().aligned(Align::Centre),
            ink,
            Rect::at(x, version_y, inside, version),
        );

        self.scrollbar(
            Scroller::Page,
            Rect::at(0.0, 0.0, width, height),
            content,
            0.0,
        );
    }

    /// The brand mark, the product's name, and the two controls
    /// tucked away on the right: within reach, never at the centre
    /// of attention.
    fn header(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let brand = self.px(layout::BRAND);
        self.brand(Rect::at(x, y, brand, brand));

        let button = self.px(layout::BUTTON);
        let gap = self.px(design::SPACE_2);
        let middle = y + (brand - button) / 2.0;
        let settings = Rect::at(x + width - button, middle, button, button);
        let journal = settings.shifted(-(button + gap), 0.0);
        self.icon_button(settings, &icons::SETTINGS, Target::OpenSettings, false);
        self.icon_button(journal, &icons::JOURNAL, Target::OpenJournal, false);

        let since = x + brand + self.px(design::SPACE_3);
        self.draw_text(
            "ZyrDesk",
            self.pen(design::TITLE).in_bold().ellipsized(),
            self.colours.text,
            Rect {
                left: since,
                top: y,
                right: journal.left - gap,
                bottom: y + brand,
            },
        );
        brand
    }

    /// What this computer is: its name, its state, its fingerprint, and
    /// what is left to do for it to work.
    fn this_computer(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let tag = self.line_height(self.caption().in_bold());
        self.section(Rect::at(x, y, width, tag), "Cet ordinateur");
        let mut taken = tag + self.px(design::SPACE_3);
        taken += self.machine_card(x, y + taken, width);
        for (rank, to_do) in what_is_missing(self.seen).into_iter().enumerate() {
            taken += self.px(design::SPACE_3);
            taken += self.banner(
                x,
                y + taken,
                width,
                to_do.text,
                true,
                Some((to_do.button, Target::ToFix(rank))),
            );
        }
        taken
    }

    /// This machine's card: its identity at the top, its fingerprint at
    /// the bottom on its own background.
    fn machine_card(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let inside = self.px(design::SPACE_5);
        let (name, state) = (
            self.line_height(self.subtitle()),
            self.line_height(self.body()),
        );
        let gap = self.px(design::SPACE_1);
        let top_part = (name + gap + state).max(self.px(layout::SWITCH.1)) + inside * 2.0;

        let caption = self.line_height(self.caption());
        let copy_width = self.button_width("Copier", false);
        let seen = self.seen;
        let said = seen.machine.as_ref();
        let fingerprint = said.map_or_else(String::new, |said| {
            if said.fingerprint.is_empty() {
                "indisponible".to_string()
            } else {
                said.fingerprint.clone()
            }
        });
        let fingerprint_width = width - inside * 2.0 - copy_width - self.px(design::SPACE_4);
        let fingerprint_pen = self.caption().monospaced().spaced(0.02);
        let fingerprint_height = self.height_of(&fingerprint, fingerprint_pen, fingerprint_width);
        let bottom_part = (caption + gap + fingerprint_height).max(self.px(layout::BUTTON))
            + self.px(design::SPACE_3) * 2.0;

        let at = Rect::at(x, y, width, top_part + bottom_part);
        self.card(at);
        let bottom = Rect {
            top: at.top + top_part,
            ..at
        };
        // The background of the card's bottom part: the same rounded
        // rectangle, seen through its lower half, which no rounded
        // rectangle can be on its own.
        let radius = self.px(design::RADIUS_LARGE);
        if !self.silent {
            let (canvas, background) = (self.canvas, self.colours.surface_2);
            canvas.clipped(bottom, || canvas.fill(at, radius, background));
        }
        self.separator(bottom.left, bottom.top, width);
        self.stroke(at, radius, self.colours.border);

        // The top: the name, the state, and the remote access switch.
        let middle = (at.top + bottom.top) / 2.0;
        let access_caption = "Accès distant";
        let access_width = self.canvas.width_of(access_caption, self.caption());
        let switch = self.switch(
            at.right - inside - self.px(layout::SWITCH.0),
            middle,
            Toggle::Access,
        );
        self.draw_text(
            access_caption,
            self.caption(),
            self.colours.text_soft,
            Rect::at(
                switch.left - self.px(design::SPACE_3) - access_width,
                middle - caption / 2.0,
                access_width,
                caption,
            ),
        );

        let name_width = switch.left - self.px(design::SPACE_4) - (at.left + inside);
        let name_top = middle - (name + gap + state) / 2.0;
        let status_text =
            said.map_or_else(|| "Recherche du service…".to_string(), words_of_the_state);
        self.draw_text(
            &said.map_or_else(|| "…".to_string(), |said| said.name.clone()),
            self.subtitle().ellipsized(),
            self.colours.text,
            Rect::at(at.left + inside, name_top, name_width, name),
        );
        let dot = self.px(layout::DOT);
        let below_the_text = name_top + name + gap;
        self.dot(
            Rect::at(
                at.left + inside,
                below_the_text + (state - dot) / 2.0,
                dot,
                dot,
            ),
            said.map_or(self.colours.offline, |said| {
                colour_of_the_state(said, self.colours)
            }),
            said.is_some_and(|said| said.hosting),
        );
        self.draw_text(
            &status_text,
            self.body().ellipsized(),
            self.colours.text_soft,
            Rect::at(
                at.left + inside + dot + self.px(design::SPACE_2),
                below_the_text,
                name_width - dot - self.px(design::SPACE_2),
                state,
            ),
        );

        // The bottom: the fingerprint, and how to copy it.
        let bottom_top =
            (bottom.top + bottom.bottom) / 2.0 - (caption + gap + fingerprint_height) / 2.0;
        self.draw_text(
            "Empreinte de cet ordinateur",
            self.caption(),
            self.colours.text_soft,
            Rect::at(bottom.left + inside, bottom_top, fingerprint_width, caption),
        );
        self.draw_text(
            &fingerprint,
            fingerprint_pen,
            self.colours.text_faint,
            Rect::at(
                bottom.left + inside,
                bottom_top + caption + gap,
                fingerprint_width,
                fingerprint_height,
            ),
        );
        let height = self.px(layout::BUTTON);
        let can = said.is_some_and(|said| !said.fingerprint.is_empty());
        self.button(
            Rect::at(
                bottom.right - inside - copy_width,
                (bottom.top + bottom.bottom) / 2.0 - height / 2.0,
                copy_width,
                height,
            ),
            if self.state.copied.as_ref().map(|(target, _)| target)
                == Some(&Target::CopyFingerprint)
            {
                "Copié"
            } else {
                "Copier"
            },
            Kind::Quiet,
            Target::CopyFingerprint,
            can,
        );
        top_part + bottom_part
    }
}

/// What keeps the product from working, said plainly and with what it
/// takes to fix it.
///
/// Outside the walk because the click reads it again: a banner's button
/// carries only its rank, and it is here that the rank finds again what
/// it repairs.
fn what_is_missing(seen: &Seen) -> Vec<ToDo> {
    let mut missings = Vec::new();
    if seen
        .machine
        .as_ref()
        .is_some_and(|said| said.unreachable.is_some())
    {
        missings.push(ToDo {
            text: "Le service ZyrDesk ne tourne pas. Cet ordinateur ne peut ni être \
                    contrôlé ni en contrôler un autre.",
            button: "Démarrer le service",
            remedy: Remedy::StartTheService,
        });
    }
    // One folder to fill, whoever noticed it empty: this window, whose
    // player decodes with FFmpeg, or the service, whose engine encodes
    // with it. Said once either way.
    let the_service_lacks_it = seen
        .machine
        .as_ref()
        .is_some_and(|said| said.wanted && said.holdup == "engineMissing");
    if seen.ffmpeg_here == Some(false) || the_service_lacks_it {
        missings.push(ToDo {
            text: "FFmpeg manque : cet ordinateur ne peut ni être contrôlé ni en contrôler \
                    un autre. Déposez-le dans son dossier, il sera repris tout seul.",
            button: "Ouvrir le dossier",
            remedy: Remedy::Ffmpeg,
        });
    }
    missings
}
impl Painter<'_> {
    /// The other computers, and what is happening right now.
    ///
    /// A session in progress comes before the list: it is the first
    /// thing to see when opening the window, including when the window
    /// is not the one that started it.
    fn my_computers(&mut self, x: f32, y: f32, width: f32, height: f32) -> f32 {
        let tag = self.line_height(self.caption().in_bold());
        self.section(Rect::at(x, y, width, tag), "Mes ordinateurs");
        let mut taken = tag + self.px(design::SPACE_3);

        for session in &self.seen.sessions {
            taken += self.session_card(x, y + taken, width, session);
            taken += self.px(design::SPACE_3);
        }
        if let Some(notice) = &self.state.notice {
            let text = notice.text.clone();
            let is_trouble = notice.is_trouble;
            taken += self.banner(x, y + taken, width, &text, is_trouble, None);
            taken += self.px(design::SPACE_3);
        }

        if self.seen.peers.is_empty() && !self.seen.busy(self.state) {
            return taken + self.no_computer(x, y + taken, width, height - y - taken);
        }
        // What a contact has shared is put apart: it is not one's own
        // computer, and saying so on each card is not enough to tell
        // them apart at a glance.
        let (mine_ranks, shared): (Vec<usize>, Vec<usize>) =
            (0..self.seen.peers.len()).partition(|rank| {
                self.seen.peers[*rank]
                    .account
                    .as_ref()
                    .is_none_or(|account| account.shared_by.is_none())
            });
        taken += self.grid(x, y + taken, width, &mine_ranks, true);
        if !shared.is_empty() {
            taken += self.px(design::SPACE_5);
            self.section(Rect::at(x, y + taken, width, tag), "Partagés avec moi");
            taken += tag + self.px(design::SPACE_3);
            taken += self.grid(x, y + taken, width, &shared, false);
        }
        taken
    }

    /// The banner of a session in progress.
    ///
    /// The computer's card, further down, already carries its address
    /// and its state: this says what is happening, it does not repeat
    /// them.
    fn session_card(&mut self, x: f32, y: f32, width: f32, session: &Ongoing) -> f32 {
        let inside = self.px(design::SPACE_5);
        let (name, text) = (
            self.line_height(self.subtitle()),
            self.line_height(self.caption()),
        );
        let gap = self.px(design::SPACE_1);
        let at = Rect::at(x, y, width, name + gap + text + inside * 2.0);
        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(
            at,
            radius,
            self.colours.online.mixed_with(self.colours.surface_1, 0.07),
        );
        self.stroke(
            at,
            radius,
            self.colours.online.mixed_with(self.colours.border, 0.4),
        );

        let dot = self.px(layout::DOT);
        self.dot(
            Rect::at(
                at.left + inside,
                at.top + inside + (name - dot) / 2.0,
                dot,
                dot,
            ),
            self.colours.online,
            true,
        );
        let since = at.left + inside + dot + self.px(design::SPACE_2);
        self.draw_text(
            &format!("Session en cours vers {}", self.seen.name_of(session)),
            self.subtitle().ellipsized(),
            self.colours.text,
            Rect::at(since, at.top + inside, at.right - inside - since, name),
        );
        self.draw_text(
            &format!(
                "Ouverte depuis {}{}. Fermer la fenêtre termine la session.",
                duration(session.since),
                path_of(session)
            ),
            self.caption(),
            self.colours.text_soft,
            Rect::at(
                at.left + inside,
                at.top + inside + name + gap,
                width - inside * 2.0,
                text,
            ),
        );
        at.bottom - at.top
    }

    /// The grid of those computers, by their rank, and the tile that adds
    /// one when it belongs here.
    fn grid(&mut self, x: f32, y: f32, width: f32, ranks: &[usize], with_add: bool) -> f32 {
        let gap = self.px(design::SPACE_3);
        let narrowest = self.px(layout::CARD);
        let columns = (((width + gap) / (narrowest + gap)).floor() as usize).max(1);
        let column = (width - gap * (columns as f32 - 1.0)) / columns as f32;
        let inside = self.px(design::SPACE_4);
        let (name, address) = (
            self.line_height(self.subtitle()),
            self.line_height(self.caption()),
        );
        let height = inside * 2.0
            + name
            + self.px(design::SPACE_2)
            + address
            + self.px(design::SPACE_2)
            + self.px(layout::HINT);

        let how_many = ranks.len() + usize::from(with_add);
        for cell in 0..how_many {
            let at = Rect::at(
                x + (cell % columns) as f32 * (column + gap),
                y + (cell / columns) as f32 * (height + gap),
                column,
                height,
            );
            match ranks.get(cell) {
                Some(rank) => self.computer_card(at, *rank, inside, name, address),
                None => self.add_tile(at),
            }
        }
        let lines = how_many.div_ceil(columns) as f32;
        lines * height + (lines - 1.0) * gap
    }

    /// A computer's dot: green when it answers or when the account
    /// says it is ready, orange when the account says it is online
    /// without remote access, grey otherwise. The word under the name
    /// says why.
    fn presence_of(&self, peer: &Peer) -> (Colour, bool) {
        if peer.seen {
            return (self.colours.online, true);
        }
        match &peer.account {
            Some(account) if account.online && account.access == Access::Ready => {
                (self.colours.online, true)
            }
            Some(account) if account.online => (self.colours.warning, false),
            _ => (self.colours.offline, false),
        }
    }

    /// A computer card: clicking anywhere connects to it, and the buttons
    /// for its journal and its local way sit in a corner.
    fn computer_card(&mut self, at: Rect, rank: usize, inside: f32, name: f32, address: f32) {
        let peer = &self.seen.peers[rank];
        let busy = self.seen.busy(self.state);
        // The local way is only offered for a computer this network
        // announces: it is the only thing it can reach, since that is
        // the only address that comes from here.
        let here = peer.seen;
        let local_only = here && !busy && self.under_the_hand(&Target::Local(rank));
        let in_session = self
            .seen
            .sessions
            .iter()
            .any(|session| session.fingerprint == peer.fingerprint);
        // The reverse of `in_session`: not a computer this window has
        // reached, but the one controlling it right now.
        let controlling = self
            .seen
            .watching
            .iter()
            .any(|watching| watching.fingerprint == peer.fingerprint);
        let target = Target::Peer(rank);
        let hovered = !busy && self.under_the_hand(&target);
        // Pressed under the finger: one pixel down, which is what the
        // style sheet did and all that says a click has been taken.
        let at = if self.is_pressed(&target) {
            at.shifted(0.0, self.px(1.0))
        } else {
            at
        };

        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(
            at,
            radius,
            if hovered {
                self.colours.surface_2
            } else {
                self.colours.surface_1
            },
        );
        let edge = if in_session {
            self.colours.online.mixed_with(self.colours.border, 0.4)
        } else if controlling {
            self.colours.warning.mixed_with(self.colours.border, 0.4)
        } else if hovered {
            self.colours.accent
        } else {
            self.colours.border
        };
        self.stroke(at, radius, edge);

        // A busy card fades out through its words, so that its journal
        // button stays lit: it is precisely during a session that one
        // wants to read what the far machine has written. The one
        // controlling this computer does not fade either: it is
        // precisely that card one wants to see.
        let opacity = if busy && !in_session && !controlling {
            0.5
        } else {
            1.0
        };
        let dot = self.px(layout::DOT);
        let (ink, live) = self.presence_of(peer);
        self.dot(
            Rect::at(
                at.left + inside,
                at.top + inside + (name - dot) / 2.0,
                dot,
                dot,
            ),
            ink.faded(opacity),
            live,
        );
        let since = at.left + inside + dot + self.px(design::SPACE_2);
        let button = self.px(layout::BUTTON);
        // The room for the corner buttons is set aside: without it, a
        // slightly long name would run underneath them, and there is
        // one more when this computer can be reached from here, and
        // another one when it controls this one.
        let buttons = self.px(design::SPACE_6)
            + if here { button } else { 0.0 }
            + if controlling { button } else { 0.0 };
        self.draw_text(
            &peer.name,
            self.subtitle().ellipsized(),
            self.colours.text.faded(opacity),
            Rect::at(
                since,
                at.top + inside,
                (at.right - buttons - since).max(0.0),
                name,
            ),
        );
        // The grey dot says nothing on its own: what explains it is
        // written next to it.
        let below_the_name = at.top + inside + name + self.px(design::SPACE_2);
        self.draw_text(
            &below_the_name_of(peer),
            self.caption().ellipsized(),
            self.colours.text_soft.faded(opacity),
            Rect::at(
                at.left + inside,
                below_the_name,
                at.right - inside - (at.left + inside),
                address,
            ),
        );
        // What only appears on hover does not make the card move: its
        // room is set aside in advance. The word says which of the two
        // ways the hand is choosing: without it, the house in the corner
        // would be a drawing with no name.
        let hint = self.px(layout::HINT);
        if in_session || hovered || local_only || controlling {
            self.draw_text(
                match (in_session, local_only, controlling) {
                    (true, _, _) => "Session en cours",
                    (false, true, _) => "Se connecter en local",
                    (false, false, true) => "Vous contrôle actuellement",
                    (false, false, false) => "Se connecter",
                },
                self.caption(),
                if in_session {
                    self.colours.online
                } else if controlling {
                    self.colours.warning
                } else {
                    self.colours.accent
                },
                Rect::at(
                    at.left + inside,
                    below_the_name + address + self.px(design::SPACE_2),
                    at.right - inside * 2.0,
                    hint,
                ),
            );
        }

        if !busy {
            self.answers(target, at);
        }
        // Always there and never in the foreground: it waits to be
        // looked for, and it does not fade when a session takes up
        // the window.
        let corner = self.px(design::SPACE_3);
        self.icon_button(
            Rect::at(at.right - corner - button, at.top + corner, button, button),
            &icons::JOURNAL,
            Target::JournalOf(rank),
            !hovered,
        );
        // The house, next to it: through this network and nothing else.
        // It follows the card rather than the journal, since it opens a
        // session and one more session does not open.
        if here && !busy {
            self.icon_button(
                Rect::at(
                    at.right - corner - button * 2.0,
                    at.top + corner,
                    button,
                    button,
                ),
                &icons::LOCAL_NETWORK,
                Target::Local(rank),
                !hovered,
            );
        }
        // Always there when that computer controls this one, busy or
        // not: that is precisely when one wants to be able to send it
        // back. Takes the room set aside after the journal and, if there
        // is one, the local house, exactly as the width calculation
        // above counted them.
        if controlling {
            let slot = if here { 3.0 } else { 2.0 };
            self.icon_button(
                Rect::at(
                    at.right - corner - button * slot,
                    at.top + corner,
                    button,
                    button,
                ),
                &icons::CROSS,
                Target::Disconnect(rank),
                !hovered,
            );
        }
    }

    /// The tile that adds a computer: it follows the rhythm of the others
    /// without passing itself off as a computer.
    fn add_tile(&mut self, at: Rect) {
        let busy = self.seen.busy(self.state);
        let hovered = !busy && self.under_the_hand(&Target::Add);
        if hovered {
            self.fill(at, self.px(design::RADIUS_LARGE), self.colours.surface_2);
        }
        self.waiting_card(at);
        let ink = if busy {
            self.colours.text_soft.faded(0.5)
        } else if hovered {
            self.colours.text
        } else {
            self.colours.text_soft
        };
        let sign = self.px(layout::GLYPH);
        let text = self.line_height(self.caption());
        let gap = self.px(design::SPACE_1);
        let top = (at.top + at.bottom) / 2.0 - (sign + gap + text) / 2.0;
        self.icon(
            &icons::PLUS,
            Rect::at((at.left + at.right) / 2.0 - sign / 2.0, top, sign, sign),
            ink,
        );
        self.draw_text(
            "Ajouter un ordinateur",
            self.caption().aligned(Align::Centre),
            ink,
            Rect::at(at.left, top + sign + gap, at.right - at.left, text),
        );
        if !busy {
            self.answers(Target::Add, at);
        }
    }

    /// The empty screen: what is seen on a machine that has not found
    /// anyone yet.
    fn no_computer(&mut self, x: f32, y: f32, width: f32, rest: f32) -> f32 {
        let drawing = self.px(layout::EMPTY.1);
        let title = self.line_height(self.subtitle());
        let text_width = width.min(self.px(420.0)) - self.px(design::SPACE_5) * 2.0;
        let text = "Les ZyrDesk allumés sur ce réseau apparaissent ici tout seuls, sans rien à \
                   recopier. Si le réseau ne laisse pas passer les annonces, ajoutez l'autre \
                   ordinateur à la main, sur les deux machines. Avec un compte, dans les \
                   réglages, vos ordinateurs apparaissent où qu'ils soient.";
        let explanation = self.height_of(text, self.caption(), text_width);
        let button = self.px(layout::BIG_BUTTON);
        let gap = self.px(design::SPACE_3);
        let inside = self.px(design::SPACE_6);
        let content =
            drawing + gap + title + gap + explanation + gap + self.px(design::SPACE_2) + button;
        let height = (content + inside * 2.0).max(rest - self.px(layout::EDGE));

        let at = Rect::at(x, y, width, height);
        self.waiting_card(at);
        let mut top = (at.top + at.bottom) / 2.0 - content / 2.0;
        let middle = (at.left + at.right) / 2.0;
        self.icon(
            &icons::NO_COMPUTER,
            Rect::at(
                middle - self.px(layout::EMPTY.0) / 2.0,
                top,
                self.px(layout::EMPTY.0),
                drawing,
            ),
            self.colours.text_faint.faded(0.6),
        );
        top += drawing + gap;
        self.draw_text(
            "Aucun ordinateur pour l'instant",
            self.subtitle().aligned(Align::Centre),
            self.colours.text,
            Rect::at(at.left, top, width, title),
        );
        top += title + gap;
        self.draw_text(
            text,
            self.caption().aligned(Align::Centre),
            self.colours.text_soft,
            Rect::at(middle - text_width / 2.0, top, text_width, explanation),
        );
        top += explanation + gap + self.px(design::SPACE_2);
        let button_width = self.button_width("Ajouter un ordinateur", true);
        self.button(
            Rect::at(middle - button_width / 2.0, top, button_width, button),
            "Ajouter un ordinateur",
            Kind::Primary,
            Target::Add,
            true,
        );
        height
    }

    /// What this window runs, and what the service runs when the
    /// two do not date from the same day.
    fn the_version(&self) -> (String, Colour) {
        let mine = &self.seen.version;
        if mine.is_empty() {
            return (String::new(), self.colours.text_faint);
        }
        let service = self
            .seen
            .machine
            .as_ref()
            .map_or("", |said| said.service_build.as_str());
        if service.is_empty() || mine.contains(service) {
            return (mine.clone(), self.colours.text_faint);
        }
        (
            format!("{mine}, mais le service tourne encore en {service}"),
            self.colours.warning,
        )
    }
}

/// What this machine's state reads as.
fn words_of_the_state(said: &Standing) -> String {
    if said.unreachable.is_some() {
        return "Service arrêté".to_string();
    }
    if !said.wanted {
        return "Accès distant désactivé".to_string();
    }
    if said.hosting {
        return "Prêt à être contrôlé".to_string();
    }
    match said.holdup {
        "engineMissing" => "FFmpeg absent".to_string(),
        _ => "Démarrage en cours…".to_string(),
    }
}

/// And the colour of its dot. The state is never read from the colour
/// alone: the text next to it says it.
fn colour_of_the_state(said: &Standing, colours: Palette) -> Colour {
    if said.unreachable.is_some() || !said.wanted {
        return colours.offline;
    }
    if said.hosting {
        return colours.online;
    }
    if said.holdup == "starting" {
        colours.warning
    } else {
        colours.error
    }
}

/// What is written under a computer's name: what is known about it,
/// and where it comes from when that is not the network.
///
/// A computer that announces itself shows its address. A computer
/// that only the account carries shows what the account says of it:
/// online and ready, online without remote access and why, or
/// offline.
fn below_the_name_of(peer: &Peer) -> String {
    let origin = match &peer.account {
        Some(account) => match &account.shared_by {
            Some(by_whom) => format!("partagé par {by_whom}"),
            None => "compte".to_string(),
        },
        None if peer.written => "ajouté à la main".to_string(),
        None => String::new(),
    };
    let state = match &peer.account {
        Some(account) if !peer.seen => {
            if account.online {
                format!("en ligne · {}", account.access.explanation())
            } else {
                "hors ligne".to_string()
            }
        }
        _ => peer.address.clone(),
    };
    if origin.is_empty() {
        state
    } else {
        format!("{state} · {origin}")
    }
}

/// Where a device of the account stands, in words.
fn words_of_the_presence(device: &Device) -> String {
    if device.online {
        return format!("En ligne · {}", device.access.explanation());
    }
    match device.last_seen {
        Some(seen) => format!(
            "Hors ligne · vu il y a {}",
            duration(zyr_broker::now().saturating_sub(seen))
        ),
        None => "Hors ligne".to_string(),
    }
}

/// What a session goes through, and how long the road takes,
/// when the service knows.
fn path_of(session: &Ongoing) -> String {
    if session.via.is_empty() {
        return String::new();
    }
    format!(", par {} en {} ms", session.via, session.round_trip_ms)
}

/// How long a session has been open, in words.
fn duration(seconds: u64) -> String {
    if seconds < MINUTE {
        return "moins d'une minute".to_string();
    }
    let minutes = (seconds % HOUR) / MINUTE;
    if seconds < HOUR {
        return format!("{minutes} minute{}", if minutes > 1 { "s" } else { "" });
    }
    let hours = seconds / HOUR;
    if minutes == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {minutes:02}")
    }
}

/* ---- The dialogues ------------------------------------------------------ */

impl Painter<'_> {
    /// Lays down the open dialogue: what it carries, measured at the
    /// width it will have, then drawn inside it.
    ///
    /// Measuring and drawing are the same walk: a dialogue measured at
    /// one width and drawn at another would add up only until the first
    /// word that wraps.
    fn dialogue(&mut self, width: f32, height: f32) {
        let wanted = self.px(match self.state.screen {
            Screen::Adding | Screen::Account | Screen::Renaming => layout::DIALOGUE,
            Screen::Journal => layout::DIALOGUE_JOURNAL,
            Screen::Settings | Screen::Home => layout::DIALOGUE_SETTINGS,
        });
        let inside = self.px(design::SPACE_5);
        let margin = self.px(design::SPACE_6);
        let dialogue_width = wanted.min(width - margin);

        let content = self.inside(
            Rect::at(0.0, 0.0, dialogue_width - inside * 2.0, 0.0),
            true,
            height,
        ) + inside * 2.0;
        let dialogue_height = content.min(height - margin);
        let at = Rect::at(
            (width - dialogue_width) / 2.0,
            (height - dialogue_height) / 2.0,
            dialogue_width,
            dialogue_height,
        );
        self.dialogue_background(at);
        // Clipped to its card: what has scrolled above the top of the
        // dialogue, or below its bottom, would otherwise be drawn over
        // the darkened background.
        let canvas = self.canvas;
        canvas.clipped(at, || {
            self.inside(
                Rect {
                    left: at.left + inside,
                    top: at.top + inside - self.state.dialogue_scroll,
                    right: at.right - inside,
                    bottom: at.bottom,
                },
                false,
                height,
            );
        });
        self.scrollbar(
            Scroller::Dialogue,
            at,
            content,
            self.px(design::RADIUS_LARGE),
        );
    }

    /// What the open dialogue carries, measured when `silent` and drawn
    /// otherwise.
    fn inside(&mut self, at: Rect, silent: bool, height: f32) -> f32 {
        let before = self.silent;
        self.silent = before || silent;
        let taken = match self.state.screen {
            Screen::Adding => self.in_the_adding(at),
            Screen::Journal => self.in_the_journal(at, height),
            Screen::Settings => self.in_the_settings(at),
            Screen::Account => self.in_the_account(at),
            Screen::Renaming => self.in_the_renaming(at),
            // There is no dialogue then, and nothing calls this: spelled
            // out rather than filed under another screen, which it is
            // not.
            Screen::Home => 0.0,
        };
        self.silent = before;
        taken
    }

    /// Lays down the background of a dialogue.
    fn dialogue_background(&mut self, at: Rect) {
        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_2);
        self.fill(at, radius, self.colours.surface_1);
        self.stroke(at, radius, self.colours.border_strong);
    }

    /// A dialogue's header: what it is about, and the cross that
    /// closes it.
    fn dialogue_header(&mut self, at: Rect, title: &str, text: &str) -> f32 {
        let button = self.px(layout::BUTTON);
        let text_width = at.right - at.left - button - self.px(design::SPACE_4);
        let title_height = self.line_height(self.subtitle());
        let explanation = self.height_of(text, self.caption(), text_width);
        let gap = self.px(design::SPACE_1);

        self.draw_text(
            title,
            self.subtitle(),
            self.colours.text,
            Rect::at(at.left, at.top, text_width, title_height),
        );
        self.block(
            at.left,
            at.top + title_height + gap,
            text_width,
            text,
            self.caption(),
            self.colours.text_soft,
        );
        self.icon_button(
            Rect::at(at.right - button, at.top, button, button),
            &icons::CROSS,
            Target::Close,
            false,
        );
        (title_height + gap + explanation).max(button)
    }

    /// What a thing would take, without laying it down.
    ///
    /// For what has to be measured before what comes above it is laid
    /// down: the dialogue is drawn from top to bottom, and nothing else
    /// makes it possible to give back to one thing the room another will
    /// take further down.
    fn measure_only(&mut self, pass: impl FnOnce(&mut Self) -> f32) -> f32 {
        let before = self.silent;
        self.silent = true;
        let taken = pass(self);
        self.silent = before;
        taken
    }

    /// The names the open page carries, in rows that wrap, giving back
    /// the height they took.
    ///
    /// Ticked rather than typed, because they are what one wants nine
    /// times out of ten and learning them by heart is nobody's job.
    /// Nothing at all for a page that announces none, which is the case
    /// of a page that came from an older half of the product: everything
    /// is typed then, as before.
    fn the_tags(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let names = self.state.tags.clone();
        if names.is_empty() {
            return 0.0;
        }
        let ticked: Vec<String> = text_of_the_field(Field::Sift)
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let height = self.px(layout::BUTTON);
        let between = self.px(design::SPACE_2);
        let (mut line, mut bottom) = (x, y);
        for (rank, name) in names.iter().enumerate() {
            let taken = self.button_width(name, false);
            // Wrapped as soon as a name would overflow, never before: a
            // narrow dialogue puts two per row and a wide one puts them
            // all on one, without anything having to be counted in
            // advance.
            if line > x && line + taken > x + width {
                line = x;
                bottom += height + between;
            }
            self.button(
                Rect::at(line, bottom, taken, height),
                name,
                if ticked.iter().any(|text| text == name) {
                    Kind::Primary
                } else {
                    Kind::Quiet
                },
                Target::Tag(rank),
                true,
            );
            line += taken + between;
        }
        bottom + height + self.px(design::SPACE_3) - y
    }

    /// A row of actions, lined up on the right, giving back its height.
    ///
    /// What destroys goes on the left, apart from the rest: it must not
    /// be under the finger that aims next to it.
    fn actions(&mut self, at: Rect, top: f32, actions: &[(String, Kind, Target, bool)]) -> f32 {
        let height = self.px(layout::BUTTON);
        let gap = self.px(design::SPACE_3);
        let mut right = at.right;
        for (text, kind, target, enabled) in actions.iter().rev() {
            let width = self.button_width(text, false);
            self.button(
                Rect::at(right - width, top, width, height),
                text,
                *kind,
                target.clone(),
                *enabled,
            );
            right -= width + gap;
        }
        height
    }

    /// Adding a computer, and removing those that were added.
    fn in_the_adding(&mut self, at: Rect) -> f32 {
        let width = at.right - at.left;
        let mut y = at.top;
        let gap = self.px(design::SPACE_4);

        let title = self.line_height(self.subtitle());
        self.draw_text(
            "Ajouter un ordinateur",
            self.subtitle(),
            self.colours.text,
            Rect::at(at.left, y, width, title),
        );
        y += title + self.px(design::SPACE_2);
        y += self.block(
            at.left,
            y,
            width,
            "À n'utiliser que si l'ordinateur n'apparaît pas tout seul. Les deux informations se \
             lisent dans sa fenêtre ZyrDesk. À faire sur les deux machines : chacune doit \
             connaître l'autre.",
            self.caption(),
            self.colours.text_soft,
        );
        y += gap;

        for field in Field::ADD {
            y += self.field(at.left, y, width, field);
            y += gap;
        }

        let towards_it = !text_of_the_field(Field::Address).trim().is_empty();
        let can = text_of_the_field(Field::Fingerprint).trim().len() == FINGERPRINT_LENGTH
            && !(towards_it && self.seen.busy(self.state));
        y += self.px(design::SPACE_1);
        y += self.actions(
            at,
            y,
            &[
                ("Annuler".to_string(), Kind::Quiet, Target::Close, true),
                (
                    if towards_it {
                        "Se connecter"
                    } else {
                        "Autoriser"
                    }
                    .to_string(),
                    Kind::Primary,
                    Target::Connect,
                    can,
                ),
            ],
        );

        // What was added by hand is removed where it was added: a card on
        // the home window is a whole button, and a second button laid on
        // it would steal its click.
        let written_ranks = self.written_ranks();
        if !written_ranks.is_empty() {
            y += self.px(design::SPACE_5);
            let inside = self.px(design::SPACE_5);
            self.separator(at.left - inside, y, width + inside * 2.0);
            y += self.px(design::SPACE_4);
            let tag = self.line_height(self.caption().in_bold());
            self.section(
                Rect::at(at.left, y, width, tag),
                "Ordinateurs ajoutés à la main",
            );
            y += tag + self.px(design::SPACE_3);
            let button = self.px(layout::BUTTON);
            let forget_width = self.button_width("Oublier", false);
            for rank in written_ranks {
                let peer = &self.seen.peers[rank];
                let text = format!("{} · {}", peer.name, peer.address);
                self.draw_text(
                    &text,
                    self.caption().ellipsized(),
                    self.colours.text_soft,
                    Rect::at(
                        at.left,
                        y,
                        (width - forget_width - self.px(design::SPACE_3)).max(0.0),
                        button,
                    ),
                );
                self.button(
                    Rect::at(at.left + width - forget_width, y, forget_width, button),
                    "Oublier",
                    Kind::Quiet,
                    Target::Forget(rank),
                    true,
                );
                y += button + self.px(design::SPACE_2);
            }
            y -= self.px(design::SPACE_2);
        }
        y - at.top
    }

    /// The computers written down by hand, by their rank.
    fn written_ranks(&self) -> Vec<usize> {
        self.seen
            .peers
            .iter()
            .enumerate()
            .filter(|(_, peer)| peer.written)
            .map(|(rank, _)| rank)
            .collect()
    }

    /// An input field: its label, the place of the real field Windows
    /// carries, and what it has to complain about.
    fn field(&mut self, x: f32, y: f32, width: f32, field: Field) -> f32 {
        let tag = self.line_height(self.caption());
        let gap = self.px(design::SPACE_2);
        let height = self.px(layout::FIELD);
        self.draw_text(
            field.label(),
            self.caption(),
            self.colours.text_soft,
            Rect::at(x, y, width, tag),
        );
        let place = Rect::at(x, y + tag + gap, width, height);
        let radius = self.px(design::RADIUS_SMALL);
        self.fill(place, radius, self.colours.surface_2);
        self.stroke(place, radius, self.colours.border);
        if !self.silent {
            place_the_field(field, place);
        }

        let text = field.hint();
        let helper = self.height_of(&text, self.caption(), width);
        if !text.is_empty() {
            self.block(
                x,
                place.bottom + self.px(design::SPACE_1),
                width,
                &text,
                self.caption(),
                if field == Field::Fingerprint {
                    self.colours.warning
                } else {
                    self.colours.text_soft
                },
            );
        }
        tag + gap
            + height
            + if helper > 0.0 {
                self.px(design::SPACE_1) + helper
            } else {
                0.0
            }
    }

    /// The journal, this computer's or the far one's.
    fn in_the_journal(&mut self, at: Rect, window_height: f32) -> f32 {
        let width = at.right - at.left;
        let distant = self.state.journal_of.clone();
        let (title, text) = match &distant {
            Some(peer) => (
                format!("Journal de {}", peer.name),
                "Ce que l'ordinateur distant a écrit chez lui, lu d'ici, à copier tel quel en cas \
                 de problème."
                    .to_string(),
            ),
            None => (
                "Journal".to_string(),
                "Tout ce que le produit a écrit, à copier tel quel en cas de problème.".to_string(),
            ),
        };
        let mut y = at.top;
        y += self.dialogue_header(at, &title, &text);
        y += self.px(design::SPACE_4);

        // What the names will take, measured before laying down the
        // lines. It is up to the lines to give them that room: the
        // dialogue has to fit in the window, and one more row of names
        // making it grow would put "Copier" out of reach.
        let names = self.measure_only(|painter| painter.the_tags(at.left, y, width));

        // The journal is read in whole lines: it takes what room it
        // can, without ever pushing its dialogue out of the window, and
        // never less than enough to read a few of them.
        let lines = ((window_height * layout::JOURNAL.0).min(self.px(layout::JOURNAL.1)) - names)
            .max(self.px(layout::JOURNAL_AT_LEAST));
        self.the_lines(Rect::at(at.left, y, width, lines));
        y += lines + self.px(design::SPACE_4);

        // The names this page carries, then the box they fill: the
        // page narrows by itself to what is written there, and it is
        // that page "Copier" takes. Empty, nothing is sifted.
        y += self.the_tags(at.left, y, width);
        y += self.field(at.left, y, width, Field::Sift);
        y += self.px(design::SPACE_3);

        let emptying = self
            .state
            .emptying
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        let copied =
            self.state.copied.as_ref().map(|(target, _)| target) == Some(&Target::CopyJournal);
        let mut row: Vec<(String, Kind, Target, bool)> = Vec::new();
        // Opening the folder only makes sense on one's own computer: the
        // far one is on the other machine. Emptying does make sense: one
        // empties both journals, does again what does not work, and reads
        // both.
        if distant.is_none() {
            row.push((
                "Ouvrir le dossier".to_string(),
                Kind::Quiet,
                Target::OpenTheJournals,
                true,
            ));
        }
        row.push(("Actualiser".to_string(), Kind::Quiet, Target::Refresh, true));
        let sifted = !text_of_the_field(Field::Sift).trim().is_empty();
        row.push((
            if copied {
                "Copié"
            } else if sifted {
                "Copier le tri"
            } else {
                "Copier tout"
            }
            .to_string(),
            Kind::Primary,
            Target::CopyJournal,
            true,
        ));
        let height = self.actions(at, y, &row);
        // "Vider" sits at the opposite end from "Copier": the two are
        // clicked within the same minute, and a mistake costs
        // everything one was about to copy.
        let empty_width = self.button_width(if emptying { "Confirmer" } else { "Vider" }, false);
        self.button(
            Rect::at(at.left, y, empty_width, height),
            if emptying { "Confirmer" } else { "Vider" },
            if emptying { Kind::Warning } else { Kind::Quiet },
            Target::Empty,
            true,
        );
        y + height - at.top
    }

    /// The journal's text, which scrolls on its own: the most recent is
    /// at the bottom, and a journal line does not wrap.
    fn the_lines(&mut self, at: Rect) {
        let radius = self.px(design::RADIUS_SMALL);
        self.fill(at, radius, self.colours.surface_2);
        self.stroke(at, radius, self.colours.border);

        let inside = self.px(design::SPACE_4);
        let pen = self.caption().monospaced().overflowing();
        let height = self.line_height(pen);
        let inside_the_box = at.grown(-inside);
        let lines = &self.state.lines;
        let content = height * lines.len() as f32;
        let (across, asked) = self.state.lines_scroll;
        let scroll = asked.min((content - (inside_the_box.bottom - inside_the_box.top)).max(0.0));

        let canvas = self.canvas;
        let silent = self.silent;
        let colours = self.colours;
        canvas.clipped(inside_the_box, || {
            if silent {
                return;
            }
            let first = (scroll / height).floor().max(0.0) as usize;
            let how_many =
                ((inside_the_box.bottom - inside_the_box.top) / height).ceil() as usize + 1;
            for (rank, line) in lines.iter().enumerate().skip(first).take(how_many) {
                canvas.draw_text(
                    line,
                    pen,
                    colours.text_soft,
                    Rect::at(
                        inside_the_box.left - across,
                        inside_the_box.top + rank as f32 * height - scroll,
                        FAR_AWAY,
                        height,
                    ),
                );
            }
        });
        self.scrollbar(Scroller::Lines, inside_the_box, content, 0.0);
    }

    /// The settings, line by line.
    fn in_the_settings(&mut self, at: Rect) -> f32 {
        let width = at.right - at.left;
        let mut y = at.top;
        y += self.dialogue_header(
            at,
            "Réglages",
            "Ils valent pour les prochaines sessions, pas pour celle en cours.",
        );
        y += self.px(design::SPACE_4);

        let mut hide = false;
        for element in SETTINGS {
            match element {
                Element::Section(title, text) => {
                    y += self.px(design::SPACE_4);
                    let tag = self.line_height(self.caption().in_bold());
                    self.section(Rect::at(at.left, y, width, tag), title);
                    y += tag + self.px(design::SPACE_1);
                    y += self.block(
                        at.left,
                        y,
                        width,
                        text,
                        self.caption(),
                        self.colours.text_soft,
                    );
                    y += self.px(design::SPACE_2);
                }
                Element::Fold => {
                    self.separator(at.left, y, width);
                    let height = self.px(layout::BUTTON);
                    let tag = self.line_height(self.caption().in_bold());
                    self.section(
                        Rect::at(at.left, y + (height - tag) / 2.0, width, tag),
                        "Avancé",
                    );
                    let sign = self.px(layout::GLYPH);
                    self.icon(
                        if self.state.advanced {
                            &icons::CHEVRON_DOWN
                        } else {
                            &icons::CHEVRON
                        },
                        Rect::at(
                            at.left + width - sign,
                            y + (height - sign) / 2.0,
                            sign,
                            sign,
                        ),
                        self.colours.text_faint,
                    );
                    self.answers(Target::Advanced, Rect::at(at.left, y, width, height));
                    y += height;
                    hide = !self.state.advanced;
                }
                Element::Setting(setting) => {
                    if hide {
                        continue;
                    }
                    self.separator(at.left, y, width);
                    y += self.setting_line(at.left, y, width, setting);
                }
                Element::Account => {
                    self.separator(at.left, y, width);
                    y += self.account(at.left, y, width);
                }
            }
        }

        if let Some(trouble) = self.state.trouble.clone() {
            y += self.px(design::SPACE_4);
            y += self.banner(at.left, y, width, &trouble, true, None);
        }
        y - at.top
    }

    /// The account: the link as it stands, what it takes to make
    /// one or undo it, and the devices on it.
    fn account(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let inside = self.px(design::SPACE_3);
        let button = self.px(layout::BUTTON);
        let mut taken = inside;
        let Some(account) = self.seen.account.as_ref() else {
            taken += self.block(
                x,
                y + taken,
                width,
                "Le service ne répond pas : le compte se lit quand il tourne.",
                self.caption(),
                self.colours.text_soft,
            );
            return taken + inside;
        };
        let Some(link) = account.link.as_ref() else {
            let text = self.line_height(self.body());
            let button_width = self.button_width("Se connecter à un serveur", false);
            self.draw_text(
                "Aucun compte",
                self.body(),
                self.colours.text,
                Rect::at(
                    x,
                    y + taken + (button - text) / 2.0,
                    (width - button_width - self.px(design::SPACE_4)).max(0.0),
                    text,
                ),
            );
            self.button(
                Rect::at(x + width - button_width, y + taken, button_width, button),
                "Se connecter à un serveur",
                Kind::Primary,
                Target::OpenAccount,
                true,
            );
            return taken + button + inside;
        };

        // The link: who, where, and if the server answers.
        let (text, caption) = (
            self.line_height(self.body()),
            self.line_height(self.caption()),
        );
        let gap = self.px(design::SPACE_1);
        let armed = self
            .state
            .detaching
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        let detach_label = if armed { "Confirmer" } else { "Se détacher" };
        let button_width = self.button_width(detach_label, false);
        let text_width = (width - button_width - self.px(design::SPACE_4)).max(0.0);
        let top = y + taken;
        self.draw_text(
            &format!(
                "{} sur {}",
                link.username,
                if link.name.is_empty() {
                    &link.server
                } else {
                    &link.name
                }
            ),
            self.body().ellipsized(),
            self.colours.text,
            Rect::at(x, top, text_width, text),
        );
        let (link_state, ink) = if link.connected {
            ("relié".to_string(), self.colours.online)
        } else {
            (
                format!(
                    "injoignable{}",
                    link.trouble
                        .as_ref()
                        .map_or_else(String::new, |why| format!(" : {}", why.replace('\n', " ")))
                ),
                self.colours.warning,
            )
        };
        let dot = self.px(layout::DOT);
        let helper = top + text + gap;
        self.dot(
            Rect::at(x, helper + (caption - dot) / 2.0, dot, dot),
            ink,
            link.connected,
        );
        self.draw_text(
            &format!("{} · {link_state}", link.server),
            self.caption().ellipsized(),
            self.colours.text_soft,
            Rect::at(
                x + dot + self.px(design::SPACE_2),
                helper,
                (text_width - dot - self.px(design::SPACE_2)).max(0.0),
                caption,
            ),
        );
        let height = (text + gap + caption).max(button);
        self.button(
            Rect::at(
                x + width - button_width,
                top + (height - button) / 2.0,
                button_width,
                button,
            ),
            detach_label,
            if armed { Kind::Warning } else { Kind::Quiet },
            Target::Detach,
            true,
        );
        taken += height + self.px(design::SPACE_4);

        // The devices, this computer included.
        let tag = self.line_height(self.caption().in_bold());
        self.section(Rect::at(x, y + taken, width, tag), "Appareils du compte");
        taken += tag + self.px(design::SPACE_2);
        if account.devices.is_empty() {
            taken += self.block(
                x,
                y + taken,
                width,
                if link.connected {
                    "Aucun appareil pour l'instant."
                } else {
                    "La liste arrivera quand le serveur répondra."
                },
                self.caption(),
                self.colours.text_soft,
            );
        }
        for rank in 0..account.devices.len() {
            taken += self.device_line(x, y + taken, width, rank);
        }
        taken + inside
    }

    /// A device of the account: its name, where it stands, and what it
    /// takes to rename or revoke it.
    ///
    /// This computer is not revoked from here: "Se détacher", just above,
    /// does exactly that and says it with the right word.
    fn device_line(&mut self, x: f32, y: f32, width: f32, rank: usize) -> f32 {
        let Some(device) = self
            .seen
            .account
            .as_ref()
            .and_then(|account| account.devices.get(rank))
        else {
            return 0.0;
        };
        let (text, caption) = (
            self.line_height(self.body()),
            self.line_height(self.caption()),
        );
        let gap = self.px(design::SPACE_1);
        let button = self.px(layout::BUTTON);
        let armed = self
            .state
            .revocation
            .is_some_and(|(which, since)| which == rank && since.elapsed() < CONFIRM_TIME);
        let revoke_label = if armed { "Confirmer" } else { "Révoquer" };
        let rename_width = self.button_width("Renommer", false);
        let revoke_width = if device.this {
            0.0
        } else {
            self.button_width(revoke_label, false) + self.px(design::SPACE_2)
        };
        let text_width = (width - rename_width - revoke_width - self.px(design::SPACE_4)).max(0.0);
        let height = (text + gap + caption).max(button) + self.px(design::SPACE_2) * 2.0;
        let middle = y + height / 2.0;
        let top = middle - (text + gap + caption) / 2.0;

        let dot = self.px(layout::DOT);
        let ready = device.online && device.access == Access::Ready;
        self.dot(
            Rect::at(x, top + (text - dot) / 2.0, dot, dot),
            if ready {
                self.colours.online
            } else if device.online {
                self.colours.warning
            } else {
                self.colours.offline
            },
            ready,
        );
        let since = x + dot + self.px(design::SPACE_2);
        self.draw_text(
            &format!(
                "{}{}",
                device.name,
                if device.this {
                    " · cet ordinateur"
                } else {
                    ""
                }
            ),
            self.body().ellipsized(),
            self.colours.text,
            Rect::at(
                since,
                top,
                (text_width - dot - self.px(design::SPACE_2)).max(0.0),
                text,
            ),
        );
        self.draw_text(
            &words_of_the_presence(device),
            self.caption().ellipsized(),
            self.colours.text_soft,
            Rect::at(
                since,
                top + text + gap,
                (text_width - dot - self.px(design::SPACE_2)).max(0.0),
                caption,
            ),
        );

        let this = device.this;
        self.button(
            Rect::at(
                x + width - rename_width,
                middle - button / 2.0,
                rename_width,
                button,
            ),
            "Renommer",
            Kind::Quiet,
            Target::OpenRenaming(rank),
            true,
        );
        if !this {
            let button_width = revoke_width - self.px(design::SPACE_2);
            self.button(
                Rect::at(
                    x + width - rename_width - revoke_width,
                    middle - button / 2.0,
                    button_width,
                    button,
                ),
                revoke_label,
                if armed { Kind::Warning } else { Kind::Quiet },
                Target::Revoke(rank),
                true,
            );
        }
        height
    }

    /// Attaching to a server: the server, the account, and this
    /// computer's name; then, when nobody vouches for the server, its
    /// key to compare.
    fn in_the_account(&mut self, at: Rect) -> f32 {
        let width = at.right - at.left;
        let mut y = at.top;
        let gap = self.px(design::SPACE_4);

        let title = self.line_height(self.subtitle());
        self.draw_text(
            "Se connecter à un serveur ZyrDesk",
            self.subtitle(),
            self.colours.text,
            Rect::at(at.left, y, width, title),
        );
        y += title + self.px(design::SPACE_2);
        y += self.block(
            at.left,
            y,
            width,
            "Indiquez le serveur de votre installation, puis votre compte. Cet ordinateur y sera \
             rattaché sous son nom, et vos autres ordinateurs apparaîtront sur l'accueil.",
            self.caption(),
            self.colours.text_soft,
        );
        y += gap;

        let height = self.px(layout::SEGMENT) + self.px(layout::AROUND) * 2.0;
        self.segments(
            at.left + self.segments_width(Pick::SignUp),
            y + height / 2.0,
            Pick::SignUp,
        );
        y += height + gap;

        let sign_up = self.state.sign_up;
        for field in Field::ACCOUNT {
            if field.for_sign_up() && !sign_up {
                continue;
            }
            y += self.field(at.left, y, width, field);
            y += gap;
        }

        if let Some(fingerprint) = self.state.pinning.clone() {
            y += self.pinning_box(at.left, y, width, &fingerprint);
            y += gap;
        }

        let filled = [Field::Server, Field::User, Field::Password]
            .iter()
            .all(|field| !text_of_the_field(*field).trim().is_empty());
        let in_progress = self.state.attaching;
        let (text, target) = if self.state.pinning.is_some() {
            ("C'est bien lui, continuer", Target::Pin)
        } else if sign_up {
            ("Créer le compte", Target::Attach)
        } else {
            ("Se connecter", Target::Attach)
        };
        y += self.px(design::SPACE_1);
        y += self.actions(
            at,
            y,
            &[
                ("Annuler".to_string(), Kind::Quiet, Target::Close, true),
                (
                    if in_progress { "Connexion…" } else { text }.to_string(),
                    Kind::Primary,
                    target,
                    filled && !in_progress,
                ),
            ],
        );
        if let Some(trouble) = self.state.trouble.clone() {
            y += self.px(design::SPACE_4);
            y += self.banner(at.left, y, width, &trouble, true, None);
        }
        y - at.top
    }

    /// The key of a server nobody vouches for, to compare with what its
    /// installation displayed before believing it.
    fn pinning_box(&mut self, x: f32, y: f32, width: f32, fingerprint: &str) -> f32 {
        let inside = self.px(design::SPACE_4);
        let text_width = width - inside * 2.0;
        let text = "Ce serveur présente un certificat que personne ne garantit. Comparez cette \
                   empreinte avec celle que son installation a affichée. Si c'est bien la même, \
                   continuez : elle sera retenue, et un serveur qui en présenterait une autre \
                   serait refusé.";
        let fingerprint_pen = self.caption().monospaced().spaced(0.02);
        let text_height = self.height_of(text, self.caption(), text_width);
        let key_height = self.height_of(fingerprint, fingerprint_pen, text_width);
        let gap = self.px(design::SPACE_2);
        let at = Rect::at(
            x,
            y,
            width,
            text_height + gap + key_height + self.px(design::SPACE_3) * 2.0,
        );
        let radius = self.px(design::RADIUS);
        self.fill(
            at,
            radius,
            self.colours
                .warning
                .mixed_with(self.colours.surface_2, 0.08),
        );
        self.stroke(
            at,
            radius,
            self.colours.warning.mixed_with(self.colours.border, 0.4),
        );
        let top = at.top + self.px(design::SPACE_3);
        self.block(
            at.left + inside,
            top,
            text_width,
            text,
            self.caption(),
            self.colours.text_soft,
        );
        self.draw_text(
            fingerprint,
            fingerprint_pen,
            self.colours.text,
            Rect::at(
                at.left + inside,
                top + text_height + gap,
                text_width,
                key_height,
            ),
        );
        at.bottom - at.top
    }

    /// Renaming a device of the account.
    fn in_the_renaming(&mut self, at: Rect) -> f32 {
        let Some((_, name)) = self.state.renaming.clone() else {
            return 0.0;
        };
        let width = at.right - at.left;
        let mut y = at.top;
        let gap = self.px(design::SPACE_4);

        let title = self.line_height(self.subtitle());
        self.draw_text(
            &format!("Renommer « {name} »"),
            self.subtitle().ellipsized(),
            self.colours.text,
            Rect::at(at.left, y, width, title),
        );
        y += title + self.px(design::SPACE_2);
        y += self.block(
            at.left,
            y,
            width,
            "Le nom que le compte montre de cet appareil, sur tous les autres.",
            self.caption(),
            self.colours.text_soft,
        );
        y += gap;
        y += self.field(at.left, y, width, Field::NewName);
        y += gap + self.px(design::SPACE_1);

        let new_name = text_of_the_field(Field::NewName).trim().to_string();
        y += self.actions(
            at,
            y,
            &[
                ("Annuler".to_string(), Kind::Quiet, Target::Close, true),
                (
                    "Renommer".to_string(),
                    Kind::Primary,
                    Target::Rename,
                    !new_name.is_empty() && new_name != name,
                ),
            ],
        );
        y - at.top
    }

    /// A setting line: what it is about on the left, what to decide
    /// it with on the right.
    fn setting_line(&mut self, x: f32, y: f32, width: f32, setting: &Setting) -> f32 {
        let inside = self.px(design::SPACE_3);
        let control = self.control_width(&setting.control);
        let text_width = (width
            - control
            - if control > 0.0 {
                self.px(design::SPACE_4)
            } else {
                0.0
            })
        .max(self.px(80.0));
        let text = self.line_height(self.body());
        let caption = self.caption_of_the_setting(setting);
        let helper = self.height_of(&caption, self.caption(), text_width);
        let gap = if helper > 0.0 {
            self.px(design::SPACE_1)
        } else {
            0.0
        };
        let height =
            (text + gap + helper).max(self.control_height(&setting.control)) + inside * 2.0;
        let middle = y + height / 2.0;
        let top = middle - (text + gap + helper) / 2.0;

        self.draw_text(
            setting.label,
            self.body(),
            self.colours.text,
            Rect::at(x, top, text_width, text),
        );
        let mono = matches!(setting.control, Control::Opens(_, Target::OpenTheJournals));
        self.block(
            x,
            top + text + gap,
            text_width,
            &caption,
            if mono {
                self.caption().monospaced()
            } else {
                self.caption()
            },
            if mono {
                self.colours.text_faint
            } else {
                self.colours.text_soft
            },
        );

        let right = x + width;
        match &setting.control {
            Control::Status => {}
            Control::Switch(button) => {
                self.switch(right - self.px(layout::SWITCH.0), middle, *button);
            }
            Control::Segments(target) => {
                self.segments(right, middle, *target);
            }
            Control::Key(doing) => {
                let height = self.px(layout::BUTTON);
                self.key(
                    Rect::at(right - control, middle - height / 2.0, control, height),
                    *doing,
                );
            }
            Control::Opens(text, target) => {
                let height = self.px(layout::BUTTON);
                self.button(
                    Rect::at(right - control, middle - height / 2.0, control, height),
                    text,
                    Kind::Quiet,
                    target.clone(),
                    true,
                );
            }
        }
        height
    }

    /// How wide a line's control is.
    fn control_width(&self, control: &Control) -> f32 {
        match control {
            Control::Status => 0.0,
            Control::Switch(_) => self.px(layout::SWITCH.0),
            Control::Segments(target) => self.segments_width(*target),
            Control::Key(doing) => {
                self.canvas
                    .width_of(&self.words_of_the_key(*doing), self.caption().monospaced())
                    .max(self.px(layout::KEY) - self.px(design::SPACE_3) * 2.0)
                    + self.px(design::SPACE_3) * 2.0
            }
            Control::Opens(text, _) => self.button_width(text, false),
        }
    }

    fn control_height(&self, control: &Control) -> f32 {
        match control {
            Control::Status => 0.0,
            Control::Switch(_) => self.px(layout::SWITCH.1),
            Control::Segments(_) => self.px(layout::SEGMENT) + self.px(layout::AROUND) * 2.0,
            Control::Key(_) | Control::Opens(_, _) => self.px(layout::BUTTON),
        }
    }

    /// What a setting line has to say under its word.
    fn caption_of_the_setting(&self, setting: &Setting) -> String {
        match &setting.control {
            // What a session would ask for now, as the product says it and
            // not worked out again here.
            Control::Status => self
                .seen
                .settings
                .as_ref()
                .map_or_else(String::new, |said| {
                    format!(
                        "{} x {}, {} images par seconde, {} Mb/s",
                        said.width,
                        said.height,
                        said.fps,
                        (said.bitrate_kbps as f32 / 1000.0).round() as u32
                    )
                }),
            Control::Opens(_, Target::OpenTheJournals) => self.seen.folder.clone(),
            _ => setting.caption.to_string(),
        }
    }

    /// A shortcut's combination, as it reads.
    fn words_of_the_key(&self, doing: Doing) -> String {
        if self.state.listening == Some(doing) {
            return "Tapez la combinaison…".to_string();
        }
        self.seen
            .shortcuts
            .iter()
            .find(|(other, _)| *other == doing)
            .and_then(|(_, said)| said.clone())
            .unwrap_or_else(|| "Aucune".to_string())
    }

    /// A combination reads as keys and not as a sentence: the fixed-width
    /// type puts the same space under each character, and the frame says
    /// it can be clicked to change it.
    fn key(&mut self, at: Rect, doing: Doing) {
        let listening = self.state.listening == Some(doing);
        let target = Target::Shortcut(doing);
        let hovered = self.under_the_hand(&target);
        let radius = self.px(design::RADIUS_SMALL);
        self.fill(at, radius, self.colours.surface_2);
        self.stroke(
            at,
            radius,
            if listening || hovered {
                self.colours.accent
            } else {
                self.colours.border_strong
            },
        );
        let text = self.words_of_the_key(doing);
        let empty = text == "Aucune";
        self.draw_text(
            &text,
            self.caption().monospaced().aligned(Align::Centre),
            if listening {
                self.colours.accent_bright
            } else if empty {
                self.colours.text_faint
            } else {
                self.colours.text
            },
            at,
        );
        self.answers(target, at);
    }

    /// What is on screen while a session opens.
    ///
    /// It takes the whole window because it is the only thing going on,
    /// and because it is the last thing one reads before the engine lays
    /// its own picture on top: between the two there must never be a gap
    /// in which one wonders whether it is working.
    fn opening(&mut self, width: f32, height: f32) {
        let Some(opening) = self.state.opening.as_ref() else {
            return;
        };
        let whole = Rect::at(0.0, 0.0, width, height);
        self.fill(whole, 0.0, self.colours.background);

        let brand = self.px(layout::BIG_BRAND);
        let title = self.line_height(self.pen(design::TITLE).in_bold());
        let words_width = (width - self.px(design::SPACE_6) * 2.0).min(self.px(420.0));
        let towards = self.height_of(&opening.towards, self.body(), words_width);
        let (wire_width, wire_height) = (
            self.px(layout::THREAD.0).min(width * 0.6),
            self.px(layout::THREAD.1),
        );
        let detail = self
            .height_of(&opening.detail, self.caption(), words_width)
            .max(self.line_height(self.caption()));
        let gap = self.px(design::SPACE_4);
        let content = brand + gap + title + gap + towards + gap + wire_height + gap + detail;

        let middle = width / 2.0;
        let mut y = (height - content) / 2.0;
        self.brand(Rect::at(middle - brand / 2.0, y, brand, brand));
        y += brand + gap;
        self.draw_text(
            "Établissement de la connexion",
            self.pen(design::TITLE).in_bold().aligned(Align::Centre),
            self.colours.text,
            Rect::at(0.0, y, width, title),
        );
        y += title + gap;
        self.draw_text(
            &opening.towards,
            self.body().aligned(Align::Centre),
            self.colours.text_soft,
            Rect::at(middle - words_width / 2.0, y, words_width, towards),
        );
        y += towards + gap;

        // A bar that goes back and forth. It measures nothing: what is
        // being waited for here does not cut into percentages, and a bar
        // claiming otherwise would be lying.
        let track = Rect::at(middle - wire_width / 2.0, y, wire_width, wire_height);
        self.fill(track, wire_height / 2.0, self.colours.surface_3);
        let part = opening.since.elapsed().as_secs_f32() / BACK_AND_FORTH.as_secs_f32();
        let position = part.fract() * (1.0 + layout::PIECE * 2.0) - layout::PIECE;
        let piece = wire_width * layout::PIECE;
        let canvas = self.canvas;
        let accent = self.colours.accent;
        let silent = self.silent;
        canvas.clipped(track, || {
            if !silent {
                canvas.fill(
                    Rect::at(track.left + position * wire_width, y, piece, wire_height),
                    wire_height / 2.0,
                    accent,
                );
            }
        });
        y += wire_height + gap;

        self.draw_text(
            &opening.detail,
            self.caption().aligned(Align::Centre),
            self.colours.text_faint,
            Rect::at(middle - words_width / 2.0, y, words_width, detail),
        );
    }
}

/// Far enough that a journal line is never cut off by its own rect: it
/// is the box that holds it back, and the box scrolls.
const FAR_AWAY: f32 = 20_000.0;

/// Lower than any journal, which the drawing then brings back to the
/// real bottom: asking for "the very bottom" before having measured is
/// the only way to be there from the first frame.
const VERY_BOTTOM: f32 = 1.0e9;

/* ---- What draws, and what stays silent --------------------------------- */

/// The same gestures as the canvas, but doing nothing when the walk only
/// measures.
///
/// Measuring and drawing are the same walk: the height a dialogue takes
/// is what its lines take, and writing it a second time alongside would
/// be arithmetic that adds up only until the first word made longer.
impl Painter<'_> {
    fn draw_text(&self, text: &str, pen: Pen, ink: Colour, at: Rect) {
        if !self.silent {
            self.canvas.draw_text(text, pen, ink, at);
        }
    }

    fn fill(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas.fill(at, radius, ink);
        }
    }

    /// A border, which fits entirely within its rect.
    fn stroke(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas
                .stroke_inside(at, radius, self.px(layout::HAIRLINE), ink);
        }
    }

    /// An outline waiting to be filled.
    fn dashed(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas.stroke_dashed(
                at.grown(-self.px(layout::HAIRLINE) / 2.0),
                radius,
                self.px(layout::HAIRLINE),
                ink,
            );
        }
    }

    fn shadow(&self, at: Rect, radius: f32, shadow: design::Shadow) {
        if !self.silent {
            self.canvas.shadow(at, radius, shadow, self.scale);
        }
    }

    fn icon(&self, icon: &Icon, at: Rect, ink: Colour) {
        if !self.silent {
            self.canvas.icon(icon, at, ink);
        }
    }

    fn brand(&self, at: Rect) {
        if !self.silent {
            crate::logo::brand(self.canvas, at, 1.0, false);
        }
    }
}

/* ---- The mouse ---------------------------------------------------------- */

/// What is under this point, the last laid down winning: what was
/// drawn last is what is on top.
fn under(x: f32, y: f32) -> Option<Target> {
    CLICKABLES
        .lock()
        .expect("accueil")
        .iter()
        .rev()
        .find(|(_, at)| x >= at.left && x < at.right && y >= at.top && y < at.bottom)
        .map(|(target, _)| target.clone())
}

fn moves(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };

    // Asked for on every pass: without it nothing ever says that a hand
    // has left, and the last line hovered would stay hovered.
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: 0,
    };
    // SAFETY: a window of ours, and the structure it asks for.
    unsafe { TrackMouseEvent(&mut tracking) };

    let mut state = STATE.lock().expect("accueil");
    if let Some((which, since)) = state.held {
        let (content, visible, travel) = state.measured(which);
        if travel > 0.0 {
            let by = (y - since) * (content - visible) / travel;
            state.scroll_by(which, by);
        }
        state.held = Some((which, y));
        drop(state);
        invalidate(window);
        return;
    }
    let hit = under(x, y);
    if state.hover != hit {
        state.hover = hit;
        drop(state);
        invalidate(window);
    }
}

fn mouse_left(window: windows_sys::Win32::Foundation::HWND) {
    let mut state = STATE.lock().expect("accueil");
    if state.hover.is_none() {
        return;
    }
    state.hover = None;
    drop(state);
    invalidate(window);
}

fn presses(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    let mut state = STATE.lock().expect("accueil");
    match under(x, y) {
        Some(Target::Scrollbar(which)) => state.held = Some((which, y)),
        hit => state.pressed = hit,
    }
    drop(state);
    invalidate(window);
}

fn releases(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    let mut state = STATE.lock().expect("accueil");
    state.held = None;
    let pressed = state.pressed.take();
    drop(state);
    invalidate(window);

    let Some(target) = pressed else {
        return;
    };
    if under(x, y).as_ref() != Some(&target) {
        return;
    }
    if let Some(app) = program() {
        act(&app, target);
    }
}

fn wheel(window: windows_sys::Win32::Foundation::HWND, notches: f32, across: bool) {
    let mut state = STATE.lock().expect("accueil");
    let which = match state.screen {
        Screen::Home => Scroller::Page,
        // The journal's text scrolls on its own: it is what one reads
        // in this dialogue, and the dialogue itself is made to fit in
        // the window. Except when it does not fit all the same, on a
        // very short screen: the wheel then serves first to reach what
        // sticks out, or else the buttons at the bottom cannot be
        // reached.
        Screen::Journal => {
            let (content, visible, _) = state.measured(Scroller::Dialogue);
            if content > visible {
                Scroller::Dialogue
            } else {
                Scroller::Lines
            }
        }
        _ => Scroller::Dialogue,
    };
    let by = -notches * scale() * layout::NOTCH;
    if across && which == Scroller::Lines {
        // Sideways: a journal line does not wrap, and reading all of it
        // means moving along it.
        state.lines_scroll.0 = (state.lines_scroll.0 + by).max(0.0);
    } else {
        state.scroll_by(which, by);
    }
    drop(state);
    invalidate(window);
}

/// Asks for a new frame, from the drawing thread we are already on.
fn invalidate(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

    // SAFETY: a window of ours, on the thread that owns it.
    unsafe { InvalidateRect(window, std::ptr::null(), 0) };
}

/* ---- The keyboard ------------------------------------------------------- */

/// What the canvas does with a key, and whether it took it.
fn key_down(
    window: windows_sys::Win32::Foundation::HWND,
    vk: u32,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_DELETE, VK_ESCAPE, VK_RETURN,
    };

    let listening = STATE.lock().expect("accueil").listening;
    if let Some(doing) = listening {
        return the_combination(window, doing, vk, with);
    }

    let Some(app) = program() else {
        return false;
    };
    let screen = STATE.lock().expect("accueil").screen;
    match vk as u16 {
        VK_ESCAPE if screen != Screen::Home => {
            act(&app, Target::Close);
            true
        }
        VK_RETURN if matches!(screen, Screen::Adding | Screen::Account | Screen::Renaming) => {
            act(&app, Target::Confirm);
            true
        }
        VK_BACK | VK_DELETE => false,
        _ => false,
    }
}

/// What a key is worth when a shortcut is waiting for it.
///
/// The key's place and not the character on it: that is what the product
/// keeps, and it is what keeps a shortcut under the same finger from one
/// keyboard to another.
fn the_combination(
    window: windows_sys::Win32::Foundation::HWND,
    doing: Doing,
    vk: u32,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    let Some(app) = program() else {
        return false;
    };
    match vk as u16 {
        VK_ESCAPE => {
            STATE.lock().expect("accueil").listening = None;
            invalidate(window);
            return true;
        }
        VK_BACK | VK_DELETE => {
            set_the_combination(&app, doing, None);
            return true;
        }
        _ => {}
    }

    // The extended key is a different key from the one in the same place
    // in the main block: refused rather than mistaken for it.
    if with & (1 << 24) != 0 {
        return true;
    }
    let scan = ((with >> 16) & 0xFF) as u16;
    let Some(place) = crate::shortcuts::placed(scan) else {
        return true;
    };
    // SAFETY: four questions to the system about this thread's
    // keyboard.
    let modifiers = unsafe {
        Held {
            ctrl: GetKeyState(i32::from(VK_CONTROL)) < 0,
            alt: GetKeyState(i32::from(VK_MENU)) < 0,
            shift: GetKeyState(i32::from(VK_SHIFT)) < 0,
            win: GetKeyState(i32::from(VK_LWIN)) < 0 || GetKeyState(i32::from(VK_RWIN)) < 0,
        }
    };
    set_the_combination(
        &app,
        doing,
        Some(Combination {
            held: modifiers,
            key: place.to_string(),
        }),
    );
    true
}

/// Writes or removes a combination, and rereads the three.
fn set_the_combination(app: &App, doing: Doing, combination: Option<Combination>) {
    let mut state = STATE.lock().expect("accueil");
    state.listening = None;
    state.trouble = None;
    if let Err(refusal) = crate::shortcuts::bind(doing, combination) {
        state.trouble = Some(refusal);
    }
    drop(state);
    if let Some(seen) = SEEN.lock().expect("accueil").as_mut() {
        seen.shortcuts = crate::shortcuts::engraved();
    }
    redraw(app);
}

/* ---- The input fields ---------------------------------------------------- */

/// An input field, each in its own place, open for as long as the
/// dialogue that carries it.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Fingerprint,
    Address,
    Name,
    Server,
    User,
    Password,
    DeviceName,
    Email,
    Invitation,
    NewName,
    Sift,
}

impl Field {
    /// How many there are in all: each has its place, open or not.
    const COUNT: usize = 11;
    /// Those of the adding dialogue, in the order they are
    /// filled in.
    const ADD: [Field; 3] = [Field::Fingerprint, Field::Address, Field::Name];
    /// Those of the account dialogue, the last two for signing up.
    const ACCOUNT: [Field; 6] = [
        Field::Server,
        Field::User,
        Field::Password,
        Field::DeviceName,
        Field::Email,
        Field::Invitation,
    ];
    /// The one for renaming a device.
    const RENAMING: [Field; 1] = [Field::NewName];
    /// The one that sifts the journal.
    const JOURNAL: [Field; 1] = [Field::Sift];

    fn rank(self) -> usize {
        match self {
            Field::Fingerprint => 0,
            Field::Address => 1,
            Field::Name => 2,
            Field::Server => 3,
            Field::User => 4,
            Field::Password => 5,
            Field::DeviceName => 6,
            Field::Email => 7,
            Field::Invitation => 8,
            Field::NewName => 9,
            Field::Sift => 10,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Field::Fingerprint => "Empreinte",
            Field::Address => "Adresse",
            Field::Name => "Nom (facultatif)",
            Field::Server => "Serveur",
            Field::User => "Nom d'utilisateur",
            Field::Password => "Mot de passe",
            Field::DeviceName => "Nom de cet ordinateur",
            Field::Email => "Adresse e-mail (facultatif)",
            Field::Invitation => "Code d'invitation (si le serveur en demande un)",
            Field::NewName => "Nouveau nom",
            Field::Sift => "Tri",
        }
    }

    /// The watermark word, showing what the expected text looks like.
    fn example(self) -> &'static str {
        match self {
            Field::Fingerprint => "0829cc7ecb9e9ba5…",
            Field::Address => "192.168.1.20",
            Field::Name => "PC du bureau",
            Field::Server => "zyr.exemple.fr ou 192.168.1.40:8443",
            Field::User => "victor",
            Field::Password => "",
            Field::DeviceName => "PC du bureau",
            Field::Email => "victor@exemple.fr",
            Field::Invitation => "AB12-CD34",
            Field::NewName => "PC du salon",
            Field::Sift => "clipboard files",
        }
    }

    /// What is typed in it cannot be read over a shoulder.
    fn secret(self) -> bool {
        self == Field::Password
    }

    /// Those that only show when creating an account.
    fn for_sign_up(self) -> bool {
        matches!(self, Field::Email | Field::Invitation)
    }

    /// What the field has to complain about, or to explain,
    /// below it.
    fn hint(self) -> String {
        match self {
            Field::Fingerprint => {
                let how_many = text_of_the_field(self).trim().chars().count();
                if how_many == 0 || how_many == FINGERPRINT_LENGTH {
                    String::new()
                } else {
                    format!("{how_many} caractères sur {FINGERPRINT_LENGTH}")
                }
            }
            Field::Address => "Seulement si vous voulez contrôler cet ordinateur depuis ici. Il \
                               restera alors sur l'accueil, et il n'y aura plus rien à ressaisir."
                .to_string(),
            Field::Server => "Comme l'installation du serveur l'a affiché, avec le port s'il \
                               n'est pas 443. Toujours chiffré : une adresse en http:// est \
                               refusée."
                .to_string(),
            Field::Sift => "Un ou plusieurs noms ci-dessus, séparés par des espaces : la page \
                           garde l'un ou l'autre. « mot entre guillemets » cherche dans le \
                           texte, et un moins devant écarte."
                .to_string(),
            _ => String::new(),
        }
    }
}

/// The fields, and the place the last frame gave them.
static FIELDS: Mutex<[isize; Field::COUNT]> = Mutex::new([0; Field::COUNT]);
static PLACES: Mutex<[Option<Rect>; Field::COUNT]> = Mutex::new([None; Field::COUNT]);
/// The fields' font, made once for the size of the screen.
static FONT: Mutex<isize> = Mutex::new(0);

/// Remakes the fields' font at the screen's scale, and sets it on them.
///
/// A field is a system window: it carries its own font, which does not
/// follow what we draw.
fn dress_the_fields() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, FF_DONTCARE,
        FW_NORMAL, OUT_DEFAULT_PRECIS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETFONT};

    let family = wide(FIELD_FAMILY);
    // SAFETY: the name outlives the call, and the font that comes
    // back is ours until we give it back.
    let font = unsafe {
        CreateFontW(
            -((design::BODY * scale()).round() as i32),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            0,
            u32::from(CLEARTYPE_QUALITY),
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            family.as_ptr(),
        )
    };
    if font.is_null() {
        return;
    }
    let mut before = FONT.lock().expect("accueil");
    for edit in FIELDS.lock().expect("accueil").iter() {
        if *edit != 0 {
            // SAFETY: a window made by us, given a font that will
            // outlive it.
            unsafe { SendMessageW(*edit as HWND, WM_SETFONT, font as usize, 1) };
        }
    }
    if *before != 0 {
        // SAFETY: the previous font, given back once nobody uses it
        // any more.
        unsafe { DeleteObject(*before as _) };
    }
    *before = font as isize;
}

/// The fields' font family: the same as that of the rest of the
/// drawing, as far as the system knows it.
const FIELD_FAMILY: &str = "Segoe UI Variable Text";

/// Opens these real Windows fields, empty.
fn open_the_fields(which_ones: &[Field]) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::EM_SETCUEBANNER;
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, ES_AUTOHSCROLL, ES_PASSWORD, SendMessageW, WS_CHILD, WS_VISIBLE,
    };

    let canvas = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if canvas.is_null() {
        return;
    }
    let class_name = wide("EDIT");
    let mut fields = FIELDS.lock().expect("accueil");
    for field in which_ones.iter().copied() {
        let secret = if field.secret() {
            ES_PASSWORD as u32
        } else {
            0
        };
        // SAFETY: a system window, a child of ours, on the thread that
        // owns both.
        let edit = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32 | secret,
                0,
                0,
                0,
                0,
                canvas,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if edit.is_null() {
            continue;
        }
        let cue = wide(field.example());
        // SAFETY: a system window, given a word that outlives the
        // call, then a guard that outlives it: it is a plain function
        // of this program.
        unsafe {
            SendMessageW(edit, EM_SETCUEBANNER, 1, cue.as_ptr() as isize);
            SetWindowSubclass(edit, Some(in_a_field), IN_A_FIELD, field.rank());
        }
        fields[field.rank()] = edit as isize;
    }
    drop(fields);
    dress_the_fields();
}

/// The name under which our guard is set on a field.
const IN_A_FIELD: usize = 3;

/// What a dialogue's keys do in a field.
///
/// A Windows field is a window of its own: Tab, Enter and Escape never
/// get through it to us, and a dialogue in which one cannot move from one
/// field to the next is not a dialogue. So they are caught here and
/// handed to whoever they belong to.
///
/// SAFETY: called by the system on the thread that owns this field, with
/// the arguments it documents.
unsafe extern "system" fn in_a_field(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
    _who: usize,
    rank: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, SetFocus, VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_TAB,
    };
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CHAR, WM_KEYDOWN};

    let vk = holding as u16;
    let ours = vk == VK_TAB || vk == VK_RETURN || vk == VK_ESCAPE;
    // The character that follows the key is swallowed with it: without
    // this the field beeps, a tab not being a character it accepts.
    if message == WM_CHAR && (holding == 9 || holding == 13 || holding == 27) {
        return 0;
    }
    if message == WM_KEYDOWN && ours {
        match vk {
            VK_TAB => {
                // SAFETY: a question to the system about this thread's
                // keyboard, then the keyboard given to a field of
                // ours.
                let backwards = unsafe { GetKeyState(i32::from(VK_SHIFT)) } < 0;
                let fields = *FIELDS.lock().expect("accueil");
                // The next of those that are open, going round: the
                // places of the other dialogues are empty.
                let how_many = fields.len();
                let mut next = rank;
                for _ in 0..how_many {
                    next = if backwards {
                        (next + how_many - 1) % how_many
                    } else {
                        (next + 1) % how_many
                    };
                    if fields[next] != 0 {
                        break;
                    }
                }
                if fields[next] != 0 {
                    // SAFETY: a window made by us, on its thread.
                    unsafe { SetFocus(fields[next] as HWND) };
                }
            }
            // Posted and not done right away: both close the dialogue,
            // and so destroy the field we are in the middle of
            // answering in.
            other => {
                let canvas = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
                if !canvas.is_null() {
                    // SAFETY: a window of ours, posted a message that
                    // belongs only to us.
                    unsafe { PostMessageW(canvas, ACT, usize::from(other == VK_RETURN), 0) };
                }
            }
        }
        return 0;
    }
    // SAFETY: the arguments the system gave, handed back as they are.
    unsafe { DefSubclassProc(window, message, holding, with) }
}

/// Closes the open fields and gives back their places.
fn close_the_fields() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::DeleteObject;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

    let mut fields = FIELDS.lock().expect("accueil");
    for edit in fields.iter_mut() {
        if *edit != 0 {
            // SAFETY: a window made by us, destroyed once.
            unsafe { DestroyWindow(*edit as HWND) };
            *edit = 0;
        }
    }
    *PLACES.lock().expect("accueil") = [None; Field::COUNT];
    let mut font = FONT.lock().expect("accueil");
    if *font != 0 {
        // SAFETY: a font made by us, given back once.
        unsafe { DeleteObject(*font as _) };
        *font = 0;
    }
}

/// Notes where the drawing wants this field. It will be set there once
/// the frame is finished: moving a window while painting one's own
/// mixes two drawings.
fn place_the_field(field: Field, at: Rect) {
    PLACES.lock().expect("accueil")[field.rank()] = Some(at);
}

/// Sets the fields where the last frame wanted them.
fn place_the_fields() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let fields = *FIELDS.lock().expect("accueil");
    let places = *PLACES.lock().expect("accueil");
    // The text breathes inside its frame as the style sheet asks:
    // the real field is set inside it, never on its outline.
    let inside = design::SPACE_3 * scale();
    for (edit, place) in fields.iter().zip(places.iter()) {
        if *edit == 0 {
            continue;
        }
        // An open field the frame did not lay down is put away: shrunk
        // to nothing rather than left where the last frame had put it.
        let (x, y, width, height) = match place {
            Some(place) => (
                (place.left + inside).round() as i32,
                (place.top + inside / 2.0).round() as i32,
                (place.right - place.left - inside * 2.0).round() as i32,
                (place.bottom - place.top - inside).round() as i32,
            ),
            None => (0, 0, 0, 0),
        };
        // SAFETY: a window made by us, moved on the thread that owns it.
        unsafe {
            SetWindowPos(
                *edit as HWND,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
    }
}

/// Writes this text into a field, in place of what it held.
///
/// For what a dialogue already knows: this machine's name, the name of a
/// device to rename. An empty field in which one had to type again what
/// the window shows next to it would be one more copy.
fn write_in_the_field(field: Field, text: &str) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    let edit = FIELDS.lock().expect("accueil")[field.rank()];
    if edit == 0 {
        return;
    }
    let words = wide(text);
    // SAFETY: a window made by us, and a text that outlives the
    // call.
    unsafe { SetWindowTextW(edit as HWND, words.as_ptr()) };
}

/// What is written in a field.
fn text_of_the_field(field: Field) -> String {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let edit = FIELDS.lock().expect("accueil")[field.rank()];
    if edit == 0 {
        return String::new();
    }
    // SAFETY: a window made by us, whose text is read into a buffer of
    // the length it has just announced.
    unsafe {
        let how_many = GetWindowTextLengthW(edit as HWND);
        if how_many <= 0 {
            return String::new();
        }
        let mut read = vec![0u16; how_many as usize + 1];
        let read_count = GetWindowTextW(edit as HWND, read.as_mut_ptr(), how_many + 1);
        String::from_utf16_lossy(&read[..read_count.max(0) as usize])
    }
}

/// What colour to paint the inside of a field.
///
/// The field belongs to the system, which draws it itself and asks here
/// which colours to use: without this, a white field would punch a hole
/// in a dark window.
fn tint_of_the_field(surface: windows_sys::Win32::Foundation::WPARAM) -> isize {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, SetBkColor, SetTextColor,
    };

    let colours = palette();
    let (background, ink) = (colours.surface_2, colours.text);
    // SAFETY: the surface the system has just lent, and a brush it
    // will give back at the same time as it gives back the one before.
    unsafe {
        SetTextColor(surface as _, rgb(ink));
        SetBkColor(surface as _, rgb(background));
        let mut brush = BRUSH.lock().expect("accueil");
        if *brush != 0 {
            DeleteObject(*brush as _);
        }
        *brush = CreateSolidBrush(rgb(background)) as isize;
        *brush
    }
}

/// The brush for the fields' background, kept so that it can be given
/// back at the next one: the system reads the one we return and does
/// not keep it.
static BRUSH: Mutex<isize> = Mutex::new(0);

/// A colour from the design system, in the number GDI expects.
fn rgb(colour: Colour) -> u32 {
    let part = |how_many: f32| (how_many.clamp(0.0, 1.0) * 255.0).round() as u32;
    part(colour.red) | (part(colour.green) << 8) | (part(colour.blue) << 16)
}

/* ---- What a click does --------------------------------------------------- */

/// Acts on what has just been clicked.
///
/// Nothing waits here: whatever asks the service goes off on its own
/// thread and redraws on its way back. The drawing thread must never
/// wait for an answer that goes through a pipe.
fn act(app: &App, target: Target) {
    match target {
        Target::OpenJournal => open_the_journal(app, None),
        Target::JournalOf(rank) => {
            let peer = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.peers.get(rank).cloned());
            if let Some(peer) = peer {
                open_the_journal(app, Some(peer));
            }
        }
        Target::OpenSettings => {
            {
                let mut state = STATE.lock().expect("accueil");
                state.screen = Screen::Settings;
                state.dialogue_scroll = 0.0;
                state.trouble = None;
            }
            reread_the_settings(app);
            redraw(app);
        }
        Target::CopyFingerprint => {
            let fingerprint = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.machine.as_ref().map(|said| said.fingerprint.clone()))
                .unwrap_or_default();
            copy(app, &fingerprint, Target::CopyFingerprint);
        }
        Target::CopyJournal => copy_the_journal(app),
        Target::Tag(rank) => toggle_the_tag(app, rank),
        Target::ToFix(rank) => remedy_it(app, rank),
        Target::Peer(rank) => launch_the_peer(app, rank, false),
        Target::Local(rank) => launch_the_peer(app, rank, true),
        Target::Disconnect(rank) => {
            let fingerprint = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.peers.get(rank).map(|peer| peer.fingerprint.clone()));
            if let Some(fingerprint) = fingerprint {
                disconnect(app, fingerprint);
            }
        }
        Target::Add => {
            {
                let mut state = STATE.lock().expect("accueil");
                state.screen = Screen::Adding;
                state.dialogue_scroll = 0.0;
            }
            // Emptied at every opening: reopened full of the previous
            // machine, they would let the same computer be added twice
            // with a mere double click.
            close_the_fields();
            open_the_fields(&Field::ADD);
            redraw(app);
        }
        Target::Close => {
            let mut state = STATE.lock().expect("accueil");
            state.screen = Screen::Home;
            state.listening = None;
            // On closing and not on its button: the Escape key closes
            // too, and used to leave the emptying confirmation armed
            // behind a closed dialogue.
            state.emptying = None;
            state.pinning = None;
            state.renaming = None;
            drop(state);
            close_the_fields();
            redraw(app);
        }
        Target::Connect => connect(app),
        // The Enter key does what the main button of the open dialogue
        // would do.
        Target::Confirm => {
            let screen = STATE.lock().expect("accueil").screen;
            match screen {
                Screen::Adding => connect(app),
                Screen::Account => {
                    let pinning = STATE.lock().expect("accueil").pinning.clone();
                    attach(app, pinning);
                }
                Screen::Renaming => rename(app),
                // Enter in the sift box reads again right away, without
                // waiting out the timer's pause.
                Screen::Journal => reread_the_journal(app, After::Show),
                Screen::Home | Screen::Settings => {}
            }
        }
        Target::OpenAccount => open_the_account(app),
        Target::Attach => attach(app, None),
        Target::Pin => {
            let pinning = STATE.lock().expect("accueil").pinning.clone();
            attach(app, pinning);
        }
        Target::Detach => detach(app),
        Target::OpenRenaming(rank) => open_the_renaming(app, rank),
        Target::Rename => rename(app),
        Target::Revoke(rank) => revoke(app, rank),
        Target::Forget(rank) => {
            let peer = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.peers.get(rank).cloned());
            if let Some(peer) = peer {
                forget(app, peer.fingerprint);
            }
        }
        Target::Switch(button) => push(app, button),
        Target::Segment(target, rank) => pick(app, target, rank),
        Target::Shortcut(doing) => {
            let mut state = STATE.lock().expect("accueil");
            state.listening = if state.listening == Some(doing) {
                None
            } else {
                Some(doing)
            };
            drop(state);
            redraw(app);
        }
        Target::Advanced => {
            let mut state = STATE.lock().expect("accueil");
            state.advanced = !state.advanced;
            drop(state);
            redraw(app);
        }
        Target::Empty => empty_the_journal(app),
        Target::Refresh => reread_the_journal(app, After::Show),
        Target::OpenTheJournals => open_a_folder(app, "logs"),
        Target::Scrollbar(_) => {}
    }
}

/// What the button of a "to do" banner repairs.
fn remedy_it(app: &App, rank: usize) {
    let missing = SEEN
        .lock()
        .expect("accueil")
        .as_ref()
        .map(what_is_missing)
        .and_then(|missings| missings.get(rank).map(|missing| missing.remedy));
    match missing {
        Some(Remedy::StartTheService) => {
            let app = app.clone();
            crate::app::spawn(async move {
                if let Err(reason) = crate::desk::start_service().await {
                    notice(&app, &reason, true);
                }
                reread(&app).await;
                redraw(&app);
            });
        }
        Some(Remedy::Ffmpeg) => open_a_folder(app, "ffmpeg"),
        None => {}
    }
}

fn open_a_folder(app: &App, which: &'static str) {
    if let Err(reason) = crate::folders::open_folder(which.to_string()) {
        notice(app, &reason, true);
    }
}

/// Pushes a switch, and holds it in its new place until the service
/// acknowledges it.
fn push(app: &App, button: Toggle) {
    let wanted = {
        let nothing = Seen::default();
        let seen = SEEN.lock().expect("accueil");
        let mut state = STATE.lock().expect("accueil");
        let wanted = !button.is_on(seen.as_ref().unwrap_or(&nothing), &state);
        state.pushed.retain(|(target, _)| *target != button);
        state.pushed.push((button, wanted));
        state.trouble = None;
        state.notice = None;
        wanted
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        let target = match button {
            Toggle::Access => crate::desk::set_hosting(wanted).await,
            Toggle::Trust => crate::desk::set_trust(wanted).await,
            Toggle::AtBoot => crate::desk::set_at_boot(wanted).await,
            Toggle::Marking => crate::desk::set_ecn(wanted).await,
            Toggle::FixedPort => crate::desk::set_fixed_port(wanted).await,
            Toggle::Sound | Toggle::Stats => {
                write_the_settings(|chosen| {
                    if button == Toggle::Sound {
                        chosen.mute_far_speakers = wanted;
                    } else {
                        chosen.stats_overlay = wanted;
                    }
                })
                .await
            }
        };
        if let Err(reason) = target {
            say_the_trouble(&app, &reason);
        }
        STATE
            .lock()
            .expect("accueil")
            .pushed
            .retain(|(target, _)| *target != button);
        reread(&app).await;
        redraw(&app);
    });
}

/// Picks one of the sides of a segmented choice.
fn pick(app: &App, target: Pick, rank: usize) {
    if target == Pick::Theme {
        if let Some(choice) = Choice::ALL.get(rank) {
            crate::theme::choose(*choice);
            redraw(app);
        }
        return;
    }
    // This one travels nowhere: it changes the shape of the account
    // dialogue, and nothing else.
    if target == Pick::SignUp {
        STATE.lock().expect("accueil").sign_up = rank == 1;
        redraw(app);
        return;
    }
    let Some(value) = target.values().get(rank).copied() else {
        return;
    };
    let app = app.clone();
    crate::app::spawn(async move {
        let done = write_the_settings(|chosen| match target {
            Pick::Codec => chosen.codec = value.parse().unwrap_or(chosen.codec),
            Pick::Display => chosen.display = value.parse().unwrap_or(chosen.display),
            Pick::Mouse => chosen.absolute_mouse = value == "desktop",
            _ => {}
        })
        .await;
        if let Err(reason) = done {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Changes one session setting, the others staying as they are.
///
/// The whole set goes to the service so that it never has to guess
/// what stayed.
async fn write_the_settings(
    change: impl FnOnce(&mut crate::settings::Chosen),
) -> Result<(), String> {
    let mut chosen = crate::settings::Chosen::of(crate::settings::preferred().await);
    change(&mut chosen);
    crate::settings::choose(chosen).await
}

/* ---- Adding, forgetting, connecting -------------------------------------- */

/// Writes a computer down and, if it has an address, connects to it.
///
/// The fingerprint works both ways: it lets that computer in, and it
/// serves as a landmark for going to it. Without the first of the two,
/// the far machine would be refused on arrival and only half the way
/// would have been done.
fn connect(app: &App) {
    let fingerprint = text_of_the_field(Field::Fingerprint).trim().to_string();
    let address = text_of_the_field(Field::Address).trim().to_string();
    let name = text_of_the_field(Field::Name).trim().to_string();
    if fingerprint.len() != FINGERPRINT_LENGTH {
        return;
    }
    act(app, Target::Close);

    let app = app.clone();
    crate::app::spawn(async move {
        let written = crate::desk::authorize(
            fingerprint.clone(),
            (!address.is_empty()).then(|| address.clone()),
            (!name.is_empty()).then(|| name.clone()),
        )
        .await;
        if let Err(reason) = written {
            notice(&app, &reason, true);
            return;
        }
        reread(&app).await;
        if address.is_empty() {
            // Authorising shows nowhere else: without a word, the gesture
            // would look on screen exactly like doing nothing.
            notice(
                &app,
                "Cet ordinateur est autorisé à venir sur celui-ci.",
                false,
            );
            return;
        }
        redraw(&app);
        let seen_name = SEEN
            .lock()
            .expect("accueil")
            .as_ref()
            .and_then(|seen| {
                seen.peers
                    .iter()
                    .find(|peer| peer.fingerprint == fingerprint)
                    .map(|peer| peer.name.clone())
            })
            .unwrap_or_else(|| address.clone());
        // A computer just written down by hand is reached by the best
        // way: going without the server is asked for on a card, for a
        // machine this network already announces.
        launch(&app, &address, &fingerprint, &seen_name, false);
    });
}

/// Forgets a computer written by hand, from both lists at once.
fn forget(app: &App, fingerprint: String) {
    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::forget(fingerprint).await {
            act(&app, Target::Close);
            notice(&app, &reason, true);
            return;
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Disconnects the computer controlling this one right now.
fn disconnect(app: &App, fingerprint: String) {
    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::kick(fingerprint).await {
            notice(&app, &reason, true);
            return;
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Opens a session to this card's computer, by the best way or through
/// this network and nothing else.
fn launch_the_peer(app: &App, rank: usize, local_only: bool) {
    let aimed = SEEN
        .lock()
        .expect("accueil")
        .as_ref()
        .and_then(|seen| seen.peers.get(rank).cloned());
    if let Some(peer) = aimed {
        launch(
            app,
            &peer.address,
            &peer.fingerprint,
            &peer.name,
            local_only,
        );
    }
}

/// Opens a session to this computer.
///
/// `local_only` keeps it on this network: the address from here and
/// nothing else, without any server being consulted.
fn launch(app: &App, address: &str, fingerprint: &str, name: &str, local_only: bool) {
    {
        let seen = SEEN.lock().expect("accueil");
        let state = STATE.lock().expect("accueil");
        if seen.as_ref().is_some_and(|seen| seen.busy(&state)) {
            return;
        }
    }
    {
        let mut state = STATE.lock().expect("accueil");
        state.notice = None;
        state.opening = Some(Opening {
            // The name rather than the address: nobody recognises
            // their laptop by its four numbers.
            towards: name.to_string(),
            detail: "Ouverture du tunnel…".to_string(),
            since: std::time::Instant::now(),
        });
    }
    redraw(app);

    let (app, address, fingerprint) = (app.clone(), address.to_string(), fingerprint.to_string());
    crate::app::spawn(async move {
        if let Err(reason) =
            crate::session::connect(app.clone(), address, fingerprint, local_only).await
        {
            failed(&app, &reason);
        }
    });
}

/* ---- The account ---------------------------------------------------------- */

/// Opens the account dialogue with this machine's name already written.
fn open_the_account(app: &App) {
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Account;
        state.dialogue_scroll = 0.0;
        state.sign_up = false;
        state.pinning = None;
        state.attaching = false;
        state.trouble = None;
    }
    close_the_fields();
    open_the_fields(&Field::ACCOUNT);
    write_in_the_field(Field::DeviceName, &zyr_proto::machine::name());
    redraw(app);
}

/// Attaches this computer to the account written in the fields.
///
/// `pinning` is the key of a server nobody vouches for, once the person
/// has compared it: without it, such a server answers with its key and
/// the dialogue shows it, with a way to confirm it.
fn attach(app: &App, pinning: Option<String>) {
    let server = text_of_the_field(Field::Server).trim().to_string();
    let user = text_of_the_field(Field::User).trim().to_string();
    let password = text_of_the_field(Field::Password);
    if server.is_empty() || user.is_empty() || password.is_empty() {
        return;
    }
    let sign_up = STATE.lock().expect("accueil").sign_up;
    let empty_or = |field: Field| {
        let text = text_of_the_field(field).trim().to_string();
        (!text.is_empty()).then_some(text)
    };
    let request = Attach {
        server,
        username: user,
        password,
        register: sign_up.then(|| Registering {
            email: empty_or(Field::Email),
            invitation: empty_or(Field::Invitation),
        }),
        name: text_of_the_field(Field::DeviceName).trim().to_string(),
        pin: pinning.and_then(|fingerprint| fingerprint.parse().ok()),
    };
    {
        let mut state = STATE.lock().expect("accueil");
        state.attaching = true;
        state.trouble = None;
    }
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        let outcome = crate::desk::attach(request).await;
        // What the state keeps of the answer, written under its lock,
        // which is released before waiting for anything else.
        let attached = {
            let mut state = STATE.lock().expect("accueil");
            state.attaching = false;
            match outcome {
                Ok(Attached::Done) => true,
                Ok(Attached::Unpinned(presented)) => {
                    state.pinning = Some(presented);
                    false
                }
                Err(reason) => {
                    state.trouble = Some(reason);
                    false
                }
            }
        };
        if !attached {
            redraw(&app);
            return;
        }
        // The dialogue closes on the thread that owns its fields: they
        // are system windows, and destroying a window from another
        // thread destroys nothing.
        let held = app.clone();
        let _ = app.run_on_main_thread(move || act(&held, Target::Close));
        reread(&app).await;
        notice(&app, "Cet ordinateur est rattaché au compte.", false);
    });
}

/// Detaches this computer from its account. A second click is asked
/// for, and the wait lapses by itself.
fn detach(app: &App) {
    {
        let mut state = STATE.lock().expect("accueil");
        let armed = state
            .detaching
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        if !armed {
            state.detaching = Some(std::time::Instant::now());
            drop(state);
            redraw(app);
            return;
        }
        state.detaching = None;
        state.trouble = None;
    }
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::detach().await {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// The account's device at this rank, if it is still there.
fn device_of_the_account(rank: usize) -> Option<Device> {
    SEEN.lock()
        .expect("accueil")
        .as_ref()
        .and_then(|seen| seen.account.as_ref())
        .and_then(|account| account.devices.get(rank).cloned())
}

/// Opens a device's renaming, its name already written.
fn open_the_renaming(app: &App, rank: usize) {
    let Some(device) = device_of_the_account(rank) else {
        return;
    };
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Renaming;
        state.dialogue_scroll = 0.0;
        state.renaming = Some((device.id, device.name.clone()));
        state.trouble = None;
    }
    close_the_fields();
    open_the_fields(&Field::RENAMING);
    write_in_the_field(Field::NewName, &device.name);
    redraw(app);
}

/// Renames the device being renamed with what is written.
fn rename(app: &App) {
    let new_name = text_of_the_field(Field::NewName).trim().to_string();
    let Some((device, before)) = STATE.lock().expect("accueil").renaming.clone() else {
        return;
    };
    if new_name.is_empty() || new_name == before {
        return;
    }
    act(app, Target::Close);
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Settings;
        state.dialogue_scroll = 0.0;
    }
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::rename_device(device, new_name).await {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Revokes a device of the account. A second click is asked for, on
/// the same device, and the wait lapses by itself.
fn revoke(app: &App, rank: usize) {
    {
        let mut state = STATE.lock().expect("accueil");
        let armed = state
            .revocation
            .is_some_and(|(which, since)| which == rank && since.elapsed() < CONFIRM_TIME);
        if !armed {
            state.revocation = Some((rank, std::time::Instant::now()));
            drop(state);
            redraw(app);
            return;
        }
        state.revocation = None;
        state.trouble = None;
    }
    let Some(device) = device_of_the_account(rank) else {
        return;
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::revoke_device(device.id).await {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/* ---- What the session tells ---------------------------------------------- */

/// One step in the opening of a session.
///
/// Called by what drives the session: the window is the only one
/// that can say how far along something is that has no picture yet.
pub fn step(app: &App, detail: &str) {
    {
        let mut state = STATE.lock().expect("accueil");
        let Some(opening) = state.opening.as_mut() else {
            return;
        };
        opening.detail = detail.to_string();
    }
    redraw(app);
}

/// The window has nothing more to tell: what is happening now can be
/// read in what the service holds.
pub fn put_the_opening_away(app: &App) {
    let app = app.clone();
    crate::app::spawn(async move {
        reread(&app).await;
        STATE.lock().expect("accueil").opening = None;
        redraw(&app);
    });
}

/// A session that ended badly, or that could not open.
pub fn failed(app: &App, text: &str) {
    notice(app, text, true);
    put_the_opening_away(app);
}

/// The banner at the top.
fn notice(app: &App, text: &str, is_trouble: bool) {
    STATE.lock().expect("accueil").notice = Some(Notice {
        text: text.to_string(),
        is_trouble,
        since: std::time::Instant::now(),
    });
    redraw(app);
}

/// What the settings have to complain about, which lives in
/// their dialogue.
fn say_the_trouble(app: &App, text: &str) {
    STATE.lock().expect("accueil").trouble = Some(text.to_string());
    redraw(app);
}

/* ---- The journal --------------------------------------------------------- */

fn open_the_journal(app: &App, from: Option<Peer>) {
    // What was written in the box is kept: this journal is opened twice
    // in a row for the same sift, once here and once over there, and
    // typing it again would be half the work.
    let sift = text_of_the_field(Field::Sift);
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Journal;
        state.journal_of = from;
        state.emptying = None;
        state.lines_scroll = (0.0, 0.0);
        state.lines = vec!["Lecture…".to_string()];
        state.sift = None;
        // Those of the previous page are not those of this one, and that
        // is most true when going from one's own journal to the far one.
        state.tags = Vec::new();
    }
    close_the_fields();
    open_the_fields(&Field::JOURNAL);
    write_in_the_field(Field::Sift, &sift);
    redraw(app);
    reread_the_journal(app, After::Show);
}

/// What is done with the page once read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    /// Showing it, and nothing more.
    Show,
    /// Showing it and taking it: "Copier le tri" clicked on a page that
    /// was not yet the sift's.
    Take,
}

/// Asks again for the page of the open journal, sifted as the box asks.
///
/// Called from the drawing thread, which is the only one that can read
/// the box and set this window's timers.
fn reread_the_journal(app: &App, after: After) {
    // A reading that was waiting for its pause no longer has any reason
    // to be: this one replaces it.
    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window != 0 {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::KillTimer;
        // SAFETY: a timer of ours, on the thread that set it. Nothing
        // if it was not set.
        unsafe { KillTimer(window as HWND, SIFT_PAUSE) };
    }
    // Read before leaving: the question goes off on another thread,
    // and the field belongs to the drawing one.
    let sift = text_of_the_field(Field::Sift).trim().to_string();
    let from = {
        let mut state = STATE.lock().expect("accueil");
        state.sift_asked = sift.clone();
        state.journal_of.clone()
    };
    let app = app.clone();
    crate::app::spawn(async move {
        let text = match &from {
            None => crate::journal::journal(&sift).await,
            Some(peer) => crate::journal::far_journal(
                peer.address.clone(),
                peer.fingerprint.clone(),
                sift.clone(),
            )
            .await
            // Shown in the journal itself: that is where the person who
            // has just clicked is looking, and a computer that does not
            // answer is already half the answer.
            .unwrap_or_else(|reason| reason),
        };
        // Reaching a distant machine takes as long as it takes: the
        // journal may have been closed, or switched to another computer
        // or another sift, in the meantime. What arrives late does not
        // overwrite what is on screen.
        let mut state = STATE.lock().expect("accueil");
        if state.journal_of != from || state.screen != Screen::Journal || state.sift_asked != sift {
            return;
        }
        state.lines = text.lines().map(str::to_string).collect();
        state.tags = zyr_proto::journal::names_in(&text);
        state.sift = Some(sift);
        // The most recent is at the bottom: that is where what has just
        // happened is, and it is what one opens the journal to read.
        // Lower than everything rather than by a counted height: what has
        // just been read has not been measured yet, and it is the drawing
        // that will bring this number back to what there really is to
        // see.
        state.lines_scroll = (0.0, VERY_BOTTOM);
        let taken = (after == After::Take).then(|| state.lines.join("\n"));
        drop(state);
        redraw(&app);
        if let Some(whole) = taken {
            // Placed from the drawing thread, like every copy this
            // window makes.
            let held = app.clone();
            let _ = app.run_on_main_thread(move || copy(&held, &whole, Target::CopyJournal));
        }
    });
}

/// Ticks or unticks that name in the sift box.
///
/// The name is added to or removed from what is already written rather
/// than replacing it: ticking two names is what keeps both subjects at
/// once, and what had been typed by hand next to them stays where it
/// was.
fn toggle_the_tag(app: &App, rank: usize) {
    let Some(name) = STATE.lock().expect("accueil").tags.get(rank).cloned() else {
        return;
    };
    let mut words: Vec<String> = text_of_the_field(Field::Sift)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    match words.iter().position(|text| *text == name) {
        Some(at) => {
            words.remove(at);
        }
        None => words.push(name),
    }
    write_in_the_field(Field::Sift, &words.join(" "));
    // Read again right away: a click has said what it wanted, there is
    // no letter left to wait for.
    reread_the_journal(app, After::Show);
}

/// Takes the journal page, which has to answer what is written in the
/// box.
///
/// The button says "Copier le tri" and must never take anything else:
/// between the sift pasted into the box and the page narrowing there is
/// the timer's pause and the service's round trip, and that is just
/// enough time to click in between. A page that is behind is therefore
/// read again, and it is its answer that goes.
fn copy_the_journal(app: &App) {
    let sift = text_of_the_field(Field::Sift).trim().to_string();
    let page = {
        let state = STATE.lock().expect("accueil");
        (state.sift.as_deref() == Some(sift.as_str())).then(|| state.lines.join("\n"))
    };
    match page {
        Some(whole) => copy(app, &whole, Target::CopyJournal),
        None => reread_the_journal(app, After::Take),
    }
}

/// Emptying wipes out the only trace of what has just happened. A second
/// click is asked for, and the wait lapses by itself.
fn empty_the_journal(app: &App) {
    let from = {
        let mut state = STATE.lock().expect("accueil");
        let armed = state
            .emptying
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        if !armed {
            state.emptying = Some(std::time::Instant::now());
            drop(state);
            redraw(app);
            return;
        }
        state.emptying = None;
        state.lines = vec!["Vidage…".to_string()];
        state.journal_of.clone()
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        // Emptied where it is written: this machine's right away, the far
        // one's by asking it.
        let done = match &from {
            None => crate::journal::clear_journal(),
            Some(peer) => {
                crate::journal::clear_far_journal(peer.address.clone(), peer.fingerprint.clone())
                    .await
            }
        };
        if let Err(reason) = done {
            STATE.lock().expect("accueil").lines = reason.lines().map(str::to_string).collect();
            redraw(&app);
            return;
        }
        let held = app.clone();
        let _ = app.run_on_main_thread(move || reread_the_journal(&held, After::Show));
    });
}

/* ---- The clipboard -------------------------------------------------------- */

/// Copies this text, and has the button say it did.
///
/// The clipboard can refuse, and a button saying "Copié" after a refusal
/// would send someone to paste nothing on the other computer.
fn copy(app: &App, text: &str, target: Target) {
    if let Err(e) = zyr_clipboard::hold_this(&zyr_proto::clipboard::Clip::text(text)) {
        note(&format!("copie refusée : {e}"));
        notice(app, "La copie a été refusée par Windows.", true);
        return;
    }
    // What an incomplete placing would give back only concerns pictures,
    // and this button copies nothing but text: there is only one shape to
    // place, and either it is placed or the refusal above has said so.
    STATE.lock().expect("accueil").copied = Some((target, std::time::Instant::now()));
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        tokio::time::sleep(COPIED_TIME).await;
        STATE.lock().expect("accueil").copied = None;
        redraw(&app);
    });
}

/* ---- What is asked of the service again ---------------------------------- */

/// Keeps asking again what the service holds.
///
/// The service can start after this window, or stop while it is open; a
/// session can be opened from the other end. None of that goes through
/// a click.
fn watch(app: App) {
    crate::app::spawn(async move {
        // What does not move for the whole life of the program: asked
        // for once.
        {
            let mut seen = SEEN.lock().expect("accueil");
            let new = seen.get_or_insert_with(Seen::default);
            new.version = crate::desk::build();
            new.folder = crate::folders::logs_folder();
            new.shortcuts = crate::shortcuts::engraved();
        }
        loop {
            if reread(&app).await {
                redraw(&app);
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Asks again what the service says, and says whether anything has
/// changed.
///
/// What has not changed is not redrawn: the window often stays open
/// during a session, and repainting an identical frame every three
/// seconds would be processor time taken from the session's picture.
async fn reread(app: &App) -> bool {
    let machine = crate::desk::standing().await;
    let peers = crate::desk::peers().await;
    let sessions = crate::session::sessions().await;
    let watching = crate::desk::watching().await;
    let ffmpeg_here = crate::folders::ffmpeg_here();
    let settings = crate::settings::settings(app.clone()).await;
    // The account, and its devices when there is a link: without a link
    // there is nothing to ask, and without a service nothing to show.
    let account = match crate::desk::account().await {
        Ok(link) => Some(AccountState {
            devices: if link.is_some() {
                crate::desk::devices().await
            } else {
                Vec::new()
            },
            link,
        }),
        Err(_) => None,
    };

    let mut seen = SEEN.lock().expect("accueil");
    let new = seen.get_or_insert_with(Seen::default);
    let before = Seen {
        machine: new.machine.replace(machine),
        peers: std::mem::replace(&mut new.peers, peers),
        sessions: std::mem::replace(&mut new.sessions, sessions),
        watching: std::mem::replace(&mut new.watching, watching),
        ffmpeg_here: new.ffmpeg_here.replace(ffmpeg_here),
        settings: new.settings.replace(settings),
        account: std::mem::replace(&mut new.account, account),
        shortcuts: new.shortcuts.clone(),
        version: new.version.clone(),
        folder: new.folder.clone(),
    };
    let mut change = before != *new;
    drop(seen);

    // Good news goes away by itself: left on screen, it ends up
    // reading as a state. A problem stays until the next gesture,
    // since it is waiting to be answered.
    let mut state = STATE.lock().expect("accueil");
    if state
        .notice
        .as_ref()
        .is_some_and(|said| !said.is_trouble && said.since.elapsed() > NOTICE_TIME)
    {
        state.notice = None;
        change = true;
    }
    change
}

/// Rereads what the settings screen shows, and the three shortcuts.
fn reread_the_settings(app: &App) {
    let app = app.clone();
    crate::app::spawn(async move {
        let settings = crate::settings::settings(app.clone()).await;
        if let Some(seen) = SEEN.lock().expect("accueil").as_mut() {
            seen.settings = Some(settings);
            seen.shortcuts = crate::shortcuts::engraved();
        }
        redraw(&app);
    });
}
