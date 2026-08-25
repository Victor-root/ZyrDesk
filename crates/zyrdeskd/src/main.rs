//! The ZyrDesk service.
//!
//! The same executable serves in two ways. Started by Windows with its
//! reserved argument, it becomes the service. Started by hand, it serves
//! to install, start, stop or remove it.

mod control;
mod gateway;
mod known;
mod preferences;
mod restart;
mod screen;
mod supervisor;
mod ways;

#[cfg(windows)]
mod service;
#[cfg(windows)]
mod session;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "zyrdeskd",
    version = zyr_proto::PRODUCT_VERSION,
    about = "ZyrDesk service",
    long_about = "ZyrDesk service.\n\n\
                  It makes this computer reachable from elsewhere, even \
                  before anyone has logged into Windows. Installing and \
                  removing it require administrator rights."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Registers the service and starts it, in one go
    Setup,
    /// Registers the service with Windows, started automatically
    Install,
    /// Removes the service
    Uninstall,
    /// Starts the service
    Start,
    /// Stops the service
    Stop,
    /// Shows the state of the service
    Status,
}

fn main() -> ExitCode {
    // Windows starts the service with a reserved argument clap has no
    // business knowing about: it is a signal, not a command.
    #[cfg(windows)]
    if std::env::args().any(|a| a == service::SERVICE_ARGUMENT) {
        return match service::hand_over_to_windows() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => failure("le service n'a pas pu démarrer", e),
        };
    }

    // And the service starts this program again, in the session the
    // engine runs in, when the engine has to be asked to go: only a
    // program in that session can reach the engine's console, and that
    // console is the one way to ask. Nobody types this either.
    #[cfg(windows)]
    if let Some(engine) = session::the_engine_to_let_go() {
        return if session::let_the_engine_go(engine) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    match Cli::parse().command {
        Some(command) => run(command),
        None => {
            eprintln!("Ce programme est le service ZyrDesk.");
            eprintln!("Lancez « zyrdeskd --help » pour voir ce qu'il sait faire.");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn run(command: Command) -> ExitCode {
    match command {
        // Asked for by the interface, through an elevation that shows no
        // console: whatever happens is written down as well as said, or
        // a failure here would leave nothing at all behind.
        Command::Setup => match service::set_up() {
            Ok(_) => {
                noted("service installed and started from the interface");
                println!("Service installé et démarré.");
                ExitCode::SUCCESS
            }
            Err(e) => {
                let reason = with_causes(&*e);
                noted(&format!("service could not be set up: {reason}"));
                failure("mise en service", reason)
            }
        },
        Command::Install => match service::install() {
            Ok(service::Installed::Registered) => {
                println!("Service installé. Il attend d'être démarré.");
                println!("  Pour le lancer tout de suite : zyrdeskd start");
                ExitCode::SUCCESS
            }
            Ok(service::Installed::Updated) => {
                println!("Service déjà installé, sa configuration a été mise à jour.");
                println!("  Il pointe maintenant sur ce programme.");
                println!("  S'il tournait, relancez-le : zyrdeskd stop puis zyrdeskd start");
                ExitCode::SUCCESS
            }
            Err(e) => failure("installation du service", with_causes(&*e)),
        },
        Command::Uninstall => match service::uninstall() {
            Ok(()) => {
                println!("Service retiré.");
                ExitCode::SUCCESS
            }
            Err(e) => failure("retrait du service", with_causes(&e)),
        },
        Command::Start => match service::start() {
            Ok(()) => {
                println!("Service démarré.");
                ExitCode::SUCCESS
            }
            Err(e) => failure("démarrage du service", with_causes(&e)),
        },
        Command::Stop => match service::stop() {
            Ok(service::Stopped::WasRunning) => {
                println!("Service arrêté.");
                ExitCode::SUCCESS
            }
            Ok(service::Stopped::AlreadyStopped) => {
                println!("Service déjà arrêté.");
                ExitCode::SUCCESS
            }
            Err(e) => failure("arrêt du service", with_causes(&e)),
        },
        Command::Status => match service::state() {
            Ok(state) => {
                println!("{}", readable(state));
                println!("  Journal : {}", service_log().display());
                ExitCode::SUCCESS
            }
            Err(e) => failure("état du service", with_causes(&e)),
        },
    }
}

#[cfg(windows)]
fn readable(state: windows_service::service::ServiceState) -> &'static str {
    use windows_service::service::ServiceState::*;
    match state {
        Stopped => "Arrêté",
        StartPending => "En cours de démarrage",
        StopPending => "En cours d'arrêt",
        Running => "En marche",
        ContinuePending => "Reprise en cours",
        PausePending => "Mise en pause",
        Paused => "En pause",
    }
}

#[cfg(windows)]
fn service_log() -> std::path::PathBuf {
    zyr_proto::paths::logs_dir().join("service.log")
}

/// Writes a line into the service's own log.
///
/// For what is done to the service from outside it, where nothing else
/// would keep a trace: an elevation started from the interface shows no
/// console, so anything printed there is read by nobody.
#[cfg(windows)]
fn noted(what: &str) {
    if let Ok(log) = zyr_proto::log::Log::open(&service_log()) {
        log.write(&format!("{what}, {}", zyr_proto::version_line()));
    }
}

/// Outside Windows the service has no purpose: there is no service
/// control manager to talk to, and no console session to serve.
#[cfg(not(windows))]
fn run(_command: Command) -> ExitCode {
    failure(
        "service indisponible",
        "le service ZyrDesk n'existe que sous Windows",
    )
}

/// Reports a failure the same way everywhere, on the error stream.
fn failure(context: &str, error: impl std::fmt::Display) -> ExitCode {
    eprintln!("Échec : {context}");
    eprintln!("  {error}");
    ExitCode::FAILURE
}

/// Renders an error together with what caused it.
///
/// The service library hides the system's own message behind a generic
/// wrapper: without the chain, a refused installation reads « IO error
/// in winapi call » and says nothing at all.
#[cfg(windows)]
fn with_causes(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(reason) = cause {
        text.push_str(" : ");
        text.push_str(&reason.to_string());
        cause = reason.source();
    }
    text
}
