//! Opens a session on a remote computer.
//!
//! Nothing is held here, and almost nothing is decided here either: the
//! opening of a session lives in `zyr-session`, which the interface uses
//! word for word. What belongs to this command is reading the options,
//! saying out loud what is happening, and waiting for the end, since a
//! command that hands back before the session is over would look like it
//! had failed.
//!
//! A direct mode without the tunnel is kept for diagnosis, to tell a
//! tunnel problem from an engine problem in minutes. It never appears in
//! the interface.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use zyr_engine_client::SessionOutcome;
use zyr_proto::session::{Codec, DisplayMode, SessionSettings, parse_resolution};
use zyr_session::{Step, Wanted};
use zyr_transport::Fingerprint;

use crate::failure;

#[derive(ClapArgs)]
pub struct Args {
    /// Address of the remote computer
    host: String,

    /// Fingerprint of the remote computer, shown there by "zyr-cli identity"
    #[arg(long, value_name = "FINGERPRINT", required_unless_present = "direct")]
    pair: Option<Fingerprint>,

    /// Goes straight to the remote engine, without the tunnel and
    /// without the service. For diagnosis only: the address then carries
    /// the engine's port.
    #[arg(long, conflicts_with = "pair")]
    direct: bool,

    /// Requested resolution, for example 1920x1080
    #[arg(long, default_value = "1920x1080")]
    resolution: String,

    /// Frames per second
    #[arg(long, default_value_t = 60)]
    fps: u32,

    /// Video rate in kilobits per second
    #[arg(long, default_value_t = 20_000)]
    bitrate: u32,

    /// Video codec: auto, h264, hevc or av1
    #[arg(long, default_value = "auto")]
    codec: String,

    /// Display: fullscreen or windowed
    #[arg(long, default_value = "fullscreen")]
    display: String,

    /// Shows the performance statistics over the video
    #[arg(long)]
    stats: bool,

    /// Relative mouse, suited to games, instead of the desktop mouse
    #[arg(long)]
    relative_mouse: bool,

    /// Pairs again even if this computer is already known
    #[arg(long)]
    pair_again: bool,
}

pub fn run(args: Args) -> ExitCode {
    let settings = match build_settings(&args) {
        Ok(settings) => settings,
        Err(message) => return failure("réglages de session invalides", message),
    };

    let wanted = Wanted {
        host: args.host.clone(),
        peer: args.pair,
        settings,
        pair_again: args.pair_again,
        // The command line is the diagnostic path: it asks nothing of
        // the far computer's speakers, nor of the rate it serves a still
        // screen at. Both are choices made in the window, along with
        // everything else a session looks like, and both leave that
        // computer exactly as its own settings had it.
        hush_the_far_speakers: false,
        steady_far_rate: zyr_proto::session::Serving::default().steady_rate,
        // The command line is there for diagnosis: it asks for a size
        // and so for the screen needed to carry it. But it measures no
        // screen, so it has no magnification to ask for: the far
        // computer keeps its own.
        wants_a_screen_over_there: true,
        far_magnification: 0,
        // And the main screen of the far machine, which is what every
        // session asks for as long as nobody has said otherwise.
        // Choosing between several screens is done by looking at them,
        // so in the window, and never here.
        far_screen: None,
        // The command line reaches the named computer by the best way
        // available. Doing without the server is a choice made on seeing
        // that the wanted machine is in the next room, so on its card,
        // in the window.
        only_here: false,
    };

    // Nothing here can close a session while it is opening: the command
    // line waits for the opening to finish before it listens to anybody.
    let running = match zyr_session::open(&wanted, &mut |step| tell(step, &args.host), &|| true) {
        Ok(running) => running,
        Err(e) => return reported(e, &args.host),
    };

    let log = running.log().to_path_buf();
    match running.wait() {
        Ok(SessionOutcome::Ended) => {
            println!("Session terminée.");
            println!("  Journal : {}", log.display());
            ExitCode::SUCCESS
        }
        Ok(SessionOutcome::Failed) => failure(
            "la session s'est arrêtée sur une erreur",
            format!("Journal : {}", log.display()),
        ),
        Ok(SessionOutcome::Unreachable) => failure(
            "l'ordinateur distant n'a pas répondu",
            format!("Journal : {}", log.display()),
        ),
        Ok(SessionOutcome::NotPaired) => failure(
            "l'ordinateur distant ne reconnaît plus celui-ci",
            format!("Journal : {}", log.display()),
        ),
        Ok(SessionOutcome::Unknown { code }) => {
            let code = code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "interrompu".to_string());
            failure(
                "le moteur s'est arrêté sans dire pourquoi",
                format!("code {code}\n  Journal : {}", log.display()),
            )
        }
        Err(e) => failure("surveillance de la session", e),
    }
}

/// Says what is happening, in the order it happens.
fn tell(step: Step, host: &str) {
    match step {
        Step::Reached { packet } => {
            println!("Tunnel établi avec {host}.");
            println!("  Taille de paquet : {packet} octets.");
        }
        Step::Pairing { again: None } => {
            println!("Premier accès à cet ordinateur : présentation en cours...");
        }
        Step::Pairing {
            again: Some(stopped),
        } => {
            println!("Cet ordinateur ne nous reconnaît plus : nouvelle présentation...");
            println!("  Le lecteur s'est arrêté sur {stopped:?}.");
        }
        Step::PairingNeeded { pin } => {
            println!("Premier accès à cet ordinateur, sans tunnel pour porter le code.\n");
            println!("  Sur {host}, lancez maintenant :");
            println!("\n      zyr-cli host pin {pin}\n");
            println!("  En attente de l'autorisation...");
        }
        Step::Paired => println!("  Les deux ordinateurs se connaissent.\n"),
        Step::NoSoundCardHere => {
            println!("  Cet ordinateur n'a pas de sortie audio : la session sera muette.");
        }
        Step::Starting => println!("Connexion à {host}..."),
        // Nothing to say about it here: the command line has
        // no floating button to hang on it.
        Step::Showing { .. } => {}
        Step::SpeakersLeftAlone { refused } => {
            println!("  Les enceintes de {host} restent allumées : {refused}");
        }
        Step::FarPointerLeftAlone { refused } => {
            println!("  Le curseur de {host} n'a pas été réglé : {refused}");
        }
        Step::RateLeftAlone { refused } => {
            println!("  {host} garde sa cadence d'écran immobile : {refused}");
        }
        Step::ScreenLeftAlone { refused } => {
            println!("  {host} n'a pas réveillé son écran virtuel : {refused}");
        }
        Step::ScreenOverThere { wide, high } => {
            println!("  {host} affiche {wide}x{high}, c'est ce qui est demandé au lecteur");
        }
        Step::FarScreenChanging => {
            println!("  {host} change d'écran, son moteur redémarre...");
        }
        Step::FarRateChanging => {
            println!("  {host} change sa cadence d'écran immobile, son moteur redémarre...");
        }
        Step::FarScreenLeftAlone { refused } => {
            println!("  {host} garde l'écran qu'il filme : {refused}");
        }
    }
}

/// Turns a failure into the message and the exit code that go with it.
fn reported(e: zyr_session::Error, host: &str) -> ExitCode {
    use zyr_session::Error;
    match e {
        Error::EngineMissing(path) => failure(
            "moteur client introuvable",
            format!(
                "{}\n  Lancez « zyr-cli engines status » pour la marche à suivre.",
                path.display()
            ),
        ),
        Error::Service(reason) => failure("ouverture du tunnel", reason),
        Error::Pairing(reason) => pairing_failed(reason, host),
        Error::Handover(reason) => pairing_failed(reason, host),
        other => failure("ouverture de la session", other),
    }
}

fn pairing_failed(reason: impl std::fmt::Display, host: &str) -> ExitCode {
    failure(
        "appairage",
        format!("{reason}\n  Vérifiez que l'accès distant est actif sur {host}."),
    )
}

fn build_settings(args: &Args) -> Result<SessionSettings, String> {
    let (width, height) = parse_resolution(&args.resolution).map_err(|e| e.to_string())?;
    let codec: Codec = args.codec.parse()?;
    let display_mode: DisplayMode = args.display.parse()?;
    if args.fps == 0 {
        return Err("le nombre d'images par seconde doit être supérieur à zéro".to_string());
    }
    if args.bitrate == 0 {
        return Err("le débit vidéo doit être supérieur à zéro".to_string());
    }
    Ok(SessionSettings {
        width,
        height,
        fps: args.fps,
        bitrate_kbps: args.bitrate,
        codec,
        display_mode,
        // Decided by the service once the path is known, left to the
        // engine when going direct.
        packet_size: None,
        absolute_mouse: !args.relative_mouse,
        stats_overlay: args.stats,
        // Alt+Tab and the Windows key go into the session, as under the
        // interface. Nothing here can switch them back: the menu that
        // does it is the one of the floating button, which this command
        // does not open.
        system_keys: true,
    })
}
