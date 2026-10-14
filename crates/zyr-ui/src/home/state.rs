//! Where the home window stands: what is open, what the hand is on, what
//! is waiting for an answer, and everything that can be clicked.
//!
//! Nothing here is the product's: what the computers are, what the
//! service says, lives in `seen`. This is only the window's own memory
//! from one frame to the next.

use std::time::Instant;

use zyr_draw::Rect;
use zyr_draw::motion::Motion;
use zyr_proto::fact::Fact;
use zyr_session::presence::Narrowed;

use crate::shell::shortcuts::Doing;
use crate::shell::theme::Choice;

/// A page of the window, chosen in the bar on the left.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Page {
    Home,
    Computers,
    Settings,
    About,
}

/// How the computers are laid out: cards in a grid, or rows in a list.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum View {
    #[default]
    Grid,
    List,
}

/// A menu open over the page.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Menu {
    /// What can be done with the computer of this fingerprint.
    Computer(String),
    /// Which computers stay in view.
    Filter,
}

/// What the picture kept under a cover was drawn from: the same again
/// and it can be laid down as it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Kept {
    pub generation: u64,
    pub light: bool,
    pub scale: u32,
    pub size: (u32, u32),
}

/// A tab of the settings page, chosen in the row at its top.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SettingsTab {
    General,
    Account,
    Shortcuts,
    Journal,
    Advanced,
}

/// A dialog open over the page.
#[derive(Clone, PartialEq, Debug)]
pub enum Dialog {
    /// Writing a computer down by hand, and forgetting those written.
    Adding,
    /// Everything about this computer, its fingerprint included.
    Details,
    /// Attaching this computer to an account on a server.
    SignIn,
    /// Renaming a device of the account: its identifier and its name.
    Renaming { device: String, name: String },
    /// The journal of this computer, or of the one with this fingerprint
    /// at this address, named so.
    Journal { of: Option<FarJournal> },
    /// The full text of one of the licences the product is made under.
    Licence(usize),
}

/// The computer whose journal is open, as it is reached.
#[derive(Clone, PartialEq, Debug)]
pub struct FarJournal {
    pub name: String,
    pub address: String,
    pub fingerprint: String,
}

/// A switch, and what it controls.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Toggle {
    Access,
    Trust,
    AtBoot,
    FarSpeakers,
    Statistics,
    Marking,
    FixedPort,
}

/// A row of sides, only one of which is chosen.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Pick {
    Codec,
    Display,
    Mouse,
    /// In the account dialog: signing in, or creating the account.
    SignUp,
}

/// Something whose text a button copies.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Copyable {
    Fingerprint,
    Journal,
}

/// What a computer's menu offers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Offer {
    /// Opening the session through this network and nothing else.
    HereOnly,
    Journal,
    /// Sending back the computer controlling this one right now.
    SendBack,
    /// Forgetting a computer written down by hand.
    Forget,
}

/// What scrolls.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Scroller {
    Page,
    Dialog,
    /// The journal's lines, which scroll inside their dialog.
    Lines,
}

/// Everything that answers a click or a key, and what it is about.
///
/// Computers are named by their fingerprint and not by their place in a
/// list: the list is read again every few seconds, and a click must never
/// land on the computer that has just taken the place of another.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Target {
    Nav(Page),
    /// The button that unfolds the bar on the left in a narrow window,
    /// and the veil that folds it back.
    Drawer,
    Fold,
    /// The person at the bottom of the bar, which leads to the account.
    Person,
    Switch(Toggle),
    /// What repairs what keeps this computer from working.
    Remedy,
    /// Sending back whoever controls this computer, from the banner.
    SendBackAll,
    RenameThis,
    Details,
    Connect(String),
    /// Opens or closes a menu.
    Menu(Menu),
    Offer(String, Offer),
    /// The search at the top, which a click puts the keyboard in, and the
    /// cross that empties it.
    Search,
    ClearSearch,
    Narrow(Narrowed),
    /// Empties the search and lifts the filter.
    ShowAll,
    View(View),
    AddComputer,
    Theme(Choice),
    Side(Pick, usize),
    Shortcut(Doing),
    SettingsTab(SettingsTab),
    SignIn,
    Detach,
    RenameDevice(String),
    Revoke(String),
    OpenJournal,
    OpenLogs,
    /// An address on the web, opened in the browser.
    Link(&'static str),
    Licence(usize),
    CloseDialog,
    /// What the main button of the open dialog does, which is also what
    /// Enter does.
    Confirm,
    Copy(Copyable),
    Forget(String),
    JournalRefresh,
    JournalEmpty,
    JournalTag(String),
    Scrollbar(Scroller),
    DismissNotice,
}

/// What the person has just been told: a success that fades by itself,
/// or a trouble that stays until the next gesture.
pub struct Notice {
    pub text: String,
    pub trouble: bool,
    pub since: Instant,
}

/// A session being opened, or coming back.
pub struct Opening {
    /// Which computer, by the name its owner knows it by.
    pub towards: String,
    /// The step it has reached, as the session tells it.
    pub detail: String,
    pub since: Instant,
}

/// Something the drawing laid down that the hand can be on: what it
/// does when clicked, if anything, and what a bubble says over it.
#[derive(Clone)]
pub struct Hit {
    pub target: Option<Target>,
    pub at: Rect,
    pub tip: Option<String>,
}

/// How far a scrolling thing has scrolled, how far it is going, and what
/// the last frame measured of it.
#[derive(Clone, Copy, Default)]
pub struct Scrolled {
    /// Where it is shown now, easing towards `goal`.
    pub now: f32,
    pub goal: f32,
    /// What it holds, the room it is shown in, and the travel of its
    /// thumb, as the last frame laid them down.
    pub content: f32,
    pub room: f32,
    pub travel: f32,
}

impl Scrolled {
    /// Asks for this far, never beyond what there is.
    pub fn go(&mut self, to: f32) {
        self.goal = to.clamp(0.0, (self.content - self.room).max(0.0));
    }

    /// Jumps there at once: a page that changes starts at its top.
    pub fn reset(&mut self) {
        self.now = 0.0;
        self.goal = 0.0;
    }
}

/// The window's memory.
pub struct State {
    pub page: Page,
    pub dialog: Option<Dialog>,
    /// The bar on the left, unfolded over a narrow window.
    pub drawer: bool,
    /// The menu open, and where the button that opened it was.
    pub menu: Option<(Menu, Rect)>,
    pub opening: Option<Opening>,
    pub notice: Option<Notice>,
    /// What went wrong in the open dialog, said inside it rather than
    /// at the bottom of the window, where the dialog would hide it.
    pub trouble: Option<String>,

    /// What the computers are narrowed to: the words searched for, the
    /// filter, and how they are laid out.
    pub search: String,
    pub narrowed: Narrowed,
    pub view: View,

    /// What the last frame laid down, the last laid on top, and where
    /// the hand was last seen over the window.
    pub hits: Vec<Hit>,
    pub pointer: Option<(f32, f32)>,
    /// What the hand is on, and since when: a bubble waits a moment
    /// before it opens.
    pub hover: Option<usize>,
    pub hover_since: Instant,
    /// What was pressed and not yet let go.
    pub pressed: Option<Target>,
    /// What the keyboard is on, and whether the keyboard has been used
    /// since the mouse was: the ring around it shows only then.
    pub focus: Option<Target>,
    pub focus_shown: bool,

    pub page_scroll: Scrolled,
    pub dialog_scroll: Scrolled,
    pub lines_scroll: Scrolled,
    /// How far the journal's lines are pushed sideways: a line does not
    /// wrap, it is read by moving along it.
    pub lines_across: f32,
    /// The scrollbar held by a hand, and where the hand took it.
    pub held: Option<(Scroller, f32)>,

    /// Switches pushed that the service has not acknowledged yet: until
    /// it does, the switch stays where the hand put it.
    pub pushed: Vec<(Toggle, bool)>,
    /// The button asking for a second click, and since when.
    pub armed: Option<(Target, Instant)>,
    /// The button that has just copied, and since when.
    pub copied: Option<(Copyable, Instant)>,
    /// The shortcut waiting for its combination.
    pub listening: Option<Doing>,
    /// The tab the settings page shows.
    pub settings_tab: SettingsTab,
    /// What the picture kept under a dialog was drawn from, while one is
    /// kept.
    pub kept: Option<Kept>,
    /// In the account dialog: creating the account rather than signing
    /// in to it.
    pub sign_up: bool,
    /// The key of a server nobody vouches for, waiting to be compared.
    pub pinning: Option<String>,
    pub attaching: bool,

    /// The open journal, line by line, the names it carries, the sift it
    /// answers once an answer has come, and the sift last asked for.
    pub lines: Vec<String>,
    pub tags: Vec<String>,
    pub sift: Option<String>,
    pub sift_asked: String,

    pub motion: Motion,
    /// When the window must be drawn again with nothing happening: a
    /// bubble about to open, good news about to go, a button about to
    /// stop asking for its second click.
    pub wake_at: Option<Instant>,
}

impl State {
    pub fn new() -> Self {
        State {
            page: Page::Home,
            dialog: None,
            drawer: false,
            menu: None,
            opening: None,
            notice: None,
            trouble: None,
            search: String::new(),
            narrowed: Narrowed::All,
            view: View::Grid,
            hits: Vec::new(),
            pointer: None,
            hover: None,
            hover_since: Instant::now(),
            pressed: None,
            focus: None,
            focus_shown: false,
            page_scroll: Scrolled::default(),
            dialog_scroll: Scrolled::default(),
            lines_scroll: Scrolled::default(),
            lines_across: 0.0,
            held: None,
            pushed: Vec::new(),
            armed: None,
            copied: None,
            listening: None,
            settings_tab: SettingsTab::General,
            kept: None,
            sign_up: false,
            pinning: None,
            attaching: false,
            lines: Vec::new(),
            tags: Vec::new(),
            sift: None,
            sift_asked: String::new(),
            motion: Motion::new(),
            wake_at: None,
        }
    }

    /// Asks for the window to be drawn again at that instant, unless it
    /// already is earlier.
    pub fn wake(&mut self, at: Instant) {
        self.wake_at = Some(self.wake_at.map_or(at, |before| before.min(at)));
    }

    /// The target under the hand, if what the hand is on answers.
    pub fn hovered(&self) -> Option<&Target> {
        self.hover
            .and_then(|at| self.hits.get(at))
            .and_then(|hit| hit.target.as_ref())
    }

    /// The scrolling thing of that name.
    pub fn scrolled(&mut self, which: Scroller) -> &mut Scrolled {
        match which {
            Scroller::Page => &mut self.page_scroll,
            Scroller::Dialog => &mut self.dialog_scroll,
            Scroller::Lines => &mut self.lines_scroll,
        }
    }

    /// Whether that button is waiting for its second click.
    pub fn is_armed(&self, target: &Target) -> bool {
        self.armed
            .as_ref()
            .is_some_and(|(armed, since)| armed == target && since.elapsed() < CONFIRM_TIME)
    }

    /// Whether that button has just copied, and still says so.
    pub fn has_copied(&self, what: Copyable) -> bool {
        self.copied
            .is_some_and(|(copied, since)| copied == what && since.elapsed() < COPIED_TIME)
    }

    /// Says a trouble at the bottom of the window.
    pub fn tell_trouble(&mut self, fact: &Fact) {
        self.notice = Some(Notice {
            text: zyr_i18n::fact(fact),
            trouble: true,
            since: Instant::now(),
        });
    }

    /// Says a piece of good news at the bottom of the window.
    pub fn tell_news(&mut self, text: String) {
        self.notice = Some(Notice {
            text,
            trouble: false,
            since: Instant::now(),
        });
    }
}

/// How long a button stays armed for its second click.
pub const CONFIRM_TIME: std::time::Duration = std::time::Duration::from_secs(4);

/// How long « Copied » stays on a button before it says its own word
/// again.
pub const COPIED_TIME: std::time::Duration = std::time::Duration::from_millis(1600);

/// How long a piece of good news stays on screen.
pub const NEWS_TIME: std::time::Duration = std::time::Duration::from_secs(6);
