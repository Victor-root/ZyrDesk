//! Opens a session on a remote computer, and plays it without a window.
//!
//! Nothing is held here, and almost nothing is decided here either: the
//! opening lives in `zyr-session` and the player in `zyr-player`, both
//! used word for word by the interface. The player is started without a
//! window: it decodes and counts every picture and every sound, and draws
//! none. What belongs to this command is reading the options, saying out
//! loud what is happening, and printing what the session costs once a
//! second until it ends or Ctrl+C is pressed.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use clap::Args as ClapArgs;
use zyr_player::{Ending, Event, Measures, Player, Surface};
use zyr_proto::log::Log;
use zyr_proto::session::{Codec, Preferred, SessionSettings, parse_resolution};
use zyr_session::{Step, Wanted};
use zyr_transport::Fingerprint;

use crate::failure;

/// How often what the session costs is printed.
const EVERY: Duration = Duration::from_secs(1);

/// How long a player told to stop is given to say it has.
///
/// Stopping is a goodbye on a local link, answered at once; a player that
/// has not said it by then is not going to, and the command ends anyway.
const STOPPING_TAKES: Duration = Duration::from_secs(3);

/// What this command ends with when a second Ctrl+C will not wait: 128
/// and the number of the interrupt signal, the code a shell gives a
/// program Ctrl+C stopped.
const STOPPED_AT_ONCE: i32 = 130;

#[derive(ClapArgs)]
pub struct Args {
    /// Address of the remote computer
    host: String,

    /// Fingerprint of the remote computer, shown there by "zyr-cli identity"
    #[arg(long, value_name = "FINGERPRINT")]
    pair: Fingerprint,

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
}

pub fn run(args: Args) -> ExitCode {
    let settings = match build_settings(&args) {
        Ok(settings) => settings,
        Err(message) => return failure("réglages de session invalides", message),
    };
    let journal = crate::journal();
    let log = match Log::open(&journal) {
        Ok(log) => log,
        Err(e) => return failure(&format!("journal {}", journal.display()), e),
    };

    // Ctrl+C lets go of the opening where it stands, and of the session
    // once it plays.
    let interrupted = Arc::new(AtomicBool::new(false));
    if let Err(e) = on_ctrl_c(Arc::clone(&interrupted)) {
        return failure("écoute de Ctrl+C", e);
    }

    let wanted = Wanted {
        host: args.host.clone(),
        peer: args.pair,
        settings,
        // The command line is the diagnostic path: it asks nothing of
        // the far computer's speakers, which it leaves exactly as its own
        // settings had them.
        hush_the_far_speakers: false,
        // It asks for a size and so for the screen needed to carry it.
        // But it measures no screen, so it has no magnification to ask
        // for: the far computer keeps its own.
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
    let asked_at = Instant::now();
    let still_wanted = || !interrupted.load(Ordering::Relaxed);
    let opened = match zyr_session::open(&wanted, &mut |step| tell(step, &args.host), &still_wanted)
    {
        Ok(opened) => opened,
        Err(zyr_session::Error::Abandoned) => {
            println!("Ouverture abandonnée.");
            return ExitCode::SUCCESS;
        }
        Err(e) => return reported(e),
    };

    println!("Connexion à {}...", args.host);
    let (said, heard) = mpsc::channel();
    let player = match Player::start(
        &opened.link,
        // A still screen is sent again at the full rate, as in the
        // window by default: what is measured is then a steady stream.
        zyr_session::player_wants(&opened.settings, Preferred::default().steady_far_rate),
        Surface::Headless,
        log,
        Box::new(move |event| {
            let _ = said.send(event);
        }),
    ) {
        Ok(player) => player,
        Err(e) => return failure("démarrage du lecteur", e),
    };
    // The way is not tied to this program: a session played here has no
    // picture anybody else could show, so it is kept out of the sessions
    // the service lists for the window. It closes with its player's link
    // all the same.

    let ending = watch(&player, &heard, &interrupted, asked_at);
    // The way goes back to the service only now, with the player gone.
    drop(opened);
    ended(ending, &journal)
}

/// Follows the session until it ends, printing what it costs once a
/// second, and stops it on Ctrl+C.
///
/// `None` is a player told to stop that never said it had.
fn watch(
    player: &Player,
    heard: &mpsc::Receiver<Event>,
    interrupted: &AtomicBool,
    asked_at: Instant,
) -> Option<Ending> {
    let mut next = Instant::now() + EVERY;
    let mut stopped_at: Option<Instant> = None;
    loop {
        if stopped_at.is_none() && interrupted.load(Ordering::Relaxed) {
            player.stop();
            stopped_at = Some(Instant::now());
        }
        if stopped_at.is_some_and(|at| at.elapsed() > STOPPING_TAKES) {
            return None;
        }
        let now = Instant::now();
        if now >= next {
            println!("{}", measured(&player.measures()));
            next += EVERY;
        }
        // Short enough that Ctrl+C is felt at once.
        let waited = next
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100));
        match heard.recv_timeout(waited) {
            Ok(Event::Streaming {
                codec,
                width,
                height,
            }) => println!("Image : {} en {width}x{height}.", codec.name()),
            Ok(Event::FirstPicture) => println!(
                "Première image décodée, {} ms après la demande.",
                asked_at.elapsed().as_millis()
            ),
            Ok(Event::Notice(text)) => println!("  {text}"),
            Ok(Event::Ended(ending)) => return Some(ending),
            Err(RecvTimeoutError::Timeout) => {}
            // Nothing can be told any more: the player is gone with every
            // way it had of saying so.
            Err(RecvTimeoutError::Disconnected) => return Some(Ending::LinkLost),
        }
    }
}

/// Says how the session ended, and hands back the exit code that goes
/// with it.
fn ended(ending: Option<Ending>, journal: &std::path::Path) -> ExitCode {
    let journal = format!("Journal : {}", journal.display());
    match ending {
        Some(Ending::Asked) => {
            println!("Session terminée.");
            println!("  {journal}");
            ExitCode::SUCCESS
        }
        Some(Ending::HostLeft) => {
            println!("L'ordinateur distant a mis fin à la session.");
            println!("  {journal}");
            ExitCode::SUCCESS
        }
        Some(Ending::LinkLost) => failure(
            "la session s'est interrompue sans un mot de l'ordinateur distant",
            journal,
        ),
        Some(Ending::EngineFailed(why)) => failure(
            "la session s'est arrêtée sur une erreur",
            format!("{why}\n  {journal}"),
        ),
        None => failure(
            "le lecteur ne s'est pas arrêté à temps",
            format!(
                "{} s après la demande\n  {journal}",
                STOPPING_TAKES.as_secs()
            ),
        ),
    }
}

/// Sets `interrupted` at the first Ctrl+C, and ends the program at the
/// second.
///
/// The first lets go of the opening where it stands, or stops the
/// player, and waits for that to be over: giving the way back waits for
/// the service to finish whatever it was asking the far computer, which
/// can take seconds. Pressed again, it is somebody who will not wait, and
/// nothing is lost by obliging: the service closes a way nobody uses on
/// its own.
///
/// On a thread and a small runtime of their own: the rest of this command
/// waits on the session and never on a runtime.
fn on_ctrl_c(interrupted: Arc<AtomicBool>) -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    std::thread::Builder::new()
        .name("zyr-cli-ctrl-c".to_string())
        .spawn(move || {
            runtime.block_on(async {
                while tokio::signal::ctrl_c().await.is_ok() {
                    if interrupted.swap(true, Ordering::Relaxed) {
                        eprintln!("Arrêt immédiat.");
                        std::process::exit(STOPPED_AT_ONCE);
                    }
                    println!("Arrêt demandé. Ctrl+C à nouveau pour quitter sans attendre.");
                }
            });
        })?;
    Ok(())
}

/// What the session costs, on one line.
///
/// A reading not taken yet is a dash and never a nought: a player with no
/// decoded picture has no decoding time, not a decoding time of nothing.
fn measured(measures: &Measures) -> String {
    let ms = |value: Option<f64>| value.map_or("-".to_string(), |ms| format!("{ms:.1} ms"));
    let picture = match (&measures.codec, measures.width, measures.height) {
        (Some(codec), Some(width), Some(height)) => format!("{codec} {width}x{height}"),
        _ => "image -".to_string(),
    };
    format!(
        "{picture}, {} im/s | décodage {} | affichage {} | hôte {} | réseau {} | débit {} | \
         pertes {} | latence {}",
        measures
            .fps
            .map_or("-".to_string(), |fps| format!("{fps:.1}")),
        ms(measures.decode_ms),
        ms(measures.render_ms),
        ms(measures.host_ms),
        ms(measures.network_ms),
        measures
            .bitrate_mbps
            .map_or("-".to_string(), |mbps| format!("{mbps:.1} Mb/s")),
        measures
            .dropped_network_pct
            .map_or("-".to_string(), |pct| format!("{pct:.1} %")),
        ms(measures.latency_ms),
    )
}

/// Says what is happening, in the order it happens.
fn tell(step: Step, host: &str) {
    match step {
        Step::Reached => println!("Tunnel établi avec {host}."),
        Step::FarScreenLeftAlone { refused } => {
            println!("  {host} garde l'écran qu'il filme : {refused}");
        }
        Step::SpeakersLeftAlone { refused } => {
            println!("  Les enceintes de {host} restent allumées : {refused}");
        }
        Step::ScreenLeftAlone { refused } => {
            println!("  {host} n'a pas réveillé son écran virtuel : {refused}");
        }
        Step::ScreenOverThere { wide, high } => {
            println!("  {host} affiche {wide}x{high}, c'est ce qui est demandé au lecteur");
        }
        Step::NoSoundCardHere => {
            println!("  Cet ordinateur n'a pas de sortie audio : la session sera muette.");
        }
    }
}

/// Turns a failure into the message and the exit code that go with it.
fn reported(e: zyr_session::Error) -> ExitCode {
    use zyr_session::Error;
    match e {
        Error::EngineMissing(_) => failure(
            "FFmpeg manque",
            format!("{e}\n  Lancez « zyr-cli doctor » pour vérifier cet ordinateur."),
        ),
        Error::Service(reason) => failure("ouverture du tunnel", reason),
        other => failure("ouverture de la session", other),
    }
}

fn build_settings(args: &Args) -> Result<SessionSettings, String> {
    let (width, height) = parse_resolution(&args.resolution).map_err(|e| e.to_string())?;
    let codec: Codec = args.codec.parse()?;
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
        // Nothing here draws a pointer of its own, so the far computer
        // draws its own into the picture, as it does for a game.
        absolute_mouse: false,
        ..SessionSettings::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_not_taken_yet_is_a_dash_and_never_a_nought() {
        let line = measured(&Measures::default());
        assert!(line.starts_with("image -, - im/s"), "{line}");
        assert!(line.contains("décodage - |"), "{line}");
        assert!(!line.contains('0'), "{line}");
    }

    #[test]
    fn a_whole_reading_fits_on_one_line() {
        let line = measured(&Measures {
            codec: Some("HEVC".to_string()),
            width: Some(1920),
            height: Some(1080),
            fps: Some(59.96),
            decode_ms: Some(1.24),
            render_ms: Some(0.3),
            host_ms: Some(4.56),
            network_ms: Some(1.0),
            bitrate_mbps: Some(18.44),
            dropped_network_pct: Some(0.0),
            latency_ms: Some(12.34),
            ..Measures::default()
        });
        assert_eq!(
            line,
            "HEVC 1920x1080, 60.0 im/s | décodage 1.2 ms | affichage 0.3 ms | hôte 4.6 ms | \
             réseau 1.0 ms | débit 18.4 Mb/s | pertes 0.0 % | latence 12.3 ms"
        );
        assert_eq!(line.lines().count(), 1);
    }
}
