//! Opens a session on a remote computer.
//!
//! At this stage there is neither a tunnel nor a rendezvous server: the
//! session goes straight to an address on the local network, and the
//! pairing code has to be carried over to the other computer by hand.
//! The tunnel automates that exchange later on.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use zyr_engine_client::state::identifier_from_address;
use zyr_engine_client::{ClientEngine, DeviceState, SessionOutcome};
use zyr_proto::paths;
use zyr_proto::random;
use zyr_proto::session::{Codec, DisplayMode, SessionSettings, parse_resolution};

use crate::failure;

#[derive(ClapArgs)]
pub struct Args {
    /// Adresse de l'ordinateur distant sur le réseau local
    hote: String,

    /// Résolution demandée, par exemple 1920x1080
    #[arg(long, default_value = "1920x1080")]
    resolution: String,

    /// Images par seconde
    #[arg(long, default_value_t = 60)]
    fps: u32,

    /// Débit vidéo en kilobits par seconde
    #[arg(long, default_value_t = 20_000)]
    bitrate: u32,

    /// Codec vidéo : auto, h264, hevc ou av1
    #[arg(long, default_value = "auto")]
    codec: String,

    /// Affichage : fullscreen, borderless ou windowed
    #[arg(long, default_value = "fullscreen")]
    affichage: String,

    /// Affiche les statistiques de performance par-dessus la vidéo
    #[arg(long)]
    stats: bool,

    /// Souris relative, adaptée aux jeux, au lieu de la souris de bureau
    #[arg(long)]
    souris_relative: bool,

    /// Refait l'appairage même si cet ordinateur est déjà connu
    #[arg(long)]
    reappairer: bool,
}

pub fn run(args: Args) -> ExitCode {
    let settings = match build_settings(&args) {
        Ok(settings) => settings,
        Err(message) => return failure("réglages de session invalides", message),
    };

    let exe = paths::client_engine_exe();
    if !exe.is_file() {
        return failure(
            "moteur client introuvable",
            format!(
                "{}\n  Lancez « zyr-cli engines status » pour la marche à suivre.",
                exe.display()
            ),
        );
    }

    let state = DeviceState::for_device(&identifier_from_address(&args.hote));
    if args.reappairer
        && let Err(e) = state.forget()
    {
        return failure("réinitialisation de l'appairage", e);
    }

    let already_known = state.has_a_paired_host();
    let log = paths::logs_dir().join("session.log");
    let engine = ClientEngine::new(&exe, state).with_log(&log);

    if !already_known && let Err(code) = pair(&engine, &args.hote) {
        return code;
    }

    println!(
        "Connexion à {} en {}x{} à {} images par seconde...",
        args.hote, settings.width, settings.height, settings.fps
    );
    match engine.start_session(&args.hote, &settings) {
        Ok(SessionOutcome::Ended) => {
            // The engine reports success even when it gave up: the log
            // stays the only reliable source while its exit codes are
            // undifferentiated (patch P-M5).
            println!("Session terminée.");
            println!("  Journal : {}", log.display());
            ExitCode::SUCCESS
        }
        Ok(SessionOutcome::Failed { code }) => {
            let code = code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "interrompu".to_string());
            failure(
                "la session s'est arrêtée sur une erreur",
                format!("code {code}\n  Journal : {}", log.display()),
            )
        }
        Err(e) => failure("démarrage de la session", e),
    }
}

fn pair(engine: &ClientEngine, host: &str) -> Result<(), ExitCode> {
    let pin = random::pairing_pin();
    println!("Premier accès à cet ordinateur : autorisation nécessaire.\n");
    println!("  Sur {host}, lancez maintenant :");
    println!("\n      zyr-cli host pin {pin}\n");
    println!("  En attente de l'autorisation...");

    match engine.pair(host, &pin) {
        Ok(()) => {
            println!("  Autorisé.\n");
            Ok(())
        }
        Err(e) => Err(failure(
            "appairage",
            format!("{e}\n  Vérifiez que « zyr-cli host start » tourne sur {host}."),
        )),
    }
}

fn build_settings(args: &Args) -> Result<SessionSettings, String> {
    let (width, height) = parse_resolution(&args.resolution).map_err(|e| e.to_string())?;
    let codec: Codec = args.codec.parse()?;
    let display_mode: DisplayMode = args.affichage.parse()?;
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
        packet_size: None,
        absolute_mouse: !args.souris_relative,
        stats_overlay: args.stats,
    })
}
