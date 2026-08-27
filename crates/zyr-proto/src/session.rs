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

/// Size of picture to ask the far computer for.
///
/// The screen by default, and that word rather than a number on purpose:
/// a picture the size of the screen it lands on is shown pixel for
/// pixel, and any other size is thrown away twice. The far computer
/// scales its desktop to what was asked, this end scales that to the
/// screen, and neither scaling puts back a single pixel of the detail
/// the first one dropped.
///
/// A number is still offered, because the screen is not always the right
/// answer. A large screen served over a busy link costs more rate than
/// the link has; asking for less picture is the honest way to spend
/// less, and it is a choice, made once, that this remembers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Asked {
    /// Whatever the screen showing the session turns out to be.
    #[default]
    Screen,
    /// This size, whatever the screen.
    Fixed(u32, u32),
}

/// Screen assumed when this computer's own cannot be measured.
///
/// The common screen, which costs nothing to be wrong about.
pub const UNKNOWN_SCREEN: (u32, u32) = (1920, 1080);

/// The sizes offered, in the order they are offered in.
///
/// Here and not in the window: a second copy of this list written in
/// JavaScript would drift from this one the day a size is added.
pub const SIZES_OFFERED: &[Asked] = &[
    Asked::Screen,
    Asked::Fixed(3840, 2160),
    Asked::Fixed(2560, 1440),
    Asked::Fixed(1920, 1080),
    Asked::Fixed(1280, 720),
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
    pub fn size(self, screen: Option<(u32, u32)>) -> (u32, u32) {
        let (wide, high) = match self {
            Asked::Screen => screen.unwrap_or(UNKNOWN_SCREEN),
            Asked::Fixed(wide, high) => (wide, high),
        };
        if wide == 0 || high == 0 {
            return UNKNOWN_SCREEN;
        }
        (even(wide), even(high))
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
            Asked::Screen => f.write_str("screen"),
            Asked::Fixed(wide, high) => write!(f, "{wide}x{high}"),
        }
    }
}

impl std::str::FromStr for Asked {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("screen") {
            return Ok(Asked::Screen);
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
    pub fn settings(self, screen: Option<(u32, u32)>) -> SessionSettings {
        let (width, height) = self.asked.size(screen);
        SessionSettings {
            width,
            height,
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
        assert_eq!(Asked::Screen.size(Some((3840, 2160))), (3840, 2160));
        assert_eq!(Asked::Screen.size(Some((3440, 1440))), (3440, 1440));
        // Et un nombre reste un nombre, quel que soit l'écran.
        assert_eq!(
            Asked::Fixed(1920, 1080).size(Some((3840, 2160))),
            (1920, 1080)
        );
    }

    #[test]
    fn an_unmeasurable_screen_falls_back_to_the_common_one() {
        assert_eq!(Asked::Screen.size(None), UNKNOWN_SCREEN);
        assert_eq!(Asked::Screen.size(Some((0, 0))), UNKNOWN_SCREEN);
        assert_eq!(Asked::Fixed(0, 1080).size(None), UNKNOWN_SCREEN);
    }

    #[test]
    fn every_size_asked_for_can_be_cut_into_colour_by_halves() {
        // Un encodeur découpe la couleur par moitiés : une taille impaire
        // se fait arrondir quelque part où on ne le voit pas.
        for screen in [(1919, 1079), (3441, 1441), (1366, 768)] {
            let (wide, high) = Asked::Screen.size(Some(screen));
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
        assert_eq!("screen".parse::<Asked>().unwrap(), Asked::Screen);
        assert_eq!("SCREEN".parse::<Asked>().unwrap(), Asked::Screen);
        assert!("n'importe quoi".parse::<Asked>().is_err());
    }

    #[test]
    fn a_menu_line_walks_its_list_and_comes_back_to_the_start() {
        assert_eq!(next_in(SIZES_OFFERED, Asked::Screen), SIZES_OFFERED[1]);
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
            asked: Asked::Screen,
            bitrate_kbps: 15_000,
            ..Preferred::default()
        };
        assert_eq!(chosen.settings(Some((3840, 2160))).bitrate_kbps, 15_000);
        assert_eq!(chosen.settings(None).bitrate_kbps, 15_000);
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
        let settings = preferred.settings(Some((3840, 2160)));
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
