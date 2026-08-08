//! Opens a session on a remote computer.
//!
//! Everything travels through the ZyrDesk tunnel. The client engine
//! talks to local ports standing in for the remote engine's and never
//! touches the network; the two computers recognise each other by
//! fingerprint before anything else happens. Until the rendezvous server
//! exists, those fingerprints are copied across by hand, as is the
//! engine's own pairing code.
//!
//! A direct mode without the tunnel is kept for diagnosis, to tell a
//! tunnel problem from an engine problem in minutes. It never appears in
//! the interface.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::time::Duration;

use clap::Args as ClapArgs;
use zyr_engine_client::state::identifier_from_address;
use zyr_engine_client::{ClientEngine, DeviceState, SessionOutcome};
use zyr_proto::net::{TUNNEL_PORT, device_loopback_addr};
use zyr_proto::paths;
use zyr_proto::random;
use zyr_proto::session::{Codec, DisplayMode, SessionSettings, parse_resolution};
use zyr_transport::{Fingerprint, Identity, MediaProfile, TunnelEndpoint, packet_size};
use zyr_tunnel::{Tunnel, greeting};

use crate::failure;

/// Local address the remote engine is made to appear at.
///
/// One outgoing session at a time for now, so one address is enough.
const DEVICE: u16 = 0;

/// Where the tunnel leaves from: any interface, any port.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Time left to path discovery before the packet size is fixed.
///
/// The engine keeps that size for the whole session and cannot change it
/// along the way, so it is worth a moment's wait.
const PATH_DISCOVERY: Duration = Duration::from_secs(2);

#[derive(ClapArgs)]
pub struct Args {
    /// Address of the remote computer
    host: String,

    /// Fingerprint of the remote computer, shown there by "zyr-cli identity"
    #[arg(long, value_name = "FINGERPRINT", required_unless_present = "direct")]
    pair: Option<Fingerprint>,

    /// Goes straight to the remote engine, without the tunnel. For
    /// diagnosis only: the address then carries the engine's port.
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

    // The tunnel stands before the engine is told anything: what the
    // engine is handed is a local address that only exists once it is up.
    let carried = match args.pair {
        Some(peer) => match Carried::open(&args.host, peer, &settings) {
            Ok(carried) => Some(carried),
            Err(message) => return failure("ouverture du tunnel", message),
        },
        None => None,
    };
    let target = match &carried {
        Some(carried) => {
            settings.packet_size = Some(carried.packet_size);
            carried.engine_target.clone()
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
    match engine.start_session(&target, &settings) {
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

/// The tunnel held open for the length of the session.
///
/// The field order is the drop order: the pumps stop, then the transport,
/// then the threads that carried them.
struct Carried {
    _tunnel: Tunnel,
    _endpoint: TunnelEndpoint,
    _runtime: tokio::runtime::Runtime,
    /// Address the client engine is given, standing in for the remote one.
    engine_target: String,
    /// Packet size the path allows, imposed on the engine.
    packet_size: u32,
}

impl Carried {
    fn open(host: &str, peer: Fingerprint, settings: &SessionSettings) -> Result<Self, String> {
        let remote = resolve(host)?;
        let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let identity =
            Identity::load_or_create(&paths::identity_dir()).map_err(|e| e.to_string())?;
        // The window the transport keeps open follows the session that
        // was actually asked for, not a nominal one.
        let profile = MediaProfile {
            bits_per_second: u64::from(settings.bitrate_kbps) * 1000,
            frames_per_second: settings.fps,
        };

        let opened = runtime.block_on(async {
            let endpoint = TunnelEndpoint::client(
                &identity,
                peer,
                profile,
                SocketAddr::new(EVERY_INTERFACE, 0),
            )
            .map_err(|e| e.to_string())?;

            let connection = endpoint.connect(remote).await.map_err(|e| {
                format!("{host} ne répond pas sur le port {TUNNEL_PORT} : {e}")
            })?;

            // The first real exchange, and the moment authorisation is
            // proven: a connection succeeds before the other computer
            // has judged our certificate, so nothing may be announced
            // as established until this answers.
            let greeting = greeting::ask(&connection).await.map_err(|e| {
                format!(
                    "{host} a refusé cet ordinateur, ou son empreinte a changé.\n  \
                     Sur {host} : zyr-cli host authorize {}\n  Détail : {e}",
                    identity.fingerprint()
                )
            })?;

            let usable = connection
                .settled_usable_datagram(PATH_DISCOVERY)
                .await
                .ok_or("le chemin n'annonce aucune taille de datagramme")?;
            let size = packet_size(usable).map_err(|e| e.to_string())?;

            let side = IpAddr::V4(
                device_loopback_addr(DEVICE).ok_or("aucune adresse locale pour cet appareil")?,
            );
            let tunnel = Tunnel::client(connection, side, greeting.engine)
                .await
                .map_err(|e| {
                    format!("les ports locaux n'ont pas pu être ouverts : {e}\n  Une autre session ZyrDesk est peut-être déjà ouverte.")
                })?;

            Ok::<_, String>((
                endpoint,
                tunnel,
                format!("{side}:{}", greeting.engine.http()),
                size,
            ))
        })?;

        let (endpoint, tunnel, engine_target, size) = opened;
        println!("Tunnel établi avec {host}.");
        if size.reduced_by_the_path {
            println!(
                "  Taille de paquet réduite par le chemin : {} octets.",
                size.bytes
            );
        }
        Ok(Self {
            _tunnel: tunnel,
            _endpoint: endpoint,
            _runtime: runtime,
            engine_target,
            packet_size: u32::from(size.bytes),
        })
    }
}

/// Where the tunnel has to knock. Only the port is ours to add.
fn resolve(host: &str) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    format!("{host}:{TUNNEL_PORT}")
        .to_socket_addrs()
        .map_err(|e| format!("adresse « {host} » introuvable : {e}"))?
        .next()
        .ok_or_else(|| format!("adresse « {host} » introuvable"))
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
        // Decided by the tunnel once the path is known, left to the
        // engine when going direct.
        packet_size: None,
        absolute_mouse: !args.relative_mouse,
        stats_overlay: args.stats,
    })
}
