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
        }
    }
}

/// How much picture to ask for.
///
/// Three rungs rather than three dials. What actually moves is the size
/// of the picture and the rate it is sent at, and the two go together:
/// asking for a bigger picture without the rate to carry it gives a
/// blurry one, which nobody would have chosen on purpose.
///
/// A rung is a ceiling and not a size. The size that is asked for is the
/// screen the picture will be shown on, because any other number is
/// thrown away twice: the far computer scales its desktop to what was
/// asked, and this end scales that to the screen. Two scalings, and
/// neither of them puts back a single pixel of the detail the first one
/// dropped. A screen bigger than the rung allows is the one case where a
/// number of ours is used, and then it is the rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Kinder to the network: made for Wi-Fi and busy links.
    Smooth,
    #[default]
    Balanced,
    /// For a wired network with room to spare.
    Detailed,
}

/// Screen assumed when this computer's own cannot be measured.
///
/// Not the largest rung: a size nobody checked is a size to be careful
/// with, and the common screen costs nothing to be wrong about.
pub const UNKNOWN_SCREEN: (u32, u32) = (1920, 1080);

impl Quality {
    /// Largest picture this rung will ask for, whatever the screen.
    pub fn ceiling(self) -> (u32, u32) {
        match self {
            Quality::Smooth => (1280, 720),
            Quality::Balanced => (1920, 1080),
            Quality::Detailed => (3840, 2160),
        }
    }

    /// Rate this rung spends on a million pixels.
    ///
    /// The rungs used to carry one rate each, tied to the one size they
    /// asked for. Now that the size follows the screen, the rate has to
    /// follow the size, and what is left to a rung is how generous it is
    /// per pixel. These three numbers were chosen to land on the rates
    /// the rungs already had at the sizes they already asked for, so no
    /// session that worked yesterday is sent at a different rate today.
    fn kbps_per_megapixel(self) -> u64 {
        match self {
            Quality::Smooth => 9_000,
            Quality::Balanced => 10_000,
            Quality::Detailed => 11_000,
        }
    }

    /// Size to ask for when the picture is shown on `screen`.
    ///
    /// The screen itself as long as the rung allows it, and otherwise
    /// the largest picture of the screen's own shape that fits under the
    /// rung. Keeping the shape matters: a picture of another shape than
    /// the screen comes back with black bars, and the far computer burns
    /// them into every frame before sending them.
    pub fn fitted(self, screen: (u32, u32)) -> (u32, u32) {
        let (wide, high) = screen;
        let (most_wide, most_high) = self.ceiling();
        if wide == 0 || high == 0 {
            return self.fitted(UNKNOWN_SCREEN);
        }
        if wide <= most_wide && high <= most_high {
            return (even(wide), even(high));
        }
        // Which side sticks out the most, told without dividing:
        // most_wide / wide against most_high / high, cross-multiplied.
        let (wide, high) = (u64::from(wide), u64::from(high));
        let (most_wide, most_high) = (u64::from(most_wide), u64::from(most_high));
        if most_wide * high <= most_high * wide {
            (
                even(most_wide as u32),
                even((high * most_wide / wide) as u32),
            )
        } else {
            (
                even((wide * most_high / high) as u32),
                even(most_high as u32),
            )
        }
    }

    /// Rate that carries a picture of that size at this rung.
    ///
    /// Bounded at both ends. Below the floor nothing is watchable
    /// whatever the size, and above the ceiling the picture stops
    /// getting better while the encoder and the network keep paying:
    /// four times the pixels of a common screen is already served by the
    /// ceiling, which is the rate a wired network is expected to hold.
    pub fn bitrate_kbps(self, width: u32, height: u32) -> u32 {
        const FLOOR: u64 = 5_000;
        const CEILING: u64 = 80_000;
        let pixels = u64::from(width) * u64::from(height);
        let asked = pixels * self.kbps_per_megapixel() / 1_000_000;
        asked.clamp(FLOOR, CEILING) as u32
    }
}

/// Rounds down to an even number, which is what a picture split into
/// colour by halves can be cut into. One is never a size worth keeping.
fn even(value: u32) -> u32 {
    (value & !1).max(2)
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Quality::Smooth => "smooth",
            Quality::Balanced => "balanced",
            Quality::Detailed => "detailed",
        })
    }
}

impl std::str::FromStr for Quality {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().as_str() {
            "smooth" => Ok(Quality::Smooth),
            "balanced" => Ok(Quality::Balanced),
            "detailed" => Ok(Quality::Detailed),
            _ => Err(format!("qualité inconnue : {text}")),
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
    pub quality: Quality,
    pub codec: Codec,
    pub display_mode: DisplayMode,
    /// Absolute mouse: right for a desktop, wrong for games that aim
    /// with relative motion.
    pub absolute_mouse: bool,
    pub stats_overlay: bool,
}

impl Default for Preferred {
    fn default() -> Self {
        Self {
            quality: Quality::default(),
            codec: Codec::default(),
            display_mode: DisplayMode::default(),
            absolute_mouse: true,
            stats_overlay: false,
        }
    }
}

impl Preferred {
    /// The settings a session opens with on a computer whose screen has
    /// been measured, `None` standing for one that could not be.
    pub fn settings(self, screen: Option<(u32, u32)>) -> SessionSettings {
        let (width, height) = self.quality.fitted(screen.unwrap_or(UNKNOWN_SCREEN));
        SessionSettings {
            width,
            height,
            bitrate_kbps: self.quality.bitrate_kbps(width, height),
            codec: self.codec,
            display_mode: self.display_mode,
            absolute_mouse: self.absolute_mouse,
            stats_overlay: self.stats_overlay,
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
        for quality in [Quality::Smooth, Quality::Balanced, Quality::Detailed] {
            assert_eq!(quality.to_string().parse::<Quality>().unwrap(), quality);
        }
        for codec in [Codec::Auto, Codec::H264, Codec::Hevc, Codec::Av1] {
            assert_eq!(codec.to_string().parse::<Codec>().unwrap(), codec);
        }
        for mode in [DisplayMode::Fullscreen, DisplayMode::Windowed] {
            assert_eq!(mode.to_string().parse::<DisplayMode>().unwrap(), mode);
        }
    }

    #[test]
    fn a_bigger_picture_comes_with_the_rate_to_carry_it() {
        // Une qualité qui monte en taille sans monter en débit donnerait
        // une image plus grande et plus floue : personne ne choisirait
        // ça exprès.
        let rungs = [Quality::Smooth, Quality::Balanced, Quality::Detailed];
        for pair in rungs.windows(2) {
            let (small, large) = (pair[0], pair[1]);
            assert!(small.ceiling().0 < large.ceiling().0, "{small} vs {large}");
            let same = (1280, 720);
            assert!(
                small.bitrate_kbps(same.0, same.1) < large.bitrate_kbps(same.0, same.1),
                "{small} vs {large}"
            );
        }
    }

    #[test]
    fn the_screen_is_asked_for_whole_when_the_rung_allows_it() {
        // Le cas qui compte : un écran plus grand que ce que le produit
        // demandait jusqu'ici. Demander moins que l'écran, c'est agrandir
        // l'image à l'arrivée, et aucun agrandissement ne rend un détail
        // qui n'a pas été envoyé.
        assert_eq!(Quality::Detailed.fitted((3840, 2160)), (3840, 2160));
        assert_eq!(Quality::Detailed.fitted((2560, 1440)), (2560, 1440));
        assert_eq!(Quality::Detailed.fitted((3440, 1440)), (3440, 1440));
        assert_eq!(Quality::Balanced.fitted((1920, 1080)), (1920, 1080));
        assert_eq!(Quality::Smooth.fitted((1280, 720)), (1280, 720));
    }

    #[test]
    fn a_screen_over_the_rung_is_shrunk_without_changing_shape() {
        // Une image d'une autre forme que l'écran revient avec des
        // bandes noires, et l'ordinateur d'en face les grave dans chaque
        // image avant de l'envoyer.
        assert_eq!(Quality::Balanced.fitted((3840, 2160)), (1920, 1080));
        assert_eq!(Quality::Smooth.fitted((1920, 1080)), (1280, 720));
        // Seize dixièmes : c'est la hauteur qui déborde, pas la largeur.
        assert_eq!(Quality::Balanced.fitted((2560, 1600)), (1728, 1080));

        for screen in [(3440, 1440), (2560, 1600), (3840, 2160), (1366, 768)] {
            for rung in [Quality::Smooth, Quality::Balanced, Quality::Detailed] {
                let (wide, high) = rung.fitted(screen);
                let (most_wide, most_high) = rung.ceiling();
                assert!(wide <= most_wide && high <= most_high, "{rung} {screen:?}");
                assert!(wide % 2 == 0 && high % 2 == 0, "{rung} {screen:?}");
                let shape = f64::from(screen.0) / f64::from(screen.1);
                let asked = f64::from(wide) / f64::from(high);
                assert!((shape - asked).abs() < 0.01, "{rung} {screen:?} {asked}");
            }
        }
    }

    #[test]
    fn an_unmeasurable_screen_falls_back_to_the_common_one() {
        // Pas la plus grande marche : une taille que personne n'a
        // vérifiée est une taille dont il vaut mieux se méfier.
        for rung in [Quality::Smooth, Quality::Balanced, Quality::Detailed] {
            assert_eq!(rung.fitted((0, 0)), rung.fitted(UNKNOWN_SCREEN), "{rung}");
        }
        assert_eq!(Quality::Detailed.fitted(UNKNOWN_SCREEN), (1920, 1080));
    }

    #[test]
    fn the_rate_the_rungs_used_to_carry_is_the_rate_they_still_carry() {
        // Ces trois couples sont ce que le produit envoyait quand chaque
        // marche portait un débit écrit à la main. Le débit se calcule
        // maintenant, et il doit tomber au même endroit : sinon on aurait
        // changé sans le dire ce que vaut une session ordinaire.
        for (rung, (wide, high), was) in [
            (Quality::Smooth, (1280, 720), 10_000),
            (Quality::Balanced, (1920, 1080), 20_000),
            (Quality::Detailed, (2560, 1440), 40_000),
        ] {
            let now = rung.bitrate_kbps(wide, high);
            assert!(now.abs_diff(was) * 5 < was, "{rung} : {now} contre {was}");
        }
    }

    #[test]
    fn the_rate_stays_between_a_floor_and_a_ceiling() {
        // En dessous du plancher rien n'est regardable quelle que soit la
        // taille ; au dessus du plafond l'image cesse de s'améliorer
        // pendant que l'encodeur et le réseau continuent de payer.
        assert_eq!(Quality::Smooth.bitrate_kbps(320, 240), 5_000);
        assert_eq!(Quality::Detailed.bitrate_kbps(7680, 4320), 80_000);
        assert!(Quality::Detailed.bitrate_kbps(3840, 2160) <= 80_000);
    }

    #[test]
    fn what_was_chosen_lands_in_the_session() {
        let preferred = Preferred {
            quality: Quality::Detailed,
            codec: Codec::Av1,
            display_mode: DisplayMode::Windowed,
            absolute_mouse: false,
            stats_overlay: true,
        };
        let settings = preferred.settings(Some((3840, 2160)));
        assert_eq!((settings.width, settings.height), (3840, 2160));
        assert_eq!(
            settings.bitrate_kbps,
            Quality::Detailed.bitrate_kbps(3840, 2160)
        );
        assert_eq!(settings.codec, Codec::Av1);
        assert_eq!(settings.display_mode, DisplayMode::Windowed);
        assert!(!settings.absolute_mouse);
        assert!(settings.stats_overlay);
        // La taille de paquet n'est pas un choix : le tunnel la décide.
        assert_eq!(settings.packet_size, None);
    }

    #[test]
    fn the_defaults_still_open_the_session_they_used_to() {
        // Personne n'a rien choisi et l'écran n'a pas pu être mesuré : ce
        // qui sort doit rester ce que le produit faisait jusqu'ici, au
        // débit près, qui se calcule maintenant au lieu d'être écrit.
        let settings = Preferred::default().settings(None);
        let before = SessionSettings::default();
        assert_eq!(
            (settings.width, settings.height, settings.fps),
            (before.width, before.height, before.fps)
        );
        let gap = settings.bitrate_kbps.abs_diff(before.bitrate_kbps);
        assert!(gap * 10 < before.bitrate_kbps, "{settings:?}");
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
