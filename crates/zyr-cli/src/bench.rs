//! What the tunnel costs, in milliseconds and in lost packets.
//!
//! The question this bench answers is simple: between two computers,
//! does the tunnel add anything to the trip? So it measures the same
//! trip twice, with the same packets at the same cadence: once over bare
//! UDP, once through the whole tunnel, engine ports included. Only the
//! gap between the two means anything; the absolute values also carry
//! the system's own noise.
//!
//! The two computers have to know each other's fingerprint.
//! `zyr-cli identity` shows it on each machine. It never changes once
//! created.

use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, Subcommand};
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_proto::paths;
use zyr_transport::{Fingerprint, Identity, MediaProfile, Path, TunnelEndpoint};
use zyr_tunnel::pump::open_socket;
use zyr_tunnel::{Answers, Tunnel};

use crate::cpu::{self, Stopwatch};
use crate::failure;
use crate::measurement::{Outcome, gap, milliseconds};
use crate::probe::{self, Cadence};

/// The bench's port, outside the range reserved for the engines.
const TUNNEL_PORT: u16 = 47010;
/// Echo reached without the tunnel, which serves as the reference.
const DIRECT_PORT: u16 = 47011;
/// Port base the bench lends to its fake engines.
const ENGINE_BASE: u16 = 42900;
/// The bench measures one path at a time and no more.
const DEVICE: u16 = 0;
/// The host engine listens on the local machine only.
const ENGINE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// What the bench answers on ZyrDesk's own channel.
///
/// Its engines are echo sockets: they have ports and nothing else, so
/// there is nobody here to hand a pairing code to.
struct NoEngine {
    ports: EnginePorts,
}

impl Answers for NoEngine {
    fn engine(&self) -> EnginePorts {
        self.ports
    }

    fn hand_over_the_code(&self, _pin: &str, _name: &str) -> Result<(), String> {
        Err("le banc de mesure n'a pas de moteur à appairer".to_string())
    }

    fn secure_attention(&self) -> Result<(), String> {
        Err("le banc de mesure ne presse aucune touche".to_string())
    }

    fn hush_the_speakers(&self, _quiet: bool) -> Result<(), String> {
        Err("le banc de mesure n'a pas d'enceintes".to_string())
    }

    fn lock_the_screen(&self) -> Result<(), String> {
        Err("le banc de mesure n'a pas d'écran à verrouiller".to_string())
    }

    fn serve_steady(&self, _rate: bool) -> Result<(), String> {
        Err("le banc de mesure n'a pas de moteur à régler".to_string())
    }
}
/// The bench takes connections from any interface.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
/// Time given to the transport to find the path's packet size.
const PATH_DISCOVERY: Duration = Duration::from_secs(2);
/// Rhythm at which the host bench watches for the traffic to start.
const WATCH_STEP: Duration = Duration::from_millis(200);

const DEFAULT_RATE: u64 = 50;

#[derive(Subcommand)]
pub enum Action {
    /// Waits and answers the other computer's measurements
    Host(HostArgs),
    /// Measures the path towards a waiting computer
    Client(ClientArgs),
}

#[derive(Args)]
pub struct HostArgs {
    /// Fingerprint of the computer that will measure
    #[arg(long, value_name = "FINGERPRINT")]
    pair: Fingerprint,
    /// Target rate in megabits per second. Set it as on the other
    /// computer, or the return path is throttled.
    #[arg(long, default_value_t = DEFAULT_RATE, value_parser = allowed_rate())]
    rate: u64,
}

#[derive(Args)]
pub struct ClientArgs {
    /// Address of the waiting computer
    address: IpAddr,
    /// Fingerprint of that computer
    #[arg(long, value_name = "FINGERPRINT")]
    pair: Fingerprint,
    /// Target rate in megabits per second
    #[arg(long, default_value_t = DEFAULT_RATE, value_parser = allowed_rate())]
    rate: u64,
    /// Length of each burst, in seconds
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=3600))]
    duration: u64,
    /// Simulated frames per second
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=480))]
    fps: u32,
    /// Loss to provoke underneath the tunnel, per thousand packets sent.
    /// Used to check that loss does not strangle the rate.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u16).range(0..=1000))]
    loss: u16,
}

/// Bounds on the rate. A rate of zero would send nothing, and an
/// unreasonable one would saturate the network card teaching us nothing.
fn allowed_rate() -> impl clap::builder::TypedValueParser<Value = u64> {
    clap::value_parser!(u64).range(1..=1000)
}

pub fn run(action: Action) -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => return failure("démarrage du banc", e),
    };

    let outcome = match action {
        Action::Host(args) => runtime.block_on(hold_the_bench(args)),
        Action::Client(args) => runtime.block_on(measure(args)),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => failure("le banc n'a pas pu aller au bout", e),
    }
}

fn profile(rate_mbps: u64, frames_per_second: u32) -> MediaProfile {
    MediaProfile {
        bits_per_second: rate_mbps * 1_000_000,
        frames_per_second,
    }
}

/// The measured side: two echoes, one per path, and the tunnel serving them.
async fn hold_the_bench(args: HostArgs) -> Result<(), Box<dyn Error>> {
    let identity = Identity::load_or_create(&paths::identity_dir())?;
    let ports = EnginePorts::new(ENGINE_BASE)?;

    // Reference: the same echo, reached without going through the tunnel.
    let direct = open_socket(SocketAddr::new(EVERY_INTERFACE, DIRECT_PORT))?;
    tokio::spawn(async move {
        let _ = probe::echo(direct).await;
    });

    // Fake engine: the tunnel hands it what it receives on the video
    // channel, and it sends it straight back.
    let engine = open_socket(SocketAddr::new(ENGINE, ports.video()))?;
    tokio::spawn(async move {
        let _ = probe::echo(engine).await;
    });

    let endpoint = TunnelEndpoint::host(
        &identity,
        args.pair,
        profile(args.rate, 60),
        SocketAddr::new(EVERY_INTERFACE, TUNNEL_PORT),
    )?;

    println!("Banc en attente sur le port {TUNNEL_PORT}.");
    println!("  Empreinte de cet ordinateur : {}", identity.fingerprint());
    println!("  Débit servi : {} Mb/s", args.rate);
    println!("\nCtrl+C pour arrêter.\n");

    loop {
        let connection = match endpoint.accept().await {
            Ok(connection) => connection,
            // A refused device must not close the bench.
            Err(e) => {
                println!("Connexion écartée : {e}");
                continue;
            }
        };

        println!("Mesure en cours...");
        // Each measurement gets its own task: the bench has to stay ready
        // to accept the next one, or the connection after it times out
        // while waiting.
        tokio::spawn(async move {
            let observed = connection.clone();
            match Tunnel::host(connection, ENGINE, Arc::new(NoEngine { ports })).await {
                Ok(mut tunnel) => {
                    let (without, with) = serve_and_measure(&mut tunnel).await;
                    // The return trip is only visible from here: the
                    // other bench knows only what it sent itself.
                    println!("  {}", breakdown(&tunnel, &observed, "au retour"));
                    report_computation(args.rate, without, with);
                }
                Err(e) => println!("Tunnel impossible : {e}"),
            }
            println!("Mesure terminée.\n");
        });
    }
}

/// The measuring side: the same trip twice, then the report.
async fn measure(args: ClientArgs) -> Result<(), Box<dyn Error>> {
    let identity = Identity::load_or_create(&paths::identity_dir())?;
    let ports = EnginePorts::new(ENGINE_BASE)?;
    let listen = IpAddr::V4(
        device_loopback_addr(DEVICE).ok_or("aucune adresse locale disponible pour le banc")?,
    );

    println!("Empreinte de cet ordinateur : {}", identity.fingerprint());
    println!("Connexion à {}...", args.address);

    // The direct reference is never degraded: it has to stay the same
    // trip for both measurements, or the comparison says nothing.
    let path = match args.loss {
        0 => Path::Direct,
        loss_per_thousand => Path::Degraded { loss_per_thousand },
    };
    let endpoint = TunnelEndpoint::client_on_path(
        &identity,
        args.pair,
        profile(args.rate, args.fps),
        SocketAddr::new(EVERY_INTERFACE, 0),
        path,
    )?;
    let connection = endpoint
        .connect(SocketAddr::new(args.address, TUNNEL_PORT))
        .await?;

    let usable = connection
        .settled_usable_datagram(PATH_DISCOVERY)
        .await
        .ok_or("le chemin n'accepte aucun datagramme")?;
    let size = zyr_transport::packet_size(usable)?;

    let cadence = Cadence {
        size: size.bytes,
        rate_mbps: args.rate,
        frames_per_second: args.fps,
        duration: Duration::from_secs(args.duration),
    };

    println!(
        "\n{} paquets de {} octets par seconde, pendant {} s, deux fois.",
        cadence.packets_per_frame() as u64 * cadence.frames_per_second as u64,
        cadence.size,
        args.duration
    );
    if args.loss > 0 {
        println!(
            "Perte provoquée sous le tunnel : {:.1} % des paquets émis.",
            args.loss as f64 / 10.0
        );
    }

    // The processor is read on each of the two bursts: the gap between
    // them is what the tunnel costs in computation, the probe itself
    // already consuming something.
    println!("\nMesure directe...");
    let direct_computation = Stopwatch::start();
    let direct = probe::probe(
        open_socket(SocketAddr::new(EVERY_INTERFACE, 0))?,
        SocketAddr::new(args.address, DIRECT_PORT),
        cadence,
    )
    .await?;
    let direct_load = direct_computation.and_then(|s| s.load());

    println!("Mesure à travers le tunnel...");
    let tunnel = Tunnel::client(connection.clone(), listen, ports).await?;
    let tunnel_computation = Stopwatch::start();
    let through_tunnel = probe::probe(
        open_socket(SocketAddr::new(listen, 0))?,
        SocketAddr::new(listen, ports.video()),
        cadence,
    )
    .await?;
    let tunnel_load = tunnel_computation.and_then(|s| s.load());

    report(&direct, &through_tunnel, &size, &connection, &tunnel);
    report_computation(args.rate, direct_load, tunnel_load);

    // Closing cleanly frees the other bench straight away, instead of
    // leaving it to wait for the connection to expire.
    drop(tunnel);
    endpoint.close().await;
    Ok(())
}

fn report(
    direct: &Outcome,
    through_tunnel: &Outcome,
    size: &zyr_transport::PacketSize,
    connection: &zyr_transport::Connection,
    tunnel: &Tunnel,
) {
    println!("\n--- Sans tunnel (référence) ---");
    detail(direct);

    println!("\n--- À travers le tunnel ---");
    detail(through_tunnel);
    println!("  taille de paquet   {} octets", size.bytes);
    if size.reduced_by_the_path {
        println!("                     réduite par le chemin");
    }
    println!(
        "  aller-retour vu par le transport   {}",
        milliseconds(connection.round_trip())
    );

    println!(
        "  ce que ce banc voit   {}",
        breakdown(tunnel, connection, "à l'aller")
    );
    println!("                        le retour est compté par l'autre banc");

    let reading = tunnel.reading();
    if reading.too_large > 0 {
        println!(
            "  {} paquets trop gros pour le chemin : la taille demandée au \
             moteur devra baisser",
            reading.too_large
        );
    }
    if reading.unreadable > 0 {
        println!("  {} datagrammes illisibles", reading.unreadable);
    }
    if reading.no_recipient > 0 {
        println!(
            "  {} datagrammes arrivés sur un canal muet côté local",
            reading.no_recipient
        );
    }
    if reading.refused > 0 {
        println!(
            "  {} paquets refusés par le système, sans conséquence sur la session",
            reading.refused
        );
    }

    println!("\n--- Ce que coûte le tunnel ---");
    println!(
        "  médiane            {}",
        gap(direct.median, through_tunnel.median)
    );
    println!(
        "  centile 95         {}",
        gap(direct.percentile_95, through_tunnel.percentile_95)
    );
    println!(
        "  centile 99         {}",
        gap(direct.percentile_99, through_tunnel.percentile_99)
    );
    println!(
        "  perte              {:+.2} point(s)",
        through_tunnel.loss() - direct.loss()
    );
    println!(
        "  débit tenu         {:.1} Mb/s contre {:.1} Mb/s",
        through_tunnel.rate(),
        direct.rate()
    );
}

/// Serves the tunnel until it ends, telling the two measurement phases
/// apart.
///
/// The other bench measures the bare path first, then the tunnel. Seen
/// from here, the switch is the first datagram to cross: before it, this
/// bench only answers the direct echo; after it, it also runs the
/// tunnel. The gap between the two is what the tunnel costs it, as on
/// the other side. Without this split, the idle phase would water the
/// measurement down by half.
async fn serve_and_measure(tunnel: &mut Tunnel) -> (Option<f64>, Option<f64>) {
    let counters = tunnel.counters();
    let mut without_tunnel = Stopwatch::start();
    let mut with_tunnel: Option<Stopwatch> = None;
    let mut load_without = None;

    loop {
        tokio::select! {
            outcome = tunnel.wait() => {
                if let Err(e) = outcome {
                    println!("Fin de la mesure : {e}");
                }
                return (load_without, with_tunnel.and_then(|s| s.load()));
            }
            _ = tokio::time::sleep(WATCH_STEP) => {
                if with_tunnel.is_none() && counters.reading().to_engine > 0 {
                    load_without = without_tunnel.take().and_then(|s| s.load());
                    with_tunnel = Stopwatch::start();
                }
            }
        }
    }
}

/// What the tunnel costs in computation, as a share of one core.
///
/// The raw figure is not comparable to the project's threshold: the
/// bench sends and receives at once, so each end sees twice the
/// requested rate go by, where a real session only does one direction.
/// The second figure brings the cost back to what a session would pay,
/// assuming the computation follows the number of packets handled.
fn report_computation(rate_mbps: u64, direct: Option<f64>, tunnel: Option<f64>) {
    let (Some(direct), Some(tunnel)) = (direct, tunnel) else {
        println!("\n  Charge processeur : non mesurable sur cette plateforme.");
        return;
    };

    let cost = tunnel - direct;
    println!("\n--- Processeur de ce banc ---");
    println!("  sans tunnel        {direct:.1} % d'un coeur");
    println!("  avec tunnel        {tunnel:.1} % d'un coeur");
    println!(
        "  coût du tunnel     {cost:+.1} point(s) pour {} Mb/s traversés",
        rate_mbps * 2
    );
    println!(
        "  soit               {:.1} point(s) pour une session à {rate_mbps} Mb/s, \
         qui n'en fait qu'un sens",
        cost / 2.0
    );
    println!("  machine à {} coeurs", cpu::cores());
}

/// Where what is missing comes from, seen by one bench alone.
///
/// Each end knows only what it sent: the transport detects losses only
/// through the acknowledgements that come back to it. The two halves of
/// the trip therefore need both terminals.
fn breakdown(tunnel: &Tunnel, connection: &zyr_transport::Connection, way: &str) -> String {
    let dropped = tunnel
        .reading()
        .to_tunnel
        .saturating_sub(connection.datagrams_sent());
    format!(
        "{dropped} datagramme(s) jeté(s) faute de place, {} paquet(s) perdu(s) {way}",
        connection.packets_lost()
    )
}

fn detail(outcome: &Outcome) {
    println!(
        "  aller-retour       médiane {}   c95 {}   c99 {}   pire {}",
        milliseconds(outcome.median),
        milliseconds(outcome.percentile_95),
        milliseconds(outcome.percentile_99),
        milliseconds(outcome.worst)
    );
    println!(
        "  perte              {} sur {} ({:.2} %)",
        outcome.lost(),
        outcome.sent,
        outcome.loss()
    );
    println!("  débit tenu         {:.1} Mb/s", outcome.rate());
}
