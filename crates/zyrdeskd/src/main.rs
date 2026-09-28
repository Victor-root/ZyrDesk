//! The ZyrDesk service.
//!
//! The same executable serves in several ways. Started by Windows with
//! its reserved argument, it becomes the service. Started by the service
//! with another, it becomes the engine of one session, or runs one short
//! errand in the session that owns the screen. Started by hand, it serves
//! to install, start, stop or remove the service.

mod account;
mod clipboard;
mod control;
mod engine;
mod gateway;
mod incoming;
mod known;
mod machine;
mod outside;
mod pointer;
mod preferences;
mod said;
mod screen;
mod speakers;
mod supervisor;
mod transfer;
mod ways;
mod wifi;

#[cfg(windows)]
mod attention;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod text;

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
            Err(e) => failure("the service could not start", e),
        };
    }

    // And the service starts this program again, in the session that owns
    // the screen and with the system's own account, to be the engine of
    // one session: it films, encodes and plays what the far computer
    // types, over the link the service named. Nobody types this either.
    #[cfg(windows)]
    if let Some(link) = session::the_link_to_serve() {
        return match zyr_proto::log::Log::open(&zyr_proto::paths::logs_dir().join("engine.log")) {
            Ok(log) => zyr_host::serve(&link, log),
            // Nothing to say it in: the console, which the service has
            // pointed at a file of its own, is all that is left.
            Err(e) => failure("the engine could not open its journal", e),
        };
    }

    // And a third time, for the speakers of this computer. Which device
    // the desktop plays to depends on who is signed in, so the question
    // is asked from the session that owns the screen and nowhere else.
    // The answer is more than yes or no: two means the speakers were
    // already the way they were asked to be, so nothing is owed back.
    #[cfg(windows)]
    if let Some(quiet) = session::asked_about_the_speakers() {
        return ExitCode::from(session::move_the_speakers(quiet) as u8);
    }

    // And a fourth, to lock the screen. The other way round from
    // Ctrl+Alt+Del, which the service presses in its own process:
    // Windows takes that one from a service and nothing else, and this
    // one from the interactive desktop and nothing else. Both refusals
    // protect what a lock screen is worth.
    #[cfg(windows)]
    if session::asked_to_lock_the_screen() {
        return if session::lock_this_desktop() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    // And a fifth, about this computer's desk: holding it for a session
    // that is starting, or giving it back once that session has gone. The
    // same blindness again, and the worst case of it: what Windows says
    // about the arrangement of screens is answered for the window station
    // of whoever asks, and the service's has no screens on it at all, so
    // from there this computer has no screens to note and none to put
    // back.
    #[cfg(windows)]
    if let Some(asked) = session::the_desk_asked_for() {
        session::do_this_to_the_desk(asked);
        return ExitCode::SUCCESS;
    }

    // And a sixth, for the shape of this computer's pointer. The same
    // blindness in its plainest form: a pointer belongs to a desktop, and
    // the service's window station carries none. This one reads for a
    // while instead of doing one thing, and ends by itself.
    #[cfg(windows)]
    if session::asked_to_follow_the_pointer() {
        pointer::follow_the_pointer_here();
        return ExitCode::SUCCESS;
    }

    // And a seventh, for this computer's clipboard. The same blindness
    // once more: a clipboard belongs to a window station, and the
    // service's carries none. Like the pointer above it reads for a
    // while and ends by itself, and unlike it, it writes as well: what
    // was copied on the far computer is put on this one from here.
    #[cfg(windows)]
    if session::asked_to_carry_the_clipboard() {
        clipboard::carry_the_clipboard_here();
        return ExitCode::SUCCESS;
    }

    match Cli::parse().command {
        Some(command) => run(command),
        None => {
            eprintln!("This program is the ZyrDesk service.");
            eprintln!("Run « zyrdeskd --help » to see what it can do.");
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
                println!("Service installed and started.");
                ExitCode::SUCCESS
            }
            Err(e) => {
                let reason = with_causes(&*e);
                noted(&format!("service could not be set up: {reason}"));
                failure("setting up the service", reason)
            }
        },
        Command::Install => match service::install() {
            Ok(service::Installed::Registered) => {
                println!("Service installed. It is waiting to be started.");
                println!("  To start it right away: zyrdeskd start");
                ExitCode::SUCCESS
            }
            Ok(service::Installed::Updated) => {
                println!("Service already installed, its configuration has been updated.");
                println!("  It now points to this program.");
                println!("  If it was running, restart it: zyrdeskd stop then zyrdeskd start");
                ExitCode::SUCCESS
            }
            Err(e) => failure("installing the service", with_causes(&*e)),
        },
        Command::Uninstall => match service::uninstall() {
            Ok(()) => {
                println!("Service removed.");
                ExitCode::SUCCESS
            }
            Err(e) => absent_or(&e, "removing the service"),
        },
        Command::Start => match service::start() {
            Ok(()) => {
                println!("Service started.");
                ExitCode::SUCCESS
            }
            Err(e) => absent_or(&e, "starting the service"),
        },
        Command::Stop => match service::stop() {
            Ok(service::Stopped::WasRunning) => {
                println!("Service stopped.");
                ExitCode::SUCCESS
            }
            Ok(service::Stopped::AlreadyStopped) => {
                println!("Service already stopped.");
                ExitCode::SUCCESS
            }
            Err(e) => absent_or(&e, "stopping the service"),
        },
        Command::Status => match service::state() {
            Ok(state) => {
                println!("{}", readable(state));
                println!("  Journal: {}", service_log().display());
                ExitCode::SUCCESS
            }
            Err(e) => absent_or(&e, "service status"),
        },
    }
}

#[cfg(windows)]
fn readable(state: windows_service::service::ServiceState) -> &'static str {
    use windows_service::service::ServiceState::*;
    match state {
        Stopped => "Stopped",
        StartPending => "Starting",
        StopPending => "Stopping",
        Running => "Running",
        ContinuePending => "Resuming",
        PausePending => "Pausing",
        Paused => "Paused",
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
        log.about(crate::service::TAG)
            .write(&format!("{what}, {}", zyr_proto::version_line()));
    }
}

/// Outside Windows the service has no purpose: there is no service
/// control manager to talk to, and no console session to serve.
#[cfg(not(windows))]
fn run(_command: Command) -> ExitCode {
    failure(
        "service unavailable",
        "the ZyrDesk service only exists on Windows",
    )
}

/// Reports a failure the same way everywhere, on the error stream.
fn failure(context: &str, error: impl std::fmt::Display) -> ExitCode {
    eprintln!("Failure: {context}");
    eprintln!("  {error}");
    ExitCode::FAILURE
}

/// Says a service that was never installed here, or says the failure.
///
/// Windows answers the same way whether it is asked to start, stop,
/// describe or remove a service it does not know, and its own words name
/// neither the situation nor the one thing to do about it. A machine
/// where ZyrDesk has never run meets this before anything else, and it
/// meets it once.
#[cfg(windows)]
fn absent_or(error: &windows_service::Error, context: &str) -> ExitCode {
    // Started and never came back. What Windows says about it only talks
    // of a timeout, when the question is which program it started: a
    // registration holds the path it was given, and moving the repository
    // leaves it pointing at the old path. The registered path is the one
    // thing missing to see it, and Windows never says it by itself.
    if service::never_reported(error) {
        eprintln!("The ZyrDesk service was started and never answered.");
        match service::registered_at() {
            Some(program) => {
                eprintln!("  Windows starts this one:\n    {}", program.display());
                eprintln!(
                    "  If it is not the program you have just built, register it again,\n  in an administrator window:\n    zyrdeskd setup"
                );
            }
            None => eprintln!("  Windows did not say which program it starts."),
        }
        return ExitCode::FAILURE;
    }
    if !service::unknown(error) {
        return failure(context, with_causes(error));
    }
    eprintln!("The ZyrDesk service is not installed on this computer.");
    eprintln!("  To be installed once, in an administrator window:");
    eprintln!();
    eprintln!("      zyrdeskd setup");
    eprintln!();
    eprintln!("  ZyrDesk's window also does it by itself the first time it opens.");
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
        text.push_str(": ");
        text.push_str(&reason.to_string());
        cause = reason.source();
    }
    text
}
