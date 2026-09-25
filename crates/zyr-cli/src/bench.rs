//! What the tunnel costs, in milliseconds and in lost packets.
//!
//! The question this bench answers is simple: between two computers,
//! does the tunnel add anything to the trip? So it measures the same
//! trip twice, with the same packets at the same cadence: once over bare
//! UDP, once through the whole tunnel, local links included, the way a
//! session's pictures travel. Only the gap between the two means
//! anything; the absolute values also carry the system's own noise.
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
use zyr_control::link::{self, Access, Link, LinkListener};
use zyr_proto::paths;
use zyr_transport::mtu::MUX_OVERHEAD;
use zyr_transport::{Connection, Fingerprint, Identity, Media, MediaProfile, Path, TunnelEndpoint};
use zyr_tunnel::aside::{self, Given, Wanted};
use zyr_tunnel::{Answers, Tunnel, service_channel};

use crate::cpu::{self, Stopwatch};
use crate::failure;
use crate::measurement::{Outcome, gap, milliseconds};
use crate::probe::{self, Cadence, Road, open_socket};

/// The bench's port, apart from the product's own.
const TUNNEL_PORT: u16 = 47010;
/// Echo reached without the tunnel, which serves as the reference.
const DIRECT_PORT: u16 = 47011;

/// What the bench answers on ZyrDesk's own channel: nothing, since it
/// has no screen, no speakers and no journal. Only the session itself is
/// served, by a stand-in engine that sends every picture straight back.
struct Bench;

impl Answers for Bench {
    fn secure_attention(&self) -> Result<(), String> {
        Err("le banc de mesure ne presse aucune touche".to_string())
    }

    fn hush_the_speakers(&self, _quiet: bool) -> Result<(), String> {
        Err("le banc de mesure n'a pas d'enceintes".to_string())
    }

    fn lock_the_screen(&self) -> Result<(), String> {
        Err("le banc de mesure n'a pas d'écran à verrouiller".to_string())
    }

    fn screen_for_a_session(
        &self,
        _wanted: Option<zyr_proto::session::WantedScreen>,
    ) -> Result<Option<(u32, u32)>, String> {
        Err("le banc de mesure n'a pas d'écran virtuel".to_string())
    }

    fn journal(&self, _sift: &str) -> Result<String, String> {
        Err("le banc de mesure ne tient pas de journal".to_string())
    }

    fn reach_log(&self) -> Result<String, String> {
        Err("le banc de mesure ne mesure pas ce qu'il atteint".to_string())
    }

    fn empty_the_journal(&self) -> Result<(), String> {
        Err("le banc de mesure ne tient pas de journal".to_string())
    }

    fn pointer(&self) -> Result<zyr_proto::session::Pointer, String> {
        Err("le banc de mesure n'a pas de curseur".to_string())
    }

    fn screens(&self) -> Result<String, String> {
        Err("le banc de mesure ne filme aucun écran".to_string())
    }

    fn film_this_screen(&self, _id: Option<String>) -> Result<(), String> {
        Err("le banc de mesure ne filme aucun écran".to_string())
    }

    fn clipboard(
        &self,
        _pushing: Option<zyr_proto::clipboard::Clip>,
        _seen: Option<zyr_proto::clipboard::Stamp>,
    ) -> Result<Option<zyr_proto::clipboard::Clip>, String> {
        Err("le banc de mesure n'a pas de presse-papiers".to_string())
    }

    fn pieces(
        &self,
        _asking: Option<Wanted>,
        _giving: Option<Given>,
    ) -> Result<(Option<Given>, Option<Wanted>), String> {
        Err("le banc de mesure ne copie aucun fichier".to_string())
    }
}
/// The bench takes connections from any interface.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
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

/// The measured side: two echoes, one per road, and the tunnel serving
/// the second.
async fn hold_the_bench(args: HostArgs) -> Result<(), Box<dyn Error>> {
    let identity = Identity::load_or_create(&paths::identity_dir())?;

    // Reference: the same echo, reached without going through the tunnel.
    let direct = open_socket(SocketAddr::new(EVERY_INTERFACE, DIRECT_PORT))?;
    tokio::spawn(async move {
        let _ = probe::echo(direct).await;
    });

    let media = Media::from(profile(args.rate, 60));
    let endpoint = TunnelEndpoint::host(
        &identity,
        args.pair,
        media.clone(),
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
        let media = media.clone();
        // Each measurement gets its own task: the bench has to stay ready
        // to accept the next one, or the connection after it times out
        // while waiting.
        tokio::spawn(async move {
            let observed = connection.clone();
            match serve(connection, &media).await {
                Ok(mut tunnel) => {
                    let (without, with) = serve_and_measure(&mut tunnel).await;
                    // The return trip is only visible from here: the
                    // other bench knows only what it sent itself.
                    println!("  {}", breakdown(&tunnel, &observed, "au retour"));
                    report_computation(args.rate, without, with);
                }
                Err(e) => println!("Tunnel impossible : {e}"),
            }
            // The tunnel is sized for whatever comes next, as the service
            // does once a session has gone.
            media.serving_nobody();
            println!("Mesure terminée.\n");
        });
    }
}

/// Opens the session the other bench asks for, the way the service does:
/// its engine brought up on a link of its own, the answer, then the
/// tunnel.
///
/// The engine is a stand-in that sends every picture straight back, so
/// what the other bench measures is the whole road a picture takes.
async fn serve(
    connection: Connection,
    media: &Media,
) -> Result<Tunnel, Box<dyn Error + Send + Sync>> {
    let answering: Arc<dyn Answers> = Arc::new(Bench);
    let opening = aside::until_a_session_opens(&connection, answering.clone(), None).await?;
    // The bench sizes its tunnel on the session that opens on it, like the
    // service does: measuring a window worked out from anything else
    // would be measuring something the product never runs.
    media.serving(opening.serving());
    let listener = LinkListener::create(Access::SystemAndInteractive)?;
    let name = listener.name().to_string();
    let (engine, accepted) = tokio::join!(link::connect(&name), listener.accept());
    tokio::spawn(probe::echo_pictures(engine?));
    // Nothing is said to the stand-in, and what it says is heard by
    // nobody: the service's half goes at once, which the tunnel takes as
    // a service with nothing to say.
    let (side, _) = service_channel();
    opening.opened().await?;
    Ok(Tunnel::host(connection, answering, accepted?, side, None))
}

/// The measuring side: the same trip twice, then the report.
async fn measure(args: ClientArgs) -> Result<(), Box<dyn Error>> {
    let identity = Identity::load_or_create(&paths::identity_dir())?;

    println!("Empreinte de cet ordinateur : {}", identity.fingerprint());
    println!("Connexion à {}...", args.address);

    // The direct reference is never degraded: it has to stay the same
    // trip for both measurements, or the comparison says nothing.
    let path = match args.loss {
        0 => Path::Direct,
        loss_per_thousand => Path::Degraded { loss_per_thousand },
    };
    let serving = profile(args.rate, args.fps);
    let endpoint = TunnelEndpoint::client_on_path(
        &identity,
        args.pair,
        serving,
        SocketAddr::new(EVERY_INTERFACE, 0),
        path,
    )?;
    let connection = endpoint
        .connect(SocketAddr::new(args.address, TUNNEL_PORT))
        .await?;

    // What one picture datagram of the engine may weigh on this path:
    // what the path can never stop carrying, less the byte naming its
    // channel.
    let size = connection
        .guaranteed_usable_datagram()
        .and_then(|usable| usable.checked_sub(MUX_OVERHEAD))
        .ok_or("le chemin n'accepte aucun datagramme")?;

    let cadence = Cadence {
        size,
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
        Road::Bare {
            socket: open_socket(SocketAddr::new(EVERY_INTERFACE, 0))?,
            echo: SocketAddr::new(args.address, DIRECT_PORT),
        },
        cadence,
    )
    .await?;
    let direct_load = direct_computation.and_then(|s| s.load());

    println!("Mesure à travers le tunnel...");
    let (tunnel, player) = open_the_session(&connection, serving).await?;
    let tunnel_computation = Stopwatch::start();
    let through_tunnel = probe::probe(Road::Tunnel(player), cadence).await?;
    let tunnel_load = tunnel_computation.and_then(|s| s.load());

    report(&direct, &through_tunnel, size, &connection, &tunnel);
    report_computation(args.rate, direct_load, tunnel_load);

    // Closing cleanly frees the other bench straight away, instead of
    // leaving it to wait for the connection to expire.
    tunnel.close().await;
    endpoint.close().await;
    Ok(())
}

/// Opens the session on the other bench, and brings its tunnel up on
/// this side the way a way does: a link for the player, the tunnel on
/// it, and the player connected.
///
/// The way tells its player how the tunnel stands; this one has nothing
/// to tell, and its half of the service goes at once.
async fn open_the_session(
    connection: &Connection,
    serving: MediaProfile,
) -> Result<(Tunnel, Link), Box<dyn Error>> {
    aside::ask_to_open(connection, serving).await?;
    let listener = LinkListener::create(Access::SystemAndInteractive)?;
    let name = listener.name().to_string();
    let (side, _) = service_channel();
    let tunnel = Tunnel::client(connection.clone(), listener, side, None);
    let player = link::connect(&name).await?;
    Ok((tunnel, player))
}

fn report(
    direct: &Outcome,
    through_tunnel: &Outcome,
    size: u16,
    connection: &Connection,
    tunnel: &Tunnel,
) {
    println!("\n--- Sans tunnel (référence) ---");
    detail(direct);

    println!("\n--- À travers le tunnel ---");
    detail(through_tunnel);
    println!("  taille de paquet   {size} octets");
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
            "  {} paquets trop gros pour le chemin : il s'est rétréci sous \
             ce qu'il promettait",
            reading.too_large
        );
    }
    if reading.crowded > 0 {
        println!(
            "  {} paquets jetés faute de place dans la file d'envoi : le chemin \
             ne prend pas les paquets au rythme où le moteur les fait",
            reading.crowded
        );
    }
    if reading.crowded_here > 0 {
        println!(
            "  {} paquets jetés ici faute de place vers le lecteur : il ne les \
             prenait pas assez vite",
            reading.crowded_here
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
                if with_tunnel.is_none() && counters.reading().to_link > 0 {
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
fn breakdown(tunnel: &Tunnel, connection: &Connection, way: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pictures_sent_through_the_tunnel_come_back_from_the_stand_in_engine() {
        // The whole road a measurement takes, on loopback: the session
        // opened the way the other bench opens it, the stand-in engine at
        // the far end, and the probe on the player's end of the link.
        let host_identity = Identity::generate().unwrap();
        let client_identity = Identity::generate().unwrap();
        let loopback = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0);
        let media = Media::from(profile(1, 30));
        let host_endpoint = TunnelEndpoint::host(
            &host_identity,
            client_identity.fingerprint(),
            media.clone(),
            loopback,
        )
        .unwrap();
        let meeting_point = host_endpoint.local_address().unwrap();
        let serving = profile(10, 60);
        let client_endpoint = TunnelEndpoint::client(
            &client_identity,
            host_identity.fingerprint(),
            serving,
            loopback,
        )
        .unwrap();
        let (host_side, client_side) = tokio::join!(
            host_endpoint.accept(),
            client_endpoint.connect(meeting_point)
        );
        let client_side = client_side.unwrap();

        let (served, opened) = tokio::join!(
            serve(host_side.unwrap(), &media),
            open_the_session(&client_side, serving)
        );
        let host_tunnel = served.unwrap();
        let (tunnel, player) = opened.unwrap();
        // Sized on what the session asked for, as the service sizes it.
        assert_eq!(media.now(), serving);

        let size = client_side.guaranteed_usable_datagram().unwrap() - MUX_OVERHEAD;
        let cadence = Cadence {
            size,
            rate_mbps: 5,
            frames_per_second: 60,
            duration: Duration::from_millis(300),
        };
        let outcome = probe::probe(Road::Tunnel(player), cadence).await.unwrap();
        assert!(outcome.sent > 0);
        assert_eq!(outcome.lost(), 0, "nothing gets lost over loopback");
        assert!(host_tunnel.reading().to_link > 0);

        tunnel.close().await;
        host_tunnel.close().await;
    }
}
