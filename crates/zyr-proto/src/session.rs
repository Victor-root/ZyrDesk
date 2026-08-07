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
    fn defaults_are_1080p60_with_no_imposed_packet_size() {
        let settings = SessionSettings::default();
        assert_eq!(
            (settings.width, settings.height, settings.fps),
            (1920, 1080, 60)
        );
        assert_eq!(settings.packet_size, None);
    }
}
