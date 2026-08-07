mod bench;
mod connect;
mod cpu;
mod doctor;
mod engines;
mod host;
mod identity;
mod measurement;
mod probe;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "zyr-cli",
    version = zyr_proto::PRODUCT_VERSION,
    about = "ZyrDesk technical tool",
    long_about = "ZyrDesk technical tool.\n\n\
                  At this stage of the project it drives the engines \
                  directly, to check performance on a local network. The \
                  service and the interface take over at later milestones."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Checks that this machine is ready for ZyrDesk
    Doctor,
    /// Inspects the engines in place
    Engines {
        #[command(subcommand)]
        action: engines::Action,
    },
    /// Makes this computer reachable from elsewhere
    Host {
        #[command(subcommand)]
        action: host::Action,
    },
    /// Opens a session on a remote computer
    Connect(connect::Args),
    /// Shows this machine's fingerprint
    Identity,
    /// Measures what the tunnel costs between two computers
    Bench {
        #[command(subcommand)]
        action: bench::Action,
    },
}

fn main() -> std::process::ExitCode {
    match Cli::parse().command {
        Command::Doctor => doctor::run(),
        Command::Engines { action } => engines::run(action),
        Command::Host { action } => host::run(action),
        Command::Connect(args) => connect::run(args),
        Command::Identity => identity::run(),
        Command::Bench { action } => bench::run(action),
    }
}

/// Reports a failure the same way everywhere, on the error stream.
pub fn failure(context: &str, error: impl std::fmt::Display) -> std::process::ExitCode {
    eprintln!("Échec : {context}");
    eprintln!("  {error}");
    std::process::ExitCode::FAILURE
}
