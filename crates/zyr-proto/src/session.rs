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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Fullscreen,
    Borderless,
    Windowed,
}

impl DisplayMode {
    pub fn engine_value(self) -> &'static str {
        match self {
            DisplayMode::Fullscreen => "fullscreen",
            DisplayMode::Borderless => "borderless",
            DisplayMode::Windowed => "windowed",
        }
    }
}

impl fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.engine_value())
    }
}

impl std::str::FromStr for DisplayMode {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().as_str() {
            "fullscreen" | "plein-ecran" => Ok(DisplayMode::Fullscreen),
            "borderless" | "sans-bordure" => Ok(DisplayMode::Borderless),
            "windowed" | "fenetre" => Ok(DisplayMode::Windowed),
            _ => Err(format!("mode d'affichage inconnu : {text}")),
        }
    }
}

/// Settings of one session.
///
/// The defaults mirror the client engine's own for 1080p60, which the
/// comparison against unmanaged engines required at milestone M1.
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
            display_mode: DisplayMode::Fullscreen,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Kinder to the network: made for Wi-Fi and busy links.
    Smooth,
    #[default]
    Balanced,
    /// For a wired network with room to spare.
    Detailed,
}

impl Quality {
    pub fn size(self) -> (u32, u32) {
        match self {
            Quality::Smooth => (1280, 720),
            Quality::Balanced => (1920, 1080),
            Quality::Detailed => (2560, 1440),
        }
    }

    pub fn bitrate_kbps(self) -> u32 {
        match self {
            Quality::Smooth => 10_000,
            Quality::Balanced => 20_000,
            Quality::Detailed => 40_000,
        }
    }
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
    /// The settings a session opens with.
    pub fn settings(self) -> SessionSettings {
        let (width, height) = self.quality.size();
        SessionSettings {
            width,
            height,
            bitrate_kbps: self.quality.bitrate_kbps(),
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
        for mode in [
            DisplayMode::Fullscreen,
            DisplayMode::Borderless,
            DisplayMode::Windowed,
        ] {
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
            assert!(small.size().0 < large.size().0, "{small} vs {large}");
            assert!(
                small.bitrate_kbps() < large.bitrate_kbps(),
                "{small} vs {large}"
            );
        }
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
        let settings = preferred.settings();
        assert_eq!((settings.width, settings.height), (2560, 1440));
        assert_eq!(settings.bitrate_kbps, 40_000);
        assert_eq!(settings.codec, Codec::Av1);
        assert_eq!(settings.display_mode, DisplayMode::Windowed);
        assert!(!settings.absolute_mouse);
        assert!(settings.stats_overlay);
        // La taille de paquet n'est pas un choix : le tunnel la décide.
        assert_eq!(settings.packet_size, None);
    }

    #[test]
    fn the_defaults_are_what_a_session_opened_with_before() {
        // Personne n'a encore rien choisi : ce qui sort doit être
        // exactement ce que le produit faisait jusqu'ici.
        assert_eq!(Preferred::default().settings(), SessionSettings::default());
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
