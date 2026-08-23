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

/// Arguments that tell a host to close what it is showing.
///
/// Leaving a session and closing it are two different things: the host
/// keeps the desktop it opened, ready for an immediate return, until it
/// is told otherwise. This is how it is told (P-M7).
pub fn quit_arguments(host: &str) -> Vec<String> {
    vec!["quit".to_string(), host.to_string()]
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
        // Always windowed: the picture is shown inside the product's
        // own window, which puts the engine's window over it and takes
        // its frame away. Whether that window covers the screen is a
        // question for the window, not for the engine.
        "--display-mode".to_string(),
        "windowed".to_string(),
        "--video-codec".to_string(),
        settings.codec.engine_value().to_string(),
        "--video-decoder".to_string(),
        "hardware".to_string(),
        "--frame-pacing".to_string(),
        // Alt+Tab and its kin are taken by the product itself and carried
        // to the far computer, and the engine is asked for none of it.
        //
        // It was asked, and the option is real, and it cannot work here.
        // The engine takes those keys by putting itself in front of every
        // keystroke of the whole computer, and it only acts on them while
        // it believes its own window is the one the system calls the
        // front. That window is carried inside the product's own for the
        // length of a session, which makes it a child window, and the
        // system gives the front to the head of a family and never to a
        // member of it: it stops believing it at the first message that
        // asks, seconds in.
        //
        // What it does do in those few seconds is swallow Alt and Control
        // outright, both of them, before anything else on this computer
        // sees them. Every shortcut of the product's own is an Alt
        // combination, so for as long as the engine holds those keys, none
        // of them work, and the person cannot leave full screen from the
        // keyboard.
        //
        // Asked for, then, it costs the product's own shortcuts and gives
        // a few seconds of what the product now does for a whole session.
        // What tells the far computer it may put its desktop at the size
        // being asked for. Its name comes from playing games, where it
        // meant letting the far machine choose its own settings; with the
        // host engine it means nothing else than this, and without it the
        // far desktop keeps whatever shape it had and the black bars that
        // fitting it into the stream costs are burned into every frame.
        //
        // Said out loud rather than left to the engine's own default: the
        // engine keeps that answer in a settings file of its own, where a
        // single stray click in a screen the product never shows would
        // take it back.
        "--game-optimization".to_string(),
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
            display_mode: DisplayMode::Fullscreen,
            ..SessionSettings::default()
        };
        let args = session_arguments("host", &settings);
        assert_eq!(value_of(&args, "--resolution"), Some("2560x1440"));
        assert_eq!(value_of(&args, "--fps"), Some("120"));
        assert_eq!(value_of(&args, "--bitrate"), Some("80000"));
        assert_eq!(value_of(&args, "--video-codec"), Some("HEVC"));
        assert_eq!(value_of(&args, "--display-mode"), Some("windowed"));
    }

    #[test]
    fn hardware_decoding_and_frame_pacing_are_always_imposed() {
        let args = session_arguments("host", &SessionSettings::default());
        assert_eq!(value_of(&args, "--video-decoder"), Some("hardware"));
        assert!(args.iter().any(|a| a == "--frame-pacing"));
    }

    #[test]
    fn the_engine_is_never_asked_to_take_the_system_s_keys() {
        // Alt+Tab et compagnie sont repris par le produit lui-même. Le
        // moteur, lui, avale Alt et Ctrl en entier tant qu'il les tient,
        // ce qui coupe tous les raccourcis du produit, qui sont tous des
        // combinaisons Alt.
        let args = session_arguments("host", &SessionSettings::default());
        assert!(!args.iter().any(|a| a == "--capture-system-keys"));
    }

    #[test]
    fn the_far_desktop_is_always_asked_to_take_the_shape_of_the_session() {
        // C'est ce drapeau, et lui seul, qui autorise l'ordinateur d'en
        // face à mettre son bureau à la taille demandée. Sans lui il
        // garde la sienne, et l'écart entre les deux formes est gravé en
        // bandes noires dans chaque image envoyée.
        let args = session_arguments("host", &SessionSettings::default());
        assert!(args.iter().any(|a| a == "--game-optimization"));
        assert!(!args.iter().any(|a| a == "--no-game-optimization"));
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
