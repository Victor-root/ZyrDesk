//! Settings of a remote session.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Codec {
    #[default]
    Auto,
    H264,
    Hevc,
    Av1,
}

impl Codec {
    /// Value the client engine's command line expects.
    pub fn engine_value(self) -> &'static str {
        match self {
            Codec::Auto => "auto",
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::Av1 => "AV1",
        }
    }
}

/// The same spelling travels between our own programs.
///
/// One spelling and one reader rather than two of each: a second table
/// would drift from this one the day a codec is added.
impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.engine_value())
    }
}

impl std::str::FromStr for Codec {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().replace(['.', '-'], "").as_str() {
            "auto" => Ok(Codec::Auto),
            "h264" => Ok(Codec::H264),
            "hevc" | "h265" => Ok(Codec::Hevc),
            "av1" => Ok(Codec::Av1),
            _ => Err(format!("codec inconnu : {text}")),
        }
    }
}

/// How the session sits on the screen.
///
/// Two, and they describe the product's own window: the picture is shown
/// inside it rather than in a window of the engine's, so how it is shown
/// is a question about that window and not about the engine. The engine
/// is always started windowed, its window is stripped of its frame and
/// laid over ours, and it follows ours wherever it goes.
///
/// The exclusive full screen the engine can do is gone with that.
/// Nothing can be drawn over a window that owns the screen exclusively,
/// and the floating button of a session is drawn over it. It cost
/// nothing to lose: on Windows 10 and later the compositor hands the
/// screen straight to a swap chain of the kind the engine uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    /// The window takes the whole screen.
    Fullscreen,
    /// An ordinary window, which can be moved and resized.
    ///
    /// What a first session opens as. Covering the screen on the very
    /// first try leaves somebody looking at another computer's desktop
    /// with nothing of this product in sight and no way back they have
    /// been shown; a window keeps the way out where every window keeps
    /// it. The choice is remembered from there on, so anybody who wants
    /// the screen asks once.
    #[default]
    Windowed,
}

impl DisplayMode {
    fn written(self) -> &'static str {
        match self {
            DisplayMode::Fullscreen => "fullscreen",
            DisplayMode::Windowed => "windowed",
        }
    }
}

impl fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.written())
    }
}

impl std::str::FromStr for DisplayMode {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().as_str() {
            "fullscreen" | "plein-ecran" => Ok(DisplayMode::Fullscreen),
            "windowed" | "fenetre" => Ok(DisplayMode::Windowed),
            _ => Err(format!("mode d'affichage inconnu : {text}")),
        }
    }
}

/// Settings of one session.
///
/// The defaults mirror the client engine's own for 1080p60, which the
/// comparison against unmanaged engines required at milestone M1. The
/// window it opens in is ours to decide, and is decided once, where
/// `DisplayMode` says why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: Codec,
    pub display_mode: DisplayMode,
    /// Packet size to impose. `None` lets the engine decide, which is
    /// what it did before the tunnel existed; the tunnel now computes it
    /// from what the path actually carries.
    pub packet_size: Option<u32>,
    /// Absolute mouse: right for a desktop, wrong for games that aim
    /// with relative motion.
    pub absolute_mouse: bool,
    pub stats_overlay: bool,
    /// Whether Alt+Tab, Échap and the Windows key go to the session
    /// rather than to this computer.
    ///
    /// Only where the session starts: it is a switch, and the menu throws
    /// it while the picture runs.
    pub system_keys: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 20_000,
            codec: Codec::Auto,
            display_mode: DisplayMode::default(),
            packet_size: None,
            absolute_mouse: true,
            stats_overlay: false,
            system_keys: true,
        }
    }
}

/// The screen a session is going to be shown on, as this computer
/// measures it.
///
/// Its size and its rate, and both for the same reason. A picture asked
/// for at a size other than the screen it lands on is thrown away twice,
/// and one asked for at a rate other than the screen refreshes at is
/// shown unevenly: two frames land inside one refresh and one of them is
/// never seen, or a refresh comes with nothing new and shows the frame
/// before it again. Neither is put back by anything downstream, and both
/// are felt rather than seen, which is why they are worth measuring
/// rather than assuming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub wide: u32,
    pub high: u32,
    /// Times a second it refreshes, as the system rounds it: a panel at
    /// 59.997 hertz is reported as sixty, which is the number to ask for.
    pub refresh: u32,
    /// How much larger than life the system draws on it, in percent.
    ///
    /// A hundred is life size. It belongs beside the other two and not
    /// somewhere else, because a screen is not described without it: the
    /// same panel at the same size draws text half as tall at a hundred
    /// as it does at two hundred, and a session that carries the size
    /// without this one hands somebody a desk that is theirs in shape and
    /// nobody's in scale.
    ///
    /// Nought means it could not be read, and it is never asked for: a
    /// magnification nobody measured is a guess, and a guess is worse
    /// than letting the far computer keep its own recommendation.
    pub scale: u32,
}

/// Slowest and fastest a session is opened at, whatever a screen says.
///
/// A screen slower than the first is a reading that went wrong rather
/// than a screen anybody works on. Above the second, every extra frame is
/// paid in full by the far computer, which has to draw it, encode it and
/// send it, and no desktop needs it: a remote desktop is text and a
/// pointer, not a game. The ceiling covers the panels people actually sit
/// in front of, a hundred and forty-four included, and there is nowhere
/// to turn it down from, so it stands in for the dial that does not
/// exist.
pub const SLOWEST_RATE: u32 = 30;
pub const FASTEST_RATE: u32 = 144;

/// Life size: a screen drawing exactly what it is asked to draw.
pub const LIFE_SIZE: u32 = 100;

impl Screen {
    /// The rate a session opened on this screen asks for.
    ///
    /// Nought and one are what Windows answers for a screen whose rate it
    /// does not hold. They mean « not measured » and not « very slow », so
    /// they fall back to what every session asked for before anything was
    /// measured rather than down to the floor, which would make an
    /// unreadable screen worse off than an unmeasurable one.
    ///
    /// A screen faster than the ceiling is served a whole share of its
    /// own rate rather than the ceiling itself. The whole point of
    /// measuring is a picture whose frames land one to a refresh, and a
    /// rate that does not divide the screen's puts some of them two to a
    /// refresh and leaves others showing the frame before, which is the
    /// very fault this exists to close. So a screen above the ceiling is
    /// halved, then divided by three, until what is left fits.
    pub fn rate(self) -> u32 {
        match self.refresh {
            0 | 1 => SessionSettings::default().fps,
            measured if measured < SLOWEST_RATE => SLOWEST_RATE,
            measured => {
                let mut share = 1;
                while measured / share > FASTEST_RATE {
                    share += 1;
                }
                measured / share
            }
        }
    }

    /// Its size alone, for the parts that only ever cared about that.
    pub fn size(self) -> (u32, u32) {
        (self.wide, self.high)
    }
}

/// Which resolution a session runs at, and which of the two computers
/// decides it.
///
/// Three answers, and the first two are not sizes at all: they say whose
/// screen wins. That is the real question, because a picture is thrown
/// away twice whenever the two disagree. The far computer draws its
/// desktop at the size that was asked for, this end scales that to the
/// screen it lands on, and neither scaling puts back a single pixel the
/// first one dropped.
///
/// A number is still offered, because neither screen is always the right
/// answer. A large screen served over a busy link costs more rate than
/// the link has, and asking for less picture is the honest way to spend
/// less.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Asked {
    /// The screen showing the session, whatever it turns out to be.
    ///
    /// The far computer is made to match it, which is why it is the
    /// default: a pixel sent for a pixel shown, and nothing scaled at
    /// either end.
    #[default]
    Client,
    /// The far computer's own screen, left exactly as it is.
    ///
    /// Nothing over there is touched: no virtual screen, no resolution
    /// changed under whoever is sitting in front of it. What it costs is
    /// this end scaling the picture to fit, and what it buys is a machine
    /// that is not rearranged by being looked at. The size is not known
    /// here until that computer says it, which it does when the session
    /// opens.
    Host,
    /// This size, whatever either screen is.
    Fixed(u32, u32),
}

/// Screen assumed when this computer's own cannot be measured.
///
/// The common screen, which costs nothing to be wrong about.
pub const UNKNOWN_SCREEN: (u32, u32) = (1920, 1080);

/// The resolutions offered, in the order they are offered in.
///
/// Here and not in the window: a second copy of this list written in
/// JavaScript would drift from this one the day a size is added. The two
/// that name a computer rather than a number come first, because they are
/// the answer nearly everybody wants and the numbers below are the
/// exception.
///
/// The numbers themselves are the shapes screens are actually made in,
/// from a desk's largest down to a laptop's smallest, sorted by width so
/// the list reads as one scale even though several shapes share it.
pub const SIZES_OFFERED: &[Asked] = &[
    Asked::Client,
    Asked::Host,
    Asked::Fixed(3840, 2160),
    Asked::Fixed(3840, 1600),
    Asked::Fixed(3440, 1440),
    Asked::Fixed(2560, 1600),
    Asked::Fixed(2560, 1440),
    Asked::Fixed(2560, 1080),
    Asked::Fixed(1920, 1200),
    Asked::Fixed(1920, 1080),
    Asked::Fixed(1680, 1050),
    Asked::Fixed(1600, 1200),
    Asked::Fixed(1366, 768),
    Asked::Fixed(1280, 1024),
    Asked::Fixed(1280, 800),
    Asked::Fixed(1280, 720),
    Asked::Fixed(1024, 768),
];

/// The rates offered, in kilobits a second.
///
/// Wide on purpose, and open at the low end. What a link carries is not
/// something this product can work out, and neither is what a far
/// computer can encode in time: the one that matters here is the second,
/// since a computer that cannot encode a frame in sixteen milliseconds
/// sends fewer of them however empty the link is.
pub const RATES_OFFERED: &[u32] = &[
    5_000, 10_000, 15_000, 20_000, 30_000, 40_000, 60_000, 80_000,
];

/// The codecs offered.
pub const CODECS_OFFERED: &[Codec] = &[Codec::Auto, Codec::H264, Codec::Hevc, Codec::Av1];

impl Asked {
    /// The size this comes down to on a computer whose screen has been
    /// measured, `None` standing for one that could not be.
    ///
    /// The far computer's own screen comes down to this one's until that
    /// computer has said otherwise, which it does as the session opens.
    /// A number is wanted before the tunnel stands, so this is the
    /// standing-in one, and it is the best guess there is: the two are
    /// often the same screen, and where they are not, this end knows only
    /// its own.
    pub fn size(self, screen: Option<(u32, u32)>) -> (u32, u32) {
        let (wide, high) = match self {
            Asked::Client | Asked::Host => screen.unwrap_or(UNKNOWN_SCREEN),
            Asked::Fixed(wide, high) => (wide, high),
        };
        if wide == 0 || high == 0 {
            return UNKNOWN_SCREEN;
        }
        (even(wide), even(high))
    }

    /// Whether this asks the far computer for a screen of its own making.
    ///
    /// False for its own screen alone: that is the whole of what that
    /// choice means, and it is what keeps a machine from being rearranged
    /// by being looked at.
    pub fn wants_a_screen_over_there(self) -> bool {
        self != Asked::Host
    }

    /// How much larger than life that screen is asked to draw, nought
    /// asking for whatever the far computer recommends for it.
    ///
    /// A number only when it is this computer's screen being mirrored.
    /// That is the one case where the answer is known: the panel the
    /// session is watched on is measured, and a screen made in its image
    /// owes it the magnification as much as the size. A size picked by
    /// hand is nobody's panel, so there is nothing to copy and Windows'
    /// own recommendation for that size is the better answer; and the far
    /// computer's own screen is left alone entirely, magnification
    /// included, which is the whole of what that choice promises and is
    /// why no screen is asked for at all in that case.
    pub fn magnification(self, screen: Option<Screen>) -> u32 {
        match self {
            Asked::Client => screen.map_or(0, |screen| screen.scale),
            Asked::Host | Asked::Fixed(..) => 0,
        }
    }
}

/// One of the far computer's own screens, as that computer names it.
///
/// A machine with two screens plugged in serves one of them, and which
/// one is not something this end can work out: the identifier is a digest
/// that computer's engine alone computes, and nothing here has any
/// business recomputing it. So the whole description is asked for and
/// carried as it comes.
///
/// Only the screens that computer is actually showing on are ever
/// described this way. One that is switched off is not a screen anybody
/// asks to look at, and offering it would offer a black picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarScreen {
    /// What the far computer's engine takes orders by. Opaque here on
    /// purpose: it is read from that computer and handed straight back to
    /// it.
    pub id: String,
    /// Whether it is the one that computer starts its desktop on.
    ///
    /// The one a session is served from unless somebody says otherwise,
    /// which is what makes « the main one » a promise rather than
    /// whichever screen a graphics card happened to enumerate first.
    pub main: bool,
    pub wide: u32,
    pub high: u32,
    /// What the screen calls itself, which is the only part a person
    /// reads. Last, and it takes the whole of what is left: a monitor's
    /// name carries spaces far more often than not.
    pub name: String,
}

/// One spelling, written here and read here.
///
/// It crosses the tunnel and then the channel between our own programs,
/// and a second table would drift from this one the day a field is added.
impl fmt::Display for FarScreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}x{} {}",
            self.id,
            if self.main { "main" } else { "other" },
            self.wide,
            self.high,
            self.name
        )
    }
}

impl std::str::FromStr for FarScreen {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut words = text.trim().splitn(4, char::is_whitespace);
        let id = words.next().unwrap_or_default().trim();
        let which = words.next().unwrap_or_default().trim();
        let size = words.next().unwrap_or_default().trim();
        let name = words.next().unwrap_or_default().trim();
        if id.is_empty() {
            return Err(format!("écran sans identifiant : {text}"));
        }
        let main = match which {
            "main" => true,
            "other" => false,
            other => return Err(format!("« {other} » ne dit pas si c'est l'écran principal")),
        };
        let (wide, high) = parse_resolution(size).map_err(|e| e.to_string())?;
        Ok(FarScreen {
            id: id.to_string(),
            main,
            wide,
            high,
            name: name.to_string(),
        })
    }
}

/// A whole list of them, one to a line.
///
/// A line that cannot be read is dropped rather than failing the list: a
/// screen whose description this version does not understand costs that
/// screen and never the menu.
pub fn far_screens_written(screens: &[FarScreen]) -> String {
    screens
        .iter()
        .map(FarScreen::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn far_screens_read(text: &str) -> Vec<FarScreen> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.parse().ok())
        .collect()
}

/// The screen a session asks the far computer to put up for it.
///
/// A size and a magnification, and they travel as one because they are
/// one ask. A screen given the size of the panel a session is watched on
/// but not the way that panel draws is not that panel: the same pixels
/// carry text half as tall, and whoever asked to work on their own desk
/// is handed somebody else's at the right resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WantedScreen {
    pub wide: u32,
    pub high: u32,
    /// How much larger than life, in percent, as [`Screen::scale`] holds
    /// it.
    ///
    /// Nought names none, and asks for whatever the far computer's own
    /// Windows recommends for a screen that size. It is what a session
    /// that could not measure the screen it is watched on says, and what
    /// one that asked for a size rather than for a screen says: a
    /// magnification taken off a panel nobody is looking at is worse than
    /// no magnification at all.
    pub scale: u32,
}

/// One spelling of it, written here and read here.
///
/// It crosses two channels on its way over, the one between our own
/// programs and the one between the two computers, and a second table
/// would drift from this one the day a field is added.
impl fmt::Display for WantedScreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}@{}", self.wide, self.high, self.scale)
    }
}

impl std::str::FromStr for WantedScreen {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (size, magnification) = text.split_once('@').ok_or_else(|| {
            format!("écran attendu sous la forme LARGEURxHAUTEUR@AGRANDISSEMENT : {text}")
        })?;
        let (wide, high) = parse_resolution(size).map_err(|e| e.to_string())?;
        let scale = magnification
            .trim()
            .parse()
            .map_err(|_| format!("agrandissement attendu en pour cent : {magnification}"))?;
        Ok(WantedScreen { wide, high, scale })
    }
}

/// Rounds down to an even number, which is what a picture split into
/// colour by halves can be cut into. One is never a size worth keeping.
fn even(value: u32) -> u32 {
    (value & !1).max(2)
}

/// The value after this one in a list, back to the first at the end.
///
/// What a setting offered as one line of a menu does when it is clicked.
/// A value that is not in the list at all lands on the first, which is
/// what a list that has changed since a choice was written down leaves
/// behind.
pub fn next_in<T: PartialEq + Copy>(list: &[T], current: T) -> T {
    let Some(first) = list.first().copied() else {
        return current;
    };
    match list.iter().position(|value| *value == current) {
        Some(at) => list.get(at + 1).copied().unwrap_or(first),
        None => first,
    }
}

impl fmt::Display for Asked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Asked::Client => f.write_str("client"),
            Asked::Host => f.write_str("host"),
            Asked::Fixed(wide, high) => write!(f, "{wide}x{high}"),
        }
    }
}

impl std::str::FromStr for Asked {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        // « screen » is what this choice was written down as before the
        // far computer's own screen was offered beside it, and what every
        // preferences file written until then still says. Read and never
        // written, so a choice made once goes on meaning what it meant.
        if text.eq_ignore_ascii_case("client") || text.eq_ignore_ascii_case("screen") {
            return Ok(Asked::Client);
        }
        if text.eq_ignore_ascii_case("host") {
            return Ok(Asked::Host);
        }
        let (wide, high) = parse_resolution(text).map_err(|e| e.to_string())?;
        Ok(Asked::Fixed(wide, high))
    }
}

/// How the far computer takes the pictures it sends.
///
/// Two ways, and the choice between them is not obvious enough to be
/// made once for everybody: which is faster depends on the machine, and
/// on some it is not close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Capture {
    /// The way that also sees what Windows puts on its own protected
    /// desktop: the administrator prompt, and the sign-in screen. The
    /// safe answer, and the one this product started with.
    #[default]
    Duplication,
    /// Windows' own newer way of handing a window or a screen to a
    /// program. Faster on some machines, and blind to the protected
    /// desktop: an administrator prompt then shows as nothing at all.
    Windows,
}

impl Capture {
    /// What the host engine's configuration calls it.
    pub fn engine_value(self) -> &'static str {
        match self {
            Capture::Duplication => "ddx",
            Capture::Windows => "wgc",
        }
    }
}

impl fmt::Display for Capture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.engine_value())
    }
}

impl std::str::FromStr for Capture {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "ddx" | "duplication" => Ok(Capture::Duplication),
            "wgc" | "windows" => Ok(Capture::Windows),
            _ => Err(format!("façon de capturer inconnue : {text}")),
        }
    }
}

/// How this computer serves the sessions it is asked for.
///
/// Apart from `Preferred` on purpose, and it is the same distinction as
/// between a host and a client. `Preferred` is what this computer asks
/// of others; this is what others get from it. One machine is usually
/// both, and the two are still not the same settings: changing these
/// changes nothing about a session opened from here, and everything
/// about one opened towards here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Serving {
    /// Whether a still screen is sent again at the full rate.
    ///
    /// The engine only encodes a picture when the screen changes, and
    /// its own answer is half the rate that was asked for. On a desktop
    /// that costs the smoothness of a moving pointer, which is most of
    /// what a desktop is; so this is normally on.
    ///
    /// It is worth turning off on a computer that cannot keep up: it
    /// then spends nothing on a screen where nothing moved, and what it
    /// has goes to the pictures that changed.
    pub steady_rate: bool,
    pub capture: Capture,
}

impl Default for Serving {
    fn default() -> Self {
        Self {
            steady_rate: true,
            capture: Capture::default(),
        }
    }
}

/// What the person chose once, and every session then honours.
///
/// Apart from `SessionSettings` on purpose: the packet size is not a
/// choice anyone makes, it is what the path turns out to carry, and it
/// is settled when the tunnel stands. Putting it here would offer a
/// dial that the tunnel overrules a second later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferred {
    /// How much picture to ask for.
    pub asked: Asked,
    /// How much rate to carry it with, in kilobits a second.
    ///
    /// A number chosen by hand rather than worked out from the size,
    /// because the two are not tied by anything this end can measure.
    /// What a picture costs depends on what is moving in it, what a link
    /// carries depends on the link, and what a far computer can encode
    /// in time depends on that computer. Only the person watching sees
    /// all three at once.
    pub bitrate_kbps: u32,
    pub codec: Codec,
    pub display_mode: DisplayMode,
    /// Absolute mouse: right for a desktop, wrong for games that aim
    /// with relative motion.
    pub absolute_mouse: bool,
    pub stats_overlay: bool,
    /// Whether the far computer's speakers fall silent for the length of
    /// the session.
    ///
    /// A choice made here and not over there, which is the whole point:
    /// whoever takes control of a machine in another room is the one who
    /// knows that the room should go quiet, and they are not in it to
    /// walk over and say so. It travels with the session on the product's
    /// own channel, and the far computer gives its sound back when the
    /// session goes, whatever became of this one.
    ///
    /// It works because of a detail of how a computer's own output is
    /// recorded: what the far engine captures is the mix Windows hands to
    /// the sound card, copied before the card applies its own mute. The
    /// room falls silent and the session keeps its sound.
    ///
    /// Off until somebody asks. A computer that goes quiet the moment it
    /// is reached is a computer whoever sits in front of it would call
    /// broken.
    pub mute_far_speakers: bool,
    /// Whether Alt+Tab, Échap and the Windows key belong to the session
    /// or to the computer that is watching it.
    ///
    /// Windows keeps those for itself and hands them to nobody, so a
    /// session only ever gets them by stepping in front of every keystroke
    /// of the whole computer. Doing that all the time would be wrong the
    /// other way round: the hand reaching for Alt+Tab is sometimes
    /// reaching for a window of this very computer.
    ///
    /// On by default. A session whose Windows key quietly does nothing is
    /// the fault this exists to close, and the menu says which side the
    /// switch is on, so the other way round surprises nobody.
    pub system_keys: bool,
    /// Whether the far computer resends a still screen at full rate.
    ///
    /// A setting of that machine's engine, asked for from here, for the
    /// same reason its speakers are: the person who can tell whether the
    /// picture feels smooth is the one watching it, and they are not in
    /// front of the machine that would have to be told.
    ///
    /// It costs that machine a whole frame encoded sixty times a second
    /// over a desktop where nothing moves, and it buys a pointer that
    /// glides instead of stepping. Which of the two is worth more depends
    /// on the machine and on what is being done with it, so it is a
    /// choice and not a default worth defending.
    ///
    /// Its engine reads it when it starts and never again, so changing it
    /// starts that engine over. The menu therefore treats it like the
    /// size, the rate and the codec: written down, and applied when the
    /// picture is opened again.
    pub steady_far_rate: bool,
}

impl Default for Preferred {
    fn default() -> Self {
        Self {
            asked: Asked::default(),
            bitrate_kbps: SessionSettings::default().bitrate_kbps,
            codec: Codec::default(),
            display_mode: DisplayMode::default(),
            absolute_mouse: true,
            stats_overlay: false,
            mute_far_speakers: false,
            system_keys: true,
            steady_far_rate: Serving::default().steady_rate,
        }
    }
}

impl Preferred {
    /// The settings a session opens with on a computer whose screen has
    /// been measured, `None` standing for one that could not be.
    pub fn settings(self, screen: Option<Screen>) -> SessionSettings {
        let (width, height) = self.asked.size(screen.map(Screen::size));
        SessionSettings {
            width,
            height,
            // The rate of the screen it lands on, measured, and sixty
            // when it could not be. Sixty was what every session asked
            // for before anything was measured: right on the screens
            // most people have, and a picture shown unevenly on all the
            // others, which is the same mistake the size used to make.
            fps: screen.map_or(SessionSettings::default().fps, Screen::rate),
            bitrate_kbps: self.bitrate_kbps,
            codec: self.codec,
            display_mode: self.display_mode,
            absolute_mouse: self.absolute_mouse,
            stats_overlay: self.stats_overlay,
            system_keys: self.system_keys,
            ..SessionSettings::default()
        }
    }
}

/// Malformed resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidResolution(pub String);

impl fmt::Display for InvalidResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "résolution attendue sous la forme LARGEURxHAUTEUR : {}",
            self.0
        )
    }
}

impl std::error::Error for InvalidResolution {}

/// Reads a resolution such as `1920x1080`.
pub fn parse_resolution(value: &str) -> Result<(u32, u32), InvalidResolution> {
    let invalid = || InvalidResolution(value.to_string());
    let (left, right) = value.split_once(['x', 'X']).ok_or_else(invalid)?;
    let width: u32 = left.trim().parse().map_err(|_| invalid())?;
    let height: u32 = right.trim().parse().map_err(|_| invalid())?;
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un écran ordinaire de cette taille, pour les essais qui ne parlent
    /// que de taille.
    fn a_screen(wide: u32, high: u32) -> Screen {
        Screen {
            wide,
            high,
            refresh: 60,
            scale: LIFE_SIZE,
        }
    }

    #[test]
    fn codecs_carry_the_engine_spelling() {
        assert_eq!(Codec::Auto.engine_value(), "auto");
        assert_eq!(Codec::H264.engine_value(), "H.264");
        assert_eq!(Codec::Hevc.engine_value(), "HEVC");
        assert_eq!(Codec::Av1.engine_value(), "AV1");
    }

    #[test]
    fn codecs_are_read_whatever_their_spelling() {
        for (written, expected) in [
            ("auto", Codec::Auto),
            ("h264", Codec::H264),
            ("H.264", Codec::H264),
            ("hevc", Codec::Hevc),
            ("h265", Codec::Hevc),
            ("AV1", Codec::Av1),
        ] {
            assert_eq!(written.parse::<Codec>().unwrap(), expected, "{written}");
        }
        assert!("vp9".parse::<Codec>().is_err());
    }

    #[test]
    fn resolutions_valid_and_malformed() {
        assert_eq!(parse_resolution("1920x1080").unwrap(), (1920, 1080));
        assert_eq!(parse_resolution("2560X1440").unwrap(), (2560, 1440));
        for wrong in ["1920", "1920x", "x1080", "0x1080", "axb", ""] {
            assert!(parse_resolution(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn every_choice_survives_being_written_and_read_back() {
        // Ces valeurs voyagent en texte sur le canal de contrôle et
        // dans le fichier de réglages : elles doivent se relire.
        for codec in [Codec::Auto, Codec::H264, Codec::Hevc, Codec::Av1] {
            assert_eq!(codec.to_string().parse::<Codec>().unwrap(), codec);
        }
        for mode in [DisplayMode::Fullscreen, DisplayMode::Windowed] {
            assert_eq!(mode.to_string().parse::<DisplayMode>().unwrap(), mode);
        }
    }

    #[test]
    fn the_screen_is_asked_for_by_name_and_not_by_number() {
        // Le cas qui compte : une image de la taille de l'écran est
        // affichée pixel pour pixel. Demander autre chose, c'est jeter du
        // détail à un bout et l'agrandir à l'autre.
        assert_eq!(Asked::Client.size(Some((3840, 2160))), (3840, 2160));
        assert_eq!(Asked::Client.size(Some((3440, 1440))), (3440, 1440));
        // Et un nombre reste un nombre, quel que soit l'écran.
        assert_eq!(
            Asked::Fixed(1920, 1080).size(Some((3840, 2160))),
            (1920, 1080)
        );
    }

    #[test]
    fn an_unmeasurable_screen_falls_back_to_the_common_one() {
        assert_eq!(Asked::Client.size(None), UNKNOWN_SCREEN);
        assert_eq!(Asked::Client.size(Some((0, 0))), UNKNOWN_SCREEN);
        // L'écran d'en face n'est pas connu ici avant que la machine
        // d'en face ne le dise : en attendant, c'est celui-ci.
        assert_eq!(Asked::Host.size(Some((2560, 1440))), (2560, 1440));
        assert_eq!(Asked::Fixed(0, 1080).size(None), UNKNOWN_SCREEN);
    }

    #[test]
    fn the_magnification_of_this_screen_travels_only_when_it_is_this_screen() {
        // Le cas de Victor : un portable à cent vingt-cinq pour cent. La
        // session portait la taille et pas l'agrandissement, donc le
        // texte arrivait deux fois plus petit qu'à la maison.
        let mine = Screen {
            wide: 1920,
            high: 1200,
            refresh: 60,
            scale: 125,
        };
        assert_eq!(Asked::Client.magnification(Some(mine)), 125);
        // Une taille choisie à la main n'est l'écran de personne : rien à
        // copier, et la recommandation de Windows vaut mieux qu'un
        // agrandissement pris sur un autre panneau.
        assert_eq!(Asked::Fixed(1280, 720).magnification(Some(mine)), 0);
        // Et « l'écran de l'hôte » veut dire qu'on n'y touche pas, ni à
        // la taille ni au reste.
        assert_eq!(Asked::Host.magnification(Some(mine)), 0);
        // Un écran qu'on n'a pas su mesurer ne réclame rien : un
        // agrandissement deviné est pire qu'aucun.
        assert_eq!(Asked::Client.magnification(None), 0);
    }

    #[test]
    fn the_far_computers_screens_survive_being_written_and_read_back() {
        // Ils traversent le tunnel puis le canal entre nos programmes :
        // une seule écriture, une seule lecture. Le cas de Victor : deux
        // écrans allumés sur la machine d'en face.
        let screens = vec![
            FarScreen {
                id: "{daeac860-f4db-5208-b1f5-cf59444fb768}".to_string(),
                main: true,
                wide: 2560,
                high: 1440,
                name: "ROG PG279Q".to_string(),
            },
            // Un nom d'écran porte des espaces bien plus souvent qu'on ne
            // le croit : il voyage en fin de ligne pour cette raison.
            FarScreen {
                id: "{64243705-4020-5895-b923-adc862c3457e}".to_string(),
                main: false,
                wide: 1920,
                high: 1080,
                name: "Dell U2412M (HDMI)".to_string(),
            },
        ];
        let written = far_screens_written(&screens);
        assert_eq!(far_screens_read(&written), screens);
    }

    #[test]
    fn a_screen_line_this_version_cannot_read_costs_that_screen_and_not_the_list() {
        // Une version d'en face qui décrirait un écran autrement ne doit
        // pas vider le menu : on perd cet écran-là, pas les autres.
        let mixed = "{aaa} main 1920x1080 Un écran\nn'importe quoi\n{bbb} other 1280x720 Un autre";
        let read = far_screens_read(mixed);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].name, "Un écran");
        assert!(!read[1].main);
        // Et rien du tout se lit comme rien du tout.
        assert!(far_screens_read("").is_empty());
    }

    #[test]
    fn the_screen_a_session_asks_for_survives_being_written_and_read_back() {
        // Il traverse deux canaux, celui entre nos programmes et celui
        // entre les deux ordinateurs : une seule écriture, une seule
        // lecture.
        for wanted in [
            WantedScreen {
                wide: 1920,
                high: 1200,
                scale: 125,
            },
            WantedScreen {
                wide: 3840,
                high: 2160,
                scale: 0,
            },
        ] {
            let said = wanted.to_string();
            assert_eq!(said.parse::<WantedScreen>().unwrap(), wanted, "{said}");
        }
        // Une taille toute seule n'est pas cet écran-là : l'agrandissement
        // manquant se dirait zéro et rien ne distinguerait « laisse le
        // tien » d'un message tronqué.
        assert!("1920x1200".parse::<WantedScreen>().is_err());
        assert!("1920x1200@".parse::<WantedScreen>().is_err());
        assert!("1920x1200@beaucoup".parse::<WantedScreen>().is_err());
    }

    #[test]
    fn every_size_asked_for_can_be_cut_into_colour_by_halves() {
        // Un encodeur découpe la couleur par moitiés : une taille impaire
        // se fait arrondir quelque part où on ne le voit pas.
        for screen in [(1919, 1079), (3441, 1441), (1366, 768)] {
            let (wide, high) = Asked::Client.size(Some(screen));
            assert!(wide % 2 == 0 && high % 2 == 0, "{screen:?}");
        }
    }

    #[test]
    fn what_is_offered_can_be_written_and_read_back() {
        // Ces valeurs voyagent en texte sur le canal de contrôle et dans
        // le fichier de réglages : elles doivent se relire.
        for asked in SIZES_OFFERED {
            assert_eq!(asked.to_string().parse::<Asked>().unwrap(), *asked);
        }
        // « screen » est ce que les réglages déjà écrits sur les
        // machines disent pour l'écran du client : ils doivent continuer
        // de vouloir dire ça.
        assert_eq!("screen".parse::<Asked>().unwrap(), Asked::Client);
        assert_eq!("SCREEN".parse::<Asked>().unwrap(), Asked::Client);
        assert_eq!("host".parse::<Asked>().unwrap(), Asked::Host);
        assert!("n'importe quoi".parse::<Asked>().is_err());
    }

    #[test]
    fn a_menu_line_walks_its_list_and_comes_back_to_the_start() {
        assert_eq!(next_in(SIZES_OFFERED, Asked::Client), SIZES_OFFERED[1]);
        assert_eq!(
            next_in(SIZES_OFFERED, *SIZES_OFFERED.last().unwrap()),
            SIZES_OFFERED[0]
        );
        // Une valeur écrite par une version qui offrait autre chose ne
        // doit pas coincer la ligne : elle retombe sur la première.
        assert_eq!(
            next_in(SIZES_OFFERED, Asked::Fixed(640, 480)),
            SIZES_OFFERED[0]
        );
        assert_eq!(next_in(RATES_OFFERED, 20_000), 30_000);
        assert_eq!(next_in(CODECS_OFFERED, Codec::Av1), Codec::Auto);
    }

    #[test]
    fn the_rate_a_person_chose_is_the_rate_that_is_sent() {
        // Rien ne le recalcule à partir de la taille : ce que la personne
        // a choisi en regardant son image est ce qui part.
        let chosen = Preferred {
            asked: Asked::Client,
            bitrate_kbps: 15_000,
            ..Preferred::default()
        };
        assert_eq!(
            chosen.settings(Some(a_screen(3840, 2160))).bitrate_kbps,
            15_000
        );
        assert_eq!(chosen.settings(None).bitrate_kbps, 15_000);
    }

    #[test]
    fn the_session_asks_for_the_rate_of_the_screen_it_lands_on() {
        // Un écran à cent quarante-quatre reçoit cent quarante-quatre
        // images, pas soixante : deux rafraîchissements sur trois
        // montraient l'image précédente.
        let on = |refresh| {
            Preferred::default()
                .settings(Some(Screen {
                    wide: 1920,
                    high: 1080,
                    refresh,
                    scale: LIFE_SIZE,
                }))
                .fps
        };
        assert_eq!(on(60), 60);
        assert_eq!(on(144), 144);
        // Au-dessus du plafond, une part entière de la cadence de
        // l'écran et non le plafond : les images doivent continuer de
        // tomber une par rafraîchissement.
        assert_eq!(on(240), 120);
        assert_eq!(on(360), 120);
        assert_eq!(on(165), 82);
        // Sous le plancher, le plancher.
        assert_eq!(on(24), SLOWEST_RATE);
        // Zéro et un sont ce que Windows répond pour un écran dont il ne
        // tient pas la cadence : « pas mesuré » et non « très lent »,
        // donc le défaut et pas le plancher.
        assert_eq!(on(0), SessionSettings::default().fps);
        assert_eq!(on(1), SessionSettings::default().fps);
        // Écran non mesurable : ce que le produit demandait avant que
        // quoi que ce soit soit mesuré.
        assert_eq!(
            Preferred::default().settings(None).fps,
            SessionSettings::default().fps
        );
    }

    #[test]
    fn what_was_chosen_lands_in_the_session() {
        let preferred = Preferred {
            asked: Asked::Fixed(2560, 1440),
            bitrate_kbps: 40_000,
            codec: Codec::Av1,
            display_mode: DisplayMode::Windowed,
            absolute_mouse: false,
            stats_overlay: true,
            // Rien de ce champ n'atteint le moteur : il ne décrit pas
            // l'image, il dit ce qu'on demande à la machine d'en face.
            mute_far_speakers: true,
            system_keys: false,
            steady_far_rate: false,
        };
        let settings = preferred.settings(Some(a_screen(3840, 2160)));
        assert_eq!((settings.width, settings.height), (2560, 1440));
        assert_eq!(settings.bitrate_kbps, 40_000);
        assert_eq!(settings.codec, Codec::Av1);
        assert_eq!(settings.display_mode, DisplayMode::Windowed);
        assert!(!settings.absolute_mouse);
        assert!(settings.stats_overlay);
        // Le côté où l'interrupteur est laissé est celui où la session
        // suivante s'ouvre.
        assert!(!settings.system_keys);
        // La taille de paquet n'est pas un choix : le tunnel la décide.
        assert_eq!(settings.packet_size, None);
    }

    #[test]
    fn the_defaults_still_open_the_session_they_used_to() {
        // Personne n'a rien choisi et l'écran n'a pas pu être mesuré : ce
        // qui sort doit rester exactement ce que le produit faisait.
        assert_eq!(
            Preferred::default().settings(None),
            SessionSettings::default()
        );
    }

    #[test]
    fn defaults_are_1080p60_with_no_imposed_packet_size() {
        let settings = SessionSettings::default();
        assert_eq!(
            (settings.width, settings.height, settings.fps),
            (1920, 1080, 60)
        );
        assert_eq!(settings.packet_size, None);
    }
}
