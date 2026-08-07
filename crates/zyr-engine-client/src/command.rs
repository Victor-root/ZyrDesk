//! Building the client engine's command lines.
//!
//! Every option goes through the command line rather than the settings
//! file: rewriting that file while a session may be running would expose
//! us to concurrent writes.

use zyr_proto::session::SessionSettings;

/// Name of the application the host engine exposes.
///
/// The application list we generate for the host holds exactly one.
pub const APPLICATION: &str = "Desktop";

/// Arguments that pair with a host.
pub fn pairing_arguments(host: &str, pin: &str) -> Vec<String> {
    vec![
        "pair".to_string(),
        host.to_string(),
        "--pin".to_string(),
        pin.to_string(),
    ]
}

/// Arguments that start a session.
///
/// Hardware decoding is imposed: a silent fallback to software would
/// give a session that looks like it works while missing the whole
/// performance target. A visible failure is worth more.
pub fn session_arguments(host: &str, settings: &SessionSettings) -> Vec<String> {
    let mut args = vec![
        "stream".to_string(),
        host.to_string(),
        APPLICATION.to_string(),
        "--resolution".to_string(),
        format!("{}x{}", settings.width, settings.height),
        "--fps".to_string(),
        settings.fps.to_string(),
        "--bitrate".to_string(),
        settings.bitrate_kbps.to_string(),
        "--display-mode".to_string(),
        settings.display_mode.engine_value().to_string(),
        "--video-codec".to_string(),
        settings.codec.engine_value().to_string(),
        "--video-decoder".to_string(),
        "hardware".to_string(),
        "--frame-pacing".to_string(),
    ];
    if let Some(size) = settings.packet_size {
        args.push("--packet-size".to_string());
        args.push(size.to_string());
    }
    if settings.absolute_mouse {
        args.push("--absolute-mouse".to_string());
    }
    if settings.stats_overlay {
        args.push("--performance-overlay".to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::session::{Codec, DisplayMode};

    fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let index = args.iter().position(|a| a == flag)?;
        args.get(index + 1).map(String::as_str)
    }

    #[test]
    fn pairing_never_asks_a_question() {
        assert_eq!(
            pairing_arguments("127.0.0.1", "0421"),
            ["pair", "127.0.0.1", "--pin", "0421"]
        );
    }

    #[test]
    fn the_session_targets_the_host_s_only_application() {
        let args = session_arguments("192.168.1.10", &SessionSettings::default());
        assert_eq!(args[0], "stream");
        assert_eq!(args[1], "192.168.1.10");
        assert_eq!(args[2], APPLICATION);
    }

    #[test]
    fn the_settings_turn_into_options() {
        let settings = SessionSettings {
            width: 2560,
            height: 1440,
            fps: 120,
            bitrate_kbps: 80_000,
            codec: Codec::Hevc,
            display_mode: DisplayMode::Borderless,
            ..SessionSettings::default()
        };
        let args = session_arguments("host", &settings);
        assert_eq!(value_of(&args, "--resolution"), Some("2560x1440"));
        assert_eq!(value_of(&args, "--fps"), Some("120"));
        assert_eq!(value_of(&args, "--bitrate"), Some("80000"));
        assert_eq!(value_of(&args, "--video-codec"), Some("HEVC"));
        assert_eq!(value_of(&args, "--display-mode"), Some("borderless"));
    }

    #[test]
    fn hardware_decoding_and_frame_pacing_are_always_imposed() {
        let args = session_arguments("host", &SessionSettings::default());
        assert_eq!(value_of(&args, "--video-decoder"), Some("hardware"));
        assert!(args.iter().any(|a| a == "--frame-pacing"));
    }

    #[test]
    fn the_packet_size_stays_the_engine_s_business_until_imposed() {
        let args = session_arguments("host", &SessionSettings::default());
        assert!(!args.iter().any(|a| a == "--packet-size"));

        let imposed = SessionSettings {
            packet_size: Some(1264),
            ..SessionSettings::default()
        };
        let args = session_arguments("host", &imposed);
        assert_eq!(value_of(&args, "--packet-size"), Some("1264"));
    }

    #[test]
    fn the_optional_flags_follow_the_settings() {
        let without = SessionSettings {
            absolute_mouse: false,
            stats_overlay: false,
            ..SessionSettings::default()
        };
        let args = session_arguments("host", &without);
        assert!(!args.iter().any(|a| a == "--absolute-mouse"));
        assert!(!args.iter().any(|a| a == "--performance-overlay"));

        let with = SessionSettings {
            absolute_mouse: true,
            stats_overlay: true,
            ..SessionSettings::default()
        };
        let args = session_arguments("host", &with);
        assert!(args.iter().any(|a| a == "--absolute-mouse"));
        assert!(args.iter().any(|a| a == "--performance-overlay"));
    }
}
