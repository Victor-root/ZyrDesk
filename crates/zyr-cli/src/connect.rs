//! Opens a session on a remote computer.
//!
//! Nothing is held here. The service opens the way to the other computer
//! and keeps it; this command asks for one, starts the client engine on
//! the local addresses it was given, and tells the service which process
//! that way now serves. Closing this window does not end the session,
//! and losing it does not leave the way open forever.
//!
//! The client engine talks to those local addresses and never touches
//! the network. The two computers recognise each other by fingerprint
//! before anything else happens; until the rendezvous server exists,
//! fingerprints are copied across by hand, as is the engine's own
//! pairing code.
//!
//! A direct mode without the tunnel is kept for diagnosis, to tell a
//! tunnel problem from an engine problem in minutes. It never appears in
//! the interface.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use zyr_control::{Answer, Request, Service, WayId};
use zyr_engine_client::state::identifier_from_address;
use zyr_engine_client::{ClientEngine, DeviceState, SessionOutcome};
use zyr_proto::paths;
use zyr_proto::random;
use zyr_proto::session::{Codec, DisplayMode, SessionSettings, parse_resolution};
use zyr_transport::{Fingerprint, MediaProfile};

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

    /// Display: fullscreen, borderless or windowed
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
    let mut settings = match build_settings(&args) {
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

    // The way stands before the engine is told anything: what the engine
    // is handed is a local address that only exists once it is open.
    let mut driving = match args.pair {
        Some(peer) => match Driving::towards(&args.host, peer, &settings) {
            Ok(driving) => Some(driving),
            Err(message) => return failure("ouverture du tunnel", message),
        },
        None => None,
    };
    let target = match &driving {
        Some(driving) => {
            settings.packet_size = Some(u32::from(driving.packet));
            driving.target.clone()
        }
        None => args.host.clone(),
    };

    let state = DeviceState::for_device(&identifier_from_address(&args.host));
    if args.pair_again
        && let Err(e) = state.forget()
    {
        return failure("réinitialisation de l'appairage", e);
    }

    let already_known = state.has_a_paired_host();
    let log = paths::logs_dir().join("session.log");
    let engine = ClientEngine::new(&exe, state).with_log(&log);

    if !already_known && let Err(code) = pair(&engine, &target, &args.host) {
        return code;
    }

    println!(
        "Connexion à {} en {}x{} à {} images par seconde...",
        args.host, settings.width, settings.height, settings.fps
    );
    let mut session = match engine.start_session(&target, &settings) {
        Ok(session) => session,
        Err(e) => return failure("démarrage de la session", e),
    };

    // From here the session belongs to the engine and to the service.
    // This window can be closed without ending it.
    if let Some(driving) = &mut driving {
        driving.hold(session.process_id());
    }

    let outcome = session.wait();
    if let Some(driving) = &mut driving {
        driving.let_go();
    }

    match outcome {
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
        Err(e) => failure("surveillance de la session", e),
    }
}

/// The service, and the way it holds for this session.
struct Driving {
    runtime: tokio::runtime::Runtime,
    service: Service,
    way: WayId,
    /// Address the client engine is given, standing in for the remote
    /// computer.
    target: String,
    /// Packet size the path allows, imposed on the engine.
    packet: u16,
}

impl Driving {
    /// Asks the service for a way to that computer.
    fn towards(host: &str, peer: Fingerprint, settings: &SessionSettings) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;

        let mut service = runtime
            .block_on(Service::join())
            .map_err(|e| e.to_string())?;
        // The window the transport keeps open follows the session that
        // was actually asked for, not a nominal one.
        let request = Request::Reach {
            host: host.to_string(),
            peer,
            media: MediaProfile {
                bits_per_second: u64::from(settings.bitrate_kbps) * 1000,
                frames_per_second: settings.fps,
            },
        };

        let reached = match runtime
            .block_on(service.ask(&request))
            .map_err(|e| e.to_string())?
        {
            Answer::Reached(reached) => reached,
            Answer::Refused(reason) => return Err(reason),
            other => return Err(format!("réponse inattendue du service : {other}")),
        };

        println!("Tunnel établi avec {host}.");
        println!("  Taille de paquet : {} octets.", reached.packet);

        Ok(Self {
            runtime,
            service,
            way: reached.way,
            target: format!("{}:{}", reached.address, reached.engine.http()),
            packet: reached.packet,
        })
    }

    /// Tells the service which process the way now serves, so it closes
    /// on its own whatever becomes of this window.
    fn hold(&mut self, process: u32) {
        let request = Request::Hold {
            way: self.way,
            process,
        };
        if let Err(e) = self.runtime.block_on(self.service.ask(&request)) {
            eprintln!("Avertissement : le service n'a pas pris la session en charge ({e}).");
            eprintln!("  Elle se fermera avec cette fenêtre.");
        }
    }

    /// Gives the way back at the end of the session. The service would
    /// close it on its own; saying so frees the address at once.
    fn let_go(&mut self) {
        let request = Request::Release { way: self.way };
        let _ = self.runtime.block_on(self.service.ask(&request));
    }
}

/// `target` is where the engine goes, `host` what the user typed: the
/// first is a stand-in address, the second is what makes the message
/// mean something.
fn pair(engine: &ClientEngine, target: &str, host: &str) -> Result<(), ExitCode> {
    let pin = random::pairing_pin();
    println!("Premier accès à cet ordinateur : autorisation nécessaire.\n");
    println!("  Sur {host}, lancez maintenant :");
    println!("\n      zyr-cli host pin {pin}\n");
    println!("  En attente de l'autorisation...");

    match engine.pair(target, &pin) {
        Ok(()) => {
            println!("  Autorisé.\n");
            Ok(())
        }
        Err(e) => Err(failure(
            "appairage",
            format!("{e}\n  Vérifiez que l'accès distant est actif sur {host}."),
        )),
    }
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
    })
}
