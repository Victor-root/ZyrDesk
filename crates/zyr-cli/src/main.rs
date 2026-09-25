mod account;
mod bench;
mod connect;
mod cpu;
mod doctor;
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
                  For diagnosis without a window: checking this machine, \
                  opening a session and reading what it costs, measuring \
                  the tunnel between two computers."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Checks that this machine is ready for ZyrDesk
    Doctor,
    /// Opens a session on a remote computer and prints what it costs
    Connect(connect::Args),
    /// Shows this machine's fingerprint
    Identity,
    /// Measures what the tunnel costs between two computers
    Bench {
        #[command(subcommand)]
        action: bench::Action,
    },
    /// The account this computer is attached to, through the service
    Account {
        #[command(subcommand)]
        action: account::Action,
    },
}

fn main() -> std::process::ExitCode {
    match Cli::parse().command {
        Command::Doctor => doctor::run(),
        Command::Connect(args) => connect::run(args),
        Command::Identity => identity::run(),
        Command::Bench { action } => bench::run(action),
        Command::Account { action } => account::run(action),
    }
}

/// Reports a failure the same way everywhere, on the error stream.
pub fn failure(context: &str, error: impl std::fmt::Display) -> std::process::ExitCode {
    eprintln!("Échec : {context}");
    eprintln!("  {error}");
    std::process::ExitCode::FAILURE
}

/// Where what this tool's sessions and checks say in passing is written:
/// the player's lines, and why FFmpeg left an encoder out.
///
/// Its own file, beside the product's journals: the command line is a
/// tool for diagnosis, and what it says is read after the fact rather
/// than mixed into what the product says of itself.
pub fn journal() -> std::path::PathBuf {
    zyr_proto::paths::logs_dir().join("cli.log")
}
